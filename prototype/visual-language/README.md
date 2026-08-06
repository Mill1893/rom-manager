# Visual language prototype

Throwaway artifact for **Prototype the Library Browser visual language**.
It keeps the settled three-pane Library Browser interaction model fixed while
three visual systems vary theme, typography, density, spacing, box-art
treatment, status expression, and destructive-state treatment.

From `prototype/` run:

```sh
python3 -m http.server 8000
```

Open <http://localhost:8000/visual-language/>. Use the floating controls or
the left and right arrow keys to switch variants. `Risk state` opens the same
blocked, destructive Sync Plan in each visual system. `200%` simulates the
effective viewport and type size of application scaling at 200 percent.

- **A - Cartridge Index:** warm, archival, editorial, moderately dense.
- **B - Signal Deck:** monochrome technical utility, highly dense.
- **C - After Hours:** cinematic dark gallery, spacious and media-forward.

The artifact is not production code and should not be merged into `main`.
