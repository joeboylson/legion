# legion

A hub-and-spoke multi-agent harness for [Claude Code](https://claude.com/claude-code),
built on tmux. One squad per project: a `commander` that partitions work and
routes everything between operators, who only ever talk back to it — never to
each other, except through explicitly declared pipelines.

Claude Code sessions can already address each other by name (`ListAgents` /
`SendMessage`), and that name is stable for the process's lifetime. Legion
pins it at launch instead of leaving it to the random default, and scopes a
squad to a project the way a Python venv scopes a package set — nothing works
until you activate.

Terminology is deliberately military — a squad, a commander, deployed
operators, missions, pipelines. No daemon, no database, no TUI: state is
plain files, readable with `cat` and diffable with `git log`.

## Requirements

- [Claude Code](https://claude.com/claude-code) (`claude` on your `PATH`)
- `tmux`
- `git`, `uuidgen` (both standard on macOS and most Linux distros)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/joeboylson/legion/main/install.sh | bash
```

Installs `legion` to `~/.local/bin` and its templates to
`~/.local/share/legion/templates`. Make sure `~/.local/bin` is on your
`PATH`; the installer will tell you if it isn't.

## Quickstart

```bash
cd some-project
legion init                              # creates ./.legion/ (roster + activate script)
source .legion/bin/activate              # (legion:some-project) prompt, gates the commands below
legion squad "ship the export feature"   # commander + planner + builder + reviewer
legion roster                            # who has checked in
legion capture builder                   # read its pane without attaching
legion stand-down builder                # kill its tmux window
deactivate                               # leave the shell, commands stop working again
```

`legion commander`/`deploy`/`roster`/`stand-down`/`list` all refuse to run
outside an activated shell; `legion init` is the only thing that works
unactivated. `deactivate` is a shell function defined by sourcing `activate`,
undefined again on `deactivate` itself — no standing alias, just `legion`
typed out each time.

## One squad per project

`.legion/` is meant to be committed:

- **Tracked** — `squad` (squad name + config mode, see below — fixed once at
  `init` so every clone gets the same address instead of re-deriving it from
  a directory name that might differ per machine), `log.md` (freeform shared
  context), `doctrine.md` (squad-wide conventions — spliced into every
  session's prompt, commander and operators alike, not just commander's),
  `ao` (Area of Operations: extra directories/repos this squad may access,
  one absolute path per line — can point outside this project entirely),
  `operators/<operator>/` (optional, one folder per operator — see below),
  `missions/{todo,active,done}/` (the backlog — see below), and `bin/activate`.
- **Machine-local** — `local/<position>` (one resumable session-id per
  position — `commander` and every operator — meaningless on another
  machine) and `roster/<operator>.md` (live status, one file per operator,
  kept on stand-down with its status flipped rather than deleted — a record
  of who's ever been on the squad). Excluded automatically by
  `.legion/.gitignore`, so nothing to remember in the project's own
  `.gitignore`.

Every position is durable, not one-shot: the first `legion commander` or
`legion deploy <operator>` creates a real conversation and remembers its id
in `local/<position>`; standing an operator down and deploying them again
reattaches to that same conversation instead of starting fresh. `claude`
errors if you reuse a session-id and errors if you `--resume` one that's
never existed, so Legion picks whichever is correct based on whether that
position has launched before — not something you need to think about.

`commander` owns the queue; operators hold no backlog of their own and sit
idle between missions, which is fine. Each mission is one markdown file (a
couple of header lines, then the mission body), named `0001-short-slug.md` so
order is visible from `ls` alone. `commander` writes new missions into
`todo/`, moves a file to `active/` when it hands that mission to someone, and
to `done/` when they report back — a plain `mv`, no JSON to get wrong, and a
history you can read straight out of `git log`. Operators check in with
`<squad>-commander` once at startup (`SendMessage`); its system prompt tells
it to upsert `roster/<operator>.md` accordingly and relay output between
operators itself rather than letting them talk directly.

## Operators

Optional, and not seeded — an operator works from just the mission text you
give `legion deploy`. `.legion/operators/<operator>/` is where an operator
gets something more permanent, and everything under it is per operator, not
squad-wide — a planner and a builder can have entirely different
capabilities:

- `definition.md` — persistent identity (expertise, tone, constraints),
  spliced into that operator's prompt on every `legion deploy <operator>`.
- `allowed-tools` / `disallowed-tools` — one Claude Code tool pattern per
  line (`claude --help` documents the syntax), passed as `--allowedTools` /
  `--disallowedTools`. `Edit(pattern)` covers all file-editing tools. This is
  how you'd stop a planner from touching code: deny `Edit(src/**)` (or
  whatever this repo's real source paths are) on `operators/planner/`, leave
  `operators/builder/` unrestricted. There's no pattern meaning "only this
  folder" — deny rules can't express "everything except X", only concrete
  matches, so list the actual code paths.
- `settings.json` / `mcp.json` — plain Claude Code settings/MCP files for
  that operator specifically (hooks, env, MCP servers — anything `--settings`
  / `--mcp-config` accept), nothing Legion-specific about their format.

`commander` can have an `operators/commander/` too (`definition.md` only adds
to its built-in prompt, doesn't replace it).

Two things stay squad-wide rather than per-operator: `ao` (every path in it
becomes `--add-dir` on every session, operator and commander alike), and
`LEGION_CONFIG_MODE` in `squad` — `layered` (default) has every session
inherit your normal user/project/local Claude config; `isolated` drops your
global `~/.claude` config for the whole squad (`--setting-sources
project,local`) so each operator's own `settings.json`, if it has one,
becomes the only settings source that matters for it.

Example planner/builder/reviewer operator folders (including a worked
`disallowed-tools` example) ship with the tool at
`~/.local/share/legion/templates/operators/` — copy and adapt, or ask any
Claude session (including `commander` itself) to help draft one from scratch.

## Pipelines

The one deliberate exception to "operators only message commander" —
declared sequences where adjacent stages hand off directly. A plain text
file at `.legion/pipelines/<name>` lists operators in order (an example
`planner`/`builder`/`reviewer` pipeline ships at
`~/.local/share/legion/templates/pipelines/`); a mission file gets a
`pipeline: <name>` header. When an operator with a pipeline mission
finishes, it hands off straight to the next stage instead of stopping at
`commander` — updates the mission file, deploys or messages the next
operator directly, and separately sends `commander` a one-line FYI, so
`commander` stays aware without brokering every step. The mission file is
still the audit trail regardless of who's talking to whom directly.

## License

MIT — see [LICENSE](LICENSE).
