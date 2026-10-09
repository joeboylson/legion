//! How replies look in a terminal: tables and color. Piped output, and
//! Legion's tools inside Claude, get the plain text from reply_format.

use comfy_table::{presets::UTF8_FULL_CONDENSED, Attribute, Cell, Color, ContentArrangement, Table};
use legion2_proto::{pipeline_label, Activity, Deployment, Entry, EntryKind, FolderDetail, Mission, MissionStatus, Reply, SessionInfo, NAME};

use crate::reply_format::{local_time, CLOCK_FORMAT};

pub fn table(headings: &[&str], rows: Vec<Vec<Cell>>) -> String {
    let mut table = Table::new();
    table
        .load_style(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headings.iter().map(|heading| Cell::new(heading).add_attribute(Attribute::Bold)));
    rows.into_iter().for_each(|row| {
        table.add_row(row);
    });
    table.to_string()
}

fn faint(text: impl ToString) -> Cell {
    Cell::new(text).fg(Color::DarkGrey)
}

fn activity_cell(activity: Activity) -> Cell {
    let color = match activity {
        Activity::Busy => Color::Yellow,
        Activity::Idle => Color::Green,
        Activity::Permission | Activity::Halted | Activity::Limited => Color::Red,
        Activity::Starting => Color::Cyan,
        Activity::Ended => Color::DarkGrey,
    };
    Cell::new(activity.label()).fg(color)
}

fn status_cell(status: MissionStatus) -> Cell {
    let (label, color) = match status {
        MissionStatus::Waiting => ("waiting", Color::Cyan),
        MissionStatus::Started => ("started", Color::Yellow),
        MissionStatus::HandedOff => ("handed off", Color::Yellow),
        MissionStatus::Paused => ("paused", Color::DarkGrey),
        MissionStatus::Blocked => ("blocked", Color::Red),
        MissionStatus::Done => ("done", Color::Green),
    };
    Cell::new(label).fg(color)
}

fn optional(value: Option<impl ToString>) -> Cell {
    value.map_or_else(|| faint("—"), |value| Cell::new(value.to_string()))
}

pub fn sessions_table(sessions: &[SessionInfo], deployment_names: &dyn Fn(&str) -> String) -> String {
    let rows = sessions
        .iter()
        .map(|session| {
            vec![
                Cell::new(deployment_names(&session.deployment)),
                Cell::new(&session.position).add_attribute(Attribute::Bold),
                activity_cell(session.activity),
                optional(session.mission.map(|number| format!("#{number}"))),
                optional(session.context_percent.map(|percent| format!("{percent}%"))),
                optional(session.detail.as_deref()),
            ]
        })
        .collect();
    table(&["Deployment", "Position", "Doing", "Mission", "Context", "Detail"], rows)
}

pub fn missions_table(missions: &[Mission]) -> String {
    let rows = missions
        .iter()
        .map(|mission| vec![faint(format!("#{}", mission.number)), status_cell(mission.status), optional(mission.holder.as_deref()), Cell::new(&mission.title)])
        .collect();
    table(&["#", "Status", "With", "Mission"], rows)
}

fn deployments_table(deployments: &[Deployment]) -> String {
    let rows = deployments
        .iter()
        .map(|deployment| {
            let state = if deployment.closed_ms.is_some() { faint("closed") } else { Cell::new("open").fg(Color::Green) };
            vec![Cell::new(&deployment.name).add_attribute(Attribute::Bold), state, Cell::new(pipeline_label(&deployment.pipeline)), faint(&deployment.id), Cell::new(&deployment.folder)]
        })
        .collect();
    table(&["Deployment", "State", "Pipeline", "ID", "Folder"], rows)
}

pub fn operators_table(detail: &FolderDetail) -> String {
    if detail.operators.is_empty() {
        return format!("No operators yet. Add one with `{NAME} operator add <name> <what it does>`.");
    }
    let rows = detail
        .operators
        .iter()
        .map(|operator| {
            let does = match &operator.problem {
                Some(problem) => Cell::new(problem).fg(Color::Red),
                None => Cell::new(summary(&operator.definition)),
            };
            vec![Cell::new(&operator.name).add_attribute(Attribute::Bold), does, Cell::new(operator.copy_limit), optional(operator.model.as_deref())]
        })
        .collect();
    table(&["Operator", "What it does", "Copies", "Model"], rows)
}

pub fn pipelines_table(detail: &FolderDetail) -> String {
    let rows = detail
        .pipelines
        .iter()
        .map(|pipeline| {
            let route = match (&pipeline.problem, &pipeline.first) {
                (Some(problem), _) => Cell::new(problem).fg(Color::Red),
                (None, Some(_)) => Cell::new(pipeline_route(pipeline)),
                (None, None) => Cell::new("through the commander"),
            };
            vec![Cell::new(pipeline_label(&pipeline.name)).add_attribute(Attribute::Bold), Cell::new(pipeline.operators.join(", ")), route]
        })
        .collect();
    table(&["Pipeline", "Operators", "How work moves"], rows)
}

/// The steps each operator passes to first, in order from the first.
fn pipeline_route(pipeline: &legion2_proto::PipelineDetail) -> String {
    let next_of = |operator: &str| pipeline.decisions.iter().find(|decision| decision.operator == operator).map(|decision| decision.next.clone());
    let mut route = Vec::new();
    let mut current = pipeline.first.clone();
    while let Some(step) = current.filter(|step| !route.contains(step) && route.len() <= pipeline.operators.len()) {
        current = next_of(&step);
        route.push(step);
    }
    route.join(" → ")
}

/// An operator's definition in one line: its first line that isn't a heading.
pub fn summary(definition: &str) -> String {
    definition.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with('#')).unwrap_or("").to_string()
}

pub fn entries_text(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let to = entry.to.as_ref().map(|to| format!(" → {to}")).unwrap_or_default();
            let mission = entry.mission.map(|number| format!(" [#{number}]")).unwrap_or_default();
            format!(
                "{} {} {}{to} {}{mission}  {}",
                console::style(format!("#{}", entry.id)).dim(),
                console::style(local_time(entry.at_ms, CLOCK_FORMAT)).dim(),
                console::style(&entry.from).bold(),
                console::style(entry.kind.as_str()).cyan(),
                entry.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One step on a mission's way through its statuses, from its log entry.
pub fn mission_step(entry: &Entry) -> Option<String> {
    let who = &entry.from;
    match entry.kind {
        EntryKind::MissionAdded => Some("added".into()),
        EntryKind::SessionStarted => Some(format!("{who} started on it")),
        EntryKind::Handoff => {
            let next = entry.text.strip_prefix("→ ").and_then(|rest| rest.split(':').next()).unwrap_or("the next step");
            Some(format!("{who} handed it to {next}"))
        }
        EntryKind::Done => Some(format!("{who} reported it done")),
        EntryKind::Blocked => Some(format!("{who} reported it blocked")),
        EntryKind::Paused => Some("paused".into()),
        EntryKind::Resumed => Some("resumed".into()),
        EntryKind::Finished => Some("finished onto the base branch".into()),
        _ => None,
    }
}

/// A mission and its way so far: where it stands, then each step with its time.
pub fn mission_view(mission: &Mission, entries: &[Entry]) -> String {
    let holder = mission.holder.as_ref().map(|holder| format!(" with {holder}")).unwrap_or_default();
    let status = format!("{:?}", mission.status).to_lowercase();
    let steps: Vec<String> = entries
        .iter()
        .filter_map(|entry| mission_step(entry).map(|step| format!("  {}  {step}", console::style(local_time(entry.at_ms, CLOCK_FORMAT)).dim())))
        .collect();
    format!(
        "{} {}\n{}{holder}\n\n{}",
        console::style(format!("#{}", mission.number)).dim(),
        console::style(&mission.title).bold(),
        console::style(status).cyan(),
        if steps.is_empty() { "Nothing has happened yet.".to_string() } else { steps.join("\n") }
    )
}

/// The terminal's version of a reply, when it has one; plain text otherwise.
pub fn reply_display(reply: &Reply) -> Option<String> {
    match reply {
        Reply::Sessions { sessions } if !sessions.is_empty() => Some(sessions_table(sessions, &|id| id.to_string())),
        Reply::Missions { missions } if !missions.is_empty() => Some(missions_table(missions)),
        Reply::Deployments { deployments } if !deployments.is_empty() => Some(deployments_table(deployments)),
        Reply::Entries { entries } if !entries.is_empty() => Some(entries_text(entries)),
        Reply::Entry { entry } => Some(entries_text(std::slice::from_ref(entry))),
        Reply::FolderDetail { detail } => Some(format!("{}\n{}", pipelines_table(detail), operators_table(detail))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use legion2_proto::{DecisionDetail, PipelineDetail};

    use super::*;

    #[test]
    fn a_route_follows_each_operators_first_decision() {
        let decision = |operator: &str, next: &str| DecisionDetail { operator: operator.into(), condition: "c".into(), next: next.into() };
        let pipeline = PipelineDetail {
            name: "feature".into(),
            operators: vec!["planner".into(), "builder".into(), "reviewer".into()],
            first: Some("planner".into()),
            decisions: vec![decision("planner", "builder"), decision("builder", "reviewer"), decision("builder", "planner"), decision("reviewer", "done")],
            problem: None,
            file_text: None,
        };
        assert_eq!(pipeline_route(&pipeline), "planner → builder → reviewer → done");
    }

    #[test]
    fn a_summary_skips_the_heading() {
        assert_eq!(summary("# Poet\n\nYou write haiku.\nMore."), "You write haiku.");
    }

    #[test]
    fn a_missions_way_reads_as_steps() {
        let entry = |from: &str, kind, text: &str| Entry { id: 1, deployment: "d".into(), at_ms: 0, mission: Some(7), from: from.into(), to: None, kind, text: text.into(), answers: None };
        assert_eq!(mission_step(&entry("human", EntryKind::MissionAdded, "")), Some("added".into()));
        assert_eq!(mission_step(&entry("poet", EntryKind::Handoff, "→ commander: wrote it")), Some("poet handed it to commander".into()));
        assert_eq!(mission_step(&entry("editor", EntryKind::Done, "ok")), Some("editor reported it done".into()));
        assert_eq!(mission_step(&entry("poet", EntryKind::Note, "hm")), None);
    }
}
