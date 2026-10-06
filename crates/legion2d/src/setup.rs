//! A folder's shared setup, in `.<NAME>/` in the repo: settings, operators and
//! pipelines. This is what goes in git; everything else lives outside.

use std::{
    collections::BTreeMap,
    fs,
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
    pub first: String,
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

pub fn pipeline_names(folder: &Path) -> Vec<String> {
    names_in(&setup_folder(folder).join(PIPELINES_FOLDER_NAME), false, PIPELINE_FILE_EXTENSION)
}

pub fn operator_names(folder: &Path) -> Vec<String> {
    names_in(&setup_folder(folder).join(OPERATORS_FOLDER_NAME), true, "")
}

/// Checks a pipeline against the operators the folder has.
pub fn validate_pipeline(pipeline: &Pipeline, known_operators: &[String]) -> Result<(), String> {
    let is_on_team = |operator: &String| pipeline.operators.contains(operator);
    if let Some(missing) = pipeline.operators.iter().find(|operator| !known_operators.contains(operator)) {
        return Err(format!("operator {missing:?} has no folder in .{NAME}/{OPERATORS_FOLDER_NAME}"));
    }
    if !is_on_team(&pipeline.first) {
        return Err(format!("first, {:?}, isn't in its operators", pipeline.first));
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
pub fn parse_pipeline(folder: &Path, name: &str) -> Result<(Pipeline, String), String> {
    let path = pipeline_path(folder, name);
    let text = fs::read_to_string(&path).map_err(|_| {
        format!("no pipeline {name:?} in {} (it has: {})", folder.display(), pipeline_names(folder).join(", "))
    })?;
    let pipeline: Pipeline = serde_yaml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((pipeline, text))
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
    fn unknown_fields_are_refused() {
        assert!(serde_yaml::from_str::<Pipeline>("operators: []\nfirst: x\nextra: 1\n").is_err());
    }
}
