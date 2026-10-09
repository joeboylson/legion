//! legion2d: Legion's background program, one per machine. It runs every
//! Legion folder's deployments and their Claude sessions, in terminals it owns, and
//! keeps each folder's deployment log.
//!
//!     legion2d [--addon <folder>] [--claude <command>] [--app <folder>]

mod access;
mod callouts;
mod channel_delivery;
mod channel_gate;
mod channel_holds;
mod channel_log;
mod channel_routes;
mod channel_settings;
mod channel_wire;
mod channels;
mod activity_entries;
mod check_ins;
mod claude_version;
mod constants;
mod context_handover;
mod daemon;
mod entry_filter;
mod entry_routing;
mod folder_commands;
mod folder_details;
mod folder_state;
mod git;
mod ids;
mod log_commands;
mod machine_settings;
mod mission_commands;
mod mission_parts;
mod mission_status;
mod naming;
mod permission_answers;
mod prompts;
mod deployment_commands;
mod server;
mod session_arguments;
mod session_commands;
mod session_lifecycle;
mod session_restore;
mod strategist_checks;
mod sessions;
mod setup;
mod setup_commands;
mod setup_watch;
mod shared_tools;
mod stalled_work;
mod state_lookup;
mod store;
mod terminal_key;
mod team_changes;
mod tool_sharing;
mod unreported_turns;
mod web_server;

use std::{path::PathBuf, sync::atomic::Ordering};

use legion2_proto::NAME;
use tokio::{
    net::UnixListener,
    signal::unix::{signal, SignalKind},
};

use crate::{
    claude_version::{check_claude_supported, version_label},
    constants::{SETUP_WATCH_INTERVAL, WAITING_SESSIONS_CHECK_INTERVAL},
    daemon::{Config, Daemon},
    machine_settings::read_machine_settings,
    server::{claim_socket, router},
    web_server::{web_address, web_router},
};

#[derive(Debug)]
struct Arguments {
    addon_folder: PathBuf,
    claude_command: String,
    /// The built app (app/dist), served to a browser.
    app_folder: PathBuf,
}

fn usage() -> String {
    format!("usage: {NAME}d [--addon <folder>] [--claude <command>] [--app <folder>]")
}

fn parse_arguments(arguments: &[String]) -> Result<Arguments, String> {
    let defaults = Arguments {
        addon_folder: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../addon")),
        claude_command: "claude".into(),
        app_folder: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../app/dist")),
    };
    let parsed = arguments.chunks(2).try_fold(defaults, |parsed, pair| match pair {
        [flag, value] if flag == "--addon" => Ok(Arguments { addon_folder: PathBuf::from(value), ..parsed }),
        [flag, value] if flag == "--claude" => Ok(Arguments { claude_command: value.clone(), ..parsed }),
        [flag, value] if flag == "--app" => Ok(Arguments { app_folder: PathBuf::from(value), ..parsed }),
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
    // Read now to refuse a broken file at the start; it's read again each time it's needed.
    let web_port = read_machine_settings(&data_folder)?.web_port;
    let app_folder = arguments.app_folder;

    let daemon = Daemon::new(Config { data_folder, socket_path: socket_path.clone(), addon_folder: arguments.addon_folder, claude_command: arguments.claude_command, binary_folder });
    let folder_count = daemon.state.lock().unwrap().folders.len();
    println!("{NAME}d: listening on {} (Claude Code {}, {folder_count} folders)", socket_path.display(), version_label(claude_version));

    // A broken channels.json shouldn't stop legion2d: say so and run without channels.
    daemon.connect_channels();
    if let Err(error) = daemon.channels.restore() {
        eprintln!("{NAME}d: channels not restored: {error}");
    }

    // Sessions reach legion2d over the socket, so they start once it listens.
    let restoring_daemon = daemon.clone();
    tokio::task::spawn_blocking(move || restoring_daemon.restore_open_deployments());
    let checking_daemon = daemon.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(WAITING_SESSIONS_CHECK_INTERVAL);
        loop {
            interval.tick().await;
            let ticking_daemon = checking_daemon.clone();
            let _ = tokio::task::spawn_blocking(move || {
                ticking_daemon.refuse_unanswered_permissions();
                ticking_daemon.report_sessions_stuck_starting();
                ticking_daemon.ask_commander_to_check_in();
                ticking_daemon.point_out_stalled_work();
                ticking_daemon.restart_pending_sessions();
                ticking_daemon.tell_commanders_about_team_changes();
                ticking_daemon.keep_strategists();
            })
            .await;
        }
    });

    let watching_daemon = daemon.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SETUP_WATCH_INTERVAL);
        let mut last_look = std::collections::HashMap::new();
        loop {
            interval.tick().await;
            let looking_daemon = watching_daemon.clone();
            let previous = std::mem::take(&mut last_look);
            last_look = tokio::task::spawn_blocking(move || looking_daemon.report_setup_changes(&previous)).await.unwrap_or_default();
        }
    });

    // Ctrl-C by hand, or SIGTERM from launchd stopping the service.
    let mut terminate = signal(SignalKind::terminate()).map_err(|error| format!("can't listen for SIGTERM: {error}"))?;
    let (stopping, stopped) = tokio::sync::watch::channel(false);
    let stopping_daemon = daemon.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
        stopping_daemon.is_shutting_down.store(true, Ordering::SeqCst);
        println!("{NAME}d: stopping");
        let _ = stopping.send(true);
    });
    let stop_signal = |mut receiver: tokio::sync::watch::Receiver<bool>| async move {
        let _ = receiver.wait_for(|is_stopping| *is_stopping).await;
    };

    // The browser's way in is a convenience: without it legion2d still runs.
    let web_listener = match web_port {
        0 => None,
        port => match tokio::net::TcpListener::bind(web_address(port)).await {
            Ok(web_listener) => {
                println!("{NAME}d: the app is at http://{}", web_address(port));
                Some((web_listener, port))
            }
            Err(error) => {
                eprintln!("{NAME}d: can't serve the app on {}: {error}", web_address(port));
                None
            }
        },
    };
    let web_daemon = daemon.clone();
    let web_stopped = stopped.clone();
    let web_served = async move {
        let Some((web_listener, port)) = web_listener else { return Ok(()) };
        axum::serve(web_listener, web_router(web_daemon, &app_folder, port)).with_graceful_shutdown(stop_signal(web_stopped)).await
    };
    let socket_served = axum::serve(listener, router(daemon)).with_graceful_shutdown(stop_signal(stopped));
    let (served, web_result) = tokio::join!(socket_served, web_served);
    if let Err(error) = web_result {
        eprintln!("{NAME}d: the app's web address failed: {error}");
    }
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
