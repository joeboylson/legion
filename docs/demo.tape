# Records a short legion flow for the README. Regenerate with:
#   vhs docs/demo.tape
# (needs vhs: brew install vhs — and run with TMUX unset if you're
# recording from inside a tmux session yourself: env -u TMUX vhs docs/demo.tape.
# $TMUX leaking into vhs's own spawned shell makes legion grid take the
# switch-client branch instead of attach-session, which fails silently
# since that shell isn't really a tmux client — cost real time to track
# down, see the cmd_grid/cmd_attach fix in bin/legion.)
#
# One commander, two operators each writing one stanza of a poem in turn —
# every command typed and run on its own, nothing chained with && so the
# real step-by-step shape of using legion is visible, not a shortcut.

Output docs/demo.gif
Output docs/demo.mp4

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 750
Set Theme "Dracula"
Set TypingSpeed 40ms
Set PlaybackSpeed 2
Set Padding 20

Type "cd /tmp/legion-quickdemo"
Enter
Sleep 2s

Type "legion init quickdemo"
Enter
Sleep 2s

Type "source .legion/bin/activate"
Enter
Sleep 2s

Type "legion commander"
Enter
Sleep 2s

Type `legion deploy poet-1 "Write the first stanza (4 lines) of a short poem about tmux to poem.txt, then report done and stop."`
Enter
Sleep 2s

Type `legion deploy poet-2 "Wait until poem.txt exists and has a finished first stanza in it (poet-1 is writing it — check every few seconds, or message poet-1 to ask), then add a second stanza (4 lines) that continues the poem, then report done and stop."`
Enter
Sleep 2s

Type "legion grid"
Enter
Sleep 100s
