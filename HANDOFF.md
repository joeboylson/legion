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
- Folders registered: `temp_plus_platform` (not in git), where the
  `rogers-demo` deployment is open with only its commander running.
  `hello-legion2`, the test app, lived in an old session's scratchpad and is
  gone; legion2d skips it. Make a new test folder outside the scratchpad
  (and turn the strategist on in its `legion.json`) for the next test run.

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

## Added 2026-10-07, later (installed)

- **The strategist** (`strategist_checks.rs`): an optional session that
  wakes on its own timer and sends the commander speed-up suggestions with
  pros and cons; the commander has the final say. On in `hello-legion2`,
  every minute, for testing. First run: it spotted a handed-off mission no
  one had picked up, and the commander took the suggestion.
- **Postmortems after each mission** from the strategist; the newest six
  from every run in the folder go to the next commander and strategist.
- **Callouts** (`callouts.rs`): one-line heads-ups in `.legion2/callouts.md`,
  and the `toolbox`, `callouts` and `callout` tools for every session.
- **The app in a browser**: legion2d serves `app/dist` and its WebSocket at
  http://127.0.0.1:4610 (`web_server.rs`, refuses other sites); the app
  picks the browser or window connection at start (`app/src/lib/legion*.ts`).
- **The app's sidebar** starts closed, and a folder shows a green dot while
  anything in it works, as deployments do.

## Good next steps

1. **Folders that aren't in git** (IMPROVEMENTS.md has the proposed fix).
   On hold to talk through: `temp_plus_platform` is 7.1 GB, with a 175 MB
   zip, an API key file, nested repos and a `worktrees/` folder, so a
   straight snapshot would copy all of that into history and every
   mission's checkout. Options so far: filter (.gitignore, a
   `.legion2/ignore`, a size cap, skip nested repos); or run missions there
   one at a time.
2. **The strategist**: watch it on a real run. Since its first run it skips
   checks when nothing is busy or moving, and callouts and shared tools are
   announcements (no reply wanted, and replies aren't forwarded to the
   commander); neither is tested live yet.
3. Then: how missions work (to talk through).
4. A systemd unit for the service on Linux.
