//! `setup`: set up Legion in this folder or change its settings, show its
//! setup, or remove it from Legion.

use std::path::PathBuf;

use legion2_proto::{Command, Reply, NAME};

use crate::{
    arguments::SetupAction,
    ask::item,
    context::{find_setup_root, Context},
    pretty::{operators_table, pipelines_table},
};

/// Claude Code's permission modes, as a session's settings name them.
pub const PERMISSION_MODES: &[(&str, &str)] = &[
    ("auto", "Claude decides what's safe; asks about the rest"),
    ("acceptEdits", "file edits go ahead; other tools ask"),
    ("manual", "every tool asks"),
    ("bypassPermissions", "nothing asks"),
    ("dontAsk", "refuses what would ask"),
    ("plan", "plans only, changes nothing"),
];
/// The list's choice for leaving the mode as it is.
const KEEP: &str = "keep";

/// Where `setup init` works: --folder, the Legion folder you're in, or here.
fn init_path(ctx: &Context) -> Result<PathBuf, String> {
    let here = std::env::current_dir().map_err(|error| error.to_string())?;
    match ctx.folder_key() {
        Ok(key) if PathBuf::from(&key).is_absolute() => Ok(PathBuf::from(key)),
        Ok(key) => Ok(here.join(key)),
        Err(_) => Ok(find_setup_root(&here).unwrap_or(here)),
    }
}

pub fn pick_mode(ctx: &Context, given: Option<String>, prompt: &str, current: Option<&str>) -> Result<Option<String>, String> {
    if given.is_some() || !ctx.ask.is_interactive() {
        return Ok(given);
    }
    let keep_hint = current.map_or("Claude Code's own default".to_string(), |mode| format!("now {mode}"));
    let items = std::iter::once(item(KEEP.to_string(), "Leave it", keep_hint))
        .chain(PERMISSION_MODES.iter().map(|(mode, hint)| item(mode.to_string(), mode, hint)))
        .collect();
    let picked = ctx.ask.pick(None, prompt, items, "--mode", "")?;
    Ok(Some(picked).filter(|mode| mode != KEEP))
}

pub async fn run(ctx: &mut Context, action: Option<SetupAction>, check: Option<String>, mode: Option<String>, yes: bool) -> Result<(), String> {
    let is_set_up = ctx.folder_key().is_ok();
    let action = match (action, is_set_up) {
        (Some(action), _) => action,
        (None, false) => SetupAction::Init,
        (None, true) => ctx.ask.action(None, "legion2 setup")?,
    };
    match action {
        SetupAction::Init => init(ctx, check, mode).await,
        SetupAction::Show => {
            let detail = ctx.folder().await?;
            let check = detail.check.as_deref().unwrap_or("none");
            let mode = detail.permission_mode.as_deref().unwrap_or("Claude Code's default");
            println!("{} {}", console::style(&detail.folder.name).bold(), console::style(&detail.folder.path).dim());
            println!("check: {check}   permission mode: {mode}   data: {}\n", detail.outside_folder);
            println!("{}\n{}", pipelines_table(&detail), operators_table(&detail));
            Ok(())
        }
        SetupAction::Remove => {
            let detail = ctx.folder().await?;
            let prompt = format!("Remove {} from Legion? Its .{NAME}/ setup, missions and log stay on disk.", detail.folder.name);
            if !ctx.ask.confirm(yes, &prompt)? {
                return Ok(());
            }
            let reply = ctx.send(Command::FolderRemove { folder: detail.folder.path.clone() }).await?;
            crate::commands::show(&reply);
            Ok(())
        }
    }
}

async fn init(ctx: &mut Context, check: Option<String>, mode: Option<String>) -> Result<(), String> {
    let path = init_path(ctx)?;
    ctx.ask.intro(NAME);
    let Reply::Folder { folder, created_setup } = ctx.send(Command::FolderAdd { path: path.to_string_lossy().into_owned() }).await? else { return Err("unexpected reply".into()) };
    match created_setup {
        true => ctx.ask.success(format!("Set up .{NAME}/ in {} with a starting team (planner, builder, reviewer). Commit it so others get it.", folder.path)),
        false => ctx.ask.note(format!("{} is set up already; you can change its settings.", folder.name)),
    }
    let detail = ctx.read_folder(&folder.path).await?;
    // Left empty, the check stays as it is.
    let placeholder = detail.check.clone().unwrap_or_else(|| "npm test".into());
    let check = ctx.ask.optional_text(check, "The command that checks a mission's work (optional)", &placeholder)?;
    let mode = pick_mode(ctx, mode, "Which permission mode should sessions start in?", detail.permission_mode.as_deref())?;
    if check.is_some() || mode.is_some() {
        ctx.send(Command::SettingsSet { folder: folder.path.clone(), check, permission_mode: mode }).await?;
    }
    ctx.ask.outro(&format!("Ready. Add operators with `{NAME} operator add`, then a mission with `{NAME} mission add`."));
    Ok(())
}
