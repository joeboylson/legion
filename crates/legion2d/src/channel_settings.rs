//! `channels.json` in Legion's data folder: the channel this machine hosts
//! and the ones it subscribes to, so they come back when legion2d restarts.

use std::path::Path;

use legion2_proto::{ChannelSwitch, DeploymentSwitches};
use serde::{Deserialize, Serialize};

use crate::constants::CHANNELS_FILE_NAME;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ChannelSettings {
    pub hosted: Option<HostedSettings>,
    pub subscriptions: Vec<SubscriptionSettings>,
    /// Only deployments with a switch changed from "ask".
    pub switches: Vec<DeploymentSwitches>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostedSettings {
    pub port: u16,
    pub key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionSettings {
    pub address: String,
    pub key: String,
}

impl ChannelSettings {
    /// A subscription to an address already subscribed to replaces it.
    pub fn with_subscription(&self, subscription: SubscriptionSettings) -> ChannelSettings {
        let others: Vec<SubscriptionSettings> = self.subscriptions.iter().filter(|existing| existing.address != subscription.address).cloned().collect();
        ChannelSettings { subscriptions: others.into_iter().chain([subscription]).collect(), ..self.clone() }
    }

    pub fn without_subscription(&self, address: &str) -> ChannelSettings {
        let kept = self.subscriptions.iter().filter(|existing| existing.address != address).cloned().collect();
        ChannelSettings { subscriptions: kept, ..self.clone() }
    }

    pub fn with_hosted(&self, hosted: Option<HostedSettings>) -> ChannelSettings {
        ChannelSettings { hosted, ..self.clone() }
    }

    pub fn switches_for(&self, deployment: &str) -> DeploymentSwitches {
        self.switches.iter().find(|known| known.deployment == deployment).cloned().unwrap_or(DeploymentSwitches {
            deployment: deployment.to_string(),
            send: ChannelSwitch::default(),
            receive: ChannelSwitch::default(),
        })
    }

    /// Changes the switches given and keeps the other; a deployment back on
    /// "ask" for both is dropped from the list.
    pub fn with_switches(&self, deployment: &str, send: Option<ChannelSwitch>, receive: Option<ChannelSwitch>) -> ChannelSettings {
        let current = self.switches_for(deployment);
        let changed = DeploymentSwitches { send: send.unwrap_or(current.send), receive: receive.unwrap_or(current.receive), ..current };
        let is_default = changed.send == ChannelSwitch::default() && changed.receive == ChannelSwitch::default();
        let others = self.switches.iter().filter(|known| known.deployment != deployment).cloned();
        ChannelSettings { switches: others.chain((!is_default).then_some(changed)).collect(), ..self.clone() }
    }
}

/// No file means no channels; a broken one is reported, not ignored.
pub fn read_channel_settings(data_folder: &Path) -> Result<ChannelSettings, String> {
    let path = data_folder.join(CHANNELS_FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display())),
        Err(_) => Ok(ChannelSettings::default()),
    }
}

pub fn write_channel_settings(data_folder: &Path, settings: &ChannelSettings) -> Result<(), String> {
    let path = data_folder.join(CHANNELS_FILE_NAME);
    let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| format!("can't write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(address: &str, key: &str) -> SubscriptionSettings {
        SubscriptionSettings { address: address.into(), key: key.into() }
    }

    #[test]
    fn subscribing_again_replaces_the_old_key() {
        let settings = ChannelSettings::default().with_subscription(subscription("a:1", "old")).with_subscription(subscription("a:1", "new"));
        assert_eq!(settings.subscriptions, vec![subscription("a:1", "new")]);
    }

    #[test]
    fn unsubscribing_keeps_the_rest_and_the_hosted_channel() {
        let hosted = Some(HostedSettings { port: 4620, key: "k".into() });
        let settings = ChannelSettings { hosted: hosted.clone(), subscriptions: vec![subscription("a:1", "x"), subscription("b:2", "y")], switches: vec![] };
        assert_eq!(settings.without_subscription("a:1"), ChannelSettings { hosted, subscriptions: vec![subscription("b:2", "y")], switches: vec![] });
    }

    #[test]
    fn switches_start_on_ask_and_change_one_at_a_time() {
        let settings = ChannelSettings::default();
        assert_eq!((settings.switches_for("d").send, settings.switches_for("d").receive), (ChannelSwitch::Ask, ChannelSwitch::Ask));
        let send_free = settings.with_switches("d", Some(ChannelSwitch::Free), None);
        assert_eq!((send_free.switches_for("d").send, send_free.switches_for("d").receive), (ChannelSwitch::Free, ChannelSwitch::Ask));
        let receive_off = send_free.with_switches("d", None, Some(ChannelSwitch::Off));
        assert_eq!((receive_off.switches_for("d").send, receive_off.switches_for("d").receive), (ChannelSwitch::Free, ChannelSwitch::Off));
        assert_eq!(receive_off.switches_for("other").send, ChannelSwitch::Ask);
        let back_to_ask = receive_off.with_switches("d", Some(ChannelSwitch::Ask), Some(ChannelSwitch::Ask));
        assert!(back_to_ask.switches.is_empty());
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let folder = std::env::temp_dir().join(format!("legion2-channels-{}", crate::ids::new_id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(read_channel_settings(&folder), Ok(ChannelSettings::default()));
        let settings = ChannelSettings::default().with_hosted(Some(HostedSettings { port: 4620, key: "k".into() })).with_subscription(subscription("a:1", "x"));
        write_channel_settings(&folder, &settings).unwrap();
        assert_eq!(read_channel_settings(&folder), Ok(settings));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
