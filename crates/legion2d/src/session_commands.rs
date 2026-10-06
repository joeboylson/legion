//! Starting, stopping and looking into sessions.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use legion2_proto::{Activity, EntryKind, LogFilter, NewEntry, Reply, Deployment, SessionInfo, COMMANDER};

use crate::{
    shared_tools::shared_tools,
    constants::{DEFAULT_OPERATOR_COPY_LIMIT, PROMPTS_FOLDER_NAME, MISSION_BRANCH_PREFIX, RECENT_POSTMORTEM_COUNT, WORKTREES_FOLDER_NAME},
    daemon::{Daemon, State},
    git::{create_worktree, current_branch, is_git_repo},
    naming::{copy_positions, first_free_position, idle_copy_to_free, operator_of_position},
    prompts::{commander_prompt, operator_prompt, MissionBriefing, OperatorBriefing},
    session_arguments::{legion_tools_config, session_arguments, LaunchPlan},
    sessions::{spawn_session, Session, SpawnRequest},
    setup::{clear_at_percent, read_operator, read_pipeline, read_settings},
    terminal_key::TerminalKey,
};

const COMMANDER_FIRST_PROMPT: &str = "Your deployment has started. Check its missions with the missions tool and start on any that are waiting.";

fn write_prompt_file(path: &Path, prompt: &str) -> Result<(), String> {
    let folder = path.parent().ok_or_else(|| format!("{} has no folder", path.display()))?;
    std::fs::create_dir_all(folder).map_err(|error| format!("can't make {}: {error}", folder.display()))?;
    std::fs::write(path, prompt).map_err(|error| format!("can't write {}: {error}", path.display()))
}

/// A waiting session costs nothing, so only starting, busy and asking ones count.
pub fn is_busy(activity: Activity) -> bool {
    matches!(activity, Activity::Starting | Activity::Busy | Activity::Permission)
}

fn started_text(position: &str, mission: Option<u32>, starter: &str) -> String {
    match mission {
        Some(number) => format!("{position} started on mission {number} (by {starter})"),
        None => format!("{position} started (by {starter})"),
    }
}

fn operator_first_prompt(mission: Option<u32>) -> String {
    match mission {
        Some(number) => format!("Start on mission {number}."),
        None => "Wait for the commander to send you a mission.".into(),
    }
}

fn commander_launch(state: &State, folder_index: usize, deployment: &Deployment, first_prompt: Option<String>) -> Result<LaunchPlan, String> {
    let folder = &state.folders[folder_index];
    if state.running_positions(&deployment.id).iter().any(|position| position == COMMANDER) {
        return Err("the commander is already running".into());
    }
    let (pipeline, pipeline_text) = read_pipeline(&folder.path, &deployment.pipeline)?;
    let copy_limits = pipeline
        .operators
        .iter()
        .map(|operator| read_operator(&folder.path, operator).map(|read| (operator.clone(), read.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT))))
        .collect::<Result<Vec<_>, _>>()?;
    let settings = read_settings(&folder.path)?;
    let postmortem_filter = LogFilter { kinds: Some(vec![EntryKind::Postmortem]), ..Default::default() };
    let postmortems = folder.store.entries(&deployment.id, &postmortem_filter)?;
    let recent_postmortems = &postmortems[postmortems.len().saturating_sub(RECENT_POSTMORTEM_COUNT)..];
    Ok(LaunchPlan {
        position: COMMANDER.to_string(),
        system_prompt: commander_prompt(deployment, &pipeline_text, &copy_limits, recent_postmortems),
        model: None,
        permission_mode: settings.permission_mode,
        allowed_tools: Vec::new(),
        disallowed_tools: Vec::new(),
        first_prompt: first_prompt.unwrap_or_else(|| COMMANDER_FIRST_PROMPT.into()),
        clear_at: clear_at_percent(None, settings.clear_at),
    })
}

fn operator_launch(
    state: &State,
    folder_index: usize,
    deployment: &Deployment,
    operator_name: &str,
    mission: Option<u32>,
    first_prompt: Option<String>,
    max_busy_sessions: usize,
) -> Result<LaunchPlan, String> {
    let folder = &state.folders[folder_index];
    let busy_session_count = state.sessions.values().filter(|session| is_busy(session.activity)).count();
    if busy_session_count >= max_busy_sessions {
        return Err(format!("the machine is at its limit of {max_busy_sessions} busy sessions; start {operator_name} once one is free"));
    }
    let (pipeline, pipeline_text) = read_pipeline(&folder.path, &deployment.pipeline)?;
    if !pipeline.operators.iter().any(|operator| operator == operator_name) {
        return Err(format!("{operator_name} isn't on the {} pipeline ({})", deployment.pipeline, pipeline.operators.join(", ")));
    }
    let operator = read_operator(&folder.path, operator_name)?;
    let settings = read_settings(&folder.path)?;
    let copy_limit = operator.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT);
    let position = first_free_position(&copy_positions(operator_name, copy_limit), &state.running_positions(&deployment.id))
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
    let base_branch = current_branch(&folder.path);
    let operator_briefing =
        OperatorBriefing { operator: &operator, pipeline: &pipeline, pipeline_text: &pipeline_text, shared_tools: &tools, base_branch: base_branch.as_deref() };
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
    fn free_idle_copy(&self, deployment_key: &str, operator: &str, mission: Option<u32>) -> Result<(), String> {
        let freed = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let limit = read_operator(&state.folders[folder_index].path, operator)?.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT);
            // A copy already on this mission keeps what it knows: it gets a message, not a restart.
            let already_on_it = state.sessions.values().find(|session| {
                session.deployment == deployment.id && operator_of_position(&session.position) == operator && mission.is_some() && session.mission == mission
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

    pub fn start_session(
        self: &Arc<Self>,
        deployment_key: &str,
        operator: &str,
        mission: Option<u32>,
        starter: &str,
        first_prompt: Option<String>,
    ) -> Result<Reply, String> {
        if operator != COMMANDER {
            self.free_idle_copy(deployment_key, operator, mission)?;
        }
        let spawn_request = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let launch = if operator == COMMANDER {
                commander_launch(&state, folder_index, &deployment, first_prompt)?
            } else {
                operator_launch(&state, folder_index, &deployment, operator, mission, first_prompt, self.config.max_busy_sessions)?
            };
            let tools_config = legion_tools_config(&self.config.binary_folder, &self.config.socket_path, &deployment.id, &launch.position);
            let prompt_file = self.config.data_folder.join(PROMPTS_FOLDER_NAME).join(format!("{}-{}.md", deployment.id, launch.position));
            write_prompt_file(&prompt_file, &launch.system_prompt)?;
            let arguments = session_arguments(&launch, &tools_config, &prompt_file);
            let working_folder = mission_working_folder(&state, folder_index, &deployment.id, mission)?;
            SpawnRequest { deployment: deployment.id, position: launch.position, mission, working_folder, arguments, clear_at: launch.clear_at }
        };
        let deployment_id = spawn_request.deployment.clone();
        let position = spawn_request.position.clone();
        let info = spawn_session(self, spawn_request)?;
        let started = NewEntry { kind: EntryKind::SessionStarted, mission, to: None, text: started_text(&position, mission, starter), answers: None };
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
        let mut state = self.state.lock().unwrap();
        let (_, deployment) = state.find_deployment(deployment_key)?;
        state.session_in_deployment(&deployment.id, position)?.type_bytes(text.as_bytes())?;
        Ok(Reply::Done)
    }

    pub fn press_key(&self, deployment_key: &str, position: &str, key_name: &str) -> Result<Reply, String> {
        let key = TerminalKey::from_name(key_name)?;
        let mut state = self.state.lock().unwrap();
        let (_, deployment) = state.find_deployment(deployment_key)?;
        state.session_in_deployment(&deployment.id, position)?.press_key(key)?;
        Ok(Reply::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_working_sessions_are_busy() {
        assert!(is_busy(Activity::Busy) && is_busy(Activity::Permission) && is_busy(Activity::Starting));
        assert!(!is_busy(Activity::Idle) && !is_busy(Activity::Ended));
    }

    #[test]
    fn the_start_entry_names_the_mission_and_starter() {
        assert_eq!(started_text("builder", Some(3), "commander"), "builder started on mission 3 (by commander)");
        assert_eq!(started_text("commander", None, "human"), "commander started (by human)");
    }

    #[test]
    fn an_operator_without_a_mission_waits() {
        assert_eq!(operator_first_prompt(Some(2)), "Start on mission 2.");
        assert!(operator_first_prompt(None).starts_with("Wait"));
    }
}
