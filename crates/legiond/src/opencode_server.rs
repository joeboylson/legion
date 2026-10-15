//! The client for an OpenCode session's own server (`opencode serve`): its
//! address, its basic-auth login and one request at a time with a time limit.

use std::{io::Read, net::Ipv4Addr};

use serde_json::Value;

use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, header::AUTHORIZATION, Method, Request};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

use crate::ids::new_id;
use crate::constants::{OPENCODE_PASSWORD_BYTES, OPENCODE_REQUEST_TIMEOUT, OPENCODE_SESSIONS_PATH, OPENCODE_USER};

/// Where a session's server listens, the password Legion gave it, and the
/// title (`tag`) its one OpenCode conversation is made with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCodeServer {
    pub port: u16,
    pub password: String,
    pub tag: String,
}

/// A password only Legion and the session's own script know.
pub fn new_server(port: u16) -> Result<OpenCodeServer, String> {
    let mut random = [0u8; OPENCODE_PASSWORD_BYTES];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|error| format!("can't make a password: {error}"))?;
    Ok(OpenCodeServer { port, password: random.iter().map(|byte| format!("{byte:02x}")).collect(), tag: new_id() })
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

async fn send(server: &OpenCodeServer, method: Method, path: &str, body: Option<String>) -> Result<String, String> {
    let login = base64(format!("{OPENCODE_USER}:{}", server.password).as_bytes());
    let request = Request::builder()
        .method(method)
        .uri(format!("http://{}:{}{path}", Ipv4Addr::LOCALHOST, server.port))
        .header(AUTHORIZATION, format!("Basic {login}"))
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.unwrap_or_default())))
        .map_err(|error| error.to_string())?;
    // No pooling: a server that goes away shouldn't leave a dead connection to try first.
    let client = Client::builder(TokioExecutor::new()).pool_max_idle_per_host(0).build_http::<Full<Bytes>>();
    let reply = client.request(request).await.map_err(|error| error.to_string())?;
    let status = reply.status();
    let body = reply.into_body().collect().await.map_err(|error| error.to_string())?.to_bytes();
    match status.is_success() {
        true => String::from_utf8(body.to_vec()).map_err(|error| error.to_string()),
        false => Err(format!("{status}")),
    }
}

/// A request that gives up after OPENCODE_REQUEST_TIMEOUT. The body of a 2xx reply, or why not.
async fn request(server: &OpenCodeServer, method: Method, path: &str, body: Option<String>) -> Result<String, String> {
    tokio::time::timeout(OPENCODE_REQUEST_TIMEOUT, send(server, method, path, body)).await.unwrap_or_else(|_| Err("timed out".to_string()))
}

pub async fn get(server: &OpenCodeServer, path: &str) -> Result<String, String> {
    request(server, Method::GET, path, None).await
}

/// Where to ask for the sessions titled with this server's tag: the plain list holds only the newest few.
pub fn sessions_search_path(server: &OpenCodeServer) -> String {
    format!("{OPENCODE_SESSIONS_PATH}?search={}", server.tag)
}

/// Posts a JSON body.
pub async fn post(server: &OpenCodeServer, path: &str, body: &str) -> Result<String, String> {
    request(server, Method::POST, path, Some(body.to_string())).await
}

/// The server lists every session it has stored, from every folder and run,
/// and two operators may share a folder, so a session is this one's only if
/// its title is the tag Legion gave it. Its ID, if there is one; two are an
/// error, since which is this session's can't be told.
pub fn this_runs_session(reply: &str, tag: &str) -> Result<Option<String>, String> {
    let reply: Value = serde_json::from_str(reply).map_err(|error| format!("not JSON: {error}"))?;
    let sessions = reply.get("data").and_then(Value::as_array).ok_or("no data list")?;
    let mut matches = sessions.iter().filter(|session| session.get("title").and_then(Value::as_str) == Some(tag));
    match (matches.next(), matches.next()) {
        (Some(_), Some(_)) => Err(format!("more than one OpenCode session is titled {tag}")),
        (Some(session), None) => Ok(session.get("id").and_then(Value::as_str).map(str::to_string)),
        (None, _) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;
    use crate::opencode_testing::fake_server;

    #[test]
    fn base64_pads_like_the_standard() {
        assert_eq!(base64(b"opencode:pw"), "b3BlbmNvZGU6cHc=");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    #[test]
    fn only_the_session_with_our_title_is_ours() {
        let list = r#"{"data":[
            {"id":"ses_old","title":"abc","time":{"created":500},"location":{"directory":"/work"}},
            {"id":"ses_other","title":"someone else","time":{"created":2000},"location":{"directory":"/work"}},
            {"id":"ses_ours","title":"tag-1","time":{"created":1500},"location":{"directory":"/work"}},
            {"id":"ses_later","title":"tag-2","time":{"created":3000},"location":{"directory":"/work"}}]}"#;
        assert_eq!(this_runs_session(list, "tag-1"), Ok(Some("ses_ours".into())), "newer sessions in the same folder aren't ours");
        assert_eq!(this_runs_session(list, "tag-3"), Ok(None));
        assert_eq!(this_runs_session(r#"{"data":[]}"#, "tag-1"), Ok(None));
        assert!(this_runs_session("nope", "tag-1").is_err());
    }

    #[test]
    fn two_sessions_with_our_title_are_refused() {
        let list = r#"{"data":[{"id":"a","title":"tag-1"},{"id":"b","title":"tag-1"}]}"#;
        assert_eq!(this_runs_session(list, "tag-1"), Err("more than one OpenCode session is titled tag-1".to_string()));
    }

    #[test]
    fn each_session_gets_its_own_password() {
        let (first, second) = (new_server(1).unwrap(), new_server(1).unwrap());
        assert_eq!(first.password.len(), OPENCODE_PASSWORD_BYTES * 2);
        assert_ne!(first.password, second.password);
        assert_ne!(first.tag, second.tag);
    }

    #[tokio::test]
    async fn a_chunked_reply_is_joined() {
        let (server, serving) = fake_server(vec![Some("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n{\"dat\r\n4\r\na\":1\r\n0\r\n\r\n".to_string())]);
        assert_eq!(get(&server, "/x").await.unwrap(), "{\"data\":1");
        assert!(serving.join().unwrap()[0].contains("authorization: Basic b3BlbmNvZGU6cHc="));
    }

    #[tokio::test]
    async fn a_post_sends_its_body() {
        let (server, serving) = fake_server(vec![Some("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string())]);
        assert_eq!(post(&server, "/api/x", r#"{"a":1}"#).await.unwrap(), "{}");
        let request = serving.join().unwrap().remove(0);
        assert!(request.starts_with("POST /api/x HTTP/1.1") && request.ends_with(r#"{"a":1}"#), "{request}");
    }

    #[tokio::test]
    async fn a_refusal_is_an_error_and_a_silent_server_times_out() {
        let (server, serving) = fake_server(vec![Some("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string())]);
        assert!(get(&server, "/x").await.unwrap_err().contains("401"));
        serving.join().unwrap();
        let silent = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = OpenCodeServer { port: silent.local_addr().unwrap().port(), password: "pw".into(), tag: "tag-1".into() };
        assert_eq!(get(&server, "/x").await.unwrap_err(), "timed out");
    }
}
