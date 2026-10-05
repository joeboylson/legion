//! Claude sessions in terminals legion2d owns, each with the add-on loaded.
//! Terminals and processes are state and I/O, so this file isn't functional.

use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use legion2_proto::{Activity, SessionInfo, ENV_POSITION, ENV_RUN};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

use crate::{
    constants::{
        ADDON_SESSION_ENV, ADDON_SOCKET_ENV, CHILD_SESSION_MARKERS, TERMINAL_COLUMNS, TERMINAL_READ_BUFFER_BYTES, TERMINAL_ROWS,
        TERMINAL_TYPE,
    },
    daemon::Daemon,
    ids::new_id,
    terminal_key::TerminalKey,
};

pub struct Session {
    pub run: String,
    pub position: String,
    pub mission: Option<u32>,
    pub activity: Activity,
    pub can_see_state: bool,
    pub detail: Option<String>,
    /// Since when it has waited on a permission question.
    pub permission_asked_at: Option<Instant>,
    /// Set when Legion ends it on purpose, so its end isn't treated as a crash.
    pub is_stopping: bool,
    pub started_at: Instant,
    /// Whether the human has been told it seems stuck before starting.
    pub is_reported_stuck: bool,
    pub screen: vt100::Parser,
    input: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    // Dropping the terminal's master side would close the session's terminal.
    _terminal: Box<dyn MasterPty + Send>,
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            run: self.run.clone(),
            position: self.position.clone(),
            mission: self.mission,
            activity: self.activity,
            can_see_state: self.can_see_state,
            detail: self.detail.clone(),
        }
    }

    pub fn press_key(&mut self, key: TerminalKey) -> Result<(), String> {
        self.input
            .write_all(key.bytes())
            .and_then(|_| self.input.flush())
            .map_err(|error| format!("can't type into the session: {error}"))
    }

    pub fn end(&mut self) -> Result<(), String> {
        self.killer.kill().map_err(|error| format!("can't end the session: {error}"))
    }

    pub fn screen_text(&self) -> String {
        self.screen.screen().contents().trim_end().to_string()
    }
}

pub struct SpawnRequest {
    pub run: String,
    pub position: String,
    pub mission: Option<u32>,
    pub working_folder: PathBuf,
    pub arguments: Vec<String>,
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
    command.env(ENV_RUN, &request.run);
    command.env(ENV_POSITION, &request.position);
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
        run: request.run,
        position: request.position,
        mission: request.mission,
        activity: Activity::Starting,
        can_see_state: true,
        detail: None,
        permission_asked_at: None,
        is_stopping: false,
        started_at: Instant::now(),
        is_reported_stuck: false,
        screen: vt100::Parser::new(TERMINAL_ROWS, TERMINAL_COLUMNS, 0),
        input,
        killer,
        _terminal: terminal.master,
    };
    let info = session.info();
    daemon.state.lock().unwrap().sessions.insert(session_id.clone(), session);
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
        let ended_session = waiting_daemon.state.lock().unwrap().sessions.remove(&session_id);
        if let Some(session) = ended_session {
            waiting_daemon.handle_session_end(&session, &how_it_ended);
        }
    });
    Ok(info)
}
