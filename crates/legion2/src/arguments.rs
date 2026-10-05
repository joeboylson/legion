//! The legion2 command's arguments.

use clap::{Parser, Subcommand};
use legion2_proto::ENV_RUN;

#[derive(Parser)]
#[command(name = "legion2", about = "Run Legion: folders, runs, missions, sessions and the run log.")]
pub struct CommandLine {
    /// The run, by name or ID. Inside a session, its own run.
    #[arg(long, global = true, env = ENV_RUN)]
    pub run: Option<String>,
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand)]
pub enum Action {
    /// Check legion2d is running.
    Ping,
    /// Add a folder, setting up .legion2/ in it if it has none.
    Add { path: String },
    /// List folders.
    Folders,
    /// Start a run of a folder's pipeline. Its commander starts with it.
    Run {
        folder: String,
        pipeline: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// List runs.
    Runs { folder: Option<String> },
    /// Close the run: every session in it ends, and it isn't brought back.
    Close,
    /// Create a mission in the run (human only). The body comes from --body, --file or stdin.
    New {
        title: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long, conflicts_with = "body")]
        file: Option<String>,
    },
    /// List the run's missions and where each stands.
    Missions,
    /// Read a mission.
    Mission { number: u32 },
    /// Move the base branch up to a done mission's branch.
    Finish { mission: u32 },
    /// Start an operator, or the commander, in the run.
    Start {
        operator: String,
        #[arg(long)]
        mission: Option<u32>,
    },
    /// End a position's session.
    Stop { position: String },
    /// List running sessions (every run's, without --run).
    Sessions,
    /// Print a session's screen.
    Screen { position: String },
    /// Press a key in a session's terminal: enter, esc, up, down or tab.
    Key { position: String, key: String },
    /// Message a position in the run.
    Send {
        to: String,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Add a note to the run log.
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
    /// Answer a question, by its entry number. The answer goes to whoever asked.
    Answer {
        question: i64,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// List questions with no answer yet.
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
    /// Show the run log.
    Log {
        #[command(flatten)]
        filter: FilterArguments,
        /// Keep showing new entries as they land.
        #[arg(long, short)]
        follow: bool,
    },
    /// Write the run log, or part of it, to a file.
    Export {
        #[command(flatten)]
        filter: FilterArguments,
        #[arg(long, value_enum, default_value_t = ExportFormat::Jsonl)]
        format: ExportFormat,
        /// The file to write; stdout without it.
        #[arg(long, short)]
        output: Option<String>,
    },
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
