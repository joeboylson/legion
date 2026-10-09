//! Keys the admin can press in a session's terminal from outside it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKey {
    Enter,
    Escape,
    Up,
    Down,
    Tab,
}

const KEY_NAMES: &[(&str, TerminalKey)] = &[
    ("enter", TerminalKey::Enter),
    ("esc", TerminalKey::Escape),
    ("up", TerminalKey::Up),
    ("down", TerminalKey::Down),
    ("tab", TerminalKey::Tab),
];

impl TerminalKey {
    pub fn from_name(name: &str) -> Result<TerminalKey, String> {
        KEY_NAMES.iter().find(|(key_name, _)| *key_name == name).map(|(_, key)| *key).ok_or_else(|| {
            let known_names = KEY_NAMES.iter().map(|(key_name, _)| *key_name).collect::<Vec<_>>().join(", ");
            format!("no key {name:?}; keys are {known_names}")
        })
    }

    /// What the key sends down the terminal.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            TerminalKey::Enter => b"\r",
            TerminalKey::Escape => b"\x1b",
            TerminalKey::Up => b"\x1b[A",
            TerminalKey::Down => b"\x1b[B",
            TerminalKey::Tab => b"\t",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_resolves() {
        for (name, key) in KEY_NAMES {
            assert_eq!(TerminalKey::from_name(name), Ok(*key));
        }
    }

    #[test]
    fn unknown_names_list_the_known_ones() {
        let error = TerminalKey::from_name("space").unwrap_err();
        assert!(error.contains("enter, esc, up, down, tab"), "{error}");
    }

    #[test]
    fn escape_sends_escape() {
        assert_eq!(TerminalKey::Escape.bytes(), b"\x1b");
    }
}
