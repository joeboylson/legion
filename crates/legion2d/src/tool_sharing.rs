//! Sharing a tool mid-mission: copied into the folder's own `.<NAME>/tools/`
//! (not a mission's worktree), committed there, and announced to everyone
//! running in the deployment, so the next operator uses it straight away.

use std::{fs, path::Path};

use legion2_proto::{EntryKind, NewEntry, Reply};

use crate::{
    daemon::Daemon,
    git::{commit_paths, is_git_repo},
    shared_tools::shared_tools_folder,
};

pub fn tool_announcement(sharer: &str, tool_path: &Path, summary: &str) -> String {
    format!("{sharer} shared a tool: {} ({summary}). Use it instead of doing this by hand.", tool_path.display())
}

impl Daemon {
    pub fn share_tool(&self, deployment_key: &str, sharer: &str, file: &str, summary: &str) -> Result<Reply, String> {
        let source = Path::new(file);
        if !source.is_absolute() || !source.is_file() {
            return Err(format!("{file} isn't a file given by its full path"));
        }
        let name = source.file_name().ok_or_else(|| format!("{file} has no file name"))?;
        let (deployment_id, folder_path, others) = {
            let state = self.state.lock().unwrap();
            let (folder_index, deployment) = state.find_open_deployment(deployment_key)?;
            let others: Vec<String> = state.running_positions(&deployment.id).into_iter().filter(|position| position != sharer).collect();
            (deployment.id, state.folders[folder_index].path.clone(), others)
        };
        let tools_folder = shared_tools_folder(&folder_path);
        fs::create_dir_all(&tools_folder).map_err(|error| format!("can't make {}: {error}", tools_folder.display()))?;
        let shared_path = tools_folder.join(name);
        if source != shared_path {
            fs::copy(source, &shared_path).map_err(|error| format!("can't copy {file} to {}: {error}", shared_path.display()))?;
        }
        if is_git_repo(&folder_path) {
            let relative = shared_path.strip_prefix(&folder_path).unwrap_or(&shared_path);
            commit_paths(&folder_path, &[relative], &format!("Share tool {}: {summary}", name.to_string_lossy()))?;
        }
        let text = tool_announcement(sharer, &shared_path, summary);
        others.into_iter().try_for_each(|position| {
            let entry = NewEntry { kind: EntryKind::Message, mission: None, to: Some(position), text: text.clone(), answers: None };
            self.post_entry(&deployment_id, sharer, entry).map(|_| ())
        })?;
        Ok(Reply::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_announcement_says_who_where_and_what() {
        let text = tool_announcement("tester", Path::new("/repo/.legion2/tools/browser-test.js"), "runs a page's steps in Chrome");
        assert_eq!(text, "tester shared a tool: /repo/.legion2/tools/browser-test.js (runs a page's steps in Chrome). Use it instead of doing this by hand.");
    }
}
