//! What a session may ask legion2d to do. The human (a caller that isn't a
//! session) may do everything; sessions only act in their own run.

use legion2_proto::{Command, EntryKind, COMMANDER};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Commander,
    Operator,
}

pub fn role_of_position(position: &str) -> Role {
    if position == COMMANDER {
        Role::Commander
    } else {
        Role::Operator
    }
}

const OPERATOR_ENTRY_KINDS: &[EntryKind] = &[
    EntryKind::Note,
    EntryKind::Message,
    EntryKind::Question,
    EntryKind::Handoff,
    EntryKind::Done,
    EntryKind::Blocked,
    EntryKind::Suggestion,
];

/// The commander also passes the human's answers back.
const COMMANDER_ONLY_ENTRY_KINDS: &[EntryKind] =
    &[EntryKind::Paused, EntryKind::Resumed, EntryKind::Postmortem, EntryKind::Answer];

fn can_add_entry(role: Role, kind: EntryKind) -> bool {
    let is_operator_kind = OPERATOR_ENTRY_KINDS.contains(&kind);
    let is_commander_kind = role == Role::Commander && COMMANDER_ONLY_ENTRY_KINDS.contains(&kind);
    is_operator_kind || is_commander_kind
}

#[derive(Clone, Copy)]
enum AllowedTo {
    CommanderAndHuman,
    HumanOnly,
}

fn refusal(allowed_to: AllowedTo, what: &str) -> String {
    let who_can = match allowed_to {
        AllowedTo::CommanderAndHuman => "the commander or the human",
        AllowedTo::HumanOnly => "the human",
    };
    format!("a session can't {what}; that's for {who_can}")
}

/// Who else may add a kind of entry a session was refused.
fn allowed_to_add(kind: EntryKind) -> AllowedTo {
    if COMMANDER_ONLY_ENTRY_KINDS.contains(&kind) {
        AllowedTo::CommanderAndHuman
    } else {
        AllowedTo::HumanOnly
    }
}

/// The run a session's command acts in (None when it acts in none), or why
/// its role can't send it.
pub fn run_a_session_command_targets(role: Role, command: &Command) -> Result<Option<&str>, String> {
    let is_commander = role == Role::Commander;
    match command {
        Command::Ping => Ok(None),
        Command::MissionList { run } | Command::MissionRead { run, .. } | Command::Log { run, .. } => Ok(Some(run)),
        Command::SessionList { run } => run.as_deref().map(Some).ok_or_else(|| "say which run".to_string()),
        Command::Post { run, entry } if can_add_entry(role, entry.kind) => Ok(Some(run)),
        Command::Post { entry, .. } => Err(refusal(allowed_to_add(entry.kind), &format!("add {} entries", entry.kind.as_str()))),
        Command::SessionStart { run, .. }
        | Command::SessionStop { run, .. }
        | Command::Screen { run, .. }
        | Command::MissionFinish { run, .. }
            if is_commander =>
        {
            Ok(Some(run))
        }
        Command::SessionStart { .. } | Command::SessionStop { .. } | Command::Screen { .. } | Command::MissionFinish { .. } => {
            Err(refusal(AllowedTo::CommanderAndHuman, "start, stop or watch sessions, or finish missions"))
        }
        Command::MissionAdd { .. } => Err("only the human creates missions; suggest one with `legion2 suggest`".into()),
        Command::Key { .. } => Err(refusal(AllowedTo::HumanOnly, "type into a session")),
        Command::FolderAdd { .. } | Command::FolderList | Command::RunStart { .. } | Command::RunList { .. } | Command::RunClose { .. } => {
            Err(refusal(AllowedTo::HumanOnly, "add folders, or start, list or close runs"))
        }
        Command::Watch => Err(refusal(AllowedTo::HumanOnly, "watch everything")),
    }
}

#[cfg(test)]
mod tests {
    use legion2_proto::{LogFilter, NewEntry};

    use super::*;

    fn post(kind: EntryKind) -> Command {
        Command::Post { run: "r".into(), entry: NewEntry { kind, mission: None, to: None, text: String::new(), answers: None } }
    }

    fn start() -> Command {
        Command::SessionStart { run: "r".into(), operator: "builder".into(), mission: None }
    }

    #[test]
    fn roles_come_from_the_position() {
        assert_eq!(role_of_position("commander"), Role::Commander);
        assert_eq!(role_of_position("builder-2"), Role::Operator);
    }

    #[test]
    fn ping_targets_no_run() {
        assert_eq!(run_a_session_command_targets(Role::Operator, &Command::Ping), Ok(None));
    }

    #[test]
    fn operators_read_and_report_in_their_run() {
        let read = Command::Log { run: "r".into(), filter: LogFilter::default() };
        assert_eq!(run_a_session_command_targets(Role::Operator, &read), Ok(Some("r")));
        for kind in OPERATOR_ENTRY_KINDS {
            assert_eq!(run_a_session_command_targets(Role::Operator, &post(*kind)), Ok(Some("r")), "{kind:?}");
        }
    }

    #[test]
    fn operators_cant_add_commander_entries() {
        for kind in COMMANDER_ONLY_ENTRY_KINDS {
            assert!(run_a_session_command_targets(Role::Operator, &post(*kind)).is_err(), "{kind:?}");
        }
    }

    #[test]
    fn commander_adds_its_own_entries() {
        for kind in COMMANDER_ONLY_ENTRY_KINDS {
            assert_eq!(run_a_session_command_targets(Role::Commander, &post(*kind)), Ok(Some("r")), "{kind:?}");
        }
    }

    #[test]
    fn nobody_in_a_session_writes_legions_own_entries() {
        for role in [Role::Commander, Role::Operator] {
            assert!(run_a_session_command_targets(role, &post(EntryKind::SessionStarted)).is_err());
            assert!(run_a_session_command_targets(role, &post(EntryKind::Finished)).is_err());
        }
    }

    #[test]
    fn only_the_commander_starts_sessions() {
        assert_eq!(run_a_session_command_targets(Role::Commander, &start()), Ok(Some("r")));
        assert!(run_a_session_command_targets(Role::Operator, &start()).is_err());
    }

    #[test]
    fn no_session_creates_missions_or_runs() {
        let mission = Command::MissionAdd { run: "r".into(), title: "t".into(), body: "b".into() };
        let run = Command::RunStart { folder: "f".into(), pipeline: "p".into(), name: None };
        for role in [Role::Commander, Role::Operator] {
            assert!(run_a_session_command_targets(role, &mission).is_err());
            assert!(run_a_session_command_targets(role, &run).is_err());
        }
    }

    #[test]
    fn refusals_name_who_can() {
        let close = Command::RunClose { run: "r".into() };
        assert!(run_a_session_command_targets(Role::Operator, &close).unwrap_err().ends_with("that's for the human"));
        let postmortem = run_a_session_command_targets(Role::Operator, &post(EntryKind::Postmortem)).unwrap_err();
        assert!(postmortem.ends_with("the commander or the human"));
        let legion_entry = run_a_session_command_targets(Role::Commander, &post(EntryKind::Finished)).unwrap_err();
        assert!(legion_entry.ends_with("that's for the human"));
    }

    #[test]
    fn session_list_needs_a_run() {
        assert!(run_a_session_command_targets(Role::Operator, &Command::SessionList { run: None }).is_err());
    }
}
