//! Bringing sessions back after legion2d restarts: each open deployment's
//! commander, and every operator that was on a mission, under the same name.
//! Those the machine has no room for yet wait, and start as room opens up.

use std::sync::Arc;

use legion2_proto::{EntryKind, MissionStatus, NewEntry, COMMANDER, HUMAN, LEGION};

use crate::{
    daemon::{Daemon, PendingRestore},
    naming::operator_of_position,
    session_commands::{is_busy, StartRequest},
    store::RunningSession,
};

const LEGION_RESTARTED_COMMANDER_PROMPT: &str = "Legion restarted, and you with it. Every session in the deployment was cut off; Legion is starting again, under the same name and as the machine has room, the operator holding each started mission and every operator that was on a handed-off one. Operators that had finished their step on a started mission don't come back: start any of them again if the work still needs them. Read the deployment log and the missions with your tools, and carry on from there.";

pub fn restarted_operator_prompt(mission: u32, part: Option<u32>) -> String {
    let on_part = part.map(|number| format!(", on part {number}")).unwrap_or_default();
    format!("Legion restarted while you were on mission {mission}{on_part}. Read its deployment log entries with the log tool (mission {mission}) and carry on.")
}

/// Who comes back. On a started mission, only its holder: the operators that
/// finished their step on it would only wake to find nothing to do, and each
/// start takes the holder, so the holder starts last and keeps it. On any
/// other mission (handed off, paused), every operator recorded on it, since
/// one may be mid-work without holding it. An operator with no mission was
/// only waiting, so the commander can start it again when there's work.
pub fn sessions_to_restore(deployment_id: &str, recorded: &[RunningSession], held_missions: &[(String, u32)]) -> Vec<PendingRestore> {
    let held_mission_numbers: Vec<u32> = held_missions.iter().map(|(_, mission)| *mission).collect();
    let on_other_missions = recorded
        .iter()
        .filter(|session| session.position != COMMANDER)
        .filter_map(|session| session.mission.map(|mission| (session, mission)))
        .filter(|(_, mission)| !held_mission_numbers.contains(mission))
        .map(|(session, mission)| PendingRestore { deployment: deployment_id.to_string(), position: session.position.clone(), mission, part: session.part });
    let holders = held_missions.iter().filter(|(holder, _)| holder != COMMANDER).map(|(holder, mission)| {
        let part = recorded.iter().find(|session| &session.position == holder && session.mission == Some(*mission)).and_then(|session| session.part);
        PendingRestore { deployment: deployment_id.to_string(), position: holder.clone(), mission: *mission, part }
    });
    on_other_missions.chain(holders).collect()
}

fn note(to: Option<&str>, mission: Option<u32>, text: String) -> NewEntry {
    let kind = if to == Some(COMMANDER) { EntryKind::Message } else { EntryKind::Note };
    NewEntry { kind, mission, to: to.map(String::from), text, answers: None }
}

impl Daemon {
    /// After legion2d starts: every open deployment's commander starts again,
    /// then every operator that was on a mission, as the machine has room.
    pub fn restore_open_deployments(self: &Arc<Self>) {
        let deployments_to_restore: Vec<(String, Vec<PendingRestore>)> = {
            let state = self.state.lock().unwrap();
            state
                .folders
                .iter()
                .flat_map(|folder| {
                    folder.deployments.iter().filter(|deployment| deployment.closed_ms.is_none()).map(|deployment| {
                        let recorded = folder.store.running_sessions(&deployment.id).unwrap_or_default();
                        let held_missions: Vec<(String, u32)> = folder
                            .store
                            .missions(&deployment.id)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|mission| mission.status == MissionStatus::Started)
                            .filter_map(|mission| mission.holder.map(|holder| (holder, mission.number)))
                            .collect();
                        // Each session records itself again as it starts.
                        if let Err(error) = folder.store.forget_running_sessions(&deployment.id) {
                            eprintln!("{LEGION}d: {error}");
                        }
                        (deployment.id.clone(), sessions_to_restore(&deployment.id, &recorded, &held_missions))
                    })
                })
                .collect()
        };
        deployments_to_restore.into_iter().for_each(|(deployment_id, restores)| {
            let cut_off = "legion2d restarted, which cut off every session in this deployment".to_string();
            self.post_legion_entry(&deployment_id, note(None, None, cut_off));
            if let Err(error) = self.start_session(&deployment_id, COMMANDER, None, LEGION, Some(LEGION_RESTARTED_COMMANDER_PROMPT.into())) {
                self.post_legion_entry(&deployment_id, note(Some(HUMAN), None, format!("couldn't start the commander again: {error}")));
            }
            self.state.lock().unwrap().pending_restores.extend(restores);
        });
        self.restart_pending_sessions();
    }

    /// Whether one more busy session fits under the machine's limit.
    fn has_room_for_a_session(&self) -> Result<bool, String> {
        let limit = self.machine_settings()?.max_busy_sessions;
        let busy_count = self.state.lock().unwrap().sessions.values().filter(|session| is_busy(session.activity)).count();
        Ok(busy_count < limit)
    }

    /// Starts the sessions waiting to come back, oldest first, while there's room.
    pub fn restart_pending_sessions(self: &Arc<Self>) {
        loop {
            match self.has_room_for_a_session() {
                Ok(true) => {}
                Ok(false) => return,
                Err(error) => return eprintln!("{LEGION}d: {error}"),
            }
            let next = {
                let mut state = self.state.lock().unwrap();
                if state.pending_restores.is_empty() {
                    return;
                }
                state.pending_restores.remove(0)
            };
            let request = StartRequest {
                operator: operator_of_position(&next.position),
                mission: Some(next.mission),
                part: next.part,
                position: Some(&next.position),
                starter: LEGION,
                first_prompt: Some(restarted_operator_prompt(next.mission, next.part)),
            };
            if let Err(error) = self.start_session_on(&next.deployment, request) {
                let text = format!("couldn't start {} again on mission {}: {error}", next.position, next.mission);
                self.post_legion_entry(&next.deployment, note(Some(COMMANDER), Some(next.mission), text));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded(position: &str, mission: Option<u32>, part: Option<u32>) -> RunningSession {
        RunningSession { deployment: "r".into(), position: position.into(), mission, part, session_id: position.into() }
    }

    fn restore(position: &str, mission: u32, part: Option<u32>) -> PendingRestore {
        PendingRestore { deployment: "r".into(), position: position.into(), mission, part }
    }

    #[test]
    fn every_operator_on_a_mission_comes_back_under_its_name() {
        let running = [recorded("commander", None, None), recorded("bill-verifier-3", Some(2), None), recorded("builder-2", Some(4), Some(1)), recorded("planner", None, None)];
        assert_eq!(sessions_to_restore("r", &running, &[]), [restore("bill-verifier-3", 2, None), restore("builder-2", 4, Some(1))]);
    }

    #[test]
    fn holders_missing_from_the_record_come_back_too() {
        let running = [recorded("builder", Some(1), None)];
        let held = [("builder".to_string(), 1), ("parser-tester-4".to_string(), 3)];
        assert_eq!(sessions_to_restore("r", &running, &held), [restore("builder", 1, None), restore("parser-tester-4", 3, None)]);
    }

    #[test]
    fn on_a_started_mission_only_its_holder_comes_back_and_last() {
        let running = [recorded("spec-reader", Some(35), None), recorded("shipper-2", Some(35), None), recorded("pattern-checker", Some(35), Some(2)), recorded("integrator", Some(36), None)];
        let held = [("pattern-checker".to_string(), 35)];
        assert_eq!(sessions_to_restore("r", &running, &held), [restore("integrator", 36, None), restore("pattern-checker", 35, Some(2))]);
    }

    #[test]
    fn a_restarted_operator_reads_its_missions_log() {
        assert!(restarted_operator_prompt(4, None).contains("on mission 4. Read"));
        assert!(restarted_operator_prompt(4, Some(2)).contains("on mission 4, on part 2."));
        assert!(restarted_operator_prompt(4, None).contains("log tool (mission 4)"));
    }
}
