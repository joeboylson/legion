//! A message to a session that runs its own OpenCode server is posted to
//! that server, and tried again until the server takes it.

mod common;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use common::{argument_after, opencode_like_template, serve_opencode, wait_for, ThrowawayLegion, OPERATOR};
use legion_proto::{Command, EntryKind, NewEntry};

const MESSAGE: &str = "needs work: add a test";

fn times_seen(prompts: &Mutex<Vec<String>>) -> usize {
    prompts.lock().unwrap().iter().filter(|request| request.contains(MESSAGE)).count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_reaches_the_sessions_server_and_is_retried_until_taken() {
    let mut legion = ThrowawayLegion::start("opencode-messages").await;
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
    assert_eq!(argument_after(&launch, "--password").len(), 48, "Legion makes the password: {launch}");

    let is_failing = Arc::new(AtomicBool::new(true));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    serve_opencode(port, argument_after(&launch, "--session"), is_failing.clone(), prompts.clone());
    let entry = NewEntry { kind: EntryKind::Message, mission: Some(1), to: Some(OPERATOR.into()), text: MESSAGE.into(), answers: None };
    legion.send(Command::Post { deployment: deployment.clone(), entry }).await;

    wait_for(|| (times_seen(&prompts) >= 1).then_some(()));
    is_failing.store(false, Ordering::SeqCst);
    wait_for(|| (times_seen(&prompts) >= 2).then_some(()));
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!(times_seen(&prompts), 2, "one failed try, one that was taken, and no more");
    let taken = prompts.lock().unwrap().iter().rev().find(|request| request.contains(MESSAGE)).cloned().unwrap();
    assert!(taken.contains("authorization: Basic "), "{taken}");
    let body: serde_json::Value = serde_json::from_str(taken.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["delivery"], "steer", "the admin's messages steer a running turn");
    assert!(body["text"].as_str().unwrap().ends_with(MESSAGE), "{body}");
}
