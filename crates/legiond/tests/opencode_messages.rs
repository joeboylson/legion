//! A message to a session that runs its own OpenCode server is posted to
//! that server, and tried again until the server takes it.

mod common;

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use common::{fake_harness, wait_for, ThrowawayLegion, OPERATOR};
use legion_proto::{Command, EntryKind, NewEntry};

const MESSAGE: &str = "needs work: add a test";

fn opencode_like_template() -> String {
    let template = serde_json::json!({
        "name": "fake",
        "command": fake_harness(),
        "args": ["--port", "{port}", "--password", "{password}", "--session", "{session}", "--prompt", "{firstPrompt}"],
        "tools": "mcp",
        "toolName": "legion_{tool}"
    });
    serde_json::to_string_pretty(&template).unwrap()
}

fn argument_after(launch: &str, flag: &str) -> String {
    let arguments: Vec<&str> = launch.lines().filter_map(|line| line.strip_prefix("arg: ")).collect();
    arguments[arguments.iter().position(|argument| *argument == flag).unwrap() + 1].to_string()
}

fn answer(status: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

/// Plays OpenCode's server: lists one session, and takes prompts unless told to fail them.
fn serve_opencode(port: u16, tag: String, is_failing: Arc<AtomicBool>, prompts: Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut raw = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap_or(0);
                raw.extend_from_slice(&buffer[..count]);
                let text = String::from_utf8_lossy(&raw).to_string();
                let length = text.to_ascii_lowercase().split("content-length: ").nth(1).and_then(|rest| rest.split("\r\n").next()?.parse::<usize>().ok()).unwrap_or(0);
                if count == 0 || text.split_once("\r\n\r\n").is_some_and(|(_, body)| body.len() >= length) {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&raw).to_string();
            let reply = if request.starts_with("GET /api/session?search=") {
                answer(
                    "200 OK",
                    &format!(
                        r#"{{"data":[
                            {{"id":"ses_foreign","title":"someone else's","time":{{"created":99999999999999}}}},
                            {{"id":"ses_older","title":"an earlier run","time":{{"created":1}}}},
                            {{"id":"ses_1","title":{tag:?},"time":{{"created":99999999999998}}}}]}}"#
                    ),
                )
            } else if request.starts_with("POST /api/session/ses_1/prompt ") {
                prompts.lock().unwrap().push(request.clone());
                match is_failing.load(Ordering::SeqCst) {
                    true => answer("500 Internal Server Error", "no"),
                    false => answer("200 OK", "{}"),
                }
            } else {
                answer("404 Not Found", "")
            };
            let _ = stream.write_all(reply.as_bytes());
        }
    });
}

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
    assert_eq!(body["delivery"], "queue");
    assert!(body["text"].as_str().unwrap().ends_with(MESSAGE), "{body}");
}
