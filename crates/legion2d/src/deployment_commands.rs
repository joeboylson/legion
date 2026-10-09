//! Starting, listing and closing deployments.

use std::sync::Arc;

use legion2_proto::{pipeline_label, EntryKind, NewEntry, Reply, Deployment, COMMANDER, HUMAN};

use std::path::Path;

use crate::{daemon::Daemon, git::drop_worktree, ids::new_id, naming::unique_deployment_name, setup::read_pipeline, store::now_ms};

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
            text: format!("deployment {} started {}", deployment.name, legion2_proto::pipeline_phrase(&deployment.pipeline)),
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

    /// Opens a closed deployment again and starts its commander, which picks
    /// the work up from the log. Its pipeline must still be able to run.
    pub fn reopen_deployment(self: &Arc<Self>, deployment_key: &str) -> Result<Reply, String> {
        let deployment = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_deployment(deployment_key)?;
            if deployment.closed_ms.is_none() {
                return Err(format!("{} is open already", deployment.name));
            }
            let folder = &mut state.folders[folder_index];
            read_pipeline(&folder.path, &deployment.pipeline)?;
            folder.store.reopen_deployment(&deployment.id)?;
            folder.deployments.iter_mut().filter(|known| known.id == deployment.id).for_each(|known| known.closed_ms = None);
            Deployment { closed_ms: None, ..deployment.clone() }
        };
        let reopened = NewEntry { kind: EntryKind::Note, mission: None, to: None, text: format!("deployment {} reopened", deployment.name), answers: None };
        self.post_entry(&deployment.id, HUMAN, reopened)?;
        self.start_session(&deployment.id, COMMANDER, None, HUMAN, None)?;
        self.refresh_channel_deployments();
        Ok(Reply::Deployment { deployment })
    }

    /// Deletes a closed deployment for good: its log, missions, mission files
    /// and checkouts. Its branches stay, so committed work isn't lost.
    pub fn delete_deployment(&self, deployment_key: &str) -> Result<Reply, String> {
        let (deployment, folder_path, mission_files, checkouts) = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_deployment(deployment_key)?;
            if deployment.closed_ms.is_none() {
                return Err(format!("{} is open: close it first", deployment.name));
            }
            let folder = &mut state.folders[folder_index];
            let missions = folder.store.missions(&deployment.id)?;
            let checkouts: Vec<String> = missions
                .iter()
                .map(|mission| {
                    let worktree = folder.store.worktree(mission.number)?.filter(|worktree| !worktree.is_removed).map(|worktree| worktree.path);
                    let parts = folder.store.parts(mission.number)?.into_iter().map(|part| part.path);
                    Ok::<_, String>(worktree.into_iter().chain(parts).collect::<Vec<_>>())
                })
                .collect::<Result<Vec<_>, _>>()?
                .concat();
            folder.store.delete_deployment(&deployment.id)?;
            folder.deployments.retain(|known| known.id != deployment.id);
            (deployment, folder.path.clone(), missions.into_iter().map(|mission| mission.file).collect::<Vec<_>>(), checkouts)
        };
        let leftovers: Vec<String> = checkouts
            .iter()
            .filter_map(|path| drop_worktree(&folder_path, Path::new(path)).err().map(|problem| format!("{path}: {problem}")))
            .chain(mission_files.iter().filter_map(|file| std::fs::remove_file(file).err().map(|problem| format!("{file}: {problem}"))))
            .collect();
        let summary = format!("deleted {}: {} missions, its log and {} checkouts; its branches stay in git", deployment.name, mission_files.len(), checkouts.len());
        Ok(Reply::Text { text: if leftovers.is_empty() { summary } else { format!("{summary}. Couldn't remove: {}", leftovers.join("; ")) } })
    }

    /// Moves the deployment to another pipeline, once that pipeline can run
    /// here. The next team check tells its commander the new team.
    pub fn repipe_deployment(&self, deployment_key: &str, pipeline: &str) -> Result<Reply, String> {
        let pipeline = pipeline.trim();
        let (deployment, old_pipeline) = {
            let mut state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let folder = &mut state.folders[folder_index];
            read_pipeline(&folder.path, pipeline)?;
            if deployment.pipeline == pipeline {
                return Err(format!("{} already runs {}", deployment.name, pipeline_label(pipeline)));
            }
            folder.store.set_deployment_pipeline(&deployment.id, pipeline)?;
            folder.deployments.iter_mut().filter(|known| known.id == deployment.id).for_each(|known| known.pipeline = pipeline.to_string());
            (Deployment { pipeline: pipeline.to_string(), ..deployment.clone() }, deployment.pipeline)
        };
        let text = format!("deployment {} moved from {} to {}", deployment.name, pipeline_label(&old_pipeline), pipeline_label(pipeline));
        self.post_entry(&deployment.id, HUMAN, NewEntry { kind: EntryKind::Note, mission: None, to: None, text: text.clone(), answers: None })?;
        self.refresh_channel_deployments();
        Ok(Reply::Text { text })
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
