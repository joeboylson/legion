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

- **legion2d** runs as a launchd service, installed 2026-10-07 from this
  branch (`legion2 service install --claude ~/.local/bin/claude`). It starts
  at login, comes back if it crashes, and writes to
  `~/.local/share/legion2/legion2d.log`. To update it: rebuild
  (`cargo build --release -p legion2 -p legion2d`), copy both to
  `~/.local/bin` (copy to a new name, then `mv`, so the running file isn't
  overwritten in place), then `legion2 service install` again. That restarts
  it, so ask first.
- **The add-on** loads from this repo's `addon/` folder, so changes to it
  reach every session started after the change, without a reinstall.
- **The app** in development: `cd app && npm run app` (Tauri, reloading on
  change). Debug builds can save snapshots of the screen
  (`LEGION2_SNAPSHOT_DIR`).
- Folders registered: `hello-legion2` (the test app, in a scratchpad that
  goes away) and `temp_plus_platform` (not in git). The `rogers-demo`
  deployment is open there, with only its commander running.

## Added 2026-10-07 (installed)

- `share_tool` and parts, from the last handoff.
- **The service** (`legion2 service install` / `remove`). legion2d now stops
  cleanly on SIGTERM too.
- **Restarts keep operators.** Legion records each running session in the
  folder's database; after a restart every operator that was on a mission
  comes back under the same name, as the machine has room
  (`session_restore.rs`). A fresh start after a full conversation keeps the
  name and part too.
- **No restart for settings.** `settings.json` is read each time; a running
  commander is told when its pipeline or an operator's copy limit changes
  (`team_changes.rs`).
- **`flag_decision`.** A non-blocking decision for the human: it shows with
  the escalations, and can be answered (the answer goes back to the
  operator) or dismissed. New entry kind `decision`; `legion2 flag` on the
  command line.

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

1. **Folders that aren't in git** (IMPROVEMENTS.md has the proposed fix).
   On hold to talk through: `temp_plus_platform` is 7.1 GB, with a 175 MB
   zip, an API key file, nested repos and a `worktrees/` folder, so a
   straight snapshot would copy all of that into history and every
   mission's checkout. Options so far: filter (.gitignore, a
   `.legion2/ignore`, a size cap, skip nested repos); or run missions there
   one at a time.
2. **A speed role**, or a regular speed check by the commander: to talk
   through.
3. Then: how missions work (to talk through).
4. A systemd unit for the service on Linux.
