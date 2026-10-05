//! legion2d's shared state, and what everything else goes through: adding
//! run log entries, telling watchers, and dispatching commands.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use legion2_proto::{Activity, Caller, Command, Entry, Event, NewEntry, Reply, SessionInfo, HUMAN, LEGION};
use tokio::sync::broadcast;

use crate::{
    access::{role_of_position, run_a_session_command_targets},
    activity_entries::entry_for_activity_change,
    constants::{EVENT_BUFFER_SIZE, KNOWN_ADDON_VERSIONS},
    entry_routing::delivery_text,
    folder_state::{read_folder_registry, FolderState},
    sessions::Session,
};

pub struct Config {
    pub data_folder: PathBuf,
    pub socket_path: PathBuf,
    pub addon_folder: PathBuf,
    pub claude_command: String,
    /// Where legion2d and the legion2 command live.
    pub binary_folder: PathBuf,
    /// The most sessions busy at once across the machine. A waiting
    /// commander doesn't count.
    pub max_busy_sessions: usize,
    /// How long a permission request waits for an answer before it's refused.
    pub permission_timeout: Duration,
}

pub struct Daemon {
    pub config: Config,
    pub state: Mutex<State>,
    pub events: broadcast::Sender<Event>,
}

pub struct State {
    pub folders: Vec<FolderState>,
    /// By session ID.
    pub sessions: HashMap<String, Session>,
    /// When Legion last restarted each run's commander after it ended by itself.
    pub commander_restarted_at: HashMap<String, i64>,
}

/// What the add-on reports about its session.
pub struct ActivityReport {
    pub activity: Activity,
    pub detail: Option<String>,
    pub addon_version: Option<String>,
}

impl Daemon {
    pub fn new(config: Config) -> Arc<Daemon> {
        let folders = read_folder_registry(&config.data_folder)
            .into_iter()
            .filter_map(|path| {
                FolderState::open(&config.data_folder, &path)
                    .inspect_err(|error| eprintln!("{LEGION}d: skipping {}: {error}", path.display()))
                    .ok()
            })
            .collect();
        let (events, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        let state = State { folders, sessions: HashMap::new(), commander_restarted_at: HashMap::new() };
        Arc::new(Daemon { config, state: Mutex::new(state), events })
    }

    pub fn announce_session(&self, info: &SessionInfo) {
        // No one watching is fine.
        let _ = self.events.send(Event::Session { session: info.clone() });
    }

    /// Adds an entry to a run's log and tells everyone watching.
    pub fn post_entry(&self, run_id: &str, author: &str, entry: NewEntry) -> Result<Entry, String> {
        let added = {
            let state = self.state.lock().unwrap();
            let (folder_index, _) = state.find_run(run_id)?;
            state.folders[folder_index].store.add_entry(run_id, author, &entry)?
        };
        let _ = self.events.send(Event::Entry { entry: added.clone() });
        Ok(added)
    }

    /// For entries Legion writes on its own, where no caller is waiting to
    /// hear about a failure: the only place left to say so is legion2d's output.
    pub fn post_legion_entry(&self, run_id: &str, entry: NewEntry) {
        if let Err(error) = self.post_entry(run_id, LEGION, entry) {
            eprintln!("{LEGION}d: {error}");
        }
    }

    /// The add-on reporting what its session is doing. False when legion2d
    /// doesn't know the session.
    pub fn record_activity(&self, session_id: &str, report: ActivityReport) -> bool {
        let change = {
            let mut state = self.state.lock().unwrap();
            let Some(session) = state.sessions.get_mut(session_id) else { return false };
            if let Some(version) = &report.addon_version {
                session.can_see_state = KNOWN_ADDON_VERSIONS.contains(&version.as_str());
            }
            let is_unchanged = session.activity == report.activity && session.detail == report.detail;
            if !session.can_see_state || is_unchanged {
                return true;
            }
            let logged = entry_for_activity_change(&session.position, session.activity, report.activity, report.detail.as_deref());
            let is_new_permission_question = matches!(logged, Some((legion2_proto::EntryKind::Permission, _)));
            session.permission_asked_at = match report.activity {
                Activity::Permission if is_new_permission_question => Some(Instant::now()),
                Activity::Permission => session.permission_asked_at,
                _ => None,
            };
            session.activity = report.activity;
            session.detail = report.detail;
            (session.info(), logged)
        };
        let (info, logged) = change;
        self.announce_session(&info);
        if let Some((kind, text)) = logged {
            let entry = NewEntry { kind, mission: info.mission, to: Some(HUMAN.into()), text, answers: None };
            if let Err(error) = self.post_entry(&info.run, &info.position, entry) {
                eprintln!("{LEGION}d: {error}");
            }
        }
        true
    }

    /// What the add-on hands its session: entries addressed to its position
    /// that no session has had yet. None when legion2d doesn't know the session.
    pub fn take_deliveries(&self, session_id: &str) -> Result<Option<Vec<String>>, String> {
        let state = self.state.lock().unwrap();
        let Some(session) = state.sessions.get(session_id) else { return Ok(None) };
        let (folder_index, _) = state.find_run(&session.run)?;
        let store = &state.folders[folder_index].store;
        let entries = store.undelivered_entries(&session.run, &session.position)?;
        let entry_ids: Vec<i64> = entries.iter().map(|entry| entry.id).collect();
        store.mark_delivered(&entry_ids)?;
        Ok(Some(entries.iter().map(delivery_text).collect()))
    }

    /// A session may only act within its own run, and only as its role allows.
    fn check_session_may_send(&self, caller: &Caller, command: &Command) -> Result<(), String> {
        let role = role_of_position(&caller.position);
        let Some(target_run) = run_a_session_command_targets(role, command)? else { return Ok(()) };
        let state = self.state.lock().unwrap();
        let (_, target) = state.find_run(target_run)?;
        let (_, own) = state.find_run(&caller.run)?;
        if target.id != own.id {
            return Err(format!("{} can only act in its own run ({})", caller.position, own.name));
        }
        Ok(())
    }

    pub fn handle_command(self: &Arc<Self>, caller: Option<Caller>, command: Command) -> Result<Reply, String> {
        if let Some(session_caller) = &caller {
            self.check_session_may_send(session_caller, &command)?;
        }
        let author = caller.map(|session_caller| session_caller.position).unwrap_or_else(|| HUMAN.into());
        match command {
            Command::Ping => Ok(Reply::Pong { version: env!("CARGO_PKG_VERSION").into() }),
            Command::Watch => Ok(Reply::Done),
            Command::FolderAdd { path } => self.add_folder(&path),
            Command::FolderList => Ok(self.list_folders()),
            Command::RunStart { folder, pipeline, name } => self.start_run(&folder, &pipeline, name),
            Command::RunList { folder } => Ok(self.list_runs(folder.as_deref())),
            Command::RunClose { run } => self.close_run(&run),
            Command::MissionAdd { run, title, body } => self.add_mission(&run, &title, &body),
            Command::MissionList { run } => self.list_missions(&run),
            Command::MissionRead { run, mission } => self.read_mission(&run, mission),
            Command::MissionFinish { run, mission } => self.finish_mission(&run, mission),
            Command::SessionStart { run, operator, mission } => self.start_session(&run, &operator, mission, &author, None),
            Command::SessionStop { run, position } => self.stop_session(&run, &position),
            Command::SessionList { run } => self.list_sessions(run.as_deref()),
            Command::Screen { run, position } => self.read_screen(&run, &position),
            Command::Key { run, position, key } => self.press_key(&run, &position, &key),
            Command::Input { run, position, text } => self.type_input(&run, &position, &text),
            Command::Post { run, entry } => self.post_from_caller(&run, &author, entry),
            Command::Log { run, filter } => self.read_log(&run, &filter),
        }
    }
}
