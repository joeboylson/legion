//! Values only legion2d uses. Values its clients share live in legion2-proto.

use std::time::Duration;

use legion2_proto::NAME;

/// The first Claude Code with add-ons.
pub const MINIMUM_CLAUDE_VERSION: ClaudeVersion = ClaudeVersion { major: 2, minor: 1, patch: 287 };

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClaudeVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// Add-on versions this program understands. The add-on owns its own copy
/// (ADDON_VERSION in addon/hooks/register.ts): it's TypeScript, so it can't
/// import this.
pub const KNOWN_ADDON_VERSIONS: &[&str] = &["1"];

/// The add-on reads these (the names are spelled out in register.ts too).
pub const ADDON_SOCKET_ENV: &str = "LEGION_ADDON_SOCKET";
pub const ADDON_SESSION_ENV: &str = "LEGION_ADDON_SESSION";

/// Started from inside a Claude session, these mark the new one as its
/// child, which turns off saving its transcript (and so resuming it).
pub const CHILD_SESSION_MARKERS: &[&str] = &["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_ENTRYPOINT"];

/// Skills turned on in claude.ai stay out of Legion's sessions.
pub const SESSION_SETTINGS_JSON: &str = r#"{"syncClaudeAiSkills": false}"#;

pub const TERMINAL_TYPE: &str = "xterm-256color";
pub const TERMINAL_ROWS: u16 = 40;
pub const TERMINAL_COLUMNS: u16 = 120;
pub const TERMINAL_READ_BUFFER_BYTES: usize = 8192;

pub const DEFAULT_MAX_BUSY_SESSIONS: usize = 6;
pub const DEFAULT_PERMISSION_TIMEOUT_MINUTES: u64 = 30;
pub const WAITING_SESSIONS_CHECK_INTERVAL: Duration = Duration::from_secs(15);
/// A session whose add-on hasn't reported by now is likely stuck on a
/// question shown before the add-on loads, such as whether to trust the folder.
pub const SESSION_START_GRACE: Duration = Duration::from_secs(60);
pub const MAX_WORKAROUND_ATTEMPTS: u32 = 3;
pub const DEFAULT_OPERATOR_COPY_LIMIT: u32 = 1;
/// A commander that ends by itself again this soon after a restart is left down.
pub const COMMANDER_RESTART_GAP_MS: i64 = 10 * 60 * 1000;
pub const RECENT_POSTMORTEM_COUNT: usize = 3;
pub const EVENT_BUFFER_SIZE: usize = 1024;

pub const MISSION_SLUG_MAX_LENGTH: usize = 40;
pub const MISSION_BRANCH_PREFIX: &str = "mission/";
pub const CHECK_OUTPUT_TAIL_LINES: usize = 15;

pub const FOLDERS_FOLDER_NAME: &str = "folders";
pub const MISSIONS_FOLDER_NAME: &str = "missions";
pub const WORKTREES_FOLDER_NAME: &str = "worktrees";
pub const DATABASE_FILE_NAME: &str = "legion.db";
pub const FOLDER_REGISTRY_FILE_NAME: &str = "folders.json";
pub const MACHINE_SETTINGS_FILE_NAME: &str = "settings.json";

pub const ADDON_STATE_ROUTE: &str = "/sessions/{id}/state";
pub const ADDON_INBOX_ROUTE: &str = "/sessions/{id}/inbox";
pub const WEBSOCKET_ROUTE: &str = "/ws";

/// Every message legion2d hands a session starts with this.
pub fn delivery_prefix() -> String {
    format!("[{NAME}]")
}

/// Lets a session run the legion2 command without asking each time.
pub fn legion_command_tool_rule() -> String {
    format!("Bash({NAME}:*)")
}
