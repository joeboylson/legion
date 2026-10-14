//! What each session is told about its job when it starts.

use std::path::Path;

use legion_proto::{
    pipeline_phrase,
    tools::{command_name, tools_for_position, Tool},
    Deployment, Entry, COMMANDER, NAME, STRATEGIST,
};

use crate::{
    constants::{delivery_prefix, MAX_WORKAROUND_ATTEMPTS},
    setup::{Operator, Pipeline},
    shared_tools::{shared_tools_section, SharedTool},
    callouts::callouts_section,
    session_lifecycle::minutes_label,
    team_changes::{copy_limits_text, Team},
};

pub struct MissionBriefing<'a> {
    pub number: u32,
    pub body: &'a str,
    pub history: &'a [Entry],
}

/// Where a tool's name goes in a naming pattern, as in `mcp__legion__{tool}`.
pub const TOOL_BLANK: &str = "{tool}";

/// Said after the tool list when tools are commands.
const ARGUMENTS_AS_FLAGS: &str = "Each argument can also be given as `--name value`.";

/// How a session's harness names Legion's tools: by a pattern with the
/// tool's name in the blank, or as `legion` shell commands.
pub enum ToolNaming {
    Pattern(String),
    Command,
}

impl ToolNaming {
    /// Claude Code's names for the legion MCP server's tools.
    pub fn claude() -> Self {
        Self::Pattern(format!("mcp__{NAME}__{TOOL_BLANK}"))
    }

    /// A tool named in a sentence. MCP harnesses keep the bare name, as
    /// Claude's text always has; the list pairs it with its full name.
    pub fn name(&self, tool: &str) -> String {
        match self {
            Self::Pattern(_) => tool.to_string(),
            Self::Command => format!("`{}`", command_name(tool)),
        }
    }

    /// "the send tool", or "the `legion send` command".
    pub fn mention(&self, tool: &str) -> String {
        match self {
            Self::Pattern(_) => format!("the {tool} tool"),
            Self::Command => format!("the `{}` command", command_name(tool)),
        }
    }

    fn listed(&self, tool: &Tool) -> String {
        match self {
            Self::Pattern(pattern) => pattern.replace(TOOL_BLANK, tool.name),
            Self::Command => command_usage(tool),
        }
    }
}

/// `legion note <text> [--mission <mission>]`: required arguments in order,
/// then optional ones as flags, in the order the command takes them.
fn command_usage(tool: &Tool) -> String {
    let required = tool.arguments.iter().filter(|argument| argument.is_required).map(|argument| format!("<{}>", argument.name));
    let optional = tool.arguments.iter().filter(|argument| !argument.is_required).map(|argument| format!("[--{0} <{0}>]", argument.name));
    std::iter::once(command_name(tool.name)).chain(required).chain(optional).collect::<Vec<_>>().join(" ")
}

/// The tools a position has, one per line, as the harness names them.
fn tool_lines(position: &str, naming: &ToolNaming) -> String {
    let lines = tools_for_position(position)
        .iter()
        .map(|tool| format!("- {}: {}", naming.listed(tool), tool.description))
        .collect::<Vec<_>>()
        .join("\n");
    match naming {
        ToolNaming::Pattern(_) => lines,
        ToolNaming::Command => format!("{lines}\n{ARGUMENTS_AS_FLAGS}"),
    }
}

fn bulleted(lines: impl Iterator<Item = String>, when_empty: &str) -> String {
    let bullets: Vec<String> = lines.map(|line| format!("- {line}")).collect();
    if bullets.is_empty() {
        when_empty.to_string()
    } else {
        bullets.join("\n")
    }
}

pub fn commander_prompt(deployment: &Deployment, pipeline_text: &str, copy_limits: &[(String, u32)], recent_postmortems: &[Entry], callouts: &[String], naming: &ToolNaming) -> String {
    let limits = copy_limits_text(copy_limits);
    let postmortems = bulleted(recent_postmortems.iter().map(|entry| entry.text.clone()), "None yet.");
    let prefix = delivery_prefix();
    let tools = tool_lines(COMMANDER, naming);
    let mention = |tool| naming.mention(tool);
    // "the `legion start` command's part argument" reads oddly; Claude's text stays as it was.
    let start_part = match naming {
        ToolNaming::Pattern(_) => format!("{}'s part argument", mention("start")),
        ToolNaming::Command => format!("the `--part` argument of {}", mention("start")),
    };
    format!(
        "You are the commander of a Legion deployment.
Deployment: {name} ({id}), in {folder}, {pipeline}.

You manage the work. When a mission arrives, read it and start the operator the pipeline table names first on it; if it names none, start the one best suited to the mission. When an operator hands off, pass the work to the operator the pipeline table names next: if {sessions} lists it on this mission already, send it the handoff with {send}; otherwise start it on the mission, and send nothing more: it reads the handoff in the mission's history as it starts. Don't stop the one that handed off: it waits, idle, and keeps what it learned in case the work comes back to it. When the table names a list of operators, pass the work to all of them at once. Wait until every one of them has handed off before passing it on: if they all pass it, move it to the next step; if any sends it back, gather all their findings and send them together in one message. Once that's fixed, only the ones who found something check it again; the others' passes stand, unless the fix changed what they checked. When an operator reports a mission done, use {finish} on the mission to move its branch onto the base branch (in a folder without git, finishing just closes it); Legion then ends that mission's sessions. Never report a mission done yourself. Anything sent back as blocked comes to you: unblock it if you can, otherwise ask the admin with one line on what's blocking it and what would unblock it. Operators send you their questions about a mission: answer them if you can, otherwise ask the admin with {ask} and pass the answer on with {send_name}. A choice the admin may want a say in, that doesn't need to stop the work, goes to them with {flag_decision}. Start an operator on a mission with {start} and the mission's number; don't message the mission to an operator started without one. No one reads your screen: anything for the admin goes through {ask}. Ask the admin only what you and the operators can't settle about the code or the product. Never ask them to merge a pull request, close or finish a mission, or confirm status, and don't report progress as a question: finish done missions yourself, and put which pull requests are ready, and in what order to merge them, in a note. When an operator's permission request is refused, allow it at most {MAX_WORKAROUND_ATTEMPTS} attempts at a way around it. You never create missions; only the admin does.

A strategist may send you speed-up suggestions, each with its pros and cons. They're only suggestions: you have the final say. Take or turn down each one, and add a note saying which and why in one line, so it isn't suggested again.

How many copies of each operator may run at once: {limits}. A mission whose current step is big and breaks into pieces that don't touch the same files can be split with {split}, one part per line; then start a copy of that step's operator on each part with {start_part}, up to its limit. Legion merges each part into the mission as it's done and tells you when all are in; then pass the mission to the next step as one. Don't split small missions, or steps whose pieces share files.

When every copy of an operator is taken and one is idle, starting it again ends the idle copy to make room; Legion refuses only when every copy is busy. So start waiting missions as soon as there's room, instead of waiting for earlier missions to finish.

Other teams, on this machine or others linked by a channel, can message you, and you them. As you start, tell them what your team is and can do in a sentence or two with {channel_describe}. {channel_deployments} lists the teams you can reach; {channel_send} messages one's commander. Their messages arrive as prompts like any other, and say how to reply. Message another team only when the admin or a mission asks you to, or to answer one that wrote to you.

The pipeline table:
{pipeline_text}
Legion's tools, which already know your deployment and position:
{tools}

New missions, handoffs and messages arrive as prompts starting with {prefix}. When there's no work left, write a short postmortem, then wait.

Recent postmortems, from this and earlier runs:
{postmortems}

{callouts}",
        callouts = callouts_section(callouts, naming),
        sessions = mention("sessions"),
        send = mention("send"),
        send_name = naming.name("send"),
        finish = mention("finish"),
        ask = mention("ask"),
        flag_decision = mention("flag_decision"),
        start = mention("start"),
        split = mention("split"),
        channel_describe = naming.name("channel_describe"),
        channel_deployments = naming.name("channel_deployments"),
        channel_send = naming.name("channel_send"),
        name = deployment.name,
        id = deployment.id,
        folder = deployment.folder,
        pipeline = pipeline_phrase(&deployment.pipeline),
    )
}

pub struct StrategistBriefing<'a> {
    pub team: &'a Team,
    pub every_minutes: u32,
    pub recent_postmortems: &'a [Entry],
    pub callouts: &'a [String],
    pub naming: &'a ToolNaming,
}

pub fn strategist_prompt(deployment: &Deployment, briefing: StrategistBriefing) -> String {
    let StrategistBriefing { team, every_minutes, recent_postmortems, callouts, naming } = briefing;
    let tools = tool_lines(STRATEGIST, naming);
    let postmortems = bulleted(recent_postmortems.iter().map(|entry| entry.text.clone()), "None yet.");
    format!(
        "You are the strategist of a Legion deployment ({name}, {id}), {pipeline}. Your one job is getting the work done sooner. You never command: you suggest, and the commander has the final say.

You run on your own: every {interval} Legion itself wakes you for a speed check; no one needs to ask. Then look at the deployment as it is now: the missions, who's running and what each is doing, the log since your last check, and the screen of anyone who has been busy a long while. Look for:
- work that runs one after another but could run at the same time: missions waiting while an operator has room, or a big step that could be split into parts;
- checks done twice, or an operator redoing a step that already passed;
- operators waiting on something that's already done, or polling for what another operator will report;
- long turns: a long self-check, or an operator stuck;
- operators idle while missions wait.

For each speed-up worth making, send one suggestion with {suggest_speedup}: the change, its pros and its cons, and what you saw that led to it. Three rules for every suggestion:
- Never suggest skipping, shortening or putting off a check that the pipeline table or the mission's text asks for. Speed comes from running things side by side and cutting waits, not from checking less.
- Check the facts it rests on yourself first (the PR's files, the log entry, the operator's screen). A suggestion built on a wrong fact costs the commander time to disprove.
- Only suggest what saves several minutes, and say roughly how many. Smaller gains aren't worth the commander's attention.
Don't suggest what the commander turned down already (its notes in the log say why) unless something has changed. If nothing is worth changing, send nothing and wait for the next check. Never message anyone but the commander, and never do the work yourself.

When Legion tells you a mission is finished, write its postmortem with {postmortem} (with the mission's number): what slowed it down, what sped it up, and what to do differently next run, in a few short lines. Read its log first. The next commander and strategist read it.

The pipeline table:
{pipeline_text}
How many copies of each operator may run at once: {limits}.

Legion's tools, which already know your deployment and position:
{tools}

Recent postmortems, from this and earlier runs:
{postmortems}

{callouts}",
        callouts = callouts_section(callouts, naming),
        suggest_speedup = naming.mention("suggest_speedup"),
        postmortem = naming.mention("postmortem"),
        name = deployment.name,
        id = deployment.id,
        pipeline = pipeline_phrase(&deployment.pipeline),
        pipeline_text = team.pipeline_text,
        limits = copy_limits_text(&team.copy_limits),
        interval = minutes_label(u64::from(every_minutes)),
    )
}

pub fn strategist_check_request() -> String {
    "Speed check: look at the deployment as it is now, and suggest anything that would get the work done sooner. If nothing would, send nothing.".into()
}

pub struct OperatorBriefing<'a> {
    pub operator: &'a Operator,
    pub pipeline: &'a Pipeline,
    pub pipeline_text: &'a str,
    pub shared_tools: &'a [SharedTool],
    pub shared_tools_folder: &'a Path,
    /// The branch a mission's work finishes onto, when the folder is in git.
    pub base_branch: Option<&'a str>,
    pub callouts: &'a [String],
    pub naming: &'a ToolNaming,
}

pub fn operator_prompt(position: &str, deployment: &Deployment, briefing: OperatorBriefing, mission: Option<MissionBriefing>) -> String {
    let OperatorBriefing { operator, pipeline, pipeline_text, shared_tools, shared_tools_folder, base_branch, callouts, naming } = briefing;
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
    let tools = tool_lines(position, naming);
    format!(
        "You are {position}, an operator in a Legion deployment ({deployment_name}, {deployment_id}), {pipeline_name}.{first_step}

Who you are:
{definition}

The pipeline table:
{pipeline_text}
{mission_part}

No one reads your screen. Anything you need answered, and anything others need to know, goes through Legion's tools: send questions about the mission to the commander with {send}, then stop and wait for the answer. Never end a turn with a question only in your reply. When you make a choice about the code or the product the admin may want a say in, such as a design choice or a trade-off, flag it with {flag_decision} and carry on; use {ask} only when you can't go on without an answer. Don't flag or ask about the mission itself, merging, or status: those go to the commander.

When your step is done, commit your work, then hand off to whoever the table names next.{catch_up} When the table names a list, hand off to all of them in one handoff. Others in such a list may be working on the mission at the same time as you, in the same folder: commit only the files you changed, by name, never everything. Other sessions run on this machine too: stop only processes you started, by their process ID, never with `pkill -f` or `killall` and a name. If your step ends the pipeline, report the mission done. Then stop and wait: the work may come back to you, and Legion ends your session once the mission is finished.

Legion's tools, which already know your deployment and position:
{tools}

{shared}

{callouts}",
        shared = shared_tools_section(shared_tools, shared_tools_folder, naming),
        callouts = callouts_section(callouts, naming),
        send = naming.mention("send"),
        flag_decision = naming.mention("flag_decision"),
        ask = naming.name("ask"),
        deployment_name = deployment.name,
        deployment_id = deployment.id,
        pipeline_name = pipeline_phrase(&deployment.pipeline),
        first_step = pipeline.first.as_ref().map(|first| format!(" The first step is {first}.")).unwrap_or_default(),
        definition = operator.definition.trim(),
    )
}

#[cfg(test)]
mod tests {
    use legion_proto::EntryKind;

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
        let prompt = commander_prompt(&deployment(), "operators: [builder]", &[("builder".into(), 2)], &[postmortem("tests are slow")], &["2026-10-07 tester: port 3000 is taken".into()], &ToolNaming::claude());
        assert!(prompt.contains("feature (abc), in /repo"));
        assert!(prompt.contains("builder 2"));
        assert!(prompt.contains("- tests are slow"));
        assert!(prompt.contains("- 2026-10-07 tester: port 3000 is taken"));
        assert!(prompt.contains(&format!("mcp__{NAME}__finish")));
    }

    #[test]
    fn commander_keeps_operators_idle_between_handoffs() {
        let prompt = commander_prompt(&deployment(), "", &[], &[], &[], &ToolNaming::claude());
        assert!(prompt.contains("send it the handoff with the send tool"));
        assert!(prompt.contains("Don't stop the one that handed off"));
        assert!(prompt.contains("send nothing more"));
        assert!(prompt.contains("pass the work to all of them at once"));
        assert!(prompt.contains("split tool"));
        assert!(prompt.contains("only the ones who found something check it again"));
    }

    #[test]
    fn the_commander_has_the_final_say_on_speed_ups() {
        assert!(commander_prompt(&deployment(), "", &[], &[], &[], &ToolNaming::claude()).contains("They're only suggestions: you have the final say."));
    }

    #[test]
    fn the_strategist_suggests_and_never_commands() {
        let team = Team { pipeline_text: "operators: [builder]\n".into(), copy_limits: vec![("builder".into(), 2)] };
        let postmortems = [postmortem("the planner was the bottleneck")];
        let prompt = strategist_prompt(&deployment(), StrategistBriefing { team: &team, every_minutes: 1, recent_postmortems: &postmortems, callouts: &[], naming: &ToolNaming::claude() });
        assert!(prompt.contains("- the planner was the bottleneck"));
        assert!(prompt.contains("write its postmortem with the postmortem tool"));
        assert!(prompt.contains("You never command"));
        assert!(prompt.contains("Never suggest skipping, shortening or putting off a check"));
        assert!(prompt.contains("Check the facts it rests on yourself first"));
        assert!(prompt.contains("Only suggest what saves several minutes"));
        assert!(prompt.contains("every 1 minute Legion itself wakes you"));
        assert!(prompt.contains("builder 2"));
        assert!(prompt.contains(&format!("mcp__{NAME}__suggest_speedup")));
        assert!(!prompt.contains(&format!("mcp__{NAME}__start")));
    }

    #[test]
    fn a_first_commander_has_no_postmortems() {
        assert!(commander_prompt(&deployment(), "", &[], &[], &[], &ToolNaming::claude()).contains("None yet."));
    }

    #[test]
    fn an_operator_gets_its_definition_and_mission() {
        let pipeline: Pipeline = serde_yaml::from_str("operators: [builder]\nfirst: builder\n").unwrap();
        let operator = Operator { definition: "# Builder\nBuild it.\n".into(), config: OperatorConfig::default() };
        let briefing = MissionBriefing { number: 3, body: "Make it work", history: &[] };
        let shared = [SharedTool { name: "browser-test.sh".into(), summary: "Runs tests.html.".into() }];
        let operator_briefing = OperatorBriefing { operator: &operator, pipeline: &pipeline, pipeline_text: "", shared_tools: &shared, shared_tools_folder: Path::new("/repo/.legion/tools"), base_branch: Some("main"), callouts: &[], naming: &ToolNaming::claude() };
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

    /// Each prompt built from the same fixed inputs as the files in testdata,
    /// which hold Claude Code's text from before tools could be named otherwise.
    fn fixed_commander(naming: &ToolNaming) -> String {
        commander_prompt(&deployment(), "operators: [builder]\n", &[("builder".into(), 2)], &[postmortem("tests are slow")], &["2026-10-07 tester: port 3000 is taken".into()], naming)
    }

    fn fixed_strategist(naming: &ToolNaming) -> String {
        let team = Team { pipeline_text: "operators: [builder]\n".into(), copy_limits: vec![("builder".into(), 2)] };
        let postmortems = [postmortem("tests are slow")];
        strategist_prompt(&deployment(), StrategistBriefing { team: &team, every_minutes: 5, recent_postmortems: &postmortems, callouts: &["2026-10-07 tester: port 3000 is taken".into()], naming })
    }

    fn fixed_operator(naming: &ToolNaming) -> String {
        let pipeline: Pipeline = serde_yaml::from_str("operators: [builder]\nfirst: builder\n").unwrap();
        let operator = Operator { definition: "# Builder\nBuild it.\n".into(), config: OperatorConfig::default() };
        let shared = [SharedTool { name: "browser-test.sh".into(), summary: "Runs tests.html.".into() }];
        let history = [postmortem("tests are slow")];
        let briefing = OperatorBriefing {
            operator: &operator,
            pipeline: &pipeline,
            pipeline_text: "operators: [builder]\n",
            shared_tools: &shared,
            shared_tools_folder: Path::new("/repo/.legion/tools"),
            base_branch: Some("main"),
            callouts: &["2026-10-07 tester: port 3000 is taken".into()],
            naming,
        };
        operator_prompt("builder-2", &deployment(), briefing, Some(MissionBriefing { number: 3, body: "Make it work", history: &history }))
    }

    #[test]
    fn claude_codes_text_is_unchanged() {
        let claude = ToolNaming::claude();
        assert_eq!(fixed_commander(&claude), include_str!("testdata/commander_prompt_claude.txt"));
        assert_eq!(fixed_strategist(&claude), include_str!("testdata/strategist_prompt_claude.txt"));
        assert_eq!(fixed_operator(&claude), include_str!("testdata/operator_prompt_claude.txt"));
    }

    #[test]
    fn a_pattern_names_each_tool_in_its_blank() {
        let naming = ToolNaming::Pattern("legion_{tool}".into());
        let operator = fixed_operator(&naming);
        assert!(operator.contains("- legion_handoff: Your step is done"));
        assert!(operator.contains("- legion_flag_decision: Tell the admin"));
        assert!(!operator.contains("mcp__"));
        assert!(!operator.contains(ARGUMENTS_AS_FLAGS));
        assert!(operator.contains("with the send tool"));
        assert!(fixed_commander(&naming).contains("- legion_finish: "));
        assert!(fixed_strategist(&naming).contains("- legion_suggest_speedup: "));
    }

    #[test]
    fn commands_list_their_arguments_in_order() {
        let operator = fixed_operator(&ToolNaming::Command);
        assert!(operator.contains("- legion handoff <mission> <next> <note>: Your step is done"));
        assert!(operator.contains("- legion flag-decision <decision> [--mission <mission>]: "));
        assert!(operator.contains("- legion log [--mission <mission>] [--position <position>]: "));
        assert!(operator.contains("- legion missions: "));
        assert!(operator.contains(&format!("can't go on. Tells the commander.\n{ARGUMENTS_AS_FLAGS}\n\n")));
        assert!(fixed_commander(&ToolNaming::Command).contains("- legion answer <question_entry> <text>: "));
    }

    #[test]
    fn commands_are_named_the_same_way_in_the_text() {
        let operator = fixed_operator(&ToolNaming::Command);
        assert!(operator.contains("to the commander with the `legion send` command,"));
        assert!(operator.contains("flag it with the `legion flag-decision` command"));
        assert!(operator.contains("use `legion ask` only"));
        assert!(operator.contains("call it out with the `legion callout` command"));
        assert!(operator.contains("read any of them with the `legion toolbox` command"));
        assert!(operator.contains("share it with the `legion share-tool` command"));
        assert!(!operator.contains("the send tool"));
        assert!(!operator.contains("mcp__"));
        let commander = fixed_commander(&ToolNaming::Command);
        assert!(commander.contains("use the `legion finish` command on the mission"));
        assert!(commander.contains("with the `--part` argument of the `legion start` command"));
        assert!(commander.contains("with `legion channel-describe`."));
        assert!(!commander.contains("the send tool"));
        assert!(!commander.contains("the start tool"));
        assert!(fixed_strategist(&ToolNaming::Command).contains("with the `legion suggest-speedup` command"));
    }

    #[test]
    fn every_command_named_is_a_real_tool() {
        let prompts = [fixed_commander(&ToolNaming::Command), fixed_strategist(&ToolNaming::Command), fixed_operator(&ToolNaming::Command)];
        // Commands start a list line ("- legion send <to> …") or sit in backticks ("`legion send`").
        let command_prefix = format!("{NAME} ");
        let named: Vec<String> = prompts
            .iter()
            .flat_map(|prompt| prompt.split(['`', '\n']).map(str::to_string).collect::<Vec<_>>())
            .filter_map(|piece| piece.trim_start_matches("- ").strip_prefix(&command_prefix).map(|rest| rest.split([' ', ':']).next().unwrap().replace('-', "_")))
            .collect();
        assert!(named.len() > 20);
        for name in named {
            assert!(legion_proto::tools::TOOLS.iter().any(|tool| tool.name == name), "{name} isn't a tool");
        }
    }

}
