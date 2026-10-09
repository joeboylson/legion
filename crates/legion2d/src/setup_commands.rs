//! Changing a folder's setup from the command line or the app: operators
//! and pipelines, written to `.legion2/` the same as by hand.

use legion2_proto::Reply;

use crate::{
    daemon::Daemon,
    naming::operator_of_position,
    setup::{add_operator, add_pipeline, define_operator, write_pipeline, remove_operator, remove_pipeline, set_operator, set_settings, PermissionMode, NO_PIPELINE},
};

impl Daemon {
    fn folder_path(&self, folder_key: &str) -> Result<std::path::PathBuf, String> {
        let state = self.state.lock().unwrap();
        Ok(state.folders[state.find_folder(folder_key)?].path.clone())
    }

    pub fn add_operator_to_folder(&self, folder_key: &str, name: &str, definition: &str, limit: Option<u32>) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        add_operator(&folder, name.trim(), definition, limit)?;
        Ok(Reply::Text { text: format!("added {name}; deployments with no pipeline ({NO_PIPELINE}) have it from now on") })
    }

    pub fn remove_operator_from_folder(&self, folder_key: &str, name: &str) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        let is_running = {
            let state = self.state.lock().unwrap();
            let folder_index = state.find_folder(folder_key)?;
            let deployment_ids: Vec<&str> = state.folders[folder_index].deployments.iter().map(|deployment| deployment.id.as_str()).collect();
            state.sessions.values().any(|session| operator_of_position(&session.position) == name && deployment_ids.contains(&session.deployment.as_str()))
        };
        if is_running {
            return Err(format!("{name} is running; stop it first"));
        }
        remove_operator(&folder, name.trim())?;
        Ok(Reply::Text { text: format!("removed {name}") })
    }

    pub fn add_pipeline_to_folder(&self, folder_key: &str, name: &str, operators: &[String], in_order: bool) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        add_pipeline(&folder, name.trim(), operators, in_order)?;
        let shape = if in_order { operators.join(" → ") } else { format!("{} through the commander", operators.join(", ")) };
        Ok(Reply::Text { text: format!("added the {name} pipeline: {shape}") })
    }

    pub fn set_folder_settings(&self, folder_key: &str, check: Option<String>, permission_mode: Option<String>) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        set_settings(&folder, check, parse_mode(permission_mode)?)?;
        Ok(Reply::Text { text: "saved".into() })
    }

    pub fn set_operator_settings(&self, folder_key: &str, name: &str, limit: Option<u32>, model: Option<String>, permission_mode: Option<String>) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        set_operator(&folder, name, limit, model, parse_mode(permission_mode)?)?;
        Ok(Reply::Text { text: format!("saved {name}") })
    }

    pub fn define_operator_in_folder(&self, folder_key: &str, name: &str, definition: &str) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        define_operator(&folder, name, definition)?;
        Ok(Reply::Text { text: format!("saved what {name} does") })
    }

    pub fn write_pipeline_in_folder(&self, folder_key: &str, name: &str, text: &str) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        write_pipeline(&folder, name, text)?;
        Ok(Reply::Text { text: format!("saved the {name} pipeline") })
    }

    pub fn remove_pipeline_from_folder(&self, folder_key: &str, name: &str) -> Result<Reply, String> {
        let folder = self.folder_path(folder_key)?;
        let running: Vec<String> = {
            let state = self.state.lock().unwrap();
            let folder_index = state.find_folder(folder_key)?;
            state.folders[folder_index].deployments.iter().filter(|deployment| deployment.closed_ms.is_none() && deployment.pipeline == name).map(|deployment| deployment.name.clone()).collect()
        };
        if !running.is_empty() {
            return Err(format!("{} runs {name}; close it first", running.join(" and ")));
        }
        remove_pipeline(&folder, name)?;
        Ok(Reply::Text { text: format!("removed the {name} pipeline") })
    }
}

/// A permission mode as Claude Code names it, such as acceptEdits.
fn parse_mode(mode: Option<String>) -> Result<Option<PermissionMode>, String> {
    mode.map(|mode| serde_json::from_value(serde_json::Value::String(mode.clone())).map_err(|_| format!("{mode:?} isn't a permission mode: acceptEdits, auto, bypassPermissions, manual, dontAsk or plan"))).transpose()
}
