//! legion2d's one address: a Unix socket only this machine can reach.
//! Clients use its WebSocket; the add-on in each session uses two plain
//! routes.

use std::{path::Path, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as UrlPath, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use legion2_proto::{Activity, Command, Event, Outcome, Request, ServerMessage, NAME};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::{
    constants::{ADDON_INBOX_ROUTE, ADDON_STATE_ROUTE, WEBSOCKET_ROUTE},
    daemon::{ActivityReport, Daemon},
};

/// Clears a socket file left by a legion2d that died, but refuses if
/// another one is still listening on it.
pub fn claim_socket(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let is_another_listening = std::os::unix::net::UnixStream::connect(path).is_ok();
    if is_another_listening {
        return Err(format!("another {NAME}d is running ({})", path.display()));
    }
    std::fs::remove_file(path).map_err(|error| format!("can't remove {}: {error}", path.display()))
}

pub fn router(daemon: Arc<Daemon>) -> Router {
    Router::new()
        .route(WEBSOCKET_ROUTE, get(upgrade_to_websocket))
        .route(ADDON_STATE_ROUTE, post(receive_addon_report))
        .route(ADDON_INBOX_ROUTE, get(hand_out_deliveries))
        .with_state(daemon)
}

/// What the add-on posts. `addon` and `model` come once, when it connects;
/// `context` (how full the conversation is, in percent) once it's known.
#[derive(Deserialize)]
struct AddonReport {
    state: Activity,
    detail: Option<String>,
    addon: Option<String>,
    model: Option<String>,
    context: Option<u8>,
    /// What the session wrote at the end of its turn.
    answer: Option<String>,
}

async fn receive_addon_report(State(daemon): State<Arc<Daemon>>, UrlPath(session_id): UrlPath<String>, Json(report): Json<AddonReport>) -> StatusCode {
    let activity_report = ActivityReport { activity: report.state, detail: report.detail, addon_version: report.addon, model: report.model, context_percent: report.context, final_answer: report.answer };
    match daemon.record_activity(&session_id, activity_report) {
        true => StatusCode::NO_CONTENT,
        false => StatusCode::NOT_FOUND,
    }
}

async fn hand_out_deliveries(State(daemon): State<Arc<Daemon>>, UrlPath(session_id): UrlPath<String>) -> Response {
    match daemon.take_deliveries(&session_id) {
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Ok(Some(messages)) if messages.is_empty() => StatusCode::NO_CONTENT.into_response(),
        Ok(Some(messages)) => Json(serde_json::json!({ "messages": messages })).into_response(),
    }
}

async fn upgrade_to_websocket(State(daemon): State<Arc<Daemon>>, websocket: WebSocketUpgrade) -> Response {
    websocket.on_upgrade(move |socket| serve_client(daemon, socket))
}

async fn send_message(socket: &mut WebSocket, message: &ServerMessage) -> Result<(), String> {
    let text = serde_json::to_string(message).map_err(|error| error.to_string())?;
    socket.send(Message::Text(text.into())).await.map_err(|error| error.to_string())
}

async fn answer_request(daemon: &Arc<Daemon>, text: &str) -> ServerMessage {
    let request = match serde_json::from_str::<Request>(text) {
        Ok(request) => request,
        Err(error) => return ServerMessage::Reply { id: String::new(), outcome: Outcome::Error(format!("bad request: {error}")) },
    };
    let request_id = request.id.clone();
    let handling_daemon = daemon.clone();
    // Starting a session blocks briefly; keep it off the async threads.
    let result = tokio::task::spawn_blocking(move || handling_daemon.handle_command(request.from, request.command))
        .await
        .unwrap_or_else(|error| Err(format!("{NAME}d failed: {error}")));
    let outcome = match result {
        Ok(reply) => Outcome::Ok(reply),
        Err(error) => Outcome::Error(error),
    };
    ServerMessage::Reply { id: request_id, outcome }
}

fn asks_to_watch(text: &str) -> bool {
    serde_json::from_str::<Request>(text).is_ok_and(|request| matches!(request.command, Command::Watch))
}

/// One client: a reply to each request, and once it asks to watch, every
/// event too. Ends when the client goes away.
async fn serve_client(daemon: Arc<Daemon>, mut socket: WebSocket) {
    let mut watched_events: Option<broadcast::Receiver<Event>> = None;
    loop {
        let next_event = async {
            match watched_events.as_mut() {
                Some(receiver) => receiver.recv().await.ok(),
                None => std::future::pending().await,
            }
        };
        let sent = tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(Message::Text(text))) = incoming else {
                    if matches!(incoming, Some(Ok(_))) { continue }
                    break;
                };
                if asks_to_watch(&text) {
                    watched_events = Some(daemon.events.subscribe());
                }
                let reply = answer_request(&daemon, &text).await;
                send_message(&mut socket, &reply).await
            }
            Some(event) = next_event => send_message(&mut socket, &ServerMessage::Event { event }).await,
        };
        if sent.is_err() {
            break;
        }
    }
}
