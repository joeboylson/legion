//! Keeps each OpenCode session's activity current by asking its server.

use std::{sync::Arc, time::Duration};

use tokio::task::JoinSet;

use crate::{
    daemon::{ActivityReport, Daemon},
    opencode_server::OpenCodeServer,
    opencode_state::{activity_from, ask_server, progress_after, Progress, Reply},
};

struct Question {
    session_id: String,
    server: OpenCodeServer,
    progress: Progress,
    waited: Duration,
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
                    Some(Question { session_id: session_id.clone(), server, progress: session.server_progress, waited: session.started_at.elapsed() })
                })
                .collect()
        };
        let mut asking = JoinSet::new();
        questions.into_iter().for_each(|question| {
            asking.spawn(async move {
                let reply = ask_server(&question.server, question.progress).await;
                (question, reply)
            });
        });
        while let Some(Ok((question, reply))) = asking.join_next().await {
            let applying = self.clone();
            // Applying logs to the store, which blocks.
            let _ = tokio::task::spawn_blocking(move || applying.apply_opencode_reply(question, reply)).await;
        }
    }

    fn apply_opencode_reply(&self, question: Question, reply: Reply) {
        let activity = activity_from(reply, question.progress, question.waited);
        let progress = progress_after(reply, question.progress);
        match self.state.lock().unwrap().sessions.get_mut(&question.session_id) {
            Some(session) => session.server_progress = progress,
            None => return,
        }
        let report = ActivityReport { activity, detail: None, addon_version: None, model: None, context_percent: None, final_answer: None };
        self.record_activity(&question.session_id, report);
    }
}
