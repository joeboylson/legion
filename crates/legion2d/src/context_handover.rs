//! Handing over a full conversation. Once a session's conversation passes its
//! clearAt percentage and a turn ends, Legion asks it to write a handoff note.
//! When that turn ends, Legion starts the position again with a fresh
//! conversation, which reads the note. A note the session writes itself keeps
//! what matters; Claude Code's own summary is a guess at that.

use legion2_proto::COMMANDER;

use crate::constants::{AUTOCOMPACT_PERCENT_CAP, AUTOCOMPACT_POINTS_PAST_CLEAR_AT};

/// Where a session is in handing over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HandoverPhase {
    #[default]
    Working,
    /// Asked for a handoff note; starts again when this turn ends.
    NoteAsked,
    /// Ending, to be started again.
    Restarting,
}

/// What Legion does next.
#[derive(Debug, PartialEq, Eq)]
pub enum HandoverAction {
    AskForNote { percent: u8, clear_at: u8 },
    Restart,
}

/// The step taken when one of the session's turns ends. `clear_at` is None
/// for a position that never clears. A position with nothing to hand over
/// (an operator with no mission) carries on: it clears once it has one again.
pub fn on_turn_end(phase: HandoverPhase, percent: Option<u8>, clear_at: Option<u8>, has_work: bool) -> (HandoverPhase, Option<HandoverAction>) {
    match phase {
        HandoverPhase::NoteAsked => (HandoverPhase::Restarting, Some(HandoverAction::Restart)),
        HandoverPhase::Restarting => (phase, None),
        HandoverPhase::Working => match (percent, clear_at) {
            (Some(percent), Some(clear_at)) if has_work && percent >= clear_at => {
                (HandoverPhase::NoteAsked, Some(HandoverAction::AskForNote { percent, clear_at }))
            }
            _ => (phase, None),
        },
    }
}

/// Where Claude Code summarizes a conversation itself, as a backstop for one
/// that doesn't hand over at clearAt.
pub fn autocompact_percent(clear_at: u8) -> u8 {
    clear_at.saturating_add(AUTOCOMPACT_POINTS_PAST_CLEAR_AT).min(AUTOCOMPACT_PERCENT_CAP)
}

/// What the session is asked to do before it's started again.
pub fn note_request(position: &str, mission: Option<u32>, percent: u8, clear_at: u8) -> String {
    let full = format!("Your conversation is {percent}% full; this squad clears it at {clear_at}%.");
    let note = match (position, mission) {
        (COMMANDER, _) => "add a note to the deployment log with the note tool".to_string(),
        (_, Some(number)) => format!("add a note to mission {number} with the note tool"),
        (_, None) => "add a note with the note tool".to_string(),
    };
    format!(
        "{full} Before anything else, {note}, starting 'Handoff:': what's done, what's next, the files and decisions that matter, and anything you'd hate to lose. Then end your turn. Legion will start you again with a fresh conversation that reads that note."
    )
}

/// The first prompt of the fresh session.
pub fn fresh_start_prompt(mission: Option<u32>) -> String {
    let source = match mission {
        Some(number) => format!("mission {number}'s log entries (the log tool, mission {number})"),
        None => "the deployment log (the log tool)".to_string(),
    };
    format!("Legion started you with a fresh conversation because the last one was full. Read your 'Handoff:' note in {source} and carry on from it.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_conversation_with_work_is_asked_for_a_note() {
        let (phase, action) = on_turn_end(HandoverPhase::Working, Some(31), Some(30), true);
        assert_eq!(phase, HandoverPhase::NoteAsked);
        assert_eq!(action, Some(HandoverAction::AskForNote { percent: 31, clear_at: 30 }));
    }

    #[test]
    fn the_turn_after_the_note_restarts_it() {
        assert_eq!(on_turn_end(HandoverPhase::NoteAsked, Some(33), Some(30), true), (HandoverPhase::Restarting, Some(HandoverAction::Restart)));
    }

    #[test]
    fn under_the_line_with_no_work_or_never_clearing_it_carries_on() {
        assert_eq!(on_turn_end(HandoverPhase::Working, Some(29), Some(30), true), (HandoverPhase::Working, None));
        assert_eq!(on_turn_end(HandoverPhase::Working, Some(60), Some(30), false), (HandoverPhase::Working, None));
        assert_eq!(on_turn_end(HandoverPhase::Working, Some(60), None, true), (HandoverPhase::Working, None));
        assert_eq!(on_turn_end(HandoverPhase::Working, None, Some(30), true), (HandoverPhase::Working, None));
    }

    #[test]
    fn a_restarting_session_does_nothing_more() {
        assert_eq!(on_turn_end(HandoverPhase::Restarting, Some(90), Some(30), true), (HandoverPhase::Restarting, None));
    }

    #[test]
    fn claude_summarizes_ten_points_past_clear_at_at_most_95() {
        assert_eq!(autocompact_percent(30), 40);
        assert_eq!(autocompact_percent(90), 95);
    }

    #[test]
    fn operators_note_on_their_mission_and_the_commander_in_the_log() {
        assert!(note_request("builder", Some(8), 55, 30).contains("add a note to mission 8"));
        assert!(note_request(COMMANDER, None, 40, 30).contains("to the deployment log"));
        assert!(fresh_start_prompt(Some(8)).contains("mission 8's log entries"));
    }
}
