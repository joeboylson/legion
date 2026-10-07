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
  `rogers-demo` deployment is open with only its commander and strategist
  running (all its missions are done). The strategist is on there, every 5
  minutes.
  `hello-legion2`, the test app, is in `ref/hello-legion2` (git-ignored
  here, its own git history): a tiny to-do app with the strategist on every
  minute, `auto` permissions and up to 3 builders.

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

## Where we left off (2026-10-07, end of day)

The AD-3 run in `temp_plus_platform` is finished, and its deployment
(`ad3-close`) is closed.

- **Result:** the seven AD-3 rows still open in
  `temp_plus_platform/feature_proof/overview.html` all pass. Five already
  passed on develop; AD-3-10 and AD-3-15 needed fixes, now merged:
  v2-frontend #194 and v2-core #199.
- **Jira:** AD-3 and AD-346 are IN REVIEW. AD-3 has the 16 screenshots and
  a comment saying what each proves; AD-346 has the updated overview
  (`feature-proof-overview-2026-10-07.html`). overview.html now shows all 89
  in-scope scenarios passing.
- **Proof:** `temp_plus_platform/ad3-proof/` (screenshots, queries, how to
  rerun each check), with `index.html` showing them all. Rerun with
  `legion-tools/ad3-proof.sh`.
- **Set up for it in `temp_plus_platform/.legion2/`:** the `ad3-close`
  pipeline; two new operators, `proof-checker` (proves scenarios on the
  local stack) and `opie-reviewer` (runs `/opie-pr-loop` on a PR, one round
  per message, the commander deciding on more rounds); and `auto`
  permissions for planner, builder (up to 3) and reviewer.
- **The local stack** runs v2-core and v2-frontend develop (with both fixes),
  started by naming the main checkouts on the `make` line. The devenv's own
  `.env` (from Oct 5) still points at older `worktrees/v2-*-AD-3` folders;
  it was left alone.
- **Left open, for review with Spencer:** the Columns menu still names the
  spend column "Cycle Spend" while its header says "Recurring spend"; with
  a period chosen, the idle list counts recurring charges only, to match the
  tile; after "View idle lines" the list keeps the dashboard's period when
  the Idle filter is removed (labelled); and whether "recoverable spend"
  should leave out cycles from before a line went idle.
- Lessons for Legion are under "From the AD-3 run" in IMPROVEMENTS.md.

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
   commander). Both checked live on 2026-10-07 in `ref/hello-legion2`
   (test-1, test-2): no wake-ups after the work ran out, and no forwarded
   replies to 17 announcements.
3. **Two copies of one operator on one mission** in a folder without git
   (see IMPROVEMENTS.md, "From the AD-3 run"): the AD-3 run needed an extra
   mission to review two PRs at once.
4. Then: how missions work (to talk through).
5. A systemd unit for the service on Linux.
