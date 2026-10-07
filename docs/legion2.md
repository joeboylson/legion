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
- **Strategist:** an optional session beside the commander whose one job is
  speed. It looks at the deployment on a timer and suggests speed-ups to the
  commander. It never commands.
- **Part:** a piece of a mission's current step, split off by the commander
  so copies of an operator can work on it at the same time.
- **Escalation:** anything waiting on you: a question, a permission, a
  blocked mission, a decision an operator flagged, a suggestion.

## Run it

```sh
legion2 service install --claude ~/.local/bin/claude   # runs legion2d now and at every login
legion2 add ~/code/myapp                   # sets up .legion2/ if it has none
legion2 deploy myapp feature --name first  # starts a commander
legion2 new --deployment first "Add a login page" --body "…"
```

The app (`app/`) shows every folder, deployment, mission and operator, and
the log. Open an operator for its live terminal. It runs two ways, from the
same screens:

- **In a browser:** legion2d serves the built app (`app/dist`) at
  http://127.0.0.1:4610. The tab stays open through restarts: when legion2d
  restarts, it reconnects by itself; after rebuilding the app
  (`npm run build`, or `npm run watch` to rebuild on every change), reload
  the tab. Only this machine can reach it, and legion2d refuses requests from
  any page it didn't serve. Set `webPort` in the machine settings to move it,
  or `0` to turn it off (read when legion2d starts).
- **In its own window:** `npm run app` (Tauri) for development.

`legion2 service install` sets legion2d up as a launchd service (macOS): it
starts at login, comes back if it crashes, and writes its output to
`~/.local/share/legion2/legion2d.log`. Installing again replaces the service,
which restarts legion2d; `legion2 service remove` stops it for good. To run
it by hand instead: `legion2d --claude ~/.local/bin/claude`.

## Callouts

One-line heads-ups for everyone, like a kitchen calling "behind": a gotcha, a
slow or flaky command, a file not to touch. Anyone calls one out with the
`callout` tool (or `legion2 callout <text>`), at most 200 characters, one
line. Legion adds it to `.legion2/callouts.md` with the date and who said it,
commits that file, and tells everyone running as a heads-up that wants no
reply (a shared tool is announced the same way). Every session's instructions
list the newest 20. `callouts` (the tool, or `legion2 callouts`) reads them
all.

The toolbox and callouts go through Legion's own tools, so every session can
always read the toolbox and read and write callouts, whatever its
`operator.json` or permission mode allows.

## A folder's setup: `.legion2/`

Kept in git, so everyone working on the folder shares it.

- `legion.json`: `name`; `check`, a command run on a mission's work when its
  branch is replayed onto a moved base branch (such as `npm test`);
  `permissionMode` for every session; `clearAt` (see below); `strategist`
  (below).
- `operators/<name>/definition.md`: the operator's job, in plain words.
- `operators/<name>/operator.json` (all optional): `model`,
  `permissionMode`, `limit` (copies at once, default 1), `allowedTools`,
  `disallowedTools`, `clearAt`.
- `pipelines/<name>.yaml`: `operators`, `first`, and `decisions`. Each
  operator lists conditions and who's `next`: an operator, `done`,
  `commander`, or a list of operators who work at the same time (the
  commander waits for all of them).
- `tools/`: the team's toolbox (below).
- `callouts.md`: the team's callouts (below). Legion writes it.

## The strategist

Off unless `legion.json` turns it on:

```json
"strategist": { "enabled": true, "everyMinutes": 10 }
```

Legion starts it with the commander and wakes it every `everyMinutes`
(default 10) on its own; no one has to ask. It isn't woken while nothing is
going on: no one else busy, and no mission waiting, started or handed off. Each time, it reads the
missions, the log and the screens, looking for work that could run at the
same time, checks done twice, idle operators while missions wait, polling
and long turns. For each speed-up worth making, it sends the commander one
suggestion with its pros and cons, ending "the commander has the final
say". The commander takes or turns down each one and notes why, so it isn't
suggested again.

After each mission is finished, Legion asks it for a postmortem: what slowed
the mission down, what sped it up, what to do differently next run. The
newest postmortems, the commander's and the strategist's, from this run and
earlier ones in the folder, go into the next commander's and strategist's
instructions.

legion2d holds it to that: it can read the deployment, look at screens,
write postmortems and callouts, and send messages only to the commander. It can't start, stop, split,
finish, hand off or edit files. Turning it off in `legion.json` stops it
within 15 seconds; if it ends by itself, Legion starts it again (at most
once per 10 minutes).

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

- **Everyone, the strategist too:** `missions`, `mission_read`, `sessions`,
  `log`, `toolbox` (list the shared tools, or read one), `callouts`,
  `callout`.
- **The commander and operators:** `send`, `note`, `ask` (the human),
  `flag_decision`, `share_tool`, `suggest` (a mission, to the human).

`ask` stops the work until you answer. `flag_decision` doesn't: the operator
says what it chose and why, and carries on. The decision shows with your
escalations; answer it to change course (the answer goes back to the
operator), or dismiss it.
- **Operators:** `handoff`, `part_done`, `done`, `blocked`.
- **Commander:** `start` (optionally on a part), `split`, `stop`, `screen`,
  `finish`, `pause`, `resume`, `answer`, `postmortem`.
- **Strategist:** `screen`, `suggest_speedup`, `postmortem`.

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
  `~/.local/share/legion2/settings.json`, read each time they're needed, so a
  change needs no restart. (`webPort`, the app's browser address, is the
  one setting read only at start.)
- **Tells the commander when the team changes.** A commander is told its
  pipeline and copy limits when it starts. If the pipeline or an operator's
  `operator.json` changes while it runs, Legion sends it the new ones.
- **Restarts.** A commander that ends by itself is started again, at most
  once per 10 minutes. After legion2d restarts, each open deployment's
  commander comes back, then every operator that was on a mission, under the
  same name (`builder-3` stays `builder-3`). Those the machine has no room
  for yet start as room opens up. Operators that were only waiting, with no
  mission, aren't brought back; the commander starts them when there's work.
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
- `~/.local/share/legion2/legion2d.log`: legion2d's output, when it runs as
  a service.
- `~/Library/LaunchAgents/com.legion2.legion2d.plist`: the service.

## Building

```sh
cargo test --workspace            # Rust
(cd app && npm test && npm run doctor)
claude plugin test addon          # the add-on inside each session
cargo build --release -p legion2 -p legion2d   # then copy both to ~/.local/bin
legion2 service install --claude ~/.local/bin/claude   # restarts legion2d on the new build
```

A restart cuts every session off. Operators on missions come back, but they
lose what was in their conversation, so restart when little is running.
