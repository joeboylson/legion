//! Serves legiond's socket. axum::serve has no idle limit, so this accepts
//! connections itself and closes any HTTP one that sends nothing for too long.

use std::{future::Future, time::Duration};

use axum::Router;
use hyper::server::conn::http1::{self, UpgradeableConnection};
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use legion_proto::NAME;
use tokio::{net::{UnixListener, UnixStream}, sync::watch, task::JoinSet};

use crate::constants::ACCEPT_RETRY_DELAY;

/// Answers connections until `stop` finishes, then lets the requests being
/// answered finish (a handed-out inbox message would be lost otherwise), up
/// to `drain_limit`. A connection's task ends when it upgrades to a
/// WebSocket, so those aren't waited for.
pub async fn serve_socket(listener: UnixListener, router: Router, idle_limit: Duration, drain_limit: Duration, stop: impl Future<Output = ()>) {
    tokio::pin!(stop);
    let (stopping, stopped) = watch::channel(false);
    let mut connections = JoinSet::new();
    loop {
        let stream = tokio::select! {
            _ = &mut stop => break,
            // Finished connections are cleared as they go, not kept until the end.
            Some(_) = connections.join_next() => continue,
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
        connections.spawn(serve_connection(connection, stopped.clone()));
    }
    drop(listener);
    let _ = stopping.send(true);
    let drained = tokio::time::timeout(drain_limit, async { while connections.join_next().await.is_some() {} }).await;
    if drained.is_err() {
        eprintln!("{NAME}d: {} connections were still open {drain_limit:?} after stopping; cut off", connections.len());
        connections.abort_all();
    }
}

/// hyper-util's graceful helper doesn't take connections that allow
/// upgrades, so this asks the connection to finish its request and close.
async fn serve_connection(connection: UpgradeableConnection<TokioIo<UnixStream>, TowerToHyperService<Router>>, mut stopped: watch::Receiver<bool>) {
    tokio::pin!(connection);
    tokio::select! {
        // A client that leaves or times out is not worth a log line.
        _ = &mut connection => return,
        _ = stopped.wait_for(|is_stopping| *is_stopping) => connection.as_mut().graceful_shutdown(),
    }
    let _ = connection.await;
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
        let closing = closing_after_each_answer(Router::new().route("/closing", get(|| async { "ok" })).route("/body", axum::routing::post(|body: String| async move { body })), Duration::from_millis(200));
        let router = Router::new().route("/plain", get(|| async { "ok" })).route("/ws", echo).merge(closing);
        tokio::spawn(serve_socket(listener, router, idle_limit, Duration::from_secs(10), std::future::pending()));
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
    async fn a_stalled_body_on_an_add_on_route_is_given_up_on() {
        let (mut stream, _file) = serving(Duration::from_secs(60)).await;
        stream.write_all(b"POST /body HTTP/1.1\r\nHost: legiond\r\nContent-Length: 4\r\n\r\nab").await.unwrap();
        let answered = tokio::time::timeout(Duration::from_secs(5), read_some(&mut stream)).await.expect("still waiting for the body");
        assert!(!answered.contains("200"), "{answered}");
        let closed = tokio::time::timeout(Duration::from_secs(5), read_some(&mut stream)).await;
        assert_eq!(closed.expect("still open"), "");
    }

    #[tokio::test]
    async fn a_request_in_flight_when_legiond_stops_still_gets_its_answer() {
        let file = SocketFile(std::env::temp_dir().join(format!("legiond-serving-{}-stop.sock", std::process::id())));
        let listener = UnixListener::bind(&file.0).unwrap();
        let slow = get(|| async {
            tokio::time::sleep(Duration::from_millis(400)).await;
            "done"
        });
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(serve_socket(listener, Router::new().route("/slow", slow), Duration::from_secs(60), Duration::from_secs(10), async { let _ = stopped.await; }));
        let mut stream = UnixStream::connect(&file.0).await.unwrap();
        stream.write_all(b"GET /slow HTTP/1.1\r\nHost: legiond\r\n\r\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop.send(()).unwrap();
        assert!(read_some(&mut stream).await.contains("200"));
        tokio::time::timeout(Duration::from_secs(5), serving).await.expect("didn't finish stopping").unwrap();
    }

    #[tokio::test]
    async fn a_handler_that_never_returns_and_half_sent_requests_and_an_open_websocket_do_not_hold_stopping_past_the_limit() {
        let file = SocketFile(std::env::temp_dir().join(format!("legiond-serving-{}-drain.sock", std::process::id())));
        let listener = UnixListener::bind(&file.0).unwrap();
        let stuck = get(|| std::future::pending::<&'static str>());
        let websocket = get(|upgrade: WebSocketUpgrade| async { upgrade.on_upgrade(|mut socket| async move { while socket.recv().await.is_some() {} }) });
        let router = Router::new().route("/stuck", stuck).route("/ws", websocket);
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(serve_socket(listener, router, Duration::from_secs(60), Duration::from_millis(300), async { let _ = stopped.await; }));
        let mut open = Vec::new();
        for request in ["GET /stuck HTTP/1.1\r\nHost: legiond\r\n\r\n", "GET /plain HTT", ""] {
            let mut stream = UnixStream::connect(&file.0).await.unwrap();
            stream.write_all(request.as_bytes()).await.unwrap();
            open.push(stream);
        }
        let (_websocket, _) = tokio_tungstenite::client_async("ws://legiond/ws", UnixStream::connect(&file.0).await.unwrap()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), serving).await.expect("held past the drain limit").unwrap();
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
