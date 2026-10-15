//! Whether an OpenCode session is busy or idle, read from its own server
//! (`opencode serve`) over a plain HTTP GET with basic auth.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};

use legion_proto::Activity;
use serde_json::Value;

use crate::constants::{OPENCODE_ACTIVE_PATH, OPENCODE_PASSWORD_BYTES, OPENCODE_REQUEST_TIMEOUT, OPENCODE_SESSIONS_PATH, OPENCODE_USER, SESSION_START_GRACE};

/// Where a session's server listens, and the password Legion gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCodeServer {
    pub port: u16,
    pub password: String,
}

/// What Legion has seen of a server so far.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub has_answered: bool,
    pub has_session: bool,
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

fn has_session_in(body: &str) -> Result<bool, String> {
    let reply: Value = serde_json::from_str(body).map_err(|error| format!("not JSON: {error}"))?;
    let sessions = reply.get("data").and_then(Value::as_array).ok_or("no data list")?;
    Ok(!sessions.is_empty())
}

const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    bytes
        .chunks(3)
        .flat_map(|chunk| {
            let word = chunk.iter().enumerate().fold(0u32, |word, (at, byte)| word | (*byte as u32) << (16 - 8 * at));
            (0..4).map(move |at| match at <= chunk.len() {
                true => BASE64_ALPHABET[(word >> (18 - 6 * at) & 63) as usize] as char,
                false => '=',
            })
        })
        .collect()
}

fn chunked_body(mut rest: &str) -> Result<String, String> {
    let mut body = String::new();
    loop {
        let (size_line, after) = rest.split_once("\r\n").ok_or("cut off chunk")?;
        let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16).map_err(|_| "bad chunk size".to_string())?;
        if size == 0 {
            return Ok(body);
        }
        body.push_str(after.get(..size).ok_or("cut off chunk")?);
        rest = after.get(size..).and_then(|tail| tail.strip_prefix("\r\n")).ok_or("cut off chunk")?;
    }
}

/// The body of a 200 reply; any other status is an error.
fn body_of(raw: &str) -> Result<String, String> {
    let (head, body) = raw.split_once("\r\n\r\n").ok_or("no headers")?;
    let status = head.lines().next().unwrap_or("");
    if status.split_whitespace().nth(1) != Some("200") {
        return Err(status.to_string());
    }
    match head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        true => chunked_body(body),
        false => Ok(body.to_string()),
    }
}

fn get(server: &OpenCodeServer, path: &str) -> Result<String, String> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, server.port));
    let mut stream = TcpStream::connect_timeout(&address, OPENCODE_REQUEST_TIMEOUT).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(OPENCODE_REQUEST_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(OPENCODE_REQUEST_TIMEOUT)))
        .map_err(|error| error.to_string())?;
    let login = base64(format!("{OPENCODE_USER}:{}", server.password).as_bytes());
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Basic {login}\r\nAccept: application/json\r\nConnection: close\r\n\r\n", server.port);
    stream.write_all(request.as_bytes()).map_err(|error| error.to_string())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|error| error.to_string())?;
    body_of(&raw)
}

/// Asks the server. The list of sessions is only asked for while none has
/// been seen, since a running one already shows there is one.
pub fn ask_server(server: &OpenCodeServer, progress: Progress) -> Reply {
    let Ok(is_running) = get(server, OPENCODE_ACTIVE_PATH).and_then(|body| is_running_in(&body)) else { return Reply::Silent };
    if is_running || progress.has_session {
        return Reply::Answered { is_running, has_session: true };
    }
    match get(server, OPENCODE_SESSIONS_PATH).and_then(|body| has_session_in(&body)) {
        Ok(has_session) => Reply::Answered { is_running, has_session },
        Err(_) => Reply::Silent,
    }
}

/// A password only Legion and the session's own script know.
pub fn new_server(port: u16) -> Result<OpenCodeServer, String> {
    let mut random = [0u8; OPENCODE_PASSWORD_BYTES];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|error| format!("can't make a password: {error}"))?;
    Ok(OpenCodeServer { port, password: random.iter().map(|byte| format!("{byte:02x}")).collect() })
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, thread};

    use super::*;

    const SOON: Duration = Duration::from_secs(1);
    const LATE: Duration = SESSION_START_GRACE;

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

    #[test]
    fn the_session_list_says_whether_one_exists() {
        assert_eq!(has_session_in(r#"{"data":[],"cursor":{}}"#), Ok(false));
        assert_eq!(has_session_in(r#"{"data":[{"id":"ses_1"}],"cursor":{}}"#), Ok(true));
    }

    #[test]
    fn base64_pads_like_the_standard() {
        assert_eq!(base64(b"opencode:pw"), "b3BlbmNvZGU6cHc=");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    #[test]
    fn a_chunked_body_is_joined() {
        let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"dat\r\n4\r\na\":1\r\n0\r\n\r\n";
        assert_eq!(body_of(raw).unwrap(), "{\"data\":1");
        assert_eq!(body_of("HTTP/1.1 401 Unauthorized\r\n\r\n").unwrap_err(), "HTTP/1.1 401 Unauthorized");
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

    #[test]
    fn a_server_with_a_running_session_is_busy_and_gets_the_password() {
        let (server, serving) = fake_server(vec![Some(r#"{"data":{"ses_1":{"type":"running"}}}"#)]);
        assert_eq!(ask_server(&server, Progress::default()), answered(true, true));
        let requests = serving.join().unwrap();
        assert!(requests[0].starts_with("GET /api/session/active HTTP/1.1"), "{}", requests[0]);
        assert!(requests[0].contains("Authorization: Basic b3BlbmNvZGU6cHc="), "{}", requests[0]);
    }

    #[test]
    fn an_idle_server_is_asked_for_its_sessions_until_one_exists() {
        let (server, serving) = fake_server(vec![Some(r#"{"data":{}}"#), Some(r#"{"data":[]}"#), Some(r#"{"data":{}}"#), Some(r#"{"data":[{"id":"s"}]}"#)]);
        assert_eq!(ask_server(&server, Progress::default()), answered(false, false));
        assert_eq!(ask_server(&server, Progress::default()), answered(false, true));
        serving.join().unwrap();
    }

    #[test]
    fn a_session_already_seen_isnt_listed_again() {
        let (server, serving) = fake_server(vec![Some(r#"{"data":{}}"#)]);
        assert_eq!(ask_server(&server, Progress { has_answered: true, has_session: true }), answered(false, true));
        assert_eq!(serving.join().unwrap().len(), 1);
    }

    #[test]
    fn a_refusal_a_closed_connection_and_bad_json_are_silence() {
        let (server, serving) = fake_server(vec![Some(""), None, Some("not json")]);
        assert_eq!(ask_server(&server, Progress::default()), Reply::Silent);
        assert_eq!(ask_server(&server, Progress::default()), Reply::Silent);
        assert_eq!(ask_server(&server, Progress::default()), Reply::Silent);
        serving.join().unwrap();
        assert_eq!(ask_server(&server, Progress::default()), Reply::Silent, "nothing listens any more");
    }

    #[test]
    fn each_session_gets_its_own_password() {
        let (first, second) = (new_server(1).unwrap(), new_server(1).unwrap());
        assert_eq!(first.password.len(), OPENCODE_PASSWORD_BYTES * 2);
        assert_ne!(first.password, second.password);
    }
}
