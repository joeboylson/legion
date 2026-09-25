#!/usr/bin/env bash
# Installs legion.
# Usage: curl -fsSL https://raw.githubusercontent.com/joeboylson/legion/main/install.sh | bash
set -euo pipefail

REPO="https://github.com/joeboylson/legion.git"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Cloning legion..."
git clone --depth 1 "$REPO" "$TMP" >/dev/null 2>&1

mkdir -p "$HOME/.local/bin" "$HOME/.local/share/legion"
cp "$TMP/bin/legion" "$HOME/.local/bin/legion"
chmod +x "$HOME/.local/bin/legion"
rm -rf "$HOME/.local/share/legion/templates"
cp -R "$TMP/templates" "$HOME/.local/share/legion/templates"

echo "legion installed to ~/.local/bin/legion"
if ! command -v legion >/dev/null 2>&1; then
  echo
  echo "~/.local/bin is not on your PATH yet — add this to your shell rc, then restart your shell:"
  echo '  export PATH="$HOME/.local/bin:$PATH"'
fi
echo
echo "next: cd your-project && legion init"
