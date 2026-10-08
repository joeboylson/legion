//! Telling when a key sent to a session answers its permission question.
//!
//! Claude reports a question as it appears, but not its answer: only the end
//! of the tool call, which for a slow tool comes long after. Every answer a
//! person gives goes through legion2d (the app's buttons, its terminal), so
//! legion2d marks the session busy as it passes one on.

/// Enter picks the highlighted choice, Esc refuses, and a digit picks that
/// choice. Anything else (an arrow, text) only moves or types.
pub fn answers_permission_question(bytes: &[u8]) -> bool {
    matches!(bytes, b"\r" | b"\x1b" | [b'1'..=b'9'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_key::TerminalKey;

    #[test]
    fn enter_esc_and_a_digit_answer() {
        assert!(answers_permission_question(TerminalKey::Enter.bytes()));
        assert!(answers_permission_question(TerminalKey::Escape.bytes()));
        assert!(answers_permission_question(b"1"));
        assert!(answers_permission_question(b"3"));
    }

    #[test]
    fn moving_or_typing_does_not() {
        assert!(!answers_permission_question(TerminalKey::Down.bytes()));
        assert!(!answers_permission_question(TerminalKey::Up.bytes()));
        assert!(!answers_permission_question(b"0"));
        assert!(!answers_permission_question(b"12"));
        assert!(!answers_permission_question(b"yes"));
    }
}
