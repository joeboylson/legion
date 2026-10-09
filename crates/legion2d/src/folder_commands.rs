//! Adding, listing and removing folders.

use std::path::PathBuf;

use legion2_proto::{Deployment, Reply};

use crate::{
    daemon::Daemon,
    folder_state::{write_folder_registry, FolderState},
    setup::ensure_setup,
};

impl Daemon {
    pub fn add_folder(&self, path: &str) -> Result<Reply, String> {
        let folder_path = PathBuf::from(path).canonicalize().map_err(|error| format!("{path}: {error}"))?;
        if !folder_path.is_dir() {
            return Err(format!("{} isn't a folder", folder_path.display()));
        }
        let mut state = self.state.lock().unwrap();
        if let Some(known) = state.folders.iter().find(|folder| folder.path == folder_path) {
            return Ok(Reply::Folder { folder: known.info(), created_setup: false });
        }
        let created_setup = ensure_setup(&folder_path)?;
        let folder = FolderState::open(&self.config.data_folder, &folder_path)?;
        let info = folder.info();
        state.folders.push(folder);
        write_folder_registry(&self.config.data_folder, &state.folders)?;
        Ok(Reply::Folder { folder: info, created_setup })
    }

    pub fn list_folders(&self) -> Reply {
        let state = self.state.lock().unwrap();
        Reply::Folders { folders: state.folders.iter().map(FolderState::info).collect() }
    }

    /// Only the list forgets the folder: its outside folder keeps the
    /// missions and log, so adding it again brings them back.
    pub fn remove_folder(&self, folder_key: &str) -> Result<Reply, String> {
        let mut state = self.state.lock().unwrap();
        let folder_index = state.find_folder(folder_key)?;
        let folder = &state.folders[folder_index];
        if let Some(refusal) = open_deployments_refusal(&folder.name, &folder.deployments) {
            return Err(refusal);
        }
        let removed = state.folders.remove(folder_index);
        write_folder_registry(&self.config.data_folder, &state.folders)?;
        Ok(Reply::Text {
            text: format!("removed {} ({}); its missions and log stay in {}", removed.name, removed.path.display(), removed.outside_folder.display()),
        })
    }
}

fn open_deployments_refusal(folder_name: &str, deployments: &[Deployment]) -> Option<String> {
    let open: Vec<&str> = deployments.iter().filter(|deployment| deployment.closed_ms.is_none()).map(|deployment| deployment.name.as_str()).collect();
    if open.is_empty() {
        return None;
    }
    Some(format!("{folder_name} has open deployments: {}. Close them first", open.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment(name: &str, closed_ms: Option<i64>) -> Deployment {
        Deployment { id: name.into(), name: name.into(), folder: "/app".into(), pipeline: "feature".into(), started_ms: 0, closed_ms }
    }

    #[test]
    fn a_folder_with_open_deployments_isnt_removed() {
        let deployments = [deployment("test-1", Some(5)), deployment("test-2", None), deployment("docs", None)];
        assert_eq!(open_deployments_refusal("app", &deployments).as_deref(), Some("app has open deployments: test-2, docs. Close them first"));
        assert_eq!(open_deployments_refusal("app", &deployments[..1]), None);
        assert_eq!(open_deployments_refusal("app", &[]), None);
    }
}
