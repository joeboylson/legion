# Test cases

End-to-end tests to run by hand, after the 10-operator build in
`hello-legion2`. Each says what to set up, what to do, and what has to be
true for it to pass.

## 1. Two teams talking over a channel

**Waiting on:** channels, and reaching a host from another machine. Neither
is built in legion2 yet; see "Channels" in `docs/tauri-rewrite.md`.

**Setup**

- One host machine running `legion2d`.
- A **local team**: a folder on the host, with a deployment running.
- A **remote team**: a folder on a second machine, with its own `legion2d`
  and a deployment running. The app on the host reaches it over SSH or a
  private network.
- One channel, with both deployments added to it.

**Steps**

1. Open the channel in the app. Both ends show it as up.
2. Each commander sends the channel a short description of its team.
3. Give the local team a mission that needs something from the remote team
   (for example, "ask the remote team for its API's field names").
4. With **Send** on "ask me", approve the local commander's message.
5. With **Receive** on "ask me", pass it on to the remote commander from
   Escalations.
6. Let the remote team answer. Set both switches to "free" and repeat.
7. Stop the remote `legion2d`. Start it again.

**Passes when**

- Messages reach the other team only through the channel, and only as the
  switches allow.
- Every "ask me" shows up in Escalations on the right machine.
- Two teams with the same name don't get mixed up.
- The channel shows as down within about 10 seconds of the remote stopping,
  and up again after it restarts. Nothing sent while it was down is lost.

## 2. One deployment, 10 missions at once

**Goal:** see how operators split into copies (builder, builder-2 …) when
there's more work than one copy can do, and how that feels in the app.

**Setup**

- A folder with the `released` pipeline, or a shorter one.
- In `operator.json`, set `"limit": 3` for the builder and tester, and leave
  the others at 1.
- Machine settings at their defaults (at most 6 busy sessions).

**Steps**

1. Start one deployment.
2. Add 10 small missions that don't touch the same files (for example, 10
   separate pages), one right after another.
3. Watch the sidebar, the graph and the log until all 10 are done.

**Passes when**

- The commander starts copies (`builder-2`, `builder-3`) instead of queuing
  everything behind one builder, and never more than the limit.
- No more than 6 sessions are busy at once across the machine; the rest
  wait and start as others finish.
- Each copy works only in its own mission's worktree.
- Operators with a limit of 1 work through their missions one at a time.
- Every mission finishes onto the base branch. When the base has moved on,
  the finish replays the commits and runs the folder's check.
- The graph shows each copy as its own dot, next to its operator, and lights
  the right lines.
- Notes to check by eye: is it easy to tell which copy holds which mission?
  Does the sidebar stay readable? Does anything wait without saying why?
