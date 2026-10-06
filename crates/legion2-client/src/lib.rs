//! Talking to legion2d over its WebSocket: shared by the legion2 command
//! and the app.

use futures_util::{SinkExt, StreamExt};
use legion2_proto::{Caller, Command, Outcome, Reply, Request, ServerMessage, ENV_POSITION, ENV_DEPLOYMENT, ENV_SOCKET, NAME};
use tokio::net::UnixStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

/// The URL is only for the WebSocket handshake; the socket decides where it goes.
const HANDSHAKE_URL: &str = "ws://legion2d/ws";

pub struct Client {
    websocket: WebSocketStream<UnixStream>,
    caller: Option<Caller>,
    requests_sent: u64,
}

/// Inside a session legion2d started, the deployment and position it speaks for.
fn caller_from_environment() -> Option<Caller> {
    let deployment = std::env::var(ENV_DEPLOYMENT).ok()?;
    let position = std::env::var(ENV_POSITION).ok()?;
    Some(Caller { deployment, position })
}

impl Client {
    pub async fn connect() -> Result<Client, String> {
        let socket_path = match std::env::var_os(ENV_SOCKET) {
            Some(path) => path.into(),
            None => legion2_proto::socket_path().ok_or("HOME isn't set")?,
        };
        let stream = UnixStream::connect(&socket_path)
            .await
            .map_err(|error| format!("can't reach {NAME}d at {} ({error}); is it running?", socket_path.display()))?;
        let (websocket, _) = tokio_tungstenite::client_async(HANDSHAKE_URL, stream)
            .await
            .map_err(|error| format!("can't talk to {NAME}d: {error}"))?;
        Ok(Client { websocket, caller: caller_from_environment(), requests_sent: 0 })
    }

    pub async fn ask(&mut self, command: Command) -> Result<Reply, String> {
        self.requests_sent += 1;
        let request_id = self.requests_sent.to_string();
        let request = Request { id: request_id.clone(), from: self.caller.clone(), command };
        let text = serde_json::to_string(&request).map_err(|error| error.to_string())?;
        self.websocket.send(Message::Text(text.into())).await.map_err(|error| error.to_string())?;
        loop {
            let ServerMessage::Reply { id, outcome } = self.next_message().await? else { continue };
            let is_our_reply = id == request_id || id.is_empty();
            if !is_our_reply {
                continue;
            }
            return match outcome {
                Outcome::Ok(reply) => Ok(reply),
                Outcome::Error(error) => Err(error),
            };
        }
    }

    pub async fn next_message(&mut self) -> Result<ServerMessage, String> {
        loop {
            let message = self
                .websocket
                .next()
                .await
                .ok_or(format!("{NAME}d closed the connection"))?
                .map_err(|error| error.to_string())?;
            if let Message::Text(text) = message {
                return serde_json::from_str(&text).map_err(|error| format!("can't read {NAME}d's message: {error}"));
            }
        }
    }
}
