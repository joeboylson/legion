//! Sessions in terminals legiond owns: Claude Code with the add-on loaded,
//! or a harness template's command.
//! Terminals and processes are state and I/O, so this file isn't functional.

use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

use legion_proto::{Activity, SessionInfo, ENV_DEPLOYMENT, ENV_POSITION, ENV_SOCKET, LEGION};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

use crate::{
    constants::{
        ADDON_PERMISSION_MODE_ENV, ADDON_SESSION_ENV, ADDON_SOCKET_ENV, AUTOCOMPACT_PERCENT_ENV,
        CHILD_SESSION_MARKERS, ENDED_SCREEN_DRAIN_LIMIT, ENDED_STDERR_LINES, HINDSIGHT_NO_CAPTURE, STDERR_FILE_EXTENSION, STDERR_FOLDER_NAME, TERMINAL_COLUMNS, TERMINAL_HISTORY_LINES, TERMINAL_READ_BUFFER_BYTES, TERMINAL_ROWS,
        TERMINAL_TYPE,
    },
    context_handover::{autocompact_percent, HandoverPhase},
    daemon::Daemon,
    harness::FilledCommand,
    mid_turn_handover::WrapUp,
    ids::new_id,
    opencode_server::OpenCodeServer,
    opencode_state::Progress,
    session_programs::SessionProgram,
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
    /// Started from a harness template: no add-on reports what it does, so
    /// Legion can't tell when it's busy or stuck, unless it has a server to ask.
    pub reports_itself: bool,
    /// The server its state is read from, and what has been seen of it.
    pub server: Option<OpenCodeServer>,
    pub server_progress: Progress,
    /// The ID of its conversation on that server, once the server has one.
    pub opencode_session: Option<String>,
    pub detail: Option<String>,
    pub model: Option<String>,
    /// How full its conversation is, in percent, as its add-on last said.
    pub context_percent: Option<u8>,
    /// The percentage it hands over to a fresh conversation at; None for never.
    pub clear_at: Option<u8>,
    pub handover: HandoverPhase,
    /// Set when it was asked to wrap up in the middle of a turn.
    pub wrap_up: Option<WrapUp>,
    /// Messages pushed to it since its turn began: they wait behind that turn, so if it is
    /// ended for a fresh start they are among the ones the fresh session must get again.
    pub parked_entries: Vec<i64>,
    /// When its current turn began, in milliseconds since 1970.
    pub turn_started_ms: Option<i64>,
    /// When the commander was last asked to look in on it.
    pub last_check_in_ms: Option<i64>,
    /// Since when it has waited on a permission question.
    pub permission_asked_at: Option<Instant>,
    /// Set when Legion ends it on purpose, so its end isn't treated as a crash.
    pub stopped_because: Option<String>,
    pub started_at: Instant,
    /// Whether the admin has been told it seems stuck before starting.
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

    pub fn is_stopping(&self) -> bool {
        self.stopped_because.is_some()
    }

    /// Ends it on purpose, for the reason given.
    pub fn stop(&mut self, reason: &str) -> Result<(), String> {
        self.stopped_because = Some(reason.to_string());
        self.killer.kill().map_err(|error| format!("can't end the session: {error}"))
    }

    pub fn screen_text(&self) -> String {
        self.screen.screen().contents().trim_end().to_string()
    }

    pub fn screen_history(&mut self) -> String {
        history_of(&mut self.screen)
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

/// The lines that have scrolled off the top of a screen, oldest first, as
/// plain text. vt100 shows them by scrolling its view back, so each is read
/// at its own offset, then the view goes back to the bottom.
pub fn history_of(screen: &mut vt100::Parser) -> String {
    screen.set_scrollback(usize::MAX);
    let kept = screen.screen().scrollback();
    let columns = screen.screen().size().1;
    let lines: Vec<String> = (1..=kept)
        .rev()
        .map(|offset| {
            screen.set_scrollback(offset);
            screen.screen().rows(0, columns).next().unwrap_or_default().trim_end().to_string()
        })
        .collect();
    screen.set_scrollback(0);
    lines.join("\n")
}

pub struct SpawnRequest {
    pub deployment: String,
    pub position: String,
    pub mission: Option<u32>,
    pub part: Option<u32>,
    pub working_folder: PathBuf,
    pub program: SessionProgram,
    /// The model to show before the session says which it runs.
    pub model: Option<String>,
    pub clear_at: Option<u8>,
    pub permission_mode: Option<PermissionMode>,
}

/// The program's name, as a failed start names it.
fn program_name<'a>(daemon: &'a Daemon, program: &'a SessionProgram) -> &'a str {
    match program {
        SessionProgram::Claude(_) => &daemon.config.claude_command,
        SessionProgram::Template(filled, _) => &filled.command,
    }
}

/// Claude Code with the add-on, and the add-on's settings.
fn claude_command(daemon: &Daemon, request: &SpawnRequest, arguments: &[String], session_id: &str) -> CommandBuilder {
    let config = &daemon.config;
    let mut command = CommandBuilder::new(&config.claude_command);
    command.arg("--plugin-dir");
    command.arg(&config.addon_folder);
    command.args(arguments);
    command.env(ADDON_SOCKET_ENV, &config.socket_path);
    command.env(ADDON_SESSION_ENV, session_id);
    if let Some(mode) = request.permission_mode {
        command.env(ADDON_PERMISSION_MODE_ENV, mode.flag_value());
    }
    if let Some(clear_at) = request.clear_at {
        command.env(AUTOCOMPACT_PERCENT_ENV, autocompact_percent(clear_at).to_string());
    }
    command
}

/// Where a template session's stderr goes: its terminal shows only the screen, which holds a status bar by the end.
fn stderr_path(daemon: &Daemon, session_id: &str) -> PathBuf {
    daemon.config.data_folder.join(STDERR_FOLDER_NAME).join(format!("{session_id}.{STDERR_FILE_EXTENSION}"))
}

/// A template's command, with the three settings `legion` reads, run so its stderr lands in a file.
fn template_command(daemon: &Daemon, filled: &FilledCommand, session_id: &str) -> CommandBuilder {
    let mut command = CommandBuilder::new("sh");
    command.args(["-c", r#"exec 2>>"$1"; shift; exec "$@""#, "sh"]);
    command.arg(stderr_path(daemon, session_id));
    command.arg(&filled.command);
    command.args(&filled.args);
    command.env(ENV_SOCKET, &daemon.config.socket_path);
    filled.env.iter().for_each(|(name, value)| command.env(name, value));
    command
}

/// The command for a session, with what every session gets.
fn session_command(daemon: &Daemon, request: &SpawnRequest, session_id: &str) -> CommandBuilder {
    let config = &daemon.config;
    let mut command = match &request.program {
        SessionProgram::Claude(arguments) => claude_command(daemon, request, arguments, session_id),
        SessionProgram::Template(filled, _) => template_command(daemon, filled, session_id),
    };
    command.cwd(&request.working_folder);
    command.env("TERM", TERMINAL_TYPE);
    CHILD_SESSION_MARKERS.iter().for_each(|marker| command.env_remove(marker));
    command.env(HINDSIGHT_NO_CAPTURE.0, HINDSIGHT_NO_CAPTURE.1);
    command.env(ENV_DEPLOYMENT, &request.deployment);
    command.env(ENV_POSITION, &request.position);
    // So the session finds the legion command next to legiond.
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    command.env("PATH", format!("{}:{inherited_path}", config.binary_folder.display()));
    command
}

/// A session with no add-on says why it ended only on its screen: its last line.
fn ended_with_last_line(how_it_ended: String, screen: &str) -> String {
    match screen.lines().rev().map(str::trim).find(|line| !line.is_empty()) {
        Some(line) => format!("{how_it_ended}: {line}"),
        None => how_it_ended,
    }
}

/// Why a session that ended on its own ended, for the log. A Claude session
/// says only its exit status. A template that ended on its own
/// with an error says what it wrote to stderr, else the last line on its screen.
fn ended_reason(how_it_ended: String, exited_cleanly: bool, reports_itself: bool, stderr: &str, screen: &str) -> String {
    if !reports_itself || exited_cleanly {
        return how_it_ended;
    }
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    match lines.len() {
        0 => ended_with_last_line(how_it_ended, screen),
        count => format!("{how_it_ended}: {}", lines[count.saturating_sub(ENDED_STDERR_LINES)..].join(" / ")),
    }
}

/// Starts a session. Its terminal is read, and its end noticed, on threads
/// of their own. The caller must not hold the daemon's lock.
pub fn spawn_session(daemon: &Arc<Daemon>, request: SpawnRequest) -> Result<SessionInfo, String> {
    let terminal_size = PtySize { rows: TERMINAL_ROWS, cols: TERMINAL_COLUMNS, pixel_width: 0, pixel_height: 0 };
    let terminal = native_pty_system().openpty(terminal_size).map_err(|error| format!("can't open a terminal: {error}"))?;
    let session_id = new_id();
    if request.program.reports_itself() {
        let folder = stderr_path(daemon, &session_id).with_file_name("");
        std::fs::create_dir_all(&folder).map_err(|error| format!("can't create {}: {error}", folder.display()))?;
    }
    let command = session_command(daemon, &request, &session_id);
    let mut child = terminal
        .slave
        .spawn_command(command)
        .map_err(|error| format!("can't start {}: {error}", program_name(daemon, &request.program)))?;
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
        can_see_state: !request.program.reports_itself() || request.program.server().is_some(),
        reports_itself: request.program.reports_itself(),
        server: request.program.server().cloned(),
        server_progress: Progress::default(),
        opencode_session: None,
        detail: None,
        model: request.model,
        context_percent: None,
        clear_at: request.clear_at,
        handover: HandoverPhase::Working,
        wrap_up: None,
        parked_entries: Vec::new(),
        turn_started_ms: None,
        last_check_in_ms: None,
        permission_asked_at: None,
        stopped_because: None,
        started_at: Instant::now(),
        is_reported_stuck: false,
        next_turn_answers_announcements: None,
        is_answering_announcements: false,
        screen: vt100::Parser::new(TERMINAL_ROWS, TERMINAL_COLUMNS, TERMINAL_HISTORY_LINES),
        input,
        killer,
        _terminal: terminal.master,
    };
    let info = session.info();
    let reports_itself = session.reports_itself;
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
    let (screen_drained, screen_drained_signal) = std::sync::mpsc::channel::<()>();
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
        let _ = screen_drained.send(());
    });

    let waiting_daemon = daemon.clone();
    let stderr_file = stderr_path(daemon, &session_id);
    std::thread::spawn(move || {
        let (how_it_ended, exited_cleanly) = match child.wait() {
            Ok(status) => (format!("exit status {}", status.exit_code()), status.success()),
            Err(error) => (error.to_string(), false),
        };
        if reports_itself {
            let _ = screen_drained_signal.recv_timeout(ENDED_SCREEN_DRAIN_LIMIT);
        }
        let stderr = std::fs::read_to_string(&stderr_file).unwrap_or_default();
        let _ = std::fs::remove_file(&stderr_file);
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
            let how_it_ended = ended_reason(how_it_ended, exited_cleanly, session.reports_itself, &stderr, &session.screen_text());
            waiting_daemon.handle_session_end(&session, &how_it_ended);
        }
    });
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ended_session_without_an_add_on_gives_its_last_line() {
        let screen = "Starting\nLegion's tools didn't connect to OpenCode in 60 seconds: refused\n  \n";
        assert_eq!(ended_with_last_line("exit status 1".into(), screen), "exit status 1: Legion's tools didn't connect to OpenCode in 60 seconds: refused");
        assert_eq!(ended_with_last_line("exit status 0".into(), " \n"), "exit status 0");
    }

    const STATUS_BAR: &str = "esc interrupt · legion · qwen ctrl+p menu";

    #[test]
    fn a_crashed_template_gives_its_stderr_over_its_screen() {
        let stderr = "\nopencode isn't on PATH\n  \n";
        assert_eq!(ended_reason("exit status 1".into(), false, true, stderr, STATUS_BAR), "exit status 1: opencode isn't on PATH");
        let long = "a\nb\nc\nd\n";
        assert_eq!(ended_reason("exit status 1".into(), false, true, long, STATUS_BAR), "exit status 1: b / c / d");
    }

    #[test]
    fn a_crashed_template_without_stderr_gives_its_screen_line() {
        assert_eq!(ended_reason("exit status 1".into(), false, true, " \n", "Starting\nfailed to start\n"), "exit status 1: failed to start");
    }

    #[test]
    fn claude_and_a_clean_exit_are_unchanged() {
        assert_eq!(ended_reason("exit status 1".into(), false, false, "x", "y"), "exit status 1");
        assert_eq!(ended_reason("exit status 0".into(), true, true, "x", STATUS_BAR), "exit status 0");
    }

    #[test]
    fn what_scrolls_off_the_top_is_kept_in_order() {
        let mut screen = vt100::Parser::new(3, 20, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
        assert_eq!(history_of(&mut screen), "one\ntwo");
        assert!(screen.screen().contents().starts_with("three"), "the view is back at the bottom");
    }
}
