//! Starting, listing and closing runs.

use std::sync::Arc;

use legion2_proto::{EntryKind, NewEntry, Reply, Run, COMMANDER, HUMAN};

use crate::{daemon::Daemon, ids::new_id, naming::unique_run_name, setup::read_pipeline, store::now_ms};

impl Daemon {
    pub fn start_run(self: &Arc<Self>, folder_key: &str, pipeline: &str, name: Option<String>) -> Result<Reply, String> {
        let run = {
            let mut state = self.state.lock().unwrap();
            let folder_index = state.find_folder(folder_key)?;
            let folder = &mut state.folders[folder_index];
            read_pipeline(&folder.path, pipeline)?;
            let taken_names: Vec<String> = folder.runs.iter().map(|run| run.name.clone()).collect();
            let run_name = unique_run_name(&name.unwrap_or_else(|| pipeline.to_string()), &taken_names);
            let run = Run {
                id: new_id(),
                name: run_name,
                folder: folder.path.to_string_lossy().into_owned(),
                pipeline: pipeline.to_string(),
                started_ms: now_ms(),
                closed_ms: None,
            };
            folder.store.add_run(&run)?;
            folder.runs.push(run.clone());
            run
        };
        let started = NewEntry {
            kind: EntryKind::RunStarted,
            mission: None,
            to: None,
            text: format!("run {} started on the {} pipeline", run.name, run.pipeline),
            answers: None,
        };
        self.post_entry(&run.id, HUMAN, started)?;
        self.start_session(&run.id, COMMANDER, None, HUMAN, None)?;
        Ok(Reply::Run { run })
    }

    pub fn list_runs(&self, folder_key: Option<&str>) -> Reply {
        let state = self.state.lock().unwrap();
        let runs = state
            .folders
            .iter()
            .filter(|folder| folder_key.is_none_or(|key| folder.is_named(key)))
            .flat_map(|folder| folder.runs.iter().cloned())
            .collect();
        Reply::Runs { runs }
    }

    /// Ends every session in the run. A closed run isn't brought back after
    /// a restart and takes no new missions or sessions.
    pub fn close_run(&self, run_key: &str) -> Result<Reply, String> {
        let run = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, run) = state.find_open_run(run_key)?;
            let closed_ms = now_ms();
            let folder = &mut state.folders[folder_index];
            folder.store.close_run(&run.id, closed_ms)?;
            folder.runs.iter_mut().filter(|known| known.id == run.id).for_each(|known| known.closed_ms = Some(closed_ms));
            state.sessions.values_mut().filter(|session| session.run == run.id).try_for_each(|session| {
                session.is_stopping = true;
                session.end()
            })?;
            run
        };
        let closed = NewEntry { kind: EntryKind::RunClosed, mission: None, to: None, text: format!("run {} closed", run.name), answers: None };
        self.post_entry(&run.id, HUMAN, closed)?;
        Ok(Reply::Done)
    }
}
