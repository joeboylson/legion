//! Creating, reading and finishing missions.

use std::fs;

use legion_proto::{EntryKind, LogFilter, MissionStatus, NewEntry, Reply, SessionInfo, COMMANDER, HUMAN, LEGION, STRATEGIST};

use crate::{
    constants::MISSIONS_FOLDER_NAME,
    daemon::{Daemon, State},
    git::{finish_mission_branch, FinishOutcome},
    handoff_route::{handoff_parts, waiting_mission_for_each, HandedOffMission},
    naming::mission_slug,
    session_lifecycle::end_mission_sessions,
    setup::read_settings,
    store::RunningSession,
};

/// The file a mission is written to: its number and its title.
pub fn mission_file_name(number: u32, title: &str) -> String {
    format!("{number:04}-{}.md", mission_slug(title))
}

/// What the strategist is asked once a mission is finished.
pub fn postmortem_request(mission: u32) -> String {
    format!("Mission {mission} is finished. Write its postmortem with the postmortem tool (mission {mission}): what slowed it down, what sped it up, and what to do differently next run, in a few short lines. Read its log first (the log tool, mission {mission}).")
}

/// The deployment log entry for how a finish went.
/// A mission with no branch here closes with nothing to move.
pub fn no_branch_finish_entry(mission: u32) -> NewEntry {
    NewEntry {
        kind: EntryKind::Finished,
        mission: Some(mission),
        to: None,
        text: format!("mission {mission} closed; it has no branch in this folder, so there's nothing to move"),
        answers: None,
    }
}

pub fn finish_entry(outcome: FinishOutcome, mission: u32, branch: &str, base: &str) -> NewEntry {
    let (kind, to, text) = match outcome {
        FinishOutcome::Moved => (EntryKind::Finished, None, format!("{base} moved up to {branch}")),
        FinishOutcome::Replayed { was_checked } => {
            let check_note = if was_checked { ", the check passed" } else { "" };
            (EntryKind::Finished, None, format!("{base} had moved on: replayed {branch} onto it{check_note}, and moved {base} up"))
        }
        FinishOutcome::NoChanges => (EntryKind::Finished, None, format!("{branch} had no commits; nothing to move")),
        // Left done: the builder fixes only the clash and reports done again,
        // then the finish runs again with no new round of checks.
        FinishOutcome::Clash { files } => (
            EntryKind::Message,
            Some(COMMANDER.to_string()),
            format!(
                "{branch} clashes with {base} in {}. Send it to the operator that built mission {mission}: it runs `git rebase {base}` in the mission's folder, fixes only the clash (keeping both sides where both belong), checks the change still works, commits, and reports the mission done again. Then use the finish tool again; it doesn't need to go through the checks again.",
                files.join(", ")
            ),
        ),
        FinishOutcome::NeedsHuman(reason) => {
            (EntryKind::Blocked, Some(HUMAN.to_string()), format!("{branch} can't finish on its own: {reason}. It's left as it is for you."))
        }
    };
    NewEntry { kind, mission: Some(mission), to, text, answers: None }
}

/// What a session hears when it's moved onto a mission queued behind the one it finished.
pub fn carry_on_text(finished: u32, next: u32, handoff_note: Option<&str>) -> String {
    let handed = handoff_note.map(|note| format!(" It was handed to you as:\n\n{note}")).unwrap_or_default();
    format!("Mission {finished} is finished. Carry on with mission {next}; you're on it now.{handed}")
}

/// A session moved onto a waiting mission, and what to tell it.
struct CarryOn {
    info: SessionInfo,
    message: NewEntry,
}

/// Moves each session on the finished mission onto a mission handed off to
/// its operator that no one is on, instead of ending it: a copy given a batch
/// would otherwise strand the rest of it. Only where finishing removed no
/// worktree, since a moved session stays in its folder.
fn carry_on_with_waiting_work(state: &mut State, folder_index: usize, deployment_id: &str, finished: u32) -> Result<Vec<CarryOn>, String> {
    let store = &state.folders[folder_index].store;
    let is_live_on = |mission: u32| {
        state.sessions.values().any(|session| session.deployment == deployment_id && session.mission == Some(mission) && !session.is_stopping())
    };
    let latest_handoff = |mission: u32| -> Result<Option<String>, String> {
        let handoffs = LogFilter { mission: Some(mission), kinds: Some(vec![EntryKind::Handoff]), ..Default::default() };
        Ok(store.entries(deployment_id, &handoffs)?.pop().map(|entry| entry.text))
    };
    let missions: Vec<HandedOffMission> = store
        .missions(deployment_id)?
        .into_iter()
        .filter(|mission| mission.status == MissionStatus::HandedOff)
        .map(|mission| Ok(HandedOffMission { number: mission.number, status: mission.status, latest_handoff: latest_handoff(mission.number)?, has_session: is_live_on(mission.number) }))
        .collect::<Result<_, String>>()?;
    // A part has its own checkout of the finished mission, so it has nothing to carry on with.
    let finishing: Vec<String> = state
        .sessions
        .values()
        .filter(|session| session.deployment == deployment_id && session.mission == Some(finished) && session.part.is_none() && !session.is_stopping())
        .map(|session| session.position.clone())
        .collect();
    let finishing_positions: Vec<&str> = finishing.iter().map(String::as_str).collect();
    let moves = waiting_mission_for_each(&finishing_positions, &missions);
    let note_of = |mission: u32| missions.iter().find(|waiting| waiting.number == mission).and_then(|waiting| waiting.latest_handoff.clone());
    moves
        .into_iter()
        .map(|(position, next)| {
            let (session_id, session) = state
                .sessions
                .iter_mut()
                .find(|(_, session)| session.deployment == deployment_id && session.position == position)
                .ok_or_else(|| format!("{position} isn't running"))?;
            session.mission = Some(next);
            let info = session.info();
            let running = RunningSession { deployment: info.deployment.clone(), position: info.position.clone(), mission: info.mission, part: info.part, session_id: session_id.clone() };
            state.folders[folder_index].store.record_running_session(&running)?;
            let handoff = note_of(next);
            let note = handoff.as_deref().and_then(handoff_parts).map(|(_, note)| note);
            let message = NewEntry { kind: EntryKind::Message, mission: Some(next), to: Some(position.to_string()), text: carry_on_text(finished, next, note), answers: None };
            Ok(CarryOn { info, message })
        })
        .collect()
}

/// The log entry for a mission marked done by someone other than its operator.
pub fn marked_done_entry(mission: u32, by: &str, why: &str) -> NewEntry {
    NewEntry { kind: EntryKind::Done, mission: Some(mission), to: None, text: format!("marked done by {by}: {why}"), answers: None }
}

impl Daemon {
    pub fn add_mission(&self, deployment_key: &str, title: &str, body: &str) -> Result<Reply, String> {
        let (deployment_id, number) = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let folder = &state.folders[folder_index];
            let number = folder.store.next_mission_number()?;
            let mission_path = folder.outside_folder.join(MISSIONS_FOLDER_NAME).join(mission_file_name(number, title));
            fs::write(&mission_path, format!("# {title}\n\n{}\n", body.trim()))
                .map_err(|error| format!("can't write {}: {error}", mission_path.display()))?;
            folder.store.add_mission(number, &deployment.id, title, &mission_path.to_string_lossy())?;
            (deployment.id, number)
        };
        let added = NewEntry { kind: EntryKind::MissionAdded, mission: Some(number), to: Some(COMMANDER.into()), text: title.to_string(), answers: None };
        self.post_entry(&deployment_id, HUMAN, added)?;
        let state = self.state.lock().unwrap();
        let (folder_index, _) = state.find_deployment(&deployment_id)?;
        Ok(Reply::Missions { missions: vec![state.folders[folder_index].store.mission(&deployment_id, number)?] })
    }

    pub fn list_missions(&self, deployment_key: &str) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let (folder_index, deployment) = state.find_deployment(deployment_key)?;
        Ok(Reply::Missions { missions: state.folders[folder_index].store.missions(&deployment.id)? })
    }

    pub fn read_mission(&self, deployment_key: &str, number: u32) -> Result<Reply, String> {
        let (mission, body) = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_deployment(deployment_key)?;
            state.mission_with_body(folder_index, &deployment.id, number)?
        };
        let parts = self.mission_parts(deployment_key, number)?;
        Ok(Reply::Mission { mission, body, parts })
    }

    /// For a mission whose operator never reported it done: logs who marked it
    /// and why, then finishes it the same way as the finish command.
    pub fn mark_mission_done(&self, deployment_key: &str, by: &str, number: u32, why: &str) -> Result<Reply, String> {
        let deployment_id = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let mission = state.folders[folder_index].store.mission(&deployment.id, number)?;
            if mission.status == MissionStatus::Done {
                return Err(format!("mission {number} is already done; finish it instead"));
            }
            deployment.id.clone()
        };
        self.post_entry(&deployment_id, by, marked_done_entry(number, by, why))?;
        self.finish_mission(&deployment_id, number)
    }

    pub fn finish_mission(&self, deployment_key: &str, number: u32) -> Result<Reply, String> {
        let (deployment_id, entry, has_strategist, carried_on) = {
            let mut guard = self.state.lock().unwrap();
            // Through one borrow, so its folders and sessions can be used apart.
            let state = &mut *guard;
            let (folder_index, deployment) = state.find_deployment(deployment_key)?;
            let folder = &state.folders[folder_index];
            let mission = folder.store.mission(&deployment.id, number)?;
            if mission.status != MissionStatus::Done {
                return Err(format!("mission {number} isn't done yet"));
            }
            let worktree = folder.store.worktree(number)?.filter(|worktree| !worktree.is_removed);
            let has_worktree = worktree.is_some();
            // A folder that isn't a git repo gives a mission no branch of its
            // own: its work lands elsewhere (pull requests in other repos), so
            // finishing only closes the mission.
            let (is_finished, entry) = match worktree {
                None => (true, no_branch_finish_entry(number)),
                Some(worktree) => {
                    let check_command = read_settings(&folder.path)?.check;
                    let outcome = finish_mission_branch(&folder.path, &worktree, check_command.as_deref())?;
                    let is_finished = !matches!(outcome, FinishOutcome::NeedsHuman(_) | FinishOutcome::Clash { .. });
                    if is_finished {
                        folder.store.mark_worktree_removed(number)?;
                    }
                    (is_finished, finish_entry(outcome, number, &worktree.branch, &worktree.base))
                }
            };
            let has_strategist = is_finished && state.running_positions(&deployment.id).iter().any(|position| position == STRATEGIST);
            let carried_on = match (is_finished, has_worktree) {
                (true, false) => carry_on_with_waiting_work(state, folder_index, &deployment.id, number)?,
                _ => Vec::new(),
            };
            if is_finished {
                // Operators stay idle between handoffs in case the work comes
                // back; once it's finished, nothing will.
                end_mission_sessions(&mut state.sessions, &deployment.id, number)?;
            }
            (deployment.id, entry, has_strategist, carried_on)
        };
        let finished = self.post_entry(&deployment_id, LEGION, entry)?;
        carried_on.into_iter().try_for_each(|carry_on| {
            self.announce_session(&carry_on.info);
            self.post_entry(&deployment_id, LEGION, carry_on.message).map(|_| ())
        })?;
        if has_strategist {
            let request = NewEntry { kind: EntryKind::Message, mission: Some(number), to: Some(STRATEGIST.into()), text: postmortem_request(number), answers: None };
            self.post_entry(&deployment_id, LEGION, request)?;
        }
        Ok(Reply::Entry { entry: finished })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_carrying_on_hears_which_mission_and_its_handoff() {
        let text = carry_on_text(103, 104, Some("reviewed: merge it"));
        assert!(text.starts_with("Mission 103 is finished. Carry on with mission 104; you're on it now."));
        assert!(text.ends_with("It was handed to you as:\n\nreviewed: merge it"));
    }

    #[test]
    fn the_strategist_is_asked_for_a_postmortem_of_the_mission() {
        let text = postmortem_request(7);
        assert!(text.starts_with("Mission 7 is finished."));
        assert!(text.contains("postmortem tool (mission 7)") && text.contains("log tool, mission 7"));
    }

    #[test]
    fn a_mark_done_says_who_and_why() {
        let entry = marked_done_entry(4, "commander", "the checker handed off to done");
        assert_eq!((entry.kind, entry.mission, entry.to), (EntryKind::Done, Some(4), None));
        assert_eq!(entry.text, "marked done by commander: the checker handed off to done");
    }

    #[test]
    fn mission_files_are_numbered_and_named() {
        assert_eq!(mission_file_name(7, "Add a hello file"), "0007-add-a-hello-file.md");
    }

    #[test]
    fn a_clean_finish_is_logged_as_finished() {
        let entry = finish_entry(FinishOutcome::Moved, 2, "mission/x", "main");
        assert_eq!((entry.kind, entry.to, entry.mission), (EntryKind::Finished, None, Some(2)));
        assert_eq!(entry.text, "main moved up to mission/x");
    }

    #[test]
    fn a_replay_says_whether_it_was_checked() {
        let checked = finish_entry(FinishOutcome::Replayed { was_checked: true }, 2, "b", "main");
        let unchecked = finish_entry(FinishOutcome::Replayed { was_checked: false }, 2, "b", "main");
        assert!(checked.text.contains("the check passed"));
        assert!(!unchecked.text.contains("check"));
    }

    #[test]
    fn a_clash_goes_to_the_commander_and_names_the_files() {
        let entry = finish_entry(FinishOutcome::Clash { files: vec!["style.css".into(), "app.js".into()] }, 15, "mission/0015", "main");
        assert_eq!((entry.kind, entry.to.as_deref()), (EntryKind::Message, Some(COMMANDER)));
        assert!(entry.text.contains("in style.css, app.js"));
        assert!(entry.text.contains("doesn't need to go through the checks again"));
    }

    #[test]
    fn a_stuck_finish_goes_to_the_human_as_blocked() {
        let entry = finish_entry(FinishOutcome::NeedsHuman("it clashes".into()), 2, "b", "main");
        assert_eq!((entry.kind, entry.to.as_deref()), (EntryKind::Blocked, Some(HUMAN)));
        assert!(entry.text.contains("it clashes"));
    }
}
