//! Adding to and reading the run log.

use legion2_proto::{EntryKind, LogFilter, NewEntry, Reply};

use crate::{
    daemon::Daemon,
    entry_routing::{route_entry, RoutingFacts},
};

impl Daemon {
    pub fn post_from_caller(&self, run_key: &str, author: &str, entry: NewEntry) -> Result<Reply, String> {
        let (run_id, routed) = {
            let state = self.state.lock().unwrap();
            let (folder_index, run) = state.find_run(run_key)?;
            let store = &state.folders[folder_index].store;
            let mission = entry.mission.map(|number| store.mission(&run.id, number)).transpose()?;
            let question = match entry.answers {
                Some(question_id) => store
                    .entry(&run.id, question_id)?
                    .filter(|asked| asked.kind == EntryKind::Question)
                    .map(|asked| (asked.from, asked.mission)),
                None => None,
            };
            let facts = RoutingFacts { question, mission_holder: mission.and_then(|known| known.holder) };
            (run.id, route_entry(entry, facts)?)
        };
        Ok(Reply::Entry { entry: self.post_entry(&run_id, author, routed)? })
    }

    pub fn read_log(&self, run_key: &str, filter: &LogFilter) -> Result<Reply, String> {
        let state = self.state.lock().unwrap();
        let (folder_index, run) = state.find_run(run_key)?;
        Ok(Reply::Entries { entries: state.folders[folder_index].store.entries(&run.id, filter)? })
    }
}
