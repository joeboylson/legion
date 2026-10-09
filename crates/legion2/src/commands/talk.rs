//! `answer` and `send`: the admin talking to the team.

use legion2_proto::{Command, Deployment, Entry, EntryKind, LogFilter, NewEntry, Reply, SessionInfo};

use crate::{
    ask::item,
    commands::show,
    context::{open_deployments, Context},
};

pub struct TalkInputs {
    pub text: Option<String>,
    pub file: Option<String>,
}

fn first_line(text: &str) -> String {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").chars().take(90).collect()
}

async fn open_questions(ctx: &mut Context, deployment: &str) -> Result<Vec<Entry>, String> {
    let filter = LogFilter { open_questions: true, ..Default::default() };
    match ctx.send(Command::Log { deployment: deployment.to_string(), filter }).await? {
        Reply::Entries { entries } => Ok(entries),
        other => Err(format!("unexpected reply: {other:?}")),
    }
}

/// The deployment to answer in: named, or picked from those with questions
/// waiting, each with how many. In a Legion folder, its own; anywhere else,
/// every one on this machine.
async fn deployment_with_questions(ctx: &mut Context) -> Result<Deployment, String> {
    if ctx.has_named_deployment() {
        return ctx.deployment().await;
    }
    let candidates: Vec<Deployment> = match ctx.folder_key() {
        Ok(_) => open_deployments(&ctx.folder().await?),
        Err(_) => match ctx.send(Command::DeploymentList { folder: None }).await? {
            Reply::Deployments { deployments } => deployments.into_iter().filter(|deployment| deployment.closed_ms.is_none()).collect(),
            other => return Err(format!("unexpected reply: {other:?}")),
        },
    };
    let mut waiting = Vec::new();
    for deployment in candidates {
        let count = open_questions(ctx, &deployment.id).await?.len();
        if count > 0 {
            waiting.push((deployment, count));
        }
    }
    let lines: Vec<String> = waiting.iter().map(|(deployment, count)| format!("{}: {}", deployment.name, questions_label(*count))).collect();
    if !lines.is_empty() {
        ctx.ask.note(format!("Waiting on you:\n{}", lines.join("\n")));
    }
    let items = waiting.iter().map(|(deployment, count)| item(deployment.id.clone(), &deployment.name, questions_label(*count))).collect();
    let id = ctx.ask.pick(None, "Which deployment's questions?", items, "--deployment", "no questions are waiting on you")?;
    waiting.into_iter().map(|(deployment, _)| deployment).find(|deployment| deployment.id == id).ok_or_else(|| format!("no deployment {id}"))
}

fn questions_label(count: usize) -> String {
    match count {
        1 => "1 question".into(),
        count => format!("{count} questions"),
    }
}

pub async fn answer(ctx: &mut Context, question: Option<i64>, inputs: TalkInputs) -> Result<(), String> {
    let deployment = deployment_with_questions(ctx).await?;
    let entries = open_questions(ctx, &deployment.id).await?;
    let items = entries.iter().map(|entry| item(entry.id, format!("#{} {}", entry.id, first_line(&entry.text)), format!("from {}", entry.from))).collect();
    let number = ctx.ask.pick(question, "Which question?", items, "--question", "no questions are waiting on you")?;
    if let Some(asked) = entries.iter().find(|entry| entry.id == number) {
        ctx.ask.note(format!("{} asks:\n{}", asked.from, asked.text.trim()));
    }
    let text = ctx.ask.long_text(inputs.text, inputs.file, "Your answer", "yes")?;
    let entry = NewEntry { kind: EntryKind::Answer, mission: None, to: None, text, answers: Some(number) };
    show(&ctx.send(Command::Post { deployment: deployment.id, entry }).await?);
    Ok(())
}

pub async fn running(ctx: &mut Context, deployment: &str) -> Result<Vec<SessionInfo>, String> {
    match ctx.send(Command::SessionList { deployment: Some(deployment.to_string()) }).await? {
        Reply::Sessions { sessions } => Ok(sessions),
        other => Err(format!("unexpected reply: {other:?}")),
    }
}

pub fn session_items(sessions: &[SessionInfo]) -> Vec<crate::ask::Item<String>> {
    sessions
        .iter()
        .map(|session| {
            let mission = session.mission.map(|number| format!(" on #{number}")).unwrap_or_default();
            item(session.position.clone(), &session.position, format!("{}{mission}", session.activity.label()))
        })
        .collect()
}

pub async fn send(ctx: &mut Context, to: Option<String>, inputs: TalkInputs) -> Result<(), String> {
    let deployment = ctx.deployment().await?;
    let sessions = running(ctx, &deployment.id).await?;
    let to = ctx.ask.pick(to, "Who to?", session_items(&sessions), "--to", "nobody is running in this deployment")?;
    let text = ctx.ask.long_text(inputs.text, inputs.file, &format!("Message for {to}"), "Please check the failing test first.")?;
    let entry = NewEntry { kind: EntryKind::Message, mission: None, to: Some(to), text, answers: None };
    show(&ctx.send(Command::Post { deployment: deployment.id, entry }).await?);
    Ok(())
}
