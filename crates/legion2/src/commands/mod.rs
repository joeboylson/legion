//! One module per command.

pub mod channel;
pub mod deploy;
pub mod log;
pub mod mission;
pub mod operator;
pub mod pipeline;
pub mod session;
pub mod setup;
pub mod status;
pub mod talk;

use std::io::IsTerminal;

use legion2_proto::Reply;

use crate::{pretty::reply_display, reply_format::reply_text};

/// Prints a reply: tables and color in a terminal, plain text otherwise.
pub fn show(reply: &Reply) {
    let shown = std::io::stdout().is_terminal().then(|| reply_display(reply)).flatten();
    let text = shown.unwrap_or_else(|| reply_text(reply));
    if !text.is_empty() {
        println!("{text}");
    }
}

/// Opens a file in $VISUAL or $EDITOR, vi without either.
pub fn open_in_editor(path: &std::path::Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("{} doesn't exist", path.display()));
    }
    let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".into());
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .status()
        .map_err(|error| format!("{editor}: {error}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("{editor} exited with {status}")),
    }
}
