//! Narrowing a deployment log down to what a reader asked for.

use std::collections::HashSet;

use legion2_proto::{Entry, EntryKind, LogFilter};

fn passes_filter(entry: &Entry, filter: &LogFilter, answered_question_ids: &HashSet<i64>) -> bool {
    let is_on_mission = filter.mission.is_none_or(|mission| entry.mission == Some(mission));
    let involves_position = filter
        .position
        .as_ref()
        .is_none_or(|position| &entry.from == position || entry.to.as_ref() == Some(position));
    let is_wanted_kind = filter.kinds.as_ref().is_none_or(|kinds| kinds.contains(&entry.kind));
    let is_recent_enough = filter.since_ms.is_none_or(|since| entry.at_ms >= since);
    let waits_on_an_answer = matches!(entry.kind, EntryKind::Question | EntryKind::Decision);
    let is_open_question = waits_on_an_answer && !answered_question_ids.contains(&entry.id);
    let passes_open_question_filter = !filter.open_questions || is_open_question;
    is_on_mission && involves_position && is_wanted_kind && is_recent_enough && passes_open_question_filter
}

/// `entries` are the whole deployment's, oldest first, so every answer is in view.
pub fn entries_matching(entries: Vec<Entry>, filter: &LogFilter) -> Vec<Entry> {
    let answered_question_ids: HashSet<i64> = entries.iter().filter_map(|entry| entry.answers).collect();
    entries.into_iter().filter(|entry| passes_filter(entry, filter, &answered_question_ids)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, kind: EntryKind, from: &str, to: Option<&str>, mission: Option<u32>, answers: Option<i64>) -> Entry {
        Entry { id, deployment: "r".into(), at_ms: id * 1000, mission, from: from.into(), to: to.map(String::from), kind, text: String::new(), answers }
    }

    fn sample_log() -> Vec<Entry> {
        vec![
            entry(1, EntryKind::Question, "builder", Some("human"), Some(1), None),
            entry(2, EntryKind::Question, "planner", Some("human"), Some(2), None),
            entry(3, EntryKind::Answer, "human", Some("builder"), Some(1), Some(1)),
            entry(4, EntryKind::Note, "reviewer", None, None, None),
        ]
    }

    fn ids(entries: &[Entry]) -> Vec<i64> {
        entries.iter().map(|entry| entry.id).collect()
    }

    #[test]
    fn no_filter_keeps_everything() {
        assert_eq!(ids(&entries_matching(sample_log(), &LogFilter::default())), vec![1, 2, 3, 4]);
    }

    #[test]
    fn by_mission() {
        let filter = LogFilter { mission: Some(1), ..Default::default() };
        assert_eq!(ids(&entries_matching(sample_log(), &filter)), vec![1, 3]);
    }

    #[test]
    fn by_position_counts_both_ends() {
        let filter = LogFilter { position: Some("builder".into()), ..Default::default() };
        assert_eq!(ids(&entries_matching(sample_log(), &filter)), vec![1, 3]);
    }

    #[test]
    fn by_kind_and_time() {
        let filter = LogFilter { kinds: Some(vec![EntryKind::Question, EntryKind::Note]), since_ms: Some(2000), ..Default::default() };
        assert_eq!(ids(&entries_matching(sample_log(), &filter)), vec![2, 4]);
    }

    #[test]
    fn open_questions_leave_out_answered_ones() {
        let filter = LogFilter { open_questions: true, ..Default::default() };
        assert_eq!(ids(&entries_matching(sample_log(), &filter)), vec![2]);
    }

    #[test]
    fn open_questions_include_decisions_with_no_answer() {
        let log = [sample_log(), vec![entry(5, EntryKind::Decision, "builder", Some("human"), Some(1), None), entry(6, EntryKind::Decision, "planner", Some("human"), None, None), entry(7, EntryKind::Answer, "human", Some("planner"), None, Some(6))]].concat();
        let filter = LogFilter { open_questions: true, ..Default::default() };
        assert_eq!(ids(&entries_matching(log, &filter)), vec![2, 5]);
    }
}
