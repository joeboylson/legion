//! `deploy`: start, rename or close a deployment.

use legion2_proto::{pipeline_label, Command, Deployment, FolderDetail, Reply, NAME, NO_PIPELINE};

use crate::{
    arguments::DeployAction,
    ask::item,
    commands::{
        operator::{self, OperatorInputs},
        pipeline::operator_items,
        show,
    },
    context::Context,
};

const DEFAULT_TEAM_NAME: &str = "team";

pub struct DeployInputs {
    pub pipeline: Option<String>,
    pub team: Option<Vec<String>>,
    pub name: Option<String>,
    pub yes: bool,
}

impl DeployInputs {
    pub fn none() -> DeployInputs {
        DeployInputs { pipeline: None, team: None, name: None, yes: false }
    }
}

/// A pipeline name not yet in the folder, from `wanted`.
fn free_pipeline_name(detail: &FolderDetail, wanted: &str) -> String {
    let is_taken = |name: &str| detail.pipelines.iter().any(|pipeline| pipeline.name == name);
    std::iter::once(wanted.to_string()).chain((2..).map(|number| format!("{wanted}-{number}"))).find(|name| !is_taken(name)).unwrap_or_default()
}

/// Asks for operators until the admin says that's all.
async fn add_operators(ctx: &mut Context, folder: &str) -> Result<(), String> {
    loop {
        let inputs = OperatorInputs { name: None, text: None, file: None, limit: None, model: None, mode: None, yes: false };
        let name = operator::add(ctx, folder, inputs).await?;
        ctx.ask.success(format!("added {name}"));
        if !ctx.ask.yes_no("Add another operator?", false)? {
            return Ok(());
        }
    }
}

/// Starts a deployment in the folder: operators first if it has none, then
/// how work moves, the team, and a name.
pub async fn start(ctx: &mut Context, inputs: DeployInputs) -> Result<Deployment, String> {
    let mut detail = ctx.folder().await?;
    let folder = detail.folder.path.clone();
    if detail.operators.is_empty() {
        if !ctx.ask.is_interactive() {
            return Err(format!("{} has no operators: add one with `{NAME} operator add`", detail.folder.name));
        }
        ctx.ask.note("This folder has no operators yet. Add the ones the team needs.");
        add_operators(ctx, &folder).await?;
        detail = ctx.folder().await?;
    }
    let routes = std::iter::once(item(NO_PIPELINE.to_string(), "No pipeline", "after each step the commander picks who goes next"))
        .chain(detail.pipelines.iter().filter(|pipeline| pipeline.problem.is_none() && pipeline.name != NO_PIPELINE).map(|pipeline| {
            let shape = match &pipeline.first {
                Some(first) => format!("starts with {first}: {}", pipeline.operators.join(", ")),
                None => format!("{} through the commander", pipeline.operators.join(", ")),
            };
            item(pipeline.name.clone(), format!("The {} pipeline", pipeline.name), shape)
        }))
        .collect();
    let mut pipeline = ctx.ask.pick(inputs.pipeline, "How should work move between operators?", routes, "--pipeline", "")?;
    let name = if pipeline == NO_PIPELINE {
        let everyone: Vec<String> = operator_items(&detail).into_iter().map(|choice| choice.value).collect();
        let team = ctx.ask.pick_many(inputs.team, "Who's on the team?", operator_items(&detail), "--team")?;
        let name = ctx.ask.text_or_default(inputs.name, "Name the deployment", DEFAULT_TEAM_NAME)?;
        if team.len() < everyone.len() {
            let team_name = ctx.ask.text_or_default(None, "Name this team, to use it again", &free_pipeline_name(&detail, DEFAULT_TEAM_NAME))?;
            ctx.send(Command::PipelineAdd { folder: folder.clone(), name: team_name.clone(), operators: team, in_order: false }).await?;
            pipeline = team_name;
        }
        name
    } else {
        ctx.ask.text_or_default(inputs.name, "Name the deployment", &pipeline)?
    };
    let spinner = ctx.ask.spinner("Starting the deployment and its commander");
    let started = ctx.send(Command::DeploymentStart { folder, pipeline, name: Some(name) }).await;
    match (started, spinner) {
        (Ok(Reply::Deployment { deployment }), spinner) => {
            let done = format!("{} is running ({}); its commander is starting", deployment.name, pipeline_label(&deployment.pipeline));
            match spinner {
                Some(spinner) => spinner.stop(done),
                None => println!("{done}"),
            }
            Ok(deployment)
        }
        (Ok(other), _) => Err(format!("unexpected reply: {other:?}")),
        (Err(problem), Some(spinner)) => {
            spinner.error(&problem);
            Err(problem)
        }
        (Err(problem), None) => Err(problem),
    }
}

/// A closed deployment: named, or picked from the folder's.
async fn pick_closed(ctx: &mut Context, prompt: &str) -> Result<String, String> {
    let closed: Vec<Deployment> = match ctx.named_deployment().map(str::to_string) {
        Some(named) => match ctx.send(Command::DeploymentList { folder: None }).await? {
            Reply::Deployments { deployments } => deployments.into_iter().filter(|deployment| deployment.closed_ms.is_some() && (deployment.id == named || deployment.name == named)).collect(),
            other => return Err(format!("unexpected reply: {other:?}")),
        },
        None => ctx.folder().await?.deployments.into_iter().filter(|deployment| deployment.closed_ms.is_some()).collect(),
    };
    let items = closed.iter().map(|deployment| item(deployment.id.clone(), &deployment.name, pipeline_label(&deployment.pipeline).to_string())).collect();
    ctx.ask.pick(None, prompt, items, "--deployment", "no closed deployments here")
}

/// Another pipeline the folder can run, for a deployment to move to.
async fn pick_pipeline(ctx: &mut Context, deployment: &Deployment) -> Result<String, String> {
    let detail = ctx.folder().await?;
    let choices = detail
        .pipelines
        .iter()
        .filter(|pipeline| pipeline.problem.is_none() && pipeline.name != deployment.pipeline)
        .map(|pipeline| item(pipeline.name.clone(), pipeline_label(&pipeline.name), pipeline.operators.join(", ")))
        .collect();
    ctx.ask.pick(None, &format!("Move {} to which pipeline?", deployment.name), choices, "--pipeline", "no other pipeline can run here")
}

pub async fn run(ctx: &mut Context, action: Option<DeployAction>, inputs: DeployInputs) -> Result<(), String> {
    match action.unwrap_or(DeployAction::Start) {
        DeployAction::Start => {
            ctx.ask.intro(NAME);
            start(ctx, inputs).await?;
            ctx.ask.outro(&format!("Add a mission with `{NAME} mission add`; `{NAME} status --watch` shows how it's going."));
            Ok(())
        }
        DeployAction::Edit => {
            let deployment = ctx.deployment().await?;
            let (name, pipeline) = match (inputs.name, inputs.pipeline) {
                (None, None) if !ctx.ask.is_interactive() => return Err("say what to change: --name or --pipeline".into()),
                (None, None) => {
                    let what = vec![item("name", "Its name", deployment.name.clone()), item("pipeline", "Its pipeline", pipeline_label(&deployment.pipeline).to_string())];
                    match ctx.ask.pick(None, "What should change?", what, "", "")? {
                        "name" => (Some(ctx.ask.text(None, &format!("New name for {}", deployment.name), &deployment.name, "--name")?), None),
                        _ => (None, Some(pick_pipeline(ctx, &deployment).await?)),
                    }
                }
                given => given,
            };
            if let Some(name) = name {
                show(&ctx.send(Command::DeploymentRename { deployment: deployment.id.clone(), name }).await?);
            }
            if let Some(pipeline) = pipeline {
                show(&ctx.send(Command::DeploymentRepipe { deployment: deployment.id, pipeline }).await?);
            }
            Ok(())
        }
        DeployAction::Reopen => {
            let id = pick_closed(ctx, "Reopen which deployment?").await?;
            let Reply::Deployment { deployment } = ctx.send(Command::DeploymentReopen { deployment: id }).await? else { return Err("unexpected reply".into()) };
            ctx.ask.success(format!("reopened {}; its commander is starting", deployment.name));
            Ok(())
        }
        DeployAction::Delete => {
            let id = pick_closed(ctx, "Delete which deployment?").await?;
            let prompt = "Delete it for good? Its log, missions and checkouts go; its branches stay in git.";
            if !ctx.ask.confirm(inputs.yes, prompt)? {
                return Ok(());
            }
            show(&ctx.send(Command::DeploymentDelete { deployment: id }).await?);
            Ok(())
        }
        DeployAction::Close => {
            let deployment = ctx.deployment().await?;
            if !ctx.ask.confirm(inputs.yes, &format!("Close {}? Every session in it ends.", deployment.name))? {
                return Ok(());
            }
            ctx.send(Command::DeploymentClose { deployment: deployment.id }).await?;
            ctx.ask.success(format!("closed {}", deployment.name));
            Ok(())
        }
    }
}
