//! legion2d's shared state, and what everything else goes through: adding
//! deployment log entries, telling watchers, and dispatching commands.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Instant,
};

use legion2_proto::{role_of_position, Activity, Caller, Command, Entry, Event, NewEntry, Reply, SessionInfo, HUMAN, LEGION};
use tokio::sync::broadcast;

use crate::context_handover::{note_request, on_turn_end, HandoverAction};
use crate::{
    access::run_a_session_command_targets,
    channels::ChannelManager,
    activity_entries::entry_for_activity_change,
    unreported_turns::{is_announcements_only, next_turn_is_announcements_only, unreported_turn_message},
    constants::{EVENT_BUFFER_SIZE, KNOWN_ADDON_VERSIONS},
    entry_routing::delivery_text,
    folder_state::{read_folder_registry, FolderState},
    machine_settings::{read_machine_settings, MachineSettings},
    session_commands::StartRequest,
    sessions::Session,
};

pub struct Config {
    pub data_folder: PathBuf,
    pub socket_path: PathBuf,
    pub addon_folder: PathBuf,
    pub claude_command: String,
    /// Where legion2d and the legion2 command live.
    pub binary_folder: PathBuf,
}

pub struct Daemon {
    pub config: Config,
    pub state: Mutex<State>,
    pub events: broadcast::Sender<Event>,
    /// The channel this machine hosts and the ones it subscribes to.
    pub channels: ChannelManager,
    /// Set once legion2d is stopping: sessions cut off by its exit are
    /// still remembered as running, so they come back when it starts again.
    pub is_shutting_down: AtomicBool,
}

pub struct State {
    pub folders: Vec<FolderState>,
    /// By session ID.
    pub sessions: HashMap<String, Session>,
    /// When Legion last restarted each deployment's commander after it ended by itself.
    pub commander_restarted_at: HashMap<String, i64>,
    /// Stalled work already pointed out to a commander, by deployment and stall.
    pub stalls_pointed_out: std::collections::HashSet<String>,
    /// Each deployment's pipeline and copy limits, as its commander last heard them.
    pub team_told_to_commander: HashMap<String, String>,
    /// Sessions to bring back after a restart once the machine has room.
    pub pending_restores: Vec<PendingRestore>,
    /// When each deployment's strategist last started, and was last asked for a check.
    pub strategist_started_at: HashMap<String, i64>,
    pub strategist_checked_at: HashMap<String, i64>,
}

/// A session that was running when legion2d stopped, waiting for room to start again.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingRestore {
    pub deployment: String,
    pub position: String,
    pub mission: u32,
    pub part: Option<u32>,
}

/// What the add-on reports about its session.
pub struct ActivityReport {
    pub activity: Activity,
    pub detail: Option<String>,
    pub addon_version: Option<String>,
    pub model: Option<String>,
    pub context_percent: Option<u8>,
    /// What the session wrote at the end of a turn, with an idle report.
    pub final_answer: Option<String>,
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
        let state = State {
            folders,
            sessions: HashMap::new(),
            commander_restarted_at: HashMap::new(),
            stalls_pointed_out: Default::default(),
            team_told_to_commander: HashMap::new(),
            pending_restores: Vec::new(),
            strategist_started_at: HashMap::new(),
            strategist_checked_at: HashMap::new(),
        };
        let channels = ChannelManager::new(config.data_folder.clone(), events.clone());
        Arc::new(Daemon { config, state: Mutex::new(state), events, channels, is_shutting_down: AtomicBool::new(false) })
    }

    /// Read fresh each time, so a change to settings.json needs no restart.
    pub fn machine_settings(&self) -> Result<MachineSettings, String> {
        read_machine_settings(&self.config.data_folder)
    }

    pub fn announce_session(&self, info: &SessionInfo) {
        // No one watching is fine.
        let _ = self.events.send(Event::Session { session: info.clone() });
    }

    /// Adds an entry to a deployment's log and tells everyone watching.
    pub fn post_entry(&self, deployment_id: &str, author: &str, entry: NewEntry) -> Result<Entry, String> {
        let added = {
            let state = self.state.lock().unwrap();
            let (folder_index, _) = state.find_deployment(deployment_id)?;
            state.folders[folder_index].store.add_entry(deployment_id, author, &entry)?
        };
        let _ = self.events.send(Event::Entry { entry: added.clone() });
        Ok(added)
    }

    /// For entries Legion writes on its own, where no caller is waiting to
    /// hear about a failure: the only place left to say so is legion2d's output.
    pub fn post_legion_entry(&self, deployment_id: &str, entry: NewEntry) {
        if let Err(error) = self.post_entry(deployment_id, LEGION, entry) {
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
            if report.model.is_some() {
                session.model = report.model.clone();
            }
            if report.context_percent.is_some() {
                session.context_percent = report.context_percent;
            }
            let is_unchanged = session.activity == report.activity && session.detail == report.detail;
            if !session.can_see_state || is_unchanged {
                return true;
            }
            let logged = entry_for_activity_change(&session.position, session.activity, report.activity, report.detail.as_deref());
            let is_turn_starting = report.activity == Activity::Busy && session.activity != Activity::Busy;
            let finished_turn = match report.activity {
                Activity::Idle if session.is_answering_announcements => None,
                Activity::Idle => session.turn_started_ms.zip(report.final_answer.clone()),
                _ => None,
            };
            if report.activity == Activity::Idle {
                session.is_answering_announcements = false;
            }
            session.turn_started_ms = if is_turn_starting { Some(crate::store::now_ms()) } else { session.turn_started_ms };
            if is_turn_starting {
                session.is_answering_announcements = session.next_turn_answers_announcements.take().unwrap_or(false);
            }
            let is_new_permission_question = matches!(logged, Some((legion2_proto::EntryKind::Permission, _)));
            session.permission_asked_at = match report.activity {
                Activity::Permission if is_new_permission_question => Some(Instant::now()),
                Activity::Permission => session.permission_asked_at,
                _ => None,
            };
            let is_turn_ending = report.activity == Activity::Idle && session.activity != Activity::Idle;
            let handover = if is_turn_ending {
                let has_work = session.mission.is_some() || session.position == legion2_proto::COMMANDER;
                let (phase, action) = on_turn_end(session.handover, session.context_percent, session.clear_at, has_work);
                session.handover = phase;
                action
            } else {
                None
            };
            session.activity = report.activity;
            session.detail = report.detail;
            (session.info(), logged, finished_turn, handover)
        };
        let (info, logged, finished_turn, handover) = change;
        self.announce_session(&info);
        if let Some((kind, text)) = logged {
            let entry = NewEntry { kind, mission: info.mission, to: Some(HUMAN.into()), text, answers: None };
            if let Err(error) = self.post_entry(&info.deployment, &info.position, entry) {
                eprintln!("{LEGION}d: {error}");
            }
        }
        match handover {
            Some(HandoverAction::AskForNote { percent, clear_at }) => self.ask_for_handover_note(&info, percent, clear_at),
            Some(HandoverAction::Restart) => self.end_for_fresh_start(session_id),
            // The turn that wrote the note is the handover itself, not news.
            None => {
                if let Some((turn_started_ms, answer)) = finished_turn {
                    self.forward_unreported_turn(&info, turn_started_ms, &answer);
                }
            }
        }
        true
    }

    fn ask_for_handover_note(&self, info: &SessionInfo, percent: u8, clear_at: u8) {
        let text = note_request(&info.position, info.mission, percent, clear_at);
        let entry = NewEntry { kind: legion2_proto::EntryKind::Message, mission: info.mission, to: Some(info.position.clone()), text, answers: None };
        if let Err(error) = self.post_entry(&info.deployment, LEGION, entry) {
            eprintln!("{LEGION}d: {error}");
        }
    }

    /// Ends a session that has written its handoff note; handle_session_end
    /// starts its position again.
    fn end_for_fresh_start(&self, session_id: &str) {
        let mut state = self.state.lock().unwrap();
        let Some(session) = state.sessions.get_mut(session_id) else { return };
        session.is_stopping = true;
        if let Err(error) = session.end() {
            eprintln!("{LEGION}d: {error}");
        }
    }

    /// An operator that ends a turn without reporting anything through its
    /// tools may have left a question or a result on its screen, which no
    /// one reads. Pass what it wrote to the commander.
    fn forward_unreported_turn(&self, info: &SessionInfo, turn_started_ms: i64, answer: &str) {
        let reported_kinds = {
            let state = self.state.lock().unwrap();
            let Ok((folder_index, _)) = state.find_deployment(&info.deployment) else { return };
            let filter = legion2_proto::LogFilter { position: Some(info.position.clone()), since_ms: Some(turn_started_ms), ..Default::default() };
            state.folders[folder_index]
                .store
                .entries(&info.deployment, &filter)
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| entry.from == info.position)
                .map(|entry| entry.kind)
                .collect::<Vec<_>>()
        };
        let Some(text) = unreported_turn_message(&info.position, &reported_kinds, answer) else { return };
        let entry = NewEntry { kind: legion2_proto::EntryKind::Message, mission: info.mission, to: Some(legion2_proto::COMMANDER.into()), text, answers: None };
        if let Err(error) = self.post_entry(&info.deployment, LEGION, entry) {
            eprintln!("{LEGION}d: {error}");
        }
    }

    /// What the add-on hands its session: entries addressed to its position
    /// that no session has had yet. None when legion2d doesn't know the session.
    pub fn take_deliveries(&self, session_id: &str) -> Result<Option<Vec<String>>, String> {
        let mut state = self.state.lock().unwrap();
        let Some(session) = state.sessions.get(session_id) else { return Ok(None) };
        let (folder_index, _) = state.find_deployment(&session.deployment)?;
        let store = &state.folders[folder_index].store;
        let entries = store.undelivered_entries(&session.deployment, &session.position)?;
        let entry_ids: Vec<i64> = entries.iter().map(|entry| entry.id).collect();
        store.mark_delivered(&entry_ids)?;
        let handed_kinds: Vec<legion2_proto::EntryKind> = entries.iter().map(|entry| entry.kind).collect();
        if let (Some(only_announcements), Some(session)) = (is_announcements_only(&handed_kinds), state.sessions.get_mut(session_id)) {
            session.next_turn_answers_announcements = Some(next_turn_is_announcements_only(session.next_turn_answers_announcements, only_announcements));
        }
        Ok(Some(entries.iter().map(delivery_text).collect()))
    }

    /// A session may only act within its own deployment, and only as its role allows.
    fn check_session_may_send(&self, caller: &Caller, command: &Command) -> Result<(), String> {
        let role = role_of_position(&caller.position);
        let Some(target_deployment) = run_a_session_command_targets(role, command)? else { return Ok(()) };
        let state = self.state.lock().unwrap();
        let (_, target) = state.find_deployment(target_deployment)?;
        let (_, own) = state.find_deployment(&caller.deployment)?;
        if target.id != own.id {
            return Err(format!("{} can only act in its own deployment ({})", caller.position, own.name));
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
            Command::FolderRemove { folder } => self.remove_folder(&folder),
            Command::FolderRead { folder } => self.describe_folder(&folder),
            Command::DeploymentStart { folder, pipeline, name } => self.start_deployment(&folder, &pipeline, name),
            Command::DeploymentList { folder } => Ok(self.list_deployments(folder.as_deref())),
            Command::DeploymentClose { deployment } => self.close_deployment(&deployment),
            Command::DeploymentRename { deployment, name } => self.rename_deployment(&deployment, &name),
            Command::MissionAdd { deployment, title, body } => self.add_mission(&deployment, &title, &body),
            Command::MissionList { deployment } => self.list_missions(&deployment),
            Command::MissionRead { deployment, mission } => self.read_mission(&deployment, mission),
            Command::MissionFinish { deployment, mission } => self.finish_mission(&deployment, mission),
            Command::SessionStart { deployment, operator, mission, part } => {
                self.start_session_on(&deployment, StartRequest { operator: &operator, mission, part, position: None, starter: &author, first_prompt: None })
            }
            Command::SessionStop { deployment, position } => self.stop_session(&deployment, &position),
            Command::SessionList { deployment } => self.list_sessions(deployment.as_deref()),
            Command::Screen { deployment, position } => self.read_screen(&deployment, &position),
            Command::Key { deployment, position, key } => self.press_key(&deployment, &position, &key),
            Command::Input { deployment, position, text } => self.type_input(&deployment, &position, &text),
            Command::Post { deployment, entry } => self.post_from_caller(&deployment, &author, entry),
            Command::Log { deployment, filter } => self.read_log(&deployment, &filter),
            Command::ToolShare { deployment, file, summary } => self.share_tool(&deployment, &author, &file, &summary),
            Command::MissionSplit { deployment, mission, parts } => self.split_mission(&deployment, mission, &parts),
            Command::PartFinish { deployment, mission, part, note } => self.finish_part(&deployment, &author, mission, part, &note),
            Command::Callout { deployment, text } => self.call_out(&deployment, &author, &text),
            Command::CalloutList { deployment } => self.list_callouts(&deployment),
            Command::ToolboxRead { deployment, tool } => self.read_toolbox(&deployment, tool.as_deref()),
            Command::ChannelOpen { port, key } => self.channels.open(port, key).map(|channels| Reply::Channels { channels }),
            Command::ChannelClose => self.channels.close().map(|channels| Reply::Channels { channels }),
            Command::ChannelSubscribe { address, key } => self.channels.subscribe(&address, &key).map(|channels| Reply::Channels { channels }),
            Command::ChannelUnsubscribe { address } => self.channels.unsubscribe(&address).map(|channels| Reply::Channels { channels }),
            Command::ChannelStatus => Ok(Reply::Channels { channels: self.channels.status() }),
            Command::ChannelDeployments { deployment } => self.list_channel_deployments(&deployment),
            Command::ChannelDescribe { deployment, text } => self.describe_on_channels(&deployment, &text),
            Command::ChannelSend { deployment, to, text } => self.send_on_channels(&deployment, &to, &text),
            Command::ChannelSwitch { deployment, send, receive } => self.set_channel_switches(&deployment, send, receive),
            Command::ChannelLog { limit } => Ok(Reply::ChannelLog { entries: self.channels.log(limit) }),
        }
    }
}
