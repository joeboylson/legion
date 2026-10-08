//! Claude sessions in terminals legion2d owns, each with the add-on loaded.
//! Terminals and processes are state and I/O, so this file isn't functional.

use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

use legion2_proto::{Activity, SessionInfo, ENV_DEPLOYMENT, ENV_POSITION, LEGION};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

use crate::{
    constants::{
        ADDON_PERMISSION_MODE_ENV, ADDON_SESSION_ENV, ADDON_SOCKET_ENV, AUTOCOMPACT_PERCENT_ENV,
        CHILD_SESSION_MARKERS, TERMINAL_COLUMNS, TERMINAL_READ_BUFFER_BYTES, TERMINAL_ROWS,
        TERMINAL_TYPE,
    },
    context_handover::{autocompact_percent, HandoverPhase},
    daemon::Daemon,
    ids::new_id,
    setup::PermissionMode,
    store::RunningSession,
    terminal_key::TerminalKey,
};

pub struct Session {
    pub deployment: String,
    pub position: String,
    pub mission: Option<u32>,
    /// The part of its mission it works on, if the mission is split.
    pub part: Option<u32>,
    pub activity: Activity,
    pub can_see_state: bool,
    pub detail: Option<String>,
    pub model: Option<String>,
    /// How full its conversation is, in percent, as its add-on last said.
    pub context_percent: Option<u8>,
    /// The percentage it hands over to a fresh conversation at; None for never.
    pub clear_at: Option<u8>,
    pub handover: HandoverPhase,
    /// When its current turn began, in milliseconds since 1970.
    pub turn_started_ms: Option<i64>,
    /// When the commander was last asked to look in on it.
    pub last_check_in_ms: Option<i64>,
    /// Since when it has waited on a permission question.
    pub permission_asked_at: Option<Instant>,
    /// Set when Legion ends it on purpose, so its end isn't treated as a crash.
    pub is_stopping: bool,
    pub started_at: Instant,
    /// Whether the human has been told it seems stuck before starting.
    pub is_reported_stuck: bool,
    /// Messages handed to it run as its next turn, which may be after the
    /// one it's in now: whether that next turn only answers heads-ups.
    pub next_turn_answers_announcements: Option<bool>,
    /// Its current turn only answers heads-ups, so what it writes needn't reach the commander.
    pub is_answering_announcements: bool,
    pub screen: vt100::Parser,
    input: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    // Dropping the terminal's master side would close the session's terminal.
    _terminal: Box<dyn MasterPty + Send>,
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            deployment: self.deployment.clone(),
            position: self.position.clone(),
            mission: self.mission,
            activity: self.activity,
            can_see_state: self.can_see_state,
            detail: self.detail.clone(),
            is_stuck_starting: self.is_reported_stuck && self.activity == Activity::Starting,
            model: self.model.clone(),
            context_percent: self.context_percent,
            part: self.part,
        }
    }

    pub fn press_key(&mut self, key: TerminalKey) -> Result<(), String> {
        self.type_bytes(key.bytes())
    }

    pub fn type_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.input
            .write_all(bytes)
            .and_then(|_| self.input.flush())
            .map_err(|error| format!("can't type into the session: {error}"))
    }

    pub fn end(&mut self) -> Result<(), String> {
        self.killer.kill().map_err(|error| format!("can't end the session: {error}"))
    }

    pub fn screen_text(&self) -> String {
        self.screen.screen().contents().trim_end().to_string()
    }

    /// Redraws the screen in a terminal view: clear, contents with colors,
    /// then the cursor where the session has it.
    pub fn screen_ansi(&self) -> String {
        let screen = self.screen.screen();
        let (cursor_row, cursor_column) = screen.cursor_position();
        let contents = String::from_utf8_lossy(&screen.contents_formatted()).into_owned();
        format!("{contents}\x1b[{};{}H", cursor_row + 1, cursor_column + 1)
    }
}

pub struct SpawnRequest {
    pub deployment: String,
    pub position: String,
    pub mission: Option<u32>,
    pub part: Option<u32>,
    pub working_folder: PathBuf,
    pub arguments: Vec<String>,
    pub clear_at: Option<u8>,
    pub permission_mode: Option<PermissionMode>,
}

fn claude_command(daemon: &Daemon, request: &SpawnRequest, session_id: &str) -> CommandBuilder {
    let config = &daemon.config;
    let mut command = CommandBuilder::new(&config.claude_command);
    command.arg("--plugin-dir");
    command.arg(&config.addon_folder);
    command.args(&request.arguments);
    command.cwd(&request.working_folder);
    command.env("TERM", TERMINAL_TYPE);
    CHILD_SESSION_MARKERS.iter().for_each(|marker| command.env_remove(marker));
    command.env(ADDON_SOCKET_ENV, &config.socket_path);
    command.env(ADDON_SESSION_ENV, session_id);
    if let Some(mode) = request.permission_mode {
        command.env(ADDON_PERMISSION_MODE_ENV, mode.flag_value());
    }
    command.env(ENV_DEPLOYMENT, &request.deployment);
    command.env(ENV_POSITION, &request.position);
    if let Some(clear_at) = request.clear_at {
        command.env(AUTOCOMPACT_PERCENT_ENV, autocompact_percent(clear_at).to_string());
    }
    // So the session finds the legion2 command next to legion2d.
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    command.env("PATH", format!("{}:{inherited_path}", config.binary_folder.display()));
    command
}

/// Starts a session. Its terminal is read, and its end noticed, on threads
/// of their own. The caller must not hold the daemon's lock.
pub fn spawn_session(daemon: &Arc<Daemon>, request: SpawnRequest) -> Result<SessionInfo, String> {
    let terminal_size = PtySize { rows: TERMINAL_ROWS, cols: TERMINAL_COLUMNS, pixel_width: 0, pixel_height: 0 };
    let terminal = native_pty_system().openpty(terminal_size).map_err(|error| format!("can't open a terminal: {error}"))?;
    let session_id = new_id();
    let command = claude_command(daemon, &request, &session_id);
    let mut child = terminal
        .slave
        .spawn_command(command)
        .map_err(|error| format!("can't start {}: {error}", daemon.config.claude_command))?;
    drop(terminal.slave);
    let killer = child.clone_killer();
    let mut output = terminal.master.try_clone_reader().map_err(|error| format!("can't read the terminal: {error}"))?;
    let input = terminal.master.take_writer().map_err(|error| format!("can't write to the terminal: {error}"))?;

    let session = Session {
        deployment: request.deployment,
        position: request.position,
        mission: request.mission,
        part: request.part,
        activity: Activity::Starting,
        can_see_state: true,
        detail: None,
        model: None,
        context_percent: None,
        clear_at: request.clear_at,
        handover: HandoverPhase::Working,
        turn_started_ms: None,
        last_check_in_ms: None,
        permission_asked_at: None,
        is_stopping: false,
        started_at: Instant::now(),
        is_reported_stuck: false,
        next_turn_answers_announcements: None,
        is_answering_announcements: false,
        screen: vt100::Parser::new(TERMINAL_ROWS, TERMINAL_COLUMNS, 0),
        input,
        killer,
        _terminal: terminal.master,
    };
    let info = session.info();
    let running = RunningSession { deployment: info.deployment.clone(), position: info.position.clone(), mission: info.mission, part: info.part, session_id: session_id.clone() };
    {
        let mut state = daemon.state.lock().unwrap();
        state.sessions.insert(session_id.clone(), session);
        // The session runs either way; a failure here only means it won't come back after a restart.
        let recorded = state.find_deployment(&running.deployment).and_then(|(folder_index, _)| state.folders[folder_index].store.record_running_session(&running));
        if let Err(error) = recorded {
            eprintln!("{LEGION}d: {error}");
        }
    }
    daemon.announce_session(&info);

    // Keep reading the terminal, or claude blocks once its buffer fills.
    let reading_daemon = daemon.clone();
    let reading_session_id = session_id.clone();
    std::thread::spawn(move || {
        let mut buffer = [0u8; TERMINAL_READ_BUFFER_BYTES];
        while let Ok(read_count) = output.read(&mut buffer) {
            if read_count == 0 {
                break;
            }
            if let Some(session) = reading_daemon.state.lock().unwrap().sessions.get_mut(&reading_session_id) {
                session.screen.process(&buffer[..read_count]);
            }
        }
    });

    let waiting_daemon = daemon.clone();
    std::thread::spawn(move || {
        let how_it_ended = match child.wait() {
            Ok(status) => format!("exit status {}", status.exit_code()),
            Err(error) => error.to_string(),
        };
        let ended_session = {
            let mut state = waiting_daemon.state.lock().unwrap();
            if !waiting_daemon.is_shutting_down.load(Ordering::SeqCst) {
                // By session ID: a session Legion already took out of its state (to free a copy) has none left to say which folder.
                let forgotten = state.folders.iter().try_for_each(|folder| folder.store.forget_running_session(&session_id));
                if let Err(error) = forgotten {
                    eprintln!("{LEGION}d: {error}");
                }
            }
            state.sessions.remove(&session_id)
        };
        if let Some(session) = ended_session {
            waiting_daemon.handle_session_end(&session, &how_it_ended);
        }
    });
    Ok(info)
}
