//! What Legion does on its own: when a session ends, after legion2d
//! restarts, and when a permission request goes unanswered.

use std::sync::Arc;

use legion2_proto::{EntryKind, MissionStatus, NewEntry, COMMANDER, HUMAN, LEGION};

use crate::{
    constants::{COMMANDER_RESTART_GAP_MS, MAX_WORKAROUND_ATTEMPTS, SESSION_START_GRACE},
    daemon::Daemon,
    naming::operator_of_position,
    sessions::Session,
    store::now_ms,
    terminal_key::TerminalKey,
};

const COMMANDER_RESTARTED_PROMPT: &str = "Your last session ended by itself, and Legion started you again. Read the run log and the missions with your tools, and carry on from there.";
const LEGION_RESTARTED_COMMANDER_PROMPT: &str = "Legion restarted, and you with it. Every session in the run was cut off; Legion is starting again the operators that held missions. Read the run log and the missions with your tools, and carry on from there.";

pub fn can_restart_commander(last_restart_ms: Option<i64>, now: i64) -> bool {
    last_restart_ms.is_none_or(|last| now - last >= COMMANDER_RESTART_GAP_MS)
}

fn entry(kind: EntryKind, to: Option<&str>, mission: Option<u32>, text: String) -> NewEntry {
    NewEntry { kind, mission, to: to.map(String::from), text, answers: None }
}

pub fn minutes_label(minutes: u64) -> String {
    match minutes {
        1 => "1 minute".into(),
        _ => format!("{minutes} minutes"),
    }
}

/// What the operator and the commander hear when a permission request is refused.
pub fn refusal_messages(position: &str, timeout_minutes: u64, request: &str) -> (String, String) {
    let waited = minutes_label(timeout_minutes);
    let to_operator = format!(
        "Your permission request was refused: no one answered in {waited} ({request}). Find a way around it, working closely with the commander: at most {MAX_WORKAROUND_ATTEMPTS} attempts. After that, the mission goes on as best it can without it, and the gap goes in its result; if it can't go on without it, report it blocked."
    );
    let to_commander = format!(
        "{position}'s permission request was refused after {waited} with no answer ({request}). Allow it at most {MAX_WORKAROUND_ATTEMPTS} workaround attempts."
    );
    (to_operator, to_commander)
}

pub fn stuck_starting_text(position: &str) -> String {
    let grace = SESSION_START_GRACE.as_secs();
    format!(
        "{position}'s session hasn't started after {grace} seconds. It's likely waiting on a question Legion can't see, such as whether to trust the folder. Look with `legion2 screen {position}` and answer with `legion2 key {position} <key>`."
    )
}

fn restarted_operator_prompt(mission: u32) -> String {
    format!("Legion restarted while you were on mission {mission}. Read its run log entries with the log tool (mission {mission}) and carry on.")
}

impl Daemon {
    /// If no one asked for the session to end, the commander starts again (at
    /// most once per gap), or hears that its operator's session ended.
    pub fn handle_session_end(self: &Arc<Self>, session: &Session, how_it_ended: &str) {
        self.announce_session(&legion2_proto::SessionInfo { activity: legion2_proto::Activity::Ended, ..session.info() });
        let ended_text = format!("{}'s session ended ({how_it_ended})", session.position);
        self.post_legion_entry(&session.run, entry(EntryKind::SessionEnded, None, session.mission, ended_text));
        let is_run_open = self.state.lock().unwrap().find_open_run(&session.run).is_ok();
        if session.is_stopping || !is_run_open {
            return;
        }
        if session.position != COMMANDER {
            let holding = session.mission.map(|mission| format!(" while holding mission {mission}")).unwrap_or_default();
            let text = format!("{}'s session ended by itself{holding}. Start it again if there's still work for it.", session.position);
            self.post_legion_entry(&session.run, entry(EntryKind::Message, Some(COMMANDER), session.mission, text));
            return;
        }
        let now = now_ms();
        let may_restart = {
            let mut state = self.state.lock().unwrap();
            let may_restart = can_restart_commander(state.commander_restarted_at.get(&session.run).copied(), now);
            if may_restart {
                state.commander_restarted_at.insert(session.run.clone(), now);
            }
            may_restart
        };
        if !may_restart {
            let text = "the commander ended by itself again soon after a restart, so Legion left it down. Start it with `legion2 start commander`.";
            self.post_legion_entry(&session.run, entry(EntryKind::Note, Some(HUMAN), None, text.into()));
            return;
        }
        let restart_entry = match self.start_session(&session.run, COMMANDER, None, LEGION, Some(COMMANDER_RESTARTED_PROMPT.into())) {
            Ok(_) => entry(EntryKind::Note, None, None, "the commander ended by itself; Legion started it again".into()),
            Err(error) => entry(EntryKind::Note, Some(HUMAN), None, format!("the commander ended by itself, and starting it again failed: {error}")),
        };
        self.post_legion_entry(&session.run, restart_entry);
    }

    /// After legion2d starts: every open run's commander starts again, and so
    /// does every operator that held a mission in progress.
    pub fn restore_open_runs(self: &Arc<Self>) {
        let runs_to_restore: Vec<(String, Vec<(String, u32)>)> = {
            let state = self.state.lock().unwrap();
            state
                .folders
                .iter()
                .flat_map(|folder| {
                    folder.runs.iter().filter(|run| run.closed_ms.is_none()).map(|run| {
                        let held_missions = folder
                            .store
                            .missions(&run.id)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|mission| mission.status == MissionStatus::Started)
                            .filter_map(|mission| mission.holder.map(|holder| (operator_of_position(&holder).to_string(), mission.number)))
                            .collect();
                        (run.id.clone(), held_missions)
                    })
                })
                .collect()
        };
        runs_to_restore.into_iter().for_each(|(run_id, held_missions)| {
            let cut_off = "legion2d restarted, which cut off every session in this run".to_string();
            self.post_legion_entry(&run_id, entry(EntryKind::Note, None, None, cut_off));
            if let Err(error) = self.start_session(&run_id, COMMANDER, None, LEGION, Some(LEGION_RESTARTED_COMMANDER_PROMPT.into())) {
                self.post_legion_entry(&run_id, entry(EntryKind::Note, Some(HUMAN), None, format!("couldn't start the commander again: {error}")));
            }
            held_missions.into_iter().for_each(|(operator, mission)| {
                if let Err(error) = self.start_session(&run_id, &operator, Some(mission), LEGION, Some(restarted_operator_prompt(mission))) {
                    let text = format!("couldn't start {operator} again on mission {mission}: {error}");
                    self.post_legion_entry(&run_id, entry(EntryKind::Message, Some(COMMANDER), Some(mission), text));
                }
            });
        });
    }

    /// Tells the human about sessions whose add-on hasn't reported in time,
    /// once each.
    pub fn report_sessions_stuck_starting(&self) {
        let stuck: Vec<(String, String, Option<u32>)> = {
            let mut state = self.state.lock().unwrap();
            state
                .sessions
                .values_mut()
                .filter(|session| session.activity == legion2_proto::Activity::Starting && !session.is_reported_stuck)
                .filter(|session| session.started_at.elapsed() >= SESSION_START_GRACE)
                .map(|session| {
                    session.is_reported_stuck = true;
                    (session.run.clone(), session.position.clone(), session.mission)
                })
                .collect()
        };
        stuck.into_iter().for_each(|(run_id, position, mission)| {
            self.post_legion_entry(&run_id, entry(EntryKind::Note, Some(HUMAN), mission, stuck_starting_text(&position)));
        });
    }

    /// Refuses permission requests no one has answered in time, and tells
    /// the operator and the commander how to go on without them.
    pub fn refuse_unanswered_permissions(&self) {
        let timeout = self.config.permission_timeout;
        let refused: Vec<(String, String, Option<u32>, String)> = {
            let mut state = self.state.lock().unwrap();
            state
                .sessions
                .values_mut()
                .filter(|session| session.permission_asked_at.is_some_and(|asked_at| asked_at.elapsed() >= timeout))
                .map(|session| {
                    session.permission_asked_at = None;
                    // Escape answers the question with no.
                    let pressed = session.press_key(TerminalKey::Escape);
                    if let Err(error) = pressed {
                        eprintln!("{LEGION}d: {error}");
                    }
                    (session.run.clone(), session.position.clone(), session.mission, session.detail.clone().unwrap_or_default())
                })
                .collect()
        };
        let timeout_minutes = timeout.as_secs() / 60;
        refused.into_iter().for_each(|(run_id, position, mission, request)| {
            let (to_operator, to_commander) = refusal_messages(&position, timeout_minutes, &request);
            self.post_legion_entry(&run_id, entry(EntryKind::Message, Some(&position), mission, to_operator));
            self.post_legion_entry(&run_id, entry(EntryKind::Message, Some(COMMANDER), mission, to_commander));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commander_restarts_once_per_gap() {
        assert!(can_restart_commander(None, 0));
        assert!(!can_restart_commander(Some(1_000), 1_000 + COMMANDER_RESTART_GAP_MS - 1));
        assert!(can_restart_commander(Some(1_000), 1_000 + COMMANDER_RESTART_GAP_MS));
    }

    #[test]
    fn refusals_tell_both_sides_the_attempt_limit() {
        let (to_operator, to_commander) = refusal_messages("builder", 30, "Bash: rm -rf build");
        assert!(to_operator.contains("30 minutes") && to_operator.contains("Bash: rm -rf build"));
        assert!(refusal_messages("builder", 1, "x").1.contains("after 1 minute with"));
        assert!(to_operator.contains(&format!("at most {MAX_WORKAROUND_ATTEMPTS} attempts")));
        assert!(to_commander.starts_with("builder's permission request"));
    }

    #[test]
    fn a_stuck_session_tells_the_human_how_to_look() {
        let text = stuck_starting_text("planner");
        assert!(text.contains("legion2 screen planner") && text.contains("legion2 key planner"));
    }

    #[test]
    fn a_restarted_operator_reads_its_missions_log() {
        assert!(restarted_operator_prompt(4).contains("log tool (mission 4)"));
    }
}
