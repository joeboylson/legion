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
      { "match": "mission.finish", "timeoutSeconds": 120, "onTimeout": "block",
        "hooks": [{ "type": "command", "command": "./scripts/check-ci.sh" }] },
      { "match": "*", "when": { "via": "web" }, "timeoutSeconds": 5, "onTimeout": "block",
        "hooks": [{ "type": "command", "command": "legion-web-policy" }] }
    ],
    "after": [
      { "match": "entry.added", "hooks": [{ "type": "adapter", "adapter": "supabase" }] }
    ]
  }
}
```

- **`before`** is like Claude Code's `PreToolUse`. The hook gets the envelope
  as JSON on stdin. Exit code 2 blocks the action, and what it wrote to stderr
  goes back to whoever sent it. A hook can only allow or block an action,
  never change it.
- **`after`** is like `PostToolUse`. It sees the result or event and can't block it.
  It reads from the action record (below), not from the action as it happens.
- **Hook types:** `command` (a shell command), `http` (POST to a URL) and
  `adapter` (built into legiond: TCP, Supabase, Postgres).
- **Limits on the web** are a `before` hook on `via: "web"`, not a separate
  security layer. Answering questions and approving held messages could pass;
  changing tools, permissions or folders would stay local.

## Keeping hooks from stalling Legion

`handle_command` runs every command to the end before it replies, and the
26 entries Legion writes on its own happen in the middle of its work. A hook
run inline there would hold all of Legion up while it runs, and one that
hangs would stop it. Three rules keep that from happening.

### 1. An action record, and `after` hooks read from it

Every envelope is written to a record that only grows:
`actions.jsonl` in each folder's data, next to the log. Writing it is the
last step of an action, so a written envelope is one that happened.

- `after` hooks and adapters each read the record from where they left off,
  in the background. Each keeps its own place (the last `id` it handled), so
  a slow Supabase adapter never delays a TCP one, and neither delays Legion.
- After a restart, a crash or the machine sleeping, each picks up from its
  place and catches up. Nothing is lost, and nothing is sent twice: adapters
  already drop repeats by `id`.
- A hook that keeps failing is retried with a growing wait, then marked
  stuck in the app, with its place kept, so you can fix it and let it
  catch up.
- The record is also what `cause` points into, so "this answer led to that
  message going out" can be followed back after the fact.

### 2. `before` hooks are rare and time-limited

- A `before` hook must name the actions it runs on (`mission.finish`), or
  narrow `*` with a `when` (`via: "web"`). A bare `"*"` with no `when` is
  refused when the file loads.
- Each has `timeoutSeconds` (default 10) and `onTimeout`: `block` stops
  the action and says the hook timed out; `allow` lets it through and
  writes a note to the log. Default: `block` for actions from outside the
  machine (`via: "web"`, `channel:*`), `allow` for everything else.
- `before` hooks run outside legiond's state lock, so a slow one holds up
  only its own action, not other folders or sessions.
- Legion's own actions (`actor.kind: "legion"`) don't run `before` hooks
  unless a hook names them, so a check-in or a stalled-work notice can't be
  blocked by mistake.

### 3. No loops

An action a hook causes (a hook that runs `legion send`) carries the hook's
envelope as its `cause`, and `via: "hook:<name>"`. Legion refuses an action
whose `cause` chain already passes through the same hook on the same
subject, and any chain deeper than 8. The refusal goes in the log, naming
the hook.

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
2. The action record, written as the last step of every action.
3. Hooks: `after` (read from the record) first, then `before` with its time
   limits and the loop check. `command` and `http`.
4. TCP channels moved behind an adapter that reads the record.
5. A Supabase adapter.

The local AI work fits this: sessions with no add-on report what they're
doing with a `session.report` action, and an OpenCode session's messages
can go out through an adapter-like reader of the record that posts to
`opencode serve`.

## Decisions (Joe, 2026-10-15)

1. **Rename everything.** Every `Command`, `Reply` and `Event` moves to the
   envelope and its `noun.verb` names in one go, CLI and app included.
2. **Hooks run only in trusted folders.** Trust lives in Legion's machine
   settings (`~/.local/share/legion/settings.json`), never in the repo: each
   trusted folder with a fingerprint (SHA-256) of its `hooks.json`. If the
   file changes, Legion asks again, as `direnv allow` does. Approve with
   `legion hooks trust` or a button in the app. Until then the folder's
   hooks don't run, and the app says so. Machine-wide hooks in Legion's own
   data folder are trusted by being there.
3. **`before` hooks only allow or block.** No changing an action.
4. **The action record:**
   - Where: one file per day in each folder's data,
     `actions/<YYYY-MM-DD>.jsonl`.
   - Kept 14 days, and never deleted before every `after` hook and adapter
     has read past it; capped at about 500 MB, oldest days first. An adapter
     that falls behind the cap shows as "fell behind" in the app.
   - Leaving the machine: by default an adapter sends only what happened and
     to what (`name`, `actor`, `subject`, `at`, `id`, `cause`), no text. Each
     adapter lists the action names it may send (`"send": ["mission.*",
     "entry.added"]`), and text fields (`data`) go only with
     `"includeText": true`. `session.key` and `session.input` never leave the
     machine.
   - Local hooks on this machine see whole envelopes.
5. **Remote actions use the channel permissions.** Envelopes arriving from
   another machine go through the same send and receive permissions
   channels use today (free, ask, off per deployment), and the same path:
   channel messages become envelopes too, so there is one way in. Every
   remote envelope carries its sending machine, and the receiver checks it
   against the channel it came in on.
6. **Decision records, for training small local models.** Every choice
   Legion or a commander makes (who goes next, approve or ask, stuck or
   working, take a suggestion or not) is written as a `decision.made` event:
   the options there were, the one picked, who picked it, and its `cause`. A
   later event links back when it's overturned (sent back, restarted,
   reverted), so each decision also has an outcome. Synced to Supabase,
   these become training data from every machine, without leaking anything:
   - **Fields, not prose.** The situation is stored as fields: pipeline,
     step, who handed off, which pipeline condition they claimed, fix
     rounds, minutes taken, files changed, options, choice.
   - **A local summary for the words.** Where words matter, the local model
     (spark-fast) writes a short, neutral summary of the handoff note ("tests
     pass, work committed, one open question about a config name"), and the
     summary goes up instead of the note.
   - **A last check before sending.** On the sending machine, anything that
     looks like a key, password, email address or long piece of code, or
     that matches a never-send word list (customer names), is blocked and
     kept local, with a note in the log.
   - The raw text stays in the local action record only.
   The Supabase adapter sends `decision.made` with these fields and the
   summary by default; nothing else of the text.
