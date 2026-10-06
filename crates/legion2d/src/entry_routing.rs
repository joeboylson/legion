//! Who an entry goes to, and how a delivered entry reads in a session.

use legion2_proto::{Entry, EntryKind, NewEntry, COMMANDER, HUMAN, NAME};

use crate::constants::delivery_prefix;

/// What routing needs to know from the deployment log. The caller looks it up.
#[derive(Default)]
pub struct RoutingFacts {
    /// For an answer: who asked the question it answers, and on which mission.
    pub question: Option<(String, Option<u32>)>,
    /// For a pause: who holds the mission now.
    pub mission_holder: Option<String>,
}

/// Fills in who an entry goes to, or says why it can't be added.
pub fn route_entry(entry: NewEntry, facts: RoutingFacts) -> Result<NewEntry, String> {
    let needs_mission = matches!(
        entry.kind,
        EntryKind::Handoff | EntryKind::Done | EntryKind::Blocked | EntryKind::Paused | EntryKind::Resumed
    );
    if needs_mission && entry.mission.is_none() {
        return Err(format!("a {} entry needs a mission", entry.kind.as_str()));
    }
    match entry.kind {
        EntryKind::Message if entry.to.is_none() => Err("a message needs someone to go to".into()),
        EntryKind::Message => Ok(entry),
        EntryKind::Answer => {
            let question_number = entry.answers.ok_or("an answer needs the question it answers")?;
            let (asker, question_mission) =
                facts.question.ok_or_else(|| format!("entry {question_number} isn't a question in this deployment"))?;
            Ok(NewEntry { to: Some(asker), mission: entry.mission.or(question_mission), ..entry })
        }
        EntryKind::Handoff | EntryKind::Done | EntryKind::Blocked | EntryKind::Resumed => {
            Ok(NewEntry { to: Some(COMMANDER.into()), ..entry })
        }
        EntryKind::Paused => Ok(NewEntry { to: Some(facts.mission_holder.unwrap_or_else(|| COMMANDER.into())), ..entry }),
        EntryKind::Question | EntryKind::Suggestion => Ok(NewEntry { to: Some(HUMAN.into()), ..entry }),
        EntryKind::Note | EntryKind::Postmortem => Ok(NewEntry { to: None, ..entry }),
        legion_only_kind => Err(format!("{NAME} writes {} entries itself", legion_only_kind.as_str())),
    }
}

/// How a delivered entry reads when it lands in a session.
pub fn delivery_text(entry: &Entry) -> String {
    let mission_label = entry.mission.map(|number| format!("mission {number}"));
    let on_mission = mission_label.as_ref().map(|label| format!(" {label}")).unwrap_or_default();
    let author = &entry.from;
    let text = &entry.text;
    let body = match entry.kind {
        EntryKind::MissionAdded => {
            let number = entry.mission.unwrap_or_default();
            format!("New mission {number}: {text}. Read it with the mission_read tool.")
        }
        EntryKind::Answer => format!("The human answered your question #{}: {text}", entry.answers.unwrap_or_default()),
        EntryKind::Handoff => format!("{author} handed off{on_mission}: {text}"),
        EntryKind::Done => format!("{author} reports{on_mission} done: {text}"),
        EntryKind::Blocked => format!("{author} reports{on_mission} blocked: {text}"),
        EntryKind::Paused => format!("{author} paused{on_mission}: {text}"),
        EntryKind::Resumed => format!("{author} resumed{on_mission}: {text}"),
        _ => {
            let about = mission_label.map(|label| format!(" ({label})")).unwrap_or_default();
            format!("Message from {author}{about}: {text}")
        }
    };
    format!("{} {body}", delivery_prefix())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_entry(kind: EntryKind) -> NewEntry {
        NewEntry { kind, mission: Some(1), to: None, text: "t".into(), answers: None }
    }

    fn entry(kind: EntryKind, mission: Option<u32>) -> Entry {
        Entry { id: 1, deployment: "r".into(), at_ms: 0, mission, from: "builder".into(), to: None, kind, text: "ok".into(), answers: Some(7) }
    }

    #[test]
    fn reports_go_to_the_commander() {
        for kind in [EntryKind::Handoff, EntryKind::Done, EntryKind::Blocked, EntryKind::Resumed] {
            let routed = route_entry(new_entry(kind), RoutingFacts::default()).unwrap();
            assert_eq!(routed.to.as_deref(), Some(COMMANDER), "{kind:?}");
        }
    }

    #[test]
    fn reports_need_a_mission() {
        let without_mission = NewEntry { mission: None, ..new_entry(EntryKind::Handoff) };
        assert!(route_entry(without_mission, RoutingFacts::default()).is_err());
    }

    #[test]
    fn a_pause_goes_to_whoever_holds_the_mission() {
        let facts = RoutingFacts { mission_holder: Some("builder".into()), ..Default::default() };
        assert_eq!(route_entry(new_entry(EntryKind::Paused), facts).unwrap().to.as_deref(), Some("builder"));
        let nobody = route_entry(new_entry(EntryKind::Paused), RoutingFacts::default()).unwrap();
        assert_eq!(nobody.to.as_deref(), Some(COMMANDER));
    }

    #[test]
    fn an_answer_goes_to_the_asker_on_their_mission() {
        let answer = NewEntry { mission: None, answers: Some(5), ..new_entry(EntryKind::Answer) };
        let facts = RoutingFacts { question: Some(("planner".into(), Some(3))), ..Default::default() };
        let routed = route_entry(answer, facts).unwrap();
        assert_eq!(routed.to.as_deref(), Some("planner"));
        assert_eq!(routed.mission, Some(3));
    }

    #[test]
    fn an_answer_needs_a_real_question() {
        let answer = NewEntry { answers: Some(5), ..new_entry(EntryKind::Answer) };
        assert!(route_entry(answer, RoutingFacts::default()).is_err());
        let unnumbered = NewEntry { answers: None, ..new_entry(EntryKind::Answer) };
        assert!(route_entry(unnumbered, RoutingFacts::default()).is_err());
    }

    #[test]
    fn questions_go_to_the_human_and_notes_to_no_one() {
        let question = NewEntry { to: Some("x".into()), ..new_entry(EntryKind::Question) };
        assert_eq!(route_entry(question, RoutingFacts::default()).unwrap().to.as_deref(), Some(HUMAN));
        let note = NewEntry { to: Some("x".into()), ..new_entry(EntryKind::Note) };
        assert_eq!(route_entry(note, RoutingFacts::default()).unwrap().to, None);
    }

    #[test]
    fn messages_need_an_addressee() {
        assert!(route_entry(new_entry(EntryKind::Message), RoutingFacts::default()).is_err());
        let addressed = NewEntry { to: Some("reviewer".into()), ..new_entry(EntryKind::Message) };
        assert!(route_entry(addressed, RoutingFacts::default()).is_ok());
    }

    #[test]
    fn legions_own_entries_are_refused() {
        assert!(route_entry(new_entry(EntryKind::SessionStarted), RoutingFacts::default()).is_err());
    }

    #[test]
    fn deliveries_say_who_and_what() {
        assert_eq!(delivery_text(&entry(EntryKind::Handoff, Some(2))), "[legion2] builder handed off mission 2: ok");
        assert_eq!(delivery_text(&entry(EntryKind::Message, None)), "[legion2] Message from builder: ok");
        assert_eq!(delivery_text(&entry(EntryKind::Message, Some(2))), "[legion2] Message from builder (mission 2): ok");
        assert_eq!(delivery_text(&entry(EntryKind::Answer, None)), "[legion2] The human answered your question #7: ok");
    }
}
