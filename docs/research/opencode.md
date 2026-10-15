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

14. **Request time limits (mission 8):** the provider `settings` in v2.0.25
    take `timeout`, `headerTimeout` and `chunkTimeout` (milliseconds, or
    `false` to switch one off). Read from the binary:
    - `headerTimeout`: from sending the request until response headers
      arrive. Default **300000 (5 minutes)**. With a streamed reply vLLM
      sends headers only once the prompt is read, so this is the "slow to
      read a big prompt" limit. It fires as "Timed out waiting for response
      headers" (a `provider.transport` error), the "Transport: ... timed out"
      Legion saw.
    - `chunkTimeout`: longest quiet gap between pieces of the reply. Default
      **300000**. This is the "slow to write" limit.
    - `timeout`: the whole request. No default (none). Left unset.
    - These are OpenCode's own timers. Bun's fetch has its own limit, which
      item 15 found. vLLM has no per-request time limit by default; its
      server wasn't inspected here.

    The template sets `headerTimeout` and `chunkTimeout` to **1800000 (30
    minutes)**. Nothing argued for less: a 30 words/s model writes tens of
    thousands of words in that time.

    **What still ends a hung step:** a connection that sends nothing for 30
    minutes fails with the same error, and OpenCode retries the request (a
    stalled run showed `retry.attempt` up to 4; where retries stop wasn't
    checked). A model that keeps sending is never cut off. Past that, only
    the commander ending the session does.

    *Checked:* `GET /api/config` showed both keys accepted as numbers. A fake
    provider that answers after 8 seconds, with `headerTimeout` 3000, gave
    "Timed out waiting for response headers" and retried; with 20000 the
    reply was read. Under a throwaway HOME, `opencode serve`.

15. **The real 6-minute limit (mission 10):** Bun's own fetch gives up on a
    request that is silent for **360 seconds**, with `TimeoutError: The
    operation timed out.` (code 23), the "Transport: The operation timed
    out" Legion saw at 5m59s. `headerTimeout`, `chunkTimeout` and `timeout`
    don't touch it: with the provider settings at 3600000, `false`, or
    1800000 plus `timeout: false`, a model silent for 420 seconds was still
    cut at 362 seconds (the retry arrived then); with `headerTimeout` 100000
    it was cut at about 100 seconds, so the settings do reach the request.
    - **Why:** for `@ai-sdk/openai-compatible` OpenCode uses its own client,
      which calls `fetch` without `timeout: false`. Bun's default applies.
      A bare Bun `fetch` (run with `BUN_BE_BUN=1 opencode run script.js`)
      dies at 360 seconds without `timeout: false` and lives with it.
    - **It hits `opencode mini` too.** `mini` is a Bun client of `serve`.
      With the variable set on `serve` only, a 450 second silent model was
      answered (one request, no retry) but `mini` showed "Transport: The
      operation timed out." at 5m 59s and stopped updating while `serve`
      carried on. That is the cut Legion's operators saw. With the variable
      on `mini` as well it ran to 7m 31s with no error. The template's
      `env` reaches both, as one script starts them.
    - **Only silence counts.** A reply that sends something at least every
      360 seconds (even an empty chunk) was not cut: a provider that sent a
      keepalive chunk every 20 seconds finished at 482 seconds. A model that
      is busy reading a big prompt sends nothing, so it hits the limit.
    - **Fix:** the template sets the environment variable
      `BUN_CONFIG_HTTP_IDLE_TIMEOUT` to `1800` (seconds, the same 30
      minutes). Not a setting in OpenCode's config: it has no key that
      reaches this request. Other ways tried and failed: the package name
      `aisdk:@ai-sdk/openai-compatible` (still cut at 362 s) and a preload
      script through `BUN_OPTIONS` or `bunfig.toml` (the compiled opencode
      ignores both).
    - **Backup:** a small local proxy between OpenCode and the model that
      sends the stream's headers at once and a `: ping` comment every
      minute until the model answers, then passes the answer through. It
      carried a 420 second silent model to a reply. Not needed while the
      variable works.

    *Checked:* fake provider that stays silent for 420 and 450 seconds
    (before the headers, and after them), `opencode serve` under a throwaway
    HOME, request times read from the fake's own log.

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
      "settings": { "baseURL": "{address}",
                    "headerTimeout": 1800000, "chunkTimeout": 1800000 },
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

## Parallel tool calls

OpenCode 2.0.25 has no named setting for them on `@ai-sdk/openai-compatible`.
`parallelToolCalls` and `parallel_tool_calls` in a model's or provider's
`settings` are accepted and then dropped: the request to the model had no
`parallel_tool_calls` and no `tool_choice` (checked against a capture server
that logs each request body). The raw `body` object on the model is merged
into every request, so the template sets it there:

```json
"models": { "{model}": { "name": "{model}", "body": { "parallel_tool_calls": true } } }
```

With that, the capture server saw `"parallel_tool_calls": true` on the
request that carries tools. vLLM already runs several tool calls per reply;
this only states it.

Proof on a throwaway Legion, one OpenCode operator on `spark-fast`, asked to
read four files: its OpenCode session log (`session_message`, assistant
rows) has steps with 4 reads together and with 4 mixed calls together
(`shell` and 3 `glob`). The step habits come from the extra section in its
instructions (`crates/legiond/src/prompts.rs`).

## Thinking

vLLM on the Spark thinks by default (a plain request came back with
`reasoning` text), and `reasoning_effort` only makes it think more. The
template sets the model's raw `body` to
`"chat_template_kwargs": {"enable_thinking": false}`, next to
`parallel_tool_calls` and `"reasoning_effort": "none"`. A direct request with
`reasoning_effort: "medium"` and `enable_thinking: false` came back with no `reasoning`.

OpenCode sends `reasoning_effort` only from a saved TUI variant, kept in
`~/.local/state/opencode/model.json` (live:
`{"variant":{"spark-big/qwen3.8-27b":"medium"}}`). Sessions under the real
HOME pick it up and show `[medium]`; a throwaway HOME has no such file, which
is why my captures had none. The model `body` is merged over it, so
`"reasoning_effort": "none"` in the body wins (the checker saw `medium`
replaced by `none`, and vLLM replied with `reasoning_tokens` 0).

Proof: a capture proxy in front of the Spark logged the template's requests.
Each body had `chat_template_kwargs: {"enable_thinking": false}` and
`parallel_tool_calls: true`, and (in a throwaway HOME) no `reasoning_effort`. The OpenCode session
log of that run held only `text` and `tool` blocks, no `reasoning`.

## First request size (mission 14)

A shipper's first request reached 50K tokens, and `spark-big` timed out
reading it. To see what fills it, a fake model endpoint saved the body of the
first chat request of an OpenCode operator on a throwaway Legion. The
operator had a real shipper's inputs: its 10 KB definition, a 1.6 KB mission
with 77 history notes (22 KB), the folder's 20 callouts and 8 toolbox
tools. The saved body was then sent to `spark-fast` with `max_tokens: 1`;
`usage.prompt_tokens` is the real count (Qwen's tokenizer, about 3.8 bytes
per token on this text).

| Part of the first request | Before | After |
|---|---|---|
| Legion: definition | 10.4 KB | 3.2 KB |
| Legion: mission and history | 25.5 KB | 3.3 KB |
| Legion: pipeline table | 0.2 KB | 0.2 KB |
| Legion: standing rules and working habits | 1.8 KB | 1.8 KB |
| Legion: tool list | 2.1 KB | 0.1 KB |
| Legion: toolbox | 1.7 KB | 0.5 KB |
| Legion: callouts | 4.2 KB | 1.3 KB |
| OpenCode's own system text | 6.2 KB | 0.9 KB |
| First prompt | 0.02 KB | 0.02 KB |
| Tools array: Legion's 17 tools | 6.6 KB | 6.6 KB |
| Tools array: OpenCode's own | 12.1 KB | 6.3 KB |
| **Whole body** | **71.7 KB, 18,654 tokens** | **24.6 KB, 6,479 tokens** |

The pipeline table here is tiny; a real one is about 3 KB and isn't cut.
The 50K in the log came from longer histories than this one (history is the
part that grows with every round), so the real "before" was bigger than
18.7K.

What OpenCode puts in by itself, found by reading the saved body:
- `execute` and a "Code Mode" text that lists OpenCode's own five tools
  (models, sessions, MCP resources): 4.5 KB of text and 1.2 KB of tool.
- `subagent` (2.7 KB), `webfetch`, `websearch`, `skill`, and a list of two
  skills (OpenCode's own help and bug-report skills).
- `edit`, `read`, `write`, `glob`, `grep` and `shell` stay: Legion's
  operators use them.
- OpenCode v2.0.25 doesn't read `.mcp.json` or `CLAUDE.md` from the folder.

How each is turned off, in the template's config (*checked:* the saved body
no longer holds them):
- `"websearch": false` removes `websearch`.
- Agent `permissions` rules of `deny` for `webfetch`, `subagent`, `skill`
  and `execute` remove those tools and their text. `skills: []` and
  `experimental.policies` with `tool.use` did not.

What Legion cuts for a template session (`crates/legiond/src/lean_prompt.rs`,
the callouts and toolbox sections); Claude's instructions are unchanged
(`claude_codes_text_is_unchanged`):
- The definition: the first 3,000 characters, then the path to the file to
  read whole.
- The mission: whole up to 4,000 characters, then the `mission_read` tool.
- Mission history: the newest 5 entries, each cut to 600 characters, then
  the `log` tool for the rest.
- The tool list: one line. OpenCode already sends each tool's description.
  A harness that reaches tools as commands still gets each command.
- The toolbox: the tools' names (30 at most), then the `toolbox` tool.
- Callouts: the newest 5, then the callouts file and the `callouts` tool.

A folder that already has its own copy of `opencode.json` in
`.legion/harnesses/` keeps the old config until the new one is copied in.
