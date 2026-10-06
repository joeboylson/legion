//! Where a mission stands, worked out from its deployment log entries.

use legion2_proto::{Entry, EntryKind, MissionStatus};

#[derive(Debug, PartialEq)]
pub struct Standing {
    pub status: MissionStatus,
    pub holder: Option<String>,
}

const NOT_STARTED: Standing = Standing { status: MissionStatus::Waiting, holder: None };

fn apply_entry(standing: Standing, entry: &Entry) -> Standing {
    match entry.kind {
        EntryKind::SessionStarted => Standing { status: MissionStatus::Started, holder: Some(entry.from.clone()) },
        EntryKind::Handoff => Standing { status: MissionStatus::HandedOff, holder: None },
        EntryKind::Done => Standing { status: MissionStatus::Done, holder: None },
        EntryKind::Paused => Standing { status: MissionStatus::Paused, ..standing },
        EntryKind::Resumed => Standing { status: MissionStatus::Waiting, ..standing },
        EntryKind::Blocked => Standing { status: MissionStatus::Blocked, ..standing },
        _ => standing,
    }
}

/// `entries` are one mission's, oldest first.
pub fn mission_standing(entries: &[Entry]) -> Standing {
    entries.iter().fold(NOT_STARTED, apply_entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: EntryKind, from: &str) -> Entry {
        Entry { id: 0, deployment: "r".into(), at_ms: 0, mission: Some(1), from: from.into(), to: None, kind, text: String::new(), answers: None }
    }

    #[test]
    fn a_new_mission_waits() {
        assert_eq!(mission_standing(&[entry(EntryKind::MissionAdded, "human")]), NOT_STARTED);
    }

    #[test]
    fn starting_gives_it_a_holder() {
        let standing = mission_standing(&[entry(EntryKind::SessionStarted, "builder")]);
        assert_eq!(standing, Standing { status: MissionStatus::Started, holder: Some("builder".into()) });
    }

    #[test]
    fn a_handoff_frees_it_until_the_next_start() {
        let handed_off = [entry(EntryKind::SessionStarted, "planner"), entry(EntryKind::Handoff, "planner")];
        assert_eq!(mission_standing(&handed_off), Standing { status: MissionStatus::HandedOff, holder: None });
        let picked_up = [&handed_off[..], &[entry(EntryKind::SessionStarted, "builder")]].concat();
        assert_eq!(mission_standing(&picked_up).holder.as_deref(), Some("builder"));
    }

    #[test]
    fn pausing_keeps_the_holder_and_resuming_waits() {
        let paused = [entry(EntryKind::SessionStarted, "builder"), entry(EntryKind::Paused, "commander")];
        assert_eq!(mission_standing(&paused), Standing { status: MissionStatus::Paused, holder: Some("builder".into()) });
        let resumed = [&paused[..], &[entry(EntryKind::Resumed, "human")]].concat();
        assert_eq!(mission_standing(&resumed).status, MissionStatus::Waiting);
    }

    #[test]
    fn done_is_done() {
        let finished = [entry(EntryKind::SessionStarted, "reviewer"), entry(EntryKind::Done, "reviewer"), entry(EntryKind::Note, "x")];
        assert_eq!(mission_standing(&finished), Standing { status: MissionStatus::Done, holder: None });
    }
}
