//! Claude-style tool rules (`allowedTools`, `disallowedTools`, `permissionMode`)
//! turned into the permission list of an OpenCode agent.
//!
//! OpenCode checks a tool call as an action and a resource against the list,
//! and the last rule that matches decides. `*` matches anything, and a
//! pattern ending in ` *` also matches the bare command. A call that matches
//! no rule would wait for a person, so a restricted action starts with a deny.

use legion_proto::NAME;
use serde_json::{json, Value};

use crate::setup::PermissionMode;

const ANY: &str = "*";
/// OpenCode checks write, edit and patch as this one action.
const EDIT_ACTION: &str = "edit";
const SHELL_ACTION: &str = "shell";
/// A shell command with a redirect is checked whole, so `cat a > b` would match
/// `cat *`. Denying the redirect keeps a command allow-list read-only.
const REDIRECT_PATTERN: &str = "*>*";

/// Claude tools that are one OpenCode action, by name only.
const PLAIN_TOOLS: &[(&str, &str)] = &[
    ("Read", "read"),
    ("Glob", "glob"),
    ("Grep", "grep"),
    ("LS", "list"),
    ("WebFetch", "webfetch"),
    ("WebSearch", "websearch"),
    ("Task", "subagent"),
];
const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

struct Rule {
    action: String,
    resource: String,
}

fn effect_rule(rule: &Rule, effect: &str) -> Value {
    json!({ "action": rule.action, "resource": rule.resource, "effect": effect })
}

fn cannot_map(rule: &str, why: &str) -> String {
    format!("can't map the tool rule {rule:?} onto OpenCode: {why}")
}

/// `git log:*` is the command `git log` with anything after it.
fn shell_resource(specifier: &str) -> String {
    match specifier.strip_suffix(":*") {
        Some(prefix) => format!("{prefix} {ANY}"),
        None => specifier.to_string(),
    }
}

fn mcp_action(rule: &str, name: &str) -> Result<String, String> {
    let (server, tool) = match name.split_once("__") {
        Some((server, tool)) => (server, tool),
        None => (name, ANY),
    };
    if server.is_empty() || tool.is_empty() {
        return Err(cannot_map(rule, "an MCP rule is mcp__<server> or mcp__<server>__<tool>"));
    }
    Ok(format!("{server}_{tool}"))
}

fn parse(rule: &str) -> Result<Rule, String> {
    let (name, specifier) = match rule.split_once('(') {
        Some((name, rest)) => {
            let specifier = rest.strip_suffix(')').ok_or_else(|| cannot_map(rule, "a ( is not closed"))?;
            (name, Some(specifier))
        }
        None => (rule, None),
    };
    let bare = |action: &str| match specifier {
        None => Ok(Rule { action: action.to_string(), resource: ANY.to_string() }),
        Some(_) => Err(cannot_map(rule, "only a Bash rule can name a command or a path pattern")),
    };
    if name == "Bash" {
        let resource = specifier.map_or_else(|| ANY.to_string(), shell_resource);
        return Ok(Rule { action: SHELL_ACTION.into(), resource });
    }
    if EDIT_TOOLS.contains(&name) {
        return bare(EDIT_ACTION);
    }
    if let Some((_, action)) = PLAIN_TOOLS.iter().find(|(tool, _)| *tool == name) {
        return bare(action);
    }
    if let Some(mcp) = name.strip_prefix("mcp__") {
        return bare(&mcp_action(rule, mcp)?);
    }
    if name.starts_with(&format!("{NAME}_")) {
        return bare(name);
    }
    Err(cannot_map(rule, "no OpenCode action matches this tool"))
}

/// Edit, Write and MultiEdit are one action, so listing all of them says it once.
fn parse_all(rules: &[String]) -> Result<Vec<Rule>, String> {
    let parsed = rules.iter().map(|rule| parse(rule)).collect::<Result<Vec<_>, _>>()?;
    Ok(parsed.into_iter().fold(Vec::new(), |mut kept, rule| {
        if !kept.iter().any(|other: &Rule| other.action == rule.action && other.resource == rule.resource) {
            kept.push(rule);
        }
        kept
    }))
}

/// The rules to add after a template's own defaults, so an operator's deny wins.
pub fn opencode_permissions(allowed: &[String], disallowed: &[String], mode: Option<PermissionMode>) -> Result<Vec<Value>, String> {
    let allowed = parse_all(allowed)?;
    let disallowed = parse_all(disallowed)?;
    let is_shell = |rule: &&Rule| rule.action == SHELL_ACTION;
    let shell_allows: Vec<&Rule> = allowed.iter().filter(is_shell).collect();
    // Listing some commands means only those; listing `Bash` itself leaves them all open.
    let shell_restricted = !shell_allows.is_empty() && shell_allows.iter().all(|rule| rule.resource != ANY);
    let restriction = shell_restricted.then(|| Rule { action: SHELL_ACTION.into(), resource: ANY.into() });
    let redirect = shell_restricted.then(|| Rule { action: SHELL_ACTION.into(), resource: REDIRECT_PATTERN.into() });
    let plan_mode = (mode == Some(PermissionMode::Plan)).then(|| Rule { action: EDIT_ACTION.into(), resource: ANY.into() });
    let rules = restriction
        .iter()
        .map(|rule| effect_rule(rule, "deny"))
        .chain(allowed.iter().map(|rule| effect_rule(rule, "allow")))
        .chain(redirect.iter().chain(plan_mode.iter()).chain(disallowed.iter()).map(|rule| effect_rule(rule, "deny")))
        .collect();
    Ok(rules)
}

const PERMISSIONS_PATH: [&str; 2] = ["agents", "legion"];

/// Adds the operator's rules to the template's `agents.legion.permissions`.
/// A template with no such list can't carry them, which fails loud.
pub fn add_to_config(config: Option<String>, rules: Vec<Value>, harness: &str) -> Result<Option<String>, String> {
    if rules.is_empty() {
        return Ok(config);
    }
    let config = config.ok_or_else(|| format!("harness {harness}: this operator has tool rules, but the harness has no configFile to put them in"))?;
    let mut config: Value = serde_json::from_str(&config).map_err(|error| format!("harness {harness}: its config isn't JSON: {error}"))?;
    let agent = PERMISSIONS_PATH.iter().try_fold(&mut config, |value, key| value.get_mut(key));
    let list = agent.and_then(|agent| agent.get_mut("permissions")).and_then(Value::as_array_mut).ok_or_else(|| {
        format!("harness {harness}: this operator has tool rules, but its config has no agents.{NAME}.permissions list to put them in")
    })?;
    list.extend(rules);
    serde_json::to_string_pretty(&config).map(Some).map_err(|error| format!("harness {harness}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(rules: &[&str]) -> Vec<String> {
        rules.iter().map(|rule| rule.to_string()).collect()
    }

    fn translate(allowed: &[&str], disallowed: &[&str]) -> Vec<(String, String, String)> {
        opencode_permissions(&texts(allowed), &texts(disallowed), None)
            .unwrap()
            .iter()
            .map(|rule| (rule["action"].as_str().unwrap().into(), rule["resource"].as_str().unwrap().into(), rule["effect"].as_str().unwrap().into()))
            .collect()
    }

    /// OpenCode 2.0.25's matching: `*` is anything, a trailing ` *` also matches
    /// the bare command, and the last rule that matches decides.
    fn glob(pattern: &[u8], text: &[u8]) -> bool {
        match pattern.split_first() {
            None => text.is_empty(),
            Some((b'*', rest)) => (0..=text.len()).any(|skip| glob(rest, &text[skip..])),
            Some((first, rest)) => text.split_first().is_some_and(|(head, tail)| head == first && glob(rest, tail)),
        }
    }

    fn pattern_matches(pattern: &str, text: &str) -> bool {
        let bare = pattern.strip_suffix(" *").is_some_and(|prefix| glob(prefix.as_bytes(), text.as_bytes()));
        bare || glob(pattern.as_bytes(), text.as_bytes())
    }

    fn decide(rules: &[(String, String, String)], action: &str, resource: &str) -> String {
        let template_allows_all = ("*".to_string(), "*".to_string(), "allow".to_string());
        std::iter::once(&template_allows_all)
            .chain(rules)
            .rev()
            .find(|(rule_action, rule_resource, _)| pattern_matches(rule_action, action) && pattern_matches(rule_resource, resource))
            .map(|(_, _, effect)| effect.clone())
            .unwrap()
    }

    fn read_only() -> Vec<(String, String, String)> {
        translate(
            &["Bash(git log:*)", "Bash(git status:*)", "Bash(git rev-parse:*)", "Bash(git ls-remote:*)", "Bash(cat:*)", "Bash(ls:*)", "mcp__legion"],
            &["Edit", "Write", "MultiEdit", "NotebookEdit"],
        )
    }

    #[test]
    fn a_read_only_operator_can_run_the_listed_commands_and_use_legions_tools() {
        let rules = read_only();
        for command in ["git log", "git log -5 --oneline", "git status", "git rev-parse HEAD", "git ls-remote origin", "cat README.md", "ls -la"] {
            assert_eq!(decide(&rules, "shell", command), "allow", "{command}");
        }
        assert_eq!(decide(&rules, "legion_handoff", "*"), "allow");
    }

    #[test]
    fn a_read_only_operator_is_refused_edits_and_other_commands() {
        let rules = read_only();
        for action in ["edit"] {
            assert_eq!(decide(&rules, action, "src/main.rs"), "deny");
        }
        for command in ["rm -rf x", "git push", "git logfoo", "sh -c ls", "cat a > b", "ls >> notes"] {
            assert_eq!(decide(&rules, "shell", command), "deny", "{command}");
        }
        assert_eq!(decide(&rules, "read", "src/main.rs"), "allow");
    }

    #[test]
    fn an_operator_deny_comes_last_so_it_wins() {
        let rules = translate(&["Bash", "Edit"], &["Edit", "Bash(rm:*)"]);
        assert_eq!(decide(&rules, "edit", "a.rs"), "deny");
        assert_eq!(decide(&rules, "shell", "rm -r x"), "deny");
        assert_eq!(decide(&rules, "shell", "ls"), "allow");
    }

    #[test]
    fn listing_bash_itself_leaves_every_command_open() {
        assert!(!translate(&["Bash"], &[]).iter().any(|(action, resource, effect)| (action.as_str(), resource.as_str(), effect.as_str()) == ("shell", "*", "deny")));
    }

    #[test]
    fn mcp_rules_become_the_servers_tool_actions() {
        let rules = translate(&["mcp__legion__handoff", "mcp__legion__*", "legion_note"], &[]);
        let actions: Vec<&str> = rules.iter().map(|(action, _, _)| action.as_str()).collect();
        assert_eq!(actions, ["legion_handoff", "legion_*", "legion_note"]);
    }

    #[test]
    fn plan_mode_denies_edits_and_other_modes_add_nothing() {
        let plan = opencode_permissions(&[], &[], Some(PermissionMode::Plan)).unwrap();
        assert_eq!(plan, vec![json!({ "action": "edit", "resource": "*", "effect": "deny" })]);
        assert!(opencode_permissions(&[], &[], Some(PermissionMode::BypassPermissions)).unwrap().is_empty());
    }

    #[test]
    fn a_rule_that_cant_be_mapped_fails_loud_naming_it() {
        for rule in ["Edit(src/**)", "Frobnicate", "Bash(git log", "mcp__", "mcp__legion__"] {
            let error = opencode_permissions(&texts(&[rule]), &[], None).unwrap_err();
            assert!(error.contains(&format!("{rule:?}")), "{error}");
        }
        let denied = opencode_permissions(&[], &texts(&["WebWiggle"]), None).unwrap_err();
        assert!(denied.contains("\"WebWiggle\""), "{denied}");
    }

    #[test]
    fn rules_go_on_the_end_of_the_templates_list() {
        let config = r#"{"agents": {"legion": {"permissions": [{"action": "*", "resource": "*", "effect": "allow"}]}}}"#;
        let rules = opencode_permissions(&[], &texts(&["Edit"]), None).unwrap();
        let written: Value = serde_json::from_str(&add_to_config(Some(config.into()), rules, "opencode").unwrap().unwrap()).unwrap();
        let list = written["agents"]["legion"]["permissions"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1], json!({ "action": "edit", "resource": "*", "effect": "deny" }));
    }

    #[test]
    fn no_rules_leave_the_config_as_it_was() {
        assert_eq!(add_to_config(Some("not even json".into()), Vec::new(), "pi").unwrap().as_deref(), Some("not even json"));
        assert_eq!(add_to_config(None, Vec::new(), "pi").unwrap(), None);
    }

    #[test]
    fn a_template_with_no_config_file_refuses_rules() {
        let rules = opencode_permissions(&[], &texts(&["Edit"]), None).unwrap();
        assert!(add_to_config(None, rules, "pi").unwrap_err().contains("harness pi"));
    }

    #[test]
    fn a_template_with_no_permission_list_refuses_rules() {
        let rules = opencode_permissions(&[], &texts(&["Edit"]), None).unwrap();
        let error = add_to_config(Some(r#"{"agents": {}}"#.into()), rules, "pi").unwrap_err();
        assert!(error.contains("harness pi") && error.contains("permissions"), "{error}");
    }
}
