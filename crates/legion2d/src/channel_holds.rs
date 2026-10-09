//! Channel messages a switch on "ask" holds for the human: each waits as a
//! question in its deployment's log until the human answers it. Kept in
//! `channel-holds.json` so a restart doesn't lose them.

use std::{path::Path, sync::Mutex};

use legion2_proto::ChannelDeployment;
use serde::{Deserialize, Serialize};

use crate::constants::CHANNEL_HOLDS_FILE_NAME;

/// Answers that let a held message through. Anything else turns it away.
const APPROVING_ANSWERS: &[&str] = &["yes", "y", "ok", "okay", "send", "pass", "pass it on", "approve", "approved", "allow"];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", tag = "direction")]
pub enum Held {
    /// A commander here wants to send it to another team.
    Out { from: ChannelDeployment, to: ChannelDeployment, text: String },
    /// Another team sent it to a commander here.
    In { from: ChannelDeployment, text: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hold {
    pub deployment: String,
    /// The question entry the human answers.
    pub question: i64,
    pub held: Held,
}

/// Every read-change-write of the file goes through this, so two answers at
/// once can't lose a hold.
static HOLDS_FILE: Mutex<()> = Mutex::new(());

/// What the human reads in Escalations.
pub fn question_text(held: &Held) -> String {
    match held {
        Held::Out { to, text, .. } => format!(
            "The commander wants to send this to {} ({} on {}) over the channel:\n\n{text}\n\nAnswer yes to send it. Any other answer holds it back, and the commander reads your answer.",
            to.name, to.folder, to.machine
        ),
        Held::In { from, text } => format!(
            "{} ({} on {}) sent this to the commander over the channel:\n\n{text}\n\nAnswer yes to pass it on. Any other answer turns it away, and the sender reads your answer.",
            from.name, from.folder, from.machine
        ),
    }
}

pub fn is_approval(answer: &str) -> bool {
    let answer = answer.trim().trim_end_matches(['.', '!']).to_lowercase();
    APPROVING_ANSWERS.contains(&answer.as_str())
}

fn read_holds(data_folder: &Path) -> Result<Vec<Hold>, String> {
    let path = data_folder.join(CHANNEL_HOLDS_FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display())),
        Err(_) => Ok(Vec::new()),
    }
}

fn write_holds(data_folder: &Path, holds: &[Hold]) -> Result<(), String> {
    let path = data_folder.join(CHANNEL_HOLDS_FILE_NAME);
    let text = serde_json::to_string_pretty(holds).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| format!("can't write {}: {error}", path.display()))
}

pub fn add_hold(data_folder: &Path, hold: Hold) -> Result<(), String> {
    let _writing = HOLDS_FILE.lock().unwrap();
    let holds = read_holds(data_folder)?;
    write_holds(data_folder, &holds.into_iter().chain([hold]).collect::<Vec<_>>())
}

/// The hold a question stands for, taken off the list; none when the
/// question isn't a held channel message.
pub fn take_hold(data_folder: &Path, deployment: &str, question: i64) -> Result<Option<Hold>, String> {
    let _writing = HOLDS_FILE.lock().unwrap();
    let (taken, kept): (Vec<Hold>, Vec<Hold>) = read_holds(data_folder)?.into_iter().partition(|hold| hold.deployment == deployment && hold.question == question);
    if taken.is_empty() {
        return Ok(None);
    }
    write_holds(data_folder, &kept)?;
    Ok(taken.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(key: &str) -> ChannelDeployment {
        ChannelDeployment { key: key.into(), machine: "spark".into(), name: "docs".into(), folder: "hello-docs".into(), pipeline: "docs".into(), operators: vec![], description: None }
    }

    #[test]
    fn only_a_plain_yes_lets_a_message_through() {
        assert!(["yes", "Yes.", " ok ", "send", "pass it on", "Approve!"].iter().all(|answer| is_approval(answer)));
        assert!(["no", "not yet", "yes but shorten it", ""].iter().all(|answer| !is_approval(answer)));
    }

    #[test]
    fn the_question_says_who_and_what() {
        let out = question_text(&Held::Out { from: team("m/a"), to: team("spark/b"), text: "field names?".into() });
        assert!(out.contains("send this to docs (hello-docs on spark)") && out.contains("field names?") && out.contains("Answer yes to send it"));
        let incoming = question_text(&Held::In { from: team("spark/b"), text: "here they are".into() });
        assert!(incoming.contains("docs (hello-docs on spark) sent this") && incoming.contains("Answer yes to pass it on"));
    }

    #[test]
    fn a_hold_is_taken_once_and_the_rest_stay() {
        let folder = std::env::temp_dir().join(format!("legion2-holds-{}", crate::ids::new_id()));
        std::fs::create_dir_all(&folder).unwrap();
        let hold = |question| Hold { deployment: "d".into(), question, held: Held::In { from: team("spark/b"), text: "hi".into() } };
        add_hold(&folder, hold(4)).unwrap();
        add_hold(&folder, hold(7)).unwrap();
        assert_eq!(take_hold(&folder, "d", 4).unwrap(), Some(hold(4)));
        assert_eq!(take_hold(&folder, "d", 4).unwrap(), None);
        assert_eq!(take_hold(&folder, "other", 7).unwrap(), None);
        assert_eq!(take_hold(&folder, "d", 7).unwrap(), Some(hold(7)));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
