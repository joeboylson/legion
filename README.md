# legion

Legion runs a team of [Claude Code](https://claude.com/claude-code) sessions
for each folder you add. A background program, `legiond`, keeps them running.
You drive it from the `legion` command or the Legion app.

```sh
legion service install --claude ~/.local/bin/claude   # runs legiond now and at every login
cd ~/code/myapp
legion setup init                                   # sets up .legion/ here
legion deployment start --pipeline feature --name first   # starts a commander
legion mission add --title "Add a login page" --text "…"
```

- [docs/legion.md](docs/legion.md): how it works.
- [docs/tauri-rewrite.md](docs/tauri-rewrite.md): the design, and why.
- [HANDOFF.md](HANDOFF.md): where the work stands.
