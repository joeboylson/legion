//! Splitting a mission's step into parts. The commander splits it; copies of
//! one operator work the parts at the same time, each in its own checkout
//! branched from the mission's; Legion merges each part back as it's done and
//! tells the commander once every part is in. Parts aren't missions: the
//! mission still moves through the pipeline as one.

use std::path::{Path, PathBuf};

use legion2_proto::{EntryKind, NewEntry, Part, Reply, COMMANDER, LEGION};

use crate::{
    constants::WORKTREES_FOLDER_NAME,
    daemon::Daemon,
    git::{create_part_worktree, is_git_repo, merge_branch, remove_worktree, MergeOutcome},
    store::PartCheckout,
};

/// A part's branch. Not `<mission branch>/part-1`: git can't keep a branch
/// inside another branch's name.
pub fn part_branch(mission_branch: &str, number: u32) -> String {
    format!("{mission_branch}--part-{number}")
}

pub fn part_checkout_path(worktrees_folder: &Path, mission_folder_name: &str, number: u32) -> PathBuf {
    worktrees_folder.join(format!("{mission_folder_name}--part-{number}"))
}

/// What an operator on a part is told, on top of its mission briefing.
pub fn part_instructions(mission: u32, part: &Part) -> String {
    format!(
        "You're on part {number} of mission {mission}: {brief}\n\nDo only this part; other operators are doing the other parts at the same time, in their own folders. When it's built and committed, report it with the part_done tool (mission {mission}, part {number}), not handoff. Legion merges it into the mission.",
        number = part.number,
        brief = part.brief,
    )
}

pub fn part_first_prompt(mission: u32, part: u32) -> String {
    format!("Start on part {part} of mission {mission}.")
}

pub fn split_note(mission: u32, parts: &[Part]) -> String {
    let listed: Vec<String> = parts.iter().map(|part| format!("{}) {}", part.number, part.brief)).collect();
    format!("mission {mission} is split into {} parts: {}", parts.len(), listed.join("; "))
}

pub fn all_merged_message(mission: u32) -> String {
    format!("Every part of mission {mission} is merged into its branch. Pass it to the next step in the pipeline, as one mission.")
}

pub fn part_clash_message(mission: u32, part: u32, mission_branch: &str, files: &[String]) -> String {
    format!(
        "Part {part} of mission {mission} clashes with the mission in {}. In your folder, run `git merge {mission_branch}`, fix only the clash (keeping both sides where both belong), commit, then use part_done again.",
        files.join(", ")
    )
}

impl Daemon {
    pub fn split_mission(&self, deployment_key: &str, mission: u32, briefs: &[String]) -> Result<Reply, String> {
        let (deployment_id, parts) = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let folder = &state.folders[folder_index];
            if !is_git_repo(&folder.path) {
                return Err(format!("{} isn't in git, so its missions can't be split into separate checkouts", folder.path.display()));
            }
            let worktree = folder
                .store
                .worktree(mission)?
                .filter(|worktree| !worktree.is_removed)
                .ok_or_else(|| format!("mission {mission} has no checkout yet; start someone on it before splitting it"))?;
            let mission_folder_name = Path::new(&worktree.path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            let first_number = folder.store.parts(mission)?.len() as u32 + 1;
            let worktrees_folder = folder.outside_folder.join(WORKTREES_FOLDER_NAME);
            let parts = briefs
                .iter()
                .zip(first_number..)
                .map(|(brief, number)| {
                    let checkout = PartCheckout {
                        part: Part { mission, number, brief: brief.clone(), is_merged: false },
                        path: part_checkout_path(&worktrees_folder, &mission_folder_name, number).to_string_lossy().into_owned(),
                        branch: part_branch(&worktree.branch, number),
                    };
                    create_part_worktree(&folder.path, Path::new(&checkout.path), &checkout.branch, &worktree.branch)?;
                    folder.store.add_part(&checkout)?;
                    Ok(checkout.part)
                })
                .collect::<Result<Vec<Part>, String>>()?;
            (deployment.id, parts)
        };
        let note = NewEntry { kind: EntryKind::Note, mission: Some(mission), to: None, text: split_note(mission, &parts), answers: None };
        self.post_entry(&deployment_id, COMMANDER, note)?;
        Ok(Reply::Parts { parts })
    }

    /// Merges a done part into its mission. A clash goes back to whoever
    /// did the part; once every part is in, the commander hears.
    pub fn finish_part(&self, deployment_key: &str, reporter: &str, mission: u32, number: u32, note: &str) -> Result<Reply, String> {
        let (deployment_id, outcome, is_last, mission_branch) = {
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let folder = &state.folders[folder_index];
            let worktree = folder.store.worktree(mission)?.ok_or_else(|| format!("mission {mission} has no checkout"))?;
            let parts = folder.store.parts(mission)?;
            let checkout = parts.iter().find(|checkout| checkout.part.number == number).ok_or_else(|| format!("mission {mission} has no part {number}"))?;
            if checkout.part.is_merged {
                return Err(format!("part {number} of mission {mission} is already merged"));
            }
            let outcome = merge_branch(Path::new(&worktree.path), &checkout.branch)?;
            if outcome == MergeOutcome::Merged {
                folder.store.mark_part_merged(mission, number)?;
                remove_worktree(&folder.path, Path::new(&checkout.path), &checkout.branch)?;
                // Its checkout is gone, so its session has nothing left to work in.
                state
                    .sessions
                    .values_mut()
                    .filter(|session| session.deployment == deployment.id && session.mission == Some(mission) && session.part == Some(number))
                    .try_for_each(|session| {
                        session.is_stopping = true;
                        session.end()
                    })?;
            }
            let is_last = parts.iter().all(|other| other.part.is_merged || other.part.number == number);
            (deployment.id, outcome, is_last, worktree.branch)
        };
        match outcome {
            MergeOutcome::Merged => {
                let merged = NewEntry { kind: EntryKind::Note, mission: Some(mission), to: None, text: format!("part {number} merged into mission {mission}: {note}"), answers: None };
                let entry = self.post_entry(&deployment_id, reporter, merged)?;
                if is_last {
                    let all_in = NewEntry { kind: EntryKind::Message, mission: Some(mission), to: Some(COMMANDER.into()), text: all_merged_message(mission), answers: None };
                    self.post_entry(&deployment_id, LEGION, all_in)?;
                }
                Ok(Reply::Entry { entry })
            }
            MergeOutcome::Clash { files } => {
                let text = part_clash_message(mission, number, &mission_branch, &files);
                let back = NewEntry { kind: EntryKind::Message, mission: Some(mission), to: Some(reporter.into()), text, answers: None };
                Ok(Reply::Entry { entry: self.post_entry(&deployment_id, LEGION, back)? })
            }
        }
    }

    /// A mission's parts, for reading it.
    pub fn mission_parts(&self, deployment_key: &str, mission: u32) -> Result<Vec<Part>, String> {
        let state = self.state.lock().unwrap();
        let (folder_index, _) = state.find_deployment(deployment_key)?;
        Ok(state.folders[folder_index].store.parts(mission)?.into_iter().map(|checkout| checkout.part).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_branches_beside_its_mission() {
        assert_eq!(part_branch("mission/0012-print-page", 2), "mission/0012-print-page--part-2");
        assert_eq!(part_checkout_path(Path::new("/wt"), "0012-print-page", 2), PathBuf::from("/wt/0012-print-page--part-2"));
    }

    #[test]
    fn an_operator_on_a_part_does_only_that_and_reports_with_part_done() {
        let text = part_instructions(12, &Part { mission: 12, number: 2, brief: "the tag counts".into(), is_merged: false });
        assert!(text.starts_with("You're on part 2 of mission 12: the tag counts"));
        assert!(text.contains("part_done tool (mission 12, part 2)"));
    }

    #[test]
    fn the_split_and_a_clash_say_what_happened() {
        let parts = [Part { mission: 9, number: 1, brief: "table".into(), is_merged: false }, Part { mission: 9, number: 2, brief: "styles".into(), is_merged: false }];
        assert_eq!(split_note(9, &parts), "mission 9 is split into 2 parts: 1) table; 2) styles");
        assert!(part_clash_message(9, 2, "mission/0009", &["style.css".into()]).contains("in style.css. In your folder, run `git merge mission/0009`"));
    }
}
