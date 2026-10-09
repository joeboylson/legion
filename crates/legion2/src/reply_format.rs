//! How replies and deployment log entries read on screen and in exports.

use legion2_proto::{Channels, Entry, Mission, Part, Reply, SessionInfo, NAME};

pub const CLOCK_FORMAT: &str = "%H:%M:%S";
pub const DATE_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub fn local_time(at_ms: i64, format: &str) -> String {
    jiff::Timestamp::from_millisecond(at_ms)
        .map(|timestamp| timestamp.to_zoned(jiff::tz::TimeZone::system()).strftime(format).to_string())
        .unwrap_or_default()
}

/// Who, to whom, what kind, on which mission, and what it says.
pub fn entry_body(entry: &Entry) -> String {
    let recipient = entry.to.as_ref().map(|to| format!(" → {to}")).unwrap_or_default();
    let mission = entry.mission.map(|number| format!(" [m{number}]")).unwrap_or_default();
    let answering = entry.answers.map(|question| format!(" (to #{question})")).unwrap_or_default();
    format!("{}{recipient} {}{mission}{answering}: {}", entry.from, entry.kind.as_str(), entry.text)
}

pub fn entry_line(entry: &Entry) -> String {
    format!("#{} {} {}", entry.id, local_time(entry.at_ms, CLOCK_FORMAT), entry_body(entry))
}

pub fn entry_markdown(entry: &Entry) -> String {
    format!("- #{} {} {}", entry.id, local_time(entry.at_ms, DATE_TIME_FORMAT), entry_body(entry))
}

fn holder_note(mission: &Mission) -> String {
    mission.holder.as_ref().map(|holder| format!(" ({holder})")).unwrap_or_default()
}

fn session_line(session: &SessionInfo) -> String {
    let mission = session.mission.map(|number| format!(" mission {number}")).unwrap_or_default();
    let blind_note = if session.can_see_state { "" } else { " (can't see this session's state)" };
    let detail = session.detail.as_ref().map(|detail| format!(": {detail}")).unwrap_or_default();
    format!("{} {} {}{mission}{blind_note}{detail}", session.deployment, session.position, session.activity.label())
}

fn lines<T>(items: &[T], line: impl Fn(&T) -> String) -> String {
    items.iter().map(line).collect::<Vec<_>>().join("\n")
}

/// What a reply prints. Empty lists print nothing.
pub fn reply_text(reply: &Reply) -> String {
    match reply {
        Reply::Pong { version } => format!("{NAME}d {version} is running"),
        Reply::Done => "done".into(),
        Reply::Folder { folder, created_setup } => {
            let setup_note = if *created_setup {
                format!("\nset up .{NAME}/ with a starting pipeline and operators; commit it so others get it")
            } else {
                String::new()
            };
            format!(
                "{} ({}){setup_note}\npipelines: {}\noperators: {}",
                folder.name,
                folder.path,
                folder.pipelines.join(", "),
                folder.operators.join(", ")
            )
        }
        Reply::FolderDetail { detail } => {
            let pipelines = lines(&detail.pipelines, |pipeline| {
                let problem = pipeline.problem.as_ref().map(|problem| format!("  ({problem})")).unwrap_or_default();
                format!("  {}: {}{problem}", pipeline.name, pipeline.operators.join(" → "))
            });
            let operators = lines(&detail.operators, |operator| format!("  {} (up to {} at once)", operator.name, operator.copy_limit));
            format!("{} ({})\npipelines:\n{pipelines}\noperators:\n{operators}", detail.folder.name, detail.folder.path)
        }
        Reply::Folders { folders } => lines(folders, |folder| format!("{}  {}  pipelines: {}", folder.name, folder.path, folder.pipelines.join(", "))),
        Reply::Deployment { deployment } => format!("deployment {} ({}) started {}; its commander is starting", deployment.name, deployment.id, legion2_proto::pipeline_phrase(&deployment.pipeline)),
        Reply::Deployments { deployments } => lines(deployments, |deployment| {
            let closed_note = if deployment.closed_ms.is_some() { "  (closed)" } else { "" };
            format!("{}  {}  {}  {}{closed_note}", deployment.id, deployment.name, deployment.pipeline, deployment.folder)
        }),
        Reply::Mission { mission, body, parts } => {
            let parts_note = if parts.is_empty() { String::new() } else { format!("\n\nparts:\n{}", lines(parts, part_line)) };
            format!("#{} {:?}{}\n\n{}{parts_note}", mission.number, mission.status, holder_note(mission), body.trim_end())
        }
        Reply::Parts { parts } => lines(parts, part_line),
        Reply::Missions { missions } => lines(missions, |mission| format!("#{} {:?}{}  {}", mission.number, mission.status, holder_note(mission), mission.title)),
        Reply::Session { session } => format!("{} {}", session.position, session.activity.label()),
        Reply::Sessions { sessions } => lines(sessions, session_line),
        Reply::Screen { text, .. } => text.clone(),
        Reply::Entry { entry } => entry_line(entry),
        Reply::Entries { entries } => lines(entries, entry_line),
        Reply::Text { text } => text.clone(),
        Reply::Channels { channels } => channels_text(channels),
        Reply::ChannelLog { entries } => lines(entries, |entry| {
            let between = match (&entry.from, &entry.to) {
                (Some(from), Some(to)) => format!(" {from} → {to}"),
                (Some(from), None) => format!(" {from}"),
                _ => String::new(),
            };
            let machine = entry.machine.as_deref().map(|machine| format!(" [{machine}]")).unwrap_or_default();
            format!("#{} {} {:?}{machine}{between}: {}", entry.id, local_time(entry.at_ms, CLOCK_FORMAT), entry.kind, entry.text)
        }),
    }
}

fn channels_text(channels: &Channels) -> String {
    let hosted = match &channels.hosted {
        None => "not hosting a channel".to_string(),
        Some(hosted) => {
            let state = match &hosted.problem {
                Some(problem) => format!("down: {problem}"),
                None => format!("{} subscribed", hosted.subscribers.len()),
            };
            let subscribers = lines(&hosted.subscribers, |end| format!("  {} ({})", end.machine, end.address));
            let subscribers_text = if subscribers.is_empty() { String::new() } else { format!("\n{subscribers}") };
            format!("hosting a channel on port {} ({state}); key {}{subscribers_text}", hosted.port, hosted.key)
        }
    };
    let subscriptions = lines(&channels.subscriptions, |subscription| {
        let host = subscription.host_machine.as_deref().map(|machine| format!(" ({machine})")).unwrap_or_default();
        let state = if subscription.is_up { "up".to_string() } else { format!("down: {}", subscription.problem.as_deref().unwrap_or("connecting")) };
        format!("subscribed to {}{host}: {state}", subscription.address)
    });
    let switches = lines(&channels.switches, |set| format!("deployment {}: send {}, receive {}", set.deployment, set.send.as_str(), set.receive.as_str()));
    let switches_text = if switches.is_empty() { String::new() } else { format!("{switches}\nevery other deployment: send ask, receive ask") };
    [hosted, subscriptions, switches_text].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join("\n")
}

fn part_line(part: &Part) -> String {
    let state = if part.is_merged { "merged" } else { "open" };
    format!("part {} ({state}): {}", part.number, part.brief)
}

#[cfg(test)]
mod tests {
    use legion2_proto::{Activity, EntryKind, MissionStatus};

    use super::*;

    fn entry() -> Entry {
        Entry {
            id: 9,
            deployment: "r".into(),
            at_ms: 0,
            mission: Some(2),
            from: "builder".into(),
            to: Some("commander".into()),
            kind: EntryKind::Handoff,
            text: "→ reviewer: built".into(),
            answers: None,
        }
    }

    #[test]
    fn an_entry_reads_who_to_whom_and_what() {
        assert_eq!(entry_body(&entry()), "builder → commander handoff [m2]: → reviewer: built");
        assert!(entry_line(&entry()).starts_with("#9 "));
        assert!(entry_markdown(&entry()).starts_with("- #9 "));
    }

    #[test]
    fn an_answer_names_its_question() {
        let answer = Entry { kind: EntryKind::Answer, answers: Some(4), mission: None, ..entry() };
        assert!(entry_body(&answer).contains("answer (to #4)"));
    }

    #[test]
    fn missions_show_who_holds_them() {
        let mission = Mission { number: 1, deployment: "r".into(), title: "Do it".into(), file: String::new(), status: MissionStatus::Started, holder: Some("builder".into()) };
        assert_eq!(reply_text(&Reply::Missions { missions: vec![mission] }), "#1 Started (builder)  Do it");
    }

    #[test]
    fn sessions_say_when_legion_cant_see_them() {
        let session = SessionInfo { deployment: "r".into(), position: "builder".into(), mission: None, activity: Activity::Idle, can_see_state: false, detail: None, is_stuck_starting: false, model: None, context_percent: None, part: None };
        assert_eq!(reply_text(&Reply::Sessions { sessions: vec![session] }), "r builder idle (can't see this session's state)");
    }

    #[test]
    fn empty_lists_print_nothing() {
        assert_eq!(reply_text(&Reply::Entries { entries: vec![] }), "");
    }
}
