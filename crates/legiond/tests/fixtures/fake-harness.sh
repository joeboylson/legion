#!/bin/sh
# Stands in for a harness or for Claude Code in tests. `--version` answers like Claude Code; otherwise it writes its working folder, arguments, environment and the file named by $FAKE_CONFIG to $FAKE_HARNESS_OUT/<its LEGION_POSITION>.txt, then waits to be ended.
set -eu
if [ "${1:-}" = "--version" ]; then
  echo "2.1.288 (Claude Code)"
  exit 0
fi
out="$FAKE_HARNESS_OUT/${LEGION_POSITION:-unknown}.txt"
{
  echo "cwd: $(pwd -P)"
  for argument in "$@"; do echo "arg: $argument"; done
  env | sort | sed 's/^/env: /'
  if [ -n "${FAKE_CONFIG:-}" ]; then
    echo "config:"
    cat "$FAKE_CONFIG"
    echo
  fi
} > "$out.part"
mv "$out.part" "$out"
# A file named slow-exit in the out folder makes it take a second to go once told to.
if [ -e "$FAKE_HARNESS_OUT/slow-exit" ]; then
  trap 'sleep 1; exit 0' HUP TERM INT
  sleep 3600 &
  wait
fi
exec sleep 3600
