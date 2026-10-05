//! Legion's tools inside Claude: what each is called, what it takes, who
//! gets it, and the legion2d command it becomes. The legion2 command serves
//! them; legion2d names them in each session's prompt. legion2d still checks every
//! command against the caller's role; the lists here only keep each session
//! from seeing tools it can't use.

use crate::{Command, EntryKind, LogFilter, NewEntry, COMMANDER};
use serde_json::{json, Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAudience {
    Everyone,
    CommanderOnly,
    /// Reporting on a pipeline step: only the operators doing the steps.
    OperatorsOnly,
}

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
use ToolAudience::{CommanderOnly, Everyone, OperatorsOnly};

const MISSION: ToolArgument = required("mission", Number, "The mission's number.");
const ABOUT_MISSION: ToolArgument = optional("mission", Number, "The mission it's about, if any.");

pub const TOOLS: &[Tool] = &[
    Tool { name: "missions", description: "List the run's missions and where each stands.", audience: Everyone, arguments: &[] },
    Tool { name: "mission_read", description: "Read a mission.", audience: Everyone, arguments: &[MISSION] },
    Tool { name: "sessions", description: "List who's running in the run, and what each is doing.", audience: Everyone, arguments: &[] },
    Tool {
        name: "log",
        description: "Read the run log, or part of it.",
        audience: Everyone,
        arguments: &[
            optional("mission", Number, "Only this mission's entries."),
            optional("position", Text, "Only entries from or to this position."),
        ],
    },
    Tool {
        name: "send",
        description: "Message another position in the run (commander, planner, builder-2 …).",
        audience: Everyone,
        arguments: &[required("to", Text, "The position."), required("text", Text, "The message.")],
    },
    Tool { name: "note", description: "Add a note to the run log.", audience: Everyone, arguments: &[required("text", Text, "The note."), ABOUT_MISSION] },
    Tool {
        name: "ask",
        description: "Ask the human a question you can't settle yourself. The answer comes back as a message.",
        audience: Everyone,
        arguments: &[required("question", Text, "The question, with your recommendation."), ABOUT_MISSION],
    },
    Tool {
        name: "suggest",
        description: "Suggest a mission to the human. Only the human creates missions.",
        audience: Everyone,
        arguments: &[required("text", Text, "What the mission would do, and why.")],
    },
    Tool {
        name: "handoff",
        description: "Your step is done: hand the mission to whoever the pipeline table names next. Tells the commander.",
        audience: OperatorsOnly,
        arguments: &[MISSION, required("next", Text, "The operator for the next step."), required("note", Text, "What you did.")],
    },
    Tool {
        name: "done",
        description: "The mission has reached the end of the pipeline. Tells the commander.",
        audience: OperatorsOnly,
        arguments: &[MISSION, required("summary", Text, "What happened, and anything left open.")],
    },
    Tool {
        name: "blocked",
        description: "The mission can't go on. Tells the commander.",
        audience: OperatorsOnly,
        arguments: &[MISSION, required("reason", Text, "What's blocking it, and what would unblock it.")],
    },
    Tool {
        name: "start",
        description: "Start an operator, on a mission if given. Legion refuses more copies than the operator's limit.",
        audience: CommanderOnly,
        arguments: &[required("operator", Text, "The operator, as named in the pipeline."), optional("mission", Number, "The mission to start it on.")],
    },
    Tool { name: "stop", description: "End a position's session.", audience: CommanderOnly, arguments: &[required("position", Text, "The position.")] },
    Tool { name: "screen", description: "Read a position's screen.", audience: CommanderOnly, arguments: &[required("position", Text, "The position.")] },
    Tool {
        name: "finish",
        description: "Move the base branch up to a done mission's branch. Anything that doesn't go cleanly goes to the human.",
        audience: CommanderOnly,
        arguments: &[MISSION],
    },
    Tool {
        name: "pause",
        description: "Pause a mission. Tells whoever holds it to stop at the next good point.",
        audience: CommanderOnly,
        arguments: &[MISSION, required("why", Text, "Why it's paused.")],
    },
    Tool { name: "resume", description: "Resume a paused mission.", audience: CommanderOnly, arguments: &[MISSION, required("note", Text, "What changed.")] },
    Tool {
        name: "answer",
        description: "Pass the human's answer back to whoever asked, by the question's entry number.",
        audience: CommanderOnly,
        arguments: &[required("question_entry", Number, "The question's entry number."), required("text", Text, "The human's answer.")],
    },
    Tool {
        name: "postmortem",
        description: "Record gotchas and learnings for the next commander, once the work runs out.",
        audience: CommanderOnly,
        arguments: &[required("text", Text, "The postmortem.")],
    },
];

pub fn is_offered_to(audience: ToolAudience, is_commander: bool) -> bool {
    match audience {
        ToolAudience::Everyone => true,
        ToolAudience::CommanderOnly => is_commander,
        ToolAudience::OperatorsOnly => !is_commander,
    }
}

pub fn tools_for_position(position: &str) -> Vec<&'static Tool> {
    let is_commander = position == COMMANDER;
    TOOLS.iter().filter(|tool| is_offered_to(tool.audience, is_commander)).collect()
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

fn post(run: &str, kind: EntryKind, mission: Option<u32>, to: Option<String>, text: String) -> Command {
    Command::Post { run: run.to_string(), entry: NewEntry { kind, mission, to, text, answers: None } }
}

/// The legion2d command a tool call becomes, in the caller's own run.
pub fn command_for_tool_call(name: &str, arguments: &Value, run: &str, position: &str) -> Result<Command, String> {
    let is_offered = tools_for_position(position).iter().any(|tool| tool.name == name);
    if !is_offered {
        return Err(format!("no tool {name:?} for {position}"));
    }
    let run = run.to_string();
    let command = match name {
        "missions" => Command::MissionList { run },
        "mission_read" => Command::MissionRead { run, mission: required_mission(arguments)? },
        "sessions" => Command::SessionList { run: Some(run) },
        "log" => {
            let filter = LogFilter {
                mission: mission_number(arguments, "mission")?,
                position: arguments.get("position").and_then(Value::as_str).map(str::to_string),
                ..Default::default()
            };
            Command::Log { run, filter }
        }
        "send" => post(&run, EntryKind::Message, None, Some(text_argument(arguments, "to")?), text_argument(arguments, "text")?),
        "note" => post(&run, EntryKind::Note, mission_number(arguments, "mission")?, None, text_argument(arguments, "text")?),
        "ask" => post(&run, EntryKind::Question, mission_number(arguments, "mission")?, None, text_argument(arguments, "question")?),
        "suggest" => post(&run, EntryKind::Suggestion, None, None, text_argument(arguments, "text")?),
        "handoff" => {
            let handoff_text = format!("→ {}: {}", text_argument(arguments, "next")?, text_argument(arguments, "note")?);
            post(&run, EntryKind::Handoff, Some(required_mission(arguments)?), None, handoff_text)
        }
        "done" => post(&run, EntryKind::Done, Some(required_mission(arguments)?), None, text_argument(arguments, "summary")?),
        "blocked" => post(&run, EntryKind::Blocked, Some(required_mission(arguments)?), None, text_argument(arguments, "reason")?),
        "start" => Command::SessionStart { run, operator: text_argument(arguments, "operator")?, mission: mission_number(arguments, "mission")? },
        "stop" => Command::SessionStop { run, position: text_argument(arguments, "position")? },
        "screen" => Command::Screen { run, position: text_argument(arguments, "position")? },
        "finish" => Command::MissionFinish { run, mission: required_mission(arguments)? },
        "pause" => post(&run, EntryKind::Paused, Some(required_mission(arguments)?), None, text_argument(arguments, "why")?),
        "resume" => post(&run, EntryKind::Resumed, Some(required_mission(arguments)?), None, text_argument(arguments, "note")?),
        "answer" => {
            let question = i64::try_from(number_argument(arguments, "question_entry")?).map_err(|_| "question_entry is too large")?;
            let entry = NewEntry { kind: EntryKind::Answer, mission: None, to: None, text: text_argument(arguments, "text")?, answers: Some(question) };
            Command::Post { run, entry }
        }
        "postmortem" => post(&run, EntryKind::Postmortem, None, None, text_argument(arguments, "text")?),
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
            "note": "n", "summary": "s", "reason": "r", "operator": "builder", "why": "w"
        });
        for tool in TOOLS {
            let caller = if tool.audience == ToolAudience::OperatorsOnly { "builder" } else { COMMANDER };
            assert!(command_for_tool_call(tool.name, &full_arguments, "run", caller).is_ok(), "{}", tool.name);
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
    fn a_tool_not_offered_is_refused() {
        let call = command_for_tool_call("start", &json!({ "operator": "x" }), "run", "builder");
        assert!(call.unwrap_err().contains("no tool"));
    }

    #[test]
    fn missing_or_wrong_arguments_are_named() {
        assert_eq!(command_for_tool_call("send", &json!({ "text": "hi" }), "run", "builder").unwrap_err(), "to is missing");
        assert!(command_for_tool_call("done", &json!({ "mission": "two", "summary": "s" }), "run", "builder").unwrap_err().contains("whole number"));
    }

    #[test]
    fn a_handoff_names_the_next_operator() {
        let call = command_for_tool_call("handoff", &json!({ "mission": 3, "next": "reviewer", "note": "built" }), "run", "builder").unwrap();
        let Command::Post { entry, .. } = call else { panic!() };
        assert_eq!((entry.kind, entry.mission, entry.text.as_str()), (EntryKind::Handoff, Some(3), "→ reviewer: built"));
    }

    #[test]
    fn calls_act_in_the_callers_run() {
        let call = command_for_tool_call("missions", &json!({}), "run-7", "builder").unwrap();
        assert!(matches!(call, Command::MissionList { run } if run == "run-7"));
    }

    #[test]
    fn schemas_list_required_arguments() {
        let handoff = TOOLS.iter().find(|tool| tool.name == "handoff").unwrap();
        let schema = input_schema(handoff);
        assert_eq!(schema["required"], json!(["mission", "next", "note"]));
        assert_eq!(schema["properties"]["mission"]["type"], "integer");
    }
}
