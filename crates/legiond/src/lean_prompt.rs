//! The short form of an operator's instructions, for sessions on a harness
//! template. A local model reads the whole opening request before it writes a
//! token, so what it can read later, by tool or file, stays out of it.

use std::path::Path;

use legion_proto::Entry;

use crate::{
    harness::TOOL_BLANK,
    prompts::{bulleted, MissionBriefing, ToolNaming},
};

/// Longest operator definition told whole; the rest is a file to read.
const DEFINITION_LIMIT: usize = 3000;
/// Longest mission text told whole; the rest is a tool to call.
const MISSION_LIMIT: usize = 4000;
/// The newest this many mission history entries are told.
const HISTORY_SHOWN: usize = 5;
/// Longest one history entry is told; handoff notes run to thousands.
const HISTORY_ENTRY_LIMIT: usize = 600;

/// `text` cut to at most `limit` characters at a line or word end, with a mark that it was cut.
fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit).collect();
    let end = head.rfind(['\n', ' ']).unwrap_or(head.len());
    format!("{} …", head[..end].trim_end())
}

pub fn definition(text: &str, file: &Path) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= DEFINITION_LIMIT {
        return trimmed.to_string();
    }
    format!("{}\n\nThat is the start of your definition. Read the whole of it in {} before you begin.", cut(trimmed, DEFINITION_LIMIT), file.display())
}

pub fn tools_line(naming: &ToolNaming) -> String {
    match naming {
        ToolNaming::Pattern(pattern) => format!("Legion's tools, which already know your deployment and position, are in your tool list as {}.", pattern.replace(TOOL_BLANK, "<name>")),
        ToolNaming::Command => String::new(),
    }
}

fn history_line(entry: &Entry) -> String {
    format!("{}: {} {}", entry.from, entry.kind.as_str(), cut(&entry.text, HISTORY_ENTRY_LIMIT))
}

pub fn mission(briefing: &MissionBriefing, naming: &ToolNaming) -> String {
    let body = briefing.body.trim();
    let body = match body.chars().count() > MISSION_LIMIT {
        true => format!("{}\n\nThat is the start of the mission. Read all of it with {}.", cut(body, MISSION_LIMIT), naming.mention("mission_read")),
        false => body.to_string(),
    };
    let newest = &briefing.history[briefing.history.len().saturating_sub(HISTORY_SHOWN)..];
    let earlier = briefing.history.len() - newest.len();
    let heading = match earlier {
        0 => "What's happened on it so far:".to_string(),
        _ => format!("What's happened on it so far (the newest {}; the other {earlier} and the full notes are in {}):", newest.len(), naming.mention("log")),
    };
    format!("Your mission is #{}:\n{body}\n\n{heading}\n{}", briefing.number, bulleted(newest.iter().map(history_line), "Nothing yet."))
}

#[cfg(test)]
mod tests {
    use legion_proto::EntryKind;

    use super::*;

    fn entry(text: &str) -> Entry {
        Entry { id: 1, deployment: "abc".into(), at_ms: 0, mission: Some(3), from: "builder".into(), to: None, kind: EntryKind::Note, text: text.into(), answers: None }
    }

    #[test]
    fn a_short_text_is_told_whole() {
        assert_eq!(cut("one two", 20), "one two");
    }

    #[test]
    fn a_long_text_is_cut_at_a_word_and_marked() {
        assert_eq!(cut("one two three", 9), "one two …");
        assert_eq!(cut("first line\nsecond line", 15), "first line …");
    }

    #[test]
    fn a_short_definition_is_told_whole_and_a_long_one_points_to_its_file() {
        let file = Path::new("/repo/.legion/operators/shipper/definition.md");
        assert_eq!(definition("# Builder\nBuild it.\n", file), "# Builder\nBuild it.");
        let long = "Rule one.\n".repeat(DEFINITION_LIMIT);
        let told = definition(&long, file);
        assert!(told.len() < DEFINITION_LIMIT + 200);
        assert!(told.starts_with("Rule one."));
        assert!(told.ends_with("Read the whole of it in /repo/.legion/operators/shipper/definition.md before you begin."));
    }

    #[test]
    fn mission_history_keeps_the_newest_entries_cut_short() {
        let history: Vec<Entry> = (0..8).map(|number| entry(&format!("note {number} {}", "word ".repeat(400)))).collect();
        let told = mission(&MissionBriefing { number: 3, body: "Make it work", history: &history }, &ToolNaming::Pattern("legion_{tool}".into()));
        assert!(told.starts_with("Your mission is #3:\nMake it work\n\nWhat's happened on it so far (the newest 5; the other 3 and the full notes are in the log tool):"));
        assert!(!told.contains("note 2 "));
        assert!(told.contains("note 3 ") && told.contains("note 7 "));
        assert!(told.len() < 5 * (HISTORY_ENTRY_LIMIT + 60) + 400);
    }

    #[test]
    fn a_short_history_is_told_whole() {
        let told = mission(&MissionBriefing { number: 3, body: "Make it work", history: &[entry("done")] }, &ToolNaming::Command);
        assert!(told.ends_with("What's happened on it so far:\n- builder: note done"));
        assert!(mission(&MissionBriefing { number: 3, body: "x", history: &[] }, &ToolNaming::Command).ends_with("Nothing yet."));
    }

    #[test]
    fn a_long_mission_points_to_the_tool_that_reads_it() {
        let body = "word ".repeat(MISSION_LIMIT);
        let told = mission(&MissionBriefing { number: 3, body: &body, history: &[] }, &ToolNaming::Command);
        assert!(told.contains("Read all of it with the `legion mission-read` command."));
        assert!(told.len() < MISSION_LIMIT + 300);
    }

    #[test]
    fn tools_are_named_once_for_an_mcp_harness_and_listed_for_commands() {
        assert_eq!(tools_line(&ToolNaming::Pattern("legion_{tool}".into())), "Legion's tools, which already know your deployment and position, are in your tool list as legion_<name>.");
        assert_eq!(tools_line(&ToolNaming::Command), "");
    }
}
