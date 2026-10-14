//! `session`: stop or watch a session, or press a key in one. Sessions are
//! started by their commander, never by hand.

use legion2_proto::{Command, Reply};

use crate::{
    arguments::SessionAction,
    ask::item,
    commands::{
        show,
        talk::{running, session_items},
    },
    context::Context,
    live::{present, start},
};

const KEYS: &[(&str, &str)] = &[("enter", "Enter"), ("esc", "Esc"), ("up", "Up arrow"), ("down", "Down arrow"), ("tab", "Tab")];

pub struct SessionInputs {
    pub position: Option<String>,
    pub key: Option<String>,
    pub yes: bool,
}

pub async fn run(ctx: &mut Context, action: Option<SessionAction>, inputs: SessionInputs) -> Result<(), String> {
    let action = ctx.ask.action(action, "legion2 session")?;
    let deployment = ctx.deployment().await?;
    match action {
        SessionAction::Stop => {
            let sessions = running(ctx, &deployment.id).await?;
            let position = ctx.ask.pick(inputs.position, "Stop which session?", session_items(&sessions), "--position", "nobody is running")?;
            if !ctx.ask.confirm(inputs.yes, &format!("Stop {position}?"))? {
                return Ok(());
            }
            show(&ctx.send(Command::SessionStop { deployment: deployment.id, position }).await?);
            Ok(())
        }
        SessionAction::Watch => {
            let sessions = running(ctx, &deployment.id).await?;
            let position = ctx.ask.pick(inputs.position, "Watch which session?", session_items(&sessions), "--position", "nobody is running")?;
            let live = start(true, ctx.ask.is_interactive())?;
            loop {
                let Reply::Screen { text, ansi, .. } = ctx.send(Command::Screen { deployment: deployment.id.clone(), position: position.clone() }).await? else { return Err("unexpected reply".into()) };
                if !present(live.as_ref(), if live.is_some() { &ansi } else { &text })? {
                    return Ok(());
                }
            }
        }
        SessionAction::Key => {
            let sessions = running(ctx, &deployment.id).await?;
            let position = ctx.ask.pick(inputs.position, "Which session?", session_items(&sessions), "--position", "nobody is running")?;
            let keys = KEYS.iter().map(|(key, label)| item(key.to_string(), label, "")).collect();
            let key = ctx.ask.pick(inputs.key, "Press which key?", keys, "--key", "")?;
            show(&ctx.send(Command::Key { deployment: deployment.id, position, key }).await?);
            Ok(())
        }
    }
}
