//! What two Legions say to each other over a channel: one JSON message per
//! line. The subscriber opens with its key; the host lets it in or turns it
//! away; then the subscriber checks in and the host answers each time. Each
//! end says which deployments it has open (the host: every one it knows),
//! and messages between deployments' commanders pass through the host.

use legion2_proto::ChannelDeployment;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireMessage {
    Hello { key: String, machine: String },
    Welcome { machine: String },
    Refused { reason: String },
    Ping,
    Pong,
    /// The deployments open at this end: a subscriber's own, or every one
    /// the host can reach.
    Deployments { deployments: Vec<ChannelDeployment> },
    /// A message for the commander of the deployment `to` (a key).
    Relay { from: ChannelDeployment, to: String, text: String },
    /// A message that didn't get through, on its way back to the sender.
    Undeliverable { from: String, to: String, text: String, reason: String },
}

pub const WRONG_KEY: &str = "wrong key";

/// One message as one line, ending in a newline.
pub fn encode(message: &WireMessage) -> String {
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    line
}

pub fn decode(line: &str) -> Result<WireMessage, String> {
    serde_json::from_str(line.trim_end()).map_err(|error| format!("not a channel message: {error}"))
}

/// The host's answer to a hello: let it in with this machine's name, or say why not.
pub fn answer_hello(message: &WireMessage, expected_key: &str, host_machine: &str) -> WireMessage {
    match message {
        WireMessage::Hello { key, .. } if key == expected_key => WireMessage::Welcome { machine: host_machine.into() },
        WireMessage::Hello { .. } => WireMessage::Refused { reason: WRONG_KEY.into() },
        _ => WireMessage::Refused { reason: "say hello first".into() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_survive_a_round_trip_as_one_line() {
        let hello = WireMessage::Hello { key: "k".into(), machine: "laptop".into() };
        let line = encode(&hello);
        assert!(line.ends_with('\n') && line.matches('\n').count() == 1);
        assert_eq!(decode(&line), Ok(hello));
        assert_eq!(decode(&encode(&WireMessage::Ping)), Ok(WireMessage::Ping));
    }

    #[test]
    fn the_host_lets_in_only_the_right_key() {
        let hello = |key: &str| WireMessage::Hello { key: key.into(), machine: "laptop".into() };
        assert_eq!(answer_hello(&hello("k"), "k", "host"), WireMessage::Welcome { machine: "host".into() });
        assert_eq!(answer_hello(&hello("nope"), "k", "host"), WireMessage::Refused { reason: WRONG_KEY.into() });
        assert!(matches!(answer_hello(&WireMessage::Ping, "k", "host"), WireMessage::Refused { .. }));
    }

    #[test]
    fn garbage_is_not_a_message() {
        assert!(decode("hello there").is_err());
    }
}
