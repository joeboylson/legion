//! Follows each OpenCode session's event stream and writes one line per
//! finished step to the deployment log, so a frozen screen doesn't hide
//! what the server is doing.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use http_body_util::BodyExt;
use hyper::Method;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use legion_proto::{EntryKind, NewEntry};
use tokio::task::JoinHandle;

use crate::{
    constants::{OPENCODE_EVENTS_PATH, OPENCODE_EVENTS_RETRY},
    daemon::Daemon,
    opencode_server::{authorized_request, OpenCodeServer},
    opencode_steps::{events_in, Steps},
};

/// The tasks following a server's events, by session.
pub type Followers = HashMap<String, JoinHandle<()>>;

/// Opens the stream and hands each finished step's line to `on_line` until it ends; why it ended.
async fn read_stream(server: &OpenCodeServer, steps: &mut Steps, on_line: &impl Fn(String)) -> String {
    // No time limit: the stream is quiet while the session is.
    let client = Client::builder(TokioExecutor::new()).pool_max_idle_per_host(0).build_http();
    let request = match authorized_request(server, Method::GET, OPENCODE_EVENTS_PATH, None) {
        Ok(request) => request,
        Err(error) => return error,
    };
    let reply = match client.request(request).await {
        Ok(reply) if reply.status().is_success() => reply,
        Ok(reply) => return reply.status().to_string(),
        Err(error) => return error.to_string(),
    };
    let mut body = reply.into_body();
    let mut pending = String::new();
    loop {
        match body.frame().await {
            Some(Ok(frame)) => {
                let chunk = frame.into_data().map(|data| String::from_utf8_lossy(&data).into_owned()).unwrap_or_default();
                events_in(&mut pending, &chunk).iter().filter_map(|event| steps.follow(event)).for_each(on_line);
            }
            Some(Err(error)) => return error.to_string(),
            None => return "the stream closed".to_string(),
        }
    }
}

impl Daemon {
    /// Where a session's lines go now: its deployment, position and mission as they stand.
    fn step_entry_target(&self, session_id: &str) -> Option<(String, String, Option<u32>)> {
        let state = self.state.lock().unwrap();
        let session = state.sessions.get(session_id)?;
        Some((session.deployment.clone(), session.position.clone(), session.mission))
    }

    fn log_step(&self, session_id: &str, line: String) {
        let Some((deployment, position, mission)) = self.step_entry_target(session_id) else { return };
        let entry = NewEntry { kind: EntryKind::Activity, mission, to: None, text: line, answers: None };
        if let Err(error) = self.post_entry(&deployment, &position, entry) {
            eprintln!("{}d: {error}", legion_proto::LEGION);
        }
    }

    /// Follows one server for as long as the task lives: when the stream
    /// breaks it waits and opens it again, with the steps in flight dropped.
    async fn follow_steps_of(self: Arc<Self>, session_id: String, server: OpenCodeServer) {
        // One writer, so lines land in the order the steps ended.
        let (lines, mut waiting) = tokio::sync::mpsc::unbounded_channel::<String>();
        let writing = self.clone();
        let id = session_id.clone();
        tokio::spawn(async move {
            while let Some(line) = waiting.recv().await {
                let (daemon, id) = (writing.clone(), id.clone());
                // Logging writes to the store, which blocks.
                let _ = tokio::task::spawn_blocking(move || daemon.log_step(&id, line)).await;
            }
        });
        loop {
            let mut steps = Steps::default();
            read_stream(&server, &mut steps, &|line| {
                let _ = lines.send(line);
            })
            .await;
            tokio::time::sleep(OPENCODE_EVENTS_RETRY).await;
        }
    }

    /// One tick: every OpenCode session has a task following its events, and
    /// one whose session has ended has none.
    pub fn follow_opencode_steps(self: &Arc<Self>, followers: &Arc<Mutex<Followers>>) {
        let servers: HashMap<String, OpenCodeServer> = {
            let state = self.state.lock().unwrap();
            state.sessions.iter().filter(|(_, session)| !session.is_stopping()).filter_map(|(id, session)| Some((id.clone(), session.server.clone()?))).collect()
        };
        let mut following = followers.lock().unwrap();
        following.retain(|session_id, task| {
            let is_live = servers.contains_key(session_id);
            if !is_live {
                task.abort();
            }
            is_live
        });
        servers.into_iter().for_each(|(session_id, server)| {
            following.entry(session_id.clone()).or_insert_with(|| tokio::spawn(self.clone().follow_steps_of(session_id, server)));
        });
    }
}
