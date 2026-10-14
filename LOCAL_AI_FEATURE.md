# Local AI feature

Let operators run on our own models (on the Spark) instead of Claude, so we use fewer Claude tokens. In the long run, legion should be able to run on local models alone.

## The idea

Each operator gets two settings:

1. **Program**: what runs the operator. Claude Code, or OpenCode, which works with any model.
2. **Model**: which model that program uses, picked from a named list in `legion.json`.

The named list works the way Parser Studio's `models.yaml` does: every entry has a name, an address and a model. That lets operators point at different addresses and use different models.

```json
"models": {
  "spark-big":  { "address": "http://spark:8000/v1", "model": "<model>" },
  "spark-fast": { "address": "http://spark:8002/v1", "model": "<model>" }
}
```

Running only on local models then just means setting every operator to OpenCode and a Spark model.

## How it runs

- OpenCode runs on the laptop, next to the code and the local stack. It only sends the thinking to the Spark's address, over Tailscale. Edits and commands still happen on the laptop.
- Legion gives OpenCode the same tools it gives Claude Code operators: hand off, ask a question, call out, read missions.
- The commander stays on Claude Code for now. It decides who works next, where a mistake costs the most, and it's only one session.

## Why not a plain call to the model

A plain call only sends text and gets text back. The operator couldn't read files, run commands or hand off work, and those are the jobs we want local models to take over.

## Tools: open to any harness

The harness could be OpenCode, Pi or anything else, so legion's tools can't be tied to Claude Code.

They nearly aren't already. Each session gets legion's tools from a small tool server (`legion2` started with its tools subcommand). The server knows who's calling from three settings: the deployment, the position, and the address of legion2d. See `legion_tools_config` in `crates/legion2d/src/session_arguments.rs`. The server speaks MCP, the standard way AI programs plug in outside tools.

Two ways in, both ending at the same legion2d:

1. **MCP**, for harnesses that support it (Claude Code, OpenCode, Goose, Codex). They point at the same tool server.
2. **Plain commands**, for harnesses that can only run shell commands (Pi doesn't support MCP out of the box). These don't exist yet. For example `legion2 handoff --to reviewer "..."` and `legion2 ask "..."`, using the same three settings.

### What does change for each harness

- **Starting a session.** Legion passes Claude-only flags today (`--mcp-config`, `--model`, `--permission-mode`, the system prompt file). Each harness needs its own way to receive those.
- **Knowing what a session is doing.** Busy, idle, out of usage, how full its context is. Legion reads this from Claude Code today. Each harness needs its own way to report it.

### How to handle both

- **Starting: one config template per harness.** The template is a command with blanks (prompt, model, tool server, permissions). Legion fills in the blanks and runs it, without knowing anything else about the harness. Adding a harness means adding a template, not changing legion's code.
- **Status: the session tells legion.** Config can't say whether a session is busy or idle. Instead, the session reports it through legion's tools ("working", "done", "stuck"), so it works the same in every harness.
- **Types: JSON Schema, not TypeScript.** Legion's config files already have a `$schema` field. A schema with one type per harness gives editor checks and catches mistakes, without adding a new language to legion.

### Option: use ACP instead of one template per harness

ACP (Agent Client Protocol) is a standard from Zed, released in August 2025, for controlling a coding agent from outside. It covers the jobs legion would otherwise build again for every harness:

- starting a session, and passing in tool servers such as legion's
- sending messages to a running session
- knowing when a session has finished a turn
- handling permission requests

If legion spoke ACP, it would need one piece of code for every harness that supports ACP, instead of a template and status reports for each. Harnesses without ACP would still use the template route above.

Support, from third-party blogs, so check against Zed's docs:

- **Built in:** Gemini CLI, Goose, Copilot CLI.
- **Through an add-on:** Claude Code (Zed's add-on), Codex.
- **Probably:** OpenCode (one product announcement, not confirmed).
- **Unknown:** Pi.

Still to check: whether ACP reports how full a session's memory is (legion needs that to start a fresh session), and how its permission requests line up with each operator's allow and deny lists.

Sources: [morphllm](https://www.morphllm.com/agent-client-protocol), [mcp.directory](https://mcp.directory/blog/agent-client-protocol-vs-mcp-2026), [noqta](https://www.noqta.tn/en/blog/agent-client-protocol-acp-universal-ai-coding-editor-standard-2026).

## Config examples

These are sketches. The OpenCode and Pi flags still need checking against each tool's docs.

### Harness templates: `.legion2/harnesses/<name>.json`

Legion fills in the `{blanks}` and runs the command.

Claude Code, which is what legion already runs today:

```json
{
  "$schema": "https://legion/schemas/harness.json",
  "name": "claude-code",
  "command": "claude",
  "args": [
    "--mcp-config", "{toolsConfig}",
    "--append-system-prompt-file", "{systemPromptFile}",
    "--model", "{model}",
    "--permission-mode", "{permissionMode}",
    "--", "{firstPrompt}"
  ],
  "tools": "mcp"
}
```

OpenCode, using MCP:

```json
{
  "$schema": "https://legion/schemas/harness.json",
  "name": "opencode",
  "command": "opencode",
  "args": ["--model", "{modelProvider}/{model}", "--prompt", "{firstPrompt}"],
  "env": { "OPENCODE_CONFIG": "{harnessConfigFile}" },
  "tools": "mcp"
}
```

Pi, using plain commands:

```json
{
  "$schema": "https://legion/schemas/harness.json",
  "name": "pi",
  "command": "pi",
  "args": [
    "--provider", "{modelProvider}",
    "--model", "{model}",
    "--append-system-prompt", "{systemPromptFile}",
    "{firstPrompt}"
  ],
  "tools": "commands"
}
```

`tools` says which way in the harness uses: `mcp` gets the tool server, and `commands` gets the three settings as environment variables so it can call `legion2 handoff`, `legion2 ask` and the rest.

### Models: `.legion2/legion.json`

```json
{
  "name": "temp_plus_platform",
  "strategist": { "enabled": true, "everyMinutes": 20 },
  "models": {
    "claude-opus":  { "harness": "claude-code", "model": "opus" },
    "spark-big":    { "harness": "opencode", "address": "http://spark:8000/v1", "model": "<model>" },
    "spark-fast":   { "harness": "pi",       "address": "http://spark:8002/v1", "model": "<model>" }
  }
}
```

### Operators: `.legion2/operators/<name>/operator.json`

An operator picks one of the named models. The harness comes with it.

```json
{
  "model": "spark-fast",
  "limit": 2,
  "permissionMode": "auto"
}
```

Running only on local models then means every operator, and the commander, picks a `spark-*` entry.

### Status reports from a session

The session calls this through legion's tools, either by MCP or as a plain command:

```
legion2 report working  "running the db checks"
legion2 report done     "handed off to reviewer"
legion2 report stuck    "stack won't start; asked the commander"
```

(`legion2 status` is already taken by the command that shows what's going on, hence `report`.)

## Running models on the Spark

### The machine

An NVIDIA GB10 with 121 GiB of memory shared by the processor and the graphics chip. That's plenty of room, but the memory is slow to read. So the models that run fast here are the ones that use only a small part of themselves for each word ("mixture of experts" models, written like `35B-A3B`: 35B in total, 3B used per word). Models that use all of themselves every time (`27B`, `32B`) are smarter for their size but slower.

### What runs models today

Ollama, as `ollama.service`, on port 11434, open to the network. Nothing is loaded right now. Installed: qwen3.6 (and a `:scan` copy), qwen2.5-coder:32b (and two copies under other names), gemma3:27b. qwen2.5-coder and gemma3 are outdated for agent work and can go.

### Switch to vLLM

- **Why:** several operators will call the Spark at the same time. vLLM answers many requests together. Ollama mostly answers them one after another, so the second operator waits on the first.
- **NVIDIA's version:** a ready-made Docker image (`nvcr.io/nvidia/vllm`) from NVIDIA's download site (NGC), already set up for the Spark's chip. NVIDIA's Spark setup guides walk through running it. Downloading it may need a free NVIDIA account key.
- **Plain vLLM** works too, but the Spark pairs an uncommon processor (ARM) with a very new graphics chip, so getting it to install and use the chip properly can mean hunting for matching versions of its parts. Start with NVIDIA's image.
- **One model per vLLM:** each model runs as its own vLLM, on its own port, and each grabs a fixed share of the memory when it starts. Set those shares so the models we run together fit side by side, with room left for their conversations.
- **Room for long conversations:** set each model's context size high (64K+ words). Operators read a lot of files, and the defaults are too small for them.
- **Address:** vLLM speaks the same language as OpenAI's service (`http://spark:<port>/v1`), which OpenCode and Pi both understand.
- Keep Ollama around for trying out new models quickly. It's easy to set up and is fine for one or two operators at a time.

### Cost and licenses

vLLM and every model below are open source under licenses that let us use them at work for free. Once a model is downloaded, it runs on the Spark with no tokens and no outside service.

## Which models could stand in for Claude

From what fits on one Spark as of October 2026. The scores are the makers' own, from their own test setups, so treat them as rough and try each model on real legion work before trusting it.

| Stands in for | Model | Speed on one Spark | Coding score* | Notes |
|---|---|---|---|---|
| Haiku | **Qwen3.6-35B-A3B** | ~80–100 words/s | 73.4 | Already installed (as `qwen3.6`). Fast and cheap. Good for small, clear jobs: lookups, checks, simple edits. |
| Sonnet | **Qwen3.6-27B** | ~30 words/s | 77.2 | The strongest coder that fits. Slower because it uses all of itself every time. |
| Sonnet (newer) | **Qwen3.8-27B** | ~50 words/s (one report) | not yet checked | Released Aug 2026. Worth testing against Qwen3.6-27B once there are real reviews. |
| Sonnet (other choice) | **Qwen3-Coder-Next** (80B-A3B) | ~47 words/s | 70.6 | Built for coding agents. One Spark owner reported it handled tool calls reliably with OpenCode. |
| Opus | **Nothing on one Spark** | | | The open models near Opus (DeepSeek V4 Flash, ~300B in total) need two Sparks linked together, and run at ~15 words/s even then. On one Spark, the closest is the best of the Sonnet row. |

\* SWE-bench Verified: the share of real GitHub bugs the model fixed on its own.

**What this means for the plan:** the Spark can take over Haiku-level and some Sonnet-level operator work. Opus-level work (the commander, hard reviews, tricky debugging) stays on Claude.

**First lineup to try:** Qwen3.6-35B-A3B as `spark-fast` and Qwen3.6-27B as `spark-big`, each in its own vLLM. At 4-bit they need roughly 22 GB and 17 GB, which leaves plenty of room for long conversations and for several operators at once.

### Later: an Opus-level model for overnight runs

DwarfStar 4 (formerly `ds4.c`, by antirez, who made Redis) is a small program written in C that runs DeepSeek V4 Flash, an open model near Opus level, on one Spark. It fits by shrinking the model hard and reading parts of it from the SSD as needed. One person has already run it on a Spark.

- **Speed:** about 8–15 words/s writing (Qwen3.6-27B does ~30, Qwen3.6-35B-A3B does ~80–100). Reading is slower still, about 250 words/s, so a prompt with a lot of files in it can take a few minutes before any answer starts. It saves prompts it has already read to disk, so resending the same big prompt drops from ~2 minutes to a few seconds.
- **Memory:** it uses almost all of the Spark (~115 GB), so it can't run next to the daytime models.
- **Fit:** too slow for daytime operator work, but a good match for overnight runs: deep code reviews, long investigations, hard jobs the daytime models can't do. At night we'd stop the daytime vLLMs, start DwarfStar 4, and swap back in the morning.
- **Quality:** no formal tests yet. One tester found its code review and summaries sensible.

Not part of the first version. Look at it once the daytime lineup works.

Sources: [DwarfStar 4 on a DGX Spark (May 2026)](https://dev.classmethod.jp/en/articles/dgx-spark-dwarfstar4-deepseek-v4-flash-bench/), [how ds4 reads from the SSD](https://instagit.com/antirez/ds4/how-ds4-ssd-streaming-works-for-large-models/), [ds4 roadmap](https://pasqualepillitteri.it/de/news/2256/ds4-antirez-deepseek-v4-flash-inferenz-engine)

**Dropped from earlier ideas:** gpt-oss-120b and GLM-4.5-Air. Newer Qwen models beat them at coding, and Spark owners have reported gpt-oss-120b sending broken tool calls.

### Sources

- [GPUwerk: best open-weight models to run on a DGX Spark (Sep 2026)](https://gpuwerk.com/dgx-spark/guides/best-models-2026/)
- [Qwen3.6-27B announcement](https://qwen.ai/blog?id=qwen3.6-27b) and [model card](https://huggingface.co/Qwen/Qwen3.6-27B)
- [NVIDIA forum: Qwen3.6-35B-A3B on the Spark](https://forums.developer.nvidia.com/t/qwen-qwen3-6-35b-a3b-and-fp8-has-landed/366822)
- [NVIDIA forum: best models for agentic coding on the Spark](https://forums.developer.nvidia.com/t/best-models-configurations-for-agentic-coding-with-dgx-nvidia-asus-dell-lenovo-msi/366825)
- [ai-muninn: 8 models on the Spark for agents](https://ai-muninn.com/en/blog/dgx-spark-ollama-benchmark-8-models)
- [Can the Spark run DeepSeek V4 Flash?](https://www.mayhemcode.com/2026/08/can-dgx-spark-run-deepseek-v4-flash-304b.html)
- [Qwen3.8 open-weights release](https://runtimewire.com/article/alibaba-qwen38-24t-open-weights-enterprise-license)

## Set aside for now

- Testing the lineup on real legion work, and settling on the models once that's done.
- DwarfStar 4 for overnight runs (see above).
