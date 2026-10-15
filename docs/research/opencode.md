# OpenCode v2.0.25: one Legion session

What we found by running `/opt/homebrew/bin/opencode` (v2.0.25) by hand on
2026-10-14/15, with a throwaway HOME and never touching
`~/.config/opencode/`. Draft 5 can copy the template at the end.

## Answers

1. **Command.** `opencode mini` is the one to use: it is interactive in the
   terminal and takes `--model provider/model`, `--agent` and `--prompt`.
   Top-level `opencode` has `--prompt` but no `--model` or `--agent`.
   *Checked by:* `opencode --help`, `opencode mini --help`.

2. **A session's own config.** `OPENCODE_CONFIG=<file>` works, but only in
   the environment of the **server** process. A plain `opencode` talks to
   the shared background service, which never sees the client's
   environment. That is why the earlier spike saw nothing in
   `opencode debug config`. With `--standalone`, the client starts its own
   private server, which inherits `OPENCODE_CONFIG`. An `opencode.json` in
   the working folder is also read, for that folder only.
   *Checked by:* `opencode serve` with `OPENCODE_CONFIG` set, then
   `GET /api/config?location[directory]=…` listed the file. In the hand
   run (below), `mini --standalone` with `OPENCODE_CONFIG` used the file's
   agent, provider and MCP server.
   Also: with a throwaway HOME, the background service fails to start
   because port 49374 is already taken by Joe's real service, so tests
   *must* use `--standalone` or `opencode serve --port`.

3. **Worktree `opencode.json`.** Not needed, since rule 2 works.

4. **System prompt.** In v2.0.25, `instructions` in the config is accepted
   but has no effect: neither an absolute nor a folder-relative path
   reached the model. What works is an agent with
   `"system": "{file:<path>}"`. The file's text goes first in the system
   prompt, followed by OpenCode's own notes (model and tools).
   *Checked by:* a codename in the file, asked of the model. With the
   agent, it answered "HERON-3". With `instructions`, it answered with its
   model name.

5. **Tool names.** By default v2 hides MCP tools behind one `execute`
   tool (OpenCode calls this "Code Mode"). The model has to write
   JavaScript like `tools.legion.missions()`. Qwen got this wrong: it
   called `legion.missions` directly and failed. With `"codemode": false`
   on the server, the model gets them as plain tools named
   `legion_<tool>`. So **`toolName` is `legion_{tool}`**, and the template
   sets `codemode: false`.
   *Checked by:* a fake OpenAI endpoint that logged each request's
   `tools` list, run once with Code Mode and once without.

6. **Permissions.** v2 uses rule lists:
   `{"action", "resource", "effect": allow|deny|ask}`. They can sit on the
   config, on an agent, or on a session made over HTTP. The default
   `build` agent already allows everything except reading `.env` files
   (ask) and the browser (deny). `--auto` ("auto-approve what isn't
   denied") exists on top-level `opencode` and `run`, but **not on
   `mini`**. For an unattended operator, the agent carries
   `[{"action":"*","resource":"*","effect":"allow"}]`, which is about the
   same as Claude's `bypassPermissions`. Claude's `plan` is like OpenCode's
   built-in `plan` agent, which denies `edit`. `default` and `acceptEdits`
   would be `ask` rules, which block a session nobody watches. There is no
   flag to set the mode, so `{permissionMode}` isn't used. Not tested: the
   `question` tool (the model asking the user) may still wait for an
   answer, so draft 5 may want to add
   `{"action":"question","resource":"*","effect":"deny"}`.
   *Checked by:* `GET /api/agent` (each agent's rules), `mini --help`, and
   the hand run, which needed no approvals.

7. **Provider and model.** The provider **must** be in the config, since
   no flag can define one. v2 calls the key `providers`: `package`,
   `settings.baseURL`, `models`. The model can come from `--model` or from
   `"model"` in the config. The template sets both. Under a throwaway HOME
   there are built-in `opencode/*` cloud models, so always name the model.
   *Checked by:* `opencode models --server …` listed
   `spark-fast/qwen3.6-35b-a3b` only once the config had it.

8. **Typed text.** If the session is idle, text plus Enter starts a turn.
   If it is busy, the text waits in a queue (the status line shows
   "1 pending"). It runs on its own when the current turn ends. Nothing
   was lost and the turn wasn't interrupted.
   *Checked by:* `tmux send-keys` during a long answer. The queued "reply
   PONG" ran right after it.

9. **Messages without typing (decision 6): yes, over HTTP.** Every
   OpenCode TUI is a client of a server with an HTTP API.
   `POST /api/session/{id}/prompt` with
   `{"text": "...", "delivery": "queue"}` adds a message. It runs now if
   the session is idle, or after the current turn if it's busy.
   `"delivery": "steer"` exists too, but we didn't try it. The message
   shows up in the terminal like typed text. To make this work, Legion
   runs `opencode serve --hostname 127.0.0.1 --port <port>` itself, with
   `OPENCODE_CONFIG` and `OPENCODE_PASSWORD` set, then starts
   `opencode mini --server http://127.0.0.1:<port> --session <id>` in the
   terminal. `--session` creates a session with the ID Legion picks (it
   must start with `ses`), so Legion knows where to post.
   `GET /api/event` and `POST /api/experimental/session/{id}/wait` might
   tell Legion when a turn ends, but we didn't try them.
   **Recommendation:** use this server route for draft 11, not typing.
   *Checked by:* a session `ses_legiontest0001`. A curl POST got the reply
   "VIAHTTP" in the tmux screen. `--standalone` won't do for this: its
   private server listens on a random port with a random password.
   Not tried: plugins and `opencode acp` (draft 10).

10. **Template blanks.** The `--standalone` template below uses only known
    blanks. The server route in 9 needs three that aren't on draft 2's
    list: **a port, a server password and a session ID**. That is a gap
    for draft 2. Also, the server is a second process that Legion must
    start and stop. `{permissionMode}`, `{toolsConfig}` and
    `{toolsConfigFile}` aren't used.

11. **Tool server.** The `mcp.servers.legion` block in the config runs
    `{legionCommand} mcp` with the three `LEGION_*` variables, mirroring
    `legion_tools_config` (`crates/legiond/src/session_arguments.rs`).

12. **No leaks.** The config is in a file only that session's server reads.
    A server started without `OPENCODE_CONFIG` showed only the home config
    folder in `/api/config`, with no Legion agent, MCP server or
    `spark-fast`. Nothing is written to the working folder or to
    `~/.config/opencode/`.

**Watch out:** in the first hand run, the first prompt arrived before the
tool server had connected, so that turn had no `legion_*` tools. The next
turn had them. Draft 5 needs to handle this, either by sending the first
prompt over HTTP once `GET /api/mcp` says `connected`, or by not putting
anything in `--prompt` that needs Legion's tools.

## Template (draft 2's shape)

```json
{
  "name": "opencode",
  "command": "opencode",
  "args": ["mini", "--standalone", "--model", "{modelProvider}/{model}",
           "--agent", "legion", "--prompt", "{firstPrompt}"],
  "env": { "OPENCODE_CONFIG": "{configFile}" },
  "toolName": "legion_{tool}",
  "tools": "mcp",
  "configFile": {
    "$schema": "https://opencode.ai/config.json",
    "model": "{modelProvider}/{model}",
    "default_agent": "legion",
    "providers": {
      "{modelProvider}": {
        "package": "@ai-sdk/openai-compatible",
        "settings": { "baseURL": "{address}" },
        "models": { "{model}": { "name": "{model}" } }
      }
    },
    "agents": {
      "legion": {
        "mode": "primary",
        "system": "{file:{systemPromptFile}}",
        "permissions": [{ "action": "*", "resource": "*", "effect": "allow" }]
      }
    },
    "mcp": {
      "servers": {
        "legion": {
          "type": "local",
          "codemode": false,
          "command": ["{legionCommand}", "mcp"],
          "environment": {
            "LEGION_DEPLOYMENT": "{deployment}",
            "LEGION_POSITION": "{position}",
            "LEGION_SOCKET": "{socket}"
          }
        }
      }
    }
  }
}
```

Legion starts it in `{workingFolder}`, because `mini` uses the folder it
starts in. Note that `"{file:{systemPromptFile}}"` nests a Legion blank
inside OpenCode's own `{file:…}` syntax, so draft 2's filler must leave
`{file:` alone.

## Hand run (2026-10-15)

- A throwaway legiond, built from this worktree, ran under
  `HOME=/tmp/lg.XXXX` with `{"webPort":0}` in its `settings.json`. The
  socket was at `$HOME/.local/share/legion/legiond.sock`. The short path
  matters: a socket path inside the scratchpad was too long (SUN_LEN).
- The template was filled by hand: `spark-fast`, `qwen3.6-35b-a3b`,
  `http://spark:8002/v1`, deployment `test`, position `builder`. It ran
  in tmux under a second throwaway HOME (for OpenCode).
- From inside OpenCode, the model listed: `legion_ask`, `legion_blocked`,
  `legion_callout`, `legion_callouts`, `legion_done`,
  `legion_flag_decision`, `legion_handoff`, `legion_log`,
  `legion_mission_read`, `legion_missions`, `legion_note`,
  `legion_part_done`, `legion_send`, `legion_sessions`,
  `legion_share_tool`, `legion_suggest`, `legion_toolbox`.
- `legion_missions` reached the test legiond and returned
  `no deployment "test"`. That's expected: no deployment was made, so the
  call connects but has nothing to show.
