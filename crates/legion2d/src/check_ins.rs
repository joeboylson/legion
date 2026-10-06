//! Asking the commander to look in on a long turn. An operator that has
//! worked for a while may be stuck (a command that never returns, a loop),
//! and nothing else would notice.

use crate::constants::OPERATOR_CHECK_IN_INTERVAL_MS;

/// Whether an operator working since `turn_started_ms` is due a check-in:
/// the interval has passed since its turn began, or since the last one.
pub fn is_check_in_due(turn_started_ms: Option<i64>, last_check_in_ms: Option<i64>, now_ms: i64) -> bool {
    let Some(turn_started_ms) = turn_started_ms else { return false };
    let since = last_check_in_ms.map_or(turn_started_ms, |checked| checked.max(turn_started_ms));
    now_ms - since >= OPERATOR_CHECK_IN_INTERVAL_MS
}

pub fn check_in_request(position: &str, mission: Option<u32>, working_ms: i64) -> String {
    let minutes = working_ms / 60_000;
    let on_mission = mission.map(|number| format!(" on mission {number}")).unwrap_or_default();
    format!(
        "{position} has been working for {minutes} minutes{on_mission}. Read its screen with the screen tool. If it's making progress, leave it be. If it's stuck (a command that never returns, the same step over and over, waiting on something), send it a message to get it moving, or ask the human."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE_MS: i64 = 60_000;

    #[test]
    fn a_turn_is_due_once_the_interval_has_passed() {
        assert!(!is_check_in_due(Some(0), None, OPERATOR_CHECK_IN_INTERVAL_MS - 1));
        assert!(is_check_in_due(Some(0), None, OPERATOR_CHECK_IN_INTERVAL_MS));
    }

    #[test]
    fn the_next_is_due_an_interval_after_the_last() {
        let checked = OPERATOR_CHECK_IN_INTERVAL_MS;
        assert!(!is_check_in_due(Some(0), Some(checked), checked + MINUTE_MS));
        assert!(is_check_in_due(Some(0), Some(checked), checked * 2));
    }

    #[test]
    fn a_check_in_from_an_earlier_turn_doesnt_count() {
        let new_turn = 10 * MINUTE_MS;
        assert!(!is_check_in_due(Some(new_turn), Some(MINUTE_MS), new_turn + MINUTE_MS));
    }

    #[test]
    fn a_session_not_in_a_turn_is_never_due() {
        assert!(!is_check_in_due(None, None, 99 * MINUTE_MS));
    }

    #[test]
    fn the_request_says_who_how_long_and_what_to_do() {
        let text = check_in_request("builder", Some(8), 5 * MINUTE_MS);
        assert!(text.starts_with("builder has been working for 5 minutes on mission 8."));
        assert!(text.contains("screen tool"));
    }
}
