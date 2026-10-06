//! A folder legion2d runs, its outside folder, and the list of folders.

use std::{
    fs,
    path::{Path, PathBuf},
};

use legion2_proto::{Folder, Deployment};

use crate::{
    constants::{DATABASE_FILE_NAME, FOLDERS_FOLDER_NAME, FOLDER_REGISTRY_FILE_NAME, MISSIONS_FOLDER_NAME},
    ids::short_hash,
    setup::{operator_names, pipeline_names, read_settings},
    store::Store,
};

pub struct FolderState {
    pub path: PathBuf,
    pub name: String,
    /// Where its missions, worktrees and database live, outside the repo.
    pub outside_folder: PathBuf,
    pub store: Store,
    pub deployments: Vec<Deployment>,
}

/// One per folder: its name, plus a hash of its path so two repos with the
/// same name don't share one.
pub fn outside_folder_for(data_folder: &Path, folder: &Path, folder_name: &str) -> PathBuf {
    let path_hash = short_hash(folder.to_string_lossy().as_bytes());
    data_folder.join(FOLDERS_FOLDER_NAME).join(format!("{folder_name}-{path_hash}"))
}

impl FolderState {
    pub fn open(data_folder: &Path, path: &Path) -> Result<FolderState, String> {
        let settings = read_settings(path)?;
        let outside_folder = outside_folder_for(data_folder, path, &settings.name);
        let missions_folder = outside_folder.join(MISSIONS_FOLDER_NAME);
        fs::create_dir_all(&missions_folder).map_err(|error| format!("can't create {}: {error}", missions_folder.display()))?;
        let store = Store::open(&outside_folder.join(DATABASE_FILE_NAME))?;
        let folder_path = path.to_string_lossy().into_owned();
        let deployments = store.deployments()?.into_iter().map(|deployment| Deployment { folder: folder_path.clone(), ..deployment }).collect();
        Ok(FolderState { path: path.to_path_buf(), name: settings.name, outside_folder, store, deployments })
    }

    pub fn info(&self) -> Folder {
        Folder {
            path: self.path.to_string_lossy().into_owned(),
            name: self.name.clone(),
            pipelines: pipeline_names(&self.path),
            operators: operator_names(&self.path),
        }
    }

    /// A folder is named by its name or its path.
    pub fn is_named(&self, key: &str) -> bool {
        let is_same_path = PathBuf::from(key).canonicalize().is_ok_and(|path| path == self.path);
        self.name == key || is_same_path
    }
}

pub fn read_folder_registry(data_folder: &Path) -> Vec<PathBuf> {
    fs::read_to_string(data_folder.join(FOLDER_REGISTRY_FILE_NAME))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<PathBuf>>(&text).ok())
        .unwrap_or_default()
}

pub fn write_folder_registry(data_folder: &Path, folders: &[FolderState]) -> Result<(), String> {
    let paths: Vec<&PathBuf> = folders.iter().map(|folder| &folder.path).collect();
    let text = serde_json::to_string_pretty(&paths).map_err(|error| error.to_string())?;
    let registry_path = data_folder.join(FOLDER_REGISTRY_FILE_NAME);
    fs::write(&registry_path, text).map_err(|error| format!("can't write {}: {error}", registry_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_name_different_paths_get_different_outside_folders() {
        let data = Path::new("/data");
        let first = outside_folder_for(data, Path::new("/a/app"), "app");
        let second = outside_folder_for(data, Path::new("/b/app"), "app");
        assert_ne!(first, second);
        assert!(first.starts_with("/data/folders"));
        assert!(first.file_name().unwrap().to_string_lossy().starts_with("app-"));
    }

    #[test]
    fn a_missing_registry_means_no_folders() {
        assert!(read_folder_registry(Path::new("/nowhere/at/all")).is_empty());
    }
}
