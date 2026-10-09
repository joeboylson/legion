//! What every command works with: legion2d, the asker, and which folder and
//! deployment it means. The folder is --folder, or the nearest one up from
//! here with a `.legion2/` setup, the way git finds `.git`. The deployment
//! is --deployment, or the folder's open one, picked from a list when it
//! has several.

use std::path::{Path, PathBuf};

use legion2_client::Client;
use legion2_proto::{Command, Deployment, FolderDetail, Reply, NAME};

use crate::ask::{item, Asker};

pub fn find_setup_root(start: &Path) -> Option<PathBuf> {
    let setup_folder_name = format!(".{NAME}");
    start.ancestors().find(|folder| folder.join(&setup_folder_name).is_dir()).map(Path::to_path_buf)
}

pub fn open_deployments(detail: &FolderDetail) -> Vec<Deployment> {
    detail.deployments.iter().filter(|deployment| deployment.closed_ms.is_none()).cloned().collect()
}

pub struct Context {
    pub client: Client,
    pub ask: Asker,
    folder_flag: Option<String>,
    deployment_flag: Option<String>,
}

impl Context {
    pub async fn connect(folder_flag: Option<String>, deployment_flag: Option<String>) -> Result<Context, String> {
        Ok(Context { client: Client::connect().await?, ask: Asker::new(), folder_flag, deployment_flag })
    }

    pub fn named_deployment(&self) -> Option<&str> {
        self.deployment_flag.as_deref()
    }

    pub fn has_named_deployment(&self) -> bool {
        self.deployment_flag.is_some()
    }

    pub async fn send(&mut self, command: Command) -> Result<Reply, String> {
        self.client.ask(command).await
    }

    /// The folder's key for legion2d: --folder, or the path of the folder you're in.
    pub fn folder_key(&self) -> Result<String, String> {
        if let Some(named) = &self.folder_flag {
            return Ok(named.clone());
        }
        let here = std::env::current_dir().map_err(|error| error.to_string())?;
        find_setup_root(&here)
            .map(|root| root.to_string_lossy().into_owned())
            .ok_or_else(|| format!("{} isn't in a Legion folder: set one up with `{NAME} setup`, or name one with --folder", here.display()))
    }

    pub async fn read_folder(&mut self, key: &str) -> Result<FolderDetail, String> {
        match self.send(Command::FolderRead { folder: key.to_string() }).await? {
            Reply::FolderDetail { detail } => Ok(detail),
            other => Err(format!("unexpected reply: {other:?}")),
        }
    }

    /// The folder this command means, added to legion2d if it hasn't been.
    pub async fn folder(&mut self) -> Result<FolderDetail, String> {
        let key = self.folder_key()?;
        if let Ok(detail) = self.read_folder(&key).await {
            return Ok(detail);
        }
        if self.folder_flag.is_some() {
            return self.read_folder(&key).await;
        }
        self.send(Command::FolderAdd { path: key.clone() }).await?;
        self.read_folder(&key).await
    }

    /// The open deployment this command means.
    pub async fn deployment(&mut self) -> Result<Deployment, String> {
        if let Some(named) = self.deployment_flag.clone() {
            let Reply::Deployments { deployments } = self.send(Command::DeploymentList { folder: None }).await? else { return Err("unexpected reply".into()) };
            let matching: Vec<Deployment> = deployments.into_iter().filter(|deployment| deployment.id == named || deployment.name == named).collect();
            let open: Vec<&Deployment> = matching.iter().filter(|deployment| deployment.closed_ms.is_none()).collect();
            return match (open.as_slice(), matching.is_empty()) {
                ([only], _) => Ok((*only).clone()),
                ([], true) => Err(format!("no deployment {named}")),
                ([], false) => Err(format!("{named} is closed")),
                (_, _) => Err(format!("more than one open deployment is called {named}; use its ID")),
            };
        }
        let detail = self.folder().await?;
        let open = open_deployments(&detail);
        let items = open.iter().map(|deployment| item(deployment.id.clone(), &deployment.name, &deployment.pipeline)).collect();
        let empty = format!("no deployment is running in {}: start one with `{NAME} deploy`", detail.folder.name);
        let id = self.ask.pick(None, "Which deployment?", items, "--deployment", &empty)?;
        open.into_iter().find(|deployment| deployment.id == id).ok_or_else(|| format!("no deployment {id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_setup_up_from_here_is_the_folder() {
        let top = std::env::temp_dir().join(format!("legion2-find-{}", std::process::id()));
        let inner = top.join("app/src/deep");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::create_dir_all(top.join(format!("app/.{NAME}"))).unwrap();
        assert_eq!(find_setup_root(&inner), Some(top.join("app")));
        assert_eq!(find_setup_root(&top.join("app")), Some(top.join("app")));
        assert_eq!(find_setup_root(&top), None);
        std::fs::remove_dir_all(&top).unwrap();
    }
}
