//! An OpenCode conversation that passes clearAt in the middle of a turn is
//! asked to wrap up then, not when the turn ends; if it keeps growing, the
//! turn is stopped through its server and a fresh conversation starts.

mod common;

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use common::{argument_after, opencode_like_template, serve_opencode_mid_turn, wait_for, ThrowawayLegion, OPERATOR};
use legion_proto::{Activity, Command, EntryKind, Reply};

fn count_of(requests: &Mutex<Vec<String>>, start: &str) -> usize {
    requests.lock().unwrap().iter().filter(|request| request.starts_with(start)).count()
}

async fn log_texts(legion: &mut ThrowawayLegion, deployment: &str) -> Vec<String> {
    let Reply::Entries { entries } = legion.send(Command::Log { deployment: deployment.into(), filter: Default::default() }).await else { panic!("no log") };
    entries.into_iter().filter(|entry| entry.kind == EntryKind::Activity).map(|entry| entry.text).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_fills_the_conversation_is_asked_to_wrap_up_then_stopped_and_handed_over() {
    let mut legion = ThrowawayLegion::start("opencode-mid-turn").await;
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
    // A turn is running, and its conversation fills up while it does.
    let tokens = Arc::new(AtomicU64::new(30_000));
    let is_running = Arc::new(AtomicBool::new(true));
    let requests = Arc::new(Mutex::new(Vec::new()));
    serve_opencode_mid_turn(port, argument_after(&launch, "--session"), tokens.clone(), is_running.clone(), requests.clone());

    while legion.sessions(&deployment).await.iter().all(|session| session.activity != Activity::Busy) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    // Past clearAt (60%), not yet clearAt + 10.
    tokens.store(65_000, Ordering::SeqCst);
    wait_for(|| requests.lock().unwrap().iter().find(|request| request.contains("Wrap up now") && request.contains(r#""delivery":"steer""#)).map(|_| ()));
    assert_eq!(count_of(&requests, "POST /api/session/ses_1/interrupt "), 0, "the turn is given time to wrap up");

    tokens.store(75_000, Ordering::SeqCst);
    wait_for(|| (count_of(&requests, "POST /api/session/ses_1/interrupt ") == 1).then_some(()));

    let fresh = wait_for(|| std::fs::read_to_string(legion.out.join(format!("{OPERATOR}.txt"))).ok().filter(|record| record.contains("fresh conversation")));
    assert!(fresh.contains("Read your 'Handoff:' note in mission 1"), "{fresh}");
    assert_eq!(count_of(&requests, "POST /api/session/ses_1/prompt "), 1, "asked to wrap up once");
    let lines = log_texts(&mut legion, &deployment).await;
    let said = |part: &str| lines.iter().filter(|line| line.contains(part)).count();
    assert_eq!((said("asked it to wrap up"), said("stopped it, to hand over"), said("handing over to a fresh conversation")), (1, 1, 1), "{lines:?}");
}
