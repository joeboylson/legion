//! Between the channels and the deployments here: what this machine tells
//! the channels it has open, what commanders send and ask, and what arrives
//! for them. A channel message reaches a commander as a deployment log
//! message, the way a message from its own team does.

use std::sync::{Arc, Weak};

use legion2_proto::{ChannelDeployment, ChannelSwitch, EntryKind, NewEntry, Reply, COMMANDER};

use crate::{
    channel_holds::Held,
    channels::Inbound,
    daemon::Daemon,
    setup::read_pipeline,
};

/// What a commander hears when its message waits for the human.
const HELD_FOR_APPROVAL: &str = "Held for the human to approve. You'll get a message once it's sent or turned down; carry on meanwhile.";

/// A deployment's key on the channels: the machine's name and its ID.
pub fn channel_key(machine: &str, deployment_id: &str) -> String {
    format!("{machine}/{deployment_id}")
}

/// What a commander reads when a message arrives from another team.
pub fn arrival_text(from: &ChannelDeployment, text: &str) -> String {
    format!(
        "Message over the channel from {} ({} on {}, pipeline {}):\n{text}\n\nReply with channel_send to {}.",
        from.name, from.folder, from.machine, from.pipeline, from.key
    )
}

pub fn deployments_text(deployments: &[ChannelDeployment], own_key: &str) -> String {
    if deployments.iter().all(|deployment| deployment.key == own_key) {
        return "No other teams are on the channels. The human opens or subscribes to a channel to link Legions.".into();
    }
    deployments
        .iter()
        .filter(|deployment| deployment.key != own_key)
        .map(|deployment| {
            let description = deployment.description.as_deref().unwrap_or("(no description yet)");
            format!(
                "{}: {} ({} on {}), pipeline {}: {}. {description}",
                deployment.key,
                deployment.name,
                deployment.folder,
                deployment.machine,
                deployment.pipeline,
                deployment.operators.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Daemon {
    /// Hands the channels the way to reach the deployments here, and tells
    /// them which are open.
    pub fn connect_channels(self: &Arc<Self>) {
        let daemon: Weak<Daemon> = Arc::downgrade(self);
        self.channels.take_deliveries(Arc::new(move |inbound| daemon.upgrade().ok_or("Legion is stopping")?.take_from_channel(inbound)));
        self.refresh_channel_deployments();
    }

    /// Every open deployment here, as the channels see it.
    fn channel_deployments_here(&self) -> Vec<ChannelDeployment> {
        let state = self.state.lock().unwrap();
        state
            .folders
            .iter()
            .flat_map(|folder| {
                folder.deployments.iter().filter(|deployment| deployment.closed_ms.is_none()).map(move |deployment| {
                    let key = channel_key(self.channels.machine(), &deployment.id);
                    let operators = read_pipeline(&folder.path, &deployment.pipeline).map(|(pipeline, _)| pipeline.operators).unwrap_or_default();
                    ChannelDeployment {
                        description: self.channels.description(&key),
                        key,
                        machine: self.channels.machine().to_string(),
                        name: deployment.name.clone(),
                        folder: folder.name.clone(),
                        pipeline: deployment.pipeline.clone(),
                        operators,
                    }
                })
            })
            .collect()
    }

    /// A deployment here opened, closed or changed its description.
    pub fn refresh_channel_deployments(&self) {
        self.channels.set_local(self.channel_deployments_here());
    }

    pub fn own_channel_deployment(&self, deployment_key: &str) -> Result<ChannelDeployment, String> {
        let deployment_id = self.state.lock().unwrap().find_open_deployment(deployment_key)?.1.id;
        let key = channel_key(self.channels.machine(), &deployment_id);
        self.channel_deployments_here().into_iter().find(|deployment| deployment.key == key).ok_or_else(|| format!("deployment {deployment_key} isn't open"))
    }

    pub fn list_channel_deployments(&self, deployment_key: &str) -> Result<Reply, String> {
        let own = self.own_channel_deployment(deployment_key)?;
        Ok(Reply::Text { text: deployments_text(&self.channels.status().deployments, &own.key) })
    }

    pub fn describe_on_channels(&self, deployment_key: &str, text: &str) -> Result<Reply, String> {
        let own = self.own_channel_deployment(deployment_key)?;
        self.channels.describe(&own.key, text);
        self.refresh_channel_deployments();
        Ok(Reply::Done)
    }

    /// Goes straight out with the send switch on "free"; on "ask" it waits
    /// for the human.
    pub fn send_on_channels(&self, deployment_key: &str, to: &str, text: &str) -> Result<Reply, String> {
        let own = self.own_channel_deployment(deployment_key)?;
        let to = to.trim();
        if own.key == to {
            return Err("that's your own deployment".into());
        }
        let deployment_id = self.local_deployment_id(&own.key)?;
        match self.channels.switches(&deployment_id).send {
            ChannelSwitch::Off => Err("the human has turned sending off for this deployment: it can't message other teams".into()),
            ChannelSwitch::Free => self.channels.send(own, to, text).map(|_| Reply::Done),
            ChannelSwitch::Ask => {
                let target = self.channels.reaches(to).ok_or_else(|| format!("no open deployment {to} on the channels; channel_deployments lists who's there"))?;
                self.hold_for_human(&deployment_id, Held::Out { from: own, to: target, text: text.to_string() })?;
                Ok(Reply::Text { text: HELD_FOR_APPROVAL.into() })
            }
        }
    }

    /// The ID of a deployment on this machine, from its channel key.
    pub fn local_deployment_id(&self, key: &str) -> Result<String, String> {
        key.strip_prefix(&format!("{}/", self.channels.machine())).map(str::to_string).ok_or_else(|| format!("{key} isn't on this machine"))
    }

    /// Something from the channels for a deployment here. A message waits
    /// for the human with the receive switch on "ask"; on "off" it's turned
    /// away and its sender told.
    fn take_from_channel(&self, inbound: Inbound) -> Result<(), String> {
        let (key, text) = match inbound {
            Inbound::Message { to, from, text } => {
                let deployment_id = self.local_deployment_id(&to)?;
                self.state.lock().unwrap().find_open_deployment(&deployment_id)?;
                match self.channels.switches(&deployment_id).receive {
                    ChannelSwitch::Off => return Err("this team has turned off messages from other teams".into()),
                    ChannelSwitch::Ask => return self.hold_for_human(&deployment_id, Held::In { from, text }),
                    ChannelSwitch::Free => (to, arrival_text(&from, &text)),
                }
            }
            Inbound::Undelivered { from, to, reason } => (from, format!("Your channel message to {to} didn't get through: {reason}")),
            Inbound::Gone { here, gone } => {
                (here, format!("{} ({}), a team you've been talking to over the channel, is gone: its deployment closed or its Legion went away. Messages to it won't get through.", gone.name, gone.key))
            }
        };
        let deployment_id = self.local_deployment_id(&key)?;
        self.tell_commander(&deployment_id, text)
    }

    pub fn tell_commander(&self, deployment_id: &str, text: String) -> Result<(), String> {
        self.state.lock().unwrap().find_open_deployment(deployment_id)?;
        let entry = NewEntry { kind: EntryKind::Message, mission: None, to: Some(COMMANDER.into()), text, answers: None };
        self.post_entry(deployment_id, legion2_proto::LEGION, entry).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment(key: &str, description: Option<&str>) -> ChannelDeployment {
        ChannelDeployment {
            key: key.into(),
            machine: "m".into(),
            name: "feature".into(),
            folder: "app".into(),
            pipeline: "feature".into(),
            operators: vec!["planner".into(), "builder".into()],
            description: description.map(String::from),
        }
    }

    #[test]
    fn the_list_leaves_out_your_own_team() {
        let text = deployments_text(&[deployment("m/a", None), deployment("m/b", Some("Builds the to-do app."))], "m/a");
        assert!(!text.contains("m/a"));
        assert!(text.contains("m/b") && text.contains("Builds the to-do app.") && text.contains("planner, builder"));
    }

    #[test]
    fn alone_on_the_channels_says_so() {
        assert!(deployments_text(&[deployment("m/a", None)], "m/a").starts_with("No other teams"));
    }

    #[test]
    fn an_arrival_says_who_from_and_how_to_reply() {
        let text = arrival_text(&deployment("m/b", None), "hello");
        assert!(text.contains("hello") && text.contains("channel_send to m/b"));
    }
}
