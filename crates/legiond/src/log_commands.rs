//! Adding to and reading the deployment log.

use legion_proto::{EntryKind, LogFilter, MissionStatus, NewEntry, Reply, HUMAN};

use crate::{
    daemon::Daemon,
    entry_routing::{copy_on_mission, route_entry, RoutingFacts},
    opencode_turn_end::ends_the_turn,
};

impl Daemon {
    pub fn post_from_caller(&self, deployment_key: &str, author: &str, entry: NewEntry) -> Result<Reply, String> {
        let mut unreachable = None;
        let (deployment_id, routed) = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_deployment(deployment_key)?;
            let store = &state.folders[folder_index].store;
            let mission = entry.mission.map(|number| store.mission(&deployment.id, number)).transpose()?;
            let question = match entry.answers {
                Some(question_id) => store
                    .entry(&deployment.id, question_id)?
                    .filter(|asked| matches!(asked.kind, EntryKind::Question | EntryKind::Decision))
                    .map(|asked| (asked.from, asked.mission)),
                None => None,
            };
            let deployment_sessions: Vec<(String, Option<u32>)> = state
                .sessions
                .values()
                .filter(|session| session.deployment == deployment.id && !session.is_stopping())
                .map(|session| (session.position.clone(), session.mission))
                .collect();
            let author_mission = deployment_sessions.iter().find(|(position, _)| position == author).and_then(|(_, on)| *on);
            let mission_copy = match (&entry.kind, &entry.to) {
                (EntryKind::Message, Some(to)) => copy_on_mission(to, entry.mission.or(author_mission), &deployment_sessions),
                _ => None,
            };
            let target = mission_copy.as_ref().or(entry.to.as_ref());
            if let (EntryKind::Message, Some(to)) = (&entry.kind, target) {
                if to != HUMAN && !deployment_sessions.iter().any(|(position, _)| position == to) {
                    unreachable = Some(to.clone());
                }
            }
            let recipient_mission = match (&entry.kind, mission_copy.as_ref().or(entry.to.as_ref())) {
                (EntryKind::Message, Some(to)) => deployment_sessions.iter().find(|(position, _)| position == to).and_then(|(_, on)| *on),
                _ => None,
            };
            let question_mission_is_done = question
                .as_ref()
                .and_then(|(_, asked_on)| *asked_on)
                .and_then(|number| store.mission(&deployment.id, number).ok())
                .is_some_and(|asked_on| asked_on.status == MissionStatus::Done);
            let facts = RoutingFacts { question, question_mission_is_done, mission_holder: mission.and_then(|known| known.holder), mission_copy, recipient_mission };
            (deployment.id, route_entry(entry, facts)?)
        };
        if let Some(to) = unreachable {
            let note = format!("{author}'s message to {to} wasn't sent: {to} isn't running");
            self.post_legion_entry(&deployment_id, NewEntry { kind: EntryKind::Note, mission: routed.mission, to: None, text: note, answers: None });
            return Err(format!("{to} isn't running: hand off to it instead, or ask the commander to start it"));
        }
        let posted = self.post_entry(&deployment_id, author, routed)?;
        if let (EntryKind::Answer, Some(question)) = (posted.kind, posted.answers) {
            self.settle_channel_hold(&deployment_id, question, &posted.text);
        }
        if ends_the_turn(posted.kind) {
            self.end_opencode_turn(&deployment_id, author);
        }
        Ok(Reply::Entry { entry: posted })
    }

    pub fn read_log(&self, deployment_key: &str, filter: &LogFilter) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let (folder_index, deployment) = state.find_deployment(deployment_key)?;
        Ok(Reply::Entries { entries: state.folders[folder_index].store.entries(&deployment.id, filter)? })
    }
}
