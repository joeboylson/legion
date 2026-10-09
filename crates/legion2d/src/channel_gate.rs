//! The human's say over channel messages: setting a deployment's switches,
//! holding a message as a question, and acting on the answer.

use legion2_proto::{ChannelSwitch, EntryKind, NewEntry, Reply, HUMAN, LEGION};

use crate::{
    channel_delivery::arrival_text,
    channel_holds::{add_hold, is_approval, question_text, take_hold, Held, Hold},
    daemon::Daemon,
};

impl Daemon {
    pub fn set_channel_switches(&self, deployment_key: &str, send: Option<ChannelSwitch>, receive: Option<ChannelSwitch>) -> Result<Reply, String> {
        let deployment_id = self.state.lock().unwrap().find_deployment(deployment_key)?.1.id;
        let channels = self.channels.set_switches(&deployment_id, send, receive)?;
        Ok(Reply::Channels { channels })
    }

    /// Asks the human about a held message; it waits until they answer.
    pub fn hold_for_human(&self, deployment_id: &str, held: Held) -> Result<(), String> {
        let question = NewEntry { kind: EntryKind::Question, mission: None, to: Some(HUMAN.into()), text: question_text(&held), answers: None };
        let asked = self.post_entry(deployment_id, LEGION, question)?;
        add_hold(&self.config.data_folder, Hold { deployment: deployment_id.to_string(), question: asked.id, held })
    }

    /// An answer to a held message's question sends it on or turns it away.
    /// Any other answer is left alone.
    pub fn settle_channel_hold(&self, deployment_id: &str, question: i64, answer: &str) {
        let hold = match take_hold(&self.config.data_folder, deployment_id, question) {
            Ok(Some(hold)) => hold,
            Ok(None) => return,
            Err(error) => return eprintln!("{LEGION}d: {error}"),
        };
        let told = match (hold.held, is_approval(answer)) {
            (Held::Out { from, to, text }, true) => {
                let outcome = match self.channels.send(from, &to.key, &text) {
                    Ok(()) => "it's sent".to_string(),
                    Err(error) => format!("it didn't go: {error}"),
                };
                self.tell_commander(deployment_id, format!("The human approved your channel message to {} ({}); {outcome}.", to.name, to.key))
            }
            (Held::Out { to, .. }, false) => {
                self.tell_commander(deployment_id, format!("The human held back your channel message to {} ({}). Their answer: {answer}", to.name, to.key))
            }
            (Held::In { from, text }, true) => self.tell_commander(deployment_id, arrival_text(&from, &text)),
            (Held::In { from, .. }, false) => self.own_channel_deployment(deployment_id).and_then(|own| {
                let reason = format!("the human on {} didn't pass it on to {}'s commander. Their answer: {answer}", own.machine, own.name);
                self.channels.turn_back(&from.key, &own.key, &reason)
            }),
        };
        if let Err(error) = told {
            eprintln!("{LEGION}d: {error}");
        }
    }
}
