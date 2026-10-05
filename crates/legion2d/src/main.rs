//! legion2d: Legion's background program.
//!
//! For now it runs one Claude session in a terminal it owns, with Legion's
//! add-on loaded, and prints what the session is doing as the add-on
//! reports it. A line typed on stdin goes to the session as a message; lines
//! starting with / are commands (see `help`).
//!
//!     legion2d --folder <path> [--addon <dir>] [--claude <bin>] [-- <claude args>...]

use std::{
    io::{Read, Write},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Path as UrlPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use tokio::{io::AsyncBufReadExt, net::UnixListener};

/// Every name the new version uses comes from this, so it can run beside
/// today's Legion and switch to "legion" in one change.
const NAME: &str = "legion2";
/// The first Claude Code with add-ons.
const MIN_CLAUDE: (u32, u32, u32) = (2, 1, 287);
/// Add-on versions this program understands (ADDON_VERSION in addon/hooks/register.ts).
const KNOWN_ADDON_VERSIONS: &[&str] = &["1"];
const ROWS: u16 = 40;
const COLS: u16 = 120;

#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Activity {
    Starting,
    Idle,
    Busy,
    Permission,
    Ended,
}

impl Activity {
    fn label(self) -> &'static str {
        match self {
            Activity::Starting => "starting",
            Activity::Idle => "idle",
            Activity::Busy => "busy",
            Activity::Permission => "waiting on a permission question",
            Activity::Ended => "ended",
        }
    }
}

/// What the add-on posts. `addon` and `claude` come once, when it connects.
#[derive(Deserialize)]
struct Report {
    state: Activity,
    detail: Option<String>,
    addon: Option<String>,
    claude: Option<String>,
}

struct Session {
    id: String,
    activity: Activity,
    /// False once the add-on reports a version this program doesn't know.
    can_see_state: bool,
    inbox: Vec<String>,
    screen: vt100::Parser,
    input: Box<dyn Write + Send>,
    // Dropping the terminal's master side would close the session's terminal.
    _master: Box<dyn MasterPty + Send>,
}

type Shared = Arc<Mutex<Session>>;

struct Args {
    folder: PathBuf,
    addon: PathBuf,
    claude: String,
    extra: Vec<String>,
}

impl Args {
    fn parse() -> Result<Args, String> {
        let mut folder = None;
        let mut addon = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../addon"));
        let mut claude = "claude".to_string();
        let mut extra = Vec::new();
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            let mut value = || it.next().ok_or(format!("{arg} needs a value"));
            match arg.as_str() {
                "--folder" => folder = Some(PathBuf::from(value()?)),
                "--addon" => addon = PathBuf::from(value()?),
                "--claude" => claude = value()?,
                "--" => {
                    extra = it.collect();
                    break;
                }
                _ => return Err(format!("unknown argument {arg}\n{}", usage())),
            }
        }
        let folder = folder.ok_or_else(usage)?;
        let folder = folder.canonicalize().map_err(|e| format!("{}: {e}", folder.display()))?;
        let addon = addon.canonicalize().map_err(|e| format!("add-on {}: {e}", addon.display()))?;
        Ok(Args { folder, addon, claude, extra })
    }
}

fn usage() -> String {
    format!("usage: {NAME}d --folder <path> [--addon <dir>] [--claude <bin>] [-- <claude args>...]")
}

fn help() {
    println!(
        "Type a line to send it to the session as a message. Commands:\n  \
         /state        what the session is doing\n  \
         /screen       print the session's screen\n  \
         /keys <text>  type text straight into the session's terminal\n  \
         /key <name>   press a key in the session's terminal: enter, esc, up, down, tab\n  \
         /quit         end the session\n  \
         /help         this list"
    );
}

fn say(msg: &str) {
    println!("{NAME}d: {msg}");
}

/// Refuses a Claude Code older than the first one with add-ons.
fn check_claude(bin: &str) -> Result<(), String> {
    let out = Command::new(bin)
        .arg("--version")
        .output()
        .map_err(|e| format!("can't run {bin}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let found = text.split_whitespace().next().unwrap_or("");
    let parts: Vec<u32> = found.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return Err(format!("can't read the Claude Code version from {bin} --version: {text:?}"));
    }
    let (min_a, min_b, min_c) = MIN_CLAUDE;
    if (parts[0], parts[1], parts[2]) < MIN_CLAUDE {
        return Err(format!(
            "Claude Code {found} is too old: Legion needs {min_a}.{min_b}.{min_c} or later for its add-on"
        ));
    }
    Ok(())
}

fn data_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME isn't set".to_string())?;
    let dir = PathBuf::from(home).join(".local/share").join(NAME);
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Clears a socket file left by a program that died, but refuses if another
/// one is still listening on it.
fn claim_socket(path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        return Err(format!("another {NAME}d is running ({})", path.display()));
    }
    std::fs::remove_file(path).map_err(|e| format!("can't remove {}: {e}", path.display()))
}

fn new_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", nanos, std::process::id())
}

async fn report_state(
    State(session): State<Shared>,
    UrlPath(id): UrlPath<String>,
    Json(report): Json<Report>,
) -> StatusCode {
    let mut s = session.lock().unwrap();
    if id != s.id {
        return StatusCode::NOT_FOUND;
    }
    if let Some(version) = &report.addon {
        s.can_see_state = KNOWN_ADDON_VERSIONS.contains(&version.as_str());
        say(&format!(
            "add-on {version} connected (Claude Code {})",
            report.claude.as_deref().unwrap_or("?")
        ));
        if !s.can_see_state {
            say("can't see this session's state: the add-on is a version this program doesn't know");
        }
    }
    if s.can_see_state && report.state != s.activity {
        s.activity = report.state;
        let detail = report.detail.map(|d| format!(" ({d})")).unwrap_or_default();
        say(&format!("{}{detail}", report.state.label()));
    }
    StatusCode::NO_CONTENT
}

async fn take_inbox(State(session): State<Shared>, UrlPath(id): UrlPath<String>) -> Response {
    let mut s = session.lock().unwrap();
    if id != s.id {
        return StatusCode::NOT_FOUND.into_response();
    }
    if s.inbox.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }
    let messages = std::mem::take(&mut s.inbox);
    Json(serde_json::json!({ "messages": messages })).into_response()
}

fn type_keys(session: &Shared, keys: &[u8]) {
    let mut s = session.lock().unwrap();
    if let Err(e) = s.input.write_all(keys).and_then(|_| s.input.flush()) {
        say(&format!("can't type into the session: {e}"));
    }
}

fn named_key(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "enter" => b"\r",
        "esc" => b"\x1b",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "tab" => b"\t",
        _ => return None,
    })
}

fn print_screen(session: &Shared) {
    let s = session.lock().unwrap();
    let contents = s.screen.screen().contents();
    let rule = "─".repeat(COLS as usize);
    println!("{rule}\n{}\n{rule}", contents.trim_end());
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{NAME}d: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args = Args::parse()?;
    check_claude(&args.claude)?;
    let socket = data_dir()?.join(format!("{NAME}d.sock"));
    claim_socket(&socket)?;
    let listener =
        UnixListener::bind(&socket).map_err(|e| format!("can't listen on {}: {e}", socket.display()))?;

    let pty = native_pty_system()
        .openpty(PtySize { rows: ROWS, cols: COLS, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("can't open a terminal: {e}"))?;
    let id = new_id();
    let mut cmd = CommandBuilder::new(&args.claude);
    cmd.arg("--plugin-dir");
    cmd.arg(&args.addon);
    cmd.args(&args.extra);
    cmd.cwd(&args.folder);
    cmd.env("TERM", "xterm-256color");
    // Started from inside a Claude session, these mark the new one as its
    // child, which turns off saving its transcript (and so resuming it).
    for marker in ["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_ENTRYPOINT"] {
        cmd.env_remove(marker);
    }
    cmd.env("LEGION_ADDON_SOCKET", &socket);
    cmd.env("LEGION_ADDON_SESSION", &id);
    let mut child = pty
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("can't start {}: {e}", args.claude))?;
    drop(pty.slave);
    let mut killer = child.clone_killer();
    let mut output = pty.master.try_clone_reader().map_err(|e| format!("can't read the terminal: {e}"))?;
    let input = pty.master.take_writer().map_err(|e| format!("can't write to the terminal: {e}"))?;

    let session: Shared = Arc::new(Mutex::new(Session {
        id: id.clone(),
        activity: Activity::Starting,
        can_see_state: true,
        inbox: Vec::new(),
        screen: vt100::Parser::new(ROWS, COLS, 0),
        input,
        _master: pty.master,
    }));
    say(&format!("session {id} starting in {}", args.folder.display()));

    // Keep reading the terminal, or claude blocks once its buffer fills.
    let reader_session = session.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match output.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => reader_session.lock().unwrap().screen.process(&buf[..n]),
            }
        }
    });

    // For now the program runs one session, so it ends with it.
    let socket_file = socket.clone();
    std::thread::spawn(move || {
        let how = match child.wait() {
            Ok(status) => format!("exit status {}", status.exit_code()),
            Err(e) => e.to_string(),
        };
        say(&format!("session ended ({how})"));
        let _ = std::fs::remove_file(&socket_file);
        std::process::exit(0);
    });

    let app = Router::new()
        .route("/sessions/{id}/state", post(report_state))
        .route("/sessions/{id}/inbox", get(take_inbox))
        .with_state(session.clone());
    tokio::spawn(async move { axum::serve(listener, app).await });

    help();
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim_end();
        match line {
            "" => {}
            "/help" => help(),
            "/state" => {
                let s = session.lock().unwrap();
                let seen = if s.can_see_state { "" } else { " (can't see this session's state)" };
                say(&format!("{}{seen}", s.activity.label()));
            }
            "/screen" => print_screen(&session),
            _ if line.starts_with("/key ") => match named_key(&line["/key ".len()..]) {
                Some(keys) => type_keys(&session, keys),
                None => say("keys are enter, esc, up, down and tab"),
            },
            "/quit" => {
                if let Err(e) = killer.kill() {
                    say(&format!("can't end the session: {e}"));
                }
            }
            _ if line.starts_with("/keys ") => type_keys(&session, line["/keys ".len()..].as_bytes()),
            _ if line.starts_with('/') => say(&format!("unknown command {line}; /help lists them")),
            _ => {
                session.lock().unwrap().inbox.push(line.to_string());
                say("message queued; the session gets it once it's free");
            }
        }
    }
    // stdin closed: keep the session running until it ends by itself.
    std::future::pending::<()>().await;
    Ok(())
}
