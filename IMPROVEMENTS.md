# Improvements to try next run

Found while running the `hello-legion2` test rounds. Newest at the bottom.

- **End operators the pipeline can't reach again.** Once the work has moved
  past them for good (the planner, once building has started), end them
  instead of leaving them idle until the mission is finished.
- **A model per role.** Haiku for quicker jobs like the docs-writer and
  release manager; Sonnet or stronger for the builder and architect, set in
  each `operator.json`.
- **Less "ended its turn without reporting" noise.** When an operator has
  already handed off and only answers a late message, don't pass the reply on
  to the commander.
- **Handoff to a list shows as `["reviewer"]`.** When an operator sends the
  handoff's `next` as a list, Legion prints the list as written. It should
  join the names: `→ reviewer` or `→ security-reviewer, docs-writer`.
- **One planner sets the pace for a batch.** With 10 missions, every one
  waits in line for the single planner. Allow 2 or 3 planner copies for
  batches, or let the commander skip planning for small, clear missions.
- **The planner started building.** On mission 10 it wrote `stats.html`
  itself. Lock it to its plan in `operator.json` (block Write and Edit outside
  `PLAN.md`), so it can't do the builder's job.
- **Tell the commander when room opens up.** In the batch run the commander
  sat idle with missions waiting and the planner idle, until it was told.
  When an operator goes idle and a waiting mission needs it, Legion should
  tell the commander, the same way it does for long turns.
- **The commander loses track in a busy batch.** With several handoffs at
  once, it missed one (mission 11 sat handed off to the reviewer) and left
  the idle planner unused. After each handoff, Legion could list the
  missions handed off but not picked up, and the waiting ones with a free
  operator.
- **A finish clash goes to the human first.** When a mission's commits clash
  with the base branch, Legion marks it blocked and tells the human, but the
  commander handles it on its own (sends the builder to replay and fix). Tell
  the commander first; escalate only if that fails.
- **A fix after a finish clash sends the mission through every check again.**
  Replaying onto the base resets it, so it went tester → reviewer again even
  when only `style.css` lines moved. A clash fix that changes nothing else
  could go straight back to the finish.
- **Two copies of one operator on the same mission clobber each other.** Two
  builders worked in mission 15's folder at once and broke a replay half way
  through. (Legion now refuses to start a second copy on a mission one already
  holds.)
- **Operators don't hear the human's decisions.** The tester and reviewer both
  asked to undo a change the human had asked for. The commander should pass
  the human's decisions to everyone on the mission.
- **legion2d doesn't start on its own after a reboot.** The plan says it
  starts when the machine starts and brings open deployments back; nothing
  sets that up yet (a launchd agent on macOS, a systemd user unit on Linux).
  (Done for macOS: `legion2 service install`. Linux still needs a systemd unit.)
- **A folder that isn't in git gets no protection.** Missions there work
  straight in the folder, so two at once can overwrite each other and nothing
  is committed. Legion should run them one at a time there, or offer to set
  up git when the folder is added.
  - **Proposed fix: Legion keeps its own git history for the folder.** When
    a folder that isn't in git is added, Legion records a snapshot of its
    files in a git history stored in Legion's data folder; git can keep its
    records apart from the files it tracks, so nothing is added to the
    user's folder. Each mission gets its own copy from that history, as it
    does in a git folder. When a mission finishes, Legion merges its changes
    in and writes the result into the user's folder. Before each finish,
    Legion records the folder again, so changes the user made by hand are
    kept. This also fixes `finish` refusing, and operators trying git, in
    such folders (see below).
- **Missions that all add to one file clash at the finish.** In the batch run,
  several pages each appended to the end of `style.css`, so their branches
  collided. Planners could give each mission its own stylesheet, or the
  commander could finish missions that touch the same file one at a time.

## From the rogers-demo run (temp_plus_platform, 2026-10-06)

Reported by the session running it. "Built, not installed" means the code
is written and tested, but legion2d hasn't been reinstalled with it yet.

- **One copy per mission.** The check that stops a second copy starting on a
  mission another copy holds also stops two copies from sharing the work, so
  work had to be split into one mission per bill. (Built, not installed:
  parts let copies share one mission, each on its own part.)
- **A restart loses operators.** After a legion2d restart, only the
  commander and each mission's holder came back. The other operators on
  those missions (3 bill-verifiers, a db-checker) lost their work and had to
  be started by hand. (Done: every operator on a mission comes back, as the
  machine has room.)
- **Copies get new names on a restart.** parser-tester-4 came back as
  parser-tester-3, which confused operators messaging each other by name.
  (Done: they come back under the same names.)
- **The machine limit is read only at start.** Raising `maxBusySessions`
  means a restart. The default of 6 held back a squad of 8+ operators.
  (Done: settings.json is read each time.)
- **A session lost at the limit isn't retried.** After an Esc, bill-creator
  ended (exit 129), couldn't be started again because 6 sessions were busy,
  and nothing tried again later.
- **The commander can't interrupt a long turn.** Messages wait behind it (a
  13-minute self-check), and the commander had to ask the human to press Esc.
- **Decisions don't reach the human.** An operator raised a real design
  choice as a message to the commander, so it never showed in questions or
  Escalations. Operators need a way to flag something for the human that
  isn't blocking. (Done: the `flag_decision` tool.)
- **Missions can't be edited.** When requirements changed mid-run, missions
  3-5 kept the old wording, and the fix went out as commander messages.
- **Only the human creates missions.** The commander had to suggest each
  one. (Built, not installed: the commander can split a mission into parts
  itself, which covers much of this.)
- **Mission status goes stale.** `missions` showed "Started (db-checker)"
  while parser-tester did the work, and "Waiting" right after a resume while
  an operator was busy.
- **`stop` returns before the session has ended.** An immediate `start` of
  the same operator failed with "already on mission".
- **Permission modes.**
  - A session set to `manual` ended up in auto mode, because Claude Code's
    permission prompt offers "Yes, and switch to auto mode". A setting to
    keep a session's mode locked would help.
  - A deny rule always beats an allow rule, so "deny Edit on the project,
    allow legion-tools/" fails without saying so. A per-operator list of
    folders it may write would be cleaner.
  - There's no setting for extra folders an operator may use (v1 had
    `addDirs`).
- **Auto mode blocks local database work.** It refused a local `DROP
  DATABASE` and seeding even after the human approved them, then refused
  that session's later reads too. The human ran the commands.
- **No rules for answering permission prompts.** There's no way to approve
  or deny operators' prompts by rule (approve local, deny staging), so the
  human has to watch screens.
- **No Legion tools in the human's own Claude session.** `legion2 mcp` only
  serves sessions legion2d started; the human gets just the command line.
- **legion2d isn't a service.** No launchd agent and no default log file; it
  needs a manual start after every reboot. (Done: `legion2 service install`.)
- **Pipelines can't express rounds.** Conditions had to say "round 1" and
  "round 2" in plain text. (Already installed: a step's `next` can be a
  list, such as `[security-reviewer, docs-writer]`; those operators work at
  once and the commander waits for all of them. Rounds aren't covered.)
- **Operators can't write a shared tools folder.** They can't write in
  `.legion2/`, so the team made `legion-tools/` with a README index. (Built,
  not installed: the `share_tool` tool has Legion copy a tool into
  `.legion2/tools/` and tell everyone, so operators don't need to write
  there.)
- **The commander doesn't push for speed.** Every speed-up was found by the
  human: running bill-making alongside the baseline round, one checker per
  bill, skipping the browser test for known-good bills, uploading the next
  bill once the last one was delivered instead of after all its checks,
  stopping an operator redoing a step that had already passed, and cutting
  short a 13-minute self-check. The commander mostly passed messages along
  and waited.
- **A speed role (Joe's idea).** A position beside the commander whose only
  job is speed: it watches the log and the screens, and suggests or makes
  speed-ups. That covers work that could run in parallel, waits on steps
  already done, checks done twice, long turns, idle operators, and polling
  that should be a message. It could be a built-in position, or part of the
  commander's instructions with a regular check-in.
- **Operators poll instead of waiting.** Parser-testers ran shell loops
  checking the database for the last bill's upload, though a peer was due to
  message them. The loops set off permission prompts (unsetopt, sh -c,
  loops) and stalled. A built-in "wait for a message" tool, or a rule against
  polling for something a peer will report, would fix it.
- **Harmless commands stall on prompts in manual mode.** Plain reads (ls, a
  SELECT in a psql loop) stop on prompts because of shell features (unsetopt,
  globs, sh -c). Operators don't know which commands set off prompts. A hint
  in their instructions, or a way to pre-approve read-only commands, would
  help.
- **Operators re-check their own work.** bill-creator spent most of its time
  checking its own output, though the next step is a dedicated checker. A
  pipeline could say "don't check this; the next operator does".
- **Status written on screen instead of reported.** "Ended its turn without
  reporting through its tools" comes up a lot: operators write their status
  on screen instead of handing off or messaging, and Legion has to pass it
  on to the commander, which costs extra back and forth.
- **The human's own hooks reach operators.** Sessions load the user's
  `~/.claude` settings, so Joe's personal hooks (such as "search OPIE first",
  which blocks Read and Grep until an OPIE search has run) also block
  operators: db-checker couldn't Read `legion-tools/README.md` to edit it.
  Operators may need settings of their own, or a way to leave out the user's
  hooks.
- **Operators keep browsing in manual mode.** db-checker kept running `ls` on
  scratchpad folders, setting off a prompt each time, even after being told
  twice to stop; it doesn't keep "don't do this" across turns. Exact paths in
  the handoff, or a shared scratchpad for each mission, would save the
  browsing.
- **A restart gives an operator a new scratchpad.** Each restart makes a new
  scratchpad folder, so a restarted operator can't find its own earlier
  files without listing `/private/tmp`. A scratchpad that stays put for each
  operator in a deployment would fix it.
- **The stalled-work reminder can't tell a planned wait.** "Mission N was
  handed off and no one is working on it" keeps firing for per-bill missions
  whose next step is done on purpose by an operator on another mission
  (db-checker on mission 2). There's no way to say "this mission's next step
  happens in mission 2". (It fires once per handoff, so each new handoff
  brings it back.)
- **The commander keeps the pipeline it started with.** An operator and a
  pipeline step added mid-deployment (rule-editor, on bill-test) never
  reached the running commander, which told the human "the pipeline has no
  such operator". Its instructions hold the pipeline and copy limits as they
  were when it started. Legion could tell the commander when the pipelines
  or operators change, or the commander could read them fresh before each
  routing decision. (Done: Legion tells the commander when they change.)
- **`finish` refuses in a folder that isn't in git.** It says "no branch to
  finish", so a done mission's sessions stay open and the commander asks the
  human. There, finish could simply end the mission's sessions.
- **Operators try git in a folder that isn't in git.** They run `git status`
  and get stuck on prompts. Their instructions should say whether the folder
  is in git.
- **Read-only work still sets off prompts.** Writing output to a file
  (`> file`) and MCP tools prompt in manual mode. `operator.json` can allow
  `mcp__opie`, but nothing allows "write output to a file in my folder". Each
  operator could get a scratch folder it may write, and be told to use the
  Write tool instead of `>`.
