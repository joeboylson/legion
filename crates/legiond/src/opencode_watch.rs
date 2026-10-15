//! Keeps each OpenCode session's activity current by asking its server.

use std::time::Duration;

use crate::{
    daemon::{ActivityReport, Daemon},
    opencode_state::{activity_from, ask_server, progress_after, OpenCodeServer, Progress, Reply},
};

struct Question {
    session_id: String,
    server: OpenCodeServer,
    progress: Progress,
    waited: Duration,
}

impl Daemon {
    /// Asks every OpenCode session's server in turn, off the lock, then
    /// applies what each said through the path a Claude session's reports take.
    pub fn read_opencode_states(&self) {
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
        let answers: Vec<(Question, Reply)> = std::thread::scope(|scope| {
            let asking: Vec<_> = questions.into_iter().map(|question| scope.spawn(move || (ask_server(&question.server, question.progress), question))).collect();
            asking.into_iter().filter_map(|asked| asked.join().ok()).map(|(reply, question)| (question, reply)).collect()
        });
        answers.into_iter().for_each(|(question, reply)| self.apply_opencode_reply(question, reply));
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
