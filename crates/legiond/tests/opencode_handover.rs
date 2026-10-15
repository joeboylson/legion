//! An OpenCode session whose conversation passes clearAt, as its own server
//! reports it, is asked for a handoff note and then started again fresh, as
//! a Claude session is.

mod common;

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use common::{argument_after, opencode_like_template, serve_opencode_listing_late, serve_opencode_with_context, wait_for, ThrowawayLegion, OPERATOR};
use legion_proto::{Activity, Command, EntryKind, NewEntry};

fn note_asked(prompts: &Mutex<Vec<String>>) -> bool {
    prompts.lock().unwrap().iter().any(|request| request.contains("Handoff:") && request.contains("90% full"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_opencode_conversation_writes_a_note_and_starts_again() {
    let mut legion = ThrowawayLegion::start("opencode-handover").await;
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
    let is_running = Arc::new(AtomicBool::new(false));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    serve_opencode_with_context(port, argument_after(&launch, "--session"), 90_000, is_running.clone(), prompts.clone());

    wait_for(|| note_asked(&prompts).then_some(()));
    // The turn that writes the note: busy, then idle again.
    is_running.store(true, Ordering::SeqCst);
    while legion.sessions(&deployment).await.iter().all(|session| session.activity != Activity::Busy) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    is_running.store(false, Ordering::SeqCst);

    let fresh = wait_for(|| std::fs::read_to_string(legion.out.join(format!("{OPERATOR}.txt"))).ok().filter(|record| record.contains("fresh conversation")));
    assert!(fresh.contains("Read your 'Handoff:' note in mission 1"), "{fresh}");

    // The fresh session's server lists its conversation late; a message sent at once still arrives.
    let fresh_prompts = Arc::new(Mutex::new(Vec::new()));
    let fresh_port: u16 = argument_after(&fresh, "--port").parse().unwrap();
    serve_opencode_listing_late(fresh_port, argument_after(&fresh, "--session"), Arc::new(AtomicUsize::new(3)), Arc::new(Mutex::new("ses_1".into())), fresh_prompts.clone());
    let entry = NewEntry { kind: EntryKind::Message, mission: Some(1), to: Some(OPERATOR.into()), text: "after the handover".into(), answers: None };
    legion.send(Command::Post { deployment, entry }).await;
    wait_for(|| fresh_prompts.lock().unwrap().iter().any(|request| request.contains("after the handover")).then_some(()));
}
