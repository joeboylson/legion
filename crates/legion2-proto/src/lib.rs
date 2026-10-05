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
/// it knows which run and position it speaks for.
pub const ENV_RUN: &str = "LEGION2_RUN";
pub const ENV_POSITION: &str = "LEGION2_POSITION";
/// Set when the `legion2` command should reach a legion2d elsewhere.
pub const ENV_SOCKET: &str = "LEGION2_SOCKET";

/// Who the human is in the run log.
pub const HUMAN: &str = "human";
/// Entries Legion writes itself come from this.
pub const LEGION: &str = NAME;
pub const COMMANDER: &str = "commander";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Request {
    pub id: String,
    /// The run and position the caller speaks for, when it's a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Caller>,
    pub command: Command,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Caller {
    pub run: String,
    pub position: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Ping,
    /// Adds a folder, setting up `.legion2/` in it if it has none.
    FolderAdd { path: String },
    FolderList,
    RunStart { folder: String, pipeline: String, name: Option<String> },
    RunList { folder: Option<String> },
    /// Ends every session in the run; it isn't brought back after a restart.
    RunClose { run: String },
    MissionAdd { run: String, title: String, body: String },
    MissionList { run: String },
    MissionRead { run: String, mission: u32 },
    /// Moves the base branch up to a done mission's branch, replaying it
    /// and running the folder's check first if the base has moved on.
    MissionFinish { run: String, mission: u32 },
    /// Starts an operator, or the commander, in the run.
    SessionStart { run: String, operator: String, mission: Option<u32> },
    SessionStop { run: String, position: String },
    SessionList { run: Option<String> },
    Screen { run: String, position: String },
    Key { run: String, position: String, key: String },
    /// Adds an entry to the run log. Messages, handoffs and answers addressed
    /// to a position are handed to its session.
    Post { run: String, entry: NewEntry },
    Log { run: String, filter: LogFilter },
    /// Sends events from now on, for as long as the connection stays open.
    Watch,
}

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
    pub answers: Option<i64>,
}

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
    pub since_ms: Option<i64>,
    /// Questions that have no answer yet, only.
    #[serde(default)]
    pub open_questions: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Reply { id: String, #[serde(flatten)] outcome: Outcome },
    Event { event: Event },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(Reply),
    Error(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Pong { version: String },
    Done,
    Folder { folder: Folder, created_setup: bool },
    Folders { folders: Vec<Folder> },
    Run { run: Run },
    Runs { runs: Vec<Run> },
    Mission { mission: Mission, body: String },
    Missions { missions: Vec<Mission> },
    Session { session: SessionInfo },
    Sessions { sessions: Vec<SessionInfo> },
    Screen { text: String },
    Entry { entry: Entry },
    Entries { entries: Vec<Entry> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Entry { entry: Entry },
    Session { session: SessionInfo },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Folder {
    pub path: String,
    pub name: String,
    pub pipelines: Vec<String>,
    pub operators: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Run {
    pub id: String,
    pub name: String,
    pub folder: String,
    pub pipeline: String,
    pub started_ms: i64,
    pub closed_ms: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Mission {
    pub number: u32,
    pub run: String,
    pub title: String,
    pub file: String,
    pub status: MissionStatus,
    /// Who holds it now, if anyone.
    pub holder: Option<String>,
}

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionInfo {
    pub run: String,
    pub position: String,
    pub mission: Option<u32>,
    pub activity: Activity,
    /// False when the add-on is a version legion2d doesn't know.
    pub can_see_state: bool,
    /// What the session is waiting on, for a permission question.
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Starting,
    Idle,
    Busy,
    Permission,
    /// Hit its usage limit; the add-on tells it to carry on once it resets.
    Limited,
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
            Activity::Ended => "ended",
        }
    }
}

/// One event in a run. Entries are only ever added.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entry {
    pub id: i64,
    pub run: String,
    pub at_ms: i64,
    pub mission: Option<u32>,
    pub from: String,
    pub to: Option<String>,
    pub kind: EntryKind,
    pub text: String,
    pub answers: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    RunStarted,
    RunClosed,
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
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::RunStarted => "run_started",
            EntryKind::RunClosed => "run_closed",
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
        let reply = ServerMessage::Reply { id: "1".into(), outcome: Outcome::Error("no run".into()) };
        assert!(matches!(round_trip(&reply), ServerMessage::Reply { outcome: Outcome::Error(text), .. } if text == "no run"));
    }

    #[test]
    fn commands_are_tagged_by_type() {
        let request = Request { id: "1".into(), from: None, command: Command::MissionFinish { run: "r".into(), mission: 3 } };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["command"]["type"], "mission_finish");
        assert!(json.get("from").is_none());
        assert!(matches!(round_trip(&request).command, Command::MissionFinish { mission: 3, .. }));
    }

    #[test]
    fn every_entry_kind_names_itself_the_way_it_serializes() {
        let kinds = [
            EntryKind::RunStarted,
            EntryKind::RunClosed,
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
