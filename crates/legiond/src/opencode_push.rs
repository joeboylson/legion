//! Pushes messages to the sessions that run their own OpenCode server. A
//! Claude session's add-on asks for its messages; these sessions have no
//! add-on, so Legion brings them. The tick that pushes is also the retry.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use legion_proto::{Entry, LEGION};

use crate::{
    constants::SESSION_START_GRACE,
    daemon::{mark_handed, pending_entries, Daemon},
    entry_routing::delivery_text,
    opencode_delivery::{deliver_in_order, find_session_id, Delivery, Failure},
    opencode_server::OpenCodeServer,
};

/// What the last try said went wrong, by session: so the same failure is
/// logged once, not every second.
pub type ReportedFailures = HashMap<String, (i64, String)>;

/// Which sessions are being pushed to right now, and what has been logged.
#[derive(Default)]
pub struct PushBook {
    in_flight: HashSet<String>,
    reported: ReportedFailures,
}

/// A session with messages waiting, copied out of the state so no lock is held while talking to its server.
#[derive(Clone)]
struct Waiting {
    session_id: String,
    deployment: String,
    deployment_name: String,
    position: String,
    server: OpenCodeServer,
    opencode_session: Option<String>,
    /// How long the session has been running.
    waited: Duration,
    entries: Vec<Entry>,
}

/// A conversation that isn't there yet is expected while its session starts:
/// the message is kept and tried again, and only a wait past the grace is news.
fn is_worth_logging(failure: &Failure, waited: Duration) -> bool {
    !matches!(failure, Failure::NoSessionYet) || waited >= SESSION_START_GRACE
}

/// True the first time this failure is seen for this entry.
fn is_new_failure(reported: &mut ReportedFailures, session_id: &str, entry_id: i64, reason: &str) -> bool {
    let failure = (entry_id, reason.to_string());
    reported.insert(session_id.to_string(), failure.clone()).as_ref() != Some(&failure)
}

impl Daemon {
    /// Every session that runs its own server and isn't being stopped.
    fn opencode_session_ids(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state.sessions.iter().filter(|(_, session)| session.server.is_some() && !session.is_stopping()).map(|(session_id, _)| session_id.clone()).collect()
    }

    /// A session's waiting messages as they are now: read after the session
    /// is claimed, so a push still running never sees what it just sent.
    fn opencode_session_waiting(&self, session_id: &str) -> Option<Waiting> {
        let state = self.state.lock().unwrap();
        let session = state.sessions.get(session_id)?;
        let server = session.server.clone()?;
        let entries = pending_entries(&state, session_id).unwrap_or_else(|error| {
            eprintln!("{LEGION}d: can't look up messages for {} ({}): {error}", session.position, session.deployment);
            None
        })?;
        let deployment_name = state.find_deployment(&session.deployment).map(|(_, deployment)| deployment.name).unwrap_or_else(|_| session.deployment.clone());
        Some(Waiting {
            session_id: session_id.to_string(),
            deployment: session.deployment.clone(),
            deployment_name,
            position: session.position.clone(),
            server,
            opencode_session: session.opencode_session.clone(),
            waited: session.started_at.elapsed(),
            entries,
        })
    }

    fn remember_opencode_session(&self, session_id: &str, opencode_session: &str) {
        if let Some(session) = self.state.lock().unwrap().sessions.get_mut(session_id) {
            session.opencode_session = Some(opencode_session.to_string());
        }
    }

    fn forget_opencode_session(&self, session_id: &str) {
        if let Some(session) = self.state.lock().unwrap().sessions.get_mut(session_id) {
            session.opencode_session = None;
        }
    }

    /// False once the session has ended or is being ended.
    fn is_session_live(&self, session_id: &str) -> bool {
        self.state.lock().unwrap().sessions.get(session_id).is_some_and(|session| !session.is_stopping())
    }

    /// Only after its server took the message is the entry marked delivered.
    fn record_pushed(&self, waiting: &Waiting, entry: &Entry) {
        let mut state = self.state.lock().unwrap();
        if let Err(error) = mark_handed(&mut state, &waiting.deployment, &waiting.session_id, std::slice::from_ref(entry)) {
            eprintln!("{LEGION}d: delivered entry {} to {} ({}) but couldn't record it: {error}", entry.id, waiting.position, waiting.deployment_name);
        }
    }

    /// One tick: each OpenCode session gets a task of its own to post its
    /// waiting messages to its server, oldest first, so a hung server delays
    /// no other. A message its server doesn't take stays undelivered and is
    /// tried again next tick; a session still being pushed is skipped. A
    /// message is only given up on when its session has ended.
    pub async fn push_messages_to_opencode_sessions(self: &Arc<Self>, book: &Arc<Mutex<PushBook>>) {
        for session_id in self.opencode_session_ids() {
            if !book.lock().unwrap().in_flight.insert(session_id.clone()) {
                continue;
            }
            let (daemon, book) = (self.clone(), book.clone());
            tokio::spawn(async move {
                let looking = daemon.clone();
                let looked_for = session_id.clone();
                let waiting = tokio::task::spawn_blocking(move || looking.opencode_session_waiting(&looked_for)).await.ok().flatten().filter(|waiting| !waiting.entries.is_empty());
                let failure = match &waiting {
                    Some(waiting) => daemon.push_to(&Arc::new(waiting.clone())).await,
                    None => None,
                };
                let is_live = daemon.is_session_live(&session_id);
                let mut locked = book.lock().unwrap();
                locked.in_flight.remove(&session_id);
                let Some(waiting) = waiting else { return };
                let (position, deployment) = (&waiting.position, &waiting.deployment_name);
                match failure {
                    Some((entry_id, _)) if !is_live => {
                        locked.reported.remove(&session_id);
                        eprintln!("{LEGION}d: entry {entry_id} to {position} ({deployment}) wasn't delivered: its session ended");
                    }
                    Some((entry_id, reason)) if is_worth_logging(&reason, waiting.waited) && is_new_failure(&mut locked.reported, &session_id, entry_id, &reason.to_string()) => {
                        eprintln!("{LEGION}d: couldn't deliver entry {entry_id} to {position} ({deployment}): {reason}");
                    }
                    Some(_) => {}
                    None => {
                        if let Some((entry_id, _)) = locked.reported.remove(&session_id) {
                            eprintln!("{LEGION}d: delivered entry {entry_id} to {position} ({deployment}) after the earlier failure");
                        }
                    }
                }
            });
        }
    }

    /// Posts one session's waiting messages in order; the entry that failed and why.
    async fn push_to(self: &Arc<Self>, waiting: &Arc<Waiting>) -> Option<(i64, Failure)> {
        let opencode_session = match self.opencode_session_of(waiting).await {
            Ok(id) => id,
            Err(reason) => return waiting.entries.first().map(|entry| (entry.id, reason)),
        };
        let messages: Vec<(i64, String, Delivery)> = waiting.entries.iter().map(|entry| (entry.id, delivery_text(entry), Delivery::from_sender(&entry.from))).collect();
        let failure = deliver_in_order(&waiting.server, &opencode_session, &messages, |entry_id| {
            let (daemon, waiting) = (self.clone(), waiting.clone());
            async move {
                // Recording writes to the store, which blocks.
                let _ = tokio::task::spawn_blocking(move || {
                    if let Some(entry) = waiting.entries.iter().find(|entry| entry.id == entry_id) {
                        daemon.record_pushed(&waiting, entry);
                    }
                })
                .await;
            }
        })
        .await;
        if matches!(failure, Some((_, Failure::SessionGone))) {
            self.forget_opencode_session(&waiting.session_id);
        }
        failure
    }

    /// Its conversation's ID on its server, asked for once and kept.
    async fn opencode_session_of(&self, waiting: &Waiting) -> Result<String, Failure> {
        if let Some(known) = &waiting.opencode_session {
            return Ok(known.clone());
        }
        let found = find_session_id(&waiting.server).await?;
        self.remember_opencode_session(&waiting.session_id, &found);
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_conversation_is_news_only_after_the_start_grace() {
        assert!(!is_worth_logging(&Failure::NoSessionYet, SESSION_START_GRACE - Duration::from_secs(1)));
        assert!(is_worth_logging(&Failure::NoSessionYet, SESSION_START_GRACE));
        assert!(is_worth_logging(&Failure::SessionGone, Duration::ZERO));
        assert!(is_worth_logging(&Failure::Other("refused".into()), Duration::ZERO));
    }

    #[test]
    fn a_failure_is_new_once_per_entry_and_reason() {
        let mut reported = ReportedFailures::new();
        assert!(is_new_failure(&mut reported, "s", 3, "refused"));
        assert!(!is_new_failure(&mut reported, "s", 3, "refused"));
        assert!(is_new_failure(&mut reported, "s", 3, "answered 500"));
        assert!(is_new_failure(&mut reported, "s", 4, "answered 500"));
        assert!(is_new_failure(&mut reported, "other", 4, "answered 500"));
    }
}
