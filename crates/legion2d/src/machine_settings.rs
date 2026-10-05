//! `~/.local/share/<NAME>/settings.json`: settings for this machine.

use std::path::Path;

use serde::Deserialize;

use crate::constants::{DEFAULT_MAX_BUSY_SESSIONS, DEFAULT_PERMISSION_TIMEOUT_MINUTES, MACHINE_SETTINGS_FILE_NAME};

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct MachineSettings {
    pub max_busy_sessions: usize,
    pub permission_timeout_minutes: u64,
}

impl Default for MachineSettings {
    fn default() -> Self {
        MachineSettings { max_busy_sessions: DEFAULT_MAX_BUSY_SESSIONS, permission_timeout_minutes: DEFAULT_PERMISSION_TIMEOUT_MINUTES }
    }
}

pub fn parse_machine_settings(text: &str) -> Result<MachineSettings, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

/// No file means the defaults.
pub fn read_machine_settings(data_folder: &Path) -> Result<MachineSettings, String> {
    let path = data_folder.join(MACHINE_SETTINGS_FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_machine_settings(&text).map_err(|error| format!("{}: {error}", path.display())),
        Err(_) => Ok(MachineSettings::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_values_take_the_defaults() {
        assert_eq!(parse_machine_settings("{}"), Ok(MachineSettings::default()));
        let partial = parse_machine_settings(r#"{"maxBusySessions": 2}"#).unwrap();
        assert_eq!(partial.max_busy_sessions, 2);
        assert_eq!(partial.permission_timeout_minutes, DEFAULT_PERMISSION_TIMEOUT_MINUTES);
    }

    #[test]
    fn unknown_settings_are_refused() {
        assert!(parse_machine_settings(r#"{"maxBusy": 2}"#).is_err());
    }
}
