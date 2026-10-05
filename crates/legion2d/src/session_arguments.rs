//! The arguments a session's `claude` starts with.

use crate::constants::{legion_command_tool_rule, SESSION_SETTINGS_JSON};

pub struct SessionArgumentsRequest<'a> {
    pub system_prompt: &'a str,
    pub model: Option<&'a str>,
    pub allowed_tools: &'a [String],
    pub disallowed_tools: &'a [String],
    pub first_prompt: &'a str,
}

pub fn session_arguments(request: SessionArgumentsRequest) -> Vec<String> {
    let settings = ["--settings".to_string(), SESSION_SETTINGS_JSON.to_string()];
    let system_prompt = ["--append-system-prompt".to_string(), request.system_prompt.to_string()];
    let allowed = std::iter::once("--allowedTools".to_string())
        .chain(std::iter::once(legion_command_tool_rule()))
        .chain(request.allowed_tools.iter().cloned());
    let disallowed: Vec<String> = match request.disallowed_tools {
        [] => Vec::new(),
        rules => std::iter::once("--disallowedTools".to_string()).chain(rules.iter().cloned()).collect(),
    };
    let model: Vec<String> = request.model.map(|model| vec!["--model".to_string(), model.to_string()]).unwrap_or_default();
    // A tool list flag would take the first prompt as one more rule without `--`.
    let first_prompt = ["--".to_string(), request.first_prompt.to_string()];
    settings
        .into_iter()
        .chain(system_prompt)
        .chain(allowed)
        .chain(disallowed)
        .chain(model)
        .chain(first_prompt)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(allowed: &'a [String], disallowed: &'a [String], model: Option<&'a str>) -> SessionArgumentsRequest<'a> {
        SessionArgumentsRequest { system_prompt: "be good", model, allowed_tools: allowed, disallowed_tools: disallowed, first_prompt: "go" }
    }

    #[test]
    fn always_allows_the_legion_command_and_ends_with_the_prompt() {
        let arguments = session_arguments(request(&[], &[], None));
        assert!(arguments.contains(&legion_command_tool_rule()));
        assert_eq!(arguments[arguments.len() - 2..], ["--".to_string(), "go".to_string()]);
        assert!(!arguments.contains(&"--disallowedTools".to_string()));
        assert!(!arguments.contains(&"--model".to_string()));
    }

    #[test]
    fn operator_rules_and_model_are_passed_on() {
        let allowed = vec!["Edit(src/**)".to_string()];
        let disallowed = vec!["Bash(rm:*)".to_string()];
        let arguments = session_arguments(request(&allowed, &disallowed, Some("haiku")));
        let position_of = |value: &str| arguments.iter().position(|argument| argument == value).unwrap();
        assert!(position_of("Edit(src/**)") > position_of("--allowedTools"));
        assert_eq!(position_of("Bash(rm:*)"), position_of("--disallowedTools") + 1);
        assert_eq!(arguments[position_of("--model") + 1], "haiku");
    }

    #[test]
    fn skills_from_claude_ai_stay_out() {
        let arguments = session_arguments(request(&[], &[], None));
        assert_eq!(arguments[0..2], ["--settings".to_string(), SESSION_SETTINGS_JSON.to_string()]);
    }
}
