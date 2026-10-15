//! Serves legiond's socket. axum::serve has no idle limit, so this accepts
//! connections itself and closes any HTTP one that sends nothing for too long.

use std::{future::Future, time::Duration};

use axum::Router;
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use legion_proto::NAME;
use tokio::net::UnixListener;

use crate::constants::ACCEPT_RETRY_DELAY;

/// Answers connections until `stop` finishes. A connection left open when
/// legiond stops is dropped with the process.
pub async fn serve_socket(listener: UnixListener, router: Router, idle_limit: Duration, stop: impl Future<Output = ()>) {
    tokio::pin!(stop);
    loop {
        let stream = tokio::select! {
            _ = &mut stop => return,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(error) => {
                    eprintln!("{NAME}d: can't accept a connection: {error}");
                    tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
                    continue;
                }
            },
        };
        let connection = http1::Builder::new()
            .timer(TokioTimer::new())
            .header_read_timeout(idle_limit)
            .serve_connection(TokioIo::new(stream), TowerToHyperService::new(router.clone()))
            .with_upgrades();
        tokio::spawn(async move {
            // A client that leaves or times out is not worth a log line.
            let _ = connection.await;
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::{extract::ws::WebSocketUpgrade, routing::get, Router};
    use futures_util::{SinkExt, StreamExt};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
    };
    use tokio_tungstenite::tungstenite::Message;

    use super::*;
    use crate::server::closing_after_each_answer;

    const REQUEST: &str = "GET /plain HTTP/1.1\r\nHost: legiond\r\n\r\n";

    /// A socket file that goes away with the test.
    struct SocketFile(PathBuf);

    impl Drop for SocketFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    async fn serving(idle_limit: Duration) -> (UnixStream, SocketFile) {
        static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let number = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let file = SocketFile(std::env::temp_dir().join(format!("legiond-serving-{}-{number}.sock", std::process::id())));
        let listener = UnixListener::bind(&file.0).unwrap();
        let echo = get(|upgrade: WebSocketUpgrade| async {
            upgrade.on_upgrade(|mut socket| async move {
                while let Some(Ok(message)) = socket.recv().await {
                    let _ = socket.send(message).await;
                }
            })
        });
        let closing = closing_after_each_answer(Router::new().route("/closing", get(|| async { "ok" })));
        let router = Router::new().route("/plain", get(|| async { "ok" })).route("/ws", echo).merge(closing);
        tokio::spawn(serve_socket(listener, router, idle_limit, std::future::pending()));
        (UnixStream::connect(&file.0).await.unwrap(), file)
    }

    async fn read_some(stream: &mut UnixStream) -> String {
        let mut buffer = [0u8; 1024];
        let read = stream.read(&mut buffer).await.unwrap();
        String::from_utf8_lossy(&buffer[..read]).to_lowercase()
    }

    #[tokio::test]
    async fn an_idle_connection_is_closed_after_the_limit() {
        let (mut stream, _file) = serving(Duration::from_millis(200)).await;
        stream.write_all(REQUEST.as_bytes()).await.unwrap();
        assert!(read_some(&mut stream).await.contains("200"));
        let closed = tokio::time::timeout(Duration::from_secs(5), read_some(&mut stream)).await;
        assert_eq!(closed.expect("still open after the limit"), "");
    }

    #[tokio::test]
    async fn a_connection_in_use_stays_open() {
        let (mut stream, _file) = serving(Duration::from_millis(400)).await;
        for _ in 0..3 {
            stream.write_all(REQUEST.as_bytes()).await.unwrap();
            assert!(read_some(&mut stream).await.contains("200"));
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    #[tokio::test]
    async fn the_closing_layer_ends_the_connection_and_a_plain_route_keeps_it() {
        let (mut closing, _file) = serving(Duration::from_secs(60)).await;
        closing.write_all(b"GET /closing HTTP/1.1\r\nHost: legiond\r\n\r\n").await.unwrap();
        assert!(read_some(&mut closing).await.contains("connection: close"));
        assert_eq!(read_some(&mut closing).await, "", "the connection should be closed");

        let (mut plain, _file) = serving(Duration::from_secs(60)).await;
        plain.write_all(REQUEST.as_bytes()).await.unwrap();
        assert!(!read_some(&mut plain).await.contains("connection: close"));
    }

    #[tokio::test]
    async fn a_websocket_upgrades_and_outlives_the_idle_limit() {
        let (stream, _file) = serving(Duration::from_millis(200)).await;
        let (mut websocket, _) = tokio_tungstenite::client_async("ws://legiond/ws", stream).await.unwrap();
        tokio::time::sleep(Duration::from_millis(600)).await;
        websocket.send(Message::Text("hello".into())).await.unwrap();
        assert_eq!(websocket.next().await.unwrap().unwrap(), Message::Text("hello".into()));
    }
}
