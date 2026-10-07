//! Creating, reading and finishing missions.

use std::fs;

use legion2_proto::{EntryKind, MissionStatus, NewEntry, Reply, COMMANDER, HUMAN, LEGION, STRATEGIST};

use crate::{
    constants::MISSIONS_FOLDER_NAME,
    daemon::Daemon,
    git::{finish_mission_branch, FinishOutcome},
    naming::mission_slug,
    session_lifecycle::end_mission_sessions,
    setup::read_settings,
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

    pub fn finish_mission(&self, deployment_key: &str, number: u32) -> Result<Reply, String> {
        let (deployment_id, entry, has_strategist) = {
            let mut guard = self.state.lock().unwrap();
            // Through one borrow, so its folders and sessions can be used apart.
            let state = &mut *guard;
            let (folder_index, deployment) = state.find_deployment(deployment_key)?;
            let folder = &state.folders[folder_index];
            let mission = folder.store.mission(&deployment.id, number)?;
            if mission.status != MissionStatus::Done {
                return Err(format!("mission {number} isn't done yet"));
            }
            let worktree = folder
                .store
                .worktree(number)?
                .filter(|worktree| !worktree.is_removed)
                .ok_or_else(|| format!("mission {number} has no branch to finish"))?;
            let check_command = read_settings(&folder.path)?.check;
            let outcome = finish_mission_branch(&folder.path, &worktree, check_command.as_deref())?;
            let is_finished = !matches!(outcome, FinishOutcome::NeedsHuman(_) | FinishOutcome::Clash { .. });
            let has_strategist = is_finished && state.running_positions(&deployment.id).iter().any(|position| position == STRATEGIST);
            if is_finished {
                folder.store.mark_worktree_removed(number)?;
                // Operators stay idle between handoffs in case the work comes
                // back; once it's finished, nothing will.
                end_mission_sessions(&mut state.sessions, &deployment.id, number)?;
            }
            (deployment.id, finish_entry(outcome, number, &worktree.branch, &worktree.base), has_strategist)
        };
        let finished = self.post_entry(&deployment_id, LEGION, entry)?;
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
    fn the_strategist_is_asked_for_a_postmortem_of_the_mission() {
        let text = postmortem_request(7);
        assert!(text.starts_with("Mission 7 is finished."));
        assert!(text.contains("postmortem tool (mission 7)") && text.contains("log tool, mission 7"));
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
