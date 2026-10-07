# legion2

The new Legion: a background program, `legion2d`, runs a team of Claude Code
sessions for each folder you add. You drive it from the `legion2` command or
the Legion app. No tmux. The design and its reasons are in
[tauri-rewrite.md](tauri-rewrite.md); what to try next is in
[../IMPROVEMENTS.md](../IMPROVEMENTS.md).

## Words

- **Commander:** the lead session of a deployment. It routes work and is the
  only one that starts or stops sessions.
- **Operator:** any other session, named for its job (`planner`, `builder`).
  Copies of one operator are `builder`, `builder-2`, `builder-3`.
- **Pipeline:** who a mission goes to next, as a table in a YAML file.
- **Deployment:** one running commander and its operators, in one folder,
  on one pipeline.
- **Mission:** one task, written in a file that never changes. Only you
  create missions.
- **Part:** a piece of a mission's current step, split off by the commander
  so copies of an operator can work on it at the same time.
- **Escalation:** anything waiting on you: a question, a permission, a
  blocked mission, a suggestion.

## Run it

```sh
legion2d --claude ~/.local/bin/claude      # the background program
legion2 add ~/code/myapp                   # sets up .legion2/ if it has none
legion2 deploy myapp feature --name first  # starts a commander
legion2 new --deployment first "Add a login page" --body "…"
```

The app (`app/`, `npm run app` for development) shows every folder,
deployment, mission and operator, and the log. Open an operator for its live
terminal.

legion2d doesn't start by itself after a reboot yet; start it again by hand.

## A folder's setup: `.legion2/`

Kept in git, so everyone working on the folder shares it.

- `legion.json`: `name`; `check`, a command run on a mission's work when its
  branch is replayed onto a moved base branch (such as `npm test`);
  `permissionMode` for every session; `clearAt` (see below).
- `operators/<name>/definition.md`: the operator's job, in plain words.
- `operators/<name>/operator.json` (all optional): `model`,
  `permissionMode`, `limit` (copies at once, default 1), `allowedTools`,
  `disallowedTools`, `clearAt`.
- `pipelines/<name>.yaml`: `operators`, `first`, and `decisions`. Each
  operator lists conditions and who's `next`: an operator, `done`,
  `commander`, or a list of operators who work at the same time (the
  commander waits for all of them).
- `tools/`: the team's toolbox (below).

## How a mission moves

1. You add a mission. The commander starts the pipeline's first operator on
   it, in the mission's own git worktree on a `mission/<number>-<name>`
   branch.
2. Each operator does its step, commits, catches its branch up with the
   base branch, and hands off, naming who's next. The handoff goes to the
   commander, which starts that operator or messages it if it's already
   running. Operators stay idle after handing off, in case the work comes
   back.
3. The last operator reports the mission done. The commander finishes it:
   Legion moves the base branch up, replaying the commits if the base has
   moved on. If they clash, the mission goes back to its builder to fix just
   the clash, then finishes with no new round of checks. When it's
   finished, Legion ends the mission's sessions.

A big step can be **split into parts** (`split`, commander only). Each part
gets its own checkout branched from the mission's; a copy of the operator
works each part and reports it with `part_done`. Legion merges each part as
it's done and tells the commander once every part is in.

## Legion's tools inside each session

- **Everyone:** `missions`, `mission_read`, `sessions`, `log`, `send`,
  `note`, `ask` (the human), `share_tool`, `suggest` (a mission, to the
  human).
- **Operators:** `handoff`, `part_done`, `done`, `blocked`.
- **Commander:** `start` (optionally on a part), `split`, `stop`, `screen`,
  `finish`, `pause`, `resume`, `answer`, `postmortem`.

legion2d checks every call against the caller's role, whatever it sees.

## The team's toolbox

An operator that does something a second time by hand (a browser test, a
check, a setup step) makes it a tool and calls `share_tool`. Legion copies
it into the folder's `.legion2/tools/`, commits just that file, and tells
everyone running. Every operator's instructions list the toolbox with each
tool's first comment line.

## What Legion does on its own

- **Hands over a full conversation.** Past `clearAt` (default 30%, or
  `false` for never), a session is asked for a "Handoff:" note, then started
  again fresh, reading it. Claude Code's own summary is set 10 points later
  as a backstop.
- **Checks on long turns.** After 5 minutes of work, and every 5 after, the
  commander is asked to read that operator's screen.
- **Points out stalled work** to an idle commander: a handoff no one picked
  up after 30 seconds, or a waiting mission whose first operator has room.
- **Makes room.** When every copy of an operator is taken and one is idle,
  starting it again ends the idle one. It won't start a second copy on a
  mission or part one already holds; it says to message that one instead.
- **Limits and timeouts.** At most `maxBusySessions` busy sessions on the
  machine (default 6); an unanswered permission request is refused after
  `permissionTimeoutMinutes` (default 30). Both are in
  `~/.local/share/legion2/settings.json`.
- **Restarts.** A commander that ends by itself is started again, at most
  once per 10 minutes. After legion2d restarts, each open deployment's
  commander and each mission's holder come back.
- **Usage limits.** A session at its limit carries on once it resets.
- **Reports sessions stuck before they start** (likely a "trust this
  folder?" question), and passes on what an operator wrote on screen when
  it ended a turn without reporting.

## Where things live

- `~/.local/share/legion2/legion2d.sock`: how the command and the app reach
  legion2d.
- `~/.local/share/legion2/folders/<name>-<hash>/`: each folder's deployment
  log (`legion.db`, only ever added to), mission files and worktrees.
- `~/.local/share/legion2/prompts/`: each session's instructions, as Claude
  reads them.

## Building

```sh
cargo test --workspace            # Rust
(cd app && npm test && npm run doctor)
claude plugin test addon          # the add-on inside each session
cargo build --release -p legion2 -p legion2d   # then copy both to ~/.local/bin
```

Only restart legion2d when no mission is running: a restart cuts every
session off.
