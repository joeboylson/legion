//! A throwaway Legion for tests: the built legiond under a temp HOME, with
//! the fake harness standing in for Claude Code, and a git folder set up
//! with one operator on a one-step pipeline.

// Each test file uses its own share of these.
#![allow(dead_code)]

use std::{
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    process::{Child, Command as Process, Stdio},
    time::{Duration, Instant},
};

use legion_client::Client;
use legion_proto::{Command, Reply, SessionInfo, ENV_DEPLOYMENT, ENV_POSITION, ENV_SOCKET};

pub const OPERATOR: &str = "echoer";
const PIPELINE: &str = "solo";
const WAIT_LIMIT: Duration = Duration::from_secs(20);
const POLL_GAP: Duration = Duration::from_millis(100);
/// Set by the shell or the terminal, not by Legion, and different per machine.
const MACHINE_ENV: [&str; 3] = ["SHLVL", "_", "SHELL"];

pub fn fake_harness() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-harness.sh"))
}

pub fn addon_folder() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../addon")).canonicalize().unwrap()
}

pub fn binary_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_legiond")).parent().unwrap().to_path_buf()
}

fn run(program: &str, arguments: &[&str], folder: &Path) {
    let status = Process::new(program).args(arguments).current_dir(folder).status().unwrap();
    assert!(status.success(), "{program} {arguments:?} failed");
}

pub struct ThrowawayLegion {
    /// Short, under /tmp: the socket's path must fit a Unix socket's limit.
    pub home: PathBuf,
    pub repo: PathBuf,
    pub out: PathBuf,
    legiond: Child,
    pub client: Client,
}

impl ThrowawayLegion {
    pub async fn start(name: &str) -> ThrowawayLegion {
        let home = PathBuf::from(format!("/tmp/lg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let data = home.join(".local/share/legion");
        let repo = home.join("repo");
        let out = home.join("out");
        [&data, &repo, &out].iter().for_each(|folder| std::fs::create_dir_all(folder).unwrap());
        std::fs::write(data.join("settings.json"), r#"{"webPort": 0}"#).unwrap();
        run("git", &["init", "-q"], &repo);
        run("git", &["-c", "user.name=test", "-c", "user.email=test@test", "commit", "-q", "--allow-empty", "-m", "init"], &repo);
        let addon = addon_folder();
        let fake = fake_harness();
        let legiond = Process::new(env!("CARGO_BIN_EXE_legiond"))
            .args(["--claude", fake.to_str().unwrap(), "--addon", addon.to_str().unwrap()])
            .env_clear()
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .env("FAKE_HARNESS_OUT", &out)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let socket = data.join("legiond.sock");
        wait_for(|| socket.exists().then_some(()));
        // The client reads these; this test process may itself run inside a Legion session.
        std::env::remove_var(ENV_DEPLOYMENT);
        std::env::remove_var(ENV_POSITION);
        std::env::set_var(ENV_SOCKET, &socket);
        let client = Client::connect().await.unwrap();
        ThrowawayLegion { home, repo, out, legiond, client }
    }

    pub fn folder(&self) -> String {
        self.repo.to_string_lossy().into_owned()
    }

    /// A second connection that speaks as the session at `position`, the way its tools do.
    pub async fn client_as(&self, deployment: &str, position: &str) -> Client {
        std::env::set_var(ENV_DEPLOYMENT, deployment);
        std::env::set_var(ENV_POSITION, position);
        let client = Client::connect().await.unwrap();
        std::env::remove_var(ENV_DEPLOYMENT);
        std::env::remove_var(ENV_POSITION);
        client
    }

    pub async fn send(&mut self, command: Command) -> Reply {
        self.client.ask(command).await.unwrap()
    }

    /// The folder, one operator, and a one-step pipeline with it.
    pub async fn set_up_folder(&mut self) {
        let folder = self.folder();
        self.send(Command::FolderAdd { path: folder.clone() }).await;
        let definition = "Say echo, then hand off.".to_string();
        self.send(Command::OperatorAdd { folder: folder.clone(), name: OPERATOR.into(), definition, limit: None }).await;
        self.send(Command::PipelineAdd { folder, name: PIPELINE.into(), operators: vec![OPERATOR.into()], in_order: true }).await;
    }

    /// Starts a deployment with one mission. Returns the deployment's ID.
    pub async fn start_deployment_with_a_mission(&mut self) -> String {
        let Reply::Deployment { deployment } = self.send(Command::DeploymentStart { folder: self.folder(), pipeline: PIPELINE.into(), name: None }).await else {
            panic!("no deployment");
        };
        self.send(Command::MissionAdd { deployment: deployment.id.clone(), title: "tiny".into(), body: "Say echo.".into() }).await;
        deployment.id
    }

    pub async fn start_operator(&mut self, deployment: &str) -> Result<Reply, String> {
        self.client.ask(Command::SessionStart { deployment: deployment.into(), operator: OPERATOR.into(), mission: Some(1), part: None }).await
    }

    /// Starts a deployment and the operator on its first mission. Returns
    /// the deployment's ID.
    pub async fn start_operator_on_a_mission(&mut self) -> String {
        let deployment = self.start_deployment_with_a_mission().await;
        self.start_operator(&deployment).await.unwrap();
        deployment
    }

    pub async fn set_operator_model(&mut self, model: &str) {
        self.send(Command::OperatorSet { folder: self.folder(), name: OPERATOR.into(), limit: None, model: Some(model.into()), permission_mode: None }).await;
    }

    pub fn setup_file(&self, name: &str) -> PathBuf {
        self.repo.join(".legion").join(name)
    }

    pub fn data_file(&self, name: &str) -> PathBuf {
        self.home.join(".local/share/legion").join(name)
    }

    pub async fn sessions(&mut self, deployment: &str) -> Vec<SessionInfo> {
        let Reply::Sessions { sessions } = self.send(Command::SessionList { deployment: Some(deployment.into()) }).await else { panic!("no sessions") };
        sessions
    }

    /// What the fake wrote when it started as `position`.
    pub fn record_of(&self, position: &str) -> String {
        let path = self.out.join(format!("{position}.txt"));
        wait_for(|| std::fs::read_to_string(&path).ok())
    }

    /// The record with this machine's paths and the session's IDs replaced
    /// by names, so it reads the same on every run.
    pub fn normalized(&self, record: &str, deployment: &str) -> String {
        let home = self.home.to_string_lossy().into_owned();
        let private_home = format!("/private{home}");
        record
            .lines()
            .filter(|line| !MACHINE_ENV.iter().any(|name| line.starts_with(&format!("env: {name}="))))
            .map(|line| match line.strip_prefix("env: LEGION_ADDON_SESSION=") {
                Some(_) => "env: LEGION_ADDON_SESSION=<session>".to_string(),
                None => line.to_string(),
            })
            .map(|line| {
                line.replace(&private_home, "<home>")
                    .replace(&home, "<home>")
                    .replace(binary_folder().to_str().unwrap(), "<bin>")
                    .replace(addon_folder().to_str().unwrap(), "<addon>")
                    .replace(deployment, "<deployment>")
            })
            .map(|line| without_folder_id(&line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Drop for ThrowawayLegion {
    fn drop(&mut self) {
        let _ = Process::new("kill").arg(self.legiond.id().to_string()).status();
        let _ = self.legiond.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// `folders/repo-02bc918b/` → `folders/repo-<id>/`: the ID differs each run.
fn without_folder_id(line: &str) -> String {
    let marker = "/folders/repo-";
    match line.find(marker) {
        Some(at) => {
            let id_start = at + marker.len();
            let id_end = line[id_start..].find('/').map_or(line.len(), |offset| id_start + offset);
            format!("{}<id>{}", &line[..id_start], &line[id_end..])
        }
        None => line.to_string(),
    }
}

pub fn wait_for<T>(check: impl Fn() -> Option<T>) -> T {
    let started = Instant::now();
    loop {
        if let Some(found) = check() {
            return found;
        }
        assert!(started.elapsed() < WAIT_LIMIT, "gave up waiting");
        std::thread::sleep(POLL_GAP);
    }
}

pub fn opencode_like_template() -> String {
    let template = serde_json::json!({
        "name": "fake",
        "command": fake_harness(),
        "args": ["--port", "{port}", "--password", "{password}", "--session", "{session}", "--prompt", "{firstPrompt}"],
        "tools": "mcp",
        "toolName": "legion_{tool}"
    });
    serde_json::to_string_pretty(&template).unwrap()
}

pub fn argument_after(launch: &str, flag: &str) -> String {
    let arguments: Vec<&str> = launch.lines().filter_map(|line| line.strip_prefix("arg: ")).collect();
    arguments[arguments.iter().position(|argument| *argument == flag).unwrap() + 1].to_string()
}

fn answer(status: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

/// Reads each request off a connection and writes back what `reply` makes of it.
pub fn serve_http(port: u16, reply: impl Fn(&str) -> String + Send + 'static) {
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
            let _ = stream.write_all(reply(&String::from_utf8_lossy(&raw)).as_bytes());
        }
    });
}

/// What OpenCode's server answers to listing sessions: ours among others.
pub fn session_list(tag: &str) -> String {
    answer(
        "200 OK",
        &format!(
            r#"{{"data":[
                {{"id":"ses_foreign","title":"someone else's","time":{{"created":99999999999999}}}},
                {{"id":"ses_older","title":"an earlier run","time":{{"created":1}}}},
                {{"id":"ses_1","title":{tag:?},"time":{{"created":99999999999998}}}}]}}"#
        ),
    )
}

/// Plays OpenCode's server: lists one session, and takes prompts unless told to fail them.
pub fn serve_opencode(port: u16, tag: String, is_failing: Arc<AtomicBool>, prompts: Arc<Mutex<Vec<String>>>) {
    serve_http(port, move |request| {
        if request.starts_with("GET /api/session?search=") {
            session_list(&tag)
        } else if request.starts_with("GET /api/session/ses_1/inbox ") {
            prompts.lock().unwrap().push(request.to_string());
            answer("200 OK", r#"{"data":[{"id":"msg_parked","delivery":"queue"}]}"#)
        } else if request.starts_with("POST /api/session/ses_1/prompt ") || request.starts_with("POST /api/session/ses_1/interrupt ") || request.starts_with("PATCH /api/session/ses_1/inbox/") {
            prompts.lock().unwrap().push(request.to_string());
            match is_failing.load(Ordering::SeqCst) {
                true => answer("500 Internal Server Error", "no"),
                false => answer("200 OK", "{}"),
            }
        } else {
            answer("404 Not Found", "")
        }
    });
}

/// Plays a server whose session has used `tokens` of a 100000-token context
/// on its newest step, is running a turn while `is_running`, and keeps the
/// prompts it is sent.
pub fn serve_opencode_with_context(port: u16, tag: String, tokens: u64, is_running: Arc<AtomicBool>, prompts: Arc<Mutex<Vec<String>>>) {
    serve_http(port, move |request| {
        if request.starts_with("GET /api/session/active ") {
            let running = is_running.load(Ordering::SeqCst);
            answer("200 OK", if running { r#"{"data":{"ses_1":{"type":"running"}}}"# } else { r#"{"data":{}}"# })
        } else if request.starts_with("GET /api/session?search=") {
            session_list(&tag)
        } else if request.starts_with("GET /api/session/ses_1/message?") {
            let step = format!(r#"{{"data":[{{"type":"assistant","model":{{"id":"m","providerID":"p"}},"tokens":{{"input":{tokens},"output":0,"reasoning":0,"cache":{{"read":0,"write":0}}}}}}]}}"#);
            answer("200 OK", &step)
        } else if request.starts_with("GET /api/model ") {
            answer("200 OK", r#"{"data":[{"id":"m","providerID":"p","limit":{"context":100000,"output":8192}}]}"#)
        } else if request.starts_with("POST /api/session/ses_1/prompt ") {
            prompts.lock().unwrap().push(request.to_string());
            answer("200 OK", "{}")
        } else {
            answer("404 Not Found", "")
        }
    });
}

/// Plays a server whose conversation for `tag` isn't listed for the first
/// `misses` lookups, then is listed as `live_id`. Prompts to `live_id` are
/// kept and taken; to any other ID they get 404, as a conversation the
/// server no longer has would.
pub fn serve_opencode_listing_late(port: u16, tag: String, misses: Arc<AtomicUsize>, live_id: Arc<Mutex<String>>, prompts: Arc<Mutex<Vec<String>>>) {
    serve_http(port, move |request| {
        let live = live_id.lock().unwrap().clone();
        if request.starts_with("GET /api/session?search=") {
            let is_missing = misses.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| left.checked_sub(1)).is_ok();
            match is_missing {
                true => answer("200 OK", r#"{"data":[]}"#),
                false => answer("200 OK", &format!(r#"{{"data":[{{"id":{live:?},"title":{tag:?},"time":{{"created":1}}}}]}}"#)),
            }
        } else if let Some(id) = request.strip_prefix("POST /api/session/").and_then(|rest| rest.split_once("/prompt ")).map(|(id, _)| id.to_string()) {
            match id == live {
                true => {
                    prompts.lock().unwrap().push(request.to_string());
                    answer("200 OK", "{}")
                }
                false => answer("404 Not Found", ""),
            }
        } else {
            answer("404 Not Found", "")
        }
    });
}

/// Like `serve_opencode_with_context`, but the tokens can grow while a turn runs, and the turn
/// keeps running after an interrupt (a parked message starting a new one), so only Legion can end the session.
pub fn serve_opencode_mid_turn(port: u16, tag: String, tokens: Arc<AtomicU64>, is_running: Arc<AtomicBool>, requests: Arc<Mutex<Vec<String>>>) {
    serve_http(port, move |request| {
        if request.starts_with("GET /api/session/active ") {
            let running = is_running.load(Ordering::SeqCst);
            answer("200 OK", if running { r#"{"data":{"ses_1":{"type":"running"}}}"# } else { r#"{"data":{}}"# })
        } else if request.starts_with("GET /api/session?search=") {
            session_list(&tag)
        } else if request.starts_with("GET /api/session/ses_1/message?") {
            let used = tokens.load(Ordering::SeqCst);
            let step = format!(r#"{{"data":[{{"type":"assistant","model":{{"id":"m","providerID":"p"}},"tokens":{{"input":{used},"output":0,"reasoning":0,"cache":{{"read":0,"write":0}}}}}}]}}"#);
            answer("200 OK", &step)
        } else if request.starts_with("GET /api/model ") {
            answer("200 OK", r#"{"data":[{"id":"m","providerID":"p","limit":{"context":100000,"output":8192}}]}"#)
        } else if request.starts_with("GET /api/session/ses_1/inbox ") {
            answer("200 OK", r#"{"data":[{"id":"msg_parked","delivery":"queue"}]}"#)
        } else if request.starts_with("PATCH /api/session/ses_1/inbox/") {
            requests.lock().unwrap().push(request.to_string());
            answer("200 OK", "{}")
        } else if request.starts_with("POST /api/session/ses_1/prompt ") || request.starts_with("POST /api/session/ses_1/interrupt ") {
            requests.lock().unwrap().push(request.to_string());
            answer("200 OK", "{}")
        } else {
            answer("404 Not Found", "")
        }
    });
}
