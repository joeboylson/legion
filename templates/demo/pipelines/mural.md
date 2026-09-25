# mural

One mission per tile. The painter draws it, the critic checks it against the brief, and a tile that breaks the rules goes back to its painter. The critic sends a tile back at most once, and only for a broken seam point, ring arc or colour.

```mermaid
flowchart TD
    painter[painter: draw the tile in tiles/tile-TILE.svg, following brief.md] --> critic[critic: check the tile's palette, rules, seam points and ring arcs]
    critic --> ok{the tile's seams, rings and colours are right, or it has been sent back once already}
    ok -->|no| painter
    ok -->|yes| done([done: report the tile to commander])
```
