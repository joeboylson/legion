//! Refusing a Claude Code too old for the add-on.

use std::process::Command;

use crate::constants::{ClaudeVersion, MINIMUM_CLAUDE_VERSION};

/// Reads "2.1.288 (Claude Code)", the output of `claude --version`.
pub fn parse_claude_version(version_output: &str) -> Option<ClaudeVersion> {
    let version_word = version_output.split_whitespace().next()?;
    let numbers: Vec<u32> = version_word.split('.').map(str::parse).collect::<Result<_, _>>().ok()?;
    match numbers.as_slice() {
        [major, minor, patch] => Some(ClaudeVersion { major: *major, minor: *minor, patch: *patch }),
        _ => None,
    }
}

pub fn version_label(version: ClaudeVersion) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

/// Runs `<claude> --version` and refuses one older than the add-on needs.
pub fn check_claude_supported(claude_command: &str) -> Result<ClaudeVersion, String> {
    let output = Command::new(claude_command)
        .arg("--version")
        .output()
        .map_err(|error| format!("can't run {claude_command}: {error}"))?;
    let version_output = String::from_utf8_lossy(&output.stdout);
    let version = parse_claude_version(&version_output)
        .ok_or_else(|| format!("can't read the Claude Code version from {claude_command} --version: {version_output:?}"))?;
    if version < MINIMUM_CLAUDE_VERSION {
        return Err(format!(
            "Claude Code {} is too old: Legion needs {} or later for its add-on",
            version_label(version),
            version_label(MINIMUM_CLAUDE_VERSION)
        ));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_word() {
        assert_eq!(parse_claude_version("2.1.288 (Claude Code)"), Some(ClaudeVersion { major: 2, minor: 1, patch: 288 }));
    }

    #[test]
    fn refuses_anything_else() {
        assert_eq!(parse_claude_version(""), None);
        assert_eq!(parse_claude_version("2.1 (Claude Code)"), None);
        assert_eq!(parse_claude_version("v2.1.288"), None);
        assert_eq!(parse_claude_version("2.1.288.1"), None);
    }

    #[test]
    fn versions_compare_by_number_not_text() {
        let older = parse_claude_version("2.1.99").unwrap();
        assert!(older < MINIMUM_CLAUDE_VERSION);
        assert!(parse_claude_version("2.10.0").unwrap() > MINIMUM_CLAUDE_VERSION);
    }

    #[test]
    fn labels_read_like_the_original() {
        assert_eq!(version_label(MINIMUM_CLAUDE_VERSION), "2.1.287");
    }
}
