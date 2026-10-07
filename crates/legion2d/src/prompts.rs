//! What each session is told about its job when it starts.

use std::path::Path;

use legion2_proto::{tools::tools_for_position, Entry, Deployment, COMMANDER, NAME};

use crate::{
    constants::{delivery_prefix, MAX_WORKAROUND_ATTEMPTS},
    setup::{Operator, Pipeline},
    shared_tools::{shared_tools_section, SharedTool},
    team_changes::copy_limits_text,
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

pub fn commander_prompt(deployment: &Deployment, pipeline_text: &str, copy_limits: &[(String, u32)], recent_postmortems: &[Entry]) -> String {
    let limits = copy_limits_text(copy_limits);
    let postmortems = bulleted(recent_postmortems.iter().map(|entry| entry.text.clone()), "None yet.");
    let prefix = delivery_prefix();
    let tools = tool_lines(COMMANDER);
    format!(
        "You are the commander of a Legion deployment.
Deployment: {name} ({id}), in {folder}, on the {pipeline} pipeline.

You manage the work. When a mission arrives, read it and start the pipeline's first operator on it. When an operator hands off, pass the work to the operator the pipeline table names next: if the sessions tool lists it on this mission already, send it the handoff with the send tool; otherwise start it on the mission, and send nothing more: it reads the handoff in the mission's history as it starts. Don't stop the one that handed off: it waits, idle, and keeps what it learned in case the work comes back to it. When the table names a list of operators, pass the work to all of them at once. Wait until every one of them has handed off before passing it on: if they all pass it, move it to the next step; if any sends it back, gather all their findings and send them together in one message. Once that's fixed, only the ones who found something check it again; the others' passes stand, unless the fix changed what they checked. When an operator reports a mission done, use the finish tool on the mission to move its branch onto the base branch; Legion then ends that mission's sessions. Never report a mission done yourself. Anything sent back as blocked comes to you: unblock it if you can, otherwise ask the human with one line on what's blocking it and what would unblock it. Operators send you their questions about a mission: answer them if you can, otherwise ask the human with the ask tool and pass the answer on with send. A choice the human may want a say in, that doesn't need to stop the work, goes to them with the flag_decision tool. Start an operator on a mission with the start tool and the mission's number; don't message the mission to an operator started without one. No one reads your screen: anything for the human goes through the ask tool. When an operator's permission request is refused, allow it at most {MAX_WORKAROUND_ATTEMPTS} attempts at a way around it. You never create missions; only the human does.

How many copies of each operator may run at once: {limits}. A mission whose current step is big and breaks into pieces that don't touch the same files can be split with the split tool, one part per line; then start a copy of that step's operator on each part with the start tool's part argument, up to its limit. Legion merges each part into the mission as it's done and tells you when all are in; then pass the mission to the next step as one. Don't split small missions, or steps whose pieces share files.

When every copy of an operator is taken and one is idle, starting it again ends the idle copy to make room; Legion refuses only when every copy is busy. So start waiting missions as soon as there's room, instead of waiting for earlier missions to finish.

The pipeline table:
{pipeline_text}
Legion's tools, which already know your deployment and position:
{tools}

New missions, handoffs and messages arrive as prompts starting with {prefix}. When there's no work left, write a short postmortem, then wait.

Recent postmortems:
{postmortems}",
        name = deployment.name,
        id = deployment.id,
        folder = deployment.folder,
        pipeline = deployment.pipeline,
    )
}

pub struct OperatorBriefing<'a> {
    pub operator: &'a Operator,
    pub pipeline: &'a Pipeline,
    pub pipeline_text: &'a str,
    pub shared_tools: &'a [SharedTool],
    pub shared_tools_folder: &'a Path,
    /// The branch a mission's work finishes onto, when the folder is in git.
    pub base_branch: Option<&'a str>,
}

pub fn operator_prompt(position: &str, deployment: &Deployment, briefing: OperatorBriefing, mission: Option<MissionBriefing>) -> String {
    let OperatorBriefing { operator, pipeline, pipeline_text, shared_tools, shared_tools_folder, base_branch } = briefing;
    // Other missions finish onto the base meanwhile; catching up before a
    // handoff fixes clashes while this one's work is fresh, not at the finish.
    let catch_up = base_branch
        .map(|base| format!(" If you committed changes, first bring your branch up to date with {base}: run `git rebase {base}`, fix any clash, and check your work again."))
        .unwrap_or_default();
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
        "You are {position}, an operator in a Legion deployment ({deployment_name}, {deployment_id}), on the {pipeline_name} pipeline. The first step is {first}.

Who you are:
{definition}

The pipeline table:
{pipeline_text}
{mission_part}

No one reads your screen. Anything you need answered, and anything others need to know, goes through Legion's tools: send questions about the mission to the commander with the send tool, then stop and wait for the answer. Never end a turn with a question only in your reply. When you make a choice the human may want a say in, such as a design choice or a trade-off, flag it with the flag_decision tool and carry on; use ask only when you can't go on without an answer.

When your step is done, commit your work, then hand off to whoever the table names next.{catch_up} When the table names a list, hand off to all of them in one handoff. Others in such a list may be working on the mission at the same time as you, in the same folder: commit only the files you changed, by name, never everything. Other sessions run on this machine too: stop only processes you started, by their process ID, never with `pkill -f` or `killall` and a name. If your step ends the pipeline, report the mission done. Then stop and wait: the work may come back to you, and Legion ends your session once the mission is finished.

Legion's tools, which already know your deployment and position:
{tools}

{shared}",
        shared = shared_tools_section(shared_tools, shared_tools_folder),
        deployment_name = deployment.name,
        deployment_id = deployment.id,
        pipeline_name = deployment.pipeline,
        first = pipeline.first,
        definition = operator.definition.trim(),
    )
}

#[cfg(test)]
mod tests {
    use legion2_proto::EntryKind;

    use super::*;
    use crate::setup::OperatorConfig;

    fn deployment() -> Deployment {
        Deployment { id: "abc".into(), name: "feature".into(), folder: "/repo".into(), pipeline: "feature".into(), started_ms: 0, closed_ms: None }
    }

    fn postmortem(text: &str) -> Entry {
        Entry { id: 1, deployment: "abc".into(), at_ms: 0, mission: None, from: "commander".into(), to: None, kind: EntryKind::Postmortem, text: text.into(), answers: None }
    }

    #[test]
    fn commander_hears_its_deployment_limits_and_history() {
        let prompt = commander_prompt(&deployment(), "operators: [builder]", &[("builder".into(), 2)], &[postmortem("tests are slow")]);
        assert!(prompt.contains("feature (abc), in /repo"));
        assert!(prompt.contains("builder 2"));
        assert!(prompt.contains("- tests are slow"));
        assert!(prompt.contains(&format!("mcp__{NAME}__finish")));
    }

    #[test]
    fn commander_keeps_operators_idle_between_handoffs() {
        let prompt = commander_prompt(&deployment(), "", &[], &[]);
        assert!(prompt.contains("send it the handoff with the send tool"));
        assert!(prompt.contains("Don't stop the one that handed off"));
        assert!(prompt.contains("send nothing more"));
        assert!(prompt.contains("pass the work to all of them at once"));
        assert!(prompt.contains("split tool"));
        assert!(prompt.contains("only the ones who found something check it again"));
    }

    #[test]
    fn a_first_commander_has_no_postmortems() {
        assert!(commander_prompt(&deployment(), "", &[], &[]).contains("None yet."));
    }

    #[test]
    fn an_operator_gets_its_definition_and_mission() {
        let pipeline: Pipeline = serde_yaml::from_str("operators: [builder]\nfirst: builder\n").unwrap();
        let operator = Operator { definition: "# Builder\nBuild it.\n".into(), config: OperatorConfig::default() };
        let briefing = MissionBriefing { number: 3, body: "Make it work", history: &[] };
        let shared = [SharedTool { name: "browser-test.sh".into(), summary: "Runs tests.html.".into() }];
        let operator_briefing = OperatorBriefing { operator: &operator, pipeline: &pipeline, pipeline_text: "", shared_tools: &shared, shared_tools_folder: Path::new("/repo/.legion2/tools"), base_branch: Some("main") };
        let prompt = operator_prompt("builder-2", &deployment(), operator_briefing, Some(briefing));
        assert!(prompt.starts_with("You are builder-2"));
        assert!(prompt.contains("Build it."));
        assert!(prompt.contains("Your mission is #3:\nMake it work"));
        assert!(prompt.contains("Nothing yet."));
        assert!(prompt.contains(&format!("mcp__{NAME}__handoff")));
        assert!(!prompt.contains(&format!("mcp__{NAME}__start")));
        assert!(prompt.contains("No one reads your screen"));
        assert!(prompt.contains("browser-test.sh: Runs tests.html."));
        assert!(prompt.contains("run `git rebase main`"));
    }
}
