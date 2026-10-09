//! Who an entry goes to, and how a delivered entry reads in a session.

use legion2_proto::{Entry, EntryKind, NewEntry, COMMANDER, HUMAN, NAME};

use crate::{constants::delivery_prefix, naming::operator_of_position};

/// What routing needs to know from the deployment log. The caller looks it up.
#[derive(Default)]
pub struct RoutingFacts {
    /// For an answer: who sent the question or decision it answers, and on which mission.
    pub question: Option<(String, Option<u32>)>,
    /// For an answer: the question's mission is finished. The answer is kept
    /// in the log but goes to no one: whoever holds the asker's position now
    /// works on something else.
    pub question_mission_is_done: bool,
    /// For a pause: who holds the mission now.
    pub mission_holder: Option<String>,
    /// For a message to an operator's bare name: the copy working the
    /// sender's mission, when that's another copy.
    pub mission_copy: Option<String>,
}

/// An operator's bare name is one particular copy (`shipper`), often on
/// another mission than the sender's. A message about a mission goes to the
/// copy working it instead. A numbered copy (`shipper-2`) is taken as meant.
pub fn copy_on_mission(to: &str, mission: Option<u32>, sessions: &[(String, Option<u32>)]) -> Option<String> {
    let mission = mission?;
    let is_bare_name = operator_of_position(to) == to;
    let named_is_on_it = sessions.iter().any(|(position, on)| position == to && *on == Some(mission));
    if !is_bare_name || named_is_on_it {
        return None;
    }
    sessions
        .iter()
        .find(|(position, on)| operator_of_position(position) == to && *on == Some(mission))
        .map(|(position, _)| position.clone())
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
        EntryKind::Message => Ok(match facts.mission_copy {
            Some(copy) => NewEntry { to: Some(copy), ..entry },
            None => entry,
        }),
        EntryKind::Answer => {
            let question_number = entry.answers.ok_or("an answer needs the question it answers")?;
            let (asker, question_mission) =
                facts.question.ok_or_else(|| format!("entry {question_number} isn't a question or decision in this deployment"))?;
            let to = if facts.question_mission_is_done { None } else { Some(asker) };
            Ok(NewEntry { to, mission: entry.mission.or(question_mission), ..entry })
        }
        EntryKind::Handoff | EntryKind::Done | EntryKind::Blocked | EntryKind::Resumed => {
            Ok(NewEntry { to: Some(COMMANDER.into()), ..entry })
        }
        EntryKind::Paused => Ok(NewEntry { to: Some(facts.mission_holder.unwrap_or_else(|| COMMANDER.into())), ..entry }),
        EntryKind::Question | EntryKind::Suggestion | EntryKind::Decision => Ok(NewEntry { to: Some(HUMAN.into()), ..entry }),
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
        EntryKind::Answer => format!("The admin answered your #{}: {text}", entry.answers.unwrap_or_default()),
        EntryKind::Handoff => format!("{author} handed off{on_mission}: {text}"),
        EntryKind::Done => format!("{author} reports{on_mission} done: {text}"),
        EntryKind::Blocked => format!("{author} reports{on_mission} blocked: {text}"),
        EntryKind::Paused => format!("{author} paused{on_mission}: {text}"),
        EntryKind::Resumed => format!("{author} resumed{on_mission}: {text}"),
        EntryKind::Announcement => format!("Heads-up, no reply needed: {text}"),
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
        let decision = NewEntry { to: Some("commander".into()), ..new_entry(EntryKind::Decision) };
        assert_eq!(route_entry(decision, RoutingFacts::default()).unwrap().to.as_deref(), Some(HUMAN));
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
    fn an_answer_on_a_finished_mission_is_logged_but_goes_to_no_one() {
        let mut answer = new_entry(EntryKind::Answer);
        answer.answers = Some(7);
        let facts = RoutingFacts { question: Some(("shipper-3".into(), Some(30))), question_mission_is_done: true, ..Default::default() };
        let routed = route_entry(answer, facts).unwrap();
        assert_eq!(routed.to, None);
        assert_eq!(routed.mission, Some(1));
    }

    #[test]
    fn a_message_to_a_bare_name_goes_to_the_copy_on_the_mission() {
        let sessions = [("shipper".to_string(), Some(28)), ("shipper-2".to_string(), Some(29)), ("shipper-3".to_string(), Some(30))];
        assert_eq!(copy_on_mission("shipper", Some(29), &sessions), Some("shipper-2".into()));
        assert_eq!(copy_on_mission("shipper", Some(28), &sessions), None);
        assert_eq!(copy_on_mission("shipper-3", Some(29), &sessions), None);
        assert_eq!(copy_on_mission("shipper", None, &sessions), None);
        assert_eq!(copy_on_mission("shipper", Some(31), &sessions), None);
        let mut message = new_entry(EntryKind::Message);
        message.to = Some("shipper".into());
        let facts = RoutingFacts { mission_copy: Some("shipper-2".into()), ..Default::default() };
        assert_eq!(route_entry(message, facts).unwrap().to.as_deref(), Some("shipper-2"));
    }

    #[test]
    fn deliveries_say_who_and_what() {
        assert_eq!(delivery_text(&entry(EntryKind::Handoff, Some(2))), "[legion2] builder handed off mission 2: ok");
        assert_eq!(delivery_text(&entry(EntryKind::Message, None)), "[legion2] Message from builder: ok");
        assert_eq!(delivery_text(&entry(EntryKind::Message, Some(2))), "[legion2] Message from builder (mission 2): ok");
        assert_eq!(delivery_text(&entry(EntryKind::Answer, None)), "[legion2] The admin answered your #7: ok");
        assert_eq!(delivery_text(&entry(EntryKind::Announcement, None)), "[legion2] Heads-up, no reply needed: ok");
    }
}
