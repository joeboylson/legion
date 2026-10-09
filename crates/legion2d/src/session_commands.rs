//! Starting, stopping and looking into sessions.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use legion2_proto::{role_of_position, Activity, Deployment, EntryKind, LogFilter, NewEntry, Reply, Role, SessionInfo, COMMANDER, STRATEGIST};

use crate::{
    shared_tools::{shared_tools, shared_tools_folder},
    mission_parts::{part_first_prompt, part_instructions},
    store::PartCheckout,
    constants::{STRATEGIST_DISALLOWED_TOOLS, DEFAULT_OPERATOR_COPY_LIMIT, PROMPTS_FOLDER_NAME, MISSION_BRANCH_PREFIX, RECENT_POSTMORTEM_COUNT, WORKTREES_FOLDER_NAME},
    daemon::{Daemon, State},
    git::{create_worktree, current_branch, is_git_repo},
    naming::{choose_position, copy_positions, idle_copy_to_free, operator_of_position},
    permission_answers::answers_permission_question,
    callouts::read_callouts,
    prompts::{commander_prompt, operator_prompt, strategist_prompt, MissionBriefing, OperatorBriefing, StrategistBriefing},
    session_arguments::{legion_tools_config, session_arguments, LaunchPlan},
    team_changes::{read_team, team_fingerprint, Team},
    sessions::{spawn_session, Session, SpawnRequest},
    setup::{clear_at_percent, read_operator, read_pipeline, read_settings, strategist_check_minutes},
    terminal_key::TerminalKey,
};

/// What to start, and on what.
pub struct StartRequest<'a> {
    pub operator: &'a str,
    pub mission: Option<u32>,
    /// One part of a split mission: the session works in that part's
    /// checkout and is told to do only that part.
    pub part: Option<u32>,
    /// The position to take when it's free, such as builder-2: a session
    /// coming back after a restart keeps its name.
    pub position: Option<&'a str>,
    pub starter: &'a str,
    /// Replaces the usual first prompt, as when Legion restarts a session.
    pub first_prompt: Option<String>,
}

const STRATEGIST_FIRST_PROMPT: &str = "Wait for Legion's first speed check.";
const COMMANDER_FIRST_PROMPT: &str = "Your deployment has started. Check its missions with the missions tool and start on any that are waiting.";

fn write_prompt_file(path: &Path, prompt: &str) -> Result<(), String> {
    let folder = path.parent().ok_or_else(|| format!("{} has no folder", path.display()))?;
    std::fs::create_dir_all(folder).map_err(|error| format!("can't make {}: {error}", folder.display()))?;
    std::fs::write(path, prompt).map_err(|error| format!("can't write {}: {error}", path.display()))
}

/// A waiting session costs nothing, so only starting, busy and asking ones
/// count, and a halted one, which holds its work until a person answers.
pub fn is_busy(activity: Activity) -> bool {
    matches!(activity, Activity::Starting | Activity::Busy | Activity::Permission | Activity::Halted)
}

fn started_text(position: &str, mission: Option<u32>, part: Option<u32>, starter: &str) -> String {
    match (mission, part) {
        (Some(number), Some(part)) => format!("{position} started on part {part} of mission {number} (by {starter})"),
        (Some(number), None) => format!("{position} started on mission {number} (by {starter})"),
        (None, _) => format!("{position} started (by {starter})"),
    }
}

/// The checkout of the part a session starts on, if it starts on one.
fn part_checkout_for(state: &State, folder_index: usize, mission: Option<u32>, part: Option<u32>) -> Result<Option<PartCheckout>, String> {
    let Some(number) = part else { return Ok(None) };
    let mission = mission.ok_or("a part needs its mission too")?;
    let checkout = state.folders[folder_index]
        .store
        .parts(mission)?
        .into_iter()
        .find(|checkout| checkout.part.number == number)
        .ok_or_else(|| format!("mission {mission} has no part {number}"))?;
    if checkout.part.is_merged {
        return Err(format!("part {number} of mission {mission} is already merged"));
    }
    Ok(Some(checkout))
}

fn operator_first_prompt(mission: Option<u32>) -> String {
    match mission {
        Some(number) => format!("Start on mission {number}."),
        None => "Wait for the commander to send you a mission.".into(),
    }
}

/// The commander's launch, and the team it's told about, to notice changes later.
fn commander_launch(state: &State, folder_index: usize, deployment: &Deployment, first_prompt: Option<String>) -> Result<(LaunchPlan, String), String> {
    let folder = &state.folders[folder_index];
    if state.running_positions(&deployment.id).iter().any(|position| position == COMMANDER) {
        return Err("the commander is already running".into());
    }
    let team = read_team(&folder.path, &deployment.pipeline);
    let fingerprint = team_fingerprint(&team);
    let Team { pipeline_text, copy_limits } = team?;
    let settings = read_settings(&folder.path)?;
    let recent_postmortems = folder.store.recent_entries_of_kind(EntryKind::Postmortem, RECENT_POSTMORTEM_COUNT)?;
    let launch = LaunchPlan {
        position: COMMANDER.to_string(),
        system_prompt: commander_prompt(deployment, &pipeline_text, &copy_limits, &recent_postmortems, &read_callouts(&folder.path)),
        model: None,
        permission_mode: settings.permission_mode,
        allowed_tools: Vec::new(),
        disallowed_tools: Vec::new(),
        first_prompt: first_prompt.unwrap_or_else(|| COMMANDER_FIRST_PROMPT.into()),
        clear_at: clear_at_percent(None, settings.clear_at),
    };
    Ok((launch, fingerprint))
}

/// The strategist works in the folder itself, reading only; like the
/// commander, it doesn't count against the machine's limit when it starts.
fn strategist_launch(state: &State, folder_index: usize, deployment: &Deployment) -> Result<LaunchPlan, String> {
    let folder = &state.folders[folder_index];
    if state.running_positions(&deployment.id).iter().any(|position| position == STRATEGIST) {
        return Err("the strategist is already running".into());
    }
    let settings = read_settings(&folder.path)?;
    let every_minutes = strategist_check_minutes(&settings).ok_or("the strategist isn't enabled in legion.json")?;
    let team = read_team(&folder.path, &deployment.pipeline)?;
    let recent_postmortems = folder.store.recent_entries_of_kind(EntryKind::Postmortem, RECENT_POSTMORTEM_COUNT)?;
    let briefing = StrategistBriefing { team: &team, every_minutes, recent_postmortems: &recent_postmortems, callouts: &read_callouts(&folder.path) };
    Ok(LaunchPlan {
        position: STRATEGIST.to_string(),
        system_prompt: strategist_prompt(deployment, briefing),
        model: None,
        permission_mode: settings.permission_mode,
        allowed_tools: Vec::new(),
        disallowed_tools: STRATEGIST_DISALLOWED_TOOLS.iter().map(|tool| tool.to_string()).collect(),
        first_prompt: STRATEGIST_FIRST_PROMPT.into(),
        clear_at: clear_at_percent(None, settings.clear_at),
    })
}

fn operator_launch(
    state: &State,
    folder_index: usize,
    deployment: &Deployment,
    request: &StartRequest,
    first_prompt: Option<String>,
    max_busy_sessions: usize,
) -> Result<LaunchPlan, String> {
    let folder = &state.folders[folder_index];
    let operator_name = request.operator;
    let mission = request.mission;
    let busy_session_count = state.sessions.values().filter(|session| is_busy(session.activity)).count();
    if busy_session_count >= max_busy_sessions {
        return Err(format!("the machine is at its limit of {max_busy_sessions} busy sessions; start {operator_name} once one is free"));
    }
    let (pipeline, pipeline_text) = read_pipeline(&folder.path, &deployment.pipeline)?;
    if !pipeline.operators.iter().any(|operator| operator == operator_name) {
        return Err(format!("{operator_name} isn't on the team {} ({})", legion2_proto::pipeline_phrase(&deployment.pipeline), pipeline.operators.join(", ")));
    }
    let operator = read_operator(&folder.path, operator_name)?;
    let settings = read_settings(&folder.path)?;
    let copy_limit = operator.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT);
    let position = choose_position(&copy_positions(operator_name, copy_limit), &state.running_positions(&deployment.id), request.position)
        .ok_or_else(|| format!("{operator_name} is at its limit of {copy_limit} running at once"))?;
    let mission_with_history = mission
        .map(|number| {
            let (_, body) = state.mission_with_body(folder_index, &deployment.id, number)?;
            let history = folder.store.entries(&deployment.id, &LogFilter { mission: Some(number), ..Default::default() })?;
            Ok::<_, String>((number, body, history))
        })
        .transpose()?;
    let briefing = mission_with_history.as_ref().map(|(number, body, history)| MissionBriefing { number: *number, body, history });
    let tools = shared_tools(&folder.path);
    let tools_folder = shared_tools_folder(&folder.path);
    let base_branch = current_branch(&folder.path);
    let callouts = read_callouts(&folder.path);
    let operator_briefing = OperatorBriefing {
        operator: &operator,
        pipeline: &pipeline,
        pipeline_text: &pipeline_text,
        shared_tools: &tools,
        shared_tools_folder: &tools_folder,
        base_branch: base_branch.as_deref(),
        callouts: &callouts,
    };
    let system_prompt = operator_prompt(&position, deployment, operator_briefing, briefing);
    Ok(LaunchPlan {
        position,
        system_prompt,
        model: operator.config.model,
        permission_mode: operator.config.permission_mode.or(settings.permission_mode),
        clear_at: clear_at_percent(operator.config.clear_at, settings.clear_at),
        allowed_tools: operator.config.allowed_tools,
        disallowed_tools: operator.config.disallowed_tools,
        first_prompt: first_prompt.unwrap_or_else(|| operator_first_prompt(mission)),
    })
}

/// A mission works in its own worktree, made the first time anyone starts
/// on it. Without git, or once it's finished, sessions work in the folder.
fn mission_working_folder(state: &State, folder_index: usize, deployment_id: &str, mission: Option<u32>) -> Result<PathBuf, String> {
    let folder = &state.folders[folder_index];
    let Some(number) = mission else { return Ok(folder.path.clone()) };
    if !is_git_repo(&folder.path) {
        return Ok(folder.path.clone());
    }
    let worktree = match folder.store.worktree(number)? {
        Some(existing) => existing,
        None => {
            let mission_file = folder.store.mission(deployment_id, number)?.file;
            let worktree_name = Path::new(&mission_file).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
            let worktree_path = folder.outside_folder.join(WORKTREES_FOLDER_NAME).join(&worktree_name);
            let created = create_worktree(&folder.path, &worktree_path, &format!("{MISSION_BRANCH_PREFIX}{worktree_name}"))?;
            folder.store.add_worktree(number, &created)?;
            created
        }
    };
    Ok(if worktree.is_removed { folder.path.clone() } else { PathBuf::from(worktree.path) })
}

impl Daemon {
    /// `first_prompt` replaces the usual one, as when Legion restarts a session.
    /// When every copy of an operator is taken and one is idle, ends that one
    /// so the new work can start. Its handoff note is in the log.
    fn free_idle_copy(&self, deployment_key: &str, operator: &str, mission: Option<u32>, part: Option<u32>) -> Result<(), String> {
        let freed = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let limit = read_operator(&state.folders[folder_index].path, operator)?.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT);
            // A copy already on this mission (or part) keeps what it knows: it gets a message, not a restart.
            let already_on_it = state.sessions.values().find(|session| {
                session.deployment == deployment.id
                    && operator_of_position(&session.position) == operator
                    && mission.is_some()
                    && session.mission == mission
                    && session.part == part
            });
            if let Some(session) = already_on_it {
                return Err(format!(
                    "{} is already on mission {} ({}): send it the work with the send tool instead of starting it",
                    session.position,
                    session.mission.unwrap_or_default(),
                    session.activity.label()
                ));
            }
            let running: Vec<(String, bool)> = state
                .sessions
                .values()
                .filter(|session| session.deployment == deployment.id)
                .map(|session| (session.position.clone(), session.activity == Activity::Idle))
                .collect();
            let Some(position) = idle_copy_to_free(&copy_positions(operator, limit), &running) else { return Ok(()) };
            let session_id = state.sessions.iter().find(|(_, session)| session.deployment == deployment.id && session.position == position).map(|(id, _)| id.clone());
            // Taken out now, so its position is free for the new session at once.
            let Some(mut session) = session_id.and_then(|id| state.sessions.remove(&id)) else { return Ok(()) };
            session.is_stopping = true;
            session.end()?;
            (deployment.id, session.info())
        };
        let (deployment_id, info) = freed;
        self.announce_session(&SessionInfo { activity: Activity::Ended, ..info.clone() });
        let held = info.mission.map(|number| format!(" on mission {number}")).unwrap_or_default();
        let wanted = mission.map(|number| format!(" on mission {number}")).unwrap_or_default();
        let text = format!("{} was idle{held}, so Legion ended it to start {operator}{wanted}", info.position);
        self.post_entry(&deployment_id, legion2_proto::LEGION, NewEntry { kind: EntryKind::SessionEnded, mission: info.mission, to: None, text, answers: None })?;
        Ok(())
    }

    /// `first_prompt` replaces the usual one, as when Legion restarts a session.
    pub fn start_session(
        self: &Arc<Self>,
        deployment_key: &str,
        operator: &str,
        mission: Option<u32>,
        starter: &str,
        first_prompt: Option<String>,
    ) -> Result<Reply, String> {
        self.start_session_on(deployment_key, StartRequest { operator, mission, part: None, position: None, starter, first_prompt })
    }

    pub fn start_session_on(self: &Arc<Self>, deployment_key: &str, request: StartRequest) -> Result<Reply, String> {
        let StartRequest { operator, mission, part, starter, .. } = request;
        let max_busy_sessions = self.machine_settings()?.max_busy_sessions;
        let role = role_of_position(operator);
        if role == Role::Operator {
            self.free_idle_copy(deployment_key, operator, mission, part)?;
        }
        let (spawn_request, commanders_team) = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let part_checkout = part_checkout_for(&state, folder_index, mission, part)?;
            let first_prompt = match (&part_checkout, mission) {
                (Some(checkout), Some(number)) => Some(request.first_prompt.clone().unwrap_or_else(|| part_first_prompt(number, checkout.part.number))),
                _ => request.first_prompt.clone(),
            };
            let (mut launch, commanders_team) = match role {
                Role::Commander => {
                    let (launch, fingerprint) = commander_launch(&state, folder_index, &deployment, first_prompt)?;
                    (launch, Some(fingerprint))
                }
                Role::Strategist => (strategist_launch(&state, folder_index, &deployment)?, None),
                Role::Operator => (operator_launch(&state, folder_index, &deployment, &request, first_prompt, max_busy_sessions)?, None),
            };
            if let (Some(checkout), Some(number)) = (&part_checkout, mission) {
                launch.system_prompt = format!("{}\n\n{}", launch.system_prompt, part_instructions(number, &checkout.part));
            }
            let tools_config = legion_tools_config(&self.config.binary_folder, &self.config.socket_path, &deployment.id, &launch.position);
            let prompt_file = self.config.data_folder.join(PROMPTS_FOLDER_NAME).join(format!("{}-{}.md", deployment.id, launch.position));
            write_prompt_file(&prompt_file, &launch.system_prompt)?;
            let arguments = session_arguments(&launch, &tools_config, &prompt_file);
            let working_folder = match &part_checkout {
                Some(checkout) => PathBuf::from(&checkout.path),
                None => mission_working_folder(&state, folder_index, &deployment.id, mission)?,
            };
            let spawn_request = SpawnRequest { deployment: deployment.id, position: launch.position, mission, part, working_folder, arguments, clear_at: launch.clear_at, permission_mode: launch.permission_mode };
            (spawn_request, commanders_team)
        };
        let deployment_id = spawn_request.deployment.clone();
        let position = spawn_request.position.clone();
        let info = spawn_session(self, spawn_request)?;
        if let Some(fingerprint) = commanders_team {
            self.record_team_told_to_commander(&deployment_id, fingerprint);
        }
        let started = NewEntry { kind: EntryKind::SessionStarted, mission, to: None, text: started_text(&position, mission, part, starter), answers: None };
        self.post_entry(&deployment_id, &position, started)?;
        Ok(Reply::Session { session: info })
    }

    pub fn stop_session(&self, deployment_key: &str, position: &str) -> Result<Reply, String> {
        let mut state = self.state.lock().unwrap();
        let (_, deployment) = state.find_deployment(deployment_key)?;
        let session = state.session_in_deployment(&deployment.id, position)?;
        session.is_stopping = true;
        session.end()?;
        Ok(Reply::Done)
    }

    pub fn list_sessions(&self, deployment_key: Option<&str>) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let deployment_id = deployment_key.map(|key| state.find_deployment(key).map(|(_, deployment)| deployment.id)).transpose()?;
        let mut sessions: Vec<SessionInfo> = state
            .sessions
            .values()
            .filter(|session| deployment_id.as_ref().is_none_or(|id| &session.deployment == id))
            .map(Session::info)
            .collect();
        sessions.sort_by(|first, second| (&first.deployment, &first.position).cmp(&(&second.deployment, &second.position)));
        Ok(Reply::Sessions { sessions })
    }

    pub fn read_screen(&self, deployment_key: &str, position: &str) -> Result<Reply, String> {
        let mut state = self.state.lock().unwrap();
        let (_, deployment) = state.find_deployment(deployment_key)?;
        let session = state.session_in_deployment(&deployment.id, position)?;
        Ok(Reply::Screen { text: session.screen_text(), ansi: session.screen_ansi() })
    }

    pub fn type_input(&self, deployment_key: &str, position: &str, text: &str) -> Result<Reply, String> {
        self.send_to_terminal(deployment_key, position, text.as_bytes())
    }

    pub fn press_key(&self, deployment_key: &str, position: &str, key_name: &str) -> Result<Reply, String> {
        self.send_to_terminal(deployment_key, position, TerminalKey::from_name(key_name)?.bytes())
    }

    /// Sends what a person typed or pressed. An answer to a waiting
    /// permission question marks the session busy at once: Claude says only
    /// when the tool call ends, which for a slow tool is long after.
    fn send_to_terminal(&self, deployment_key: &str, position: &str, bytes: &[u8]) -> Result<Reply, String> {
        let answered = {
            let mut state = self.state.lock().unwrap();
            let (_, deployment) = state.find_deployment(deployment_key)?;
            let session = state.session_in_deployment(&deployment.id, position)?;
            session.type_bytes(bytes)?;
            let is_answer = session.activity == Activity::Permission && answers_permission_question(bytes);
            if is_answer {
                session.activity = Activity::Busy;
                session.detail = None;
                session.permission_asked_at = None;
            }
            is_answer.then(|| session.info())
        };
        if let Some(info) = answered {
            self.announce_session(&info);
        }
        Ok(Reply::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_working_sessions_are_busy() {
        assert!(is_busy(Activity::Busy) && is_busy(Activity::Permission) && is_busy(Activity::Starting) && is_busy(Activity::Halted));
        assert!(!is_busy(Activity::Idle) && !is_busy(Activity::Ended));
    }

    #[test]
    fn the_start_entry_names_the_mission_and_starter() {
        assert_eq!(started_text("builder", Some(3), None, "commander"), "builder started on mission 3 (by commander)");
        assert_eq!(started_text("builder-2", Some(3), Some(2), "commander"), "builder-2 started on part 2 of mission 3 (by commander)");
        assert_eq!(started_text("commander", None, None, "human"), "commander started (by human)");
    }

    #[test]
    fn an_operator_without_a_mission_waits() {
        assert_eq!(operator_first_prompt(Some(2)), "Start on mission 2.");
        assert!(operator_first_prompt(None).starts_with("Wait"));
    }
}
