//! Talking to legiond over its WebSocket: shared by the legion command
//! and the app.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::{SinkExt, StreamExt};
use legion_proto::{Caller, Command, Outcome, Reply, Request, ServerMessage, ENV_POSITION, ENV_DEPLOYMENT, ENV_SOCKET, NAME};
use tokio::{net::UnixStream, time::timeout};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

/// The URL is only for the WebSocket handshake; the socket decides where it goes.
const HANDSHAKE_URL: &str = "ws://legiond/ws";

pub struct Client {
    socket_path: PathBuf,
    /// None until the first call, and again after a connection is dropped
    /// or given up on; the next call dials a fresh one.
    websocket: Option<WebSocketStream<UnixStream>>,
    caller: Option<Caller>,
    requests_sent: u64,
}

/// Why one try at a call failed.
enum Failure {
    /// The connection went away: worth one more try on a fresh one.
    Dropped(String),
    /// legiond got the call and answered with an error.
    Refused(String),
}

/// Inside a session legiond started, the deployment and position it speaks for.
fn caller_from_environment() -> Option<Caller> {
    let deployment = std::env::var(ENV_DEPLOYMENT).ok()?;
    let position = std::env::var(ENV_POSITION).ok()?;
    Some(Caller { deployment, position })
}

fn socket_path_from_environment() -> Result<PathBuf, String> {
    match std::env::var_os(ENV_SOCKET) {
        Some(path) => Ok(path.into()),
        None => legion_proto::socket_path().ok_or("HOME isn't set".into()),
    }
}

/// Unique across connections and processes: legiond tells a repeat of a call from a new call by it.
fn call_key(call_number: u64) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
    format!("{}-{nanos}-{call_number}", std::process::id())
}

fn didnt_answer(reason: &str) -> String {
    format!("Legion didn't answer: {reason}")
}

impl Client {
    /// A client that dials when it first has something to say.
    pub fn at(socket_path: PathBuf) -> Client {
        Client { socket_path, websocket: None, caller: caller_from_environment(), requests_sent: 0 }
    }

    pub fn from_environment() -> Result<Client, String> {
        Ok(Client::at(socket_path_from_environment()?))
    }

    pub async fn connect() -> Result<Client, String> {
        let mut client = Client::from_environment()?;
        client.dial().await?;
        Ok(client)
    }

    async fn dial(&mut self) -> Result<&mut WebSocketStream<UnixStream>, String> {
        if self.websocket.is_none() {
            let stream = UnixStream::connect(&self.socket_path)
                .await
                .map_err(|error| format!("can't reach {NAME}d at {} ({error}); is it running?", self.socket_path.display()))?;
            let (websocket, _) = tokio_tungstenite::client_async(HANDSHAKE_URL, stream)
                .await
                .map_err(|error| format!("can't talk to {NAME}d: {error}"))?;
            self.websocket = Some(websocket);
        }
        Ok(self.websocket.as_mut().expect("just dialled"))
    }

    /// Asks once and waits as long as it takes: for the command line, which a person can stop.
    pub async fn ask(&mut self, command: Command) -> Result<Reply, String> {
        self.try_ask(command, None).await.map_err(|failure| match failure {
            Failure::Dropped(reason) | Failure::Refused(reason) => reason,
        })
    }

    /// Always ends within `limit`, with an answer or an error. A dropped
    /// connection is retried once on a fresh one, under the same key so
    /// legiond doesn't run the call twice; a call that took too long isn't
    /// retried, and its connection is dropped so its late answer can't be
    /// read as a later call's.
    pub async fn ask_within(&mut self, command: Command, limit: Duration) -> Result<Reply, String> {
        let key = call_key(self.requests_sent + 1);
        let tries = async {
            match self.try_ask(command.clone(), Some(key.clone())).await {
                Err(Failure::Dropped(_)) => self.try_ask(command, Some(key)).await,
                outcome => outcome,
            }
        };
        let outcome = timeout(limit, tries).await;
        match outcome {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(Failure::Refused(error))) => Err(error),
            Ok(Err(Failure::Dropped(reason))) => Err(didnt_answer(&reason)),
            Err(_) => {
                self.websocket = None;
                // The call may have reached legiond and run, so say not to ask blindly again.
                Err(didnt_answer(&format!("no reply within {} seconds; it may still be carried out, so check the log before asking again", limit.as_secs_f64())))
            }
        }
    }

    async fn try_ask(&mut self, command: Command, key: Option<String>) -> Result<Reply, Failure> {
        let result = self.send_and_wait(command, key).await;
        if matches!(result, Err(Failure::Dropped(_))) {
            self.websocket = None;
        }
        result
    }

    async fn send_and_wait(&mut self, command: Command, key: Option<String>) -> Result<Reply, Failure> {
        self.requests_sent += 1;
        let request_id = self.requests_sent.to_string();
        let request = Request { id: request_id.clone(), from: self.caller.clone(), key, command };
        let text = serde_json::to_string(&request).map_err(|error| Failure::Refused(error.to_string()))?;
        let websocket = self.dial().await.map_err(Failure::Dropped)?;
        websocket.send(Message::Text(text.into())).await.map_err(|error| Failure::Dropped(error.to_string()))?;
        loop {
            let ServerMessage::Reply { id, outcome } = self.next_message().await.map_err(Failure::Dropped)? else { continue };
            let is_our_reply = id == request_id || id.is_empty();
            if !is_our_reply {
                continue;
            }
            return match outcome {
                Outcome::Ok(reply) => Ok(reply),
                Outcome::Error(error) => Err(Failure::Refused(error)),
            };
        }
    }

    pub async fn next_message(&mut self) -> Result<ServerMessage, String> {
        let closed = || format!("{NAME}d closed the connection");
        loop {
            let message = self
                .websocket
                .as_mut()
                .ok_or_else(closed)?
                .next()
                .await
                .ok_or_else(closed)?
                .map_err(|error| error.to_string())?;
            if let Message::Text(text) = message {
                return serde_json::from_str(&text).map_err(|error| format!("can't read {NAME}d's message: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::{net::UnixListener, sync::mpsc};

    use super::*;

    const TEST_LIMIT: Duration = Duration::from_millis(200);

    /// What a fake legiond does with a connection.
    #[derive(Clone, Copy)]
    enum Behaviour {
        /// Reads the call and never answers.
        Silent,
        /// Reads the call, then closes.
        Drop,
        Answer,
    }

    struct FakeLegiond {
        socket_path: PathBuf,
        keys_seen: mpsc::UnboundedReceiver<Option<String>>,
    }

    impl Drop for FakeLegiond {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }

    fn pong() -> Reply {
        Reply::Pong { version: "test".into() }
    }

    async fn read_request(websocket: &mut WebSocketStream<UnixStream>) -> Request {
        loop {
            if let Message::Text(text) = websocket.next().await.unwrap().unwrap() {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    async fn behave(stream: UnixStream, behaviour: Behaviour, keys_seen: mpsc::UnboundedSender<Option<String>>) {
        let mut websocket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let request = read_request(&mut websocket).await;
        keys_seen.send(request.key.clone()).unwrap();
        match behaviour {
            Behaviour::Silent => std::future::pending::<()>().await,
            Behaviour::Drop => drop(websocket),
            Behaviour::Answer => {
                let reply = ServerMessage::Reply { id: request.id, outcome: Outcome::Ok(pong()) };
                websocket.send(Message::Text(serde_json::to_string(&reply).unwrap().into())).await.unwrap();
                std::future::pending::<()>().await
            }
        }
    }

    /// Connection n (from 0) behaves as `behaviours[n]`; later ones answer.
    fn fake_legiond(name: &str, behaviours: Vec<Behaviour>) -> FakeLegiond {
        let socket_path = PathBuf::from(format!("/tmp/lgc-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let (keys_sender, keys_seen) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            for connection in 0.. {
                let (stream, _) = listener.accept().await.unwrap();
                let behaviour = behaviours.get(connection).copied().unwrap_or(Behaviour::Answer);
                tokio::spawn(behave(stream, behaviour, keys_sender.clone()));
            }
        });
        FakeLegiond { socket_path, keys_seen }
    }

    #[tokio::test]
    async fn a_legiond_that_never_replies_ends_in_an_error() {
        let legiond = fake_legiond("silent", vec![Behaviour::Silent]);
        let mut client = Client::at(legiond.socket_path.clone());
        let error = client.ask_within(Command::Ping, TEST_LIMIT).await.unwrap_err();
        assert!(error.starts_with("Legion didn't answer:"), "{error}");
        assert!(client.websocket.is_none(), "a timed-out connection must not be reused");
    }

    #[tokio::test]
    async fn the_call_after_a_timeout_uses_a_fresh_connection() {
        let legiond = fake_legiond("after-timeout", vec![Behaviour::Silent]);
        let mut client = Client::at(legiond.socket_path.clone());
        client.ask_within(Command::Ping, TEST_LIMIT).await.unwrap_err();
        assert!(matches!(client.ask_within(Command::Ping, TEST_LIMIT).await, Ok(Reply::Pong { .. })));
    }

    #[tokio::test]
    async fn a_dropped_connection_is_retried_once_with_the_same_key() {
        let mut legiond = fake_legiond("dropped-once", vec![Behaviour::Drop]);
        let mut client = Client::at(legiond.socket_path.clone());
        assert!(matches!(client.ask_within(Command::Ping, TEST_LIMIT).await, Ok(Reply::Pong { .. })));
        let (first, second) = (legiond.keys_seen.recv().await.unwrap(), legiond.keys_seen.recv().await.unwrap());
        assert!(first.is_some());
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn a_connection_dropped_twice_is_an_error() {
        let legiond = fake_legiond("dropped-twice", vec![Behaviour::Drop, Behaviour::Drop]);
        let mut client = Client::at(legiond.socket_path.clone());
        let error = client.ask_within(Command::Ping, TEST_LIMIT).await.unwrap_err();
        assert!(error.starts_with("Legion didn't answer:"), "{error}");
    }

    #[tokio::test]
    async fn no_legiond_is_an_error_not_a_hang() {
        let mut client = Client::at(PathBuf::from("/tmp/lgc-nothing-here.sock"));
        let error = client.ask_within(Command::Ping, TEST_LIMIT).await.unwrap_err();
        assert!(error.starts_with("Legion didn't answer: can't reach"), "{error}");
    }
}
