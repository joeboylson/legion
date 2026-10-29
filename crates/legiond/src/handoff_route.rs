//! Who a handoff names next, read from its text ("→ checker, reviewer: built").

use legion_proto::MissionStatus;

use crate::naming::operator_of_position;

const HANDOFF_PREFIX: &str = "→ ";
const NEXT_AND_NOTE_SEPARATOR: &str = ": ";
const NEXT_OPERATOR_SEPARATOR: char = ',';

/// The operators a handoff names next, and its note.
pub fn handoff_parts(text: &str) -> Option<(Vec<&str>, &str)> {
    let (next, note) = text.strip_prefix(HANDOFF_PREFIX)?.split_once(NEXT_AND_NOTE_SEPARATOR)?;
    let operators = next.split(NEXT_OPERATOR_SEPARATOR).map(str::trim).filter(|name| !name.is_empty()).collect();
    Some((operators, note))
}

pub fn names_operator(text: &str, operator: &str) -> bool {
    handoff_parts(text).is_some_and(|(next, _)| next.contains(&operator))
}

/// A handoff to its own operator starts no one: the author still has the step.
pub fn is_handoff_to_self(author: &str, text: &str) -> bool {
    names_operator(text, operator_of_position(author))
}

/// A mission's standing as far as picking it up goes.
pub struct HandedOffMission {
    pub number: u32,
    pub status: MissionStatus,
    /// The text of its latest handoff, if any.
    pub latest_handoff: Option<String>,
    pub has_session: bool,
}

fn is_waiting_on(mission: &HandedOffMission, operator: &str) -> bool {
    mission.status == MissionStatus::HandedOff
        && !mission.has_session
        && mission.latest_handoff.as_deref().is_some_and(|text| names_operator(text, operator))
}

/// Gives each position the first mission handed off to its operator that no
/// session is on, so a copy carries on with work queued behind the mission it
/// just finished. No two positions get the same mission; any left over get none.
pub fn waiting_mission_for_each<'a>(positions: &[&'a str], missions: &[HandedOffMission]) -> Vec<(&'a str, u32)> {
    positions.iter().fold(Vec::new(), |given: Vec<(&'a str, u32)>, position| {
        let operator = operator_of_position(position);
        let next = missions
            .iter()
            .filter(|mission| is_waiting_on(mission, operator))
            .map(|mission| mission.number)
            .find(|number| !given.iter().any(|(_, taken)| taken == number));
        match next {
            Some(number) => given.into_iter().chain([(*position, number)]).collect(),
            None => given,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handed_off(number: u32, handoff: &str, has_session: bool) -> HandedOffMission {
        HandedOffMission { number, status: MissionStatus::HandedOff, latest_handoff: Some(handoff.into()), has_session }
    }

    #[test]
    fn a_handoff_names_its_next_operators_and_note() {
        assert_eq!(handoff_parts("→ checker, reviewer: built: a.rb"), Some((vec!["checker", "reviewer"], "built: a.rb")));
    }

    #[test]
    fn text_that_isnt_a_handoff_names_no_one() {
        assert_eq!(handoff_parts("built it"), None);
        assert!(!names_operator("built it", "builder"));
    }

    #[test]
    fn a_handoff_to_ones_own_operator_is_to_self() {
        assert!(is_handoff_to_self("opie-reviewer", "→ opie-reviewer: again"));
        assert!(is_handoff_to_self("builder-2", "→ checker, builder: again"));
    }

    #[test]
    fn a_handoff_to_another_operator_isnt_to_self() {
        assert!(!is_handoff_to_self("builder", "→ builder-checker: built"));
        assert!(!is_handoff_to_self("opie-reviewer", "→ integrator: passed"));
    }

    #[test]
    fn a_copy_gets_the_mission_handed_to_its_operator_that_no_one_is_on() {
        let missions = [handed_off(101, "→ integrator: reviewed", false), handed_off(103, "→ integrator: reviewed", true)];
        assert_eq!(waiting_mission_for_each(&["integrator"], &missions), vec![("integrator", 101)]);
    }

    #[test]
    fn two_copies_never_get_the_same_mission() {
        let missions = [handed_off(101, "→ integrator: reviewed", false), handed_off(104, "→ integrator: reviewed", false)];
        assert_eq!(waiting_mission_for_each(&["integrator", "integrator-2"], &missions), vec![("integrator", 101), ("integrator-2", 104)]);
    }

    #[test]
    fn a_copy_with_nothing_waiting_gets_nothing() {
        let missions = [handed_off(101, "→ integrator: reviewed", false)];
        assert_eq!(waiting_mission_for_each(&["integrator", "integrator-2"], &missions), vec![("integrator", 101)]);
    }

    #[test]
    fn a_mission_handed_to_someone_else_doesnt_wait_on_the_operator() {
        let missions = [handed_off(101, "→ reviewer: built", false)];
        assert_eq!(waiting_mission_for_each(&["integrator"], &missions), Vec::<(&str, u32)>::new());
    }

    #[test]
    fn a_mission_no_longer_handed_off_doesnt_wait() {
        let started = HandedOffMission { status: MissionStatus::Started, ..handed_off(101, "→ integrator: reviewed", false) };
        assert_eq!(waiting_mission_for_each(&["integrator"], &[started]), Vec::<(&str, u32)>::new());
    }
}
