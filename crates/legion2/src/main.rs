//! The legion2 command. Everything it does goes through legion2d.
//!
//! Inside a session legion2d started, the run and position come from the
//! environment; outside one, say which run with --run.

mod arguments;
mod mcp_server;
mod plan;
mod reply_format;
mod time_span;

use std::io::Read;

use clap::Parser;
use legion2_client::Client;
use legion2_proto::{Command, Entry, Event, LogFilter, Reply, ServerMessage, NAME};

use crate::{
    arguments::{Action, CommandLine, ExportFormat},
    plan::{plan_action, Plan},
    reply_format::{entry_line, entry_markdown, reply_text},
};

fn read_mission_body(action: &Action) -> Result<Option<String>, String> {
    let Action::New { body, file, .. } = action else { return Ok(None) };
    match (body, file) {
        (Some(text), _) => Ok(Some(text.clone())),
        (None, Some(path)) => std::fs::read_to_string(path).map(Some).map_err(|error| format!("{path}: {error}")),
        (None, None) => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).map_err(|error| error.to_string())?;
            Ok(Some(text))
        }
    }
}

fn print_reply(reply: &Reply) {
    let text = reply_text(reply);
    if !text.is_empty() {
        println!("{text}");
    }
}

/// Whether a new entry belongs in a followed log.
fn passes_follow_filter(entry: &Entry, run_id: &str, filter: &LogFilter) -> bool {
    let is_same_run = entry.run == run_id;
    let is_on_mission = filter.mission.is_none_or(|mission| entry.mission == Some(mission));
    let involves_position = filter.position.as_ref().is_none_or(|position| &entry.from == position || entry.to.as_ref() == Some(position));
    let is_wanted_kind = filter.kinds.as_ref().is_none_or(|kinds| kinds.contains(&entry.kind));
    is_same_run && is_on_mission && involves_position && is_wanted_kind
}

async fn follow_log(client: &mut Client, run: String, filter: LogFilter) -> Result<(), String> {
    let reply = client.ask(Command::Log { run: run.clone(), filter: filter.clone() }).await?;
    print_reply(&reply);
    // Events carry the run's ID, and the run may have been named.
    let run_id = match client.ask(Command::RunList { folder: None }).await? {
        Reply::Runs { runs } => runs.into_iter().find(|known| known.id == run || known.name == run).map(|known| known.id).unwrap_or(run),
        _ => run,
    };
    client.ask(Command::Watch).await?;
    loop {
        let ServerMessage::Event { event: Event::Entry { entry } } = client.next_message().await? else { continue };
        if passes_follow_filter(&entry, &run_id, &filter) {
            println!("{}", entry_line(&entry));
        }
    }
}

async fn export_log(client: &mut Client, run: String, filter: LogFilter, format: ExportFormat, output: Option<String>) -> Result<(), String> {
    let Reply::Entries { entries } = client.ask(Command::Log { run, filter }).await? else { return Err("unexpected reply".into()) };
    let exported: String = match format {
        ExportFormat::Jsonl => entries.iter().map(|entry| serde_json::to_string(entry).map(|line| line + "\n")).collect::<Result<_, _>>().map_err(|error| error.to_string())?,
        ExportFormat::Markdown => entries.iter().map(|entry| entry_markdown(entry) + "\n").collect(),
    };
    match output {
        Some(path) => {
            std::fs::write(&path, exported).map_err(|error| format!("{path}: {error}"))?;
            eprintln!("wrote {} entries to {path}", entries.len());
        }
        None => print!("{exported}"),
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{NAME}: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let command_line = CommandLine::parse();
    let mission_body = read_mission_body(&command_line.action)?;
    let now_ms = jiff::Timestamp::now().as_millisecond();
    let plan = plan_action(command_line.action, command_line.run, now_ms, mission_body)?;
    if matches!(plan, Plan::ServeTools) {
        return mcp_server::serve_tools().await;
    }
    let mut client = Client::connect().await?;
    match plan {
        Plan::ServeTools => Ok(()),
        Plan::Ask(command) => {
            print_reply(&client.ask(command).await?);
            Ok(())
        }
        Plan::Follow { run, filter } => follow_log(&mut client, run, filter).await,
        Plan::Export { run, filter, format, output } => export_log(&mut client, run, filter, format, output).await,
    }
}

#[cfg(test)]
mod tests {
    use legion2_proto::EntryKind;

    use super::*;

    fn entry(run: &str, kind: EntryKind, mission: Option<u32>) -> Entry {
        Entry { id: 1, run: run.into(), at_ms: 0, mission, from: "builder".into(), to: None, kind, text: String::new(), answers: None }
    }

    #[test]
    fn following_keeps_to_the_run_and_filter() {
        let filter = LogFilter { mission: Some(2), kinds: Some(vec![EntryKind::Note]), ..Default::default() };
        assert!(passes_follow_filter(&entry("r", EntryKind::Note, Some(2)), "r", &filter));
        assert!(!passes_follow_filter(&entry("other", EntryKind::Note, Some(2)), "r", &filter));
        assert!(!passes_follow_filter(&entry("r", EntryKind::Note, Some(3)), "r", &filter));
        assert!(!passes_follow_filter(&entry("r", EntryKind::Done, Some(2)), "r", &filter));
    }
}
