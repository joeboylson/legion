# Handoff: legion2

Where the rewrite stands, for the next session. Updated 2026-10-08 (afternoon).

## Read first

- [docs/legion2.md](docs/legion2.md): how legion2 works.
- [docs/tauri-rewrite.md](docs/tauri-rewrite.md): the design, and why. Step 5
  (channels, then reaching another machine) is now half done: see below.
- [IMPROVEMENTS.md](IMPROVEMENTS.md): what to fix or try next.
- [TEST_CASES.md](TEST_CASES.md): end-to-end tests to run by hand.

## Rules

- Code follows `~/@/1_Projects/code_workspaces/temp_plus_platform/DEV_GUIDELINES.md`:
  one concern per file, named constants with one owner, guard clauses, fail
  loud, test behaviour, comments that only say why. Extremely functional.
- Docs are short and plain: no jargon, no made-up terms.
- The app uses shadcn components restyled to Slag (workbench mode). React
  Doctor (`npm run doctor`) must be clean. Never run Prettier: the repo has
  no config and it rewrites files in the wrong style.
- **After any change under `app/`, run `npm run build` in `app/`.** Joe uses
  the browser version (legion2d serves `app/dist` at http://127.0.0.1:4610).
- Commit or push only when asked.
- **Never restart legion2d without asking.** To update it: `cargo build
  --release -p legion2 -p legion2d`, copy both to `~/.local/bin` under a new
  name and `mv` them over, then `legion2 service install --claude
  ~/.local/bin/claude`. That restarts it and cuts every session; operators on
  missions come back fresh.
- No screen reading in the product: use Claude Code's built-ins (the add-on's
  events) and Legion's own commands. Reading a screen by hand to check
  something is fine.
- `cargo` is at `~/.cargo/bin/cargo` (not on PATH in Bash). App types:
  `PATH="$HOME/.cargo/bin:$PATH" npm run types` in `app/`.

## What's running

- **legion2d**: launchd service, built from this branch (with all the
  uncommitted work below), log at `~/.local/share/legion2/legion2d.log`.
- **Every session Legion starts gets `HINDSIGHT_NO_CAPTURE=1`**
  (`constants.rs`, set in `sessions.rs`), so the Hindsight session miner
  skips them.
- **A channel**: this machine hosts one on port 4620 and subscribes to
  itself (`127.0.0.1:4620`), so deployments here message each other the way
  two machines would. `legion2 channel status` / `log`. Settings in
  `~/.local/share/legion2/channels.json`, log in `channel-log.jsonl`.
- **Deployments**:
  - `v2-q4` (temp_plus_platform, pipeline `v2-q4-ship-it`): the fast V2 Q4
    ticket team. Use it for all Q4 tickets. Missions 10–26 are finished.
  - `test-3` (ref/hello-legion2) and `docs-1` (ref/hello-docs): the channel
    test pair. Idle.
- Test folders in `ref/` (git-ignored here): `hello-legion2` (auto mode),
  `hello-manual` (manual mode, every tool asks: good for blocker tests),
  `hello-docs` (a writer → editor docs team).

## Committed (`explore/tauri`)

- `031f82f` blockers from Claude Code's own signals.
- `3c36833` app: tabs, deployment tree, Blockers page.
- Channels (host/subscribe over TCP, directory, relayed messages, the
  channel log, the Channels tab), `legion2 rename`, `finish` without git,
  quieter commander/operator prompts, and `HINDSIGHT_NO_CAPTURE=1` on
  every session. Commander tools: `channel_deployments`,
  `channel_describe`, `channel_send`; CLI `legion2 channel
  open|close|subscribe|unsubscribe|status|log`.

## Blockers (how they work now)

- Detection, all from Claude Code built-ins: a permission question when the
  add-on's `tool.check` returns "ask" in a mode where a person answers
  (legion2d passes `LEGION_PERMISSION_MODE`); **halted** when a turn ends
  "aborted", or ends right after a call refused at the dialog. Claude's
  `PermissionRequest` and permission notifications never reach a hooks
  module: don't rely on them.
- Answers: Allow types "1"; Deny presses Esc then sends a carry-on message;
  Carry on sends a message. A halted session carries on by itself after 30 s.
  legion2d marks a session busy as soon as it passes on an answering key.
- Not solved: the "trust this folder?" start question (pressing keys blind
  is unsafe: the highlighted choice is "No, exit"), and a flagged command's
  blocker shows only Claude's reason, not the command.

## temp_plus_platform: the `v2-q4-ship-it` pipeline

Goal: PRs that need nothing from the human but the merge. Files in
`temp_plus_platform/.legion2/` (not a git repo; a backup of the pre-tuning
version is in this session's scratchpad only).

- `spec-reader` → `shipper` → `pattern-checker` → `opie-reviewer`.
- A fix to an open PR skips the brief; the pattern checker does a short
  pass (probes on the fix, remove-each-guard across the whole PR, spec and
  PR body, conventions on touched files); then one OPIE round.
- **spec-reader** writes a numbered rules checklist (limits, error codes
  and params, raise vs return, invariants, link lifecycle) from the node,
  its parent chain and its links.
- **shipper** builds to the checklist, attacks its own change first, keeps
  the whole PR body current, and ends it with a `## Changes for
  spec-graph` yaml block (built / rename / rule / error / question) for
  the spec-graph side to read in. New API label keys get en/fr wording in
  a linked v2-frontend PR (v2-core ADR-0022).
- **pattern-checker** runs hostile probe specs (every association writer,
  partly loaded records, nil everywhere, pasted Unicode, deletes, shared
  rows, bulk writes, migration from scratch), removes each guard to prove
  a spec fails, checks the spec item by item, then patterns. Repo-wide
  gaps are reported separately, not fixed in the PR.
- **opie-reviewer** checks a won't-do's facts against the spec graph.
- Commits: `/dev-workflow` Step 5 only (one-line conventional subject, no
  body, no trailer).
- Errors follow ADR-0022: the API sends codes and params, the frontend
  words them. The api's own validation text stays English on purpose.
- Toolbox: `spec-graph.sh` (read the spec graph: `ticket`, `node`,
  `chain`, `links`, `action`, `questions`, `search`); `wt-run.sh` gives
  each worktree its own test DB by default (never the shared core_test).
- Jira through `~/.local/bin/twg`.

## How the tuning went

After each batch, an independent adversarial review per PR (one subagent
each, read-only, probes in a throwaway worktree) found what the pipeline
missed, and the definitions were tuned from it. Four rounds on #200–#202:
must-fixes went from several per PR to none (#201) or one cross-repo
item (#200). Keep doing a review after a batch or two of new tickets until
they come back clean, then stop.

## Where the work stands

- **#200 (AD-401, with v2-frontend #195), #201 (AD-397), #202 (AD-402):
  merged, done in Jira.** Spec-graph updates from their PR bodies are
  with Joe.
- **Next tickets** (Tenancy, Brands & Access, AD-294), all unassigned, To
  Do: AD-393 (record which sign-on provider each sign-in used), AD-396
  (record each bulk sign-on import), AD-400 (block access for one
  person); then AD-398, AD-399.
- **To do, no ticket yet:** `.strip` misses invisible and look-alike
  characters (non-breaking and zero-width spaces, en dashes, bidi marks)
  in codes and names repo-wide (department.rb:19, cost_centre_match.rb:24,
  site.rb).
- **Open for later tickets:** Company contact phones (company.rb:57-67)
  aren't stored in E.164; unassigned country codes like +999 are accepted.

## Good next steps

1. Run AD-393, AD-396, AD-400 through `v2-q4`, then an independent review
   of each PR to confirm the tuning holds.
2. Channels: the receive/send switches ("ask me"), letting a team accept
   work from another, and reaching another machine.
3. Folders that aren't in git (IMPROVEMENTS.md), the trust-folder start
   blocker, and a systemd unit for Linux.
