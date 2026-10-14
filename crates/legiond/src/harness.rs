//! Harness templates: `.<NAME>/harnesses/<name>.json`, one per program that
//! can run a session. Legion fills in a template's `{blanks}` to get the
//! command to run.

// Nothing starts a session from a template yet; draft 5 does.
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use legion_proto::NAME;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::setup::setup_folder;

pub const HARNESSES_FOLDER_NAME: &str = "harnesses";
const HARNESS_FILE_EXTENSION: &str = ".json";
/// Claude Code is started by Legion itself, so it can't have a template.
const CLAUDE_CODE_HARNESS: &str = "claude-code";
/// The blank in `toolName` that stands for one of Legion's tool names.
const TOOL_BLANK: &str = "{tool}";

/// How a harness reaches Legion's tools: the MCP tool server, or the
/// `legion` command with the three settings in its environment.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolRoute {
    Mcp,
    Commands,
}

/// One `args` item. A group is left out whole when any blank in it has no
/// value, so a flag never goes in without its value.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ArgumentItem {
    One(String),
    Group(Vec<String>),
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessTemplate {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    pub name: String,
    pub command: String,
    pub args: Vec<ArgumentItem>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub tools: ToolRoute,
    /// Written to a file for each session; its path fills `{configFile}`.
    #[serde(default)]
    pub config_file: Option<Value>,
    /// How the harness shows Legion's tools to the model, e.g. `legion_{tool}`.
    #[serde(default)]
    pub tool_name: Option<String>,
}

/// The values for a template's blanks. None means the blank has no value.
#[derive(Default, Debug, Clone)]
pub struct Blanks {
    pub first_prompt: Option<String>,
    pub system_prompt_file: Option<String>,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub address: Option<String>,
    pub tools_config: Option<String>,
    pub tools_config_file: Option<String>,
    pub config_file: Option<String>,
    pub permission_mode: Option<String>,
    pub legion_command: Option<String>,
    pub deployment: Option<String>,
    pub position: Option<String>,
    pub socket: Option<String>,
    pub working_folder: Option<String>,
}

impl Blanks {
    /// A blank's value; the outer None means there's no such blank. This is
    /// the one list of known blanks.
    fn value(&self, blank: &str) -> Option<Option<&str>> {
        let field = match blank {
            "firstPrompt" => &self.first_prompt,
            "systemPromptFile" => &self.system_prompt_file,
            "model" => &self.model,
            "modelProvider" => &self.model_provider,
            "address" => &self.address,
            "toolsConfig" => &self.tools_config,
            "toolsConfigFile" => &self.tools_config_file,
            "configFile" => &self.config_file,
            "permissionMode" => &self.permission_mode,
            "legionCommand" => &self.legion_command,
            "deployment" => &self.deployment,
            "position" => &self.position,
            "socket" => &self.socket,
            "workingFolder" => &self.working_folder,
            _ => return None,
        };
        Some(field.as_deref())
    }
}

/// A filled template. `config_file` is the text to write for the session.
#[derive(Debug, PartialEq, Eq)]
pub struct FilledCommand {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub config_file: Option<String>,
}

fn harnesses_folder(folder: &Path) -> PathBuf {
    setup_folder(folder).join(HARNESSES_FOLDER_NAME)
}

/// The harness templates in the folder, by name. They're read, and checked,
/// only when used. No harnesses folder means none.
pub fn harness_names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(harnesses_folder(folder))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|item| item.path().is_file())
        .filter_map(|item| item.file_name().to_string_lossy().strip_suffix(HARNESS_FILE_EXTENSION).map(str::to_string))
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .collect();
    names.sort();
    names
}

pub fn read_harness(folder: &Path, name: &str) -> Result<HarnessTemplate, String> {
    let path = harnesses_folder(folder).join(format!("{name}{HARNESS_FILE_EXTENSION}"));
    if name == CLAUDE_CODE_HARNESS {
        return Err(format!("{}: Claude Code is started by Legion itself, not from a harness file", path.display()));
    }
    let text = fs::read_to_string(&path).map_err(|_| format!("no harness {name} in .{NAME}/{HARNESSES_FOLDER_NAME}/"))?;
    let template: HarnessTemplate = serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    check_template(&template, name).map_err(|problem| format!("{}: {problem}", path.display()))?;
    Ok(template)
}

fn check_template(template: &HarnessTemplate, file_name: &str) -> Result<(), String> {
    if template.name != file_name {
        return Err(format!("name {:?} doesn't match the file name {file_name:?}", template.name));
    }
    let known = Blanks::default();
    let unknown = template_texts(template).into_iter().flat_map(blank_names).find(|blank| known.value(blank).is_none());
    if let Some(blank) = unknown {
        return Err(format!("unknown blank {{{blank}}}"));
    }
    if template.tool_name.as_deref().is_some_and(|tool_name| !tool_name.contains(TOOL_BLANK)) {
        return Err(format!("toolName must contain {TOOL_BLANK}"));
    }
    Ok(())
}

/// Every text Legion fills in. `toolName` isn't one: its `{tool}` is filled
/// per tool, by whoever shows the tools.
fn template_texts(template: &HarnessTemplate) -> Vec<&str> {
    let args = template.args.iter().flat_map(|item| match item {
        ArgumentItem::One(text) => vec![text.as_str()],
        ArgumentItem::Group(texts) => texts.iter().map(String::as_str).collect(),
    });
    std::iter::once(template.command.as_str())
        .chain(args)
        .chain(template.env.values().map(String::as_str))
        .chain(template.config_file.iter().flat_map(json_texts))
        .collect()
}

/// Every key and string at any depth.
fn json_texts(value: &Value) -> Vec<&str> {
    match value {
        Value::String(text) => vec![text.as_str()],
        Value::Array(items) => items.iter().flat_map(json_texts).collect(),
        Value::Object(entries) => entries.iter().flat_map(|(key, value)| std::iter::once(key.as_str()).chain(json_texts(value))).collect(),
        _ => Vec::new(),
    }
}

#[derive(Clone, Copy)]
enum Piece<'a> {
    Literal(&'a str),
    Blank(&'a str),
}

/// A blank is `{` a letter, then letters or digits, `}`. Any other braces are
/// plain text, so JSON-looking text passes through.
fn is_blank_name(text: &str) -> bool {
    let mut characters = text.chars();
    characters.next().is_some_and(|first| first.is_ascii_alphabetic()) && characters.all(|character| character.is_ascii_alphanumeric())
}

fn literal(text: &str) -> Vec<Piece<'_>> {
    match text.is_empty() {
        true => Vec::new(),
        false => vec![Piece::Literal(text)],
    }
}

fn pieces(text: &str) -> Vec<Piece<'_>> {
    let Some(open) = text.find('{') else {
        return literal(text);
    };
    let after = &text[open + 1..];
    match after.find('}').map(|close| &after[..close]).filter(|name| is_blank_name(name)) {
        Some(name) => [literal(&text[..open]), vec![Piece::Blank(name)], pieces(&after[name.len() + 1..])].concat(),
        None => [literal(&text[..open + 1]), pieces(after)].concat(),
    }
}

fn blank_names(text: &str) -> Vec<&str> {
    pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Blank(name) => Some(name),
            Piece::Literal(_) => None,
        })
        .collect()
}

/// Fills in one pass, so a value is never scanned for blanks. Err is the
/// first blank with no value.
fn fill_text(text: &str, blanks: &Blanks) -> Result<String, String> {
    pieces(text)
        .into_iter()
        .map(|piece| match piece {
            Piece::Literal(literal) => Ok(literal.to_string()),
            Piece::Blank(blank) => blanks.value(blank).flatten().map(str::to_string).ok_or_else(|| blank.to_string()),
        })
        .collect()
}

/// Values go in as strings, never parsed as JSON.
fn fill_json(value: &Value, blanks: &Blanks) -> Result<Value, String> {
    match value {
        Value::String(text) => fill_text(text, blanks).map(Value::String),
        Value::Array(items) => items.iter().map(|item| fill_json(item, blanks)).collect::<Result<_, _>>().map(Value::Array),
        Value::Object(entries) => entries
            .iter()
            .map(|(key, value)| Ok((fill_text(key, blanks)?, fill_json(value, blanks)?)))
            .collect::<Result<Map<_, _>, String>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}

fn fill_argument(item: &ArgumentItem, blanks: &Blanks) -> Result<Vec<String>, String> {
    match item {
        ArgumentItem::One(text) => fill_text(text, blanks).map(|filled| vec![filled]),
        // fill_text only fails on a blank with no value, which leaves the group out.
        ArgumentItem::Group(texts) => Ok(texts.iter().map(|text| fill_text(text, blanks)).collect::<Result<_, _>>().unwrap_or_default()),
    }
}

pub fn fill(template: &HarnessTemplate, blanks: &Blanks) -> Result<FilledCommand, String> {
    let no_value = |blank: String| format!("harness {}: blank {{{blank}}} has no value", template.name);
    let command = fill_text(&template.command, blanks).map_err(no_value)?;
    let args = template
        .args
        .iter()
        .map(|item| fill_argument(item, blanks))
        .collect::<Result<Vec<_>, _>>()
        .map_err(no_value)?
        .concat();
    let env = template
        .env
        .iter()
        .map(|(key, value)| fill_text(value, blanks).map(|filled| (key.clone(), filled)))
        .collect::<Result<_, _>>()
        .map_err(no_value)?;
    let config_file = template
        .config_file
        .as_ref()
        .map(|config| fill_json(config, blanks).map_err(no_value))
        .transpose()?
        .map(|config| serde_json::to_string_pretty(&config).map_err(|error| format!("harness {}: {error}", template.name)))
        .transpose()?;
    Ok(FilledCommand { command, args, env, config_file })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENCODE: &str = r#"{
  "$schema": "https://legion/schemas/harness.json",
  "name": "opencode",
  "command": "opencode",
  "args": ["--model", "{modelProvider}/{model}", "--prompt", "{firstPrompt}"],
  "env": { "OPENCODE_CONFIG": "{configFile}" },
  "tools": "mcp",
  "toolName": "legion_{tool}",
  "configFile": {
    "mcp": {
      "legion": {
        "command": ["{legionCommand}", "mcp"],
        "environment": { "LEGION_DEPLOYMENT": "{deployment}", "LEGION_POSITION": "{position}", "LEGION_SOCKET": "{socket}" },
        "type": "local"
      }
    },
    "provider": {
      "{modelProvider}": { "npm": "@ai-sdk/openai-compatible", "options": { "baseURL": "{address}" } }
    }
  }
}"#;

    fn scratch_folder() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("{NAME}-harness-{}", crate::ids::new_id()));
        fs::create_dir_all(harnesses_folder(&folder)).unwrap();
        folder
    }

    fn write_harness(folder: &Path, name: &str, text: &str) -> PathBuf {
        let path = harnesses_folder(folder).join(format!("{name}{HARNESS_FILE_EXTENSION}"));
        fs::write(&path, text).unwrap();
        path
    }

    fn opencode_blanks() -> Blanks {
        Blanks {
            first_prompt: Some("Start on mission 3.".into()),
            model: Some("qwen3".into()),
            model_provider: Some("spark-fast".into()),
            address: Some("http://spark:8002/v1".into()),
            config_file: Some("/tmp/session/opencode.json".into()),
            legion_command: Some("/bin/legion".into()),
            deployment: Some("2ca9d8ac".into()),
            position: Some("builder".into()),
            socket: Some("/tmp/legion.sock".into()),
            ..Default::default()
        }
    }

    fn template(json: &str) -> HarnessTemplate {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_opencode_template_parses_and_fills() {
        let folder = scratch_folder();
        write_harness(&folder, "opencode", OPENCODE);
        let harness = read_harness(&folder, "opencode").unwrap();
        assert_eq!(harness.tools, ToolRoute::Mcp);
        assert_eq!(harness.tool_name.as_deref(), Some("legion_{tool}"));
        let filled = fill(&harness, &opencode_blanks()).unwrap();
        assert_eq!(filled.command, "opencode");
        assert_eq!(filled.args, ["--model", "spark-fast/qwen3", "--prompt", "Start on mission 3."]);
        assert_eq!(filled.env, BTreeMap::from([("OPENCODE_CONFIG".to_string(), "/tmp/session/opencode.json".to_string())]));
        let config = r#"{
  "mcp": {
    "legion": {
      "command": [
        "/bin/legion",
        "mcp"
      ],
      "environment": {
        "LEGION_DEPLOYMENT": "2ca9d8ac",
        "LEGION_POSITION": "builder",
        "LEGION_SOCKET": "/tmp/legion.sock"
      },
      "type": "local"
    }
  },
  "provider": {
    "spark-fast": {
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "http://spark:8002/v1"
      }
    }
  }
}"#;
        assert_eq!(filled.config_file.as_deref(), Some(config));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn an_unknown_blank_is_refused_naming_it_and_the_file() {
        let folder = scratch_folder();
        let path = write_harness(&folder, "pi", r#"{"name": "pi", "command": "pi", "args": [["--x", "{harnessConfigFile}"]], "tools": "commands"}"#);
        let error = read_harness(&folder, "pi").unwrap_err();
        assert_eq!(error, format!("{}: unknown blank {{harnessConfigFile}}", path.display()));
        write_harness(&folder, "pi", r#"{"name": "pi", "command": "pi", "args": [], "tools": "mcp", "configFile": {"{nope}": 1}}"#);
        assert!(read_harness(&folder, "pi").unwrap_err().ends_with("unknown blank {nope}"), "config keys are checked too");
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_name_that_doesnt_match_the_file_is_refused() {
        let folder = scratch_folder();
        let path = write_harness(&folder, "pi", r#"{"name": "opencode", "command": "pi", "args": [], "tools": "commands"}"#);
        let error = read_harness(&folder, "pi").unwrap_err();
        assert_eq!(error, format!(r#"{}: name "opencode" doesn't match the file name "pi""#, path.display()));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_claude_code_file_is_refused() {
        let folder = scratch_folder();
        let path = write_harness(&folder, CLAUDE_CODE_HARNESS, r#"{"name": "claude-code", "command": "claude", "args": [], "tools": "mcp"}"#);
        let error = read_harness(&folder, CLAUDE_CODE_HARNESS).unwrap_err();
        assert_eq!(error, format!("{}: Claude Code is started by Legion itself, not from a harness file", path.display()));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_missing_harness_and_bad_tool_name_are_refused() {
        let folder = scratch_folder();
        assert_eq!(read_harness(&folder, "pi").unwrap_err(), format!("no harness pi in .{NAME}/{HARNESSES_FOLDER_NAME}/"));
        let path = write_harness(&folder, "pi", r#"{"name": "pi", "command": "pi", "args": [], "tools": "commands", "toolName": "legion"}"#);
        assert_eq!(read_harness(&folder, "pi").unwrap_err(), format!("{}: toolName must contain {{tool}}", path.display()));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_group_with_an_empty_blank_is_left_out() {
        let harness = template(r#"{"name": "pi", "command": "pi", "args": [["--permission-mode", "{permissionMode}"], ["--print"], "{firstPrompt}"], "tools": "commands"}"#);
        let blanks = Blanks { first_prompt: Some("go".into()), ..Default::default() };
        assert_eq!(fill(&harness, &blanks).unwrap().args, ["--print", "go"]);
        let with_mode = Blanks { permission_mode: Some("auto".into()), ..blanks };
        assert_eq!(fill(&harness, &with_mode).unwrap().args, ["--permission-mode", "auto", "--print", "go"]);
    }

    #[test]
    fn a_single_argument_with_no_value_fails_loud() {
        let harness = template(r#"{"name": "pi", "command": "pi", "args": ["--model", "{model}"], "tools": "commands"}"#);
        assert_eq!(fill(&harness, &Blanks::default()).unwrap_err(), "harness pi: blank {model} has no value");
        let empty = Blanks { model: Some(String::new()), ..Default::default() };
        assert_eq!(fill(&harness, &empty).unwrap().args, ["--model", ""], "an empty string is a value");
    }

    #[test]
    fn values_are_filled_once_and_other_braces_are_text() {
        let harness = template(r#"{"name": "pi", "command": "pi", "args": ["{\"a\": {}}", "{model}{model}", "{{model}}"], "tools": "commands"}"#);
        let blanks = Blanks { model: Some("{firstPrompt}".into()), ..Default::default() };
        assert_eq!(fill(&harness, &blanks).unwrap().args, [r#"{"a": {}}"#, "{firstPrompt}{firstPrompt}", "{{firstPrompt}}"]);
    }

    #[test]
    fn unknown_fields_are_refused_and_schema_is_accepted() {
        assert!(serde_json::from_str::<HarnessTemplate>(r#"{"name": "pi", "command": "pi", "args": [], "tools": "mcp", "extra": 1}"#).is_err());
        assert!(serde_json::from_str::<HarnessTemplate>(r#"{"name": "pi", "command": "pi", "args": [], "tools": "smoke"}"#).is_err());
        assert!(serde_json::from_str::<HarnessTemplate>(r#"{"$schema": "x", "name": "pi", "command": "pi", "args": [], "tools": "mcp"}"#).is_ok());
    }

    #[test]
    fn harness_names_are_the_sorted_json_files() {
        let missing = std::env::temp_dir().join(format!("{NAME}-no-harness-{}", crate::ids::new_id()));
        assert!(harness_names(&missing).is_empty());
        let folder = scratch_folder();
        ["pi.json", "opencode.json", ".hidden.json", "notes.md"].iter().for_each(|file| fs::write(harnesses_folder(&folder).join(file), "{}").unwrap());
        assert_eq!(harness_names(&folder), ["opencode", "pi"]);
        fs::remove_dir_all(&folder).unwrap();
    }
}
