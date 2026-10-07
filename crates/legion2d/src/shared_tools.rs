//! The team's toolbox: tools the operators make for each other, in the
//! folder's own `.<NAME>/tools/`, such as a browser test harness. A tool is
//! shared mid-mission (see tool_sharing.rs), so everyone has it at once.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::setup::setup_folder;

pub const TOOLS_FOLDER_NAME: &str = "tools";
/// How a tool's first line starts when it says what the tool does.
const COMMENT_MARKERS: &[&str] = &["#!", "#", "//", "--"];
const NO_SUMMARY: &str = "(says nothing about itself; read it)";

#[derive(Debug, PartialEq, Eq)]
pub struct SharedTool {
    pub name: String,
    pub summary: String,
}

/// What a tool says it does: its first comment line after any `#!` line.
pub fn tool_summary(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("#!"))
        .find_map(|line| {
            let trimmed = line.trim();
            COMMENT_MARKERS.iter().find_map(|marker| trimmed.strip_prefix(marker)).map(|rest| rest.trim().to_string())
        })
        .filter(|summary| !summary.is_empty())
        .unwrap_or_else(|| NO_SUMMARY.to_string())
}

/// Where a folder's shared tools live: in the folder itself, never in a
/// mission's worktree, so every mission sees a tool the moment it's shared.
pub fn shared_tools_folder(folder: &Path) -> PathBuf {
    setup_folder(folder).join(TOOLS_FOLDER_NAME)
}

/// The tools in a folder, by name. A folder with none has an empty list.
pub fn shared_tools(folder: &Path) -> Vec<SharedTool> {
    let Ok(entries) = fs::read_dir(shared_tools_folder(folder)) else { return Vec::new() };
    let mut tools: Vec<SharedTool> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .map(|name| {
            let text = fs::read_to_string(shared_tools_folder(folder).join(&name)).unwrap_or_default();
            SharedTool { summary: tool_summary(&text), name }
        })
        .collect();
    tools.sort_by(|first, second| first.name.cmp(&second.name));
    tools
}

/// What an operator is told about the team's toolbox. `tools_folder` is the
/// full path, since operators work in their mission's own worktree.
pub fn shared_tools_section(tools: &[SharedTool], tools_folder: &Path) -> String {
    let folder = tools_folder.display();
    let listed = match tools {
        [] => "None yet.".to_string(),
        _ => tools.iter().map(|tool| format!("- {folder}/{}: {}", tool.name, tool.summary)).collect::<Vec<_>>().join("\n"),
    };
    format!(
        "The team's toolbox, in {folder}:
{listed}

Use these before doing anything by hand. Building the toolbox is part of your job: the second time you do something by hand (a browser test, a check, a setup step, a long command), or when the next operator will need it too, stop and make it a tool. Start it with a comment line saying what it does and how to run it, then share it with the share_tool tool right away; Legion tells everyone running. Improve a tool rather than copying it, and share it again after."
    )
}

#[cfg(test)]
mod tests {
    use legion2_proto::NAME;

    use super::*;

    #[test]
    fn a_tool_says_what_it_does_in_its_first_comment() {
        assert_eq!(tool_summary("#!/bin/sh\n# Runs tests.html in headless Chrome: ./browser-test.sh\nset -e\n"), "Runs tests.html in headless Chrome: ./browser-test.sh");
        assert_eq!(tool_summary("// Checks saved data. node check.js <file>\n"), "Checks saved data. node check.js <file>");
        assert_eq!(tool_summary("echo hi\n"), NO_SUMMARY);
    }

    #[test]
    fn the_section_lists_each_tool_or_says_there_are_none() {
        let tools = [SharedTool { name: "browser-test.sh".into(), summary: "Runs tests.html.".into() }];
        let folder = Path::new("/repo/.legion2/tools");
        assert!(shared_tools_section(&tools, folder).contains("- /repo/.legion2/tools/browser-test.sh: Runs tests.html."));
        assert!(shared_tools_section(&[], folder).contains("None yet."));
        assert!(shared_tools_section(&[], folder).contains("share_tool"));
    }

    #[test]
    fn tools_are_read_from_the_folder_by_name() {
        let folder = std::env::temp_dir().join(format!("{NAME}-shared-tools-{}", std::process::id()));
        let tools_folder = setup_folder(&folder).join(TOOLS_FOLDER_NAME);
        fs::create_dir_all(&tools_folder).unwrap();
        fs::write(tools_folder.join("b.sh"), "# Second.\n").unwrap();
        fs::write(tools_folder.join("a.js"), "// First.\n").unwrap();
        fs::write(tools_folder.join(".hidden"), "# Skipped.\n").unwrap();
        let names: Vec<String> = shared_tools(&folder).into_iter().map(|tool| tool.name).collect();
        fs::remove_dir_all(&folder).unwrap();
        assert_eq!(names, ["a.js", "b.sh"]);
    }

    #[test]
    fn a_folder_without_tools_has_none() {
        assert!(shared_tools(Path::new("/no/such/folder")).is_empty());
    }
}
