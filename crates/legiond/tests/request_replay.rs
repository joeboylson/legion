//! A call with a key runs once, even when its connection drops mid-run and
//! the caller asks again on a fresh one.

mod common;

use common::ThrowawayLegion;
use futures_util::{SinkExt, StreamExt};
use legion_proto::{Command, Outcome, Reply, Request, ServerMessage, ENV_SOCKET};
use tokio::net::UnixStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

async fn dial() -> WebSocketStream<UnixStream> {
    let stream = UnixStream::connect(std::env::var_os(ENV_SOCKET).unwrap()).await.unwrap();
    tokio_tungstenite::client_async("ws://legiond/ws", stream).await.unwrap().0
}

async fn send(websocket: &mut WebSocketStream<UnixStream>, key: &str, command: Command) {
    let request = Request { id: "1".into(), from: None, key: Some(key.into()), command };
    websocket.send(Message::Text(serde_json::to_string(&request).unwrap().into())).await.unwrap();
}

async fn outcome_of(websocket: &mut WebSocketStream<UnixStream>) -> Outcome {
    loop {
        let Message::Text(text) = websocket.next().await.unwrap().unwrap() else { continue };
        let ServerMessage::Reply { outcome, .. } = serde_json::from_str(&text).unwrap() else { continue };
        return outcome;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repeated_key_adds_one_mission_and_gets_the_same_reply() {
    let mut legion = ThrowawayLegion::start("replay").await;
    legion.set_up_folder().await;
    let deployment = legion.start_deployment_with_a_mission().await;
    let add = || Command::MissionAdd { deployment: deployment.clone(), title: "once".into(), body: "Say echo.".into() };

    let mut dropped = dial().await;
    send(&mut dropped, "call-1", add()).await;
    drop(dropped);
    let mut fresh = dial().await;
    send(&mut fresh, "call-1", add()).await;
    let first = outcome_of(&mut fresh).await;
    let mut later = dial().await;
    send(&mut later, "call-1", add()).await;
    let second = outcome_of(&mut later).await;

    assert!(matches!(first, Outcome::Ok(Reply::Missions { .. })), "{first:?}");
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    let Reply::Missions { missions } = legion.send(Command::MissionList { deployment: deployment.clone() }).await else { panic!("no missions") };
    assert_eq!(missions.len(), 2, "the start's mission and one \"once\"");
}
