//! Each mission works in its own git worktree, on its own branch. Finishing
//! moves the base branch up to it: straight away if the base hasn't moved,
//! or after replaying the mission's commits and running the folder's check
//! if it has. Anything that doesn't go cleanly is left for the human.

use std::{path::Path, process::Command};

use crate::{constants::CHECK_OUTPUT_TAIL_LINES, store::Worktree};

fn run_git(folder: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(arguments)
        .output()
        .map_err(|error| format!("can't run git: {error}"))?;
    if !output.status.success() {
        return Err(format!("git {}: {}", arguments.join(" "), String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn is_git_repo(folder: &Path) -> bool {
    run_git(folder, &["rev-parse", "--is-inside-work-tree"]).is_ok_and(|answer| answer == "true")
}

fn current_branch(folder: &Path) -> Option<String> {
    run_git(folder, &["symbolic-ref", "--short", "HEAD"]).ok()
}

/// Makes a worktree for a mission, branching from whatever the folder is on.
pub fn create_worktree(folder: &Path, worktree_path: &Path, branch: &str) -> Result<Worktree, String> {
    let base = current_branch(folder)
        .ok_or_else(|| format!("{} isn't on a branch, so a mission has nothing to start from", folder.display()))?;
    let base_commit = run_git(folder, &["rev-parse", "HEAD"])?;
    let path = worktree_path.to_string_lossy().into_owned();
    run_git(folder, &["worktree", "add", "-b", branch, &path, &base_commit])?;
    Ok(Worktree { path, branch: branch.to_string(), base, base_commit, is_removed: false })
}

#[derive(Debug, PartialEq)]
pub enum FinishOutcome {
    /// The base had stayed put, so it just moved up to the mission's branch.
    Moved,
    /// The base had moved on: the commits were replayed onto it, the check
    /// (if any) passed, and the base moved up.
    Replayed { was_checked: bool },
    NoChanges,
    /// Left for the human, with why.
    NeedsHuman(String),
}

/// The last lines of a failed check's output: enough to see why.
pub fn output_tail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let first_kept_line = lines.len().saturating_sub(CHECK_OUTPUT_TAIL_LINES);
    lines[first_kept_line..].join("\n")
}

fn run_check(worktree: &Path, check_command: &str) -> Result<Option<String>, String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(check_command)
        .current_dir(worktree)
        .output()
        .map_err(|error| format!("can't run the check: {error}"))?;
    if output.status.success() {
        return Ok(None);
    }
    let combined_output = String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr);
    Ok(Some(output_tail(&combined_output)))
}

/// Moves the base branch up to the mission's branch, only if the base is
/// still where it was a moment ago.
fn move_base_up(folder: &Path, worktree: &Worktree, base_commit_now: &str) -> Result<(), String> {
    let is_base_checked_out = current_branch(folder).as_deref() == Some(worktree.base.as_str());
    if is_base_checked_out {
        // The folder has the base checked out, so its files have to move with it.
        return run_git(folder, &["merge", "--ff-only", &worktree.branch]).map(|_| ());
    }
    let branch_tip = run_git(folder, &["rev-parse", &worktree.branch])?;
    let base_ref = format!("refs/heads/{}", worktree.base);
    run_git(folder, &["update-ref", &base_ref, &branch_tip, base_commit_now]).map(|_| ())
}

fn replay_onto_moved_base(folder: &Path, worktree: &Worktree, check_command: Option<&str>, base_commit_now: &str) -> Result<FinishOutcome, String> {
    let worktree_path = Path::new(&worktree.path);
    if let Err(clash) = run_git(worktree_path, &["rebase", &worktree.base]) {
        run_git(worktree_path, &["rebase", "--abort"])?;
        return Ok(FinishOutcome::NeedsHuman(format!("its commits clash with {} when replayed onto it: {clash}", worktree.base)));
    }
    let check_failure = match check_command {
        Some(command) => run_check(worktree_path, command)?.map(|tail| (command, tail)),
        None => None,
    };
    if let Some((command, tail)) = check_failure {
        return Ok(FinishOutcome::NeedsHuman(format!("the check (`{command}`) failed after replaying it onto {}:\n{tail}", worktree.base)));
    }
    move_base_up(folder, worktree, base_commit_now)?;
    Ok(FinishOutcome::Replayed { was_checked: check_command.is_some() })
}

pub fn finish_mission_branch(folder: &Path, worktree: &Worktree, check_command: Option<&str>) -> Result<FinishOutcome, String> {
    let worktree_path = Path::new(&worktree.path);
    let has_uncommitted_changes = !run_git(worktree_path, &["status", "--porcelain"])?.is_empty();
    if has_uncommitted_changes {
        return Ok(FinishOutcome::NeedsHuman(format!("its worktree has changes that aren't committed ({})", worktree.path)));
    }
    let commit_range = format!("{}..{}", worktree.base_commit, worktree.branch);
    let commits_ahead: u32 = run_git(folder, &["rev-list", "--count", &commit_range])?.parse().map_err(|_| "git gave a count that isn't a number")?;
    let base_commit_now = run_git(folder, &["rev-parse", &worktree.base])?;
    let has_base_moved = base_commit_now != worktree.base_commit;
    let outcome = match (commits_ahead, has_base_moved) {
        (0, _) => FinishOutcome::NoChanges,
        (_, false) => {
            move_base_up(folder, worktree, &base_commit_now)?;
            FinishOutcome::Moved
        }
        (_, true) => replay_onto_moved_base(folder, worktree, check_command, &base_commit_now)?,
    };
    if matches!(outcome, FinishOutcome::NeedsHuman(_)) {
        return Ok(outcome);
    }
    run_git(folder, &["worktree", "remove", &worktree.path])?;
    run_git(folder, &["branch", "-D", &worktree.branch])?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    fn temporary_repo(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("legion2-git-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let _ = fs::remove_dir_all(folder.with_extension("wt"));
        fs::create_dir_all(&folder).unwrap();
        for arguments in [&["init", "-q", "-b", "main"][..], &["config", "user.email", "t@t"], &["config", "user.name", "t"]] {
            run_git(&folder, arguments).unwrap();
        }
        commit_file(&folder, "readme", "hi");
        folder
    }

    fn commit_file(folder: &Path, name: &str, text: &str) {
        fs::write(folder.join(name), text).unwrap();
        run_git(folder, &["add", name]).unwrap();
        run_git(folder, &["commit", "-q", "-m", name]).unwrap();
    }

    fn new_worktree(folder: &Path) -> Worktree {
        create_worktree(folder, &folder.with_extension("wt"), "mission/test").unwrap()
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        let output: String = (1..=20).map(|n| format!("{n}\n")).collect();
        assert_eq!(output_tail(&output).lines().next(), Some("6"));
        assert_eq!(output_tail("one\ntwo"), "one\ntwo");
    }

    #[test]
    fn an_unmoved_base_just_moves_up() {
        let repo = temporary_repo("moved");
        let worktree = new_worktree(&repo);
        commit_file(Path::new(&worktree.path), "work", "done");
        assert_eq!(finish_mission_branch(&repo, &worktree, None), Ok(FinishOutcome::Moved));
        assert!(repo.join("work").exists());
    }

    #[test]
    fn a_moved_base_gets_a_replay_and_the_check() {
        let repo = temporary_repo("replayed");
        let worktree = new_worktree(&repo);
        commit_file(Path::new(&worktree.path), "work", "done");
        commit_file(&repo, "other", "meanwhile");
        assert_eq!(finish_mission_branch(&repo, &worktree, Some("true")), Ok(FinishOutcome::Replayed { was_checked: true }));
        assert!(repo.join("work").exists() && repo.join("other").exists());
    }

    #[test]
    fn a_failing_check_waits_for_the_human() {
        let repo = temporary_repo("failing");
        let worktree = new_worktree(&repo);
        commit_file(Path::new(&worktree.path), "work", "done");
        commit_file(&repo, "other", "meanwhile");
        let outcome = finish_mission_branch(&repo, &worktree, Some("echo nope; false")).unwrap();
        assert!(matches!(outcome, FinishOutcome::NeedsHuman(why) if why.contains("nope")));
        assert!(Path::new(&worktree.path).exists());
    }

    #[test]
    fn a_clash_waits_for_the_human() {
        let repo = temporary_repo("clash");
        let worktree = new_worktree(&repo);
        commit_file(Path::new(&worktree.path), "readme", "mine");
        commit_file(&repo, "readme", "theirs");
        let outcome = finish_mission_branch(&repo, &worktree, None).unwrap();
        assert!(matches!(outcome, FinishOutcome::NeedsHuman(why) if why.contains("clash")));
    }

    #[test]
    fn no_commits_means_nothing_to_move() {
        let repo = temporary_repo("empty");
        let worktree = new_worktree(&repo);
        assert_eq!(finish_mission_branch(&repo, &worktree, None), Ok(FinishOutcome::NoChanges));
    }
}
