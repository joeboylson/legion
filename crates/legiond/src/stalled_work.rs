//! Work the commander has lost track of: a mission handed off that no one
//! picked up, or a mission waiting while its first operator has room. In a
//! busy deployment the commander drops these; Legion points them out, once
//! each, while the commander is idle.

use legion_proto::MissionStatus;

use crate::constants::STALL_GRACE_MS;

pub struct MissionView {
    pub number: u32,
    pub status: MissionStatus,
    /// When it was last handed off, if it was.
    pub handed_off_at_ms: Option<i64>,
    /// Whether a session is working on it now.
    pub is_being_worked: bool,
    /// Who its last handoff named next, as written ("checker, reviewer").
    pub handed_to: Option<String>,
    /// Whether any operator it was handed to has a session running.
    pub is_handed_to_running: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Stall {
    NotPickedUp { mission: u32, since_ms: i64, handed_to: Option<String>, is_handed_to_running: bool },
    WaitingWithRoom { mission: u32 },
}

impl Stall {
    /// What it's remembered by, so each is pointed out only once.
    pub fn key(&self) -> String {
        match self {
            // Pointed out again once whoever it went to stops running: a batch it
            // was queued in ended with its session, and the work is stranded.
            Stall::NotPickedUp { mission, since_ms, is_handed_to_running, .. } => format!("not-picked-up:{mission}:{since_ms}:{is_handed_to_running}"),
            Stall::WaitingWithRoom { mission } => format!("waiting:{mission}"),
        }
    }
}

/// Whether a session on a handed-off mission is working it: busy now, or it
/// took a turn since the handoff and paused (a long run in the background).
/// A session that handed the mission on took its last turn before.
pub fn works_on_mission(is_busy: bool, turn_started_ms: Option<i64>, handed_off_at_ms: Option<i64>) -> bool {
    is_busy || turn_started_ms.zip(handed_off_at_ms).is_some_and(|(turn, handoff)| turn >= handoff)
}

/// The stalls in one deployment. `first_has_room`: the pipeline's first
/// operator has a free or idle copy to start on a waiting mission.
pub fn stalls(missions: &[MissionView], first_has_room: bool, now_ms: i64) -> Vec<Stall> {
    missions
        .iter()
        .filter_map(|mission| match (mission.status, mission.handed_off_at_ms) {
            (MissionStatus::HandedOff, Some(since_ms)) if !mission.is_being_worked && now_ms - since_ms >= STALL_GRACE_MS => {
                Some(Stall::NotPickedUp { mission: mission.number, since_ms, handed_to: mission.handed_to.clone(), is_handed_to_running: mission.is_handed_to_running })
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
            Stall::NotPickedUp { mission, handed_to: Some(next), is_handed_to_running: false, .. } => {
                format!("- mission {mission} was handed off to {next}, which isn't running: start {next} on mission {mission}")
            }
            Stall::NotPickedUp { mission, handed_to: Some(next), is_handed_to_running: true, .. } => {
                format!("- mission {mission} was handed off to {next} and no one is working on it: send it to {next}, or start a copy on it")
            }
            Stall::NotPickedUp { mission, handed_to: None, .. } => format!("- mission {mission} was handed off and no one is working on it: start or message whoever is next"),
            Stall::WaitingWithRoom { mission } => format!("- mission {mission} is waiting and {first_operator} has room: start it"),
        })
        .collect();
    format!("Work waiting on you:\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mission(number: u32, status: MissionStatus, handed_off_at_ms: Option<i64>, is_being_worked: bool) -> MissionView {
        MissionView { number, status, handed_off_at_ms, is_being_worked, handed_to: None, is_handed_to_running: false }
    }

    fn not_picked_up(handed_to: Option<&str>, is_handed_to_running: bool) -> Stall {
        Stall::NotPickedUp { mission: 101, since_ms: 0, handed_to: handed_to.map(String::from), is_handed_to_running }
    }

    #[test]
    fn a_session_paused_since_the_handoff_still_works_the_mission() {
        assert!(works_on_mission(true, None, Some(100)));
        assert!(works_on_mission(false, Some(150), Some(100)));
        assert!(!works_on_mission(false, Some(50), Some(100)));
        assert!(!works_on_mission(false, None, Some(100)));
    }

    #[test]
    fn a_handoff_no_one_picked_up_stalls_after_the_grace() {
        let missions = [mission(11, MissionStatus::HandedOff, Some(0), false)];
        assert!(stalls(&missions, false, STALL_GRACE_MS - 1).is_empty());
        assert_eq!(stalls(&missions, false, STALL_GRACE_MS), vec![Stall::NotPickedUp { mission: 11, since_ms: 0, handed_to: None, is_handed_to_running: false }]);
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
        let at = |since_ms| Stall::NotPickedUp { mission: 11, since_ms, handed_to: None, is_handed_to_running: false };
        assert_ne!(at(1).key(), at(2).key());
        let text = stall_reminder(&[Stall::WaitingWithRoom { mission: 15 }], "planner");
        assert!(text.contains("mission 15 is waiting and planner has room"));
    }

    #[test]
    fn a_handoff_is_pointed_out_again_once_whoever_it_went_to_stops_running() {
        assert_ne!(not_picked_up(Some("integrator"), true).key(), not_picked_up(Some("integrator"), false).key());
    }

    #[test]
    fn the_reminder_names_who_to_start_when_they_arent_running() {
        let text = stall_reminder(&[not_picked_up(Some("integrator"), false)], "planner");
        assert!(text.contains("mission 101 was handed off to integrator, which isn't running: start integrator on mission 101"));
    }

    #[test]
    fn the_reminder_names_who_to_send_it_to_when_they_are_running() {
        let text = stall_reminder(&[not_picked_up(Some("integrator"), true)], "planner");
        assert!(text.contains("handed off to integrator and no one is working on it: send it to integrator"));
    }
}
