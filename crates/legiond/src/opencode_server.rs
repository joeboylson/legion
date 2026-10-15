//! The client for an OpenCode session's own server (`opencode serve`): its
//! address, its basic-auth login and one request at a time with a time limit.

use std::{io::Read, net::Ipv4Addr};

use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, header::AUTHORIZATION, Method, Request};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

use crate::constants::{OPENCODE_PASSWORD_BYTES, OPENCODE_REQUEST_TIMEOUT, OPENCODE_USER};

/// Where a session's server listens, and the password Legion gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCodeServer {
    pub port: u16,
    pub password: String,
}

/// A password only Legion and the session's own script know.
pub fn new_server(port: u16) -> Result<OpenCodeServer, String> {
    let mut random = [0u8; OPENCODE_PASSWORD_BYTES];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|error| format!("can't make a password: {error}"))?;
    Ok(OpenCodeServer { port, password: random.iter().map(|byte| format!("{byte:02x}")).collect() })
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

/// For the next mission, which sends prompts to the server.
#[allow(dead_code)]
pub async fn post(server: &OpenCodeServer, path: &str, body: &str) -> Result<String, String> {
    request(server, Method::POST, path, Some(body.to_string())).await
}

#[cfg(test)]
mod tests {
    use std::{io::Write, net::TcpListener, thread};

    use super::*;

    #[test]
    fn base64_pads_like_the_standard() {
        assert_eq!(base64(b"opencode:pw"), "b3BlbmNvZGU6cHc=");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    #[test]
    fn each_session_gets_its_own_password() {
        let (first, second) = (new_server(1).unwrap(), new_server(1).unwrap());
        assert_eq!(first.password.len(), OPENCODE_PASSWORD_BYTES * 2);
        assert_ne!(first.password, second.password);
    }

    fn one_reply(reply: &'static str) -> (OpenCodeServer, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = OpenCodeServer { port: listener.local_addr().unwrap().port(), password: "pw".into() };
        let serving = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (server, serving)
    }

    #[tokio::test]
    async fn a_chunked_reply_is_joined() {
        let (server, serving) = one_reply("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n{\"dat\r\n4\r\na\":1\r\n0\r\n\r\n");
        assert_eq!(get(&server, "/x").await.unwrap(), "{\"data\":1");
        assert!(serving.join().unwrap().contains("authorization: Basic b3BlbmNvZGU6cHc="));
    }

    #[tokio::test]
    async fn a_post_sends_its_body() {
        let (server, serving) = one_reply("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        assert_eq!(post(&server, "/api/x", r#"{"a":1}"#).await.unwrap(), "{}");
        let request = serving.join().unwrap();
        assert!(request.starts_with("POST /api/x HTTP/1.1") && request.ends_with(r#"{"a":1}"#), "{request}");
    }

    #[tokio::test]
    async fn a_refusal_is_an_error_and_a_silent_server_times_out() {
        let (server, serving) = one_reply("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        assert!(get(&server, "/x").await.unwrap_err().contains("401"));
        serving.join().unwrap();
        let silent = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = OpenCodeServer { port: silent.local_addr().unwrap().port(), password: "pw".into() };
        assert_eq!(get(&server, "/x").await.unwrap_err(), "timed out");
    }
}
