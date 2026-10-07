# Handoff: legion2

Where the rewrite stands, for the next session. Updated 2026-10-07.

## Read first

- [docs/legion2.md](docs/legion2.md): how legion2 works now.
- [docs/tauri-rewrite.md](docs/tauri-rewrite.md): the design, and why.
- [IMPROVEMENTS.md](IMPROVEMENTS.md): what to fix or try next, from the
  test runs and a live run in `temp_plus_platform`.
- [TEST_CASES.md](TEST_CASES.md): end-to-end tests to run by hand.

## Rules

- Code follows `~/@/1_Projects/code_workspaces/temp_plus_platform/DEV_GUIDELINES.md`:
  one concern per file, named constants with one owner, guard clauses, fail
  loud, test behaviour, and comments that only say why.
- Docs are short and plain: no jargon, no made-up terms.
- The app uses shadcn components, restyled to the Slag house style
  (workbench mode). React Doctor (`npm run doctor`) must be clean.
- Commit or push only when asked.
- **Never restart legion2d, or the app, without asking.** The installed
  copy runs real missions; a restart cuts every session off.

## What's running

- **legion2d** is installed: `~/.local/bin/legion2d` and `legion2`, built
  2026-10-06. It's started by hand: `legion2d --claude ~/.local/bin/claude`.
  It isn't a service yet, so it doesn't come back after a reboot.
- **The add-on** loads from this repo's `addon/` folder, so changes to it
  reach every session started after the change, without a reinstall.
- **The app** in development: `cd app && npm run app` (Tauri, reloading on
  change). Debug builds can save snapshots of the screen
  (`LEGION2_SNAPSHOT_DIR`); this session's helper for that was
  `scratchpad/snapshot-app.sh`, which won't exist in a new session.
- Folders registered: `hello-legion2` (the test app, in a scratchpad that
  goes away) and `temp_plus_platform` (not in git).

## Built but not installed

Committed with this handoff, tested, but not in the installed copy. To use
them, rebuild (`cargo build --release -p legion2 -p legion2d`), copy both to
`~/.local/bin`, and restart legion2d when no mission is running.

- **`share_tool`:** an operator puts a tool into `.legion2/tools/`. Legion
  copies it there, commits that one file and tells everyone running.
  Operators' instructions push them to build the toolbox.
- **Parts:** the commander `split`s a mission's step into parts, each in its
  own checkout; copies work on them at once and report with `part_done`;
  Legion merges each one and tells the commander when all are in. The app's
  mission dialog lists parts, and cards show `m12 · part 2`.

## Where things stand

- Steps 1 to 4 of the build order are done (see tauri-rewrite.md). Step 5,
  channels and reaching another machine, hasn't started.
- Two test runs on `hello-legion2`:
  - 10 operators added one at a time over 8 missions, which built a to-do
    app to version 1.0.0.
  - 10 missions at once, which worked out how operator copies behave.
  Both passed, with the fixes noted in IMPROVEMENTS.md.
- `temp_plus_platform` ran a live deployment (rogers-demo). Its 32 findings
  are under "From the rogers-demo run" in IMPROVEMENTS.md.

## Good next steps

In rough order of how much they'd help, all from IMPROVEMENTS.md:

1. Run legion2d as a service (a launchd agent), with a log file.
2. Give folders that aren't in git their own git history in Legion's data
   folder (the proposed fix is written up), so missions there get their
   own copies.
3. After a restart, bring back every operator that was on a mission, under
   the same names.
4. A speed role, or a regular speed check by the commander: parallel work,
   repeated checks, polling, long turns.
5. Re-read the machine settings and pipelines without a restart; tell the
   commander when the pipelines or operators change.
6. A way for operators to flag a decision for the human that isn't blocking.
