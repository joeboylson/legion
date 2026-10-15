//! A stand-in for an OpenCode server, for the tests of what talks to one.

use std::{
    io::{Read, Write},
    net::TcpListener,
    thread::{self, JoinHandle},
};

use crate::opencode_server::OpenCodeServer;

/// A whole HTTP reply.
pub fn http(status: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

/// A 200 reply with this body.
pub fn ok(body: &str) -> Option<String> {
    Some(http("200 OK", body))
}

fn content_length(request: &str) -> usize {
    request.to_ascii_lowercase().split("content-length: ").nth(1).and_then(|rest| rest.split("\r\n").next()?.parse().ok()).unwrap_or(0)
}

/// Answers each connection with the next reply, or with nothing for None,
/// and gives back every request it was sent when joined.
pub fn fake_server(replies: Vec<Option<String>>) -> (OpenCodeServer, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = OpenCodeServer { port: listener.local_addr().unwrap().port(), password: "pw".into(), tag: "tag-1".into() };
    let serving = thread::spawn(move || {
        replies
            .into_iter()
            .map(|reply| {
                let (mut stream, _) = listener.accept().unwrap();
                let mut raw = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let count = stream.read(&mut buffer).unwrap_or(0);
                    raw.extend_from_slice(&buffer[..count]);
                    let text = String::from_utf8_lossy(&raw);
                    if count == 0 || text.split_once("\r\n\r\n").is_some_and(|(head, body)| body.len() >= content_length(head)) {
                        break;
                    }
                }
                if let Some(reply) = reply {
                    stream.write_all(reply.as_bytes()).unwrap();
                }
                String::from_utf8_lossy(&raw).into_owned()
            })
            .collect()
    });
    (server, serving)
}
