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

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub name: String,
    /// A command that checks a mission's work, such as `npm test`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
}

/// `.<NAME>/operators/<name>/operator.json`. Everything is optional.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperatorConfig {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    pub model: Option<String>,
    /// How many copies may run at once.
    pub limit: Option<u32>,
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
    pub next: String,
}

const STARTING_PIPELINE: &str = "\
# The route a mission takes through the squad. The commander and operators
# read this table to decide who goes next; Legion doesn't route work itself.
# `next` is an operator, `done` or `commander`.
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
    let settings = Settings { name: folder_name, check: None };
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
    let bad_route = pipeline.decisions.iter().find_map(|(operator, decisions)| {
        decisions
            .iter()
            .find(|decision| decision.next != PIPELINE_END && decision.next != COMMANDER && !is_on_team(&decision.next))
            .map(|decision| (operator, decision))
    });
    match bad_route {
        Some((operator, decision)) => Err(format!(
            "{operator} sends {:?} to {:?}, which isn't an operator, {PIPELINE_END} or {COMMANDER}",
            decision.condition, decision.next
        )),
        None => Ok(()),
    }
}

/// Reads and checks a pipeline. Returns it with its text as written, which
/// is what the commander and operators are shown.
pub fn read_pipeline(folder: &Path, name: &str) -> Result<(Pipeline, String), String> {
    let path = setup_folder(folder).join(PIPELINES_FOLDER_NAME).join(format!("{name}{PIPELINE_FILE_EXTENSION}"));
    let text = fs::read_to_string(&path).map_err(|_| {
        format!("no pipeline {name:?} in {} (it has: {})", folder.display(), pipeline_names(folder).join(", "))
    })?;
    let pipeline: Pipeline = serde_yaml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
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
    fn decisions_route_to_team_done_or_commander() {
        let routes = "operators: [builder]\nfirst: builder\ndecisions:\n  builder:\n    - {condition: a, next: done}\n    - {condition: b, next: commander}\n";
        assert_eq!(validate_pipeline(&parse(routes), &team(&["builder"])), Ok(()));
        let stray = "operators: [builder]\nfirst: builder\ndecisions:\n  builder:\n    - {condition: a, next: tester}\n";
        assert!(validate_pipeline(&parse(stray), &team(&["builder", "tester"])).is_err());
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(serde_yaml::from_str::<Pipeline>("operators: []\nfirst: x\nextra: 1\n").is_err());
    }
}
