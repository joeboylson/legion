//! `log`: the deployment log, filtered, followed live, or written to a file.

use legion2_proto::{Command, Entry, EntryKind, Event, LogFilter, Reply, ServerMessage};

use crate::{
    commands::show,
    context::Context,
    live::Live,
    pretty::entries_text,
    reply_format::entry_markdown,
    time_span::parse_time_span_ms,
};

pub struct LogInputs {
    pub watch: bool,
    pub mission: Option<u32>,
    pub position: Option<String>,
    pub kinds: Vec<String>,
    pub since: Option<String>,
    pub export: Option<String>,
}

pub fn log_filter(inputs: &LogInputs, now_ms: i64) -> Result<LogFilter, String> {
    let kinds = inputs.kinds.iter().map(|kind| EntryKind::parse(kind).ok_or_else(|| format!("no entry kind {kind:?}"))).collect::<Result<Vec<_>, _>>()?;
    let since_ms = inputs.since.as_deref().map(parse_time_span_ms).transpose()?.map(|span| now_ms - span);
    Ok(LogFilter { mission: inputs.mission, position: inputs.position.clone(), kinds: if kinds.is_empty() { None } else { Some(kinds) }, since_ms, open_questions: false })
}

/// Whether a new entry belongs in a followed log.
pub fn passes_filter(entry: &Entry, deployment_id: &str, filter: &LogFilter) -> bool {
    let is_same_deployment = entry.deployment == deployment_id;
    let is_on_mission = filter.mission.is_none_or(|mission| entry.mission == Some(mission));
    let involves_position = filter.position.as_ref().is_none_or(|position| &entry.from == position || entry.to.as_ref() == Some(position));
    let is_wanted_kind = filter.kinds.as_ref().is_none_or(|kinds| kinds.contains(&entry.kind));
    is_same_deployment && is_on_mission && involves_position && is_wanted_kind
}

fn export_text(entries: &[Entry], path: &str) -> Result<String, String> {
    match path.ends_with(".md") {
        true => Ok(entries.iter().map(|entry| entry_markdown(entry) + "\n").collect()),
        false => entries.iter().map(|entry| serde_json::to_string(entry).map(|line| line + "\n")).collect::<Result<_, _>>().map_err(|error| error.to_string()),
    }
}

pub async fn run(ctx: &mut Context, inputs: LogInputs) -> Result<(), String> {
    let deployment = ctx.deployment().await?;
    let filter = log_filter(&inputs, jiff::Timestamp::now().as_millisecond())?;
    let reply = ctx.send(Command::Log { deployment: deployment.id.clone(), filter: filter.clone() }).await?;
    if let Some(path) = &inputs.export {
        let Reply::Entries { entries } = reply else { return Err("unexpected reply".into()) };
        std::fs::write(path, export_text(&entries, path)?).map_err(|error| format!("{path}: {error}"))?;
        ctx.ask.success(format!("wrote {} entries to {path}", entries.len()));
        return Ok(());
    }
    show(&reply);
    if !inputs.watch || !ctx.ask.is_interactive() {
        return Ok(());
    }
    ctx.send(Command::Watch).await?;
    let live = Live::scrolling()?;
    live.add(&console::style("q to quit").dim().to_string())?;
    loop {
        tokio::select! {
            message = ctx.client.next_message() => {
                if let ServerMessage::Event { event: Event::Entry { entry } } = message? {
                    if passes_filter(&entry, &deployment.id, &filter) {
                        live.add(&entries_text(std::slice::from_ref(&entry)))?;
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                if live.is_quitting()? {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(deployment: &str, kind: EntryKind, mission: Option<u32>) -> Entry {
        Entry { id: 1, deployment: deployment.into(), at_ms: 0, mission, from: "builder".into(), to: None, kind, text: String::new(), answers: None }
    }

    #[test]
    fn following_keeps_to_the_deployment_and_filter() {
        let filter = LogFilter { mission: Some(2), kinds: Some(vec![EntryKind::Note]), ..Default::default() };
        assert!(passes_filter(&entry("r", EntryKind::Note, Some(2)), "r", &filter));
        assert!(!passes_filter(&entry("other", EntryKind::Note, Some(2)), "r", &filter));
        assert!(!passes_filter(&entry("r", EntryKind::Note, Some(3)), "r", &filter));
        assert!(!passes_filter(&entry("r", EntryKind::Done, Some(2)), "r", &filter));
    }

    #[test]
    fn kinds_and_since_become_the_filter() {
        let inputs = LogInputs { watch: false, mission: None, position: None, kinds: vec!["question".into(), "answer".into()], since: Some("1m".into()), export: None };
        let filter = log_filter(&inputs, 120_000).unwrap();
        assert_eq!(filter.kinds, Some(vec![EntryKind::Question, EntryKind::Answer]));
        assert_eq!(filter.since_ms, Some(60_000));
        let bad = LogInputs { kinds: vec!["nonsense".into()], ..inputs };
        assert!(log_filter(&bad, 0).is_err());
    }
}
