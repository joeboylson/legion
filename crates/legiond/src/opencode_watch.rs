//! Keeps each OpenCode session's activity current by asking its server.

use std::{sync::Arc, time::Duration};

use tokio::task::JoinSet;

use crate::{
    daemon::{ActivityReport, Daemon},
    opencode_context::ask_conversation,
    opencode_server::OpenCodeServer,
    opencode_state::{activity_from, ask_server, progress_after, Progress, Reply},
};

struct Question {
    session_id: String,
    server: OpenCodeServer,
    progress: Progress,
    opencode_session: Option<String>,
    waited: Duration,
}

/// What one ask of a server brought back.
struct Answer {
    reply: Reply,
    opencode_session: Option<String>,
    context_percent: Option<u8>,
}

impl Daemon {
    /// Asks every OpenCode session's server, one task each so a hung one
    /// delays no other, with the lock held for neither the asking nor the
    /// applying's slow parts. Each answer goes through the path a Claude
    /// session's reports take.
    pub async fn read_opencode_states(self: Arc<Self>) {
        let questions: Vec<Question> = {
            let state = self.state.lock().unwrap();
            state
                .sessions
                .iter()
                .filter_map(|(session_id, session)| {
                    let server = session.server.clone()?;
                    Some(Question { session_id: session_id.clone(), server, progress: session.server_progress, opencode_session: session.opencode_session.clone(), waited: session.started_at.elapsed() })
                })
                .collect()
        };
        let mut asking = JoinSet::new();
        questions.into_iter().for_each(|question| {
            asking.spawn(async move {
                let reply = ask_server(&question.server, question.progress).await;
                // A server that has answered about a session may be asked how full it is.
                let (opencode_session, context_percent) = match reply {
                    Reply::Answered { has_session: true, .. } => ask_conversation(&question.server, question.opencode_session.clone()).await,
                    _ => (None, None),
                };
                (question, Answer { reply, opencode_session, context_percent })
            });
        });
        while let Some(Ok((question, answer))) = asking.join_next().await {
            let applying = self.clone();
            // Applying logs to the store, which blocks.
            let _ = tokio::task::spawn_blocking(move || applying.apply_opencode_answer(question, answer)).await;
        }
    }

    fn apply_opencode_answer(self: &Arc<Self>, question: Question, answer: Answer) {
        let activity = activity_from(answer.reply, question.progress, question.waited);
        let progress = progress_after(answer.reply, question.progress);
        match self.state.lock().unwrap().sessions.get_mut(&question.session_id) {
            Some(session) => {
                session.server_progress = progress;
                session.opencode_session = answer.opencode_session.or(session.opencode_session.take());
            }
            None => return,
        }
        let report = ActivityReport { activity, detail: None, addon_version: None, model: None, context_percent: answer.context_percent, final_answer: None };
        self.record_activity(&question.session_id, report);
        self.hand_over_mid_turn(&question.session_id);
    }
}
