//! Everything about one folder, for the app's folder page.

use std::path::Path;

use legion2_proto::{DecisionDetail, FolderDetail, OperatorDetail, PipelineDetail, Reply};

use crate::{
    constants::DEFAULT_OPERATOR_COPY_LIMIT,
    daemon::Daemon,
    setup::{operator_names, parse_pipeline, pipeline_names, read_operator, read_settings, validate_pipeline, Operator, Pipeline},
};

/// A pipeline as the page shows it, with why Legion can't use it if it can't.
pub fn describe_pipeline(name: &str, parsed: Result<Pipeline, String>, known_operators: &[String]) -> PipelineDetail {
    match parsed {
        Err(problem) => PipelineDetail { name: name.into(), operators: vec![], first: None, decisions: vec![], problem: Some(problem) },
        Ok(pipeline) => {
            let decisions = pipeline
                .decisions
                .iter()
                .flat_map(|(operator, decisions)| {
                    // One row per operator the work goes to, so the app draws each.
                    decisions.iter().flat_map(move |decision| {
                        decision.next.targets().into_iter().map(move |target| DecisionDetail {
                            operator: operator.clone(),
                            condition: decision.condition.clone(),
                            next: target.to_string(),
                        })
                    })
                })
                .collect();
            let problem = validate_pipeline(&pipeline, known_operators).err();
            PipelineDetail { name: name.into(), operators: pipeline.operators, first: Some(pipeline.first), decisions, problem }
        }
    }
}

/// An operator as the page shows it, with why Legion can't read it if it can't.
pub fn describe_operator(name: &str, read: Result<Operator, String>) -> OperatorDetail {
    match read {
        Err(problem) => OperatorDetail {
            name: name.into(),
            definition: String::new(),
            model: None,
            copy_limit: DEFAULT_OPERATOR_COPY_LIMIT,
            permission_mode: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            problem: Some(problem),
        },
        Ok(operator) => OperatorDetail {
            name: name.into(),
            definition: operator.definition,
            model: operator.config.model,
            copy_limit: operator.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT),
            permission_mode: operator.config.permission_mode.map(|mode| mode.flag_value().to_string()),
            allowed_tools: operator.config.allowed_tools,
            disallowed_tools: operator.config.disallowed_tools,
            problem: None,
        },
    }
}

fn describe_setup(folder: &Path) -> (Vec<PipelineDetail>, Vec<OperatorDetail>) {
    let known_operators = operator_names(folder);
    let pipelines = pipeline_names(folder)
        .iter()
        .map(|name| describe_pipeline(name, parse_pipeline(folder, name).map(|(pipeline, _)| pipeline), &known_operators))
        .collect();
    let operators = known_operators.iter().map(|name| describe_operator(name, read_operator(folder, name))).collect();
    (pipelines, operators)
}

impl Daemon {
    pub fn describe_folder(&self, folder_key: &str) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let folder = &state.folders[state.find_folder(folder_key)?];
        let settings = read_settings(&folder.path)?;
        let (pipelines, operators) = describe_setup(&folder.path);
        let detail = FolderDetail {
            folder: folder.info(),
            check: settings.check,
            permission_mode: settings.permission_mode.map(|mode| mode.flag_value().to_string()),
            outside_folder: folder.outside_folder.to_string_lossy().into_owned(),
            pipelines,
            operators,
            deployments: folder.deployments.clone(),
        };
        Ok(Reply::FolderDetail { detail })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::OperatorConfig;

    fn team(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_pipeline_lists_every_decision() {
        let pipeline: Pipeline =
            serde_yaml::from_str("operators: [builder, reviewer]\nfirst: builder\ndecisions:\n  reviewer:\n    - {condition: approved, next: done}\n    - {condition: needs work, next: builder}\n").unwrap();
        let detail = describe_pipeline("feature", Ok(pipeline), &team(&["builder", "reviewer"]));
        assert_eq!(detail.first.as_deref(), Some("builder"));
        assert_eq!(detail.decisions.len(), 2);
        assert_eq!(detail.decisions[1].next, "builder");
        assert_eq!(detail.problem, None);
    }

    #[test]
    fn a_pipeline_with_a_missing_operator_says_so() {
        let pipeline: Pipeline = serde_yaml::from_str("operators: [tester]\nfirst: tester\n").unwrap();
        let detail = describe_pipeline("qa", Ok(pipeline), &team(&["builder"]));
        assert!(detail.problem.unwrap().contains("tester"));
    }

    #[test]
    fn an_unreadable_pipeline_keeps_its_name_and_problem() {
        let detail = describe_pipeline("broken", Err("bad yaml".into()), &[]);
        assert_eq!((detail.name.as_str(), detail.problem.as_deref()), ("broken", Some("bad yaml")));
    }

    #[test]
    fn an_operator_shows_its_limit_or_the_default() {
        let operator = Operator { definition: "# Builder".into(), config: OperatorConfig::default() };
        assert_eq!(describe_operator("builder", Ok(operator)).copy_limit, DEFAULT_OPERATOR_COPY_LIMIT);
        let broken = describe_operator("ghost", Err("no definition.md".into()));
        assert_eq!(broken.problem.as_deref(), Some("no definition.md"));
    }
}
