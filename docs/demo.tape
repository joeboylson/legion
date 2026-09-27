# Records a short, single-operator legion flow for the README. Regenerate
# with: vhs docs/demo.tape  (needs vhs: brew install vhs)
#
# Kept deliberately small: one commander, one quick task, one clean attach.
# A fuller multi-operator demo (`legion demo`) exists but is much harder to
# record reliably — cmd_demo's own internal grid-attach, called after
# several other tmux operations in the same script run, fails silently in
# a nested/scripted terminal like this one even though the squad itself
# runs fine underneath. A single, fresh attach right after one session is
# created doesn't hit that problem.

Output docs/demo.gif
Output docs/demo.mp4

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Dracula"
Set TypingSpeed 40ms
Set PlaybackSpeed 2
Set Padding 20

Type "cd /tmp/legion-quickdemo && legion init quickdemo && source .legion/bin/activate"
Enter
Sleep 1.5s

Type `legion commander "Write a short haiku about tmux to haiku.txt, then cat it, then report done and stop."`
Enter
Sleep 1.5s

Type "legion grid"
Enter
Sleep 60s
