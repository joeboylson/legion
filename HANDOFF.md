# Handoff: legion

Where Legion stands, for the next session. Updated 2026-10-16, early morning,
from the LEGION-DEV Claude session.

## Read first

- [docs/work/2026-10-16-next.md](docs/work/2026-10-16-next.md): **what to do
  next**, in order. Start here.
- [docs/legion.md](docs/legion.md): how Legion works.
- [docs/research/opencode.md](docs/research/opencode.md): everything learned
  about running OpenCode on the Spark (timeouts, thinking, prompt size,
  compaction, permissions).
- [docs/hooks.md](docs/hooks.md): the plan for one message shape, an action
  record, hooks and adapters, with Joe's decisions.
  [docs/work/2026-10-15-hooks.md](docs/work/2026-10-15-hooks.md) splits it
  into 18 draft missions.
- [LOCAL_AI_FEATURE.md](LOCAL_AI_FEATURE.md): the local AI plan, and
  [docs/work/2026-10-14-local-ai.md](docs/work/2026-10-14-local-ai.md) its
  drafts and decisions.
- [docs/migrating.md](docs/migrating.md): moving another machine from the old
  setup.
- [IMPROVEMENTS.md](IMPROVEMENTS.md), [TEST_CASES.md](TEST_CASES.md).

## Rules

- Code follows `~/@/1_Projects/code_workspaces/temp_plus_platform/DEV_GUIDELINES.md`:
  one concern per file, named constants with one owner, guard clauses, fail
  loud, test behaviour, comments that only say why. Extremely functional.
- Docs are short and plain: no jargon, no made-up terms.
- The app uses shadcn restyled to Slag (workbench mode). `npm run doctor`
  must be clean. Never run Prettier. After any change under `app/`, run
  `npm run build` in `app/` (legiond serves `app/dist` at
  http://127.0.0.1:4610).
- Commits: one-line conventional subject, no body, no trailer.
- **Never restart legiond without Joe's OK.** To update it: `cargo build
  --release -p legion -p legiond` in this repo, copy both to `~/.local/bin`
  under a new name and `mv` them over, then `legion service install --claude
  ~/.local/bin/claude`. That cuts every session; they come back fresh. Warn
  the V2-Q4 session before and tell it after.
- legiond loads `addon/` and `app/dist` from this folder, so a change there
  reaches running sessions as soon as it lands on `main`.
- `cargo` is at `~/.cargo/bin/cargo`.

## The rename (done 2026-10-14)

`legion2` became `legion` everywhere: crates, binaries, data
(`~/.local/share/legion`), `.legion/` folders, the service
(`com.legion.legiond`). The old tmux version is archived in
`~/.local/share/legion-tmux-archive-2026-10-14/`. `explore/tauri` was merged
into `main`; there's no `develop` branch any more.

## What's running

- **legiond** as a launchd service, installed 2026-10-15 15:06 from `main`
  at `a7d2246`. Log: `~/.local/share/legion/legiond.log`. Its open-files
  limit is 8,192.
- **Not installed yet** (on `main`, waiting on a restart Joe calls): dev
  missions 16–23. That covers OpenCode step lines in the log, marking a
  mission done (and a handoff to done counting as done), early-message
  retries, a handoff ending an OpenCode turn, the mid-turn handover at
  clearAt, and per-operator permissions for OpenCode. Mission 21 is on `main`
  but Legion shows it as blocked; see the next list, item 6.
- **The Spark** (`spark`, over Tailscale), vLLM:
  - `spark-big` on port 8000: `qwen3.8-27b`, 131K context.
  - `spark-fast` on port 8002: `qwen3.6-35b-a3b`, 131K context.
  - Prefix caching is on (about 89% reuse). Only about 2 spark-big requests
    run at once.
- **caffeinate** keeps the Mac awake until about 09:00 for V2-Q4's overnight
  run. Sleep stalls every session.
- **Deployments**:
  - `legion` (this repo): `dev` closed for the night, `hooks-find` closed.
  - `temp_plus_platform`: `v2-q4` is run by the separate **V2-Q4** Claude
    session (pipeline `v2-q4-foreman`: a spark-fast mapper and foreman, and
    spark-big task-builders).
  - `ref/hello-legion2`: `spark-test` (planner and reviewer on Sonnet,
    builder on spark-big). Idle.

## Legion builds Legion

This repo is a Legion folder itself (`.legion/` is git-ignored).

- Pipeline `ship`: builder → checker → done, with the planner only for
  missions too big for one session. Pipeline `find-work`: a work-finder
  turns a design doc into draft missions for Joe.
- Operators, up to 3 copies each: builder (Sonnet), checker (Opus: tries to
  break the work and proves it, with a throwaway Legion under a temp HOME
  only when sessions' behaviour changes), planner (Sonnet), work-finder
  (Opus). Each is told never to touch the live legiond.
- Missions build in the shared `~/.cache/legion-dev-target` and touch
  `crates/**/*.rs` before trusting a test. The finish check builds in its
  own `~/.cache/legion-finish-target`, so other missions' builds can't break
  it.
- Missions land straight on `main` when they finish.

## Local AI: where it stands

Operators can run on OpenCode with the Spark models. In `legion.json`,
`models` names each one (`harness`, `address`, `model`); an operator picks
one with `"model": "spark-big"` in `operator.json`. A folder copies
`templates/harnesses/opencode.json` into `.legion/harnesses/`. The commander
and strategist stay on Claude.

What works, and what each fix was:

- Sessions start from the template: OpenCode's own server on a free port,
  waiting until Legion's tools connect, then the first prompt.
- Busy or idle comes from that server; messages are posted to it.
- The 6-minute cutoff was Bun's 360-second idle limit; the template sets
  `BUN_CONFIG_HTTP_IDLE_TIMEOUT` to 1800.
- No thinking: the template sends `reasoning_effort: none` and
  `enable_thinking: false` (OpenCode otherwise sends its saved "medium").
- Several tool calls per step (`parallel_tool_calls`), and instructions to
  batch reads, start writing early and commit after each piece.
- A trimmed opening prompt (about 18.7K → 6.7K tokens in testing) and
  OpenCode's unused built-in tools switched off.
- The models' real limits, early compaction, and a 60% clearAt for
  OpenCode sessions (Claude stays at 30%).

Still rough, all in the next list: Spark timeouts at 41% and 72% context,
start-up reads and the log tool filling a fresh builder's context,
early messages lost, an operator's `done` closing a mission mid-build, and
a foreman that ran destructive docker and rails commands before per-operator
permissions were installed.

## Working with V2-Q4

The V2-Q4 session (temp_plus_platform) runs real missions on the Spark
models and reports problems to this session (LEGION-DEV) by message. Each
one became a mission here or a note in the next list, and V2-Q4 got a
workaround until the fix was installed. Tell it the template commit to copy
whenever `templates/harnesses/opencode.json` changes. Its copy is from
`a7d2246`.

## Ideas discussed, not started

- **Small trained models.** A local decision model for narrow choices (who
  goes next, approve or ask, stuck or working), trained on Legion's own
  decisions. Joe's plan: Supabase first, so decisions from every machine
  land in one place, with `decision.made` events designed for training
  (docs/hooks.md, decision 6). A proposal for a Claude session on the Spark
  (Unsloth, Qwen3-1.7B with LoRA, served as `legion-router` on port 8004,
  compared with spark-fast given examples) was drafted but not sent, and its
  examples file doesn't exist yet.
- **A v2-trained coder.** Train spark-fast first on v2's merged PRs and
  fix lists, served beside the plain model, then compare on new tickets.
- **Jev** (TypeSafe AI's decision model) fits the same jobs, but it's hosted
  only, so Legion's state would leave the machine.

## Next

[docs/work/2026-10-16-next.md](docs/work/2026-10-16-next.md), in order:
landing a mission must not freeze legiond, the two flaky tests, Joe's review
of the hooks drafts, the restart, then V2-Q4's notes.
