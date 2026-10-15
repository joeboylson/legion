//! A handoff from an OpenCode session ends its turn through its own server,
//! and a message to a position that isn't running is refused.

mod common;

use std::sync::{atomic::AtomicBool, Arc, Mutex};

use common::{argument_after, opencode_like_template, serve_opencode, wait_for, ThrowawayLegion, OPERATOR};
use legion_proto::{Command, EntryKind, NewEntry};

fn interrupts(requests: &Mutex<Vec<String>>) -> usize {
    requests.lock().unwrap().iter().filter(|request| request.starts_with("POST /api/session/ses_1/interrupt ")).count()
}

fn post(kind: EntryKind, to: Option<&str>, text: &str) -> NewEntry {
    NewEntry { kind, mission: Some(1), to: to.map(str::to_string), text: text.into(), answers: None }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handoff_ends_the_turn_and_a_send_to_nobody_fails() {
    let mut legion = ThrowawayLegion::start("opencode-turn-end").await;
    legion.set_up_folder().await;
    let settings_path = legion.setup_file("legion.json");
    let mut settings: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    settings["models"] = serde_json::json!({ "fake-model": { "harness": "fake", "address": "http://fake:1/v1", "model": "fake-1" } });
    std::fs::write(&settings_path, settings.to_string()).unwrap();
    std::fs::create_dir_all(legion.setup_file("harnesses")).unwrap();
    std::fs::write(legion.setup_file("harnesses/fake.json"), opencode_like_template()).unwrap();
    let deployment = legion.start_deployment_with_a_mission().await;
    legion.set_operator_model("fake-model").await;
    legion.start_operator(&deployment).await.unwrap();
    let launch = legion.record_of(OPERATOR);
    let port: u16 = argument_after(&launch, "--port").parse().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    serve_opencode(port, argument_after(&launch, "--session"), Arc::new(AtomicBool::new(false)), requests.clone());
    let mut session = legion.client_as(&deployment, OPERATOR).await;

    session.ask(Command::Post { deployment: deployment.clone(), entry: post(EntryKind::Note, None, "working") }).await.unwrap();
    assert_eq!(interrupts(&requests), 0, "a note doesn't end the turn");
    session.ask(Command::Post { deployment: deployment.clone(), entry: post(EntryKind::Handoff, None, "→ done: built") }).await.unwrap();
    wait_for(|| (interrupts(&requests) == 1).then_some(()));

    let refused = session.ask(Command::Post { deployment: deployment.clone(), entry: post(EntryKind::Message, Some("ghost"), "hello?") }).await.unwrap_err();
    assert_eq!(refused, "ghost isn't running: hand off to it instead, or ask the commander to start it");
    let log = legion.send(Command::Log { deployment: deployment.clone(), filter: Default::default() }).await;
    let legion_proto::Reply::Entries { entries } = log else { panic!("no log") };
    assert!(entries.iter().any(|entry| entry.kind == EntryKind::Note && entry.text.contains("message to ghost wasn't sent")), "{entries:?}");
    assert!(!entries.iter().any(|entry| entry.text == "hello?"), "the refused message isn't logged as sent");
}
