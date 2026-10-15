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
        atomic::{AtomicBool, Ordering},
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

/// Plays OpenCode's server: lists one session, and takes prompts unless told to fail them.
pub fn serve_opencode(port: u16, tag: String, is_failing: Arc<AtomicBool>, prompts: Arc<Mutex<Vec<String>>>) {
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
