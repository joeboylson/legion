# Hooks: one message shape for every action

A proposal, not built yet. The goal is that every action in Legion goes
through one place, written in one shape, so that:

- hooks can run before or after any action, the way Claude Code's hooks do;
- adapters (TCP, Supabase, Postgres) can carry actions between machines;
- operators, the admin, Legion itself and other machines all talk the same way.

## What's there today

Most of Legion already uses one shape. The `legion` command, the app and the
tools inside each session all send a `Command` in a `Request`, and
`Daemon::handle_command(caller, command)` in `crates/legiond/src/daemon.rs`
handles every one. Operators talk the same way: `Post { deployment, entry }`,
with `entry_routing.rs` deciding who it reaches.

Four paths go around it, so a hook would miss them:

| Path | Today | Why it matters |
|---|---|---|
| What Legion does on its own | `post_entry` called directly, in 26 places | Check-ins, holds, stalled-work notices and handoffs would never reach a hook. |
| The add-on in each session | Two HTTP routes (`AddonReport`, the inbox) | Session state and deliveries have their own shape. |
| Channels between machines | `WireMessage` (`Relay`, `Undeliverable`, …) | Its own shape; this is where adapters go. |
| `Event` | A separate enum, not linked to what caused it | You can't follow "this answer led to that message going out". |

Nothing says where an action came from, either. The author is a session's
position or `admin`, so a command from the web and one from the terminal look
the same.

## The envelope

Every action, result and event is one envelope:

```jsonc
{
  "v": 1,
  "id": "01JA2…",              // a ULID: unique and sortable; adapters drop repeats by it
  "at": 1760450000000,         // milliseconds since 1970
  "type": "action",            // action | result | event
  "name": "mission.finish",    // noun.verb; events in the past tense: "mission.finished"
  "actor": {                   // who did it
    "kind": "admin",           // admin | session | legion | remote
    "machine": "joes-mbp",
    "deployment": null,        // set for a session
    "position": null,
    "via": "app"               // cli | app | mcp | addon | web | channel:<adapter>
  },
  "subject": {                 // what it's about; hooks can match on these
    "folder": null, "deployment": "d_4f2", "mission": 3, "part": null, "position": null
  },
  "data": { },                 // the action's own fields, typed per name in legion-proto
  "cause": "01JA1…",           // the envelope that led to this one, if any
  "reply_to": null,            // results only: the action's id
  "outcome": null              // results only: { "ok": … } or { "error": "…" }
}
```

How today's types fit:

- **`Command` becomes an action.** Each variant gets a name:
  `MissionFinish` is `mission.finish`, `SessionStart` is `session.start`,
  `Post` is `entry.post`. Its fields go in `data`; the IDs it's about go in
  `subject`.
- **`Reply` becomes a result**, tied to its action by `reply_to`.
- **`Event` becomes an event**, with `cause` pointing at the action behind it.
- **Add-on reports become `session.report` actions** from the session, with
  `via: "addon"`. Inbox deliveries become `entry.added` events for that session.
- **The 26 internal `post_entry` calls become actions** from
  `actor.kind: "legion"`. This is the change that matters most: one place
  everything passes through, so hooks see all of it.
- **Channels carry envelopes.** An adapter sends envelopes and hands over the
  ones it receives, which arrive as `actor.kind: "remote"`,
  `via: "channel:<adapter>"`. An adapter needs only `send(envelope)` and a
  stream of incoming envelopes.

## How operators talk

A builder handing off to the reviewer:

```json
{ "type": "action", "name": "entry.post",
  "actor": { "kind": "session", "deployment": "d_4f2", "position": "builder", "via": "mcp" },
  "subject": { "deployment": "d_4f2", "mission": 3 },
  "data": { "kind": "handoff", "to": "reviewer", "text": "Ready for review on mission/3" } }
```

Running it gives an `entry.added` event whose `cause` is this action, and that
event is what reaches the reviewer. A commander on another machine sending a
message looks the same; only `actor.machine` and `via` differ.

## Hooks

Set in `.legion/hooks.json` for a folder, or in Legion's data folder for the
whole machine:

```json
{
  "hooks": {
    "before": [
      { "match": "mission.finish", "hooks": [{ "type": "command", "command": "./scripts/check-ci.sh" }] },
      { "match": "*", "when": { "via": "web" }, "hooks": [{ "type": "command", "command": "legion-web-policy" }] }
    ],
    "after": [
      { "match": "entry.added", "hooks": [{ "type": "adapter", "adapter": "supabase" }] }
    ]
  }
}
```

- **`before`** is like Claude Code's `PreToolUse`. The hook gets the envelope
  as JSON on stdin. Exit code 2 blocks the action, and what it wrote to stderr
  goes back to whoever sent it. Printing `{"data": …}` changes the action.
- **`after`** is like `PostToolUse`. It sees the result or event and can't block it.
- **Hook types:** `command` (a shell command), `http` (POST to a URL) and
  `adapter` (built into legiond: TCP, Supabase, Postgres).
- **Limits on the web** are a `before` hook on `via: "web"`, not a separate
  security layer. Answering questions and approving held messages could pass;
  changing tools, permissions or folders would stay local.

## Open questions

1. **Do reads fire hooks?** Proposed: no by default. `folder.read`,
   `log.read`, `session.screen` and the like run hooks only when a hook names
   them, or every screen refresh would run hooks.
2. **Keystrokes.** `session.key` and `session.input` are frequent and can hold
   secrets. Proposed: kept out of `after` hooks and adapters unless named.
3. **CLI names.** `legion mission finish` already reads as `mission.finish`.
   Proposed: make the two match exactly.

## Order of work

1. The envelope, and one place every action goes through, with the 26
   internal calls moved onto it.
2. Hooks: `before` and `after`, `command` and `http`.
3. TCP channels moved behind an adapter.
4. A Supabase adapter.
