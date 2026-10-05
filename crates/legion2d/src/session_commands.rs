//! Starting, stopping and looking into sessions.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use legion2_proto::{Activity, EntryKind, LogFilter, NewEntry, Reply, Run, SessionInfo, COMMANDER};

use crate::{
    constants::{DEFAULT_OPERATOR_COPY_LIMIT, MISSION_BRANCH_PREFIX, RECENT_POSTMORTEM_COUNT, WORKTREES_FOLDER_NAME},
    daemon::{Daemon, State},
    git::{create_worktree, is_git_repo},
    naming::{copy_positions, first_free_position},
    prompts::{commander_prompt, operator_prompt, MissionBriefing},
    session_arguments::{legion_tools_config, session_arguments, LaunchPlan},
    sessions::{spawn_session, Session, SpawnRequest},
    setup::{read_operator, read_pipeline},
    terminal_key::TerminalKey,
};

const COMMANDER_FIRST_PROMPT: &str = "Your run has started. Check its missions with the missions tool and start on any that are waiting.";

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

fn commander_launch(state: &State, folder_index: usize, run: &Run, first_prompt: Option<String>) -> Result<LaunchPlan, String> {
    let folder = &state.folders[folder_index];
    if state.running_positions(&run.id).iter().any(|position| position == COMMANDER) {
        return Err("the commander is already running".into());
    }
    let (pipeline, pipeline_text) = read_pipeline(&folder.path, &run.pipeline)?;
    let copy_limits = pipeline
        .operators
        .iter()
        .map(|operator| read_operator(&folder.path, operator).map(|read| (operator.clone(), read.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT))))
        .collect::<Result<Vec<_>, _>>()?;
    let postmortem_filter = LogFilter { kinds: Some(vec![EntryKind::Postmortem]), ..Default::default() };
    let postmortems = folder.store.entries(&run.id, &postmortem_filter)?;
    let recent_postmortems = &postmortems[postmortems.len().saturating_sub(RECENT_POSTMORTEM_COUNT)..];
    Ok(LaunchPlan {
        position: COMMANDER.to_string(),
        system_prompt: commander_prompt(run, &pipeline_text, &copy_limits, recent_postmortems),
        model: None,
        allowed_tools: Vec::new(),
        disallowed_tools: Vec::new(),
        first_prompt: first_prompt.unwrap_or_else(|| COMMANDER_FIRST_PROMPT.into()),
    })
}

fn operator_launch(
    state: &State,
    folder_index: usize,
    run: &Run,
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
    let (pipeline, pipeline_text) = read_pipeline(&folder.path, &run.pipeline)?;
    if !pipeline.operators.iter().any(|operator| operator == operator_name) {
        return Err(format!("{operator_name} isn't on the {} pipeline ({})", run.pipeline, pipeline.operators.join(", ")));
    }
    let operator = read_operator(&folder.path, operator_name)?;
    let copy_limit = operator.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT);
    let position = first_free_position(&copy_positions(operator_name, copy_limit), &state.running_positions(&run.id))
        .ok_or_else(|| format!("{operator_name} is at its limit of {copy_limit} running at once"))?;
    let mission_with_history = mission
        .map(|number| {
            let (_, body) = state.mission_with_body(folder_index, &run.id, number)?;
            let history = folder.store.entries(&run.id, &LogFilter { mission: Some(number), ..Default::default() })?;
            Ok::<_, String>((number, body, history))
        })
        .transpose()?;
    let briefing = mission_with_history.as_ref().map(|(number, body, history)| MissionBriefing { number: *number, body, history });
    let system_prompt = operator_prompt(&position, run, &operator, &pipeline, &pipeline_text, briefing);
    Ok(LaunchPlan {
        position,
        system_prompt,
        model: operator.config.model,
        allowed_tools: operator.config.allowed_tools,
        disallowed_tools: operator.config.disallowed_tools,
        first_prompt: first_prompt.unwrap_or_else(|| operator_first_prompt(mission)),
    })
}

/// A mission works in its own worktree, made the first time anyone starts
/// on it. Without git, or once it's finished, sessions work in the folder.
fn mission_working_folder(state: &State, folder_index: usize, run_id: &str, mission: Option<u32>) -> Result<PathBuf, String> {
    let folder = &state.folders[folder_index];
    let Some(number) = mission else { return Ok(folder.path.clone()) };
    if !is_git_repo(&folder.path) {
        return Ok(folder.path.clone());
    }
    let worktree = match folder.store.worktree(number)? {
        Some(existing) => existing,
        None => {
            let mission_file = folder.store.mission(run_id, number)?.file;
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
    pub fn start_session(
        self: &Arc<Self>,
        run_key: &str,
        operator: &str,
        mission: Option<u32>,
        starter: &str,
        first_prompt: Option<String>,
    ) -> Result<Reply, String> {
        let spawn_request = {
            let state = self.state.lock().unwrap();
            let (folder_index, run) = state.find_open_run(run_key)?;
            let launch = if operator == COMMANDER {
                commander_launch(&state, folder_index, &run, first_prompt)?
            } else {
                operator_launch(&state, folder_index, &run, operator, mission, first_prompt, self.config.max_busy_sessions)?
            };
            let tools_config = legion_tools_config(&self.config.binary_folder, &self.config.socket_path, &run.id, &launch.position);
            let arguments = session_arguments(&launch, &tools_config);
            let working_folder = mission_working_folder(&state, folder_index, &run.id, mission)?;
            SpawnRequest { run: run.id, position: launch.position, mission, working_folder, arguments }
        };
        let run_id = spawn_request.run.clone();
        let position = spawn_request.position.clone();
        let info = spawn_session(self, spawn_request)?;
        let started = NewEntry { kind: EntryKind::SessionStarted, mission, to: None, text: started_text(&position, mission, starter), answers: None };
        self.post_entry(&run_id, &position, started)?;
        Ok(Reply::Session { session: info })
    }

    pub fn stop_session(&self, run_key: &str, position: &str) -> Result<Reply, String> {
        let mut state = self.state.lock().unwrap();
        let (_, run) = state.find_run(run_key)?;
        let session = state.session_in_run(&run.id, position)?;
        session.is_stopping = true;
        session.end()?;
        Ok(Reply::Done)
    }

    pub fn list_sessions(&self, run_key: Option<&str>) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let run_id = run_key.map(|key| state.find_run(key).map(|(_, run)| run.id)).transpose()?;
        let mut sessions: Vec<SessionInfo> = state
            .sessions
            .values()
            .filter(|session| run_id.as_ref().is_none_or(|id| &session.run == id))
            .map(Session::info)
            .collect();
        sessions.sort_by(|first, second| (&first.run, &first.position).cmp(&(&second.run, &second.position)));
        Ok(Reply::Sessions { sessions })
    }

    pub fn read_screen(&self, run_key: &str, position: &str) -> Result<Reply, String> {
        let mut state = self.state.lock().unwrap();
        let (_, run) = state.find_run(run_key)?;
        Ok(Reply::Screen { text: state.session_in_run(&run.id, position)?.screen_text() })
    }

    pub fn press_key(&self, run_key: &str, position: &str, key_name: &str) -> Result<Reply, String> {
        let key = TerminalKey::from_name(key_name)?;
        let mut state = self.state.lock().unwrap();
        let (_, run) = state.find_run(run_key)?;
        state.session_in_run(&run.id, position)?.press_key(key)?;
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
