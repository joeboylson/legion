//! `channel`: see the channels, open or close one, subscribe, and set a
//! deployment's send and receive switches.

use legion2_proto::{ChannelSwitch, Command, Reply, DEFAULT_CHANNEL_PORT};

use crate::{
    arguments::ChannelAction,
    ask::item,
    commands::show,
    context::Context,
    live::{present, start},
    reply_format::reply_text,
};

pub struct ChannelInputs {
    pub port: Option<u16>,
    pub key: Option<String>,
    pub address: Option<String>,
    pub send: Option<String>,
    pub receive: Option<String>,
    pub watch: bool,
    pub yes: bool,
}

const SWITCHES: &[(ChannelSwitch, &str)] = &[
    (ChannelSwitch::Off, "never"),
    (ChannelSwitch::Ask, "ask me each time, in answer"),
    (ChannelSwitch::Free, "always, without asking"),
];

pub fn parse_switch(word: &str) -> Result<ChannelSwitch, String> {
    SWITCHES.iter().map(|(switch, _)| *switch).find(|switch| switch.as_str() == word).ok_or_else(|| format!("{word:?}: use off, ask or free"))
}

/// One switch: the flag's when given; asked, starting from where it is, in
/// a terminal; otherwise left as it is.
fn pick_switch(ctx: &Context, given: Option<String>, prompt: &str, current: ChannelSwitch) -> Result<Option<ChannelSwitch>, String> {
    match (given, ctx.ask.is_interactive()) {
        (Some(word), _) => parse_switch(&word).map(Some),
        (None, false) => Ok(None),
        (None, true) => {
            let items = SWITCHES.iter().map(|(switch, hint)| item(*switch, switch.as_str(), if *switch == current { format!("{hint} (now)") } else { hint.to_string() })).collect();
            ctx.ask.pick(None, prompt, items, "", "").map(Some)
        }
    }
}

pub async fn run(ctx: &mut Context, action: Option<ChannelAction>, inputs: ChannelInputs) -> Result<(), String> {
    match action.unwrap_or(ChannelAction::Status) {
        ChannelAction::Status => {
            show(&ctx.send(Command::ChannelStatus).await?);
            Ok(())
        }
        ChannelAction::Open => {
            let port = ctx.ask.text_or_default(inputs.port.map(|port| port.to_string()), "Which port?", &DEFAULT_CHANNEL_PORT.to_string())?;
            let port: u16 = port.trim().parse().map_err(|_| format!("{port:?} isn't a port"))?;
            let key = ctx.ask.optional_text(inputs.key, "The key subscribers give (blank makes one)", "")?;
            show(&ctx.send(Command::ChannelOpen { port, key }).await?);
            Ok(())
        }
        ChannelAction::Close => {
            if !ctx.ask.confirm(inputs.yes, "Stop hosting the channel? Its subscribers are cut off.")? {
                return Ok(());
            }
            show(&ctx.send(Command::ChannelClose).await?);
            Ok(())
        }
        ChannelAction::Subscribe => {
            let address = ctx.ask.text(inputs.address, "The channel's address", "spark.tailb50373.ts.net:4620", "--address")?;
            let key = ctx.ask.text(inputs.key, "Its key", "", "--key")?;
            show(&ctx.send(Command::ChannelSubscribe { address, key }).await?);
            Ok(())
        }
        ChannelAction::Unsubscribe => {
            let Reply::Channels { channels } = ctx.send(Command::ChannelStatus).await? else { return Err("unexpected reply".into()) };
            let items = channels.subscriptions.iter().map(|subscription| item(subscription.address.clone(), &subscription.address, if subscription.is_up { "up" } else { "down" })).collect();
            let address = ctx.ask.pick(inputs.address, "Unsubscribe from which?", items, "--address", "not subscribed to any channel")?;
            show(&ctx.send(Command::ChannelUnsubscribe { address }).await?);
            Ok(())
        }
        ChannelAction::Switch => {
            let deployment = ctx.deployment().await?;
            let Reply::Channels { channels } = ctx.send(Command::ChannelStatus).await? else { return Err("unexpected reply".into()) };
            let current = channels.switches.iter().find(|set| set.deployment == deployment.id);
            let send = pick_switch(ctx, inputs.send, &format!("Can {}'s commander message other teams?", deployment.name), current.map_or(ChannelSwitch::Ask, |set| set.send))?;
            let receive = pick_switch(ctx, inputs.receive, "Do other teams' messages reach it?", current.map_or(ChannelSwitch::Ask, |set| set.receive))?;
            if send.is_none() && receive.is_none() {
                return Err("say what to set: --send or --receive, each off, ask or free".into());
            }
            show(&ctx.send(Command::ChannelSwitch { deployment: deployment.id, send, receive }).await?);
            Ok(())
        }
        ChannelAction::Log => {
            let live = start(inputs.watch, ctx.ask.is_interactive())?;
            loop {
                let text = reply_text(&ctx.send(Command::ChannelLog { limit: 50 }).await?);
                if !present(live.as_ref(), &text)? {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_are_off_ask_or_free() {
        assert_eq!(parse_switch("free"), Ok(ChannelSwitch::Free));
        assert!(parse_switch("maybe").is_err());
    }
}
