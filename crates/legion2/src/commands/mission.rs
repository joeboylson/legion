//! `mission`: add, list, read, pause, resume or finish missions.

use legion2_proto::{Command, Deployment, EntryKind, LogFilter, Mission, MissionStatus, NewEntry, Reply, NAME};

use crate::{
    arguments::MissionAction,
    ask::{item, Item},
    commands::{deploy, show},
    context::{open_deployments, Context},
    live::{present, start},
    pretty::{mission_view, missions_table},
};

pub struct MissionInputs {
    pub title: Option<String>,
    pub text: Option<String>,
    pub file: Option<String>,
    pub number: Option<u32>,
    pub watch: bool,
}

/// The deployment a new mission goes to. With none running and a person
/// at the keyboard, it helps start one.
async fn deployment_for_new_mission(ctx: &mut Context, has_named_deployment: bool) -> Result<Deployment, String> {
    if has_named_deployment {
        return ctx.deployment().await;
    }
    let detail = ctx.folder().await?;
    if !open_deployments(&detail).is_empty() || !ctx.ask.is_interactive() {
        return ctx.deployment().await;
    }
    ctx.ask.intro(NAME);
    ctx.ask.note("No deployment is running here yet. Let's start one for this mission.");
    deploy::start(ctx, deploy::DeployInputs::none()).await
}

async fn missions(ctx: &mut Context, deployment: &Deployment) -> Result<Vec<Mission>, String> {
    match ctx.send(Command::MissionList { deployment: deployment.id.clone() }).await? {
        Reply::Missions { missions } => Ok(missions),
        other => Err(format!("unexpected reply: {other:?}")),
    }
}

fn mission_items(missions: &[Mission], wanted: impl Fn(&Mission) -> bool) -> Vec<Item<u32>> {
    missions.iter().filter(|mission| wanted(mission)).map(|mission| item(mission.number, format!("#{} {}", mission.number, mission.title), format!("{:?}", mission.status).to_lowercase())).collect()
}

async fn post(ctx: &mut Context, deployment: &Deployment, kind: EntryKind, mission: u32, text: String) -> Result<(), String> {
    let entry = NewEntry { kind, mission: Some(mission), to: None, text, answers: None };
    show(&ctx.send(Command::Post { deployment: deployment.id.clone(), entry }).await?);
    Ok(())
}

pub async fn run(ctx: &mut Context, action: Option<MissionAction>, inputs: MissionInputs, has_named_deployment: bool) -> Result<(), String> {
    let action = ctx.ask.action(action, "legion2 mission")?;
    if action == MissionAction::Add {
        let deployment = deployment_for_new_mission(ctx, has_named_deployment).await?;
        let title = ctx.ask.text(inputs.title, "Mission title", "Autumn rain haiku", "--title")?;
        let body = ctx.ask.long_text(inputs.text, inputs.file, "What should it get done?", "Details, links, and what done looks like.")?;
        let Reply::Missions { missions: added } = ctx.send(Command::MissionAdd { deployment: deployment.id.clone(), title, body }).await? else { return Err("unexpected reply".into()) };
        let number = added.first().map(|mission| format!("#{} ", mission.number)).unwrap_or_default();
        ctx.ask.outro(&format!("Added mission {number}to {}. `{NAME} status --watch` shows how it's going.", deployment.name));
        return Ok(());
    }
    let deployment = ctx.deployment().await?;
    let all = missions(ctx, &deployment).await?;
    let empty = format!("{} has no missions yet: add one with `{NAME} mission add`", deployment.name);
    match action {
        MissionAction::Add => Ok(()),
        MissionAction::List => {
            let live = start(inputs.watch, ctx.ask.is_interactive())?;
            loop {
                let shown = missions(ctx, &deployment).await?;
                let text = if shown.is_empty() { empty.clone() } else { missions_table(&shown) };
                if !present(live.as_ref(), &text)? {
                    return Ok(());
                }
            }
        }
        MissionAction::Read => {
            let number = ctx.ask.pick(inputs.number, "Which mission?", mission_items(&all, |_| true), "--number", &empty)?;
            if !inputs.watch {
                show(&ctx.send(Command::MissionRead { deployment: deployment.id, mission: number }).await?);
                return Ok(());
            }
            // Watched, it shows where it stands and each step so far, as they come.
            let live = start(true, ctx.ask.is_interactive())?;
            loop {
                let Reply::Mission { mission, .. } = ctx.send(Command::MissionRead { deployment: deployment.id.clone(), mission: number }).await? else { return Err("unexpected reply".into()) };
                let filter = LogFilter { mission: Some(number), ..Default::default() };
                let Reply::Entries { entries } = ctx.send(Command::Log { deployment: deployment.id.clone(), filter }).await? else { return Err("unexpected reply".into()) };
                if !present(live.as_ref(), &mission_view(&mission, &entries))? {
                    return Ok(());
                }
            }
        }
        MissionAction::Pause => {
            let is_going = |mission: &Mission| matches!(mission.status, MissionStatus::Started | MissionStatus::HandedOff | MissionStatus::Blocked);
            let number = ctx.ask.pick(inputs.number, "Which mission?", mission_items(&all, is_going), "--number", "no mission is in progress")?;
            let why = ctx.ask.long_text(inputs.text, inputs.file, "Why pause it?", "Waiting on the design review.")?;
            post(ctx, &deployment, EntryKind::Paused, number, why).await
        }
        MissionAction::Resume => {
            let number = ctx.ask.pick(inputs.number, "Which mission?", mission_items(&all, |mission| mission.status == MissionStatus::Paused), "--number", "no mission is paused")?;
            let note = ctx.ask.long_text(inputs.text, inputs.file, "Anything to tell whoever picks it up?", "The review is done: go ahead.")?;
            post(ctx, &deployment, EntryKind::Resumed, number, note).await
        }
        MissionAction::Finish => {
            let number = ctx.ask.pick(inputs.number, "Which mission?", mission_items(&all, |mission| mission.status == MissionStatus::Done), "--number", "no mission is done yet")?;
            show(&ctx.send(Command::MissionFinish { deployment: deployment.id, mission: number }).await?);
            Ok(())
        }
    }
}
