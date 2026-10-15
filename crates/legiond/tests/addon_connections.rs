//! The add-on's calls end their connections once answered, so Claude Code
//! can't keep them open; the WebSocket route is left alone.

mod common;

use common::ThrowawayLegion;
use legion_proto::ENV_SOCKET;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

async fn answer_to(request: &str) -> String {
    let mut stream = UnixStream::connect(std::env::var_os(ENV_SOCKET).unwrap()).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buffer = [0u8; 2048];
    let read = stream.read(&mut buffer).await.unwrap();
    String::from_utf8_lossy(&buffer[..read]).to_lowercase()
}

#[tokio::test(flavor = "multi_thread")]
async fn addon_answers_close_the_connection_and_the_websocket_route_does_not() {
    let _legion = ThrowawayLegion::start("addon-close").await;

    let report = answer_to("POST /sessions/none/state HTTP/1.1\r\nHost: legiond\r\nContent-Type: application/json\r\nContent-Length: 17\r\n\r\n{\"state\":\"idle\"}  ").await;
    assert!(report.starts_with("http/1.1 404"), "{report}");
    assert!(report.contains("connection: close"), "{report}");

    let inbox = answer_to("GET /sessions/none/inbox HTTP/1.1\r\nHost: legiond\r\n\r\n").await;
    assert!(inbox.starts_with("http/1.1 404"), "{inbox}");
    assert!(inbox.contains("connection: close"), "{inbox}");

    let websocket = answer_to("GET /ws HTTP/1.1\r\nHost: legiond\r\n\r\n").await;
    assert!(!websocket.contains("connection: close"), "{websocket}");
}
