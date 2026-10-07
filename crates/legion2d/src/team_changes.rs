//! A commander is told its pipeline and copy limits when it starts. When
//! someone changes the pipeline or an operator while it runs, Legion tells
//! it the new ones, so it doesn't route by what it started with.

use std::path::Path;

use legion2_proto::{EntryKind, NewEntry, COMMANDER, LEGION};

use crate::{
    constants::DEFAULT_OPERATOR_COPY_LIMIT,
    daemon::Daemon,
    setup::{read_operator, read_pipeline},
};

/// A pipeline as the commander hears it: its table as written, and how many
/// copies of each of its operators may run at once.
#[derive(Debug, PartialEq)]
pub struct Team {
    pub pipeline_text: String,
    pub copy_limits: Vec<(String, u32)>,
}

pub fn read_team(folder: &Path, pipeline_name: &str) -> Result<Team, String> {
    let (pipeline, pipeline_text) = read_pipeline(folder, pipeline_name)?;
    let copy_limits = pipeline
        .operators
        .iter()
        .map(|operator| read_operator(folder, operator).map(|read| (operator.clone(), read.config.limit.unwrap_or(DEFAULT_OPERATOR_COPY_LIMIT))))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Team { pipeline_text, copy_limits })
}

pub fn copy_limits_text(copy_limits: &[(String, u32)]) -> String {
    copy_limits.iter().map(|(operator, limit)| format!("{operator} {limit}")).collect::<Vec<_>>().join(", ")
}

/// What's compared to tell a change: the team, or why it can't be read.
pub fn team_fingerprint(team: &Result<Team, String>) -> String {
    match team {
        Ok(team) => format!("{}\n{}", team.pipeline_text, copy_limits_text(&team.copy_limits)),
        Err(problem) => problem.clone(),
    }
}

pub fn team_change_message(pipeline_name: &str, team: &Result<Team, String>) -> String {
    match team {
        Ok(team) => format!(
            "The {pipeline_name} pipeline or its operators changed. Route by this from now on, not by what you started with.\n\nThe pipeline table:\n{}\nHow many copies of each operator may run at once: {}.",
            team.pipeline_text,
            copy_limits_text(&team.copy_limits)
        ),
        Err(problem) => format!("The {pipeline_name} pipeline can't be read any more, so starting operators will fail until it's fixed: {problem}"),
    }
}

impl Daemon {
    /// Remembers the team a commander was just told about.
    pub fn record_team_told_to_commander(&self, deployment_id: &str, fingerprint: String) {
        self.state.lock().unwrap().team_told_to_commander.insert(deployment_id.to_string(), fingerprint);
    }

    /// Tells each running commander about changes to its pipeline or operators, once each.
    pub fn tell_commanders_about_team_changes(&self) {
        let messages: Vec<(String, String)> = {
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            let commander_deployments: Vec<String> = state.sessions.values().filter(|session| session.position == COMMANDER).map(|session| session.deployment.clone()).collect();
            commander_deployments
                .into_iter()
                .filter_map(|deployment_id| {
                    let (folder_index, deployment) = state.find_open_deployment(&deployment_id).ok()?;
                    let team = read_team(&state.folders[folder_index].path, &deployment.pipeline);
                    let fingerprint = team_fingerprint(&team);
                    let told = state.team_told_to_commander.insert(deployment_id.clone(), fingerprint.clone());
                    let has_changed = told.is_some_and(|previous| previous != fingerprint);
                    has_changed.then(|| (deployment_id, team_change_message(&deployment.pipeline, &team)))
                })
                .collect()
        };
        messages.into_iter().for_each(|(deployment_id, text)| {
            let entry = NewEntry { kind: EntryKind::Message, mission: None, to: Some(COMMANDER.into()), text, answers: None };
            if let Err(error) = self.post_entry(&deployment_id, LEGION, entry) {
                eprintln!("{LEGION}d: {error}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(limit: u32) -> Result<Team, String> {
        Ok(Team { pipeline_text: "operators: [builder]\n".into(), copy_limits: vec![("builder".into(), limit)] })
    }

    #[test]
    fn a_new_copy_limit_changes_the_fingerprint() {
        assert_eq!(team_fingerprint(&team(2)), team_fingerprint(&team(2)));
        assert_ne!(team_fingerprint(&team(2)), team_fingerprint(&team(3)));
    }

    #[test]
    fn the_commander_hears_the_new_table_and_limits() {
        let text = team_change_message("feature", &team(3));
        assert!(text.starts_with("The feature pipeline or its operators changed."));
        assert!(text.contains("operators: [builder]"));
        assert!(text.ends_with("at once: builder 3."));
    }

    #[test]
    fn a_broken_pipeline_says_why() {
        let text = team_change_message("feature", &Err("no operator \"tester\"".into()));
        assert!(text.contains("can't be read any more") && text.ends_with("no operator \"tester\""));
    }
}
