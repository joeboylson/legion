//! The arguments a session's `claude` starts with.

use std::path::Path;

use legion2_proto::{ENV_POSITION, ENV_DEPLOYMENT, ENV_SOCKET, NAME};
use serde_json::json;

use crate::{
    constants::{legion_tools_rule, LEGION_TOOLS_SUBCOMMAND, SESSION_SETTINGS_JSON},
    setup::PermissionMode,
};

/// What a session is told and allowed, before Legion adds its own tools.
pub struct LaunchPlan {
    pub position: String,
    pub system_prompt: String,
    pub model: Option<String>,
    pub permission_mode: Option<PermissionMode>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub first_prompt: String,
    /// The percentage its conversation hands over at; None for never.
    pub clear_at: Option<u8>,
}

/// The MCP config that gives a session Legion's tools, speaking for its deployment
/// and position.
pub fn legion_tools_config(binary_folder: &Path, socket_path: &Path, deployment_id: &str, position: &str) -> String {
    let command = binary_folder.join(NAME).to_string_lossy().into_owned();
    let environment = json!({
        ENV_DEPLOYMENT: deployment_id,
        ENV_POSITION: position,
        ENV_SOCKET: socket_path.to_string_lossy(),
    });
    let server = json!({ "command": command, "args": [LEGION_TOOLS_SUBCOMMAND], "env": environment });
    json!({ "mcpServers": { NAME: server } }).to_string()
}

/// `system_prompt_file` holds the plan's system prompt. Passed as a file, the
/// prompt stays out of the session's command line, where a `pkill -f` run by
/// any session could match its words and end it.
pub fn session_arguments(plan: &LaunchPlan, tools_config: &str, system_prompt_file: &Path) -> Vec<String> {
    let settings = ["--settings".to_string(), SESSION_SETTINGS_JSON.to_string()];
    let tools = ["--mcp-config".to_string(), tools_config.to_string()];
    let system_prompt = ["--append-system-prompt-file".to_string(), system_prompt_file.to_string_lossy().into_owned()];
    let allowed = std::iter::once("--allowedTools".to_string())
        .chain(std::iter::once(legion_tools_rule()))
        .chain(plan.allowed_tools.iter().cloned());
    let disallowed: Vec<String> = match plan.disallowed_tools.as_slice() {
        [] => Vec::new(),
        rules => std::iter::once("--disallowedTools".to_string()).chain(rules.iter().cloned()).collect(),
    };
    let model: Vec<String> = plan.model.iter().flat_map(|model| ["--model".to_string(), model.clone()]).collect();
    let permission_mode: Vec<String> =
        plan.permission_mode.iter().flat_map(|mode| ["--permission-mode".to_string(), mode.flag_value().to_string()]).collect();
    // A list flag would take the first prompt as one more entry without `--`.
    let first_prompt = ["--".to_string(), plan.first_prompt.clone()];
    settings
        .into_iter()
        .chain(tools)
        .chain(system_prompt)
        .chain(allowed)
        .chain(disallowed)
        .chain(model)
        .chain(permission_mode)
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
            permission_mode: None,
            allowed_tools: allowed.iter().map(|rule| rule.to_string()).collect(),
            disallowed_tools: disallowed.iter().map(|rule| rule.to_string()).collect(),
            first_prompt: "go".into(),
            clear_at: None,
        }
    }

    #[test]
    fn always_allows_legions_tools_and_ends_with_the_prompt() {
        let arguments = session_arguments(&plan(&[], &[], None), "{}", Path::new("prompt.md"));
        assert!(arguments.contains(&legion_tools_rule()));
        assert_eq!(arguments[arguments.len() - 2..], ["--".to_string(), "go".to_string()]);
        assert!(!arguments.contains(&"--disallowedTools".to_string()));
        assert!(!arguments.contains(&"--model".to_string()));
    }

    #[test]
    fn the_system_prompt_goes_by_file_not_on_the_command_line() {
        let arguments = session_arguments(&plan(&[], &[], None), "{}", Path::new("/prompts/builder.md"));
        let flag_at = arguments.iter().position(|argument| argument == "--append-system-prompt-file").unwrap();
        assert_eq!(arguments[flag_at + 1], "/prompts/builder.md");
        assert!(!arguments.contains(&"be good".to_string()));
    }

    #[test]
    fn operator_rules_and_model_are_passed_on() {
        let arguments = session_arguments(&plan(&["Edit(src/**)"], &["Bash(rm:*)"], Some("haiku")), "{}", Path::new("prompt.md"));
        let position_of = |value: &str| arguments.iter().position(|argument| argument == value).unwrap();
        assert!(position_of("Edit(src/**)") > position_of("--allowedTools"));
        assert_eq!(position_of("Bash(rm:*)"), position_of("--disallowedTools") + 1);
        assert_eq!(arguments[position_of("--model") + 1], "haiku");
    }

    #[test]
    fn a_permission_mode_is_passed_on() {
        let with_mode = LaunchPlan { permission_mode: Some(PermissionMode::Auto), ..plan(&[], &[], None) };
        let arguments = session_arguments(&with_mode, "{}", Path::new("prompt.md"));
        let flag_at = arguments.iter().position(|argument| argument == "--permission-mode").unwrap();
        assert_eq!(arguments[flag_at + 1], "auto");
        assert!(!session_arguments(&plan(&[], &[], None), "{}", Path::new("prompt.md")).contains(&"--permission-mode".to_string()));
    }

    #[test]
    fn skills_from_claude_ai_stay_out_and_tools_come_in() {
        let arguments = session_arguments(&plan(&[], &[], None), "CONFIG", Path::new("prompt.md"));
        assert_eq!(arguments[0..4], ["--settings".to_string(), SESSION_SETTINGS_JSON.to_string(), "--mcp-config".to_string(), "CONFIG".to_string()]);
    }

    #[test]
    fn the_tools_speak_for_the_sessions_deployment_and_position() {
        let config: Value = serde_json::from_str(&legion_tools_config(Path::new("/bin"), Path::new("/data/legion2d.sock"), "deployment-1", "builder-2")).unwrap();
        let server = &config["mcpServers"][NAME];
        assert_eq!(server["command"], format!("/bin/{NAME}"));
        assert_eq!(server["args"][0], LEGION_TOOLS_SUBCOMMAND);
        assert_eq!(server["env"][ENV_DEPLOYMENT], "deployment-1");
        assert_eq!(server["env"][ENV_POSITION], "builder-2");
        assert_eq!(server["env"][ENV_SOCKET], "/data/legion2d.sock");
    }
}
