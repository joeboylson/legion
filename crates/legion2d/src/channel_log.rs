//! `channel-log.jsonl` in Legion's data folder: everything the channels
//! carried and saw on this machine, one entry per line, oldest first. It
//! outlives restarts, so the app can show the whole story.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use legion2_proto::{ChannelLogEntry, ChannelLogKind};

use crate::{constants::CHANNEL_LOG_FILE_NAME, store::now_ms};

pub struct ChannelLog {
    path: PathBuf,
    next_id: Mutex<u64>,
}

/// What one entry says, before it's given its number and time.
pub struct LogLine {
    pub kind: ChannelLogKind,
    pub machine: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub text: String,
}

impl LogLine {
    pub fn new(kind: ChannelLogKind, text: impl Into<String>) -> LogLine {
        LogLine { kind, machine: None, from: None, to: None, text: text.into() }
    }

    pub fn machine(self, machine: impl Into<String>) -> LogLine {
        LogLine { machine: Some(machine.into()), ..self }
    }

    pub fn between(self, from: impl Into<String>, to: impl Into<String>) -> LogLine {
        LogLine { from: Some(from.into()), to: Some(to.into()), ..self }
    }
}

/// Lines that don't read as entries are skipped: a half-written last line
/// after a crash shouldn't hide the rest.
fn parse_entries(text: &str) -> Vec<ChannelLogEntry> {
    text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

fn read_entries(path: &Path) -> Vec<ChannelLogEntry> {
    std::fs::read_to_string(path).map(|text| parse_entries(&text)).unwrap_or_default()
}

impl ChannelLog {
    pub fn open(data_folder: &Path) -> ChannelLog {
        let path = data_folder.join(CHANNEL_LOG_FILE_NAME);
        let next_id = read_entries(&path).last().map(|entry| entry.id + 1).unwrap_or(1);
        ChannelLog { path, next_id: Mutex::new(next_id) }
    }

    /// Adds an entry; the entry as kept, or why it couldn't be.
    pub fn record(&self, line: LogLine) -> Result<ChannelLogEntry, String> {
        let mut next_id = self.next_id.lock().unwrap();
        let entry = ChannelLogEntry { id: *next_id, at_ms: now_ms(), kind: line.kind, machine: line.machine, from: line.from, to: line.to, text: line.text };
        let mut text = serde_json::to_string(&entry).map_err(|error| error.to_string())?;
        text.push('\n');
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|error| format!("can't write {}: {error}", self.path.display()))?;
        *next_id += 1;
        Ok(entry)
    }

    /// The latest `limit` entries, oldest first.
    pub fn latest(&self, limit: usize) -> Vec<ChannelLogEntry> {
        let entries = read_entries(&self.path);
        let skipped = entries.len().saturating_sub(limit);
        entries.into_iter().skip(skipped).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("legion2-channel-log-{}", crate::ids::new_id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn entries_are_numbered_kept_and_read_back_newest_last() {
        let folder = temp_folder();
        let log = ChannelLog::open(&folder);
        log.record(LogLine::new(ChannelLogKind::ChannelOpened, "hosting on port 4620")).unwrap();
        let sent = log.record(LogLine::new(ChannelLogKind::Sent, "hello").machine("laptop").between("m/a", "laptop/b")).unwrap();
        assert_eq!(sent.id, 2);
        assert_eq!(sent.from.as_deref(), Some("m/a"));
        assert_eq!(log.latest(10).iter().map(|entry| entry.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(log.latest(1).iter().map(|entry| entry.id).collect::<Vec<_>>(), vec![2]);

        // A restart carries on numbering after the last entry.
        let reopened = ChannelLog::open(&folder);
        assert_eq!(reopened.record(LogLine::new(ChannelLogKind::ChannelClosed, "closed")).unwrap().id, 3);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_broken_line_is_skipped() {
        let entries = parse_entries("{\"not\": \"an entry\"}\n{\"id\":4,\"at_ms\":1,\"kind\":\"sent\",\"machine\":null,\"from\":null,\"to\":null,\"text\":\"hi\"}\n{\"id\":5,");
        assert_eq!(entries.iter().map(|entry| entry.id).collect::<Vec<_>>(), vec![4]);
    }
}
