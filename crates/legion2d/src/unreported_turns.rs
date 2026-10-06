//! Turns that end with something on a session's screen and nothing in the
//! deployment log: a question or a result no one would otherwise see.

use legion2_proto::{EntryKind, COMMANDER};

use crate::access::is_session_entry_kind;

/// What the commander hears when an operator ends a turn having reported
/// nothing through its tools, or None when there's nothing to pass on.
pub fn unreported_turn_message(position: &str, kinds_added_this_turn: &[EntryKind], final_answer: &str) -> Option<String> {
    let is_commander = position == COMMANDER;
    let has_reported = kinds_added_this_turn.iter().any(|kind| is_session_entry_kind(*kind));
    let answer = final_answer.trim();
    if is_commander || has_reported || answer.is_empty() {
        return None;
    }
    Some(format!("{position} ended its turn without reporting through its tools, and wrote this on its screen:\n{answer}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_left_on_screen_is_passed_on() {
        let message = unreported_turn_message("planner", &[], "Should it be HTML or React?").unwrap();
        assert!(message.starts_with("planner ended its turn without reporting"));
        assert!(message.ends_with("Should it be HTML or React?"));
    }

    #[test]
    fn a_turn_that_reported_is_left_alone() {
        assert_eq!(unreported_turn_message("planner", &[EntryKind::Handoff], "Handed off."), None);
        assert_eq!(unreported_turn_message("planner", &[EntryKind::Message], "Asked the commander."), None);
    }

    #[test]
    fn legions_own_entries_dont_count_as_reporting() {
        assert!(unreported_turn_message("planner", &[EntryKind::SessionStarted, EntryKind::Permission], "Which file?").is_some());
    }

    #[test]
    fn the_commander_and_empty_answers_are_left_alone() {
        assert_eq!(unreported_turn_message(COMMANDER, &[], "Standing by."), None);
        assert_eq!(unreported_turn_message("planner", &[], "   "), None);
    }
}
