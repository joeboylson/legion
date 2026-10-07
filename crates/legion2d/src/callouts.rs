//! Callouts: one-line heads-ups anyone calls out for everyone, like a
//! kitchen calling "behind". Kept in the folder's `.<NAME>/callouts.md`,
//! committed there, told at once to everyone running, and listed in every
//! session's instructions on later runs.

use std::{fs, path::Path, path::PathBuf};

use legion2_proto::{EntryKind, NewEntry, Reply};

use crate::{
    daemon::Daemon,
    git::{commit_paths, is_git_repo},
    setup::setup_folder,
    store::now_ms,
};

pub const CALLOUTS_FILE_NAME: &str = "callouts.md";
/// Long enough for a gotcha and its fix; a callout isn't a document.
pub const CALLOUT_MAX_LENGTH: usize = 200;
/// The newest this many go in each session's instructions.
pub const CALLOUTS_SHOWN: usize = 20;
const CALLOUT_LINE_PREFIX: &str = "- ";
const CALLOUTS_FILE_HEADER: &str = "# Callouts\n\nOne-line heads-ups the team called out for everyone, oldest first. Legion adds to this file.\n\n";

pub fn callouts_path(folder: &Path) -> PathBuf {
    setup_folder(folder).join(CALLOUTS_FILE_NAME)
}

/// A callout as given, trimmed, or why it isn't one.
pub fn checked_callout(text: &str) -> Result<String, String> {
    let callout = text.trim();
    if callout.is_empty() {
        return Err("a callout needs something to say".into());
    }
    if callout.contains('\n') {
        return Err("a callout is one line; put anything longer in a tool or a doc".into());
    }
    if callout.chars().count() > CALLOUT_MAX_LENGTH {
        return Err(format!("a callout is at most {CALLOUT_MAX_LENGTH} characters; say it shorter"));
    }
    Ok(callout.to_string())
}

/// `- 2026-10-07 tester: the dev server takes 20s to start`.
pub fn callout_line(date: &str, author: &str, callout: &str) -> String {
    format!("{CALLOUT_LINE_PREFIX}{date} {author}: {callout}")
}

/// The file's callouts, newest last.
pub fn callouts_in(text: &str) -> Vec<String> {
    text.lines().filter_map(|line| line.strip_prefix(CALLOUT_LINE_PREFIX)).map(str::to_string).collect()
}

pub fn read_callouts(folder: &Path) -> Vec<String> {
    fs::read_to_string(callouts_path(folder)).map(|text| callouts_in(&text)).unwrap_or_default()
}

/// What every session is told about callouts: the newest ones, and when to make one.
pub fn callouts_section(callouts: &[String]) -> String {
    let newest = &callouts[callouts.len().saturating_sub(CALLOUTS_SHOWN)..];
    let listed = match newest {
        [] => "None yet.".to_string(),
        _ => newest.iter().map(|callout| format!("- {callout}")).collect::<Vec<_>>().join("\n"),
    };
    format!(
        "Callouts, the team's heads-ups from earlier work, newest last:
{listed}

When you learn something everyone working here should know (a gotcha, a slow or flaky command, a file not to touch, a setting that matters), call it out with the callout tool in one short line, like a kitchen calling out \"behind\". Not for status or progress: those go in handoffs and notes."
    )
}

pub fn callout_announcement(author: &str, callout: &str) -> String {
    format!("{author} calls out: {callout}")
}

/// Today's date as `YYYY-MM-DD`, from milliseconds since 1970.
pub fn date_of(milliseconds: i64) -> String {
    let days = milliseconds.div_euclid(86_400_000);
    // Howard Hinnant's days-to-civil-date algorithm.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

impl Daemon {
    pub fn list_callouts(&self, deployment_key: &str) -> Result<Reply, String> {
        let folder_path = {
            let state = self.state.lock().unwrap();
            let (folder_index, _) = state.find_deployment(deployment_key)?;
            state.folders[folder_index].path.clone()
        };
        let callouts = read_callouts(&folder_path);
        let text = if callouts.is_empty() { "No callouts yet.".to_string() } else { callouts.join("\n") };
        Ok(Reply::Text { text })
    }

    pub fn call_out(&self, deployment_key: &str, author: &str, text: &str) -> Result<Reply, String> {
        let callout = checked_callout(text)?;
        let (deployment_id, folder_path, others) = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let others: Vec<String> = state.running_positions(&deployment.id).into_iter().filter(|position| position != author).collect();
            (deployment.id, state.folders[folder_index].path.clone(), others)
        };
        let path = callouts_path(&folder_path);
        let existing = fs::read_to_string(&path).unwrap_or_else(|_| CALLOUTS_FILE_HEADER.to_string());
        let updated = format!("{existing}{}\n", callout_line(&date_of(now_ms()), author, &callout));
        fs::write(&path, updated).map_err(|error| format!("can't write {}: {error}", path.display()))?;
        if is_git_repo(&folder_path) {
            let relative = path.strip_prefix(&folder_path).unwrap_or(&path);
            commit_paths(&folder_path, &[relative], &format!("Callout from {author}: {callout}"))?;
        }
        let announcement = callout_announcement(author, &callout);
        others.into_iter().try_for_each(|position| {
            let entry = NewEntry { kind: EntryKind::Announcement, mission: None, to: Some(position), text: announcement.clone(), answers: None };
            self.post_entry(&deployment_id, author, entry).map(|_| ())
        })?;
        Ok(Reply::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_callout_is_one_short_line() {
        assert_eq!(checked_callout("  dev server takes 20s to start \n"), Ok("dev server takes 20s to start".into()));
        assert!(checked_callout("   ").is_err());
        assert!(checked_callout("one\ntwo").unwrap_err().contains("one line"));
        assert!(checked_callout(&"x".repeat(CALLOUT_MAX_LENGTH + 1)).unwrap_err().contains("at most"));
        assert!(checked_callout(&"x".repeat(CALLOUT_MAX_LENGTH)).is_ok());
    }

    #[test]
    fn callouts_are_lines_in_the_file_with_date_and_author() {
        let line = callout_line("2026-10-07", "tester", "tests.html needs a fresh tab");
        assert_eq!(line, "- 2026-10-07 tester: tests.html needs a fresh tab");
        let file = format!("{CALLOUTS_FILE_HEADER}{line}\n- 2026-10-08 builder: style.css is shared\n");
        assert_eq!(callouts_in(&file), ["2026-10-07 tester: tests.html needs a fresh tab", "2026-10-08 builder: style.css is shared"]);
    }

    #[test]
    fn the_section_shows_only_the_newest() {
        let many: Vec<String> = (1..=CALLOUTS_SHOWN + 5).map(|number| format!("callout {number}")).collect();
        let section = callouts_section(&many);
        assert!(!section.contains("- callout 5\n") && section.contains("- callout 6\n"));
        assert!(section.contains(&format!("- callout {}", CALLOUTS_SHOWN + 5)));
        assert!(callouts_section(&[]).contains("None yet."));
        assert!(callouts_section(&[]).contains("callout tool"));
    }

    #[test]
    fn dates_come_out_as_year_month_day() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_791_331_200_000), "2026-10-07");
        assert_eq!(date_of(951_782_400_000), "2000-02-29");
    }

    #[test]
    fn everyone_hears_who_called_it_out() {
        assert_eq!(callout_announcement("tester", "port 3000 is taken"), "tester calls out: port 3000 is taken");
    }
}
