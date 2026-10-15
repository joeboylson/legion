//! An OpenCode turn can run a whole task, so its conversation may pass
//! clearAt, and reach OpenCode's own compaction, before the turn ends. Legion
//! reads how full it is on every poll: past clearAt it asks the session to
//! wrap up, and if the turn doesn't end soon, or the conversation keeps
//! growing, it stops the turn. The turn ending then hands over as usual.

use std::{sync::Arc, time::Instant};

use legion_proto::{Activity, EntryKind, NewEntry, COMMANDER};

use crate::{
    constants::{OPENCODE_STOP_POINTS_PAST_CLEAR_AT, OPENCODE_WRAP_UP_LIMIT},
    context_handover::{wrap_up_request, HandoverPhase},
    daemon::Daemon,
    opencode_delivery::{find_session_id, interrupt_turn, post_message, Delivery},
    opencode_server::OpenCodeServer,
};

/// When a session was asked to wrap up mid-turn, and whether its turn has been stopped since.
#[derive(Clone, Copy, Debug)]
pub struct WrapUp {
    pub asked_at: Instant,
    pub is_stopped: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MidTurnStep {
    AskToWrapUp { percent: u8, clear_at: u8 },
    StopTurn,
}

/// The step to take on a poll that finds the session in the middle of a turn.
pub fn step_during_turn(phase: HandoverPhase, wrap_up: Option<WrapUp>, now: Instant, percent: Option<u8>, clear_at: Option<u8>, has_work: bool) -> Option<MidTurnStep> {
    let (percent, clear_at) = percent.zip(clear_at)?;
    match (phase, wrap_up) {
        (HandoverPhase::Working, _) if has_work && percent >= clear_at => Some(MidTurnStep::AskToWrapUp { percent, clear_at }),
        (HandoverPhase::NoteAsked, Some(wrap_up)) if !wrap_up.is_stopped => {
            let is_overdue = now.saturating_duration_since(wrap_up.asked_at) >= OPENCODE_WRAP_UP_LIMIT;
            let is_far_past = percent >= clear_at.saturating_add(OPENCODE_STOP_POINTS_PAST_CLEAR_AT);
            (is_overdue || is_far_past).then_some(MidTurnStep::StopTurn)
        }
        _ => None,
    }
}

fn log_text(position: &str, step: &MidTurnStep) -> String {
    match step {
        MidTurnStep::AskToWrapUp { percent, clear_at } => format!("{position}'s conversation is {percent}% full (clears at {clear_at}%) in the middle of a turn: asked it to wrap up"),
        MidTurnStep::StopTurn => format!("{position} didn't end its turn after being asked to wrap up: stopped it, to hand over"),
    }
}

/// The session's conversation on its server: the one saved, or the one found.
async fn conversation_on(server: &OpenCodeServer, known: Option<String>) -> Result<String, String> {
    match known {
        Some(id) => Ok(id),
        None => find_session_id(server).await.map_err(|failure| failure.to_string()),
    }
}

impl Daemon {
    /// Hands over now: the session is ended and its position starts again fresh.
    /// Messages parked for it are not steered into the old conversation; they stay in the mission's log, which the fresh one reads.
    fn restart_fresh(&self, session_id: &str) {
        match self.state.lock().unwrap().sessions.get_mut(session_id) {
            Some(session) => session.handover = HandoverPhase::Restarting,
            None => return,
        }
        self.end_for_fresh_start(session_id);
    }

    /// Called after each poll of an OpenCode session: acts if its conversation
    /// has filled up while it is still in a turn.
    pub fn hand_over_mid_turn(self: &Arc<Self>, session_id: &str) {
        let decided = {
            let mut state = self.state.lock().unwrap();
            let Some(session) = state.sessions.get_mut(session_id) else { return };
            let Some(server) = session.server.clone() else { return };
            if session.activity != Activity::Busy || session.is_stopping() {
                return;
            }
            let has_work = session.mission.is_some() || session.position == COMMANDER;
            let Some(step) = step_during_turn(session.handover, session.wrap_up, Instant::now(), session.context_percent, session.clear_at, has_work) else { return };
            match &step {
                MidTurnStep::AskToWrapUp { .. } => {
                    session.handover = HandoverPhase::NoteAsked;
                    session.wrap_up = Some(WrapUp { asked_at: Instant::now(), is_stopped: false });
                }
                MidTurnStep::StopTurn => session.wrap_up = session.wrap_up.map(|wrap_up| WrapUp { is_stopped: true, ..wrap_up }),
            }
            (step, session.info(), server, session.opencode_session.clone())
        };
        let (step, info, server, known) = decided;
        let (daemon, session_id) = (self.clone(), session_id.to_string());
        self.post_legion_entry(&info.deployment, NewEntry { kind: EntryKind::Activity, mission: info.mission, to: None, text: log_text(&info.position, &step), answers: None });
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        runtime.spawn(async move {
            let done = async {
                let conversation = conversation_on(&server, known).await?;
                match step {
                    MidTurnStep::AskToWrapUp { percent, clear_at } => {
                        let text = wrap_up_request(&info.position, info.mission, percent, clear_at);
                        post_message(&server, &conversation, &text, Delivery::Steer).await.map_err(|failure| failure.to_string())
                    }
                    MidTurnStep::StopTurn => {
                        interrupt_turn(&server, &conversation).await?;
                        // Not waiting to see the turn end: a message delivered now could start
                        // another turn in the conversation being thrown away, and nothing would hand over.
                        let daemon = daemon.clone();
                        let _ = tokio::task::spawn_blocking(move || daemon.restart_fresh(&session_id)).await;
                        Ok(())
                    }
                }
            };
            if let Err(reason) = done.await {
                eprintln!("{}d: couldn't hand over {} ({}) mid-turn: {reason}", legion_proto::LEGION, info.position, info.deployment);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const ASKED: MidTurnStep = MidTurnStep::AskToWrapUp { percent: 61, clear_at: 60 };

    fn asked_at(at: Instant) -> Option<WrapUp> {
        Some(WrapUp { asked_at: at, is_stopped: false })
    }

    #[test]
    fn crossing_clear_at_mid_turn_asks_to_wrap_up() {
        let now = Instant::now();
        assert_eq!(step_during_turn(HandoverPhase::Working, None, now, Some(61), Some(60), true), Some(ASKED));
        assert_eq!(step_during_turn(HandoverPhase::Working, None, now, Some(59), Some(60), true), None);
    }

    #[test]
    fn nothing_is_asked_without_work_a_clear_at_or_a_reading() {
        let now = Instant::now();
        assert_eq!(step_during_turn(HandoverPhase::Working, None, now, Some(90), Some(60), false), None);
        assert_eq!(step_during_turn(HandoverPhase::Working, None, now, Some(90), None, true), None);
        assert_eq!(step_during_turn(HandoverPhase::Working, None, now, None, Some(60), true), None);
    }

    #[test]
    fn a_session_asked_to_wrap_up_is_given_time_then_stopped() {
        let start = Instant::now();
        let wrap_up = asked_at(start);
        let early = start + OPENCODE_WRAP_UP_LIMIT - Duration::from_secs(1);
        assert_eq!(step_during_turn(HandoverPhase::NoteAsked, wrap_up, early, Some(65), Some(60), true), None);
        let late = start + OPENCODE_WRAP_UP_LIMIT;
        assert_eq!(step_during_turn(HandoverPhase::NoteAsked, wrap_up, late, Some(65), Some(60), true), Some(MidTurnStep::StopTurn));
    }

    #[test]
    fn growing_ten_points_past_clear_at_stops_the_turn_at_once() {
        let now = Instant::now();
        assert_eq!(step_during_turn(HandoverPhase::NoteAsked, asked_at(now), now, Some(69), Some(60), true), None);
        assert_eq!(step_during_turn(HandoverPhase::NoteAsked, asked_at(now), now, Some(70), Some(60), true), Some(MidTurnStep::StopTurn));
    }

    #[test]
    fn each_step_is_taken_once() {
        let start = Instant::now();
        let stopped = Some(WrapUp { asked_at: start, is_stopped: true });
        let late = start + OPENCODE_WRAP_UP_LIMIT * 2;
        assert_eq!(step_during_turn(HandoverPhase::NoteAsked, stopped, late, Some(95), Some(60), true), None);
        assert_eq!(step_during_turn(HandoverPhase::Restarting, asked_at(start), late, Some(95), Some(60), true), None);
    }

    #[test]
    fn a_note_asked_at_turn_end_is_not_stopped_mid_turn() {
        let now = Instant::now();
        assert_eq!(step_during_turn(HandoverPhase::NoteAsked, None, now + OPENCODE_WRAP_UP_LIMIT, Some(95), Some(60), true), None);
    }
}
