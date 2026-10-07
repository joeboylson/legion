//! What Legion does on its own: when a session ends, when a session is
//! stuck starting or working a long while, when work stalls, when a
//! permission request goes unanswered, and when a mission is finished.

use std::{collections::HashMap, sync::Arc, time::Duration};

use legion2_proto::{role_of_position, EntryKind, LogFilter, NewEntry, Role, COMMANDER, HUMAN, LEGION, STRATEGIST};

use crate::{
    stalled_work::{stall_reminder, stalls, MissionView, Stall},
    session_commands::{is_busy, StartRequest},
    setup::{read_operator, read_pipeline},
    check_ins::{check_in_request, is_check_in_due},
    context_handover::{fresh_start_prompt, HandoverPhase},
    constants::{SECONDS_PER_MINUTE, COMMANDER_RESTART_GAP_MS, DEFAULT_OPERATOR_COPY_LIMIT, MAX_WORKAROUND_ATTEMPTS, SESSION_START_GRACE},
    daemon::Daemon,
    naming::operator_of_position,
    sessions::Session,
    store::now_ms,
    terminal_key::TerminalKey,
};

const COMMANDER_RESTARTED_PROMPT: &str = "Your last session ended by itself, and Legion started you again. Read the deployment log and the missions with your tools, and carry on from there.";

pub fn can_restart_commander(last_restart_ms: Option<i64>, now: i64) -> bool {
    last_restart_ms.is_none_or(|last| now - last >= COMMANDER_RESTART_GAP_MS)
}

fn entry(kind: EntryKind, to: Option<&str>, mission: Option<u32>, text: String) -> NewEntry {
    NewEntry { kind, mission, to: to.map(String::from), text, answers: None }
}

/// Ends every session working on the mission, on purpose: none of them is
/// treated as a crash or brought back.
pub fn end_mission_sessions(sessions: &mut HashMap<String, Session>, deployment_id: &str, mission: u32) -> Result<(), String> {
    sessions
        .values_mut()
        .filter(|session| session.deployment == deployment_id && session.mission == Some(mission))
        .try_for_each(|session| {
            session.is_stopping = true;
            session.end()
        })
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

impl Daemon {
    /// If no one asked for the session to end, the commander starts again (at
    /// most once per gap), or hears that its operator's session ended.
    pub fn handle_session_end(self: &Arc<Self>, session: &Session, how_it_ended: &str) {
        self.announce_session(&legion2_proto::SessionInfo { activity: legion2_proto::Activity::Ended, ..session.info() });
        let ended_text = format!("{}'s session ended ({how_it_ended})", session.position);
        self.post_legion_entry(&session.deployment, entry(EntryKind::SessionEnded, None, session.mission, ended_text));
        let is_deployment_open = self.state.lock().unwrap().find_open_deployment(&session.deployment).is_ok();
        if session.handover == HandoverPhase::Restarting && is_deployment_open {
            return self.start_fresh(session);
        }
        // The strategist isn't the commander's to start; Legion's timer brings it back.
        if session.is_stopping || !is_deployment_open || session.position == STRATEGIST {
            return;
        }
        if session.position != COMMANDER {
            let holding = session.mission.map(|mission| format!(" while holding mission {mission}")).unwrap_or_default();
            let text = format!("{}'s session ended by itself{holding}. Start it again if there's still work for it.", session.position);
            self.post_legion_entry(&session.deployment, entry(EntryKind::Message, Some(COMMANDER), session.mission, text));
            return;
        }
        let now = now_ms();
        let may_restart = {
            let mut state = self.state.lock().unwrap();
            let may_restart = can_restart_commander(state.commander_restarted_at.get(&session.deployment).copied(), now);
            if may_restart {
                state.commander_restarted_at.insert(session.deployment.clone(), now);
            }
            may_restart
        };
        if !may_restart {
            let text = "the commander ended by itself again soon after a restart, so Legion left it down. Start it with `legion2 start commander`.";
            self.post_legion_entry(&session.deployment, entry(EntryKind::Note, Some(HUMAN), None, text.into()));
            return;
        }
        let restart_entry = match self.start_session(&session.deployment, COMMANDER, None, LEGION, Some(COMMANDER_RESTARTED_PROMPT.into())) {
            Ok(_) => entry(EntryKind::Note, None, None, "the commander ended by itself; Legion started it again".into()),
            Err(error) => entry(EntryKind::Note, Some(HUMAN), None, format!("the commander ended by itself, and starting it again failed: {error}")),
        };
        self.post_legion_entry(&session.deployment, restart_entry);
    }

    /// Starts a position again, under the same name, after it handed over a full conversation.
    fn start_fresh(self: &Arc<Self>, session: &Session) {
        let mission = if session.position == COMMANDER { None } else { session.mission };
        let request = StartRequest {
            operator: operator_of_position(&session.position),
            mission,
            part: session.part,
            position: Some(&session.position),
            starter: LEGION,
            first_prompt: Some(fresh_start_prompt(mission)),
        };
        let fresh_entry = match self.start_session_on(&session.deployment, request) {
            Ok(_) => entry(EntryKind::Note, None, session.mission, format!("{}'s conversation was full; Legion started it again fresh", session.position)),
            Err(error) => entry(EntryKind::Note, Some(HUMAN), session.mission, format!("couldn't start {} again after its handover: {error}", session.position)),
        };
        self.post_legion_entry(&session.deployment, fresh_entry);
    }

    /// Tells the human about sessions whose add-on hasn't reported in time,
    /// once each.
    pub fn report_sessions_stuck_starting(&self) {
        let stuck: Vec<(legion2_proto::SessionInfo, Option<u32>)> = {
            let mut state = self.state.lock().unwrap();
            state
                .sessions
                .values_mut()
                .filter(|session| session.activity == legion2_proto::Activity::Starting && !session.is_reported_stuck)
                .filter(|session| session.started_at.elapsed() >= SESSION_START_GRACE)
                .map(|session| {
                    session.is_reported_stuck = true;
                    (session.info(), session.mission)
                })
                .collect()
        };
        stuck.into_iter().for_each(|(info, mission)| {
            self.announce_session(&info);
            self.post_legion_entry(&info.deployment, entry(EntryKind::Note, Some(HUMAN), mission, stuck_starting_text(&info.position)));
        });
    }

    /// Asks the commander to look in on each operator that has been working
    /// a long while.
    pub fn ask_commander_to_check_in(&self) {
        let now = now_ms();
        let due: Vec<(String, String, Option<u32>, i64)> = {
            let mut state = self.state.lock().unwrap();
            state
                .sessions
                .values_mut()
                .filter(|session| role_of_position(&session.position) == Role::Operator && session.activity == legion2_proto::Activity::Busy)
                .filter(|session| is_check_in_due(session.turn_started_ms, session.last_check_in_ms, now))
                .map(|session| {
                    session.last_check_in_ms = Some(now);
                    let working_ms = now - session.turn_started_ms.unwrap_or(now);
                    (session.deployment.clone(), session.position.clone(), session.mission, working_ms)
                })
                .collect()
        };
        due.into_iter().for_each(|(deployment, position, mission, working_ms)| {
            self.post_legion_entry(&deployment, entry(EntryKind::Message, Some(COMMANDER), mission, check_in_request(&position, mission, working_ms)));
        });
    }

    /// Tells each idle commander about work it has lost track of, once each.
    pub fn point_out_stalled_work(&self) {
        let now = now_ms();
        let reminders: Vec<(String, String)> = {
            let mut state = self.state.lock().unwrap();
            let idle_commanders: Vec<String> = state
                .sessions
                .values()
                .filter(|session| session.position == COMMANDER && session.activity == legion2_proto::Activity::Idle)
                .map(|session| session.deployment.clone())
                .collect();
            let found: Vec<(String, String, Vec<Stall>)> = idle_commanders
                .iter()
                .filter_map(|deployment_id| {
                    let (folder_index, deployment) = state.find_open_deployment(deployment_id).ok()?;
                    let folder = &state.folders[folder_index];
                    let (pipeline, _) = read_pipeline(&folder.path, &deployment.pipeline).ok()?;
                    let limit = read_operator(&folder.path, &pipeline.first).ok()?.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT);
                    let deployment_sessions: Vec<&Session> = state.sessions.values().filter(|session| &session.deployment == deployment_id).collect();
                    let first_copies: Vec<&&Session> =
                        deployment_sessions.iter().filter(|session| operator_of_position(&session.position) == pipeline.first).collect();
                    let first_has_room = first_copies.len() < limit as usize || first_copies.iter().any(|session| session.activity == legion2_proto::Activity::Idle);
                    let missions: Vec<MissionView> = folder
                        .store
                        .missions(deployment_id)
                        .ok()?
                        .into_iter()
                        .map(|mission| {
                            let handoffs = LogFilter { mission: Some(mission.number), kinds: Some(vec![EntryKind::Handoff]), ..Default::default() };
                            MissionView {
                                number: mission.number,
                                status: mission.status,
                                handed_off_at_ms: folder.store.entries(deployment_id, &handoffs).ok().and_then(|entries| entries.last().map(|entry| entry.at_ms)),
                                is_being_worked: deployment_sessions.iter().any(|session| session.mission == Some(mission.number) && is_busy(session.activity)),
                            }
                        })
                        .collect();
                    Some((deployment_id.clone(), pipeline.first, stalls(&missions, first_has_room, now)))
                })
                .collect();
            found
                .into_iter()
                .filter_map(|(deployment_id, first, found_stalls)| {
                    let fresh: Vec<Stall> = found_stalls
                        .into_iter()
                        .filter(|stall| state.stalls_pointed_out.insert(format!("{deployment_id}:{}", stall.key())))
                        .collect();
                    (!fresh.is_empty()).then(|| (deployment_id, stall_reminder(&fresh, &first)))
                })
                .collect()
        };
        reminders.into_iter().for_each(|(deployment_id, text)| {
            self.post_legion_entry(&deployment_id, entry(EntryKind::Message, Some(COMMANDER), None, text));
        });
    }

    /// Refuses permission requests no one has answered in time, and tells
    /// the operator and the commander how to go on without them.
    pub fn refuse_unanswered_permissions(&self) {
        let timeout_minutes = match self.machine_settings() {
            Ok(settings) => settings.permission_timeout_minutes,
            Err(error) => return eprintln!("{LEGION}d: {error}"),
        };
        let timeout = Duration::from_secs(timeout_minutes * SECONDS_PER_MINUTE);
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
                    (session.deployment.clone(), session.position.clone(), session.mission, session.detail.clone().unwrap_or_default())
                })
                .collect()
        };
        refused.into_iter().for_each(|(deployment_id, position, mission, request)| {
            let (to_operator, to_commander) = refusal_messages(&position, timeout_minutes, &request);
            self.post_legion_entry(&deployment_id, entry(EntryKind::Message, Some(&position), mission, to_operator));
            self.post_legion_entry(&deployment_id, entry(EntryKind::Message, Some(COMMANDER), mission, to_commander));
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
}
