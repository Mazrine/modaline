# Progress Log

> **Resume here.** This file is the hand-off note: where the project stands, what
> was done, and the exact next step.

## Status snapshot

- `cargo test` → **26 tests green**
- `cargo clippy --all-targets` → **clean**
- App launches on NixOS via `steam-run` (the plain loader can't find the
  wayland/xkbcommon libs).
- Last completed batch: **layer groups & masks**.

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
10. **Transforms + blur/contour batch** — `:fliph`/`:flipv`/`:rot90` now transform
    the selection's content (single cel, undoable) when a selection exists;
    blur (`n`) and contour (`C`) tools. 2 new tests.
11. **Palettes + transform-undo batch** — `.gpl`/`.hex`/`.pal` palette
    import/export (`:pal`), palette add/remove UI, and whole-sprite transforms
    are now undoable via a sprite-snapshot undo step. 3 new tests.
12. **Layer groups & masks batch** — nested groups via a flat `child_level`
    tree (Aseprite-style): group compositing with group visibility / opacity /
    blend / mask, subtree-aware add/remove/move, `:newgroup` / `:indent` /
    `:outdent`, per-layer `editable` lock (`:lock`/`:unlock`) enforced on all
    paint paths, per-layer alpha masks (`:mask add|apply|del`) that follow
    flips/rotate/resize and persist in `.vpx`. `.ase` now reads/writes layer
    groups, child levels and the editable flag. 5 new tests.

## Next step (exact)

The immediate order is below. **The full backlog lives in `docs/ROADMAP.md`** —
every remaining item is recorded there, so nothing depends on chat history.

### 1. Aseprite format fidelity

- Read linked cels (type 1) and compressed tilesets (type 3); read/write cel
  opacity; write indexed-color (depth 8/16); add round-trip tests against real
  Aseprite-saved fixtures. (Layer groups + child levels now round-trip.)

### 2. Tile mode

- Tile grid + wrapping paint for seamless textures.

### After that (P1+)

Frame tags & reorder, onion-skin settings, layer drag-reorder, text tool, palette
reorder / HSV-hex picker / color modes, symmetry settings, grid & snap, export
options (sprite sheet/GIF/JSON/slices), filters, selection expand/border/feather,
free transform (rotate/scale/skew), color profiles, shaders/wgpu, persistence —
all detailed in `docs/ROADMAP.md`.

## Known limitations (do not forget)

- Symmetry does **not** apply to bucket fill or paste.
- Mask pixels cannot be painted directly: masks are created from the selection
  (or fully opaque), baked, or removed.
- New layers are added as siblings; nesting is an explicit `:indent`.
- New groups start empty and do not adopt the active layer.
- `.ase` carries groups/flags but not masks (masks are `.vpx`-only).
- Lasso/polygon are mouse-driven; keyboard selection is marquee (`v`) + magic
  wand (`w`).
- Selection is single-layer for copy/cut/move (not merged across layers).
- `.ase` export is depth-32 RGBA only (indexed import is supported).
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
+ `Enter`, `s` symmetry painting, `z` gradient, `a` spray, `q` shade, `n` blur,
`C` contour, `:fliph` with a selection, `Ctrl-c` in another app → `Ctrl-v` here,
`:w x.ase` then reopen.
