//! A message for an OpenCode session reaches it though its conversation
//! isn't listed yet, or the server's ID for it changed.

mod common;

use std::sync::{atomic::AtomicUsize, Arc, Mutex};

use common::{argument_after, opencode_like_template, serve_opencode_listing_late, wait_for, ThrowawayLegion, OPERATOR};
use legion_proto::{Command, EntryKind, NewEntry};

async fn start_session(name: &str) -> (ThrowawayLegion, String, u16, String) {
    let mut legion = ThrowawayLegion::start(name).await;
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
    let port = argument_after(&launch, "--port").parse().unwrap();
    let tag = argument_after(&launch, "--session");
    (legion, deployment, port, tag)
}

async fn say(legion: &mut ThrowawayLegion, deployment: &str, text: &str) {
    let entry = NewEntry { kind: EntryKind::Message, mission: Some(1), to: Some(OPERATOR.into()), text: text.into(), answers: None };
    legion.send(Command::Post { deployment: deployment.into(), entry }).await;
}

fn reached(prompts: &Mutex<Vec<String>>, session: &str, text: &str) -> Option<()> {
    prompts.lock().unwrap().iter().any(|request| request.starts_with(&format!("POST /api/session/{session}/prompt ")) && request.contains(text)).then_some(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_sent_before_the_conversation_exists_waits_and_arrives() {
    let (mut legion, deployment, port, tag) = start_session("opencode-late").await;
    let prompts = Arc::new(Mutex::new(Vec::new()));
    serve_opencode_listing_late(port, tag, Arc::new(AtomicUsize::new(4)), Arc::new(Mutex::new("ses_1".into())), prompts.clone());
    say(&mut legion, &deployment, "right after start").await;
    wait_for(|| reached(&prompts, "ses_1", "right after start"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_saved_conversation_the_server_dropped_is_found_again() {
    let (mut legion, deployment, port, tag) = start_session("opencode-dropped").await;
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let live = Arc::new(Mutex::new("ses_1".to_string()));
    serve_opencode_listing_late(port, tag, Arc::new(AtomicUsize::new(0)), live.clone(), prompts.clone());
    say(&mut legion, &deployment, "first").await;
    wait_for(|| reached(&prompts, "ses_1", "first"));
    *live.lock().unwrap() = "ses_2".into();
    say(&mut legion, &deployment, "second").await;
    wait_for(|| reached(&prompts, "ses_2", "second"));
}
