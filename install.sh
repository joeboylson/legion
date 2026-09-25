#!/usr/bin/env bash
# Installs legion.
# Usage: curl -fsSL https://raw.githubusercontent.com/joeboylson/legion/main/install.sh | bash
#    or, from a clone, ./install.sh to install (or reinstall) that clone's copy.
set -euo pipefail

REPO="https://github.com/joeboylson/legion.git"

# Run from a clone: install the files next to this script as they are, local
# edits included. Piped from curl there's no script file, so fetch main.
SRC=""
if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  [ -f "$here/bin/legion" ] && [ -d "$here/templates" ] && SRC="$here"
fi
if [ -z "$SRC" ]; then
  SRC="$(mktemp -d)"
  trap 'rm -rf "$SRC"' EXIT
  echo "Cloning legion..."
  git clone --depth 1 "$REPO" "$SRC" >/dev/null 2>&1
else
  echo "Installing from $SRC"
fi

mkdir -p "$HOME/.local/bin" "$HOME/.local/share/legion"
cp "$SRC/bin/legion" "$HOME/.local/bin/legion"
chmod +x "$HOME/.local/bin/legion"
rm -rf "$HOME/.local/share/legion/templates"
cp -R "$SRC/templates" "$HOME/.local/share/legion/templates"

echo "legion installed to ~/.local/bin/legion"
if ! command -v legion >/dev/null 2>&1; then
  echo
  echo "~/.local/bin is not on your PATH yet — add this to your shell rc, then restart your shell:"
  echo '  export PATH="$HOME/.local/bin:$PATH"'
fi
echo
echo "next: cd your-project && legion init"
