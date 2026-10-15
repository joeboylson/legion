//! Whether an OpenCode session is busy or idle, read from its own server
//! (`opencode serve`) over a plain HTTP GET with basic auth.

use std::time::Duration;

use legion_proto::Activity;
use serde_json::Value;

use crate::{
    constants::{OPENCODE_ACTIVE_PATH, OPENCODE_SESSIONS_PATH, SESSION_START_GRACE},
    opencode_server::{get, this_runs_session, OpenCodeServer},
};

/// What Legion has seen of a server so far.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub has_answered: bool,
    pub has_session: bool,
}

/// Which of the server's sessions are this run's: made since it started, in its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunScope {
    pub started_ms: i64,
    pub directory: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// No connection, a timeout, a refusal or something that isn't its answer.
    Silent,
    Answered { is_running: bool, has_session: bool },
}

/// Starting until the server has answered and holds a session, so a session
/// that hasn't begun isn't shown idle, and one still loading its tools isn't
/// shown unreachable. After that, silence means unreachable.
pub fn activity_from(reply: Reply, progress: Progress, waited: Duration) -> Activity {
    match reply {
        Reply::Answered { is_running: true, .. } => Activity::Busy,
        Reply::Answered { has_session: true, .. } => Activity::Idle,
        Reply::Answered { .. } => Activity::Starting,
        Reply::Silent if progress.has_answered || waited >= SESSION_START_GRACE => Activity::Unreachable,
        Reply::Silent => Activity::Starting,
    }
}

pub fn progress_after(reply: Reply, progress: Progress) -> Progress {
    match reply {
        Reply::Silent => progress,
        Reply::Answered { has_session, .. } => Progress { has_answered: true, has_session: progress.has_session || has_session },
    }
}

/// A session is running when the active list has an entry of type "running".
fn is_running_in(body: &str) -> Result<bool, String> {
    let reply: Value = serde_json::from_str(body).map_err(|error| format!("not JSON: {error}"))?;
    let active = reply.get("data").and_then(Value::as_object).ok_or("no data object")?;
    Ok(active.values().any(|entry| entry.get("type").and_then(Value::as_str) == Some("running")))
}

/// Asks the server. The list of sessions is only asked for while none of
/// this run's has been seen, since a running one already shows there is one.
pub async fn ask_server(server: &OpenCodeServer, progress: Progress, scope: &RunScope) -> Reply {
    let Ok(is_running) = get(server, OPENCODE_ACTIVE_PATH).await.and_then(|body| is_running_in(&body)) else { return Reply::Silent };
    if is_running || progress.has_session {
        return Reply::Answered { is_running, has_session: true };
    }
    match get(server, OPENCODE_SESSIONS_PATH).await.and_then(|body| this_runs_session(&body, scope.started_ms, &scope.directory)) {
        Ok(session) => Reply::Answered { is_running, has_session: session.is_some() },
        Err(_) => Reply::Silent,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    const SOON: Duration = Duration::from_secs(1);
    const LATE: Duration = SESSION_START_GRACE;

    fn scope() -> RunScope {
        RunScope { started_ms: 1000, directory: "/work".into() }
    }

    fn answered(is_running: bool, has_session: bool) -> Reply {
        Reply::Answered { is_running, has_session }
    }

    #[test]
    fn a_running_session_is_busy() {
        assert_eq!(activity_from(answered(true, true), Progress::default(), SOON), Activity::Busy);
    }

    #[test]
    fn nothing_running_with_a_session_is_idle() {
        assert_eq!(activity_from(answered(false, true), Progress::default(), SOON), Activity::Idle);
    }

    #[test]
    fn nothing_running_and_no_session_yet_is_starting() {
        assert_eq!(activity_from(answered(false, false), Progress::default(), LATE), Activity::Starting);
    }

    #[test]
    fn silence_is_starting_while_it_loads_then_unreachable() {
        assert_eq!(activity_from(Reply::Silent, Progress::default(), SOON), Activity::Starting);
        assert_eq!(activity_from(Reply::Silent, Progress::default(), LATE), Activity::Unreachable);
    }

    #[test]
    fn silence_after_it_answered_is_unreachable_at_once() {
        let progress = Progress { has_answered: true, has_session: true };
        assert_eq!(activity_from(Reply::Silent, progress, SOON), Activity::Unreachable);
    }

    #[test]
    fn progress_remembers_an_answer_and_a_session_for_good() {
        let seen = progress_after(answered(false, true), Progress::default());
        assert_eq!(seen, Progress { has_answered: true, has_session: true });
        assert_eq!(progress_after(answered(false, false), seen), seen);
        assert_eq!(progress_after(Reply::Silent, seen), seen);
    }

    #[test]
    fn the_active_list_says_running_only_for_running_entries() {
        assert_eq!(is_running_in(r#"{"data":{"ses_1":{"type":"running"}}}"#), Ok(true));
        assert_eq!(is_running_in(r#"{"data":{}}"#), Ok(false));
        assert_eq!(is_running_in(r#"{"data":{"ses_1":{"type":"other"}}}"#), Ok(false));
        assert!(is_running_in("<html>").is_err());
        assert!(is_running_in(r#"{"error":"x"}"#).is_err());
    }

    /// A server that answers each connection with the next reply: an empty one is a 401, None closes the connection unanswered.
    fn fake_server(replies: Vec<Option<&'static str>>) -> (OpenCodeServer, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = OpenCodeServer { port: listener.local_addr().unwrap().port(), password: "pw".into() };
        let serving = thread::spawn(move || {
            replies
                .into_iter()
                .map(|reply| {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut request = [0u8; 2048];
                    let read = stream.read(&mut request).unwrap();
                    if let Some(body) = reply {
                        let status = if body.is_empty() { "401 Unauthorized" } else { "200 OK" };
                        write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                    }
                    String::from_utf8_lossy(&request[..read]).into_owned()
                })
                .collect()
        });
        (server, serving)
    }

    #[tokio::test]
    async fn a_server_with_a_running_session_is_busy_and_gets_the_password() {
        let (server, serving) = fake_server(vec![Some(r#"{"data":{"ses_1":{"type":"running"}}}"#)]);
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, answered(true, true));
        let requests = serving.join().unwrap();
        assert!(requests[0].starts_with("GET /api/session/active HTTP/1.1"), "{}", requests[0]);
        assert!(requests[0].to_lowercase().contains("authorization: basic b3blbmnvzgu6chc="), "{}", requests[0]);
    }

    #[tokio::test]
    async fn an_idle_server_is_asked_for_its_sessions_until_one_exists() {
        let (server, serving) = fake_server(vec![Some(r#"{"data":{}}"#), Some(r#"{"data":[]}"#), Some(r#"{"data":{}}"#), Some(r#"{"data":[{"id":"s","time":{"created":1200},"location":{"directory":"/work"}}]}"#)]);
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, answered(false, false));
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, answered(false, true));
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn sessions_of_an_earlier_run_or_another_folder_leave_it_starting() {
        let old = r#"{"data":[{"id":"a","time":{"created":999},"location":{"directory":"/work"}},{"id":"b","time":{"created":2000},"location":{"directory":"/other"}}]}"#;
        let (server, serving) = fake_server(vec![Some(r#"{"data":{}}"#), Some(old)]);
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, answered(false, false));
        serving.join().unwrap();
    }

    #[tokio::test]
    async fn a_session_already_seen_isnt_listed_again() {
        let (server, serving) = fake_server(vec![Some(r#"{"data":{}}"#)]);
        assert_eq!(ask_server(&server, Progress { has_answered: true, has_session: true }, &scope()).await, answered(false, true));
        assert_eq!(serving.join().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_refusal_a_closed_connection_and_bad_json_are_silence() {
        let (server, serving) = fake_server(vec![Some(""), None, Some("not json")]);
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, Reply::Silent);
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, Reply::Silent);
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, Reply::Silent);
        serving.join().unwrap();
        assert_eq!(ask_server(&server, Progress::default(), &scope()).await, Reply::Silent, "nothing listens any more");
    }
}
