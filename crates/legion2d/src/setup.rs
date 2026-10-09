//! A folder's shared setup, in `.<NAME>/` in the repo: settings, operators and
//! pipelines. This is what goes in git; everything else lives outside.

use std::{
    collections::BTreeMap,
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use legion2_proto::{COMMANDER, NAME};
use serde::{Deserialize, Serialize};

pub const SETTINGS_FILE_NAME: &str = "legion.json";
pub const PIPELINES_FOLDER_NAME: &str = "pipelines";
pub const OPERATORS_FOLDER_NAME: &str = "operators";
pub const DEFINITION_FILE_NAME: &str = "definition.md";
pub const OPERATOR_CONFIG_FILE_NAME: &str = "operator.json";
pub const PIPELINE_FILE_EXTENSION: &str = ".yaml";
/// Where a pipeline ends. A decision can also send work to the commander.
pub const PIPELINE_END: &str = "done";
pub const STARTING_PIPELINE_NAME: &str = "feature";
pub use legion2_proto::{NO_PIPELINE, OLD_NO_PIPELINE};
/// What the commander and operators read about a pipeline with no decisions.
const NO_PIPELINE_NOTE: &str = "There's no pipeline: every step goes through the commander. The commander starts whichever operator suits the mission first. When an operator's step is done, it hands off to commander (next: commander). The commander then picks who works on it next, or, once nothing more is needed, tells the operator that did the last step to report the mission done.";
/// The longest an operator's one-line summary gets in the team's roster.
const SUMMARY_MAX_CHARS: usize = 140;

/// Claude Code's permission modes (`claude --permission-mode`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    AcceptEdits,
    Auto,
    BypassPermissions,
    Manual,
    DontAsk,
    Plan,
}

impl PermissionMode {
    pub fn flag_value(self) -> &'static str {
        match self {
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Auto => "auto",
            PermissionMode::BypassPermissions => "bypassPermissions",
            PermissionMode::Manual => "manual",
            PermissionMode::DontAsk => "dontAsk",
            PermissionMode::Plan => "plan",
        }
    }
}

/// When a conversation hands over to a fresh one: at a whole-number
/// percentage from 1 to 100, or `false` for never.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum ClearAt {
    Percent(ClearPercent),
    Never(NeverClear),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(try_from = "u8", into = "u8")]
pub struct ClearPercent(u8);

impl TryFrom<u8> for ClearPercent {
    type Error = String;
    fn try_from(percent: u8) -> Result<Self, String> {
        match percent {
            1..=100 => Ok(ClearPercent(percent)),
            _ => Err(format!("clearAt must be a percentage from 1 to 100, not {percent}")),
        }
    }
}

impl From<ClearPercent> for u8 {
    fn from(percent: ClearPercent) -> u8 {
        percent.0
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(try_from = "bool", into = "bool")]
pub struct NeverClear;

impl TryFrom<bool> for NeverClear {
    type Error = String;
    fn try_from(value: bool) -> Result<Self, String> {
        match value {
            false => Ok(NeverClear),
            true => Err("clearAt can't be true: give a percentage, or false to never clear".into()),
        }
    }
}

impl From<NeverClear> for bool {
    fn from(_: NeverClear) -> bool {
        false
    }
}

/// The percentage a position hands over at, or None for never: its
/// operator's clearAt, else the folder's, else the default.
pub fn clear_at_percent(operator: Option<ClearAt>, folder: Option<ClearAt>) -> Option<u8> {
    match operator.or(folder) {
        None => Some(crate::constants::DEFAULT_CLEAR_AT_PERCENT),
        Some(ClearAt::Percent(percent)) => Some(percent.into()),
        Some(ClearAt::Never(_)) => None,
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub name: String,
    /// A command that checks a mission's work, such as `npm test`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// The permission mode every session in the folder starts in, unless its
    /// operator sets its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,
    /// When every session in the folder hands over to a fresh conversation,
    /// unless its operator sets its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_at: Option<ClearAt>,
    /// The strategist, which suggests speed-ups to the commander. Off
    /// unless enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategist: Option<StrategistSettings>,
}

/// `"strategist": { "enabled": true, "everyMinutes": 10 }` in legion.json.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrategistSettings {
    pub enabled: bool,
    /// How often it looks for speed-ups.
    #[serde(default = "default_strategist_check_minutes")]
    pub every_minutes: NonZeroU32,
}

fn default_strategist_check_minutes() -> NonZeroU32 {
    NonZeroU32::new(crate::constants::DEFAULT_STRATEGIST_CHECK_MINUTES).unwrap_or(NonZeroU32::MIN)
}

/// How often the strategist checks, or None when it's off.
pub fn strategist_check_minutes(settings: &Settings) -> Option<u32> {
    settings.strategist.filter(|strategist| strategist.enabled).map(|strategist| strategist.every_minutes.get())
}

/// `.<NAME>/operators/<name>/operator.json`. Everything is optional.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperatorConfig {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<PermissionMode>,
    /// How many copies may run at once.
    pub limit: Option<u32>,
    pub clear_at: Option<ClearAt>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
}

pub struct Operator {
    pub definition: String,
    pub config: OperatorConfig,
}

/// `.<NAME>/pipelines/<name>.yaml`: who's on the team, who starts, and
/// where each operator's decisions send the work. Legion checks it but never
/// routes by it; the commander and operators read it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pipeline {
    pub operators: Vec<String>,
    /// Who starts each mission; with none, the commander picks.
    #[serde(default)]
    pub first: Option<String>,
    #[serde(default)]
    pub decisions: BTreeMap<String, Vec<Decision>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub condition: String,
    pub next: Next,
}

/// Who a decision sends the work to: one operator (or `done`, or
/// `commander`), or several operators who work on it at the same time.
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Next {
    One(String),
    Together(Vec<String>),
}

impl Next {
    pub fn targets(&self) -> Vec<&str> {
        match self {
            Next::One(target) => vec![target.as_str()],
            Next::Together(targets) => targets.iter().map(String::as_str).collect(),
        }
    }
}

const STARTING_PIPELINE: &str = "\
# The route a mission takes through the squad. The commander and operators
# read this table to decide who goes next; Legion doesn't route work itself.
# `next` is an operator, `done` or `commander`, or a list of operators who
# work on the mission at the same time, such as [security-reviewer, docs-writer].
# The commander waits for each of them to hand off before moving it on.
operators: [planner, builder, reviewer]
first: planner
decisions:
  planner:
    - condition: the plan is ready
      next: builder
  builder:
    - condition: the change is built and committed
      next: reviewer
    - condition: the plan is wrong or unclear
      next: planner
  reviewer:
    - condition: approved
      next: done
    - condition: needs work
      next: builder
    - condition: blocked
      next: commander
";

const STARTING_OPERATORS: &[(&str, &str)] = &[
    ("planner", include_str!("../../../templates/operators/planner/definition.md")),
    ("builder", include_str!("../../../templates/operators/builder/definition.md")),
    ("reviewer", include_str!("../../../templates/operators/reviewer/definition.md")),
];

pub fn setup_folder(folder: &Path) -> PathBuf {
    folder.join(format!(".{NAME}"))
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("can't create {}: {error}", parent.display()))?;
    }
    fs::write(path, text).map_err(|error| format!("can't write {}: {error}", path.display()))
}

/// Sets up `.<NAME>/` with a starting pipeline and operators, unless the
/// folder already has one. Returns whether it made one.
pub fn ensure_setup(folder: &Path) -> Result<bool, String> {
    let setup = setup_folder(folder);
    let has_setup = setup.join(SETTINGS_FILE_NAME).exists();
    if has_setup {
        return Ok(false);
    }
    let folder_name = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| NAME.into());
    let settings = Settings { name: folder_name, ..Default::default() };
    let settings_json = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    let pipeline_path = setup.join(PIPELINES_FOLDER_NAME).join(format!("{STARTING_PIPELINE_NAME}{PIPELINE_FILE_EXTENSION}"));
    write_file(&setup.join(SETTINGS_FILE_NAME), &(settings_json + "\n"))?;
    write_file(&pipeline_path, STARTING_PIPELINE)?;
    STARTING_OPERATORS.iter().try_for_each(|(operator, definition)| {
        write_file(&setup.join(OPERATORS_FOLDER_NAME).join(operator).join(DEFINITION_FILE_NAME), definition)
    })?;
    Ok(true)
}

pub fn read_settings(folder: &Path) -> Result<Settings, String> {
    let path = setup_folder(folder).join(SETTINGS_FILE_NAME);
    let text = fs::read_to_string(&path).map_err(|error| format!("can't read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn names_in(folder: &Path, want_folders: bool, suffix: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|item| item.path().is_dir() == want_folders)
        .filter_map(|item| item.file_name().to_string_lossy().strip_suffix(suffix).map(str::to_string))
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// Every pipeline the folder can run: its files, and none.
pub fn pipeline_names(folder: &Path) -> Vec<String> {
    let mut names = names_in(&setup_folder(folder).join(PIPELINES_FOLDER_NAME), false, PIPELINE_FILE_EXTENSION);
    if !names.iter().any(|name| name == NO_PIPELINE) {
        names.push(NO_PIPELINE.into());
        names.sort();
    }
    names
}

pub fn operator_names(folder: &Path) -> Vec<String> {
    names_in(&setup_folder(folder).join(OPERATORS_FOLDER_NAME), true, "")
}

/// Positions and words that mean something else to Legion.
const RESERVED_NAMES: &[&str] = &[COMMANDER, legion2_proto::STRATEGIST, legion2_proto::HUMAN, legion2_proto::LEGION, PIPELINE_END];

/// Operator and pipeline names become folder, file and position names:
/// lowercase letters, digits and dashes.
pub fn check_name(kind: &str, name: &str) -> Result<(), String> {
    let is_plain = !name.is_empty() && name.chars().all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-');
    if !is_plain || name.starts_with('-') {
        return Err(format!("{kind} name {name:?}: use lowercase letters, digits and dashes, like poem-writer"));
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(format!("{name:?} means something else to Legion; pick another {kind} name"));
    }
    Ok(())
}

/// A new operator: its definition, and a copy limit when it isn't the default.
pub fn add_operator(folder: &Path, name: &str, definition: &str, limit: Option<u32>) -> Result<(), String> {
    check_name("operator", name)?;
    let operator_folder = setup_folder(folder).join(OPERATORS_FOLDER_NAME).join(name);
    if operator_folder.exists() {
        return Err(format!("{name} is an operator already; edit {}", operator_folder.join(DEFINITION_FILE_NAME).display()));
    }
    let definition = definition.trim();
    if definition.is_empty() {
        return Err(format!("say what {name} does"));
    }
    let heading = if definition.starts_with('#') { String::new() } else { format!("# {name}\n\n") };
    write_file(&operator_folder.join(DEFINITION_FILE_NAME), &format!("{heading}{definition}\n"))?;
    match limit {
        Some(limit) => write_file(&operator_folder.join(OPERATOR_CONFIG_FILE_NAME), &format!("{{\n  \"limit\": {limit}\n}}\n")),
        None => Ok(()),
    }
}

/// Takes an operator out of the folder, unless a pipeline file still names it.
pub fn remove_operator(folder: &Path, name: &str) -> Result<(), String> {
    let operator_folder = setup_folder(folder).join(OPERATORS_FOLDER_NAME).join(name);
    if !operator_folder.is_dir() {
        return Err(format!("no operator {name} in {}", folder.display()));
    }
    let naming: Vec<String> = pipeline_names(folder)
        .into_iter()
        .filter(|pipeline| pipeline_path(folder, pipeline).exists())
        .filter(|pipeline| parse_pipeline(folder, pipeline).is_ok_and(|(read, _)| read.operators.iter().any(|operator| operator == name)))
        .collect();
    if !naming.is_empty() {
        return Err(format!("the {} pipeline names {name}; take it out of {} first", naming.join(" and "), if naming.len() == 1 { "it" } else { "them" }));
    }
    fs::remove_dir_all(&operator_folder).map_err(|error| format!("can't remove {}: {error}", operator_folder.display()))
}

/// Changes the folder's check command and permission mode; an empty check
/// takes it away. Its name stays: it names where the folder's data lives.
pub fn set_settings(folder: &Path, check: Option<String>, permission_mode: Option<PermissionMode>) -> Result<(), String> {
    let settings = read_settings(folder)?;
    let check = match check {
        Some(command) if command.trim().is_empty() => None,
        Some(command) => Some(command.trim().to_string()),
        None => settings.check,
    };
    let changed = Settings { check, permission_mode: permission_mode.or(settings.permission_mode), ..settings };
    let text = serde_json::to_string_pretty(&changed).map_err(|error| error.to_string())?;
    write_file(&setup_folder(folder).join(SETTINGS_FILE_NAME), &(text + "\n"))
}

/// Changes an operator's copies, model and permission mode, keeping the
/// rest of its operator.json. A model of "default" takes it away.
pub fn set_operator(folder: &Path, name: &str, limit: Option<u32>, model: Option<String>, permission_mode: Option<PermissionMode>) -> Result<(), String> {
    let operator_folder = setup_folder(folder).join(OPERATORS_FOLDER_NAME).join(name);
    if !operator_folder.is_dir() {
        return Err(format!("no operator {name} in {}", folder.display()));
    }
    let config_path = operator_folder.join(OPERATOR_CONFIG_FILE_NAME);
    let mut config: serde_json::Map<String, serde_json::Value> = match fs::read_to_string(&config_path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| format!("{}: {error}", config_path.display()))?,
        Err(_) => serde_json::Map::new(),
    };
    if let Some(limit) = limit {
        config.insert("limit".into(), limit.into());
    }
    match model.as_deref() {
        Some("default") => {
            config.remove("model");
        }
        Some(model) => {
            config.insert("model".into(), model.into());
        }
        None => {}
    }
    if let Some(mode) = permission_mode {
        config.insert("permissionMode".into(), mode.flag_value().into());
    }
    let text = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
    write_file(&config_path, &(text + "\n"))?;
    read_operator(folder, name).map(|_| ())
}

pub fn pipeline_file_text(folder: &Path, name: &str) -> Option<String> {
    fs::read_to_string(pipeline_path(folder, name)).ok()
}

/// Replaces what an operator does, keeping its operator.json.
pub fn define_operator(folder: &Path, name: &str, definition: &str) -> Result<(), String> {
    let path = setup_folder(folder).join(OPERATORS_FOLDER_NAME).join(name).join(DEFINITION_FILE_NAME);
    if !path.is_file() {
        return Err(format!("no operator {name} in {}", folder.display()));
    }
    if definition.trim().is_empty() {
        return Err(format!("say what {name} does"));
    }
    write_file(&path, &format!("{}\n", definition.trim_end()))
}

/// Replaces a pipeline's file, refusing text that isn't a pipeline the
/// folder can run, so a typo can't stop its deployments.
pub fn write_pipeline(folder: &Path, name: &str, text: &str) -> Result<(), String> {
    let path = pipeline_path(folder, name);
    if !path.is_file() {
        return Err(format!("no pipeline file {name} in {}", folder.display()));
    }
    let pipeline: Pipeline = serde_yaml::from_str(text).map_err(|error| format!("that isn't a pipeline: {error}"))?;
    validate_pipeline(&pipeline, &operator_names(folder))?;
    write_file(&path, text)
}

pub fn remove_pipeline(folder: &Path, name: &str) -> Result<(), String> {
    let path = pipeline_path(folder, name);
    if !path.is_file() {
        let why = if name == NO_PIPELINE { "no pipeline has no file; it's always there".to_string() } else { format!("no pipeline {name} in {}", folder.display()) };
        return Err(why);
    }
    fs::remove_file(&path).map_err(|error| format!("can't remove {}: {error}", path.display()))
}

/// A pipeline's file. In order, each operator passes to the next and the
/// last finishes it; otherwise it has no decisions, and the commander picks.
pub fn pipeline_yaml(operators: &[String], in_order: bool) -> String {
    let team = format!("operators: [{}]\n", operators.join(", "));
    if !in_order {
        return format!("# No set order: with no decisions, every step goes back to the commander,\n# who picks who works on the mission next.\n{team}");
    }
    let steps: Vec<String> = operators
        .iter()
        .enumerate()
        .map(|(index, operator)| {
            let next = operators.get(index + 1).map_or(PIPELINE_END, String::as_str);
            format!("  {operator}:\n    - condition: its step is done\n      next: {next}\n    - condition: blocked\n      next: {COMMANDER}\n")
        })
        .collect();
    format!("# Each operator passes the mission to the next; the last one finishes it.\n{team}first: {}\ndecisions:\n{}", operators[0], steps.concat())
}

pub fn add_pipeline(folder: &Path, name: &str, operators: &[String], in_order: bool) -> Result<(), String> {
    check_name("pipeline", name)?;
    let path = pipeline_path(folder, name);
    if path.exists() || name == NO_PIPELINE || name == OLD_NO_PIPELINE {
        return Err(format!("{name} is a pipeline already; pick another name"));
    }
    if operators.is_empty() {
        return Err("name at least one operator".into());
    }
    let known = operator_names(folder);
    if let Some(missing) = operators.iter().find(|operator| !known.contains(operator)) {
        return Err(format!("no operator {missing} in {} (it has: {})", folder.display(), known.join(", ")));
    }
    write_file(&path, &pipeline_yaml(operators, in_order))
}

/// An operator's definition in one line: its first line that isn't a heading.
pub fn operator_summary(definition: &str) -> String {
    let line = definition.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with('#')).unwrap_or("");
    match line.char_indices().nth(SUMMARY_MAX_CHARS) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

/// A pipeline with no decisions has no set order: every operator hands its
/// step back to the commander. Its table says so, with who's on the team.
fn as_commanders_pick(folder: &Path, pipeline: Pipeline, written: &str) -> (Pipeline, String) {
    let roster: Vec<String> = pipeline
        .operators
        .iter()
        .map(|operator| {
            let summary = read_operator(folder, operator).map(|read| operator_summary(&read.definition)).unwrap_or_default();
            format!("- {operator}: {summary}")
        })
        .collect();
    let decisions = pipeline
        .operators
        .iter()
        .map(|operator| (operator.clone(), vec![Decision { condition: "its step is done".into(), next: Next::One(COMMANDER.into()) }]))
        .collect();
    let text = format!("{written}{NO_PIPELINE_NOTE}\nThe operators:\n{}\n", roster.join("\n"));
    (Pipeline { decisions, ..pipeline }, text)
}

/// Checks a pipeline against the operators the folder has.
pub fn validate_pipeline(pipeline: &Pipeline, known_operators: &[String]) -> Result<(), String> {
    let is_on_team = |operator: &String| pipeline.operators.contains(operator);
    if pipeline.operators.is_empty() {
        return Err(format!("it has no operators; add one with `{NAME} operator add`"));
    }
    if let Some(missing) = pipeline.operators.iter().find(|operator| !known_operators.contains(operator)) {
        return Err(format!("operator {missing:?} has no folder in .{NAME}/{OPERATORS_FOLDER_NAME}"));
    }
    if let Some(first) = pipeline.first.as_ref().filter(|first| !is_on_team(first)) {
        return Err(format!("first, {first:?}, isn't in its operators"));
    }
    if let Some(stranger) = pipeline.decisions.keys().find(|operator| !is_on_team(operator)) {
        return Err(format!("decisions for {stranger:?}, which isn't in its operators"));
    }
    pipeline.decisions.iter().try_for_each(|(operator, decisions)| {
        decisions.iter().try_for_each(|decision| match route_problem(&decision.next, &is_on_team) {
            Some(problem) => Err(format!("{operator} sends {:?} {problem}", decision.condition)),
            None => Ok(()),
        })
    })
}

/// What's wrong with where a decision sends the work, if anything. Work that
/// splits goes to operators only: `done` and `commander` end or pause it.
fn route_problem(next: &Next, is_on_team: &dyn Fn(&String) -> bool) -> Option<String> {
    match next {
        Next::One(target) if target == PIPELINE_END || target == COMMANDER || is_on_team(target) => None,
        Next::One(target) => Some(format!("to {target:?}, which isn't an operator, {PIPELINE_END} or {COMMANDER}")),
        Next::Together(targets) if targets.len() < 2 => Some("to a list of fewer than two operators; name one without a list".into()),
        Next::Together(targets) => targets
            .iter()
            .find(|target| !is_on_team(target))
            .map(|stranger| format!("to {stranger:?} alongside others, but only operators on the team can share the work")),
    }
}

fn pipeline_path(folder: &Path, name: &str) -> PathBuf {
    setup_folder(folder).join(PIPELINES_FOLDER_NAME).join(format!("{name}{PIPELINE_FILE_EXTENSION}"))
}

/// Reads a pipeline without checking it against the folder's operators.
/// No pipeline needs no file: it's every operator in the folder.
pub fn parse_pipeline(folder: &Path, name: &str) -> Result<(Pipeline, String), String> {
    let path = pipeline_path(folder, name);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) if name == NO_PIPELINE || name == OLD_NO_PIPELINE => {
            let every_operator = Pipeline { operators: operator_names(folder), first: None, decisions: BTreeMap::new() };
            return Ok(as_commanders_pick(folder, every_operator, ""));
        }
        Err(_) => return Err(format!("no pipeline {name:?} in {} (it has: {})", folder.display(), pipeline_names(folder).join(", "))),
    };
    let pipeline: Pipeline = serde_yaml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    match pipeline.decisions.is_empty() {
        true => Ok(as_commanders_pick(folder, pipeline, &text)),
        false => Ok((pipeline, text)),
    }
}

/// Reads and checks a pipeline. Returns it with its text as written, which
/// is what the commander and operators are shown.
pub fn read_pipeline(folder: &Path, name: &str) -> Result<(Pipeline, String), String> {
    let (pipeline, text) = parse_pipeline(folder, name)?;
    let path = pipeline_path(folder, name);
    validate_pipeline(&pipeline, &operator_names(folder)).map_err(|problem| format!("{}: {problem}", path.display()))?;
    Ok((pipeline, text))
}

pub fn read_operator(folder: &Path, name: &str) -> Result<Operator, String> {
    let operator_folder = setup_folder(folder).join(OPERATORS_FOLDER_NAME).join(name);
    let definition = fs::read_to_string(operator_folder.join(DEFINITION_FILE_NAME))
        .map_err(|_| format!("operator {name:?} has no {DEFINITION_FILE_NAME} in {}", operator_folder.display()))?;
    let config_path = operator_folder.join(OPERATOR_CONFIG_FILE_NAME);
    let config = match fs::read_to_string(&config_path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| format!("{}: {error}", config_path.display()))?,
        Err(_) => OperatorConfig::default(),
    };
    Ok(Operator { definition, config })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_at_takes_a_percentage_or_false() {
        let parse = |json: &str| serde_json::from_str::<OperatorConfig>(json).map(|config| config.clear_at);
        assert_eq!(clear_at_percent(parse(r#"{"clearAt": 45}"#).unwrap(), None), Some(45));
        assert_eq!(clear_at_percent(parse(r#"{"clearAt": false}"#).unwrap(), None), None);
        assert!(parse(r#"{"clearAt": true}"#).is_err());
        assert!(parse(r#"{"clearAt": 0}"#).is_err());
        assert!(parse(r#"{"clearAt": 101}"#).is_err());
    }

    #[test]
    fn an_operators_clear_at_beats_the_folders_which_beats_the_default() {
        let percent = |value: u8| Some(ClearAt::Percent(ClearPercent::try_from(value).unwrap()));
        assert_eq!(clear_at_percent(percent(50), percent(40)), Some(50));
        assert_eq!(clear_at_percent(None, percent(40)), Some(40));
        assert_eq!(clear_at_percent(None, None), Some(crate::constants::DEFAULT_CLEAR_AT_PERCENT));
    }

    fn team(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn parse(text: &str) -> Pipeline {
        serde_yaml::from_str(text).unwrap()
    }

    #[test]
    fn the_starting_pipeline_is_valid() {
        let known = team(&["planner", "builder", "reviewer"]);
        assert_eq!(validate_pipeline(&parse(STARTING_PIPELINE), &known), Ok(()));
    }

    #[test]
    fn every_operator_needs_a_folder() {
        let error = validate_pipeline(&parse(STARTING_PIPELINE), &team(&["planner", "builder"])).unwrap_err();
        assert!(error.contains("reviewer"), "{error}");
    }

    #[test]
    fn first_must_be_on_the_team() {
        let pipeline = parse("operators: [builder]\nfirst: planner\n");
        assert!(validate_pipeline(&pipeline, &team(&["builder", "planner"])).is_err());
    }

    #[test]
    fn decisions_only_for_team_members() {
        let pipeline = parse("operators: [builder]\nfirst: builder\ndecisions:\n  planner: []\n");
        assert!(validate_pipeline(&pipeline, &team(&["builder", "planner"])).is_err());
    }

    #[test]
    fn a_step_can_send_work_to_several_operators_at_once() {
        let routes = "operators: [tester, security, docs]\nfirst: tester\ndecisions:\n  tester:\n    - {condition: works, next: [security, docs]}\n";
        let pipeline = parse(routes);
        assert_eq!(validate_pipeline(&pipeline, &team(&["tester", "security", "docs"])), Ok(()));
        assert_eq!(pipeline.decisions["tester"][0].next.targets(), vec!["security", "docs"]);
    }

    #[test]
    fn work_splits_only_between_two_or_more_team_members() {
        let known = team(&["tester", "security"]);
        let with_done = "operators: [tester, security]\nfirst: tester\ndecisions:\n  tester:\n    - {condition: a, next: [security, done]}\n";
        let alone = "operators: [tester, security]\nfirst: tester\ndecisions:\n  tester:\n    - {condition: a, next: [security]}\n";
        assert!(validate_pipeline(&parse(with_done), &known).unwrap_err().contains("done"));
        assert!(validate_pipeline(&parse(alone), &known).unwrap_err().contains("fewer than two"));
    }

    #[test]
    fn decisions_route_to_team_done_or_commander() {
        let routes = "operators: [builder]\nfirst: builder\ndecisions:\n  builder:\n    - {condition: a, next: done}\n    - {condition: b, next: commander}\n";
        assert_eq!(validate_pipeline(&parse(routes), &team(&["builder"])), Ok(()));
        let stray = "operators: [builder]\nfirst: builder\ndecisions:\n  builder:\n    - {condition: a, next: tester}\n";
        assert!(validate_pipeline(&parse(stray), &team(&["builder", "tester"])).is_err());
    }

    #[test]
    fn permission_modes_read_as_claude_spells_them() {
        let settings: Settings = serde_json::from_str(r#"{"name": "app", "permissionMode": "auto"}"#).unwrap();
        assert_eq!(settings.permission_mode, Some(PermissionMode::Auto));
        let config: OperatorConfig = serde_json::from_str(r#"{"permissionMode": "acceptEdits"}"#).unwrap();
        assert_eq!(config.permission_mode.map(PermissionMode::flag_value), Some("acceptEdits"));
        assert!(serde_json::from_str::<Settings>(r#"{"name": "app", "permissionMode": "yolo"}"#).is_err());
    }

    #[test]
    fn the_strategist_is_off_unless_enabled() {
        let read = |json: &str| strategist_check_minutes(&serde_json::from_str::<Settings>(json).unwrap());
        assert_eq!(read(r#"{"name": "app"}"#), None);
        assert_eq!(read(r#"{"name": "app", "strategist": {"enabled": false, "everyMinutes": 1}}"#), None);
        assert_eq!(read(r#"{"name": "app", "strategist": {"enabled": true}}"#), Some(crate::constants::DEFAULT_STRATEGIST_CHECK_MINUTES));
        assert_eq!(read(r#"{"name": "app", "strategist": {"enabled": true, "everyMinutes": 1}}"#), Some(1));
        assert!(serde_json::from_str::<Settings>(r#"{"name": "app", "strategist": {"enabled": true, "everyMinutes": 0}}"#).is_err());
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(serde_yaml::from_str::<Pipeline>("operators: []\nfirst: x\nextra: 1\n").is_err());
    }

    fn scratch_folder() -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("legion2-setup-{}", crate::ids::new_id()));
        fs::create_dir_all(setup_folder(&folder)).unwrap();
        folder
    }

    #[test]
    fn no_pipeline_is_every_operator_through_the_commander() {
        let folder = scratch_folder();
        assert!(read_pipeline(&folder, NO_PIPELINE).is_err(), "no operators, no team");
        add_operator(&folder, "poem-writer", "# Poem Writer\n\nYou write haiku into text files.", None).unwrap();
        add_operator(&folder, "editor", "Tightens each poem.", Some(2)).unwrap();
        let (pipeline, text) = read_pipeline(&folder, NO_PIPELINE).unwrap();
        assert!(read_pipeline(&folder, OLD_NO_PIPELINE).is_ok(), "deployments started as hub still run");
        assert_eq!(pipeline.operators, vec!["editor".to_string(), "poem-writer".to_string()]);
        assert_eq!(pipeline.first, None);
        assert!(pipeline.decisions.values().all(|decisions| decisions[0].next == Next::One(COMMANDER.into())));
        assert!(text.contains("every step goes through the commander") && text.contains("- poem-writer: You write haiku into text files."));
        assert!(pipeline_names(&folder).contains(&NO_PIPELINE.to_string()));
        assert_eq!(read_operator(&folder, "editor").unwrap().config.limit, Some(2));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_new_pipeline_runs_in_order_or_through_the_commander() {
        let folder = scratch_folder();
        ["writer", "editor"].iter().for_each(|name| add_operator(&folder, name, "Does a step.", None).unwrap());
        let team = vec!["writer".to_string(), "editor".to_string()];
        add_pipeline(&folder, "poems", &team, true).unwrap();
        let (in_order, _) = read_pipeline(&folder, "poems").unwrap();
        assert_eq!(in_order.first.as_deref(), Some("writer"));
        assert_eq!(in_order.decisions["writer"][0].next, Next::One("editor".into()));
        assert_eq!(in_order.decisions["editor"][0].next, Next::One(PIPELINE_END.into()));
        add_pipeline(&folder, "loose", &team, false).unwrap();
        let (loose, text) = read_pipeline(&folder, "loose").unwrap();
        assert_eq!((loose.first, loose.operators.len()), (None, 2));
        assert!(text.contains("every step goes through the commander"));
        assert!(add_pipeline(&folder, "poems", &team, true).is_err());
        assert!(add_pipeline(&folder, "bad", &["ghost".to_string()], true).is_err());
        assert!(remove_operator(&folder, "writer").unwrap_err().contains("names writer"));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn names_are_plain_and_not_legions_own() {
        assert!(check_name("operator", "poem-writer-2").is_ok());
        assert!(["Poem", "poem writer", "-x", "", "commander", "done"].iter().all(|name| check_name("operator", name).is_err()));
    }

    #[test]
    fn a_summary_is_the_first_line_that_isnt_a_heading() {
        assert_eq!(operator_summary("# Builder\n\nYou build the change.\nMore."), "You build the change.");
        assert!(operator_summary(&"x".repeat(500)).ends_with('…'));
    }


    #[test]
    fn a_pipeline_file_is_only_replaced_by_one_that_runs() {
        let folder = scratch_folder();
        add_operator(&folder, "poet", "Writes haiku.", None).unwrap();
        add_pipeline(&folder, "poems", &["poet".to_string()], true).unwrap();
        assert!(write_pipeline(&folder, "poems", "operators: [ghost]\n").unwrap_err().contains("ghost"));
        assert!(write_pipeline(&folder, "poems", "not: [a pipeline\n").is_err());
        write_pipeline(&folder, "poems", "operators: [poet]\n").unwrap();
        assert_eq!(pipeline_file_text(&folder, "poems").as_deref(), Some("operators: [poet]\n"));
        define_operator(&folder, "poet", "# Poet\n\nWrites limericks now.").unwrap();
        assert!(read_operator(&folder, "poet").unwrap().definition.contains("limericks"));
        assert!(define_operator(&folder, "ghost", "x").is_err());
        fs::remove_dir_all(&folder).unwrap();
    }
}
