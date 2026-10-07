//! Turns that end with something on a session's screen and nothing in the
//! deployment log: a question or a result no one would otherwise see.

use legion2_proto::{role_of_position, EntryKind, Role};

use crate::access::is_session_entry_kind;

/// Whether the turn about to start only answers announcements, judged by
/// what was just handed to the session; None when nothing was.
pub fn is_announcements_only(handed_kinds: &[EntryKind]) -> Option<bool> {
    match handed_kinds {
        [] => None,
        kinds => Some(kinds.iter().all(|kind| *kind == EntryKind::Announcement)),
    }
}

/// Several handings can wait for the same next turn: it only answers
/// heads-ups if every one of them was only heads-ups.
pub fn next_turn_is_announcements_only(waiting: Option<bool>, just_handed: bool) -> bool {
    waiting.unwrap_or(true) && just_handed
}

/// What the commander hears when an operator ends a turn having reported
/// nothing through its tools, or None when there's nothing to pass on.
pub fn unreported_turn_message(position: &str, kinds_added_this_turn: &[EntryKind], final_answer: &str) -> Option<String> {
    // The commander's screen is its own; the strategist ending a turn with nothing to suggest is normal.
    let is_operator = role_of_position(position) == Role::Operator;
    let has_reported = kinds_added_this_turn.iter().any(|kind| is_session_entry_kind(*kind));
    let answer = final_answer.trim();
    if !is_operator || has_reported || answer.is_empty() {
        return None;
    }
    Some(format!("{position} ended its turn without reporting through its tools, and wrote this on its screen:\n{answer}"))
}

#[cfg(test)]
mod tests {
    use legion2_proto::{COMMANDER, STRATEGIST};

    use super::*;

    #[test]
    fn a_turn_handed_only_announcements_needs_no_report() {
        assert_eq!(is_announcements_only(&[EntryKind::Announcement, EntryKind::Announcement]), Some(true));
        assert_eq!(is_announcements_only(&[EntryKind::Announcement, EntryKind::Handoff]), Some(false));
        assert_eq!(is_announcements_only(&[]), None);
    }

    #[test]
    fn real_work_waiting_for_the_next_turn_keeps_it_reported() {
        assert!(next_turn_is_announcements_only(None, true));
        assert!(next_turn_is_announcements_only(Some(true), true));
        assert!(!next_turn_is_announcements_only(Some(false), true));
        assert!(!next_turn_is_announcements_only(Some(true), false));
    }

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
    fn the_commander_the_strategist_and_empty_answers_are_left_alone() {
        assert_eq!(unreported_turn_message(COMMANDER, &[], "Standing by."), None);
        assert_eq!(unreported_turn_message(STRATEGIST, &[], "Nothing to speed up."), None);
        assert_eq!(unreported_turn_message("planner", &[], "   "), None);
    }
}
