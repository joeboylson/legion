//! Turning the command line into what to ask legion2d.

use legion2_proto::{Command, EntryKind, LogFilter, NewEntry, ENV_RUN};

use crate::{
    arguments::{Action, ExportFormat, FilterArguments},
    time_span::parse_time_span_ms,
};

pub enum Plan {
    /// Serve Legion's tools over stdin and stdout.
    ServeTools,
    /// Ask once and print the reply.
    Ask(Command),
    /// Print the log, then keep printing new entries that pass the filter.
    Follow { run: String, filter: LogFilter },
    Export { run: String, filter: LogFilter, format: ExportFormat, output: Option<String> },
}

pub fn log_filter(arguments: &FilterArguments, now_ms: i64) -> Result<LogFilter, String> {
    let kinds = arguments
        .kinds
        .iter()
        .map(|kind_name| EntryKind::parse(kind_name).ok_or_else(|| format!("no entry kind {kind_name:?}")))
        .collect::<Result<Vec<_>, _>>()?;
    let since_ms = arguments.since.as_deref().map(parse_time_span_ms).transpose()?.map(|span| now_ms - span);
    Ok(LogFilter {
        mission: arguments.mission,
        position: arguments.position.clone(),
        kinds: if kinds.is_empty() { None } else { Some(kinds) },
        since_ms,
        open_questions: false,
    })
}

fn entry(run: String, kind: EntryKind, mission: Option<u32>, to: Option<String>, words: &[String]) -> Command {
    Command::Post { run, entry: NewEntry { kind, mission, to, text: words.join(" "), answers: None } }
}

/// `mission_body` is the new mission's text, already read from --body,
/// --file or stdin.
pub fn plan_action(action: Action, run: Option<String>, now_ms: i64, mission_body: Option<String>) -> Result<Plan, String> {
    let needs_run = || run.clone().ok_or_else(|| format!("say which run with --run, or set {ENV_RUN}"));
    let command = match action {
        Action::Ping => Command::Ping,
        Action::Mcp => return Ok(Plan::ServeTools),
        Action::Add { path } => Command::FolderAdd { path },
        Action::Folders => Command::FolderList,
        Action::Run { folder, pipeline, name } => Command::RunStart { folder, pipeline, name },
        Action::Runs { folder } => Command::RunList { folder },
        Action::Close => Command::RunClose { run: needs_run()? },
        Action::New { title, .. } => Command::MissionAdd { run: needs_run()?, title, body: mission_body.unwrap_or_default() },
        Action::Missions => Command::MissionList { run: needs_run()? },
        Action::Mission { number } => Command::MissionRead { run: needs_run()?, mission: number },
        Action::Finish { mission } => Command::MissionFinish { run: needs_run()?, mission },
        Action::Start { operator, mission } => Command::SessionStart { run: needs_run()?, operator, mission },
        Action::Stop { position } => Command::SessionStop { run: needs_run()?, position },
        Action::Sessions => Command::SessionList { run: run.clone() },
        Action::Screen { position } => Command::Screen { run: needs_run()?, position },
        Action::Key { position, key } => Command::Key { run: needs_run()?, position, key },
        Action::Send { to, text } => entry(needs_run()?, EntryKind::Message, None, Some(to), &text),
        Action::Note { mission, text } => entry(needs_run()?, EntryKind::Note, mission, None, &text),
        Action::Ask { mission, text } => entry(needs_run()?, EntryKind::Question, mission, None, &text),
        Action::Answer { question, text } => Command::Post {
            run: needs_run()?,
            entry: NewEntry { kind: EntryKind::Answer, mission: None, to: None, text: text.join(" "), answers: Some(question) },
        },
        Action::Questions => Command::Log { run: needs_run()?, filter: LogFilter { open_questions: true, ..Default::default() } },
        Action::Handoff { mission, next, text } => {
            let handoff_text = [format!("→ {next}:")].into_iter().chain(text).collect::<Vec<_>>();
            entry(needs_run()?, EntryKind::Handoff, Some(mission), None, &handoff_text)
        }
        Action::Done { mission, text } => entry(needs_run()?, EntryKind::Done, Some(mission), None, &text),
        Action::Blocked { mission, text } => entry(needs_run()?, EntryKind::Blocked, Some(mission), None, &text),
        Action::Pause { mission, text } => entry(needs_run()?, EntryKind::Paused, Some(mission), None, &text),
        Action::Resume { mission, text } => entry(needs_run()?, EntryKind::Resumed, Some(mission), None, &text),
        Action::Suggest { text } => entry(needs_run()?, EntryKind::Suggestion, None, None, &text),
        Action::Postmortem { text } => entry(needs_run()?, EntryKind::Postmortem, None, None, &text),
        Action::Log { filter, follow: false } => Command::Log { run: needs_run()?, filter: log_filter(&filter, now_ms)? },
        Action::Log { filter, follow: true } => return Ok(Plan::Follow { run: needs_run()?, filter: log_filter(&filter, now_ms)? }),
        Action::Export { filter, format, output } => {
            return Ok(Plan::Export { run: needs_run()?, filter: log_filter(&filter, now_ms)?, format, output })
        }
    };
    Ok(Plan::Ask(command))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(String::from).collect()
    }

    fn asked(plan: Plan) -> Command {
        match plan {
            Plan::Ask(command) => command,
            _ => panic!("expected a single request"),
        }
    }

    #[test]
    fn run_commands_need_a_run() {
        assert!(plan_action(Action::Missions, None, 0, None).is_err());
        assert!(matches!(asked(plan_action(Action::Missions, Some("r".into()), 0, None).unwrap()), Command::MissionList { run } if run == "r"));
    }

    #[test]
    fn folder_commands_dont() {
        assert!(matches!(asked(plan_action(Action::Folders, None, 0, None).unwrap()), Command::FolderList));
    }

    #[test]
    fn a_handoff_names_the_next_operator() {
        let action = Action::Handoff { mission: 2, next: "reviewer".into(), text: words("it builds") };
        let Command::Post { entry, .. } = asked(plan_action(action, Some("r".into()), 0, None).unwrap()) else { panic!() };
        assert_eq!((entry.kind, entry.mission, entry.text.as_str()), (EntryKind::Handoff, Some(2), "→ reviewer: it builds"));
    }

    #[test]
    fn an_answer_points_at_its_question() {
        let action = Action::Answer { question: 16, text: words("no thanks") };
        let Command::Post { entry, .. } = asked(plan_action(action, Some("r".into()), 0, None).unwrap()) else { panic!() };
        assert_eq!((entry.answers, entry.text.as_str()), (Some(16), "no thanks"));
    }

    #[test]
    fn a_new_mission_takes_the_body_it_was_given() {
        let action = Action::New { title: "Do it".into(), body: None, file: None };
        let command = asked(plan_action(action, Some("r".into()), 0, Some("details".into())).unwrap());
        assert!(matches!(command, Command::MissionAdd { body, .. } if body == "details"));
    }

    #[test]
    fn filters_turn_spans_into_start_times() {
        let arguments = FilterArguments { since: Some("1h".into()), kinds: vec!["question".into()], ..Default::default() };
        let filter = log_filter(&arguments, 10_000_000).unwrap();
        assert_eq!(filter.since_ms, Some(10_000_000 - 3_600_000));
        assert_eq!(filter.kinds, Some(vec![EntryKind::Question]));
    }

    #[test]
    fn unknown_kinds_are_refused() {
        let arguments = FilterArguments { kinds: vec!["gossip".into()], ..Default::default() };
        assert!(log_filter(&arguments, 0).is_err());
    }

    #[test]
    fn following_and_exporting_are_their_own_plans() {
        let follow = Action::Log { filter: FilterArguments::default(), follow: true };
        assert!(matches!(plan_action(follow, Some("r".into()), 0, None).unwrap(), Plan::Follow { .. }));
        let export = Action::Export { filter: FilterArguments::default(), format: ExportFormat::Markdown, output: None };
        assert!(matches!(plan_action(export, Some("r".into()), 0, None).unwrap(), Plan::Export { .. }));
    }
}
