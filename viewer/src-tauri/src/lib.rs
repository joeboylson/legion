use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Deserialize, Clone)]
struct SquadEntry {
    path: String,
    name: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    squads: Vec<SquadEntry>,
}

#[derive(Serialize)]
struct OperatorInfo {
    operator: String,
    window: String,
}

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
struct PtyState(Mutex<HashMap<String, PtySession>>);

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("squads.json"))
}

fn read_config(app: &AppHandle) -> Result<Config, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn write_config(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let path = config_path(app)?;
    let raw = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

// Squad name lives in .legion/squad as `LEGION_SQUAD_NAME="..."` — a small
// shell-sourceable file, not JSON. Parse the one line we care about rather
// than shelling out to bash just to read a variable.
fn read_squad_name(project_path: &str) -> Result<String, String> {
    let squad_file = PathBuf::from(project_path).join(".legion").join("squad");
    let raw = fs::read_to_string(&squad_file)
        .map_err(|_| format!("no .legion/squad found at {project_path} — is this a legion project?"))?;
    for line in raw.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("LEGION_SQUAD_NAME=") {
            let name = rest.trim_matches('"');
            return Ok(name.to_string());
        }
    }
    Err("LEGION_SQUAD_NAME not found in .legion/squad".to_string())
}

#[tauri::command]
fn list_squads(app: AppHandle) -> Result<Vec<SquadEntry>, String> {
    Ok(read_config(&app)?.squads)
}

#[tauri::command]
fn add_squad(app: AppHandle, path: String) -> Result<SquadEntry, String> {
    let name = read_squad_name(&path)?;
    let mut cfg = read_config(&app)?;
    if !cfg.squads.iter().any(|s| s.path == path) {
        cfg.squads.push(SquadEntry {
            path: path.clone(),
            name: name.clone(),
        });
        write_config(&app, &cfg)?;
    }
    Ok(SquadEntry { path, name })
}

#[tauri::command]
fn remove_squad(app: AppHandle, path: String) -> Result<(), String> {
    let mut cfg = read_config(&app)?;
    cfg.squads.retain(|s| s.path != path);
    write_config(&app, &cfg)
}

// Every legion window is named literally "<squad>-<operator>" (see
// open_window() in the legion CLI) — no separate lookup table, just filter
// tmux's live window list by that prefix.
#[tauri::command]
fn list_operators(squad_name: String) -> Result<Vec<OperatorInfo>, String> {
    let output = std::process::Command::new("tmux")
        .args(["list-windows", "-a", "-F", "#{window_name}"])
        .output()
        .map_err(|e| format!("failed to run tmux: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let prefix = format!("{squad_name}-");
    let mut ops: Vec<OperatorInfo> = text
        .lines()
        .filter(|w| w.starts_with(&prefix))
        .map(|w| OperatorInfo {
            operator: w[prefix.len()..].to_string(),
            window: w.to_string(),
        })
        .collect();
    ops.sort_by(|a, b| {
        // commander first, then alphabetical
        (a.operator != "commander", &a.operator).cmp(&(b.operator != "commander", &b.operator))
    });
    Ok(ops)
}

// Spawns `tmux attach -t <window>` behind a real PTY. This is a genuine
// second tmux client on that window — input typed into the resulting
// xterm.js tile is a live keystroke into that operator's actual session,
// same as attaching in a terminal by hand.
#[tauri::command]
fn spawn_terminal(app: AppHandle, id: String, window: String, rows: u16, cols: u16) -> Result<(), String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let mut cmd = CommandBuilder::new("tmux");
    cmd.args(["attach-session", "-t", &window]);

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    let state = app.state::<PtyState>();
    state.0.lock().unwrap().insert(
        id.clone(),
        PtySession {
            master: pair.master,
            writer,
            child,
        },
    );

    let app_handle = app.clone();
    let event_id = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    if app_handle
                        .emit(&format!("pty-data-{event_id}"), chunk)
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = app_handle.emit(&format!("pty-closed-{event_id}"), ());
    });

    Ok(())
}

#[tauri::command]
fn write_terminal(state: tauri::State<PtyState>, id: String, data: String) -> Result<(), String> {
    let mut sessions = state.0.lock().unwrap();
    if let Some(session) = sessions.get_mut(&id) {
        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn resize_terminal(state: tauri::State<PtyState>, id: String, rows: u16, cols: u16) -> Result<(), String> {
    let sessions = state.0.lock().unwrap();
    if let Some(session) = sessions.get(&id) {
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn close_terminal(state: tauri::State<PtyState>, id: String) -> Result<(), String> {
    let mut sessions = state.0.lock().unwrap();
    if let Some(mut session) = sessions.remove(&id) {
        let _ = session.child.kill();
    }
    Ok(())
}

use std::io::Read;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(PtyState::default())
        .invoke_handler(tauri::generate_handler![
            list_squads,
            add_squad,
            remove_squad,
            list_operators,
            spawn_terminal,
            write_terminal,
            resize_terminal,
            close_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
