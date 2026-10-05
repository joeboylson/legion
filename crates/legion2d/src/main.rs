//! legion2d: Legion's background program, one per machine. It runs every
//! Legion folder's runs and their Claude sessions, in terminals it owns, and
//! keeps each folder's run log.
//!
//!     legion2d [--addon <folder>] [--claude <command>]

mod access;
mod activity_entries;
mod claude_version;
mod constants;
mod daemon;
mod entry_filter;
mod entry_routing;
mod folder_commands;
mod folder_state;
mod git;
mod ids;
mod log_commands;
mod machine_settings;
mod mission_commands;
mod mission_status;
mod naming;
mod prompts;
mod run_commands;
mod server;
mod session_arguments;
mod session_commands;
mod session_lifecycle;
mod sessions;
mod setup;
mod state_lookup;
mod store;
mod terminal_key;

use std::{path::PathBuf, time::Duration};

use legion2_proto::NAME;
use tokio::net::UnixListener;

use crate::{
    claude_version::{check_claude_supported, version_label},
    constants::WAITING_SESSIONS_CHECK_INTERVAL,
    daemon::{Config, Daemon},
    machine_settings::read_machine_settings,
    server::{claim_socket, router},
};

#[derive(Debug)]
struct Arguments {
    addon_folder: PathBuf,
    claude_command: String,
}

fn usage() -> String {
    format!("usage: {NAME}d [--addon <folder>] [--claude <command>]")
}

fn parse_arguments(arguments: &[String]) -> Result<Arguments, String> {
    let defaults = Arguments { addon_folder: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../addon")), claude_command: "claude".into() };
    let parsed = arguments.chunks(2).try_fold(defaults, |parsed, pair| match pair {
        [flag, value] if flag == "--addon" => Ok(Arguments { addon_folder: PathBuf::from(value), ..parsed }),
        [flag, value] if flag == "--claude" => Ok(Arguments { claude_command: value.clone(), ..parsed }),
        [flag] => Err(format!("{flag} needs a value\n{}", usage())),
        [flag, _] => Err(format!("unknown argument {flag}\n{}", usage())),
        _ => Ok(parsed),
    })?;
    let addon_folder = parsed.addon_folder.canonicalize().map_err(|error| format!("add-on {}: {error}", parsed.addon_folder.display()))?;
    Ok(Arguments { addon_folder, ..parsed })
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{NAME}d: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let command_line: Vec<String> = std::env::args().skip(1).collect();
    let arguments = parse_arguments(&command_line)?;
    let claude_version = check_claude_supported(&arguments.claude_command)?;
    let data_folder = legion2_proto::data_dir().ok_or("HOME isn't set")?;
    std::fs::create_dir_all(&data_folder).map_err(|error| format!("can't create {}: {error}", data_folder.display()))?;
    let socket_path = legion2_proto::socket_path().ok_or("HOME isn't set")?;
    claim_socket(&socket_path)?;
    let listener = UnixListener::bind(&socket_path).map_err(|error| format!("can't listen on {}: {error}", socket_path.display()))?;
    let binary_folder = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .ok_or("can't tell where legion2d lives")?;
    let settings = read_machine_settings(&data_folder)?;

    let daemon = Daemon::new(Config {
        data_folder,
        socket_path: socket_path.clone(),
        addon_folder: arguments.addon_folder,
        claude_command: arguments.claude_command,
        binary_folder,
        max_busy_sessions: settings.max_busy_sessions,
        permission_timeout: Duration::from_secs(settings.permission_timeout_minutes * 60),
    });
    let folder_count = daemon.state.lock().unwrap().folders.len();
    println!("{NAME}d: listening on {} (Claude Code {}, {folder_count} folders)", socket_path.display(), version_label(claude_version));

    // Sessions reach legion2d over the socket, so they start once it listens.
    let restoring_daemon = daemon.clone();
    tokio::task::spawn_blocking(move || restoring_daemon.restore_open_runs());
    let checking_daemon = daemon.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(WAITING_SESSIONS_CHECK_INTERVAL);
        loop {
            interval.tick().await;
            let ticking_daemon = checking_daemon.clone();
            let _ = tokio::task::spawn_blocking(move || {
                ticking_daemon.refuse_unanswered_permissions();
                ticking_daemon.report_sessions_stuck_starting();
            })
            .await;
        }
    });

    let stop_signal = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let served = axum::serve(listener, router(daemon)).with_graceful_shutdown(stop_signal).await;
    std::fs::remove_file(&socket_path).map_err(|error| format!("can't remove {}: {error}", socket_path.display()))?;
    served.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn flags_take_values() {
        let parsed = parse_arguments(&words(&["--claude", "/bin/claude"])).unwrap();
        assert_eq!(parsed.claude_command, "/bin/claude");
    }

    #[test]
    fn a_flag_without_a_value_is_refused() {
        assert!(parse_arguments(&words(&["--claude"])).unwrap_err().contains("needs a value"));
    }

    #[test]
    fn unknown_flags_are_refused() {
        assert!(parse_arguments(&words(&["--model", "x"])).unwrap_err().contains("unknown argument"));
    }
}
