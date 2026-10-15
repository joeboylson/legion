//! Whether a message to a position has anyone to reach, now or very soon.

use legion_proto::{COMMANDER, HUMAN, STRATEGIST};

use crate::{context_handover::HandoverPhase, daemon::State};

impl State {
    /// True if a message to `position` will be read: the admin; the commander and strategist, which
    /// Legion starts again by itself; a session running, or ending only to start again from a
    /// handover; or one waiting to come back after legiond restarted.
    pub fn is_position_reachable(&self, deployment_id: &str, position: &str) -> bool {
        [HUMAN, COMMANDER, STRATEGIST].contains(&position)
            || self.sessions.values().any(|session| session.deployment == deployment_id && session.position == position && (!session.is_stopping() || session.handover == HandoverPhase::Restarting))
            || self.pending_restores.iter().any(|restore| restore.deployment == deployment_id && restore.position == position)
    }
}
