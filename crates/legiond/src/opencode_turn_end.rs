//! After a handoff, done or blocked, an OpenCode session has nothing left to
//! do, but its model may keep its turn going (sleeping, polling), and
//! messages to a busy session wait for its turn to end. So Legion ends it.

use legion_proto::{EntryKind, LEGION};

use crate::{
    daemon::Daemon,
    opencode_delivery::{find_session_id, interrupt_turn},
    opencode_server::OpenCodeServer,
};

/// True for the entries that say a session's step is over.
pub fn ends_the_turn(kind: EntryKind) -> bool {
    matches!(kind, EntryKind::Handoff | EntryKind::Done | EntryKind::Blocked)
}

impl Daemon {
    /// The server and saved conversation of the OpenCode session at this position, if it runs one.
    fn opencode_session_at(&self, deployment_id: &str, position: &str) -> Option<(OpenCodeServer, Option<String>)> {
        let state = self.state.lock().unwrap();
        state.sessions.values().find(|session| session.deployment == deployment_id && session.position == position).and_then(|session| session.server.clone().map(|server| (server, session.opencode_session.clone())))
    }

    /// Stops the current turn of the session at `position` through its own server,
    /// in the background. A session without a server (Claude) is left alone.
    pub fn end_opencode_turn(&self, deployment_id: &str, position: &str) {
        let Some((server, known)) = self.opencode_session_at(deployment_id, position) else { return };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        let (deployment_id, position) = (deployment_id.to_string(), position.to_string());
        runtime.spawn(async move {
            let ended = match known {
                Some(id) => interrupt_turn(&server, &id).await,
                None => match find_session_id(&server).await {
                    Ok(id) => interrupt_turn(&server, &id).await,
                    Err(failure) => Err(failure.to_string()),
                },
            };
            if let Err(reason) = ended {
                eprintln!("{LEGION}d: couldn't end the turn of {position} ({deployment_id}): {reason}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_handoff_done_and_blocked_end_a_turn() {
        assert!([EntryKind::Handoff, EntryKind::Done, EntryKind::Blocked].into_iter().all(ends_the_turn));
        assert!(![EntryKind::Message, EntryKind::Note, EntryKind::Question].into_iter().any(ends_the_turn));
    }
}
