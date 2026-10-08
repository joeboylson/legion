//! The messages legion2d and its clients send each other over its WebSocket.
//!
//! A client sends a [`Request`]: an ID it picks and a [`Command`]. legion2d
//! answers with a [`ServerMessage::Reply`] carrying the same ID. A client
//! that sent [`Command::Watch`] also gets [`ServerMessage::Event`]s as things
//! happen, without asking.

use serde::{Deserialize, Serialize};

pub mod tools;

/// Every name the new version uses comes from this, so it can run beside
/// today's Legion and switch to "legion" in one change.
pub const NAME: &str = "legion2";

/// Where legion2d keeps its data and its socket: `~/.local/share/<NAME>`.
pub fn data_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".local/share").join(NAME))
}

/// The one address legion2d listens on. Only this machine can reach it;
/// another machine forwards it over SSH.
pub fn socket_path() -> Option<std::path::PathBuf> {
    Some(data_dir()?.join(format!("{NAME}d.sock")))
}

/// Set in every session legion2d starts, so the `legion2` command run inside
/// it knows which deployment and position it speaks for.
pub const ENV_DEPLOYMENT: &str = "LEGION2_DEPLOYMENT";
pub const ENV_POSITION: &str = "LEGION2_POSITION";
/// Set when the `legion2` command should reach a legion2d elsewhere.
pub const ENV_SOCKET: &str = "LEGION2_SOCKET";

/// Who the human is in the deployment log.
pub const HUMAN: &str = "human";
/// Entries Legion writes itself come from this.
pub const LEGION: &str = NAME;
pub const COMMANDER: &str = "commander";
/// The position that watches a deployment for ways to go faster and
/// suggests them to the commander. It never commands.
pub const STRATEGIST: &str = "strategist";

/// What a position may do, from its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Commander,
    Strategist,
    Operator,
}

pub fn role_of_position(position: &str) -> Role {
    match position {
        COMMANDER => Role::Commander,
        STRATEGIST => Role::Strategist,
        _ => Role::Operator,
    }
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Request {
    pub id: String,
    /// The deployment and position the caller speaks for, when it's a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Caller>,
    pub command: Command,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Caller {
    pub deployment: String,
    pub position: String,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Ping,
    /// Adds a folder, setting up `.legion2/` in it if it has none.
    FolderAdd { path: String },
    FolderList,
    /// Everything about one folder: its settings, pipelines, operators and deployments.
    FolderRead { folder: String },
    DeploymentStart { folder: String, pipeline: String, name: Option<String> },
    DeploymentList { folder: Option<String> },
    /// Ends every session in the deployment; it isn't brought back after a restart.
    DeploymentClose { deployment: String },
    MissionAdd { deployment: String, title: String, body: String },
    MissionList { deployment: String },
    MissionRead { deployment: String, mission: u32 },
    /// Moves the base branch up to a done mission's branch, replaying it
    /// and running the folder's check first if the base has moved on.
    MissionFinish { deployment: String, mission: u32 },
    /// Starts an operator, or the commander, in the deployment.
    /// `part`: one part of a split mission, worked in its own checkout.
    SessionStart { deployment: String, operator: String, mission: Option<u32>, part: Option<u32> },
    SessionStop { deployment: String, position: String },
    SessionList { deployment: Option<String> },
    Screen { deployment: String, position: String },
    Key { deployment: String, position: String, key: String },
    /// Types straight into a session's terminal, as the app's live terminal does.
    Input { deployment: String, position: String, text: String },
    /// Adds an entry to the deployment log. Messages, handoffs and answers addressed
    /// to a position are handed to its session.
    Post { deployment: String, entry: NewEntry },
    Log { deployment: String, filter: LogFilter },
    /// Copies a tool into the folder's shared tools, where every mission sees
    /// it at once, and tells everyone running in the deployment about it.
    ToolShare { deployment: String, file: String, summary: String },
    /// Splits a mission's current step into parts that can be worked at the
    /// same time, each in its own checkout branched from the mission's.
    MissionSplit { deployment: String, mission: u32, parts: Vec<String> },
    /// A part is done: Legion merges it into the mission's branch.
    PartFinish { deployment: String, mission: u32, part: u32, note: String },
    /// A one-line heads-up for everyone: kept in the folder's callouts and
    /// told to everyone running.
    Callout { deployment: String, text: String },
    /// The folder's callouts, oldest first.
    CalloutList { deployment: String },
    /// The folder's shared tools, or one tool's text when `tool` names it.
    ToolboxRead { deployment: String, tool: Option<String> },
    /// Sends events from now on, for as long as the connection stays open.
    Watch,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewEntry {
    pub kind: EntryKind,
    #[serde(default)]
    pub mission: Option<u32>,
    #[serde(default)]
    pub to: Option<String>,
    pub text: String,
    /// For an answer: the question entry it answers.
    #[serde(default)]
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub answers: Option<i64>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct LogFilter {
    #[serde(default)]
    pub mission: Option<u32>,
    /// Entries from or to this position.
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub kinds: Option<Vec<EntryKind>>,
    /// Milliseconds since 1970.
    #[serde(default)]
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub since_ms: Option<i64>,
    /// Questions and decisions that have no answer yet, only.
    #[serde(default)]
    pub open_questions: bool,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Reply { id: String, #[serde(flatten)] outcome: Outcome },
    Event { event: Event },
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(Reply),
    Error(String),
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Pong { version: String },
    Done,
    Folder { folder: Folder, created_setup: bool },
    Folders { folders: Vec<Folder> },
    FolderDetail { detail: FolderDetail },
    Deployment { deployment: Deployment },
    Deployments { deployments: Vec<Deployment> },
    Mission { mission: Mission, body: String, parts: Vec<Part> },
    Parts { parts: Vec<Part> },
    Missions { missions: Vec<Mission> },
    Session { session: SessionInfo },
    Sessions { sessions: Vec<SessionInfo> },
    /// `text` is the screen as plain text; `ansi` redraws it, colors and
    /// cursor included, in a terminal view.
    Screen { text: String, ansi: String },
    Entry { entry: Entry },
    Entries { entries: Vec<Entry> },
    /// Plain text to show as it is.
    Text { text: String },
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Entry { entry: Entry },
    Session { session: SessionInfo },
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Folder {
    pub path: String,
    pub name: String,
    pub pipelines: Vec<String>,
    pub operators: Vec<String>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FolderDetail {
    pub folder: Folder,
    /// The command that checks a mission's work, if any.
    pub check: Option<String>,
    pub permission_mode: Option<String>,
    /// Where its missions, worktrees and deployment log live, outside the repo.
    pub outside_folder: String,
    pub pipelines: Vec<PipelineDetail>,
    pub operators: Vec<OperatorDetail>,
    pub deployments: Vec<Deployment>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PipelineDetail {
    pub name: String,
    pub operators: Vec<String>,
    pub first: Option<String>,
    pub decisions: Vec<DecisionDetail>,
    /// Why Legion can't use it as written, if it can't.
    pub problem: Option<String>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DecisionDetail {
    pub operator: String,
    pub condition: String,
    pub next: String,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OperatorDetail {
    pub name: String,
    pub definition: String,
    pub model: Option<String>,
    pub copy_limit: u32,
    pub permission_mode: Option<String>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    /// Why Legion can't read it, if it can't.
    pub problem: Option<String>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Deployment {
    pub id: String,
    pub name: String,
    pub folder: String,
    pub pipeline: String,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub started_ms: i64,
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub closed_ms: Option<i64>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Mission {
    pub number: u32,
    pub deployment: String,
    pub title: String,
    pub file: String,
    pub status: MissionStatus,
    /// Who holds it now, if anyone.
    pub holder: Option<String>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Waiting,
    Started,
    HandedOff,
    Paused,
    Blocked,
    Done,
}

/// One part of a split mission: a piece of its current step, worked in its
/// own checkout and merged back into the mission's branch when done.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub mission: u32,
    pub number: u32,
    pub brief: String,
    pub is_merged: bool,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionInfo {
    pub deployment: String,
    pub position: String,
    pub mission: Option<u32>,
    pub activity: Activity,
    /// False when the add-on is a version legion2d doesn't know.
    pub can_see_state: bool,
    /// What the session is waiting on, for a permission question.
    pub detail: Option<String>,
    /// It hasn't started in time: it's likely waiting on a question shown
    /// before the add-on loads, such as whether to trust the folder.
    pub is_stuck_starting: bool,
    /// The model it runs, as Claude names it; known once its add-on reports.
    pub model: Option<String>,
    /// How full its conversation is, in percent; known once its add-on reports.
    pub context_percent: Option<u8>,
    /// The part of its mission it works on, if the mission is split.
    pub part: Option<u32>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Starting,
    Idle,
    Busy,
    Permission,
    /// Hit its usage limit; the add-on tells it to carry on once it resets.
    Limited,
    /// A person stopped its turn (Esc, or a deny, in its terminal); it waits
    /// to be told what to do next.
    Halted,
    Ended,
}

impl Activity {
    pub fn label(self) -> &'static str {
        match self {
            Activity::Starting => "starting",
            Activity::Idle => "idle",
            Activity::Busy => "busy",
            Activity::Permission => "waiting on a permission question",
            Activity::Limited => "waiting for its usage limit to reset",
            Activity::Halted => "halted, waiting to be told what to do next",
            Activity::Ended => "ended",
        }
    }
}

/// One event in a deployment. Entries are only ever added.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entry {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub id: i64,
    pub deployment: String,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub at_ms: i64,
    pub mission: Option<u32>,
    pub from: String,
    pub to: Option<String>,
    pub kind: EntryKind,
    pub text: String,
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub answers: Option<i64>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    DeploymentStarted,
    DeploymentClosed,
    MissionAdded,
    SessionStarted,
    SessionEnded,
    Permission,
    Message,
    Note,
    Question,
    Answer,
    Handoff,
    Done,
    Blocked,
    Paused,
    Resumed,
    Postmortem,
    Suggestion,
    /// A mission's branch made it onto the base branch.
    Finished,
    /// A choice a session made and carried on with, for the human to
    /// overrule with an answer if they want.
    Decision,
    /// A heads-up handed to everyone running, such as a callout or a shared
    /// tool. No reply is wanted.
    Announcement,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::DeploymentStarted => "deployment_started",
            EntryKind::DeploymentClosed => "deployment_closed",
            EntryKind::MissionAdded => "mission_added",
            EntryKind::SessionStarted => "session_started",
            EntryKind::SessionEnded => "session_ended",
            EntryKind::Permission => "permission",
            EntryKind::Message => "message",
            EntryKind::Note => "note",
            EntryKind::Question => "question",
            EntryKind::Answer => "answer",
            EntryKind::Handoff => "handoff",
            EntryKind::Done => "done",
            EntryKind::Blocked => "blocked",
            EntryKind::Paused => "paused",
            EntryKind::Resumed => "resumed",
            EntryKind::Postmortem => "postmortem",
            EntryKind::Suggestion => "suggestion",
            EntryKind::Finished => "finished",
            EntryKind::Decision => "decision",
            EntryKind::Announcement => "announcement",
        }
    }

    pub fn parse(s: &str) -> Option<EntryKind> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }

    /// Kinds handed to the session of the position they're addressed to.
    pub fn is_delivered(self) -> bool {
        matches!(
            self,
            EntryKind::MissionAdded
                | EntryKind::Message
                | EntryKind::Answer
                | EntryKind::Handoff
                | EntryKind::Done
                | EntryKind::Blocked
                | EntryKind::Paused
                | EntryKind::Resumed
                | EntryKind::Announcement
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Serialize + for<'de> Deserialize<'de>>(value: &T) -> T {
        serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
    }

    #[test]
    fn replies_carry_their_request_id_and_outcome() {
        let reply = ServerMessage::Reply { id: "7".into(), outcome: Outcome::Ok(Reply::Done) };
        let json = serde_json::to_value(&reply).unwrap();
        assert_eq!(json["type"], "reply");
        assert_eq!(json["id"], "7");
        assert!(matches!(round_trip(&reply), ServerMessage::Reply { outcome: Outcome::Ok(Reply::Done), .. }));
    }

    #[test]
    fn errors_come_back_as_text() {
        let reply = ServerMessage::Reply { id: "1".into(), outcome: Outcome::Error("no deployment".into()) };
        assert!(matches!(round_trip(&reply), ServerMessage::Reply { outcome: Outcome::Error(text), .. } if text == "no deployment"));
    }

    #[test]
    fn commands_are_tagged_by_type() {
        let request = Request { id: "1".into(), from: None, command: Command::MissionFinish { deployment: "r".into(), mission: 3 } };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["command"]["type"], "mission_finish");
        assert!(json.get("from").is_none());
        assert!(matches!(round_trip(&request).command, Command::MissionFinish { mission: 3, .. }));
    }

    #[test]
    fn every_entry_kind_names_itself_the_way_it_serializes() {
        let kinds = [
            EntryKind::DeploymentStarted,
            EntryKind::DeploymentClosed,
            EntryKind::MissionAdded,
            EntryKind::SessionStarted,
            EntryKind::SessionEnded,
            EntryKind::Permission,
            EntryKind::Message,
            EntryKind::Note,
            EntryKind::Question,
            EntryKind::Answer,
            EntryKind::Handoff,
            EntryKind::Done,
            EntryKind::Blocked,
            EntryKind::Paused,
            EntryKind::Resumed,
            EntryKind::Postmortem,
            EntryKind::Suggestion,
            EntryKind::Finished,
            EntryKind::Decision,
            EntryKind::Announcement,
        ];
        for kind in kinds {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
            assert_eq!(EntryKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(EntryKind::parse("gossip"), None);
    }

    #[test]
    fn only_addressed_work_is_delivered() {
        assert!(EntryKind::Handoff.is_delivered());
        assert!(!EntryKind::Note.is_delivered());
        assert!(!EntryKind::Question.is_delivered());
    }
}
