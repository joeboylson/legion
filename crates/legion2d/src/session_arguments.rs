//! The arguments a session's `claude` starts with.

use std::path::Path;

use legion2_proto::{ENV_POSITION, ENV_RUN, ENV_SOCKET, NAME};
use serde_json::json;

use crate::constants::{legion_tools_rule, LEGION_TOOLS_SUBCOMMAND, SESSION_SETTINGS_JSON};

/// What a session is told and allowed, before Legion adds its own tools.
pub struct LaunchPlan {
    pub position: String,
    pub system_prompt: String,
    pub model: Option<String>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub first_prompt: String,
}

/// The MCP config that gives a session Legion's tools, speaking for its run
/// and position.
pub fn legion_tools_config(binary_folder: &Path, socket_path: &Path, run_id: &str, position: &str) -> String {
    let command = binary_folder.join(NAME).to_string_lossy().into_owned();
    let environment = json!({
        ENV_RUN: run_id,
        ENV_POSITION: position,
        ENV_SOCKET: socket_path.to_string_lossy(),
    });
    let server = json!({ "command": command, "args": [LEGION_TOOLS_SUBCOMMAND], "env": environment });
    json!({ "mcpServers": { NAME: server } }).to_string()
}

pub fn session_arguments(plan: &LaunchPlan, tools_config: &str) -> Vec<String> {
    let settings = ["--settings".to_string(), SESSION_SETTINGS_JSON.to_string()];
    let tools = ["--mcp-config".to_string(), tools_config.to_string()];
    let system_prompt = ["--append-system-prompt".to_string(), plan.system_prompt.clone()];
    let allowed = std::iter::once("--allowedTools".to_string())
        .chain(std::iter::once(legion_tools_rule()))
        .chain(plan.allowed_tools.iter().cloned());
    let disallowed: Vec<String> = match plan.disallowed_tools.as_slice() {
        [] => Vec::new(),
        rules => std::iter::once("--disallowedTools".to_string()).chain(rules.iter().cloned()).collect(),
    };
    let model: Vec<String> = plan.model.iter().flat_map(|model| ["--model".to_string(), model.clone()]).collect();
    // A list flag would take the first prompt as one more entry without `--`.
    let first_prompt = ["--".to_string(), plan.first_prompt.clone()];
    settings
        .into_iter()
        .chain(tools)
        .chain(system_prompt)
        .chain(allowed)
        .chain(disallowed)
        .chain(model)
        .chain(first_prompt)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn plan(allowed: &[&str], disallowed: &[&str], model: Option<&str>) -> LaunchPlan {
        LaunchPlan {
            position: "builder".into(),
            system_prompt: "be good".into(),
            model: model.map(String::from),
            allowed_tools: allowed.iter().map(|rule| rule.to_string()).collect(),
            disallowed_tools: disallowed.iter().map(|rule| rule.to_string()).collect(),
            first_prompt: "go".into(),
        }
    }

    #[test]
    fn always_allows_legions_tools_and_ends_with_the_prompt() {
        let arguments = session_arguments(&plan(&[], &[], None), "{}");
        assert!(arguments.contains(&legion_tools_rule()));
        assert_eq!(arguments[arguments.len() - 2..], ["--".to_string(), "go".to_string()]);
        assert!(!arguments.contains(&"--disallowedTools".to_string()));
        assert!(!arguments.contains(&"--model".to_string()));
    }

    #[test]
    fn operator_rules_and_model_are_passed_on() {
        let arguments = session_arguments(&plan(&["Edit(src/**)"], &["Bash(rm:*)"], Some("haiku")), "{}");
        let position_of = |value: &str| arguments.iter().position(|argument| argument == value).unwrap();
        assert!(position_of("Edit(src/**)") > position_of("--allowedTools"));
        assert_eq!(position_of("Bash(rm:*)"), position_of("--disallowedTools") + 1);
        assert_eq!(arguments[position_of("--model") + 1], "haiku");
    }

    #[test]
    fn skills_from_claude_ai_stay_out_and_tools_come_in() {
        let arguments = session_arguments(&plan(&[], &[], None), "CONFIG");
        assert_eq!(arguments[0..4], ["--settings".to_string(), SESSION_SETTINGS_JSON.to_string(), "--mcp-config".to_string(), "CONFIG".to_string()]);
    }

    #[test]
    fn the_tools_speak_for_the_sessions_run_and_position() {
        let config: Value = serde_json::from_str(&legion_tools_config(Path::new("/bin"), Path::new("/data/legion2d.sock"), "run-1", "builder-2")).unwrap();
        let server = &config["mcpServers"][NAME];
        assert_eq!(server["command"], format!("/bin/{NAME}"));
        assert_eq!(server["args"][0], LEGION_TOOLS_SUBCOMMAND);
        assert_eq!(server["env"][ENV_RUN], "run-1");
        assert_eq!(server["env"][ENV_POSITION], "builder-2");
        assert_eq!(server["env"][ENV_SOCKET], "/data/legion2d.sock");
    }
}
