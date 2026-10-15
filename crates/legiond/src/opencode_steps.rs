//! Turns OpenCode's event stream into one line per finished step: what the
//! step did, and how long it took.

use std::{collections::HashMap, path::Path};

use serde_json::Value;

use crate::constants::{STEP_COMMAND_LIMIT, STEP_PARTS_LIMIT};

const PATH_KEYS: [&str; 3] = ["path", "filePath", "file_path"];
const EDIT_TOOLS: [&str; 5] = ["edit", "write", "patch", "apply_patch", "multiedit"];
const SHELL_TOOLS: [&str; 2] = ["shell", "bash"];

#[derive(Debug, Default)]
struct Tool {
    name: Option<String>,
    input: Value,
}

#[derive(Debug, Default)]
struct Step {
    started_ms: Option<i64>,
    /// In the order the model called them.
    tools: Vec<(String, Tool)>,
}

/// The steps that have begun and not yet ended, by the message they write.
#[derive(Debug, Default)]
pub struct Steps(HashMap<String, Step>);

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Read(String),
    Edited(String),
    Ran(String),
    Other(String),
}

fn text_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn file_name(input: &Value) -> String {
    let path = PATH_KEYS.iter().find_map(|key| text_at(input, key)).unwrap_or("a file");
    Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path).to_string()
}

fn first_line_cut(command: &str) -> String {
    let line = command.lines().next().unwrap_or_default().trim();
    match line.chars().count() > STEP_COMMAND_LIMIT {
        true => format!("{}…", line.chars().take(STEP_COMMAND_LIMIT).collect::<String>().trim_end()),
        false => line.to_string(),
    }
}

fn action_of(tool: &Tool) -> Action {
    let name = tool.name.as_deref().unwrap_or("a tool");
    match name {
        "read" => Action::Read(file_name(&tool.input)),
        _ if EDIT_TOOLS.contains(&name) => Action::Edited(file_name(&tool.input)),
        _ if SHELL_TOOLS.contains(&name) => Action::Ran(first_line_cut(text_at(&tool.input, "command").unwrap_or_default())),
        _ => Action::Other(name.to_string()),
    }
}

/// "read app.js", or "read 3 files" when it read several.
fn files_phrase(verb: &str, files: &[&String]) -> String {
    let mut distinct: Vec<&String> = Vec::new();
    files.iter().for_each(|file| {
        if !distinct.contains(file) {
            distinct.push(file)
        }
    });
    match distinct.as_slice() {
        [one] => format!("{verb} {one}"),
        many => format!("{verb} {} files", many.len()),
    }
}

/// One phrase per kind of action, in the order each first appeared, with
/// the calls of one kind collapsed together.
fn phrases(actions: &[Action]) -> Vec<String> {
    let reads: Vec<&String> = actions.iter().filter_map(|action| if let Action::Read(file) = action { Some(file) } else { None }).collect();
    let edits: Vec<&String> = actions.iter().filter_map(|action| if let Action::Edited(file) = action { Some(file) } else { None }).collect();
    let mut seen_reads = false;
    let mut seen_edits = false;
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Read(_) if !std::mem::replace(&mut seen_reads, true) => Some(files_phrase("read", &reads)),
            Action::Edited(_) if !std::mem::replace(&mut seen_edits, true) => Some(files_phrase("edited", &edits)),
            Action::Read(_) | Action::Edited(_) => None,
            Action::Ran(command) if command.is_empty() => Some("ran a command".to_string()),
            Action::Ran(command) => Some(format!("ran {command}")),
            Action::Other(name) => Some(format!("used {name}")),
        })
        .collect()
}

fn summary(actions: &[Action]) -> String {
    let all = phrases(actions);
    match all.len() {
        0 => "replied".to_string(),
        count if count <= STEP_PARTS_LIMIT => all.join(", "),
        count => format!("{}, and {} more", all[..STEP_PARTS_LIMIT].join(", "), count - STEP_PARTS_LIMIT),
    }
}

fn duration_text(ms: i64) -> String {
    match ms {
        ms if ms < 1000 => "<1s".to_string(),
        ms if ms < 60_000 => format!("{}s", ms / 1000),
        ms => format!("{}m{:02}s", ms / 60_000, ms / 1000 % 60),
    }
}

/// The line for a step that has ended.
fn line_for(step: &Step, ended_ms: i64) -> String {
    let actions: Vec<Action> = step.tools.iter().map(|(_, tool)| action_of(tool)).collect();
    match step.started_ms {
        Some(started) => format!("{} ({})", summary(&actions), duration_text((ended_ms - started).max(0))),
        None => summary(&actions),
    }
}

impl Steps {
    /// Takes in one event from the stream (its JSON); the line for the step it ended, if it did.
    pub fn follow(&mut self, event: &Value) -> Option<String> {
        let data = event.get("data")?;
        let message = text_at(data, "assistantMessageID")?;
        match text_at(event, "type")? {
            "session.step.started" => {
                self.0.entry(message.to_string()).or_default().started_ms = data.get("started").and_then(Value::as_i64);
                None
            }
            "session.tool.input.started" => {
                let tool = Tool { name: text_at(data, "name").map(str::to_string), input: Value::Null };
                self.0.entry(message.to_string()).or_default().tools.push((text_at(data, "id")?.to_string(), tool));
                None
            }
            "session.tool.called" => {
                let id = text_at(data, "id")?;
                let tools = &mut self.0.entry(message.to_string()).or_default().tools;
                let input = data.get("input").cloned().unwrap_or(Value::Null);
                match tools.iter_mut().find(|(known, _)| known == id) {
                    Some((_, tool)) => tool.input = input,
                    None => tools.push((id.to_string(), Tool { name: None, input })),
                }
                None
            }
            "session.step.ended" => {
                let step = self.0.remove(message)?;
                Some(line_for(&step, event.get("created").and_then(Value::as_i64)?))
            }
            _ => None,
        }
    }
}

/// The JSON of each event a chunk of the stream completes: the stream is
/// lines of `data: {json}` and the chunks cut anywhere, so what's left of a
/// line stays in `pending` for the next chunk.
pub fn events_in(pending: &mut String, chunk: &str) -> Vec<Value> {
    pending.push_str(chunk);
    let Some(last_break) = pending.rfind('\n') else { return Vec::new() };
    let complete: String = pending.drain(..=last_break).collect();
    complete.lines().filter_map(|line| line.strip_prefix("data:")).filter_map(|json| serde_json::from_str(json.trim()).ok()).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn started(message: &str, at: i64) -> Value {
        json!({"type": "session.step.started", "created": at + 900, "data": {"assistantMessageID": message, "started": at}})
    }

    fn tool_started(message: &str, id: &str, name: &str) -> Value {
        json!({"type": "session.tool.input.started", "data": {"assistantMessageID": message, "id": id, "name": name}})
    }

    fn tool_called(message: &str, id: &str, input: Value) -> Value {
        json!({"type": "session.tool.called", "data": {"assistantMessageID": message, "id": id, "input": input}})
    }

    fn ended(message: &str, at: i64) -> Value {
        json!({"type": "session.step.ended", "created": at, "data": {"assistantMessageID": message, "finish": "tool-calls"}})
    }

    /// Feeds a step's events and gives the line its end produced.
    fn line_after(events: Vec<Value>) -> Option<String> {
        let mut steps = Steps::default();
        events.iter().filter_map(|event| steps.follow(event)).last()
    }

    #[test]
    fn a_step_is_one_line_with_what_it_did_and_how_long() {
        let line = line_after(vec![
            started("m1", 1000),
            tool_started("m1", "t1", "read"),
            tool_called("m1", "t1", json!({"path": "/work/src/app.js"})),
            tool_started("m1", "t2", "shell"),
            tool_called("m1", "t2", json!({"command": "cargo test", "workdir": "/work"})),
            ended("m1", 13_000),
        ]);
        assert_eq!(line.as_deref(), Some("read app.js, ran cargo test (12s)"));
    }

    #[test]
    fn nothing_is_logged_until_the_step_ends() {
        let mut steps = Steps::default();
        let mid_step = [started("m1", 0), tool_started("m1", "t1", "shell"), tool_called("m1", "t1", json!({"command": "ls"}))];
        assert!(mid_step.iter().all(|event| steps.follow(event).is_none()));
        assert!(steps.follow(&json!({"type": "session.text.delta", "data": {"assistantMessageID": "m1"}})).is_none());
        assert!(steps.follow(&ended("m1", 500)).is_some());
        assert!(steps.follow(&ended("m1", 600)).is_none(), "a step ends once");
    }

    #[test]
    fn edits_to_several_files_collapse_to_a_count() {
        let line = line_after(vec![
            started("m1", 0),
            tool_started("m1", "t1", "edit"),
            tool_called("m1", "t1", json!({"filePath": "/a/one.rs"})),
            tool_started("m1", "t2", "write"),
            tool_called("m1", "t2", json!({"filePath": "/a/two.rs"})),
            tool_started("m1", "t3", "edit"),
            tool_called("m1", "t3", json!({"filePath": "/a/one.rs"})),
            ended("m1", 2500),
        ]);
        assert_eq!(line.as_deref(), Some("edited 2 files (2s)"));
    }

    #[test]
    fn many_different_things_are_counted_after_the_first_few() {
        let tools = ["read", "shell", "edit", "grep", "glob"];
        let mut events = vec![started("m1", 0)];
        tools.iter().enumerate().for_each(|(at, name)| {
            events.push(tool_started("m1", &at.to_string(), name));
            events.push(tool_called("m1", &at.to_string(), json!({"path": "/x/f.txt", "command": "ls"})));
        });
        events.push(ended("m1", 61_000));
        assert_eq!(line_after(events).as_deref(), Some("read f.txt, ran ls, edited f.txt, and 2 more (1m01s)"));
    }

    #[test]
    fn a_long_command_is_cut_to_its_first_line() {
        let line = line_after(vec![started("m1", 0), tool_started("m1", "t", "shell"), tool_called("m1", "t", json!({"command": format!("{}\nsecond line", "x".repeat(80))})), ended("m1", 100)]).unwrap();
        assert_eq!(line, format!("ran {}… (<1s)", "x".repeat(STEP_COMMAND_LIMIT)));
    }

    #[test]
    fn a_step_with_no_tools_is_a_reply_and_an_unknown_start_has_no_time() {
        assert_eq!(line_after(vec![started("m1", 0), ended("m1", 9000)]).as_deref(), Some("replied (9s)"));
        assert_eq!(line_after(vec![tool_started("m1", "t", "webfetch"), ended("m1", 9000)]).as_deref(), Some("used webfetch"));
    }

    #[test]
    fn steps_in_flight_stay_apart() {
        let mut steps = Steps::default();
        [started("a", 0), started("b", 0), tool_started("a", "t", "read"), tool_called("a", "t", json!({"path": "x.md"}))].iter().for_each(|event| {
            steps.follow(event);
        });
        assert_eq!(steps.follow(&ended("b", 1000)).as_deref(), Some("replied (1s)"));
        assert_eq!(steps.follow(&ended("a", 1000)).as_deref(), Some("read x.md (1s)"));
    }

    #[test]
    fn chunks_cut_anywhere_give_whole_events() {
        let mut pending = String::new();
        assert!(events_in(&mut pending, "data: {\"type\":\"server.conn").is_empty());
        let events = events_in(&mut pending, "ected\"}\n\n: heartbeat\ndata: {\"type\":\"b\"}\ndata: {\"ty");
        assert_eq!(events, vec![json!({"type": "server.connected"}), json!({"type": "b"})]);
        assert_eq!(events_in(&mut pending, "pe\":\"c\"}\n"), vec![json!({"type": "c"})]);
        assert!(events_in(&mut pending, "data: not json\n").is_empty());
    }
}
