# legion viewer

A desktop app for watching a Legion squad work. Squad list on the left,
click into one and get a live grid of every operator's terminal — real
`tmux attach` sessions rendered with [xterm.js](https://xtermjs.org), so the
tiles are genuinely interactive: click into one and type, and you're talking
directly to that operator, the same as attaching by hand.

## How it works

- Rust backend spawns a real PTY per tile running `tmux attach-session -t
  <squad>-<operator>` (via [`portable-pty`](https://docs.rs/portable-pty)),
  and streams its raw output to the frontend over Tauri events.
- Frontend renders each stream with `xterm.js`, which speaks terminal escape
  codes natively — full color, correct redraws, no polling.
- Operators are discovered by filtering `tmux list-windows` for the
  `<squad>-` prefix — no separate registry, same convention `legion` itself
  uses for addressing.
- Known squad projects are stored in a small local JSON config
  (`app_data_dir/squads.json`), added via a folder picker that validates
  `.legion/squad` exists at the chosen path.

Since it's a real second tmux client attached to the same window the actual
`claude` session is running in, sizing follows normal tmux multi-client rules
— the window's real terminal size can shrink to fit whichever attached
client is smallest, same as opening the same tmux session in two real
terminals.

## Development

```bash
npm install
npm run tauri dev
```

Needs the Tauri prerequisites (Rust toolchain, platform webview) — see
[tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/).
