//! `operator`: add, edit, remove or list the folder's operators.

use legion2_proto::{Command, FolderDetail, NAME};

use crate::{
    arguments::OperatorAction,
    ask::{item, Item},
    commands::{open_in_editor, setup::pick_mode, show},
    context::Context,
    pretty::{operators_table, summary},
};

pub const MODELS: &[(&str, &str)] = &[("default", "the session's own default"), ("opus", "the most capable"), ("sonnet", "fast and capable"), ("haiku", "quickest, for simple jobs")];
const DEFAULT_COPIES: &str = "1";

pub struct OperatorInputs {
    pub name: Option<String>,
    pub text: Option<String>,
    pub file: Option<String>,
    pub limit: Option<u32>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub yes: bool,
}

pub fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('-') && name.chars().all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
}

fn operator_items(detail: &FolderDetail) -> Vec<Item<String>> {
    detail.operators.iter().map(|operator| item(operator.name.clone(), &operator.name, summary(&operator.definition))).collect()
}

fn pick_model(ctx: &Context, given: Option<String>) -> Result<String, String> {
    let items = MODELS.iter().map(|(model, hint)| item(model.to_string(), model, hint)).collect();
    match (given, ctx.ask.is_interactive()) {
        (Some(model), _) => Ok(model),
        (None, true) => ctx.ask.pick(None, "Which model?", items, "--model", ""),
        (None, false) => Ok("default".into()),
    }
}

/// Adds one operator, asking for whatever wasn't given.
pub async fn add(ctx: &mut Context, folder: &str, inputs: OperatorInputs) -> Result<String, String> {
    let name = ctx.ask.text(inputs.name, "Name the operator", "poem-writer", "--name")?;
    if !is_plain_name(&name) {
        return Err(format!("{name:?}: use lowercase letters, digits and dashes, like poem-writer"));
    }
    let definition = ctx.ask.long_text(inputs.text, inputs.file, &format!("What does {name} do?"), "You write one haiku per mission into poems/<slug>.txt.")?;
    let copies = ctx.ask.text_or_default(inputs.limit.map(|limit| limit.to_string()), "How many copies may run at once?", DEFAULT_COPIES)?;
    let limit: u32 = copies.trim().parse().map_err(|_| format!("{copies:?} isn't a number"))?;
    let model = pick_model(ctx, inputs.model)?;
    ctx.send(Command::OperatorAdd { folder: folder.to_string(), name: name.clone(), definition, limit: (limit != 1).then_some(limit) }).await?;
    if model != "default" || inputs.mode.is_some() {
        let model = (model != "default").then_some(model);
        ctx.send(Command::OperatorSet { folder: folder.to_string(), name: name.clone(), limit: None, model, permission_mode: inputs.mode }).await?;
    }
    Ok(name)
}

pub async fn run(ctx: &mut Context, action: Option<OperatorAction>, inputs: OperatorInputs) -> Result<(), String> {
    let action = ctx.ask.action(action, "legion2 operator")?;
    let detail = ctx.folder().await?;
    let folder = detail.folder.path.clone();
    match action {
        OperatorAction::List => {
            println!("{}", operators_table(&detail));
            Ok(())
        }
        OperatorAction::Add => {
            let name = add(ctx, &folder, inputs).await?;
            ctx.ask.success(format!("added {name}; deployments with no pipeline have it from now on"));
            Ok(())
        }
        OperatorAction::Edit => {
            let empty = format!("no operators yet: add one with `{NAME} operator add`");
            let name = ctx.ask.pick(inputs.name, "Which operator?", operator_items(&detail), "--name", &empty)?;
            if inputs.limit.is_some() || inputs.model.is_some() || inputs.mode.is_some() {
                let reply = ctx.send(Command::OperatorSet { folder, name, limit: inputs.limit, model: inputs.model, permission_mode: inputs.mode }).await?;
                show(&reply);
                return Ok(());
            }
            if !ctx.ask.is_interactive() {
                return Err("say what to change: --limit, --model or --mode".into());
            }
            let current = detail.operators.iter().find(|operator| operator.name == name);
            let what = ctx.ask.pick(
                None,
                "What should change?",
                vec![
                    item("definition", "What it does", "opens in your editor"),
                    item("copies", "Copies", format!("now {}", current.map_or(1, |operator| operator.copy_limit))),
                    item("model", "Model", format!("now {}", current.and_then(|operator| operator.model.clone()).unwrap_or_else(|| "default".into()))),
                    item("mode", "Permission mode", format!("now {}", current.and_then(|operator| operator.permission_mode.clone()).unwrap_or_else(|| "the folder's".into()))),
                ],
                "",
                "",
            )?;
            match what {
                "definition" => open_in_editor(&std::path::Path::new(&folder).join(format!(".{NAME}/operators/{name}/definition.md"))),
                "copies" => {
                    let copies = ctx.ask.text(None, "How many copies may run at once?", "2", "--limit")?;
                    let limit = copies.trim().parse().map_err(|_| format!("{copies:?} isn't a number"))?;
                    show(&ctx.send(Command::OperatorSet { folder, name, limit: Some(limit), model: None, permission_mode: None }).await?);
                    Ok(())
                }
                "model" => {
                    let model = pick_model(ctx, None)?;
                    show(&ctx.send(Command::OperatorSet { folder, name, limit: None, model: Some(model), permission_mode: None }).await?);
                    Ok(())
                }
                _ => {
                    let mode = pick_mode(ctx, None, "Which permission mode should it start in?", None)?;
                    show(&ctx.send(Command::OperatorSet { folder, name, limit: None, model: None, permission_mode: mode }).await?);
                    Ok(())
                }
            }
        }
        OperatorAction::Remove => {
            let name = ctx.ask.pick(inputs.name, "Which operator?", operator_items(&detail), "--name", "no operators to remove")?;
            if !ctx.ask.confirm(inputs.yes, &format!("Remove {name} and its definition?"))? {
                return Ok(());
            }
            show(&ctx.send(Command::OperatorRemove { folder, name }).await?);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_lowercase_digits_and_dashes() {
        assert!(is_plain_name("poem-writer-2"));
        assert!(["Poem", "poem writer", "-x", ""].iter().all(|name| !is_plain_name(name)));
    }
}
