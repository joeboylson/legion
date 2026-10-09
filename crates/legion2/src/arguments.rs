//! The legion2 command's arguments. Each command does one kind of thing; the
//! word after it picks what to do (it asks when left out), and each input
//! has one flag. Whatever isn't given is asked for, in a terminal.

use clap::{Parser, Subcommand, ValueEnum};
use legion2_proto::ENV_DEPLOYMENT;

#[derive(Parser)]
#[command(name = "legion2", about = "Run Legion. Each command asks for what you leave out; q quits live views.")]
pub struct CommandLine {
    /// Another Legion folder than the one you're in, by name or path.
    #[arg(long, global = true)]
    pub folder: Option<String>,
    /// The deployment, by name or ID, when the folder runs more than one.
    #[arg(long, global = true, env = ENV_DEPLOYMENT)]
    pub deployment: Option<String>,
    /// What to do; every command is listed without one.
    #[command(subcommand)]
    pub command: Option<Top>,
}

#[derive(Subcommand)]
pub enum Top {
    /// What's going on in this Legion: deployments, sessions, missions and questions.
    Status {
        /// Every Legion folder on this machine, not just this one.
        #[arg(long)]
        all: bool,
        /// Keep it on screen, updated every 2 seconds.
        #[arg(long)]
        watch: bool,
    },
    /// Set up Legion in this folder, show its setup, or remove it.
    Setup {
        action: Option<SetupAction>,
        /// The command that checks a mission's work, such as `npm test`.
        #[arg(long)]
        check: Option<String>,
        /// The permission mode sessions start in: acceptEdits, auto, bypassPermissions, manual, dontAsk or plan.
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, short)]
        yes: bool,
    },
    /// Add, edit, remove or list operators.
    Operator {
        action: Option<OperatorAction>,
        #[arg(long)]
        name: Option<String>,
        /// What it does.
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        /// What it does, from a file.
        #[arg(long)]
        file: Option<String>,
        /// How many copies may run at once.
        #[arg(long)]
        limit: Option<u32>,
        /// opus, sonnet, haiku, or default.
        #[arg(long)]
        model: Option<String>,
        /// The permission mode its sessions start in.
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, short)]
        yes: bool,
    },
    /// Add, edit, remove or list pipelines.
    Pipeline {
        action: Option<PipelineAction>,
        #[arg(long)]
        name: Option<String>,
        /// Its operators, comma-separated, in order for an in-order route.
        #[arg(long, value_delimiter = ',')]
        operators: Option<Vec<String>>,
        #[arg(long)]
        route: Option<Route>,
        #[arg(long, short)]
        yes: bool,
    },
    /// Start, edit (rename it or change its pipeline), close, reopen or delete a deployment.
    Deploy {
        action: Option<DeployAction>,
        /// The pipeline to run, or for edit to move to; `none` for no pipeline, where the commander picks who goes next.
        #[arg(long)]
        pipeline: Option<String>,
        /// With no pipeline, the team, comma-separated, when it isn't every operator.
        #[arg(long, value_delimiter = ',')]
        team: Option<Vec<String>>,
        /// The deployment's name; for edit, its new name.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, short)]
        yes: bool,
    },
    /// Add, list, read, pause, resume or finish missions.
    Mission {
        action: Option<MissionAction>,
        #[arg(long)]
        title: Option<String>,
        /// The mission, or the note for a pause or resume.
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        /// The same, from a file.
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        number: Option<u32>,
        /// Keep it on screen, updated every 2 seconds: the list, or one mission's way through its statuses.
        #[arg(long)]
        watch: bool,
    },
    /// Answer a question waiting on you.
    Answer {
        /// The question's entry number.
        #[arg(long)]
        question: Option<i64>,
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        #[arg(long)]
        file: Option<String>,
    },
    /// Message someone on the team.
    Send {
        /// A running position, such as commander or builder-2.
        #[arg(long)]
        to: Option<String>,
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        #[arg(long)]
        file: Option<String>,
    },
    /// Start, stop or watch a session, or press a key in one.
    Session {
        action: Option<SessionAction>,
        /// For start: the operator, or commander.
        #[arg(long)]
        operator: Option<String>,
        /// For start: the mission to work on.
        #[arg(long)]
        mission: Option<u32>,
        /// A running position, such as builder-2.
        #[arg(long)]
        position: Option<String>,
        /// enter, esc, up, down or tab.
        #[arg(long)]
        key: Option<String>,
        #[arg(long, short)]
        yes: bool,
    },
    /// The deployment log.
    Log {
        /// Keep showing new entries as they land.
        #[arg(long)]
        watch: bool,
        #[arg(long)]
        mission: Option<u32>,
        /// Entries from or to this position.
        #[arg(long)]
        position: Option<String>,
        /// Only these kinds, comma-separated, such as question,answer.
        #[arg(long, value_delimiter = ',')]
        kind: Vec<String>,
        /// Only entries this recent: 30m, 2h, 1d.
        #[arg(long)]
        since: Option<String>,
        /// Write it to a file instead: .md for Markdown, anything else for JSON lines.
        #[arg(long)]
        export: Option<String>,
    },
    /// Link this Legion with others: see, open, close or subscribe to channels, and set switches.
    Channel {
        action: Option<ChannelAction>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        key: Option<String>,
        /// host:port
        #[arg(long)]
        address: Option<String>,
        /// off, ask or free.
        #[arg(long)]
        send: Option<String>,
        /// off, ask or free.
        #[arg(long)]
        receive: Option<String>,
        #[arg(long)]
        watch: bool,
        #[arg(long, short)]
        yes: bool,
    },
    /// Install or remove the background service.
    Service {
        action: Option<ServiceChoice>,
        /// The Claude command sessions run with.
        #[arg(long)]
        claude: Option<String>,
        /// The add-on folder; legion2d's own without it.
        #[arg(long)]
        addon: Option<String>,
        #[arg(long, short)]
        yes: bool,
    },
    /// Serve Legion's tools to the Claude session legion2d started this in.
    #[command(hide = true)]
    Mcp,
}

/// An action word: its name on the command line, and how the menu says it.
pub trait Choice: ValueEnum + Clone + Eq {
    fn label(&self) -> &'static str;
    fn every() -> Vec<Self> {
        Self::value_variants().to_vec()
    }
    fn word(&self) -> String {
        self.to_possible_value().map(|value| value.get_name().to_string()).unwrap_or_default()
    }
}

macro_rules! choices {
    ($name:ident { $($variant:ident => $label:expr),+ $(,)? }) => {
        #[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl Choice for $name {
            fn label(&self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }
        }
    };
}

choices!(SetupAction { Init => "Set up this folder, or change its settings", Show => "Show its operators, pipelines and settings", Remove => "Remove this folder from Legion" });
choices!(OperatorAction { Add => "Add an operator", Edit => "Edit an operator", Remove => "Remove an operator", List => "List the operators" });
choices!(PipelineAction { Add => "Add a pipeline", Edit => "Edit a pipeline's file", Remove => "Remove a pipeline", List => "List the pipelines" });
choices!(DeployAction { Start => "Start a deployment", Edit => "Rename a deployment, or change its pipeline", Close => "Close a deployment", Reopen => "Reopen a closed deployment", Delete => "Delete a closed deployment for good" });
choices!(MissionAction {
    Add => "Add a mission",
    List => "List the missions",
    Read => "Read a mission",
    Pause => "Pause a mission",
    Resume => "Resume a paused mission",
    Finish => "Finish a done mission",
});
choices!(SessionAction { Start => "Start a session", Stop => "Stop a session", Watch => "Watch a session's screen", Key => "Press a key in a session" });
choices!(ChannelAction {
    Status => "Show the channels",
    Open => "Host a channel",
    Close => "Stop hosting the channel",
    Subscribe => "Subscribe to another Legion's channel",
    Unsubscribe => "Unsubscribe from a channel",
    Switch => "Set a deployment's send and receive switches",
    Log => "Show what the channels carried",
});
choices!(ServiceChoice { Install => "Install the service and start it", Remove => "Stop the service and remove it" });
choices!(Route { Commander => "No set order: after each step the commander picks who goes next", InOrder => "In order: each operator passes to the next" });
