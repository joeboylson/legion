#!/usr/bin/env bash
# Stitches the four tiles into mural.html. Run from the demo folder.
#   bash assemble.sh          the finished mural
#   bash assemble.sh --live   a page that reloads every 2 seconds and shows a
#                             placeholder for any tile that isn't painted yet
set -euo pipefail
live=0
[ "${1:-}" = "--live" ] && live=1
placeholder() {
  cat <<SVG
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 400" width="400" height="400"><rect width="400" height="400" fill="#0A0E1A"/><rect x="1" y="1" width="398" height="398" fill="none" stroke="#1B2238" stroke-dasharray="6 6"/><text x="200" y="205" fill="#3A4468" font-family="ui-monospace, Menlo, monospace" font-size="16" text-anchor="middle">$1 · painting…</text></svg>
SVG
}
tile() {
  if [ -s "tiles/tile-$1.svg" ]; then sed '1{/^<?xml/d;}' "tiles/tile-$1.svg"; else placeholder "$1"; fi
}
# The finished mural is final: once it's written, live refreshes stop.
if [ "$live" -eq 1 ] && [ -f .final ]; then exit 0; fi
painted=0
for t in nw ne sw se; do [ -s "tiles/tile-$t.svg" ] && painted=$((painted + 1)); done
if [ "$live" -eq 0 ] && [ "$painted" -lt 4 ]; then echo "only $painted of 4 tiles are painted" >&2; exit 1; fi
if [ "$live" -eq 0 ]; then touch .final; fi
refresh=""
status='"Signal": four painters, one critic, one commander. Made by a Legion squad.'
if [ "$live" -eq 1 ]; then
  refresh='<meta http-equiv="refresh" content="2">'
  status="Live: $painted of 4 tiles painted. The squad is still working."
fi
{
  cat <<HTML
<!doctype html>
<html lang="en"><head><meta charset="utf-8">$refresh<title>Signal: a Legion mural</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #05070D; color: #8B93B8; font: 14px/1.5 ui-monospace, Menlo, monospace; }
  .mural { display: grid; grid-template-columns: 400px 400px; box-shadow: 0 0 80px #00E5FF22; }
  .mural svg { display: block; width: 400px; height: 400px; }
  p { text-align: center; margin: 20px 0 0; }
</style></head><body><div>
<div class="mural">
HTML
  for t in nw ne sw se; do tile "$t"; done
  cat <<HTML
</div>
<p>$status</p>
</div></body></html>
HTML
} > mural.html.tmp
mv mural.html.tmp mural.html
[ "$live" -eq 1 ] || echo "wrote $(pwd)/mural.html"
