# Legion rewrite: a Tauri app

> Writing rule for this doc: keep it short and plain. No jargon, no made-up
> terms.

Being built. Branch: `explore/tauri`. See "Build order" for where it's at.

## Goals

- Drop tmux. A background program on the host starts and runs the Claude sessions.
- Works on a host with no screen.
- Another machine can do everything, even when the host has no screen.
- Each repo still carries its own setup, so other devs can use it.
- Session output goes in its own folder, not the repo's `.legion` folder.
- Everything Legion does today still works.

## Terms

- **Squad:** the commander plus the operators named in a pipeline.
- **Deployment:** one working copy of a squad in one folder, on one pipeline. It
  has its own commander, operators, deployment log and ID. A deployment works through
  many missions.
- **Mission:** one task, written in a file that never changes. A deployment's
  operators carry it through the pipeline.

## Decided

**Starting a deployment takes a folder and a pipeline.** A name is optional.

**A deployment stays open, with its commander.** The commander stays up the whole
time, so you can message it even when nothing is going on. It starts
operators as missions arrive. An operator stays open, idle, until its
mission is finished, in case the work comes back to it; then Legion ends
it. An idle copy is ended sooner when new work needs its place. A session
that's waiting uses no usage.

**When the work runs out, the commander writes a short postmortem:**
gotchas and learnings from that stretch of work, as a deployment log entry. A new
commander session reads the recent postmortems first.

**No end-of-day wrap-up.** Pausing every mission does the same job.

**Sessions run as normal Claude Code.** The background program runs the
usual `claude` in a terminal it owns, instead of in tmux. The app shows that
terminal, on the host or on another machine. You can still type into the
session and use everything in Claude Code.

**Later, maybe: a data-only mode.** Claude Code can also run without a
screen and send back each step as data. That's cleaner, but there's no
screen to type into, so the app would have to rebuild Claude's own menus
and prompts. We'll leave it for later and build so it can be added: one part
of the program talks to sessions, and the rest doesn't care how.

**Sessions don't load skills from claude.ai.** Skills you turn on in
claude.ai now load in every session. Legion starts its sessions with
`"syncClaudeAiSkills": false`, so each one gets only the skills chosen for it.

**A Legion add-on runs inside each session.** Claude Code lets you add your
own code inside it. The background program loads Legion's add-on into every
session it starts (`--plugin-dir`). The add-on can:

- talk to the background program, over a local socket or by running a command
- send a message into the session as if typed, once the session is free
- see when a session starts and finishes work, so we know when it's idle
- see when a tool is about to ask for permission, and answer it, so the
  question can be answered from the app on any machine
- see usage and limit changes as they happen

So the background program no longer reads the screen or types into it. The
real Claude screen is still there to look at and type into.

Not tested yet. This comes from reading the docs (Claude Code 2.1.289).

If a Claude Code update breaks it:

- Legion checks the Claude Code version before starting a session, and
  refuses below the lowest version it supports, saying why.
- The add-on reports its own version when it connects. If the program
  doesn't recognize it, the session still runs, but the app marks it
  "can't see this session's state."
- There's no fallback to reading the screen.

**Other machines reach the host through SSH or a private network.** The
host only accepts connections from itself. To use it from somewhere else,
you connect over SSH or a private network like Tailscale, which already
handle logins and encryption. Legion builds no security of its own. One
connection point serves the app, the live terminal view, and messages from
other squads.

**How the app and the program talk.** One connection that stays open and
carries messages both ways (a WebSocket), at one local address. The app, the
`legion` command and Legion's tools inside Claude all use it.

- Each command carries an ID, and the reply carries the same ID: done, or
  failed and why.
- Live updates go to everyone connected without being asked: new deployment log
  entries, session changes, terminal output.
- Message shapes are defined once in Rust. The app's TypeScript versions
  are generated from them, so the two can't drift apart.

**The program starts when the machine starts, and brings every open deployment
back.** Every session stops if the program stops, since it owns their
terminals. On start, each commander starts in a fresh session and reads the
deployment log. Operators on missions that were in progress start fresh on those
missions. A deployment log entry records the restart.

**Anyone who can connect to a host is "the human" there.** No accounts and
no names: whoever gets in can do everything.

**The background program is its own Rust program.** It runs without the
app, so a host with no screen runs only that. The app and the `legion`
command both talk to it, the same way on the host or another machine. Rust
matches Tauri's back end, so the two share code, and it installs as one
file.

**The repo keeps only the setup.** Settings, operators, pipelines and
doctrine stay in `.legion` and go in git, so other devs can use them.
`bin/` goes away; the Rust program replaces it.

**Everything else lives outside the repo,** one folder per repo, such as
`~/.local/share/legion/<repo>/`: missions, the log, messages, the roster,
activity, channel files and worktrees. Missions and the log are not shared:
each dev has their own.

**A new dev installs once and starts with no work.** The app comes with the
background program; a host with no screen installs just the program. Add the
folder in the app, and the program finds the setup in `.legion` and starts
that repo with no missions, log or messages.

**One screen for every Legion.** In the app you add a folder. If it has no
Legion yet, the app sets one up. Each folder shows in one list, next to the
others, and you work with all of them from that one screen.

**One background program per machine.** It runs every Legion on that
machine. The app connects once per machine and can list several machines,
each with its own Legions. A folder you add from another machine is a path
on the host, not on the machine you're typing on. The folder must exist on
the host.

**Channels: only linked Legions can message each other.** Linking should be
easy in the app.

- A channels screen lets you open a new channel or subscribe to one.
- Each channel shows whether it's up. The two ends check on each other about
  every 10 seconds.
- A channel can be open with no squad running.
- From a squad's screen, you can add it to a channel.
- When a squad joins a channel, it gets a unique ID, so two squads with the
  same name don't get mixed up.
- Once it joins, its commander sends the channel a short description: what
  the team is and what it can do.
- Each deployment in a channel has two switches in the app, both starting at "ask
  me":
  - **Send:** whether the commander can message other squads: off, ask the
    human each time, or free.
  - **Receive:** whether messages from other squads reach the commander, or
    wait in "Escalations" until the human passes them on.

**A squad can run several times at once, one pipeline per deployment.** Each deployment
has its own commander, operators, sessions, messages, missions and log. A
mission belongs to a deployment, so to a squad and a pipeline. Deployments share the setup
in `.legion` and nothing else. Each deployment gets its own ID, also used for
channels. Deployments don't clash over code, because each mission already gets its
own git worktree, as it does today.

**The main screen.** We'll adjust it as we build. It starts from today's web
dash.

```
┌───────────────┬──────────────────────────────────────────┐
│ Escalations (3) │  myapp / feature deployment                      │
│               │  ┌ commander ● working  ┐ ┌ builder ● idle ┐│
│ ▾ host-1      │  │ live terminal on click│ │ ...            ││
│   ▾ myapp     │  └───────────────────────┘ └────────────────┘│
│     ● feature │  Missions   Questions   Log   Activity       │
│     ○ bugfix  │                                              │
│   ▸ other     │                                              │
│ ▸ laptop      │                                              │
│ Channels      │                                              │
└───────────────┴──────────────────────────────────────────┘
```

- Left: machines, then folders, then deployments, each with a dot showing if it's
  up. Channels at the bottom.
- "Escalations": one list of everything waiting on you, across every deployment and
  machine: questions, permission requests, ended sessions, usage limits.
- Main: the chosen deployment's positions as cards. Click one for its live
  terminal. Tabs for missions, questions, log and activity.

**Everything can be set up in the app,** and it should flow well from one
step to the next. No editing files by hand to get something done.

**How the app is built.**

- React, with shadcn for as much of the screen as possible.
- Fonts and colors from Joe's house style (`/slag-design-system`).
- No Vite dev server. The Tauri app always loads the built Vite files.

**Settings the app changes go in the same `.legion` files.** Git stays the
one place the shared setup lives, and the app shows any change made by hand.
Settings for one machine only go in the outside folder.

**Pipelines become a table of decisions that Legion reads.** Each operator
lists the decisions it can make and where each one sends the work:

```yaml
operators: [builder, reviewer, finalizer]
decisions:
  reviewer:
    - condition: approved
      next: finalizer
    - condition: needs_work
      next: builder
    - condition: blocked
      next: commander
```

The commander and operators read the table to decide who goes next; Legion
itself doesn't route work. The app draws the table as a graph you can edit,
and while a deployment is going, the graph shows who's working and where each
mission is.

**A mission is one file that describes a task, and it's never edited.**
Mission files stay in one folder and never move. Where a mission stands
(waiting, started, handed off, done) comes from its latest deployment log entry,
not from `todo/`, `active/` and `done/` folders.

**An operator starts a fresh session for each mission.** It reads the
mission file and that mission's deployment log entries, so it starts with only
what the mission needs. A session that fills up partway through a mission
is still cleared, as it is today.

**A mission's files come from git.** The list is the files changed on the
mission's branch since it started. The mission's page in the app shows the
list and the changes. Operators don't report files themselves.

**Finishing a mission's branch.** When a mission is done, the commander
moves the main branch forward to include its commits, on its own, as long
as the main branch hasn't changed since the mission started. If it has
changed, Legion replays the mission's commits on top of the new main
branch. If nothing clashes and the checks still pass, the mission finishes
on its own. A clash or a failing check sends it to "Escalations" to wait for
the human.

The checks are an optional command per folder, set in the app, such as
`npm test`. Legion runs it in the mission's worktree after the replay. A
folder with no check command (docs work, say) counts a clean replay as
passing.

**A permission request no one answers is refused after 30 minutes.**
Until then it waits in "Escalations". Once it's refused:

- The operator works closely with the commander on a way around it.
- The commander allows at most 3 attempts at a workaround.
- After that, the mission goes on as best it can without that thing, and the
  gap is written into the mission's result. If the mission can't go on
  without it, it's reported blocked.

**A blocked mission goes to the commander first.** The commander tries
what it can, such as asking the operators. If it can't unblock it, the
mission goes to "Escalations" with one line on what's blocking it and what
would unblock it. The deployment carries on with its other missions. Once the
human deals with the cause, they resume the mission, which starts fresh
with that note. The commander doesn't ask other squads for help unless the
human has allowed it.

**One mission can be paused and resumed.** Pause adds a "paused" entry to
the deployment log. The operator on it stops at the next good point, writes a short
note on where it got to, and its session closes. Resume adds a "resumed"
entry, and the commander starts the right operator in a fresh session, which
reads the mission file and its log entries, note included.

**The commander manages the work.** The operators and the commander are the
smart part; Legion is not. The commander reads the pipeline table, and
starts, scales and stops operators as work moves, as it does today. It also handles anything sent back as
"blocked", and talks to other squads. Legion only does what the commander
or the human asks, and records it in the deployment log.

**Each operator has a limit on copies running at once,** set in the app,
starting at 1. The commander can't start more than that. Missions past the
limit wait in line, and the app shows the queue. The commander is told each
operator's limit, so it plans its work around it.

**A machine-wide limit on sessions running at once,** set in the app,
starting at 6. Only busy sessions count: a commander that's just waiting
takes no place, a busy one does. Past the limit, operators wait in line the
same way, and the commander is told. A commander never waits in line. The app shows how much of the plan is used.

**A new mission goes to a deployment the human picks.** If the folder has no deployment
on the pipeline wanted, the app offers to start one there. A mission
belongs to exactly one deployment and never moves.

**Only the human creates missions,** or a Claude session that isn't part of
a squad. Operators and the commander can suggest a mission. The suggestion
shows in "Escalations", and the human decides whether to create it.

**What operators can do with Legion's tools.** A default list; the
commander and the human get more.

Operators can:

- read their mission file
- read their deployment's log, including their mission's entries and notes
- add entries to the deployment log: progress, a note, a pause note
- hand off to the next operator, as the pipeline table says
- report a mission done, or blocked (which sends it to the commander)
- ask the human a question
- suggest a mission
- message other positions in their deployment
- see which positions are running in their deployment
- list their deployment's missions and where each stands
- see their mission's changed files and the changes

Operators can't:

- start, stop or add operators (commander only)
- create missions (human only)
- pause or resume missions
- answer questions
- read another position's terminal
- change pipelines, operator settings or copy limits
- open, join or post to channels, or message other squads
- add folders, or start deployments or squads
- change app settings
- read other deployments or other Legions
- change or delete log entries (nobody can)

The commander can do everything operators can, plus:

- start, stop and add operators, within each operator's limit
- pause and resume missions
- read any position's terminal in its deployment
- pass the human's answers back to whoever asked
- open, join and post to channels, and message other squads, once the human
  has allowed it (see Channels)

The commander still can't create missions, change pipelines, operator
settings, copy limits or app settings, add folders, or start deployments or squads.

**One deployment log holds everything that happens in a deployment.** Each entry is one
event, such as "builder: done, changed `src/main.ts`." Entries are only ever
added, never changed or removed. Only Legion writes to the log; operators
add entries through a tool. The app shows each entry as it lands.

An entry about a mission carries that mission's number. A mission's history
is the deployment log narrowed to those entries; there is no separate mission log.
Entries for the whole deployment, like decisions or the end-of-day handoff, carry
no mission number. This replaces today's `log.md`.

Messages between operators are deployment log entries too, not separate inboxes.
A handoff like "builder → reviewer: ready for review" is one entry, and the
add-on delivers it to the reviewer's session.

Questions for the human are entries too, and so are the answers. "Needs
you" shows every question with no answer yet. This replaces today's
`questions/` folder.

The deployment log is stored in SQLite, one database per Legion, in its outside
folder, holding all of its deployments. Removing a Legion removes its data. An
export writes all of it, or only what
you filter for (a mission, an operator, a time span), to a file.

**The app, the `legion` command and Legion's tools inside Claude can all
do the same things.** The same program sits behind all three, so none of
them is ever the only way to do something. A host with no screen can be run
entirely from the command. The commander and operators use Legion's tools
inside Claude heavily, not the command.

**No upgrade path from today's Legion.** Hardly anyone else uses it, so
repos start fresh on the new version.

**Build order.** Each step works on its own before the next starts.

1. The background program running one session: a hidden terminal with the
   add-on, which reports idle, busy or waiting on permission, and takes a
   message in. **Done.**
2. The `legion` command and the deployment log: a full run on a host with no screen.
   **Done**: folders, deployments, missions in their own worktrees, finishing
   branches, the deployment log, questions, export, permissions per role, the
   machine-wide limit, the permission timeout, restarts, usage limits, and
   sessions stuck before they start. Tested end to end with Haiku sessions.
3. Legion's tools inside Claude: the commander and operators drive the deployment.
   **Done**: every session gets Legion's tools (served by `legion2 mcp`),
   each role sees only its own, and legion2d still checks every call.
4. The app: the main screen, live terminals, "Escalations". **Done** for this
   machine: Tauri with React and shadcn in the house style, connected to
   legion2d. Debug builds can save what the screen looks like
   (`LEGION2_SNAPSHOT_DIR`) and open a deployment at start (`LEGION2_OPEN`).
   Since then, from test runs (see `docs/legion2.md` for how each works):
   operators idle between handoffs, handing over a full conversation
   (`clearAt`), 5-minute check-ins, stalled-work reminders, steps that send
   work to several operators at once, a shared toolbox (`share_tool`),
   clashes at the finish going back to the builder, splitting a mission into
   parts, and in the app a live graph of the squad. Installed with
   `~/.local/bin/legion2d` since 2026-10-06; what's built but not installed
   yet is in `HANDOFF.md`.
5. Channels, then reaching another machine over SSH or a private network.

**While it's being built, it stays apart from today's Legion.** Everything
gets its own name until the switch:

| Today's Legion                  | The new one while it's built |
|---------------------------------|------------------------------|
| `legion` command                | `legion2`                    |
| `~/.local/share/legion/`        | `~/.local/share/legion2/`    |
| `.legion/` in a repo            | `.legion2/`                  |
| Legion's tools in Claude: `legion` | `legion2`                 |

- The name is one setting in the Rust code, so switching is one change.
- Testing happens only in throwaway repos, never one a current squad works in.
- The new program only accepts connections from its own machine and never
  touches tmux, so running squads can't be affected.
- When it's ready, the names go back to `legion` and the old version is
  removed.

## How today's features carry over

| Today, through tmux                     | In the new app                     |
|-----------------------------------------|------------------------------------|
| Types messages into a session           | The add-on sends them in           |
| Types `/clear` for a fresh start        | Starts a fresh session that reads a handoff note (`clearAt`) |
| Picks up an old session by its ID       | Same                               |
| Starts sessions with their own settings | Same                               |
| Reads the screen to see what a session is doing | The add-on reports it      |
| Finds sessions that ended               | Knows at once, since it started them |
| Tells a session to carry on once its usage limit resets | Same      |
| Restarts a commander that ended by itself, at most once per 10 minutes | Same |

## Open

**Not covered yet:** channels and reaching another machine (step 5), and
everything in `IMPROVEMENTS.md`.


## From the Claude Code news (Oct 2026)

- A session that hits its 5-hour limit may finish its current task before it
  stops.
- Sonnet 5.5 is a cheaper choice for simple, clear jobs.
