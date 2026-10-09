//! `pipeline`: add, edit, remove or list the folder's pipelines.

use legion2_proto::{Command, FolderDetail, NAME, NO_PIPELINE};

use crate::{
    arguments::{Choice, PipelineAction, Route},
    ask::{item, Item},
    commands::{open_in_editor, show},
    context::Context,
    pretty::{pipelines_table, summary},
};

pub struct PipelineInputs {
    pub name: Option<String>,
    pub operators: Option<Vec<String>>,
    pub route: Option<Route>,
    pub yes: bool,
}

/// Pipelines with a file of their own: no pipeline has none to edit or remove.
fn file_pipeline_items(detail: &FolderDetail) -> Vec<Item<String>> {
    detail
        .pipelines
        .iter()
        .filter(|pipeline| pipeline.name != NO_PIPELINE)
        .map(|pipeline| item(pipeline.name.clone(), &pipeline.name, pipeline.operators.join(", ")))
        .collect()
}

pub fn operator_items(detail: &FolderDetail) -> Vec<Item<String>> {
    detail.operators.iter().filter(|operator| operator.problem.is_none()).map(|operator| item(operator.name.clone(), &operator.name, summary(&operator.definition))).collect()
}

pub async fn run(ctx: &mut Context, action: Option<PipelineAction>, inputs: PipelineInputs) -> Result<(), String> {
    let action = ctx.ask.action(action, "legion2 pipeline")?;
    let detail = ctx.folder().await?;
    let folder = detail.folder.path.clone();
    match action {
        PipelineAction::List => {
            println!("{}", pipelines_table(&detail));
            Ok(())
        }
        PipelineAction::Add => {
            let name = ctx.ask.text(inputs.name, "Name the pipeline", "poems", "--name")?;
            let routes = Route::every().into_iter().map(|route| item(route, route.label(), "")).collect();
            let route = ctx.ask.pick(inputs.route, "How should work move between its operators?", routes, "--route", "")?;
            let operators = match route {
                Route::Commander => ctx.ask.pick_many(inputs.operators, "Who's on it?", operator_items(&detail), "--operators")?,
                Route::InOrder => ctx.ask.pick_in_order(inputs.operators, operator_items(&detail), "--operators")?,
            };
            show(&ctx.send(Command::PipelineAdd { folder, name, operators, in_order: route == Route::InOrder }).await?);
            Ok(())
        }
        PipelineAction::Edit => {
            let empty = format!("no pipeline files yet: add one with `{NAME} pipeline add`");
            let name = ctx.ask.pick(inputs.name, "Which pipeline?", file_pipeline_items(&detail), "--name", &empty)?;
            open_in_editor(&std::path::Path::new(&folder).join(format!(".{NAME}/pipelines/{name}.yaml")))
        }
        PipelineAction::Remove => {
            let name = ctx.ask.pick(inputs.name, "Which pipeline?", file_pipeline_items(&detail), "--name", "no pipeline files to remove")?;
            if !ctx.ask.confirm(inputs.yes, &format!("Remove the {name} pipeline?"))? {
                return Ok(());
            }
            show(&ctx.send(Command::PipelineRemove { folder, name }).await?);
            Ok(())
        }
    }
}
