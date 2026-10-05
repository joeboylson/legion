//! What each session is told about its job when it starts.

use legion2_proto::{tools::tools_for_position, Entry, Run, COMMANDER, NAME};

use crate::{
    constants::{delivery_prefix, MAX_WORKAROUND_ATTEMPTS},
    setup::{Operator, Pipeline},
};

pub struct MissionBriefing<'a> {
    pub number: u32,
    pub body: &'a str,
    pub history: &'a [Entry],
}

/// The tools a position has, one per line, as Claude names them.
fn tool_lines(position: &str) -> String {
    tools_for_position(position)
        .iter()
        .map(|tool| format!("- mcp__{NAME}__{}: {}", tool.name, tool.description))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bulleted(lines: impl Iterator<Item = String>, when_empty: &str) -> String {
    let bullets: Vec<String> = lines.map(|line| format!("- {line}")).collect();
    if bullets.is_empty() {
        when_empty.to_string()
    } else {
        bullets.join("\n")
    }
}

pub fn commander_prompt(run: &Run, pipeline_text: &str, copy_limits: &[(String, u32)], recent_postmortems: &[Entry]) -> String {
    let limits = copy_limits.iter().map(|(operator, limit)| format!("{operator} {limit}")).collect::<Vec<_>>().join(", ");
    let postmortems = bulleted(recent_postmortems.iter().map(|entry| entry.text.clone()), "None yet.");
    let prefix = delivery_prefix();
    let tools = tool_lines(COMMANDER);
    format!(
        "You are the commander of a Legion run.
Run: {name} ({id}), in {folder}, on the {pipeline} pipeline.

You manage the work. When a mission arrives, read it and start the pipeline's first operator on it. When an operator hands off, start the operator the pipeline table names next, and stop the one that handed off: each operator gets a fresh session per mission. When an operator reports a mission done, stop that operator, then use the finish tool on the mission to move its branch onto the base branch; never report a mission done yourself. Anything sent back as blocked comes to you: unblock it if you can, otherwise ask the human with one line on what's blocking it and what would unblock it. When an operator's permission request is refused, allow it at most {MAX_WORKAROUND_ATTEMPTS} attempts at a way around it. You never create missions; only the human does.

How many copies of each operator may run at once: {limits}. Legion refuses more, so plan around it.

The pipeline table:
{pipeline_text}
Legion's tools, which already know your run and position:
{tools}

New missions, handoffs and messages arrive as prompts starting with {prefix}. When there's no work left, write a short postmortem, then wait.

Recent postmortems:
{postmortems}",
        name = run.name,
        id = run.id,
        folder = run.folder,
        pipeline = run.pipeline,
    )
}

pub fn operator_prompt(position: &str, run: &Run, operator: &Operator, pipeline: &Pipeline, pipeline_text: &str, mission: Option<MissionBriefing>) -> String {
    let mission_part = match mission {
        Some(briefing) => {
            let history_lines = briefing.history.iter().map(|entry| format!("{}: {} {}", entry.from, entry.kind.as_str(), entry.text));
            let history = bulleted(history_lines, "Nothing yet.");
            format!("Your mission is #{}:\n{}\n\nWhat's happened on it so far:\n{history}", briefing.number, briefing.body)
        }
        None => "You have no mission yet; the commander will send one.".to_string(),
    };
    let tools = tool_lines(position);
    format!(
        "You are {position}, an operator in a Legion run ({run_name}, {run_id}), on the {pipeline_name} pipeline. The first step is {first}.

Who you are:
{definition}

The pipeline table:
{pipeline_text}
{mission_part}

When your step is done, commit your work, then hand off to whoever the table names next. If your step ends the pipeline, report the mission done. Then stop and wait; the commander ends your session.

Legion's tools, which already know your run and position:
{tools}",
        run_name = run.name,
        run_id = run.id,
        pipeline_name = run.pipeline,
        first = pipeline.first,
        definition = operator.definition.trim(),
    )
}

#[cfg(test)]
mod tests {
    use legion2_proto::EntryKind;

    use super::*;
    use crate::setup::OperatorConfig;

    fn run() -> Run {
        Run { id: "abc".into(), name: "feature".into(), folder: "/repo".into(), pipeline: "feature".into(), started_ms: 0, closed_ms: None }
    }

    fn postmortem(text: &str) -> Entry {
        Entry { id: 1, run: "abc".into(), at_ms: 0, mission: None, from: "commander".into(), to: None, kind: EntryKind::Postmortem, text: text.into(), answers: None }
    }

    #[test]
    fn commander_hears_its_run_limits_and_history() {
        let prompt = commander_prompt(&run(), "operators: [builder]", &[("builder".into(), 2)], &[postmortem("tests are slow")]);
        assert!(prompt.contains("feature (abc), in /repo"));
        assert!(prompt.contains("builder 2"));
        assert!(prompt.contains("- tests are slow"));
        assert!(prompt.contains(&format!("mcp__{NAME}__finish")));
    }

    #[test]
    fn a_first_commander_has_no_postmortems() {
        assert!(commander_prompt(&run(), "", &[], &[]).contains("None yet."));
    }

    #[test]
    fn an_operator_gets_its_definition_and_mission() {
        let pipeline: Pipeline = serde_yaml::from_str("operators: [builder]\nfirst: builder\n").unwrap();
        let operator = Operator { definition: "# Builder\nBuild it.\n".into(), config: OperatorConfig::default() };
        let briefing = MissionBriefing { number: 3, body: "Make it work", history: &[] };
        let prompt = operator_prompt("builder-2", &run(), &operator, &pipeline, "", Some(briefing));
        assert!(prompt.starts_with("You are builder-2"));
        assert!(prompt.contains("Build it."));
        assert!(prompt.contains("Your mission is #3:\nMake it work"));
        assert!(prompt.contains("Nothing yet."));
        assert!(prompt.contains(&format!("mcp__{NAME}__handoff")));
        assert!(!prompt.contains(&format!("mcp__{NAME}__start")));
    }
}
