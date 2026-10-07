//! Adding to and reading the deployment log.

use legion2_proto::{EntryKind, LogFilter, NewEntry, Reply};

use crate::{
    daemon::Daemon,
    entry_routing::{route_entry, RoutingFacts},
};

impl Daemon {
    pub fn post_from_caller(&self, deployment_key: &str, author: &str, entry: NewEntry) -> Result<Reply, String> {
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
            let facts = RoutingFacts { question, mission_holder: mission.and_then(|known| known.holder) };
            (deployment.id, route_entry(entry, facts)?)
        };
        Ok(Reply::Entry { entry: self.post_entry(&deployment_id, author, routed)? })
    }

    pub fn read_log(&self, deployment_key: &str, filter: &LogFilter) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let (folder_index, deployment) = state.find_deployment(deployment_key)?;
        Ok(Reply::Entries { entries: state.folders[folder_index].store.entries(&deployment.id, filter)? })
    }
}
