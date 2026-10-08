//! Which changes in what a session is doing the human needs to hear about.

use legion2_proto::{Activity, EntryKind};

const DEFAULT_PERMISSION_REQUEST: &str = "a tool needs permission";

/// The deployment log entry a change calls for, if any: a new permission question,
/// a new wait on a usage limit, or a halt. Each goes to the human.
pub fn entry_for_activity_change(position: &str, previous: Activity, current: Activity, detail: Option<&str>) -> Option<(EntryKind, String)> {
    if previous == current {
        return None;
    }
    match current {
        Activity::Permission => Some((EntryKind::Permission, detail.unwrap_or(DEFAULT_PERMISSION_REQUEST).to_string())),
        Activity::Limited => {
            let reset = detail.map(|reset| format!(" ({reset})")).unwrap_or_default();
            Some((EntryKind::Note, format!("{position} hit its usage limit{reset}; Legion tells it to carry on once the limit resets")))
        }
        Activity::Halted => Some((EntryKind::Note, format!("{position} was halted in its terminal and waits to be told what to do next"))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_permission_question_is_logged() {
        let logged = entry_for_activity_change("builder", Activity::Busy, Activity::Permission, Some("Bash: rm"));
        assert_eq!(logged, Some((EntryKind::Permission, "Bash: rm".into())));
    }

    #[test]
    fn a_permission_question_without_detail_still_says_so() {
        let logged = entry_for_activity_change("builder", Activity::Busy, Activity::Permission, None);
        assert_eq!(logged.map(|(_, text)| text), Some(DEFAULT_PERMISSION_REQUEST.into()));
    }

    #[test]
    fn a_new_limit_is_logged_with_its_reset() {
        let (kind, text) = entry_for_activity_change("builder", Activity::Busy, Activity::Limited, Some("five_hour resets 14:00")).unwrap();
        assert_eq!(kind, EntryKind::Note);
        assert!(text.starts_with("builder hit its usage limit (five_hour resets 14:00)"));
    }

    #[test]
    fn ordinary_changes_and_repeats_are_not() {
        assert_eq!(entry_for_activity_change("builder", Activity::Idle, Activity::Busy, None), None);
        let (kind, text) = entry_for_activity_change("builder", Activity::Busy, Activity::Halted, None).unwrap();
        assert_eq!(kind, EntryKind::Note);
        assert!(text.starts_with("builder was halted"));
        assert_eq!(entry_for_activity_change("builder", Activity::Permission, Activity::Permission, Some("x")), None);
    }
}
