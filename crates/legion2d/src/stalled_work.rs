//! Work the commander has lost track of: a mission handed off that no one
//! picked up, or a mission waiting while its first operator has room. In a
//! busy deployment the commander drops these; Legion points them out, once
//! each, while the commander is idle.

use legion2_proto::MissionStatus;

use crate::constants::STALL_GRACE_MS;

pub struct MissionView {
    pub number: u32,
    pub status: MissionStatus,
    /// When it was last handed off, if it was.
    pub handed_off_at_ms: Option<i64>,
    /// Whether a session is working on it now.
    pub is_being_worked: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Stall {
    NotPickedUp { mission: u32, since_ms: i64 },
    WaitingWithRoom { mission: u32 },
}

impl Stall {
    /// What it's remembered by, so each is pointed out only once.
    pub fn key(&self) -> String {
        match self {
            Stall::NotPickedUp { mission, since_ms } => format!("not-picked-up:{mission}:{since_ms}"),
            Stall::WaitingWithRoom { mission } => format!("waiting:{mission}"),
        }
    }
}

/// The stalls in one deployment. `first_has_room`: the pipeline's first
/// operator has a free or idle copy to start on a waiting mission.
pub fn stalls(missions: &[MissionView], first_has_room: bool, now_ms: i64) -> Vec<Stall> {
    missions
        .iter()
        .filter_map(|mission| match (mission.status, mission.handed_off_at_ms) {
            (MissionStatus::HandedOff, Some(since_ms)) if !mission.is_being_worked && now_ms - since_ms >= STALL_GRACE_MS => {
                Some(Stall::NotPickedUp { mission: mission.number, since_ms })
            }
            (MissionStatus::Waiting, _) if first_has_room => Some(Stall::WaitingWithRoom { mission: mission.number }),
            _ => None,
        })
        .collect()
}

pub fn stall_reminder(stalls: &[Stall], first_operator: &str) -> String {
    let lines: Vec<String> = stalls
        .iter()
        .map(|stall| match stall {
            Stall::NotPickedUp { mission, .. } => format!("- mission {mission} was handed off and no one is working on it: start or message whoever is next"),
            Stall::WaitingWithRoom { mission } => format!("- mission {mission} is waiting and {first_operator} has room: start it"),
        })
        .collect();
    format!("Work waiting on you:\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mission(number: u32, status: MissionStatus, handed_off_at_ms: Option<i64>, is_being_worked: bool) -> MissionView {
        MissionView { number, status, handed_off_at_ms, is_being_worked }
    }

    #[test]
    fn a_handoff_no_one_picked_up_stalls_after_the_grace() {
        let missions = [mission(11, MissionStatus::HandedOff, Some(0), false)];
        assert!(stalls(&missions, false, STALL_GRACE_MS - 1).is_empty());
        assert_eq!(stalls(&missions, false, STALL_GRACE_MS), vec![Stall::NotPickedUp { mission: 11, since_ms: 0 }]);
    }

    #[test]
    fn a_handoff_someone_is_working_on_doesnt_stall() {
        let missions = [mission(11, MissionStatus::HandedOff, Some(0), true)];
        assert!(stalls(&missions, false, STALL_GRACE_MS * 2).is_empty());
    }

    #[test]
    fn waiting_missions_stall_only_when_there_is_room() {
        let missions = [mission(15, MissionStatus::Waiting, None, false), mission(16, MissionStatus::Started, None, true)];
        assert!(stalls(&missions, false, 0).is_empty());
        assert_eq!(stalls(&missions, true, 0), vec![Stall::WaitingWithRoom { mission: 15 }]);
    }

    #[test]
    fn each_stall_is_remembered_by_its_mission_and_handoff() {
        assert_ne!(Stall::NotPickedUp { mission: 11, since_ms: 1 }.key(), Stall::NotPickedUp { mission: 11, since_ms: 2 }.key());
        let text = stall_reminder(&[Stall::WaitingWithRoom { mission: 15 }], "planner");
        assert!(text.contains("mission 15 is waiting and planner has room"));
    }
}
