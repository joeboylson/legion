//! `session`: start, stop or watch a session, or press a key in one.

use legion2_proto::{Command, MissionStatus, Reply, COMMANDER};

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
/// The list's choice for starting with no mission.
const NO_MISSION: u32 = 0;

pub struct SessionInputs {
    pub operator: Option<String>,
    pub mission: Option<u32>,
    pub position: Option<String>,
    pub key: Option<String>,
    pub yes: bool,
}

pub async fn run(ctx: &mut Context, action: Option<SessionAction>, inputs: SessionInputs) -> Result<(), String> {
    let action = ctx.ask.action(action, "legion2 session")?;
    let deployment = ctx.deployment().await?;
    match action {
        SessionAction::Start => {
            let detail = ctx.folder().await?;
            let team = detail.pipelines.iter().find(|pipeline| pipeline.name == deployment.pipeline).map(|pipeline| pipeline.operators.clone()).unwrap_or_default();
            let operators = std::iter::once(item(COMMANDER.to_string(), COMMANDER, "leads the deployment")).chain(team.iter().map(|operator| item(operator.clone(), operator, ""))).collect();
            let operator = ctx.ask.pick(inputs.operator, "Start whom?", operators, "--operator", "")?;
            let Reply::Missions { missions } = ctx.send(Command::MissionList { deployment: deployment.id.clone() }).await? else { return Err("unexpected reply".into()) };
            let open = missions.iter().filter(|mission| mission.status != MissionStatus::Done).map(|mission| item(mission.number, format!("#{} {}", mission.number, mission.title), ""));
            let choices = std::iter::once(item(NO_MISSION, "No mission", "it waits for one")).chain(open).collect();
            let given = match (inputs.mission, ctx.ask.is_interactive() && operator != COMMANDER) {
                (Some(number), _) => Some(number),
                (None, true) => None,
                (None, false) => Some(NO_MISSION),
            };
            let mission = ctx.ask.pick(given, "On which mission?", choices, "--mission", "")?;
            let reply = ctx.send(Command::SessionStart { deployment: deployment.id, operator, mission: (mission != NO_MISSION).then_some(mission), part: None }).await?;
            show(&reply);
            Ok(())
        }
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
                let Reply::Screen { text, ansi } = ctx.send(Command::Screen { deployment: deployment.id.clone(), position: position.clone() }).await? else { return Err("unexpected reply".into()) };
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
