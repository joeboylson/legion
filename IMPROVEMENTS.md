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
- **A folder that isn't in git gets no protection.** Missions there work
  straight in the folder, so two at once can overwrite each other and nothing
  is committed. Legion should run them one at a time there, or offer to set
  up git when the folder is added.
- **Missions that all add to one file clash at the finish.** In the batch run,
  several pages each appended to the end of `style.css`, so their branches
  collided. Planners could give each mission its own stylesheet, or the
  commander could finish missions that touch the same file one at a time.
