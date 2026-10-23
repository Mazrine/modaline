# Progress Log

> **Resume here.** This file is the hand-off note: where the project stands, what
> was done, and the exact next step.

## Status snapshot

- `cargo test` → **16 tests green**
- `cargo clippy --all-targets` → **clean**
- App launches on NixOS via `steam-run` (the plain loader can't find the
  wayland/xkbcommon libs).
- Last completed batch: **gradient / spray / shade tools**.

## How to resume

```sh
cd ~/projects/vixel
cargo test
cargo clippy --all-targets
steam-run cargo run --release      # or: cargo build --release && steam-run ./target/release/vixel
```

Read, in order:
1. `AGENTS.md` — module map + invariants (must not break these).
2. `docs/CURRENT_STATE.md` — what is implemented today.
3. `docs/ROADMAP.md` — planned work, ordered by priority.

## Batch history

1. **TUI v1** (ratatui) — built, then superseded by the GUI port.
2. **GUI port** — `eframe` 0.37 / glow; canvas as a single `TextureHandle`;
   true alpha, checkerboard, zoom, grid, onion skin, playback. Model reused as-is.
3. **Editing batch** — mask selection + copy/cut/move/paste, 8 per-layer blend
   modes, layer reorder/rename, duplicate frame, flip H/V + rotate 90°, ellipse.
4. **Selection/file batch** — magic wand, generalized mask-based selection,
   `.ase`/`.aseprite` import/export (raw + zlib cels, RGBA + indexed), extension-
   aware `:w`/`:export`/`:open`.
5. **Docs** — `README.md`, `AGENTS.md`, `CLAUDE.md`, `docs/CURRENT_STATE.md`,
   `docs/ROADMAP.md`.
6. **Draw/select batch** — symmetry painting (`s`) + lasso (`L`) / polygonal
   (`P`) selection. 3 new tests. Docs updated.
7. **Clipboard + thumbnails batch** — OS clipboard (Ctrl-C/X/V, image
   copy/paste through egui), timeline thumbnails, and the Ctrl-C→copy /
   Ctrl-Q→quit / Ctrl-Z→undo remap.
8. **Shapes + selection-quality batch** — filled rect/ellipse (`t`), magic-wand
   tolerance + all-layers, and selection ops (replace/add/subtract/intersect).
   2 new tests.
9. **Toolset batch** — gradient (linear/radial, `z`), spray (`a`), shade (`q`,
   right-click lightens). 1 new test.

## Next step (exact)

### 1. Selection transforms

- flip / rotate the **selected content** (active cel), not the whole sprite.
- Single-cel, so it records one undo step. Flip H/V can be mask-aware; rotate
  maps the bbox dimensions (w×h → h×w).

### 2. Blur + contour tools

- blur: average neighboring pixels (box blur, radius 1) under the cursor.
- contour: outline the selected / contiguous region.

### 3. Layer groups & masks

- Aseprite-style layer groups and per-layer alpha masks.

### 4. Aseprite format fidelity

- linked cels, cel opacity, and indexed-color export.

## Known limitations (do not forget)

- Symmetry does **not** apply to bucket fill or paste.
- Lasso/polygon are mouse-driven; keyboard selection is marquee (`v`) + magic
  wand (`w`).
- Selection is single-layer for copy/cut/move (not merged across layers).
- `.ase` export is depth-32 RGBA only (indexed import is supported).
- Whole-sprite transforms (`:fliph`/`:flipv`/`:rot90`) are not undoable yet;
  selection transforms will be.
- Timeline thumbnails rebuild every UI pass (fine for small sprites; cache by
  revision if it ever matters).
- OS paste prefers clipboard *text* when present (egui behavior); image paste is
  used when the clipboard has no usable text.
- Rendering is CPU-composited; no custom shaders (kept backend-agnostic on
  purpose for the future glow→wgpu switch).

## Not verified

Interaction paths are compiled + unit-tested and the app launches, but **not
hand-tested on a live display** in this environment. First manual pass should
cover: draw/erase, `v` marquee → `m` move, `w` wand, `L` lasso drag, `P` polygon
+ `Enter`, `s` symmetry painting, `z` gradient, `a` spray, `q` shade,
`Ctrl-c` in another app → `Ctrl-v` here, `:w x.ase` then reopen.
