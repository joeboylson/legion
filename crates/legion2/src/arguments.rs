//! The legion2 command's arguments.

use clap::{Parser, Subcommand};
use legion2_proto::{ChannelSwitch, DEFAULT_CHANNEL_PORT, ENV_DEPLOYMENT};

#[derive(Parser)]
#[command(name = "legion2", about = "Run Legion: folders, deployments, missions, sessions and the deployment log.")]
pub struct CommandLine {
    /// The deployment, by name or ID. Inside a session, its own deployment.
    #[arg(long, global = true, env = ENV_DEPLOYMENT)]
    pub deployment: Option<String>,
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand)]
pub enum Action {
    /// Check legion2d is running.
    Ping,
    /// Serve Legion's tools to the Claude session legion2d started this in.
    #[command(hide = true)]
    Mcp,
    /// Add a folder, setting up .legion2/ in it if it has none.
    Add { path: String },
    /// List folders.
    Folders,
    /// Remove a folder from Legion. Close its deployments first. Its setup, missions and log stay on disk.
    Remove { folder: String },
    /// Start a deployment of a folder's pipeline. Its commander starts with it.
    Deploy {
        folder: String,
        pipeline: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// List deployments.
    Deployments { folder: Option<String> },
    /// Give the deployment a new name, unique in its folder.
    Rename { name: String },
    /// Close the deployment: every session in it ends, and it isn't brought back.
    Close,
    /// Create a mission in the deployment (human only). The body comes from --body, --file or stdin.
    New {
        title: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long, conflicts_with = "body")]
        file: Option<String>,
    },
    /// List the deployment's missions and where each stands.
    Missions,
    /// Read a mission.
    Mission { number: u32 },
    /// Move the base branch up to a done mission's branch.
    Finish { mission: u32 },
    /// Start an operator, or the commander, in the deployment.
    Start {
        operator: String,
        #[arg(long)]
        mission: Option<u32>,
    },
    /// End a position's session.
    Stop { position: String },
    /// List running sessions (every deployment's, without --deployment).
    Sessions,
    /// Print a session's screen.
    Screen { position: String },
    /// Press a key in a session's terminal: enter, esc, up, down or tab.
    Key { position: String, key: String },
    /// Message a position in the deployment.
    Send {
        to: String,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Add a note to the deployment log.
    Note {
        #[arg(long)]
        mission: Option<u32>,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Ask the human a question.
    Ask {
        #[arg(long)]
        mission: Option<u32>,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Answer a question or decision, by its entry number. The answer goes to whoever sent it.
    Answer {
        question: i64,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// List questions and decisions with no answer yet.
    Questions,
    /// Hand a mission to the next operator. Tells the commander.
    Handoff {
        mission: u32,
        next: String,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Report a mission done. Tells the commander.
    Done {
        mission: u32,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Report a mission blocked. Tells the commander.
    Blocked {
        mission: u32,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Pause a mission. Tells whoever holds it.
    Pause {
        mission: u32,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Resume a paused mission. Tells the commander.
    Resume {
        mission: u32,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Tell the human about a choice made without stopping; they can overrule it with an answer.
    Flag {
        #[arg(long)]
        mission: Option<u32>,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Call out a one-line heads-up for everyone working in the deployment's folder.
    Callout {
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// List the folder's callouts, oldest first.
    Callouts,
    /// Suggest a mission to the human.
    Suggest {
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Record gotchas and learnings for the next commander.
    Postmortem {
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Show the deployment log.
    Log {
        #[command(flatten)]
        filter: FilterArguments,
        /// Keep showing new entries as they land.
        #[arg(long, short)]
        follow: bool,
    },
    /// Write the deployment log, or part of it, to a file.
    Export {
        #[command(flatten)]
        filter: FilterArguments,
        #[arg(long, value_enum, default_value_t = ExportFormat::Jsonl)]
        format: ExportFormat,
        /// The file to write; stdout without it.
        #[arg(long, short)]
        output: Option<String>,
    },
    /// Run legion2d as a launchd service: it starts at login and comes back if it crashes.
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Link this machine's Legion with others: host a channel, or subscribe to one.
    Channel {
        #[command(subcommand)]
        action: ChannelAction,
    },
}

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum ChannelAction {
    /// Host a channel other Legions can subscribe to. Prints the key to give them.
    Open {
        #[arg(long, default_value_t = DEFAULT_CHANNEL_PORT)]
        port: u16,
        /// The key subscribers give; one is made up without it.
        #[arg(long)]
        key: Option<String>,
    },
    /// Stop hosting the channel. Subscriptions stay.
    Close,
    /// Subscribe to another Legion's channel.
    Subscribe {
        /// host:port
        address: String,
        #[arg(long)]
        key: String,
    },
    Unsubscribe {
        /// host:port
        address: String,
    },
    /// Show the hosted channel and the subscriptions, and whether each end is up.
    Status,
    /// Set the deployment's switches: off, ask (the human approves each message) or free.
    Switch {
        /// Whether its commander's messages go out to other teams.
        #[arg(long, value_parser = parse_switch)]
        send: Option<ChannelSwitch>,
        /// Whether other teams' messages reach its commander.
        #[arg(long, value_parser = parse_switch)]
        receive: Option<ChannelSwitch>,
    },
    /// Show what the channels carried and saw on this machine, newest last.
    Log {
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum ServiceAction {
    /// Install the service and start it. Its output goes to legion2d.log in Legion's data folder.
    Install {
        /// The Claude command legion2d runs sessions with.
        #[arg(long, default_value = "claude")]
        claude: String,
        /// The add-on folder; legion2d's own default without it.
        #[arg(long)]
        addon: Option<String>,
    },
    /// Stop the service and remove it.
    Remove,
}

#[derive(clap::Args, Clone, Default)]
pub struct FilterArguments {
    #[arg(long)]
    pub mission: Option<u32>,
    /// Entries from or to this position.
    #[arg(long)]
    pub position: Option<String>,
    /// Only these kinds, e.g. --kind question --kind answer.
    #[arg(long = "kind")]
    pub kinds: Vec<String>,
    /// Only entries this recent: 30m, 2h, 1d.
    #[arg(long)]
    pub since: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum ExportFormat {
    /// One entry per line, as JSON.
    Jsonl,
    Markdown,
}

fn parse_switch(value: &str) -> Result<ChannelSwitch, String> {
    [ChannelSwitch::Off, ChannelSwitch::Ask, ChannelSwitch::Free]
        .into_iter()
        .find(|switch| switch.as_str() == value)
        .ok_or_else(|| format!("{value:?}: use off, ask or free"))
}
