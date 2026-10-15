//! Which harness, address and model an operator runs on, from its
//! operator.json `model` and the folder's named `models` in legion.json.

use std::collections::BTreeMap;

use crate::setup::{check_name, ModelEntry};

/// The harness for operators that pick no named model.
pub const CLAUDE_CODE_HARNESS: &str = "claude-code";

#[derive(Debug, PartialEq, Eq)]
pub struct ModelChoice {
    /// The named entry it came from, if any.
    pub name: Option<String>,
    pub harness: String,
    pub address: Option<String>,
    /// None means the harness's default model.
    pub model: Option<String>,
}

/// A named entry wins; any other name (opus, sonnet …) is a Claude Code model.
pub fn model_choice(operator_model: Option<&str>, models: &BTreeMap<String, ModelEntry>) -> ModelChoice {
    let Some(operator_model) = operator_model else {
        return claude_code(None);
    };
    let Some(entry) = models.get(operator_model) else {
        return claude_code(Some(operator_model.to_string()));
    };
    ModelChoice { name: Some(operator_model.to_string()), harness: entry.harness.clone(), address: entry.address.clone(), model: Some(entry.model.clone()) }
}

fn claude_code(model: Option<String>) -> ModelChoice {
    ModelChoice { name: None, harness: CLAUDE_CODE_HARNESS.to_string(), address: None, model }
}

/// Entry names become what operators write in operator.json, so they follow
/// the operator-name rule.
pub fn check_models(models: &BTreeMap<String, ModelEntry>) -> Result<(), String> {
    models.iter().try_for_each(|(name, entry)| check_entry(name, entry))
}

fn check_entry(name: &str, entry: &ModelEntry) -> Result<(), String> {
    check_name("model", name)?;
    let fields = [("harness", Some(&entry.harness)), ("address", entry.address.as_ref()), ("model", Some(&entry.model))];
    // An empty model would reach the harness as `--model ""`.
    if let Some((field, _)) = fields.iter().find(|(_, value)| value.is_some_and(|value| value.trim().is_empty())) {
        return Err(format!("model {name:?}: {field:?} is empty"));
    }
    if entry.harness == CLAUDE_CODE_HARNESS && entry.address.is_some() {
        return Err(format!("model {name:?}: Claude Code takes no address; remove \"address\""));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPARK_MODELS: &str = r#"{
        "spark-big":  { "harness": "opencode", "address": "http://spark:8000/v1", "model": "qwen3.8-27b" },
        "spark-fast": { "harness": "opencode", "address": "http://spark:8002/v1", "model": "qwen3.6-35b-a3b" }
    }"#;

    fn parse(json: &str) -> BTreeMap<String, ModelEntry> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_named_entry_brings_its_harness_address_and_model() {
        let models = parse(SPARK_MODELS);
        assert_eq!(check_models(&models), Ok(()));
        let choice = model_choice(Some("spark-fast"), &models);
        assert_eq!(
            choice,
            ModelChoice {
                name: Some("spark-fast".into()),
                harness: "opencode".into(),
                address: Some("http://spark:8002/v1".into()),
                model: Some("qwen3.6-35b-a3b".into()),
            }
        );
    }

    #[test]
    fn a_plain_claude_name_runs_on_claude_code() {
        let choice = model_choice(Some("opus"), &parse(SPARK_MODELS));
        assert_eq!(choice, ModelChoice { name: None, harness: CLAUDE_CODE_HARNESS.into(), address: None, model: Some("opus".into()) });
    }

    #[test]
    fn no_model_is_claude_codes_default_with_or_without_models() {
        let default = ModelChoice { name: None, harness: CLAUDE_CODE_HARNESS.into(), address: None, model: None };
        assert_eq!(model_choice(None, &parse(SPARK_MODELS)), default);
        assert_eq!(model_choice(None, &BTreeMap::new()), default);
    }

    #[test]
    fn claude_code_takes_no_address() {
        let models = parse(r#"{"local": {"harness": "claude-code", "address": "http://x", "model": "opus"}}"#);
        assert_eq!(check_models(&models), Err("model \"local\": Claude Code takes no address; remove \"address\"".into()));
        assert_eq!(check_models(&parse(r#"{"local": {"harness": "claude-code", "model": "opus"}}"#)), Ok(()));
    }

    #[test]
    fn empty_fields_are_refused() {
        let error = |json: &str| check_models(&parse(json)).unwrap_err();
        assert_eq!(error(r#"{"a": {"harness": "", "model": "q"}}"#), r#"model "a": "harness" is empty"#);
        assert_eq!(error(r#"{"a": {"harness": "  ", "model": "q"}}"#), r#"model "a": "harness" is empty"#);
        assert_eq!(error(r#"{"a": {"harness": "opencode", "model": ""}}"#), r#"model "a": "model" is empty"#);
        assert_eq!(error(r#"{"a": {"harness": "opencode", "address": "", "model": "q"}}"#), r#"model "a": "address" is empty"#);
    }

    #[test]
    fn entry_names_follow_the_operator_name_rule() {
        let error = check_models(&parse(r#"{"Spark": {"harness": "opencode", "model": "qwen"}}"#)).unwrap_err();
        assert_eq!(error, r#"model name "Spark": use lowercase letters, digits and dashes, like poem-writer"#);
        assert!(check_models(&parse(r#"{"commander": {"harness": "opencode", "model": "qwen"}}"#)).is_err());
    }
}
