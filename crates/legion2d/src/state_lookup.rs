//! Finding runs, folders and sessions in legion2d's state.

use legion2_proto::{Mission, Run};

use crate::{daemon::State, sessions::Session};

impl State {
    /// A run by its ID, or by its name when only one folder has a run by
    /// that name. Returns the index of its folder too.
    pub fn find_run(&self, key: &str) -> Result<(usize, Run), String> {
        let all_runs = || self.folders.iter().enumerate().flat_map(|(index, folder)| folder.runs.iter().map(move |run| (index, run)));
        if let Some((index, run)) = all_runs().find(|(_, run)| run.id == key) {
            return Ok((index, run.clone()));
        }
        let named: Vec<(usize, &Run)> = all_runs().filter(|(_, run)| run.name == key).collect();
        match named.as_slice() {
            [(index, run)] => Ok((*index, (*run).clone())),
            [] => Err(format!("no run {key:?}")),
            many => {
                let choices = many.iter().map(|(_, run)| format!("{} ({})", run.id, run.folder)).collect::<Vec<_>>().join(", ");
                Err(format!("more than one run is called {key:?}; use its ID: {choices}"))
            }
        }
    }

    pub fn find_open_run(&self, key: &str) -> Result<(usize, Run), String> {
        let (index, run) = self.find_run(key)?;
        match run.closed_ms {
            Some(_) => Err(format!("run {} is closed", run.name)),
            None => Ok((index, run)),
        }
    }

    pub fn find_folder(&self, key: &str) -> Result<usize, String> {
        let matching: Vec<usize> = (0..self.folders.len()).filter(|&index| self.folders[index].is_named(key)).collect();
        match matching.as_slice() {
            [index] => Ok(*index),
            [] => Err(format!("no folder {key:?}; add it with `legion2 add <path>`")),
            _ => Err(format!("more than one folder matches {key:?}; use its path")),
        }
    }

    pub fn session_in_run(&mut self, run_id: &str, position: &str) -> Result<&mut Session, String> {
        self.sessions
            .values_mut()
            .find(|session| session.run == run_id && session.position == position)
            .ok_or_else(|| format!("{position} isn't running in this run"))
    }

    pub fn running_positions(&self, run_id: &str) -> Vec<String> {
        self.sessions.values().filter(|session| session.run == run_id).map(|session| session.position.clone()).collect()
    }

    pub fn mission_with_body(&self, folder_index: usize, run_id: &str, number: u32) -> Result<(Mission, String), String> {
        let mission = self.folders[folder_index].store.mission(run_id, number)?;
        let body = std::fs::read_to_string(&mission.file).map_err(|error| format!("can't read {}: {error}", mission.file))?;
        Ok((mission, body))
    }
}
