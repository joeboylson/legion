//! `status`: what's going on in this Legion, or in every one with --all.

use legion2_proto::{pipeline_label, Channels, Command, Deployment, FolderDetail, LogFilter, MissionStatus, Reply, NAME};

use crate::{
    context::{open_deployments, Context},
    live::{present, start},
    pretty::{missions_table, operators_table, sessions_table},
};

pub async fn run(ctx: &mut Context, all: bool, watch: bool) -> Result<(), String> {
    let folder_key = if all { None } else { Some(ctx.folder_key()?) };
    let live = start(watch, ctx.ask.is_interactive())?;
    loop {
        let text = match &folder_key {
            Some(_) => {
                let detail = ctx.folder().await?;
                folder_status(ctx, &detail).await?
            }
            None => every_status(ctx).await?,
        };
        if !present(live.as_ref(), &text)? {
            return Ok(());
        }
    }
}

fn heading(detail: &FolderDetail) -> String {
    format!("{} {}", console::style(&detail.folder.name).bold(), console::style(&detail.folder.path).dim())
}

async fn folder_status(ctx: &mut Context, detail: &FolderDetail) -> Result<String, String> {
    let open = open_deployments(detail);
    if open.is_empty() {
        return Ok(format!(
            "{}\n\nNo deployment is running. Start one with `{NAME} deploy`, or add a mission with `{NAME} mission add` and it'll help.\n\n{}",
            heading(detail),
            operators_table(detail)
        ));
    }
    let mut blocks = vec![heading(detail)];
    for deployment in &open {
        blocks.push(deployment_status(ctx, deployment).await?);
    }
    Ok(blocks.join("\n\n"))
}

async fn deployment_status(ctx: &mut Context, deployment: &Deployment) -> Result<String, String> {
    let Reply::Sessions { sessions } = ctx.send(Command::SessionList { deployment: Some(deployment.id.clone()) }).await? else { return Err("unexpected reply".into()) };
    let Reply::Missions { missions } = ctx.send(Command::MissionList { deployment: deployment.id.clone() }).await? else { return Err("unexpected reply".into()) };
    let open_questions = LogFilter { open_questions: true, ..Default::default() };
    let Reply::Entries { entries } = ctx.send(Command::Log { deployment: deployment.id.clone(), filter: open_questions }).await? else { return Err("unexpected reply".into()) };
    let title = format!("{} {} · {}", console::style("●").green(), console::style(&deployment.name).bold(), pipeline_label(&deployment.pipeline));
    let name = deployment.name.clone();
    let (finished, going): (Vec<_>, Vec<_>) = missions.into_iter().partition(|mission| mission.status == MissionStatus::Done);
    let missions_part = match (going.is_empty(), finished.len()) {
        (true, 0) => format!("No missions yet. Add one with `{NAME} mission add`."),
        (true, done) => format!("All {done} missions done."),
        (false, 0) => missions_table(&going),
        (false, done) => format!("{}\n{done} more done.", missions_table(&going)),
    };
    let questions_part = match entries.len() {
        0 => String::new(),
        1 => format!("\n{} 1 question waiting on you: `{NAME} answer`", console::style("!").red().bold()),
        count => format!("\n{} {count} questions waiting on you: `{NAME} answer`", console::style("!").red().bold()),
    };
    Ok(format!("{title}\n{}\n{missions_part}{questions_part}", sessions_table(&sessions, &|_| name.clone())))
}

fn channels_line(channels: &Channels) -> Option<String> {
    let hosted = channels.hosted.as_ref().map(|hosted| format!("hosting port {} ({} subscribed)", hosted.port, hosted.subscribers.len()));
    let subscribed = channels.subscriptions.iter().map(|subscription| {
        let state = if subscription.is_up { console::style("up").green() } else { console::style("down").red() };
        format!("subscribed to {} ({state})", subscription.address)
    });
    let parts: Vec<String> = hosted.into_iter().chain(subscribed).collect();
    (!parts.is_empty()).then(|| format!("{} {}", console::style("Channels:").bold(), parts.join(", ")))
}

async fn every_status(ctx: &mut Context) -> Result<String, String> {
    let Reply::Folders { folders } = ctx.send(Command::FolderList).await? else { return Err("unexpected reply".into()) };
    let mut blocks = Vec::new();
    let mut quiet = Vec::new();
    for folder in folders {
        let detail = ctx.read_folder(&folder.path).await?;
        let open = open_deployments(&detail);
        if open.is_empty() {
            quiet.push(detail.folder.name.clone());
            continue;
        }
        let mut block = vec![heading(&detail)];
        for deployment in &open {
            block.push(deployment_status(ctx, deployment).await?);
        }
        blocks.push(block.join("\n\n"));
    }
    if blocks.is_empty() {
        blocks.push("Nothing is running.".into());
    }
    if !quiet.is_empty() {
        blocks.push(format!("{} {}", console::style("Nothing running in:").dim(), quiet.join(", ")));
    }
    if let Ok(Reply::Channels { channels }) = ctx.send(Command::ChannelStatus).await {
        blocks.extend(channels_line(&channels));
    }
    Ok(blocks.join("\n\n"))
}
