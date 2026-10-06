//! The Legion app's back end: passes the screen's requests to legion2d, and
//! streams legion2d's events back to the screen. Both connections come back
//! by themselves when legion2d restarts.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::Duration;

use legion2_client::Client;
use legion2_proto::{Command, Event, Reply, ServerMessage};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

/// The screen listens for these.
const EVENT_CHANNEL: &str = "legion-event";
const CONNECTION_CHANNEL: &str = "legion-connection";
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// The connection requests go over; made on first use, dropped on failure.
struct RequestConnection(Mutex<Option<Client>>);

/// Debug builds only: where to save what the screen looks like, so it can
/// be checked without a person looking. Set LEGION2_SNAPSHOT_DIR to turn it on.
const SNAPSHOT_FOLDER_ENV: &str = "LEGION2_SNAPSHOT_DIR";
/// The built page the snapshot's stylesheets and fonts come from.
const BUILT_PAGE_FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../dist");

fn snapshot_folder() -> Option<std::path::PathBuf> {
    let is_debug_build = cfg!(debug_assertions);
    let folder = std::env::var_os(SNAPSHOT_FOLDER_ENV)?;
    is_debug_build.then(|| folder.into())
}

/// Debug builds only: LEGION2_OPEN=<deployment>[/<position>] opens that deployment (and
/// that position's terminal) at start, so a snapshot can show it. With
/// snapshots on, a file in the snapshot folder says the same and is read on
/// every reload, so the view can change without relaunching the app.
const OPEN_AT_START_ENV: &str = "LEGION2_OPEN";
const OPEN_AT_START_FILE_NAME: &str = "open.txt";

#[derive(serde::Serialize, Debug, PartialEq)]
struct StartingView {
    deployment: String,
    position: Option<String>,
}

/// `deployment`, `deployment/position`, or a page named by kind (`folder:<path>`,
/// `question:<entry>`), which is passed on whole: a path has slashes too.
fn parse_starting_view(requested: &str) -> StartingView {
    let names_a_page = requested.starts_with("folder:") || requested.starts_with("question:");
    match requested.split_once('/') {
        Some((deployment, position)) if !names_a_page => StartingView { deployment: deployment.to_string(), position: Some(position.to_string()) },
        _ => StartingView { deployment: requested.to_string(), position: None },
    }
}

#[tauri::command]
fn starting_view() -> Option<StartingView> {
    let is_debug_build = cfg!(debug_assertions);
    let from_file = snapshot_folder().and_then(|folder| std::fs::read_to_string(folder.join(OPEN_AT_START_FILE_NAME)).ok());
    let requested = from_file
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .or_else(|| std::env::var(OPEN_AT_START_ENV).ok())
        .filter(|_| is_debug_build)?;
    Some(parse_starting_view(&requested))
}

#[tauri::command]
fn is_snapshotting() -> bool {
    snapshot_folder().is_some()
}

/// Saves the page as rendered (stylesheets pointed at the built files, so a
/// browser can redraw it) and every element's position.
#[tauri::command]
fn save_snapshot(page: String, layout: String) -> Result<(), String> {
    let folder = snapshot_folder().ok_or("snapshots are off")?;
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let built_assets = format!("href=\"file://{BUILT_PAGE_FOLDER}/assets/");
    let standalone_page = page.replace("href=\"/assets/", &built_assets);
    std::fs::write(folder.join("page.html"), standalone_page).map_err(|error| error.to_string())?;
    std::fs::write(folder.join("layout.json"), layout).map_err(|error| error.to_string())
}

#[tauri::command]
async fn legion_request(command: Command, connection: tauri::State<'_, RequestConnection>) -> Result<Reply, String> {
    let mut held = connection.0.lock().await;
    if held.is_none() {
        *held = Some(Client::connect().await?);
    }
    let client = held.as_mut().ok_or("not connected to legion2d")?;
    let outcome = client.ask(command).await;
    if outcome.is_err() {
        // Reconnect next time, in case legion2d restarted.
        *held = None;
    }
    outcome
}

/// Follows legion2d's events for as long as the app runs, reconnecting
/// whenever the connection drops.
async fn stream_events(app: AppHandle) {
    loop {
        let watched = async {
            let mut client = Client::connect().await?;
            client.ask(Command::Watch).await?;
            app.emit(CONNECTION_CHANNEL, true).map_err(|error| error.to_string())?;
            loop {
                if let ServerMessage::Event { event } = client.next_message().await? {
                    app.emit::<Event>(EVENT_CHANNEL, event).map_err(|error| error.to_string())?;
                }
            }
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        };
        if let Err(problem) = watched.await {
            eprintln!("legion2 app: {problem}");
        }
        let _ = app.emit(CONNECTION_CHANNEL, false);
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

fn main() {
    tauri::Builder::default()
        .manage(RequestConnection(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![legion_request, is_snapshotting, save_snapshot, starting_view])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(stream_events(handle));
            app.get_webview_window("main").ok_or("the app has no main window")?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the Legion app failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deployment_and_position_split_on_the_slash() {
        assert_eq!(parse_starting_view("q-test/builder"), StartingView { deployment: "q-test".into(), position: Some("builder".into()) });
        assert_eq!(parse_starting_view("q-test"), StartingView { deployment: "q-test".into(), position: None });
    }

    #[test]
    fn a_named_page_keeps_its_slashes() {
        assert_eq!(parse_starting_view("folder:/repo/app"), StartingView { deployment: "folder:/repo/app".into(), position: None });
    }
}
