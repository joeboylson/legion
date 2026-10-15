//! Two copies of one operator hold two missions. Both are paused, stopped,
//! resumed and started fresh; neither new session gets a pause note that is
//! stale or another mission's, whichever way its messages reach it.

mod common;

use std::{
    fs,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use common::{argument_after, opencode_like_template, serve_opencode, wait_for, ThrowawayLegion, OPERATOR};
use legion_proto::{Command, EntryKind, NewEntry, Reply};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

const NOTE_ON_57: &str = "pause note for the first mission";
const NOTE_ON_58: &str = "pause note for the second mission";
const LATER_MESSAGE: &str = "a message sent after the fresh start";
const SETTLE: Duration = Duration::from_secs(3);

struct Pair {
    legion: ThrowawayLegion,
    deployment: String,
}

fn entry(kind: EntryKind, mission: u32, text: &str) -> NewEntry {
    NewEntry { kind, mission: Some(mission), to: None, text: text.into(), answers: None }
}

impl Pair {
    /// A deployment with missions 1 and 2, and room for two copies of the operator.
    async fn start(name: &str, model: Option<&str>) -> Pair {
        let mut legion = ThrowawayLegion::start(name).await;
        legion.set_up_folder().await;
        if model.is_some() {
            let settings_path = legion.setup_file("legion.json");
            let mut settings: serde_json::Value = serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
            settings["models"] = serde_json::json!({ "fake-model": { "harness": "fake", "address": "http://fake:1/v1", "model": "fake-1" } });
            fs::write(&settings_path, settings.to_string()).unwrap();
            fs::create_dir_all(legion.setup_file("harnesses")).unwrap();
            fs::write(legion.setup_file("harnesses/fake.json"), opencode_like_template()).unwrap();
        }
        let folder = legion.folder();
        legion.send(Command::OperatorSet { folder, name: OPERATOR.into(), limit: Some(2), model: model.map(String::from), permission_mode: None }).await;
        let deployment = legion.start_deployment_with_a_mission().await;
        legion.send(Command::MissionAdd { deployment: deployment.clone(), title: "second".into(), body: "Say echo.".into() }).await;
        Pair { legion, deployment }
    }

    /// Starts a copy on the mission and says which position it got.
    async fn start_on(&mut self, mission: u32) -> String {
        let command = Command::SessionStart { deployment: self.deployment.clone(), operator: OPERATOR.into(), mission: Some(mission), part: None };
        let Reply::Session { session } = self.legion.send(command).await else { panic!("no session") };
        session.position
    }

    async fn post(&mut self, entry: NewEntry) {
        self.legion.send(Command::Post { deployment: self.deployment.clone(), entry }).await;
    }

    async fn stop(&mut self, position: &str) {
        self.legion.send(Command::SessionStop { deployment: self.deployment.clone(), position: position.into() }).await;
        loop {
            let sessions = self.legion.sessions(&self.deployment).await;
            if !sessions.iter().any(|session| session.position == position && session.activity != legion_proto::Activity::Ended) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Pauses both missions, stops both copies, resumes the first mission
    /// only, and starts a fresh copy on each, the second mission first so the
    /// copies swap positions as in the incident. Returns the fresh copies'
    /// positions, in mission order. The old launch records go first, so the
    /// new ones are the fresh copies'.
    async fn pause_stop_resume_start(&mut self) -> (String, String) {
        let first = self.start_on(1).await;
        let second = self.start_on(2).await;
        assert_ne!(first, second, "two copies");
        self.post(entry(EntryKind::Paused, 1, NOTE_ON_57)).await;
        self.post(entry(EntryKind::Paused, 2, NOTE_ON_58)).await;
        self.stop(&first).await;
        self.stop(&second).await;
        self.post(entry(EntryKind::Resumed, 1, "go on")).await;
        [&first, &second].iter().for_each(|position| {
            let _ = fs::remove_file(self.legion.out.join(format!("{position}.txt")));
        });
        let fresh_second = self.start_on(2).await;
        let fresh_first = self.start_on(1).await;
        assert_eq!((&fresh_first, &fresh_second), (&second, &first), "the copies swapped missions");
        (fresh_first, fresh_second)
    }
}

async fn claude_inbox(socket: &std::path::Path, session: &str) -> String {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    stream.write_all(format!("GET /sessions/{session}/inbox HTTP/1.1\r\nHost: legiond\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    reply
}

fn claude_session_of(legion: &ThrowawayLegion, position: &str) -> String {
    let record = legion.record_of(position);
    record.lines().find_map(|line| line.strip_prefix("env: LEGION_ADDON_SESSION=")).unwrap().to_string()
}

async fn a_claude_session_gets_no_stale_or_other_mission_pause_note() {
    let mut pair = Pair::start("pause-claude", None).await;
    let (first, second) = pair.pause_stop_resume_start().await;
    let socket = pair.legion.data_file("legiond.sock");
    let (first_session, second_session) = (claude_session_of(&pair.legion, &first), claude_session_of(&pair.legion, &second));

    for session in [&first_session, &second_session] {
        let inbox = claude_inbox(&socket, session).await;
        assert!(inbox.starts_with("HTTP/1.1 204"), "nothing stale or another mission's: {inbox}");
    }

    pair.post(NewEntry { to: Some(second.clone()), ..entry(EntryKind::Message, 2, LATER_MESSAGE) }).await;
    let inbox = claude_inbox(&socket, &second_session).await;
    assert!(inbox.contains(LATER_MESSAGE) && !inbox.contains("pause note"), "a later message still arrives: {inbox}");
}

/// A server for a session started by the test, that takes every prompt.
fn serve_for(legion: &ThrowawayLegion, position: &str) -> Arc<Mutex<Vec<String>>> {
    let launch = legion.record_of(position);
    let port: u16 = argument_after(&launch, "--port").parse().unwrap();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    serve_opencode(port, argument_after(&launch, "--session"), Arc::new(AtomicBool::new(false)), prompts.clone());
    prompts
}

fn mentions(prompts: &Mutex<Vec<String>>, text: &str) -> bool {
    prompts.lock().unwrap().iter().any(|request| request.contains(text))
}

async fn an_opencode_session_gets_no_stale_or_other_mission_pause_note() {
    let mut pair = Pair::start("pause-opencode", Some("fake-model")).await;
    // The first sessions' servers never answer, so their pause notes stay undelivered.
    let (first, second) = pair.pause_stop_resume_start().await;
    let (first_prompts, second_prompts) = (serve_for(&pair.legion, &first), serve_for(&pair.legion, &second));

    pair.post(NewEntry { to: Some(second.clone()), ..entry(EntryKind::Message, 2, LATER_MESSAGE) }).await;
    wait_for(|| mentions(&second_prompts, LATER_MESSAGE).then_some(()));
    std::thread::sleep(SETTLE);
    for prompts in [&first_prompts, &second_prompts] {
        assert!(!mentions(prompts, NOTE_ON_57) && !mentions(prompts, NOTE_ON_58), "no pause note: {:?}", prompts.lock().unwrap());
    }
    assert!(!mentions(&first_prompts, LATER_MESSAGE), "a message for the second copy stays with it");
}

// One test: the client finds legiond through a process-wide variable, so two throwaway Legions can't run side by side.
#[tokio::test(flavor = "multi_thread")]
async fn neither_a_claude_nor_an_opencode_session_gets_a_stale_or_other_mission_pause_note() {
    a_claude_session_gets_no_stale_or_other_mission_pause_note().await;
    an_opencode_session_gets_no_stale_or_other_mission_pause_note().await;
}
