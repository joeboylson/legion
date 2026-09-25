# Mural brief: "Signal"

An abstract circuit board at night: traces, nodes and chips glowing on a dark board. Four painters each draw one tile at the same time. Where two tiles meet, their traces have to line up exactly, so the four tiles read as one picture.

## Canvas

- The mural is 800 × 800, split into four 400 × 400 tiles: `nw` (top left), `ne` (top right), `sw` (bottom left) and `se` (bottom right).
- Each tile is its own SVG file, `tiles/tile-<tile>.svg`, with `viewBox="0 0 400 400"`, `width="400"` and `height="400"`. All coordinates below are the tile's own, from 0 to 400.
- Start every tile with a full background: `<rect width="400" height="400" fill="#0A0E1A"/>`.

## Palette

The only colours allowed, plus the background:

| Name | Hex |
|---|---|
| Cyan | `#00E5FF` |
| Magenta | `#FF2E88` |
| Amber | `#FFB400` |
| Violet | `#7C4DFF` |

Opacity is fine. Gradients, filters, text and images aren't allowed.

## Drawing rules

- Traces are lines or polylines with `stroke-width="4"`, `fill="none"` and `stroke-linecap="round"`. They run only horizontally, vertically or at 45°, and every corner sits on the 20-pixel grid (x and y are multiples of 20).
- Nodes are circles with `r="5"` or `r="8"`, filled with a palette colour, at trace ends and junctions.
- Chips are rectangles with a 2-pixel palette-coloured stroke and background fill. Each tile has one or two, with short "pins" (8-pixel traces) along their sides.
- Keep it small: 15 to 30 elements per tile, fixed parts included. Leave plenty of dark space. Speed matters more than detail.

## The seams (must match exactly)

Each crossing below is a trace in the given colour that touches the tile's edge at exactly that point, and runs straight out from the edge (perpendicular) for at least 20 pixels. Both tiles that share a seam draw their half, so the trace carries straight across.

**Middle vertical seam.** `nw` and `sw` draw these on their right edge (x = 400); `ne` and `se` draw them on their left edge (x = 0).
- Top pair (`nw` ↔ `ne`): y = 80 Cyan, y = 200 Magenta, y = 320 Amber
- Bottom pair (`sw` ↔ `se`): y = 60 Violet, y = 180 Cyan, y = 300 Magenta

**Middle horizontal seam.** `nw` and `ne` draw these on their bottom edge (y = 400); `sw` and `se` draw them on their top edge (y = 0).
- Left pair (`nw` ↔ `sw`): x = 100 Amber, x = 260 Cyan
- Right pair (`ne` ↔ `se`): x = 140 Magenta, x = 300 Violet

## The centrepiece

Two rings centred on the middle of the mural, where all four tiles meet. Each tile draws its quarter of both as an arc (`<path>` with an `A` command, `fill="none"`, `stroke-width="3"`):
- the inner ring, radius 160, Cyan
- the outer ring, radius 240, Magenta

The ring's centre, in each tile's own coordinates: `nw` (400, 400), `ne` (0, 400), `sw` (400, 0), `se` (0, 0).

## Yours to decide

Everything else: where the traces wander between the fixed points, where the chips sit, and which way the circuit flows. Make your tile interesting.
