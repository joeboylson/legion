# OpenCode v2.0.25: one Legion session

Found by running `/opt/homebrew/bin/opencode` by hand on 2026-10-14/15.
`~/.config/opencode/` was never edited. Draft 5 can copy the template at the end.

1. **Command:** `opencode mini`. It's interactive and takes `--model`, `--agent`
   and `--prompt`. Top-level `opencode` has no `--model` or `--agent`.
   *Checked:* `--help` on both.
2. **Own config:** `OPENCODE_CONFIG=<file>` is read by the **server**
   process only. Plain `opencode` asks the shared background service, which
   never sees it. `--standalone` (a private server) or `opencode serve` does.
   The file is merged after `~/.config/opencode/opencode.json`. Under Joe's
   real HOME, his v1 file (`provider`, `spark-fast` too) is read as v2. The
   two `spark-fast`s don't clash: the session file's name and model win, and
   Joe's `limit` is kept. His `model: spark-ollama/…` is overridden. His
   `AGENTS.md` is still loaded into the session.
   *Checked:* real HOME, `GET /api/config`, `/api/provider/spark-fast` and
   `/api/model/default` gave `spark-fast qwen3.6-35b-a3b`.
3. **Worktree `opencode.json`:** not needed.
4. **System prompt:** `instructions` is accepted but does nothing in v2.0.25.
   An agent with `"system": "{file:<path>}"` works: the file goes first in the
   system prompt. *Checked:* a codename in the file. With the agent, the model
   gave it back. With `instructions`, it didn't.
5. **Tool names:** by default MCP tools sit behind one `execute` tool ("Code
   Mode"). Qwen called them wrongly that way. With `"codemode": false` the
   model gets `legion_<tool>`, so **`toolName` is `legion_{tool}`**.
   *Checked:* a fake model endpoint logged each request's `tools` list.
6. **Permissions:** the v2 key is `permissions`, a list of
   `{action, resource, effect: allow|deny|ask}`. `mini` has no `--auto`. The
   template's agent allows `*` (close to Claude's `bypassPermissions`) and
   denies `question`, so it never waits on a person. Claude's `plan` is like
   OpenCode's `plan` agent (denies `edit`). Any `ask` rule would block.
   `{permissionMode}` isn't used. *Checked:* `GET /api/agent` with
   `OPENCODE_CONFIG` set showed the `legion` agent ending in `* allow`,
   `question deny`.
7. **Provider and model:** the provider **must** be in the config. v2 calls
   it `providers` (`package`, `settings.baseURL`, `models`). The model can be
   given by `--model` or by config `model`. The template sets both, because a
   bare HOME defaults to OpenCode's own cloud models. *Checked:*
   `opencode models` under a throwaway HOME.
8. **Typed text:** when idle, text plus Enter starts a turn. When busy, it
   queues ("1 pending") and runs after the turn. *Checked:* `tmux send-keys`.
9. **Messages without typing (decision 6):** `POST /api/session/{id}/prompt`
   with `{"text", "delivery": "queue"}`. It runs now if the session is idle,
   or after the turn if busy. `"steer"` exists but wasn't tried. It needs
   Legion to run `opencode serve --port <port>` with `OPENCODE_PASSWORD`, and
   to start `mini --server <url> --session <id>`. Three blanks are missing
   from draft 2: **port, password, session ID** (admin: draft 11 adds them;
   mission 4 added port and password).
   *Checked:* a curl POST showed up and was answered in the running terminal.
   Plugins and ACP weren't tried.
10. **Blanks:** no gap for the `--standalone` template. #3's `harness.rs`
    only fills `{` + letters/digits + `}`, so in `{file:{systemPromptFile}}`
    only `{systemPromptFile}` is filled.
11. **Tool server:** the `mcp.servers.legion` block mirrors
    `legion_tools_config`.
12. **No leaks:** a server on the real HOME without `OPENCODE_CONFIG` had no
    `legion` agent and no MCP servers. Nothing is written to the folder or to
    `~/.config/opencode/`.
13. **Busy or idle (replaces draft 7's `report` tool for OpenCode):**
    `GET /api/session/active` on the session's server (basic auth, user
    `opencode`, the `OPENCODE_PASSWORD`) answers
    `{"data":{"ses_…":{"type":"running"}}}` while a turn runs, and
    `{"data":{}}` when nothing does ("sessions absent are inactive"). It's
    `{"data":{}}` before the session exists too, so Legion also reads
    `GET /api/session` (`{"data":[{"id","title","time":{"created"},
    "location":{"directory"}}…],"cursor":{…}}`) and shows idle only once it
    lists this session's conversation. That list is global: every stored
    session from every folder and past run, and from other operators in the
    same folder, so time and folder can't tell them apart. Legion gives the
    conversation a title instead: the template makes it with
    `POST /api/session` and `{"title": "{session}"}` (the reply has
    `data.id`), then starts `opencode mini --session <id>`. Legion lists with
    `GET /api/session?search=<tag>` (the list holds only the newest 50 by
    default) and takes the one whose title equals the tag; two is an error.
    *Checked:* the POST, the title and `search` on v2.0.25 under a throwaway
    HOME; `mini --session` is from item 9.
    `GET /openapi.json` lists the API. `/api/event` is a stream (a
    `server.connected` event, then heartbeats) and wasn't needed: Legion polls.
    *Checked:* by hand on v2.0.25 with `opencode serve --port P`.

**First prompt races the tool server.** With `--standalone`, the first prompt
was sent before `legion mcp` connected. That turn had no `legion_*` tools, so
a Legion first turn would fail. v2.0.25 has no "wait for MCP" setting
(`mcp.timeout` only sets limits). **Fix for draft 5:** start
`opencode serve` and wait until `GET /api/mcp` says `connected` (about 2s
here). Then either start `mini --server … --prompt`, or post the first
prompt over HTTP. *Checked:* on a warm server under the real HOME, the
first prompt listed all 17 tools.

## Template

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
    "providers": { "{modelProvider}": {
      "package": "@ai-sdk/openai-compatible",
      "settings": { "baseURL": "{address}" },
      "models": { "{model}": { "name": "{model}" } } } },
    "agents": { "legion": {
      "mode": "primary",
      "system": "{file:{systemPromptFile}}",
      "permissions": [
        { "action": "*", "resource": "*", "effect": "allow" },
        { "action": "question", "resource": "*", "effect": "deny" } ] } },
    "mcp": { "servers": { "legion": {
      "type": "local", "codemode": false,
      "command": ["{legionCommand}", "mcp"],
      "environment": { "LEGION_DEPLOYMENT": "{deployment}",
        "LEGION_POSITION": "{position}", "LEGION_SOCKET": "{socket}" } } } }
  }
}
```

Run it in `{workingFolder}`. `mini` uses the folder it starts in.

## Hand run

The template was filled for `spark-fast` / `qwen3.6-35b-a3b` /
`http://spark:8002/v1`, deployment `test`, position `builder`. The tool
server pointed at a throwaway legiond: built from this worktree, with
`HOME=/tmp/lg.XXXX` and `{"webPort":0}`. A scratchpad-length socket path is
too long. It ran in tmux under a throwaway HOME, and again under the real
HOME. The model listed, from inside OpenCode: `legion_ask`,
`legion_blocked`, `legion_callout`, `legion_callouts`, `legion_done`,
`legion_flag_decision`, `legion_handoff`, `legion_log`,
`legion_mission_read`, `legion_missions`, `legion_note`, `legion_part_done`,
`legion_send`, `legion_sessions`, `legion_share_tool`, `legion_suggest`,
`legion_toolbox`. `legion_missions` reached the test legiond and returned
`no deployment "test"`, because no deployment was made.
