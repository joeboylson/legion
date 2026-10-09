//! The legion2 command. Everything it does goes through legion2d, except
//! installing the service.
//!
//! Each command does one kind of thing. Whatever it needs and wasn't given
//! as a flag, it asks for in a terminal; without one, it says which flag is
//! missing. Inside a session legion2d started, only `mcp` is used.

mod arguments;
mod ask;
mod commands;
mod context;
mod live;
mod mcp_server;
mod pretty;
mod reply_format;
mod service;
mod time_span;

use clap::{CommandFactory, Parser};
use legion2_proto::NAME;

use crate::{
    arguments::{CommandLine, ServiceChoice, Top},
    ask::Asker,
    commands::{
        channel::ChannelInputs,
        deploy::DeployInputs,
        log::LogInputs,
        mission::MissionInputs,
        operator::OperatorInputs,
        pipeline::PipelineInputs,
        session::SessionInputs,
        talk::TalkInputs,
    },
    context::Context,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{NAME}: {error}");
        std::process::exit(1);
    }
}

fn run_service(action: Option<ServiceChoice>, claude: Option<String>, addon: Option<String>, yes: bool) -> Result<(), String> {
    let ask = Asker::new();
    match ask.action(action, "legion2 service")? {
        ServiceChoice::Install => {
            let claude = ask.text_or_default(claude, "Which Claude command should sessions run?", "claude")?;
            println!("{}", service::install(claude, addon)?);
        }
        ServiceChoice::Remove => {
            if ask.confirm(yes, &format!("Stop {NAME}d and remove the service? Every session ends."))? {
                println!("{}", service::remove()?);
            }
        }
    }
    Ok(())
}

async fn run() -> Result<(), String> {
    let command_line = CommandLine::parse();
    // On its own, it lists what it can do, the same as --help.
    let Some(command) = command_line.command else {
        return CommandLine::command().print_help().map_err(|error| error.to_string());
    };
    match command {
        Top::Mcp => return mcp_server::serve_tools().await,
        Top::Service { action, claude, addon, yes } => return run_service(action, claude, addon, yes),
        _ => {}
    }
    let has_named_deployment = command_line.deployment.is_some();
    let mut ctx = Context::connect(command_line.folder, command_line.deployment).await?;
    match command {
        Top::Mcp | Top::Service { .. } => Ok(()),
        Top::Status { all, watch } => commands::status::run(&mut ctx, all, watch).await,
        Top::Setup { action, check, mode, yes } => commands::setup::run(&mut ctx, action, check, mode, yes).await,
        Top::Operator { action, name, text, file, limit, model, mode, yes } => {
            commands::operator::run(&mut ctx, action, OperatorInputs { name, text, file, limit, model, mode, yes }).await
        }
        Top::Pipeline { action, name, operators, route, yes } => commands::pipeline::run(&mut ctx, action, PipelineInputs { name, operators, route, yes }).await,
        Top::Deploy { action, pipeline, team, name, yes } => commands::deploy::run(&mut ctx, action, DeployInputs { pipeline, team, name, yes }).await,
        Top::Mission { action, title, text, file, number, watch } => {
            commands::mission::run(&mut ctx, action, MissionInputs { title, text, file, number, watch }, has_named_deployment).await
        }
        Top::Answer { question, text, file } => commands::talk::answer(&mut ctx, question, TalkInputs { text, file }).await,
        Top::Send { to, text, file } => commands::talk::send(&mut ctx, to, TalkInputs { text, file }).await,
        Top::Session { action, operator, mission, position, key, yes } => {
            commands::session::run(&mut ctx, action, SessionInputs { operator, mission, position, key, yes }).await
        }
        Top::Log { watch, mission, position, kind, since, export } => commands::log::run(&mut ctx, LogInputs { watch, mission, position, kinds: kind, since, export }).await,
        Top::Channel { action, port, key, address, send, receive, watch, yes } => {
            commands::channel::run(&mut ctx, action, ChannelInputs { port, key, address, send, receive, watch, yes }).await
        }
    }
}
