# Moving a machine to the new Legion

The old Legion ran squads in tmux: a bash script, `legion`, plus
`legion-mcp`, `legion-dash` and `legion-channeld`. The new one is a
background program, `legiond`, driven by the `legion` command or the app.
While it was being built it was called `legion2`. Some machines have only
the old version, some have `legion2`, and some have both. Follow the
sections that apply.

Do this only when nothing is running. Stopping either version cuts its
sessions off.

## 1. Check what's there

```sh
ls ~/.local/bin | grep legion                # legion, legion-mcp, legion-dash, legion-channeld, legion2, legion2d
ls ~/.local/share | grep legion              # legion (old), legion2
ls ~/Library/LaunchAgents | grep legion      # com.legion2.legion2d.plist
claude mcp list | grep legion                # the old version's Legion tools
```

## 2. Remove the old tmux version

1. Stand every squad down (`legion stand-down` in each project), and check
   that no Legion windows are left in `tmux ls`.
2. Archive its files rather than deleting them:

   ```sh
   ARCHIVE=~/.local/share/legion-tmux-archive-$(date +%F)
   mkdir -p "$ARCHIVE"
   pkill -f ~/.local/bin/legion-channeld || true
   mv ~/.local/share/legion "$ARCHIVE/share"
   for old in legion legion-mcp legion-dash legion-channeld; do
     mv ~/.local/bin/$old "$ARCHIVE/$old"
   done
   ```

3. Remove its tools from Claude: `claude mcp remove legion -s user`. If
   `claude mcp list` still shows `legion`, it's set up for a single project
   too. Remove it there with `-s project` or `-s local`, from that project.
4. Each project the old version ran in has an old-format `.legion/` folder.
   The new version uses the same folder name, so move the old one aside
   first: `mv .legion .legion-tmux`. Run `legion setup init` later (step 5)
   and copy over any operator definitions you want to keep
   (`.legion-tmux/operators/<name>/definition.md` into
   `.legion/operators/<name>/`).

## 3. Move legion2 to legion

Only on a machine that had `legion2`:

```sh
legion2 service remove --yes                 # stops legion2d
mv ~/.local/share/legion2 ~/.local/share/legion
mv ~/.local/share/legion/legion2d.log ~/.local/share/legion/legiond.log
rm -f ~/.local/share/legion/legion2d.sock
cat ~/.local/share/legion/folders.json       # the folders legion2 knew about
```

In each folder listed there, run `mv .legion2 .legion`. If `.legion2` is
tracked in that folder's git, use `git mv .legion2 .legion` and commit it.
Then run `rm ~/.local/bin/legion2 ~/.local/bin/legion2d`.

## 4. Build and install

You need Rust (`cargo`), Node with `npm`, and Claude Code.

```sh
cd <this repo>
git pull
cargo build --release -p legion -p legiond
(cd app && npm ci && npm run build)
cp target/release/legion  ~/.local/bin/legion
cp target/release/legiond ~/.local/bin/legiond
legion service install --claude "$(command -v claude)"
```

Keep the repo where it is. `legiond` loads its Claude add-on (`addon/`) and
the browser app (`app/dist`) from the folder it was built in. If you move
the repo, build and install again.

## 5. Check it works

```sh
legion status                                # in a folder Legion knows
tail -n 20 ~/.local/share/legion/legiond.log
```

Open http://127.0.0.1:4610 for the app. In a new project, `legion setup
init` creates `.legion/`. See [legion.md](legion.md) for everything else.
