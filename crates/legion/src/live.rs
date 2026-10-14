//! Live views: redrawn every 2 seconds, or as new lines arrive, until q.
//! Without a terminal a view prints once and ends.

use std::{io::Write, time::Duration};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    terminal::{self, ClearType},
};

pub const REDRAW_EVERY: Duration = Duration::from_secs(2);
const FOOTER: &str = "q to quit · updates every 2 seconds";

/// Whether a key quits: q, Esc, or Ctrl-C (raw mode turns it into a key).
pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc) || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

/// Whether q was pressed within `wait`.
fn quit_within(wait: Duration) -> Result<bool, String> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if !event::poll(left).map_err(|error| error.to_string())? {
            return Ok(false);
        }
        if let Event::Key(key) = event::read().map_err(|error| error.to_string())? {
            if is_quit(&key) {
                return Ok(true);
            }
        }
    }
}

/// The terminal handed over to a live view; given back when dropped.
pub struct Live {
    /// A whole-screen view redraws in place; a scrolling one adds lines.
    is_whole_screen: bool,
}

impl Live {
    pub fn whole_screen() -> Result<Live, String> {
        terminal::enable_raw_mode().map_err(|error| error.to_string())?;
        execute!(std::io::stdout(), terminal::EnterAlternateScreen, cursor::Hide).map_err(|error| error.to_string())?;
        Ok(Live { is_whole_screen: true })
    }

    pub fn scrolling() -> Result<Live, String> {
        terminal::enable_raw_mode().map_err(|error| error.to_string())?;
        Ok(Live { is_whole_screen: false })
    }

    /// Draws a whole screen, then waits up to 2 seconds. False once q is pressed.
    pub fn show(&self, text: &str) -> Result<bool, String> {
        let mut out = std::io::stdout();
        queue!(out, cursor::MoveTo(0, 0), terminal::Clear(ClearType::All)).map_err(|error| error.to_string())?;
        write!(out, "{}\r\n\r\n{}", text.replace('\n', "\r\n"), console::style(FOOTER).dim()).map_err(|error| error.to_string())?;
        out.flush().map_err(|error| error.to_string())?;
        quit_within(REDRAW_EVERY).map(|is_quit| !is_quit)
    }

    /// Adds lines to a scrolling view.
    pub fn add(&self, text: &str) -> Result<(), String> {
        let mut out = std::io::stdout();
        write!(out, "{}\r\n", text.replace('\n', "\r\n")).map_err(|error| error.to_string())?;
        out.flush().map_err(|error| error.to_string())
    }

    /// Whether q has been pressed, without waiting.
    pub fn is_quitting(&self) -> Result<bool, String> {
        quit_within(Duration::ZERO)
    }
}

/// A whole-screen live view when `watch` is asked for and there's a terminal.
pub fn start(watch: bool, is_interactive: bool) -> Result<Option<Live>, String> {
    match watch && is_interactive {
        true => Live::whole_screen().map(Some),
        false => Ok(None),
    }
}

/// Shows a view: printed once without a live one, otherwise drawn and held
/// for 2 seconds. True while it should be drawn again.
pub fn present(live: Option<&Live>, text: &str) -> Result<bool, String> {
    match live {
        None => {
            println!("{text}");
            Ok(false)
        }
        Some(view) => view.show(text),
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if self.is_whole_screen {
            let _ = execute!(std::io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        }
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_esc_and_ctrl_c_quit() {
        let key = |code, modifiers| KeyEvent::new(code, modifiers);
        assert!(is_quit(&key(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(is_quit(&key(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(is_quit(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!is_quit(&key(KeyCode::Char('c'), KeyModifiers::NONE)));
    }
}
