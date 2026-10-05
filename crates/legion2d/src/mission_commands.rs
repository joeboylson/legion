//! Creating, reading and finishing missions.

use std::fs;

use legion2_proto::{EntryKind, MissionStatus, NewEntry, Reply, COMMANDER, HUMAN, LEGION};

use crate::{
    constants::MISSIONS_FOLDER_NAME,
    daemon::Daemon,
    git::{finish_mission_branch, FinishOutcome},
    naming::mission_slug,
    setup::read_settings,
};

/// The file a mission is written to: its number and its title.
pub fn mission_file_name(number: u32, title: &str) -> String {
    format!("{number:04}-{}.md", mission_slug(title))
}

/// The run log entry for how a finish went.
pub fn finish_entry(outcome: FinishOutcome, mission: u32, branch: &str, base: &str) -> NewEntry {
    let (kind, to, text) = match outcome {
        FinishOutcome::Moved => (EntryKind::Finished, None, format!("{base} moved up to {branch}")),
        FinishOutcome::Replayed { was_checked } => {
            let check_note = if was_checked { ", the check passed" } else { "" };
            (EntryKind::Finished, None, format!("{base} had moved on: replayed {branch} onto it{check_note}, and moved {base} up"))
        }
        FinishOutcome::NoChanges => (EntryKind::Finished, None, format!("{branch} had no commits; nothing to move")),
        FinishOutcome::NeedsHuman(reason) => {
            (EntryKind::Blocked, Some(HUMAN.to_string()), format!("{branch} can't finish on its own: {reason}. It's left as it is for you."))
        }
    };
    NewEntry { kind, mission: Some(mission), to, text, answers: None }
}

impl Daemon {
    pub fn add_mission(&self, run_key: &str, title: &str, body: &str) -> Result<Reply, String> {
        let (run_id, number) = {
            let state = self.state.lock().unwrap();
            let (folder_index, run) = state.find_open_run(run_key)?;
            let folder = &state.folders[folder_index];
            let number = folder.store.next_mission_number()?;
            let mission_path = folder.outside_folder.join(MISSIONS_FOLDER_NAME).join(mission_file_name(number, title));
            fs::write(&mission_path, format!("# {title}\n\n{}\n", body.trim()))
                .map_err(|error| format!("can't write {}: {error}", mission_path.display()))?;
            folder.store.add_mission(number, &run.id, title, &mission_path.to_string_lossy())?;
            (run.id, number)
        };
        let added = NewEntry { kind: EntryKind::MissionAdded, mission: Some(number), to: Some(COMMANDER.into()), text: title.to_string(), answers: None };
        self.post_entry(&run_id, HUMAN, added)?;
        let state = self.state.lock().unwrap();
        let (folder_index, _) = state.find_run(&run_id)?;
        Ok(Reply::Missions { missions: vec![state.folders[folder_index].store.mission(&run_id, number)?] })
    }

    pub fn list_missions(&self, run_key: &str) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let (folder_index, run) = state.find_run(run_key)?;
        Ok(Reply::Missions { missions: state.folders[folder_index].store.missions(&run.id)? })
    }

    pub fn read_mission(&self, run_key: &str, number: u32) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let (folder_index, run) = state.find_run(run_key)?;
        let (mission, body) = state.mission_with_body(folder_index, &run.id, number)?;
        Ok(Reply::Mission { mission, body })
    }

    pub fn finish_mission(&self, run_key: &str, number: u32) -> Result<Reply, String> {
        let (run_id, entry) = {
            let state = self.state.lock().unwrap();
            let (folder_index, run) = state.find_run(run_key)?;
            let folder = &state.folders[folder_index];
            let mission = folder.store.mission(&run.id, number)?;
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
            let is_finished = !matches!(outcome, FinishOutcome::NeedsHuman(_));
            if is_finished {
                folder.store.mark_worktree_removed(number)?;
            }
            (run.id, finish_entry(outcome, number, &worktree.branch, &worktree.base))
        };
        Ok(Reply::Entry { entry: self.post_entry(&run_id, LEGION, entry)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_stuck_finish_goes_to_the_human_as_blocked() {
        let entry = finish_entry(FinishOutcome::NeedsHuman("it clashes".into()), 2, "b", "main");
        assert_eq!((entry.kind, entry.to.as_deref()), (EntryKind::Blocked, Some(HUMAN)));
        assert!(entry.text.contains("it clashes"));
    }
}
