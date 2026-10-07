//! What a session may ask legion2d to do. The human (a caller that isn't a
//! session) may do everything; sessions only act in their own deployment.

use legion2_proto::{Command, EntryKind, Role, COMMANDER};

const STRATEGIST_REFUSAL: &str = "the strategist only suggests: it reads the deployment and sends the commander suggestions with suggest_speedup";

/// The strategist reads, looks at screens, messages the commander, and
/// writes postmortems and callouts: nothing else.
fn strategist_command_targets(command: &Command) -> Result<Option<&str>, String> {
    match command {
        Command::Ping => Ok(None),
        Command::MissionList { deployment } | Command::MissionRead { deployment, .. } | Command::Log { deployment, .. } | Command::Screen { deployment, .. } => {
            Ok(Some(deployment))
        }
        Command::SessionList { deployment } => deployment.as_deref().map(Some).ok_or_else(|| "say which deployment".to_string()),
        Command::Post { deployment, entry } if entry.kind == EntryKind::Message && entry.to.as_deref() == Some(COMMANDER) => Ok(Some(deployment)),
        Command::Post { deployment, entry } if entry.kind == EntryKind::Postmortem => Ok(Some(deployment)),
        Command::Callout { deployment, .. } | Command::CalloutList { deployment } | Command::ToolboxRead { deployment, .. } => Ok(Some(deployment)),
        _ => Err(STRATEGIST_REFUSAL.into()),
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
    EntryKind::Decision,
];

/// The commander also passes the human's answers back.
const COMMANDER_ONLY_ENTRY_KINDS: &[EntryKind] =
    &[EntryKind::Paused, EntryKind::Resumed, EntryKind::Postmortem, EntryKind::Answer];

/// Whether a session adds this kind of entry through its tools.
pub fn is_session_entry_kind(kind: EntryKind) -> bool {
    OPERATOR_ENTRY_KINDS.contains(&kind) || COMMANDER_ONLY_ENTRY_KINDS.contains(&kind)
}

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

/// The deployment a session's command acts in (None when it acts in none), or why
/// its role can't send it.
pub fn run_a_session_command_targets(role: Role, command: &Command) -> Result<Option<&str>, String> {
    if role == Role::Strategist {
        return strategist_command_targets(command);
    }
    let is_commander = role == Role::Commander;
    match command {
        Command::Ping => Ok(None),
        Command::MissionList { deployment } | Command::MissionRead { deployment, .. } | Command::Log { deployment, .. } => Ok(Some(deployment)),
        Command::SessionList { deployment } => deployment.as_deref().map(Some).ok_or_else(|| "say which deployment".to_string()),
        Command::Post { deployment, entry } if can_add_entry(role, entry.kind) => Ok(Some(deployment)),
        Command::ToolShare { deployment, .. }
        | Command::PartFinish { deployment, .. }
        | Command::Callout { deployment, .. }
        | Command::CalloutList { deployment }
        | Command::ToolboxRead { deployment, .. } => Ok(Some(deployment)),
        Command::Post { entry, .. } => Err(refusal(allowed_to_add(entry.kind), &format!("add {} entries", entry.kind.as_str()))),
        Command::SessionStart { deployment, .. }
        | Command::SessionStop { deployment, .. }
        | Command::Screen { deployment, .. }
        | Command::MissionFinish { deployment, .. }
        | Command::MissionSplit { deployment, .. }
            if is_commander =>
        {
            Ok(Some(deployment))
        }
        Command::SessionStart { .. } | Command::SessionStop { .. } | Command::Screen { .. } | Command::MissionFinish { .. } | Command::MissionSplit { .. } => {
            Err(refusal(AllowedTo::CommanderAndHuman, "start, stop or watch sessions, or finish or split missions"))
        }
        Command::MissionAdd { .. } => Err("only the human creates missions; suggest one with the suggest tool".into()),
        Command::Key { .. } | Command::Input { .. } => Err(refusal(AllowedTo::HumanOnly, "type into a session")),
        Command::FolderAdd { .. } | Command::FolderList | Command::FolderRead { .. } | Command::DeploymentStart { .. } | Command::DeploymentList { .. } | Command::DeploymentClose { .. } => {
            Err(refusal(AllowedTo::HumanOnly, "add folders, or start, list or close deployments"))
        }
        Command::Watch => Err(refusal(AllowedTo::HumanOnly, "watch everything")),
    }
}

#[cfg(test)]
mod tests {
    use legion2_proto::{role_of_position, LogFilter, NewEntry};

    use super::*;

    fn post(kind: EntryKind) -> Command {
        Command::Post { deployment: "r".into(), entry: NewEntry { kind, mission: None, to: None, text: String::new(), answers: None } }
    }

    fn start() -> Command {
        Command::SessionStart { deployment: "r".into(), operator: "builder".into(), mission: None, part: None }
    }

    #[test]
    fn roles_come_from_the_position() {
        assert_eq!(role_of_position("commander"), Role::Commander);
        assert_eq!(role_of_position("strategist"), Role::Strategist);
        assert_eq!(role_of_position("builder-2"), Role::Operator);
    }

    fn message_to(to: &str) -> Command {
        Command::Post { deployment: "r".into(), entry: NewEntry { kind: EntryKind::Message, mission: None, to: Some(to.into()), text: String::new(), answers: None } }
    }

    #[test]
    fn the_strategist_reads_watches_and_messages_only_the_commander() {
        let screen = Command::Screen { deployment: "r".into(), position: "builder".into() };
        assert_eq!(run_a_session_command_targets(Role::Strategist, &screen), Ok(Some("r")));
        assert_eq!(run_a_session_command_targets(Role::Strategist, &message_to("commander")), Ok(Some("r")));
        assert!(run_a_session_command_targets(Role::Strategist, &message_to("builder")).is_err());
    }

    #[test]
    fn the_strategist_never_commands() {
        let stop = Command::SessionStop { deployment: "r".into(), position: "builder".into() };
        let finish = Command::MissionFinish { deployment: "r".into(), mission: 1 };
        for command in [start(), stop, finish, post(EntryKind::Note), post(EntryKind::Handoff)] {
            assert!(run_a_session_command_targets(Role::Strategist, &command).unwrap_err().contains("only suggests"));
        }
    }

    #[test]
    fn ping_targets_no_deployment() {
        assert_eq!(run_a_session_command_targets(Role::Operator, &Command::Ping), Ok(None));
    }

    #[test]
    fn operators_read_and_report_in_their_deployment() {
        let read = Command::Log { deployment: "r".into(), filter: LogFilter::default() };
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
    fn no_session_creates_missions_or_deployments() {
        let mission = Command::MissionAdd { deployment: "r".into(), title: "t".into(), body: "b".into() };
        let deployment = Command::DeploymentStart { folder: "f".into(), pipeline: "p".into(), name: None };
        for role in [Role::Commander, Role::Operator] {
            assert!(run_a_session_command_targets(role, &mission).is_err());
            assert!(run_a_session_command_targets(role, &deployment).is_err());
        }
    }

    #[test]
    fn refusals_name_who_can() {
        let close = Command::DeploymentClose { deployment: "r".into() };
        assert!(run_a_session_command_targets(Role::Operator, &close).unwrap_err().ends_with("that's for the human"));
        let postmortem = run_a_session_command_targets(Role::Operator, &post(EntryKind::Postmortem)).unwrap_err();
        assert!(postmortem.ends_with("the commander or the human"));
        let legion_entry = run_a_session_command_targets(Role::Commander, &post(EntryKind::Finished)).unwrap_err();
        assert!(legion_entry.ends_with("that's for the human"));
    }

    #[test]
    fn no_session_types_into_another() {
        let input = Command::Input { deployment: "r".into(), position: "builder".into(), text: "y".into() };
        assert!(run_a_session_command_targets(Role::Commander, &input).is_err());
    }

    #[test]
    fn session_list_needs_a_deployment() {
        assert!(run_a_session_command_targets(Role::Operator, &Command::SessionList { deployment: None }).is_err());
    }
}
