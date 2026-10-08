//! Starting, listing and closing deployments.

use std::sync::Arc;

use legion2_proto::{EntryKind, NewEntry, Reply, Deployment, COMMANDER, HUMAN};

use crate::{daemon::Daemon, ids::new_id, naming::unique_deployment_name, setup::read_pipeline, store::now_ms};

impl Daemon {
    pub fn start_deployment(self: &Arc<Self>, folder_key: &str, pipeline: &str, name: Option<String>) -> Result<Reply, String> {
        let deployment = {
            let mut state = self.state.lock().unwrap();
            let folder_index = state.find_folder(folder_key)?;
            let folder = &mut state.folders[folder_index];
            read_pipeline(&folder.path, pipeline)?;
            let taken_names: Vec<String> = folder.deployments.iter().map(|deployment| deployment.name.clone()).collect();
            let deployment_name = unique_deployment_name(&name.unwrap_or_else(|| pipeline.to_string()), &taken_names);
            let deployment = Deployment {
                id: new_id(),
                name: deployment_name,
                folder: folder.path.to_string_lossy().into_owned(),
                pipeline: pipeline.to_string(),
                started_ms: now_ms(),
                closed_ms: None,
            };
            folder.store.add_deployment(&deployment)?;
            folder.deployments.push(deployment.clone());
            deployment
        };
        let started = NewEntry {
            kind: EntryKind::DeploymentStarted,
            mission: None,
            to: None,
            text: format!("deployment {} started on the {} pipeline", deployment.name, deployment.pipeline),
            answers: None,
        };
        self.post_entry(&deployment.id, HUMAN, started)?;
        self.start_session(&deployment.id, COMMANDER, None, HUMAN, None)?;
        self.refresh_channel_deployments();
        Ok(Reply::Deployment { deployment })
    }

    pub fn list_deployments(&self, folder_key: Option<&str>) -> Reply {
        let state = self.state.lock().unwrap();
        let deployments = state
            .folders
            .iter()
            .filter(|folder| folder_key.is_none_or(|key| folder.is_named(key)))
            .flat_map(|folder| folder.deployments.iter().cloned())
            .collect();
        Reply::Deployments { deployments }
    }

    /// A new name, unique among the folder's deployments. Its ID, missions,
    /// log and sessions stay as they are.
    pub fn rename_deployment(&self, deployment_key: &str, name: &str) -> Result<Reply, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("give the deployment a name".into());
        }
        let (deployment, old_name) = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_deployment(deployment_key)?;
            let folder = &mut state.folders[folder_index];
            if folder.deployments.iter().any(|known| known.name == name && known.id != deployment.id) {
                return Err(format!("{} already has a deployment called {name}", folder.name));
            }
            folder.store.rename_deployment(&deployment.id, name)?;
            folder.deployments.iter_mut().filter(|known| known.id == deployment.id).for_each(|known| known.name = name.to_string());
            (Deployment { name: name.to_string(), ..deployment.clone() }, deployment.name)
        };
        let renamed = NewEntry { kind: EntryKind::Note, mission: None, to: None, text: format!("deployment {old_name} renamed to {name}"), answers: None };
        self.post_entry(&deployment.id, HUMAN, renamed)?;
        self.refresh_channel_deployments();
        Ok(Reply::Text { text: format!("deployment {old_name} ({}) renamed to {}", deployment.id, deployment.name) })
    }

    /// Ends every session in the deployment. A closed deployment isn't brought back after
    /// a restart and takes no new missions or sessions.
    pub fn close_deployment(&self, deployment_key: &str) -> Result<Reply, String> {
        let deployment = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let closed_ms = now_ms();
            let folder = &mut state.folders[folder_index];
            folder.store.close_deployment(&deployment.id, closed_ms)?;
            folder.deployments.iter_mut().filter(|known| known.id == deployment.id).for_each(|known| known.closed_ms = Some(closed_ms));
            state.sessions.values_mut().filter(|session| session.deployment == deployment.id).try_for_each(|session| {
                session.is_stopping = true;
                session.end()
            })?;
            deployment
        };
        let closed = NewEntry { kind: EntryKind::DeploymentClosed, mission: None, to: None, text: format!("deployment {} closed", deployment.name), answers: None };
        self.post_entry(&deployment.id, HUMAN, closed)?;
        self.refresh_channel_deployments();
        Ok(Reply::Done)
    }
}
