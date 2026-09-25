# legion

Legion runs a team of [Claude Code](https://claude.com/claude-code) sessions
in tmux, one project at a time. One session leads. The others each have a
job, like planning, building or reviewing. They talk to each other directly
to get the work done.

Everything Legion knows lives in plain files in your project's `.legion/`
folder. There's no background service and no database. You can read any of
it with `cat` and track it with git.

## Words used here

Legion's commands and files use a few names. This is what each one means:

- **Squad** — the team of sessions working on one project. Each project has
  one squad, and its name comes from `.legion/squad`.
- **Commander** — the lead session. It decides what needs doing, hands out
  work, and settles disagreements. It doesn't tell the others how to do
  their jobs.
- **Operator** — any other session on the squad, named for its job:
  `planner`, `builder`, `reviewer`, or anything you define.
- **Mission** — one piece of work, written as a markdown file.
- **Pipeline** — a fixed order that a mission moves through, like planner,
  then builder, then reviewer.

Each session gets a fixed address, `<squad>-<name>`, for example
`myapp-builder`. Claude Code sessions can already message each other by name
(`ListAgents` / `SendMessage`). Legion sets that name when it starts the
session, so the others can always find it.

## Requirements

- [Claude Code](https://claude.com/claude-code), with `claude` on your `PATH`
- `tmux`
- `git` and `uuidgen` (both come with macOS and most Linux systems)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/joeboylson/legion/main/install.sh | bash
```

This puts `legion` in `~/.local/bin` and its example files in
`~/.local/share/legion/templates`. If `~/.local/bin` isn't on your `PATH`,
the installer tells you.

If you have a clone of this repo, run `./install.sh` from it instead. It
installs the clone's own files, including any changes you've made. Run it
again after each change to reinstall.

## Quickstart

```bash
cd some-project
legion init                              # creates ./.legion/
source .legion/bin/activate              # turns on legion commands in this shell
legion deploy                            # roll call: start everyone, no work yet
legion squad "ship the export feature"   # or: start commander, planner, builder, reviewer with a goal
legion roster                            # who has checked in
legion list                              # this squad's running sessions
legion grid                              # switch to the squad's tmux window
legion attach builder                    # switch to builder's tile
legion capture builder                   # print builder's screen without switching
legion stand-down builder                # stop builder and close its tile
legion stand-down                        # stop the whole squad
deactivate                               # turn legion commands off again
```

`legion src` prints the address of this repo on GitHub.

Every command except `legion init` and `legion src` refuses to run until you've run
`source .legion/bin/activate` in that shell. That keeps you from
starting sessions in the wrong project. `deactivate` turns it off again.

## Roll call

`legion deploy` with nothing after it starts the commander and every
operator that has a folder in `.legion/operators/`. Nobody gets any work.
Each operator messages the commander with its name and a line or two on
what it does. Once they've all answered, the commander prints the list in
its window and waits for you to give it something to do.

Use this to check that your operators are set up the way you expect before
you hand out real work.

## Seeing everyone at once

Every session in a squad runs in one tmux window, named after the squad,
with a tile for each session. The label at the top of each tile shows whose
it is. Legion adds a tile when a session starts, removes it when the session
stops, and re-arranges the tiles to fit.

If you run Legion inside tmux, the window opens in your current tmux
session. Outside tmux, Legion starts a separate tmux session named after the
squad.

`legion grid` switches you to that window from anywhere, or attaches to it
if you're outside tmux. `legion attach <name>` does the same and selects
that session's tile.

## What's in `.legion/`

Commit `.legion/` with your project. Most of it is meant to be shared:

- `squad` — the squad's name and default config mode (see below). Legion
  writes the name once at `init`, so every clone uses the same addresses.
- `log.md` — shared notes and decisions. The commander reads it at startup
  and keeps it up to date.
- `doctrine.md` — rules for everyone on the squad. Legion adds it to every
  session's instructions.
- `ao` — extra folders every session may read and write, one full path per
  line. They can be outside the project.
- `operators/<name>/` — optional setup for each operator (see below).
- `pipelines/<name>` — optional fixed orders (see below).
- `missions/todo/`, `missions/active/`, `missions/done/` — the work queue.

Two folders only make sense on your machine, and `.legion/.gitignore` keeps
them out of git:

- `local/` — the Claude Code session ID for each position. Legion uses it
  to reopen the same conversation next time.
- `roster/` — one file per operator saying whether it's running. Legion
  marks an operator as stood down rather than deleting its file, so the
  folder also records everyone who has ever been on the squad.

### Folder trust

The first time Claude Code opens a folder, it asks whether you trust it.
A session waiting on that question never starts working, so Legion answers
it ahead of time. Before starting any session, Legion marks the project
folder as trusted in `~/.claude.json` (or `$CLAUDE_CONFIG_DIR/.claude.json`).
This is the same setting Claude Code saves when you answer yes yourself.
It needs `jq`. Without `jq`, Legion prints a warning and you answer the
question in each tile.

### Sessions pick up where they left off

The first time you start the commander or an operator, Legion creates a new
Claude Code conversation and saves its ID in `local/`. If you close that
operator and start it again, Legion reopens the same conversation instead
of starting fresh. You don't need to manage this.

### How work moves

The commander owns the work queue. Each mission is one markdown file with a
few header lines and then the description. Files are numbered, like
`0001-short-name.md`, so `ls` shows them in order. The commander writes new
missions into `todo/`. It moves a file to `active/` when it hands the
mission to someone, and to `done/` when they report back. A move is a plain
`mv`, so `git log` shows the history.

Each operator messages the commander every time it starts, including when
Legion reopens an earlier conversation, and again when it finishes a
mission. In between, operators talk to each other directly when
they need help. The commander doesn't pass those messages along. An
operator with nothing to do waits, and that's fine.

### One worktree per mission

Operators share one project folder, so a paused mission can leave
half-finished edits where the next mission's work goes. To keep missions
apart, each mission that changes code gets its own git worktree: a
separate checkout of the project, on its own branch, inside
`.legion/worktrees/`.

- `legion mission start <number>` creates the worktree and a
  `mission/<name>` branch from whatever branch the project is on. It adds
  `worktree:`, `branch:` and `base:` lines to the mission file.
- Operators do that mission's reading, editing, testing and committing
  inside the worktree. Mission files, the roster and the log stay in the
  project's own `.legion/`.
- `legion mission finish <number>` moves the base branch up to the
  mission branch and deletes the worktree. It only does this when the base
  branch hasn't moved on since. If it has, the builder rebases the mission
  branch first, so Legion never merges anything for you.

The commander runs both commands as part of handing out and closing
missions. The project has to be a git repo.

## Setting up an operator

An operator works fine with nothing but the mission text you give it. To
give one a lasting setup, create `.legion/operators/<name>/` with up to two
files. Each applies to that operator only, so a planner and a builder can
have completely different setups.

- `definition.md` — who the operator is: what it's good at, how it should
  work, what it must not do. Legion adds it to the operator's instructions
  every time it starts.
- `operator.json` — every setting, in one file:

```json
{
  "model": "sonnet",
  "configMode": "isolated",
  "scale": 2,
  "allowedTools": ["Edit(**/app/**)", "Bash(uv run pytest:*)"],
  "disallowedTools": ["Bash(git push:*)"],
  "settings": { "env": { "LEGION_OPERATOR": "builder" } },
  "mcpServers": { "fetch": { "command": "uv", "args": ["tool", "run", "mcp-server-fetch"] } }
}
```

All the keys are optional:

- `model` — `opus`, `sonnet`, `haiku` or a full model ID, passed to
  `claude` as `--model`. Leave it out to use `LEGION_MODEL` from
  `.legion/squad`, or Claude Code's own default if that's blank too.
- `configMode` — `layered` or `isolated` (explained below). Leave it out to
  use the squad's `LEGION_CONFIG_MODE`.
- `scale` — lets the operator run as several copies at once: `true` for up
  to 5, or a number. See below.
- `allowedTools` and `disallowedTools` — Claude Code tool rules, passed as
  `--allowedTools` and `--disallowedTools`. `claude --help` explains the
  format. Allowed tools run without asking you for permission first.
  Disallowed tools are blocked. `Edit(pattern)` covers every file-editing
  tool.
- `settings` — a normal Claude Code settings object (hooks, environment
  variables), passed as `--settings`.
- `mcpServers` — MCP servers for this operator, keyed by name, passed as
  `--mcp-config`.
- `$comment` — notes for people; Legion ignores it.

Before starting an operator, Legion checks its `operator.json` against the
rules in `templates/operator.schema.json`. If anything is wrong — a key it
doesn't know, a model that isn't a word, a server with no command — it lists
every problem and doesn't start the operator. Run `legion check` to check all
of them yourself. Point your editor at the same schema file to get
suggestions while you type.

Squads set up before `operator.json` keep working with separate files
(`model`, `config-mode`, `scale`, `allowed-tools`, `disallowed-tools`,
`settings.json`, `mcp.json`). If an operator has an `operator.json`, Legion
uses only that.

To stop a planner from editing code, list your code folders in its
`disallowedTools`, for example `Edit(src/**)`, and leave the builder's
empty. You can't write "block everything except this folder." Block rules
only match the paths you list, so list each code path you want blocked.

The commander can have an `operators/commander/` folder too. Its
`definition.md` adds to the commander's built-in instructions rather than
replacing them.

Example planner, builder and reviewer folders come with Legion, in
`~/.local/share/legion/templates/operators/`. Copy one and change it, or ask
any Claude session, including the commander, to help you write one.

### Running several copies of an operator

Give an operator a `scale` setting to let the commander run more than one
copy of it, for example several builders working on separate missions at
the same time. The first copy is plain `builder`; the others are
`builder-2`, `builder-3` and so on, up to the number you set (5 if it's
`true`). Every copy uses the same folder, so they share one
definition, one set of tool rules and one settings file.

- `legion scale up builder "<mission>"` starts the next free copy, with
  that mission, in a new tile.
- `legion scale builder` shows which copies are running.
- `legion stand-down builder-2` stops one copy.

The commander does all of this on its own when independent missions are
waiting. Each copy works on one mission at a time, in that mission's own
worktree. When a pipeline hands off to an operator with copies, it goes to
a copy with no mission open, or asks the commander for another one.

### Config mode

Config mode decides whether a session also loads your own Claude Code
settings from `~/.claude`:

- `layered` (the default) — the session loads your normal user, project and
  local settings, plus the operator's own `settings.json`.
- `isolated` — the session skips your `~/.claude` settings
  (`--setting-sources project,local`). The operator's own `settings.json`
  is then the only settings file that matters.

`LEGION_CONFIG_MODE` in `.legion/squad` sets the default for the whole
squad. An operator's `configMode` overrides it for that operator.

## Pipelines

Operators message each other whenever they need to. You don't have to set
that up. Use a pipeline when the same steps happen in the same order every
time and you want that route written down.

A pipeline is a Markdown file at `.legion/pipelines/<name>.md` holding a
[Mermaid flowchart](https://mermaid.js.org/syntax/flowchart.html). Each box is
a step named after the operator who does it. A diamond is a yes-or-no
question, written as a statement that's either true or false, and its arrows
are labelled `yes` and `no`. A rounded `done` box ends it. Legion never runs
the flowchart. The operators read it to work out who goes next, and GitHub
draws it as a diagram.

```mermaid
flowchart TD
    planner[planner: write the specs and the plan] --> builder[builder: build it and commit]
    builder -->|the plan is wrong or unclear| planner
    builder --> reviewer[reviewer: review the change]
    reviewer --> approved{the change is approved}
    approved -->|yes| done([done: report to commander])
    approved -->|no| builder
```

`legion init` writes the full rules to `.legion/pipelines/README.md`. It also
adds a note to the project's `CLAUDE.md`, so any Claude session you ask to
write a pipeline uses the same format. The example above comes with Legion in
`~/.local/share/legion/templates/pipelines/feature.md`. To send a mission
through a pipeline, add a `pipeline: <name>` line to the mission file's header.

When an operator finishes its step, it answers any question that follows and
passes the mission straight to the next operator. It updates the mission
file, starts or messages the next operator, and sends the commander a
one-line note. The mission file keeps a record of every step.

## License

MIT — see [LICENSE](LICENSE).
