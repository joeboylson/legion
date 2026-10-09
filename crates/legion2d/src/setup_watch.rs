//! Noticing edits to a folder's `.legion2/` setup made outside Legion, so
//! the app shows them at once. legion2d reads the setup fresh each time it
//! needs it; only the app has to be told. Each folder's setup is summed up
//! as every file's path, size and change time, and checked every few
//! seconds: a setup is a handful of small files.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use legion2_proto::Event;

use crate::{daemon::Daemon, setup::setup_folder};

/// One file in a setup: its path inside it, its size, and when it last changed.
type FileMark = (PathBuf, u64, u128);

/// Every file under `folder`, sorted, so two looks at an unchanged setup match.
pub fn setup_marks(folder: &Path) -> Vec<FileMark> {
    fn walk(root: &Path, current: &Path, marks: &mut Vec<FileMark>) {
        let Ok(entries) = fs::read_dir(current) else { return };
        entries.flatten().for_each(|entry| {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else { return };
            if metadata.is_dir() {
                return walk(root, &path, marks);
            }
            let changed = metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map_or(0, |since| since.as_nanos());
            marks.push((path.strip_prefix(root).unwrap_or(&path).to_path_buf(), metadata.len(), changed));
        });
    }
    let mut marks = Vec::new();
    walk(folder, folder, &mut marks);
    marks.sort();
    marks
}

/// The folders whose setup differs from the last look. A folder seen for the
/// first time isn't a change: the app read it when it was added.
pub fn changed_setups(last: &HashMap<PathBuf, Vec<FileMark>>, now: &HashMap<PathBuf, Vec<FileMark>>) -> Vec<PathBuf> {
    let mut changed: Vec<PathBuf> = now.iter().filter(|(folder, marks)| last.get(*folder).is_some_and(|before| before != *marks)).map(|(folder, _)| folder.clone()).collect();
    changed.sort();
    changed
}

impl Daemon {
    /// Looks at every folder's setup, tells the app about the ones that
    /// changed since `last`, and returns this look for the next.
    pub fn report_setup_changes(&self, last: &HashMap<PathBuf, Vec<FileMark>>) -> HashMap<PathBuf, Vec<FileMark>> {
        let folders: Vec<PathBuf> = self.state.lock().unwrap().folders.iter().map(|folder| folder.path.clone()).collect();
        let now: HashMap<PathBuf, Vec<FileMark>> = folders.into_iter().map(|folder| {
            let marks = setup_marks(&setup_folder(&folder));
            (folder, marks)
        }).collect();
        changed_setups(last, &now).into_iter().for_each(|folder| {
            // No one watching is fine.
            let _ = self.events.send(Event::SetupChanged { folder: folder.to_string_lossy().into_owned() });
        });
        now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edit_an_addition_or_a_removal_is_a_change() {
        let folder = std::env::temp_dir().join(format!("legion2-watch-{}", crate::ids::new_id()));
        fs::create_dir_all(folder.join("operators/poet")).unwrap();
        fs::write(folder.join("legion.json"), "{}").unwrap();
        let first = setup_marks(&folder);
        assert_eq!(first, setup_marks(&folder));
        fs::write(folder.join("operators/poet/definition.md"), "# Poet").unwrap();
        let added = setup_marks(&folder);
        assert_ne!(first, added);
        fs::write(folder.join("operators/poet/definition.md"), "# Poet, longer").unwrap();
        assert_ne!(added, setup_marks(&folder));
        fs::remove_dir_all(folder.join("operators/poet")).unwrap();
        assert_eq!(first, setup_marks(&folder));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn only_folders_seen_before_and_different_now_have_changed() {
        let mark = |name: &str| vec![(PathBuf::from(name), 1, 1)];
        let last = HashMap::from([(PathBuf::from("/a"), mark("x")), (PathBuf::from("/b"), mark("y"))]);
        let now = HashMap::from([(PathBuf::from("/a"), mark("x")), (PathBuf::from("/b"), mark("z")), (PathBuf::from("/new"), mark("w"))]);
        assert_eq!(changed_setups(&last, &now), vec![PathBuf::from("/b")]);
    }
}
