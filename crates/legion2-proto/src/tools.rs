//! Legion's tools inside Claude: what each is called, what it takes, who
//! gets it, and the legion2d command it becomes. The legion2 command serves
//! them; legion2d names them in each session's prompt. legion2d still checks every
//! command against the caller's role; the lists here only keep each session
//! from seeing tools it can't use.

use crate::{role_of_position, Command, EntryKind, LogFilter, NewEntry, Role, COMMANDER};
use serde_json::{json, Map, Value};

/// Who gets a tool, by role.
pub type ToolAudience = &'static [Role];

/// Reading the deployment: everyone, the strategist too.
const READERS: ToolAudience = &[Role::Commander, Role::Operator, Role::Strategist];
/// The commander and operators, who do and route the work.
const CREW: ToolAudience = &[Role::Commander, Role::Operator];
const COMMANDER_ONLY: ToolAudience = &[Role::Commander];
/// Reporting on a pipeline step: only the operators doing the steps.
const OPERATORS_ONLY: ToolAudience = &[Role::Operator];
/// Looking at screens: the commander, and the strategist looking for slow work.
const WATCHERS: ToolAudience = &[Role::Commander, Role::Strategist];
/// Writing down what to do differently next run.
const POSTMORTEM_WRITERS: ToolAudience = &[Role::Commander, Role::Strategist];
const STRATEGIST_ONLY: ToolAudience = &[Role::Strategist];

/// Every speed-up suggestion ends with this.
pub const COMMANDER_DECIDES: &str = "This is only a suggestion: the commander has the final say.";

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub audience: ToolAudience,
    /// Each argument: name, description, and whether it's a number.
    pub arguments: &'static [ToolArgument],
}

pub struct ToolArgument {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: ArgumentKind,
    pub is_required: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    Text,
    Number,
}

const fn required(name: &'static str, kind: ArgumentKind, description: &'static str) -> ToolArgument {
    ToolArgument { name, description, kind, is_required: true }
}

const fn optional(name: &'static str, kind: ArgumentKind, description: &'static str) -> ToolArgument {
    ToolArgument { name, description, kind, is_required: false }
}

use ArgumentKind::{Number, Text};

const MISSION: ToolArgument = required("mission", Number, "The mission's number.");
const ABOUT_MISSION: ToolArgument = optional("mission", Number, "The mission it's about, if any.");

pub const TOOLS: &[Tool] = &[
    Tool { name: "missions", description: "List the deployment's missions and where each stands.", audience: READERS, arguments: &[] },
    Tool { name: "mission_read", description: "Read a mission.", audience: READERS, arguments: &[MISSION] },
    Tool { name: "sessions", description: "List who's running in the deployment, and what each is doing.", audience: READERS, arguments: &[] },
    Tool {
        name: "log",
        description: "Read the deployment log, or part of it.",
        audience: READERS,
        arguments: &[
            optional("mission", Number, "Only this mission's entries."),
            optional("position", Text, "Only entries from or to this position."),
        ],
    },
    Tool {
        name: "callouts",
        description: "Read every callout the team has made in this folder, oldest first.",
        audience: READERS,
        arguments: &[],
    },
    Tool {
        name: "callout",
        description: "Call out a one-line heads-up for everyone working here, like a kitchen calling \"behind\": a gotcha, a slow or flaky command, a file not to touch. Legion records it in the folder's callouts and tells everyone running.",
        audience: READERS,
        arguments: &[required("text", Text, "The heads-up, in one short line.")],
    },
    Tool {
        name: "toolbox",
        description: "List the team's shared tools, or read one tool's text by its file name.",
        audience: READERS,
        arguments: &[optional("tool", Text, "A tool's file name, to read it.")],
    },
    Tool {
        name: "send",
        description: "Message another position in the deployment (commander, planner, builder-2 …).",
        audience: CREW,
        arguments: &[required("to", Text, "The position."), required("text", Text, "The message.")],
    },
    Tool { name: "note", description: "Add a note to the deployment log.", audience: CREW, arguments: &[required("text", Text, "The note."), ABOUT_MISSION] },
    Tool {
        name: "ask",
        description: "Ask the human a question you can't settle yourself. The answer comes back as a message.",
        audience: CREW,
        arguments: &[required("question", Text, "The question, with your recommendation."), ABOUT_MISSION],
    },
    Tool {
        name: "flag_decision",
        description: "Tell the human about a choice you made that they may want a say in, such as a design choice or a trade-off, without stopping. Carry on with your choice; if the human disagrees, their answer comes back as a message. Use ask instead only when you can't go on without an answer.",
        audience: CREW,
        arguments: &[required("decision", Text, "What you chose, the other option, and why."), ABOUT_MISSION],
    },
    Tool {
        name: "share_tool",
        description: "Put a tool you made into the team's toolbox: a script, checker or setup step you or the next operator would otherwise do by hand again. Legion copies it to the shared tools folder, where every mission sees it at once, and tells everyone running.",
        audience: CREW,
        arguments: &[required("file", Text, "The tool's full path."), required("summary", Text, "What it does and how to run it, in one line.")],
    },
    Tool {
        name: "suggest",
        description: "Suggest a mission to the human. Only the human creates missions.",
        audience: CREW,
        arguments: &[required("text", Text, "What the mission would do, and why.")],
    },
    Tool {
        name: "handoff",
        description: "Your step is done: hand the mission to whoever the pipeline table names next. Tells the commander.",
        audience: OPERATORS_ONLY,
        arguments: &[MISSION, required("next", Text, "The operator for the next step, or several, comma-separated, when the table names a list."), required("note", Text, "What you did.")],
    },
    Tool {
        name: "part_done",
        description: "Your part of a split mission is built and committed. Legion merges it into the mission and tells the commander. Use this instead of handoff when you're on a part.",
        audience: OPERATORS_ONLY,
        arguments: &[MISSION, required("part", Number, "Your part's number."), required("note", Text, "What you did.")],
    },
    Tool {
        name: "done",
        description: "The mission has reached the end of the pipeline. Tells the commander.",
        audience: OPERATORS_ONLY,
        arguments: &[MISSION, required("summary", Text, "What happened, and anything left open.")],
    },
    Tool {
        name: "blocked",
        description: "The mission can't go on. Tells the commander.",
        audience: OPERATORS_ONLY,
        arguments: &[MISSION, required("reason", Text, "What's blocking it, and what would unblock it.")],
    },
    Tool {
        name: "start",
        description: "Start an operator, on a mission if given. Legion refuses more copies than the operator's limit.",
        audience: COMMANDER_ONLY,
        arguments: &[
            required("operator", Text, "The operator, as named in the pipeline."),
            optional("mission", Number, "The mission to start it on."),
            optional("part", Number, "The part of a split mission to start it on."),
        ],
    },
    Tool {
        name: "split",
        description: "Split a mission's current step into parts that copies of one operator can work at the same time, each in its own checkout. Only for parts that don't touch the same files. Legion merges each part back into the mission as it's done, and tells you when all are in.",
        audience: COMMANDER_ONLY,
        arguments: &[MISSION, required("parts", Text, "One part per line: what that part does, in a sentence.")],
    },
    Tool {
        name: "stop",
        description: "End a position's session. Takes the position, as the sessions tool lists it, not an operator name.",
        audience: COMMANDER_ONLY,
        arguments: &[required("position", Text, "The position to end, e.g. planner or builder-2.")],
    },
    Tool { name: "screen", description: "Read a position's screen.", audience: WATCHERS, arguments: &[required("position", Text, "The position.")] },
    Tool {
        name: "finish",
        description: "Move the base branch up to a done mission's branch. Anything that doesn't go cleanly goes to the human.",
        audience: COMMANDER_ONLY,
        arguments: &[MISSION],
    },
    Tool {
        name: "pause",
        description: "Pause a mission. Tells whoever holds it to stop at the next good point.",
        audience: COMMANDER_ONLY,
        arguments: &[MISSION, required("why", Text, "Why it's paused.")],
    },
    Tool { name: "resume", description: "Resume a paused mission.", audience: COMMANDER_ONLY, arguments: &[MISSION, required("note", Text, "What changed.")] },
    Tool {
        name: "answer",
        description: "Pass the human's answer back to whoever asked, by the question's entry number.",
        audience: COMMANDER_ONLY,
        arguments: &[required("question_entry", Number, "The question's entry number."), required("text", Text, "The human's answer.")],
    },
    Tool {
        name: "postmortem",
        description: "Record what to do differently next run: gotchas and learnings, in a few short lines. The next commander and strategist read it.",
        audience: POSTMORTEM_WRITERS,
        arguments: &[required("text", Text, "The postmortem."), ABOUT_MISSION],
    },
    Tool {
        name: "suggest_speedup",
        description: "Suggest one change to the commander that would get the work done sooner, with its pros and cons. The commander decides whether to take it.",
        audience: STRATEGIST_ONLY,
        arguments: &[
            required("suggestion", Text, "The change: what to do, to which mission or operator."),
            required("pros", Text, "What it gains, such as minutes saved or work no longer done twice."),
            required("cons", Text, "What it costs or risks."),
            ABOUT_MISSION,
        ],
    },
];

pub fn tools_for_position(position: &str) -> Vec<&'static Tool> {
    let role = role_of_position(position);
    TOOLS.iter().filter(|tool| tool.audience.contains(&role)).collect()
}

/// How a speed-up suggestion reads when it reaches the commander.
pub fn speedup_text(suggestion: &str, pros: &str, cons: &str) -> String {
    format!("Speed-up suggestion: {suggestion}\nPros: {pros}\nCons: {cons}\n{COMMANDER_DECIDES}")
}

/// The JSON Schema Claude reads for a tool's arguments.
pub fn input_schema(tool: &Tool) -> Value {
    let properties: Map<String, Value> = tool
        .arguments
        .iter()
        .map(|argument| {
            let json_type = match argument.kind {
                ArgumentKind::Text => "string",
                ArgumentKind::Number => "integer",
            };
            (argument.name.to_string(), json!({ "type": json_type, "description": argument.description }))
        })
        .collect();
    let required_names: Vec<&str> = tool.arguments.iter().filter(|argument| argument.is_required).map(|argument| argument.name).collect();
    json!({ "type": "object", "properties": properties, "required": required_names })
}

pub fn tool_listing(tool: &Tool) -> Value {
    json!({ "name": tool.name, "description": tool.description, "inputSchema": input_schema(tool) })
}

fn text_argument(arguments: &Value, name: &str) -> Result<String, String> {
    arguments.get(name).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("{name} is missing"))
}

/// A text argument holding one item per line, blank lines left out.
fn lines_argument(arguments: &Value, name: &str) -> Result<Vec<String>, String> {
    let items: Vec<String> = text_argument(arguments, name)?.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect();
    if items.is_empty() {
        return Err(format!("{name} has nothing in it"));
    }
    Ok(items)
}

fn optional_number_argument(arguments: &Value, name: &str) -> Result<Option<u64>, String> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| format!("{name} must be a whole number")),
    }
}

fn number_argument(arguments: &Value, name: &str) -> Result<u64, String> {
    optional_number_argument(arguments, name)?.ok_or_else(|| format!("{name} is missing"))
}

fn mission_number(arguments: &Value, name: &str) -> Result<Option<u32>, String> {
    optional_number_argument(arguments, name)?
        .map(|number| u32::try_from(number).map_err(|_| format!("{name} is too large")))
        .transpose()
}

fn required_mission(arguments: &Value) -> Result<u32, String> {
    mission_number(arguments, "mission")?.ok_or_else(|| "mission is missing".to_string())
}

fn post(deployment: &str, kind: EntryKind, mission: Option<u32>, to: Option<String>, text: String) -> Command {
    Command::Post { deployment: deployment.to_string(), entry: NewEntry { kind, mission, to, text, answers: None } }
}

/// The legion2d command a tool call becomes, in the caller's own deployment.
pub fn command_for_tool_call(name: &str, arguments: &Value, deployment: &str, position: &str) -> Result<Command, String> {
    let is_offered = tools_for_position(position).iter().any(|tool| tool.name == name);
    if !is_offered {
        return Err(format!("no tool {name:?} for {position}"));
    }
    let deployment = deployment.to_string();
    let command = match name {
        "missions" => Command::MissionList { deployment },
        "mission_read" => Command::MissionRead { deployment, mission: required_mission(arguments)? },
        "sessions" => Command::SessionList { deployment: Some(deployment) },
        "log" => {
            let filter = LogFilter {
                mission: mission_number(arguments, "mission")?,
                position: arguments.get("position").and_then(Value::as_str).map(str::to_string),
                ..Default::default()
            };
            Command::Log { deployment, filter }
        }
        "send" => post(&deployment, EntryKind::Message, None, Some(text_argument(arguments, "to")?), text_argument(arguments, "text")?),
        "note" => post(&deployment, EntryKind::Note, mission_number(arguments, "mission")?, None, text_argument(arguments, "text")?),
        "ask" => post(&deployment, EntryKind::Question, mission_number(arguments, "mission")?, None, text_argument(arguments, "question")?),
        "flag_decision" => post(&deployment, EntryKind::Decision, mission_number(arguments, "mission")?, None, text_argument(arguments, "decision")?),
        "share_tool" => Command::ToolShare { deployment, file: text_argument(arguments, "file")?, summary: text_argument(arguments, "summary")? },
        "suggest" => post(&deployment, EntryKind::Suggestion, None, None, text_argument(arguments, "text")?),
        "handoff" => {
            let handoff_text = format!("→ {}: {}", text_argument(arguments, "next")?, text_argument(arguments, "note")?);
            post(&deployment, EntryKind::Handoff, Some(required_mission(arguments)?), None, handoff_text)
        }
        "done" => post(&deployment, EntryKind::Done, Some(required_mission(arguments)?), None, text_argument(arguments, "summary")?),
        "blocked" => post(&deployment, EntryKind::Blocked, Some(required_mission(arguments)?), None, text_argument(arguments, "reason")?),
        "start" => Command::SessionStart {
            deployment,
            operator: text_argument(arguments, "operator")?,
            mission: mission_number(arguments, "mission")?,
            part: mission_number(arguments, "part")?,
        },
        "split" => Command::MissionSplit { deployment, mission: required_mission(arguments)?, parts: lines_argument(arguments, "parts")? },
        "part_done" => Command::PartFinish {
            deployment,
            mission: required_mission(arguments)?,
            part: mission_number(arguments, "part")?.ok_or("part is missing")?,
            note: text_argument(arguments, "note")?,
        },
        "stop" => Command::SessionStop { deployment, position: text_argument(arguments, "position")? },
        "screen" => Command::Screen { deployment, position: text_argument(arguments, "position")? },
        "finish" => Command::MissionFinish { deployment, mission: required_mission(arguments)? },
        "pause" => post(&deployment, EntryKind::Paused, Some(required_mission(arguments)?), None, text_argument(arguments, "why")?),
        "resume" => post(&deployment, EntryKind::Resumed, Some(required_mission(arguments)?), None, text_argument(arguments, "note")?),
        "answer" => {
            let question = i64::try_from(number_argument(arguments, "question_entry")?).map_err(|_| "question_entry is too large")?;
            let entry = NewEntry { kind: EntryKind::Answer, mission: None, to: None, text: text_argument(arguments, "text")?, answers: Some(question) };
            Command::Post { deployment, entry }
        }
        "postmortem" => post(&deployment, EntryKind::Postmortem, mission_number(arguments, "mission")?, None, text_argument(arguments, "text")?),
        "callout" => Command::Callout { deployment, text: text_argument(arguments, "text")? },
        "callouts" => Command::CalloutList { deployment },
        "toolbox" => Command::ToolboxRead { deployment, tool: arguments.get("tool").and_then(Value::as_str).map(str::to_string) },
        "suggest_speedup" => {
            let text = speedup_text(&text_argument(arguments, "suggestion")?, &text_argument(arguments, "pros")?, &text_argument(arguments, "cons")?);
            post(&deployment, EntryKind::Message, mission_number(arguments, "mission")?, Some(COMMANDER.into()), text)
        }
        unknown => return Err(format!("no tool {unknown:?}")),
    };
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: &[&Tool]) -> Vec<&'static str> {
        tools.iter().map(|tool| tool.name).collect()
    }

    #[test]
    fn every_tool_has_a_unique_name_and_maps_to_a_command() {
        let mut all_names = names(&TOOLS.iter().collect::<Vec<_>>());
        all_names.sort();
        all_names.dedup();
        assert_eq!(all_names.len(), TOOLS.len());
        let full_arguments = json!({
            "mission": 1, "position": "builder", "to": "reviewer", "text": "t", "question": "q", "question_entry": 2, "next": "reviewer",
            "note": "n", "summary": "s", "reason": "r", "operator": "builder", "why": "w", "file": "/tools/t.sh",
            "parts": "one\ntwo", "part": 1, "decision": "d", "suggestion": "s", "pros": "p", "cons": "c"
        });
        for tool in TOOLS {
            let caller = match tool.audience[0] {
                Role::Commander => COMMANDER,
                Role::Strategist => crate::STRATEGIST,
                Role::Operator => "builder",
            };
            assert!(command_for_tool_call(tool.name, &full_arguments, "deployment", caller).is_ok(), "{}", tool.name);
        }
    }

    #[test]
    fn operators_dont_see_commander_tools() {
        let operator_tools = names(&tools_for_position("builder"));
        assert!(operator_tools.contains(&"handoff"));
        assert!(!operator_tools.contains(&"start") && !operator_tools.contains(&"postmortem"));
        let commander_tools = names(&tools_for_position(COMMANDER));
        assert!(commander_tools.contains(&"finish"));
        assert!(!commander_tools.contains(&"done") && !commander_tools.contains(&"handoff"));
    }

    #[test]
    fn the_strategist_only_reads_and_suggests() {
        let strategist_tools = names(&tools_for_position(crate::STRATEGIST));
        assert_eq!(strategist_tools, ["missions", "mission_read", "sessions", "log", "callouts", "callout", "toolbox", "screen", "postmortem", "suggest_speedup"]);
        assert!(!names(&tools_for_position("builder")).contains(&"suggest_speedup"));
    }

    #[test]
    fn a_speedup_goes_to_the_commander_with_pros_cons_and_who_decides() {
        let arguments = json!({ "suggestion": "run the checks at once", "pros": "saves 5 minutes", "cons": "two sessions busy", "mission": 3 });
        let Command::Post { entry, .. } = command_for_tool_call("suggest_speedup", &arguments, "deployment", crate::STRATEGIST).unwrap() else { panic!() };
        assert_eq!((entry.kind, entry.to.as_deref(), entry.mission), (EntryKind::Message, Some(COMMANDER), Some(3)));
        assert_eq!(entry.text, format!("Speed-up suggestion: run the checks at once\nPros: saves 5 minutes\nCons: two sessions busy\n{COMMANDER_DECIDES}"));
    }

    #[test]
    fn everyone_reads_the_toolbox_and_reads_and_writes_callouts() {
        for position in ["builder", COMMANDER, crate::STRATEGIST] {
            let offered = names(&tools_for_position(position));
            assert!(["toolbox", "callouts", "callout"].iter().all(|tool| offered.contains(tool)), "{position}");
        }
    }

    #[test]
    fn a_tool_not_offered_is_refused() {
        let call = command_for_tool_call("start", &json!({ "operator": "x" }), "deployment", "builder");
        assert!(call.unwrap_err().contains("no tool"));
    }

    #[test]
    fn missing_or_wrong_arguments_are_named() {
        assert_eq!(command_for_tool_call("send", &json!({ "text": "hi" }), "deployment", "builder").unwrap_err(), "to is missing");
        assert!(command_for_tool_call("done", &json!({ "mission": "two", "summary": "s" }), "deployment", "builder").unwrap_err().contains("whole number"));
    }

    #[test]
    fn a_handoff_names_the_next_operator() {
        let call = command_for_tool_call("handoff", &json!({ "mission": 3, "next": "reviewer", "note": "built" }), "deployment", "builder").unwrap();
        let Command::Post { entry, .. } = call else { panic!() };
        assert_eq!((entry.kind, entry.mission, entry.text.as_str()), (EntryKind::Handoff, Some(3), "→ reviewer: built"));
    }

    #[test]
    fn calls_act_in_the_callers_deployment() {
        let call = command_for_tool_call("missions", &json!({}), "deployment-7", "builder").unwrap();
        assert!(matches!(call, Command::MissionList { deployment } if deployment == "deployment-7"));
    }

    #[test]
    fn schemas_list_required_arguments() {
        let handoff = TOOLS.iter().find(|tool| tool.name == "handoff").unwrap();
        let schema = input_schema(handoff);
        assert_eq!(schema["required"], json!(["mission", "next", "note"]));
        assert_eq!(schema["properties"]["mission"]["type"], "integer");
    }
}
