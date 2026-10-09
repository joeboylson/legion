//! Finding deployments, folders and sessions in legion2d's state.

use legion2_proto::{Mission, Deployment};

use crate::{daemon::State, sessions::Session};

impl State {
    /// A deployment by its ID, or by its name when only one folder has a deployment by
    /// that name. Returns the index of its folder too.
    pub fn find_deployment(&self, key: &str) -> Result<(usize, Deployment), String> {
        let all_deployments = || self.folders.iter().enumerate().flat_map(|(index, folder)| folder.deployments.iter().map(move |deployment| (index, deployment)));
        if let Some((index, deployment)) = all_deployments().find(|(_, deployment)| deployment.id == key) {
            return Ok((index, deployment.clone()));
        }
        let named: Vec<(usize, &Deployment)> = all_deployments().filter(|(_, deployment)| deployment.name == key).collect();
        match named.as_slice() {
            [(index, deployment)] => Ok((*index, (*deployment).clone())),
            [] => Err(format!("no deployment {key:?}")),
            many => {
                let choices = many.iter().map(|(_, deployment)| format!("{} ({})", deployment.id, deployment.folder)).collect::<Vec<_>>().join(", ");
                Err(format!("more than one deployment is called {key:?}; use its ID: {choices}"))
            }
        }
    }

    pub fn find_open_deployment(&self, key: &str) -> Result<(usize, Deployment), String> {
        let (index, deployment) = self.find_deployment(key)?;
        match deployment.closed_ms {
            Some(_) => Err(format!("deployment {} is closed", deployment.name)),
            None => Ok((index, deployment)),
        }
    }

    pub fn find_folder(&self, key: &str) -> Result<usize, String> {
        let matching: Vec<usize> = (0..self.folders.len()).filter(|&index| self.folders[index].is_named(key)).collect();
        match matching.as_slice() {
            [index] => Ok(*index),
            [] => Err(format!("no folder {key:?}; set it up with `legion2 setup` inside it")),
            _ => Err(format!("more than one folder matches {key:?}; use its path")),
        }
    }

    pub fn session_in_deployment(&mut self, deployment_id: &str, position: &str) -> Result<&mut Session, String> {
        self.sessions
            .values_mut()
            .find(|session| session.deployment == deployment_id && session.position == position)
            .ok_or_else(|| format!("{position} isn't running in this deployment"))
    }

    pub fn running_positions(&self, deployment_id: &str) -> Vec<String> {
        self.sessions.values().filter(|session| session.deployment == deployment_id).map(|session| session.position.clone()).collect()
    }

    pub fn mission_with_body(&self, folder_index: usize, deployment_id: &str, number: u32) -> Result<(Mission, String), String> {
        let mission = self.folders[folder_index].store.mission(deployment_id, number)?;
        let body = std::fs::read_to_string(&mission.file).map_err(|error| format!("can't read {}: {error}", mission.file))?;
        Ok((mission, body))
    }
}
