//! Adding and listing folders.

use std::path::PathBuf;

use legion2_proto::Reply;

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
}
