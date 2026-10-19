# Progress Log

> **Resume here.** This file is the hand-off note: where the project stands, what
> was done, and the exact next step.

## Status snapshot

- `cargo test` → **13 tests green**
- `cargo clippy --all-targets` → **clean**
- App launches on NixOS via `steam-run` (the plain loader can't find the
  wayland/xkbcommon libs).
- Last completed batch: **symmetry painting + lasso/polygon selection**.

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

## Next step (exact)

### 1. OS clipboard (Ctrl-C / Ctrl-X / Ctrl-V)

egui 0.37 already surfaces the OS clipboard, so this is mostly wiring:

- `input.rs::handle_input` currently ignores `Event::Copy`, `Event::Cut`, and
  `Event::Paste`/`Event::PasteImage`. Map:
  - `Event::Copy` → `editor.yank()` and push the selection to the OS clipboard.
  - `Event::Cut` → `editor.cut()` + copy to OS clipboard.
  - `Event::PasteImage(img)` / `Event::Paste(text)` → build a `Clipboard` and
    enter paste mode.
- Copy out: `ui.ctx().copy_image(ColorImage)` (for the selection, cropped to its
  bounding box; transparent outside the mask). Paste in: `Event::PasteImage`
  arrives as `Arc<ColorImage>` — convert to `Clipboard`.
- Keep the existing internal `y`/`x`/`p` as the keyboard-first path.

### 2. Timeline thumbnails

- In `gui.rs::timeline`, render a small preview per frame (a mini composite).
- Simplest: paint `sel`-scaled filled rects from `sprite.composite(frame)`, or
  cache one `TextureHandle` per frame (invalidate on edit/frame change).

## Known limitations (do not forget)

- Symmetry does **not** apply to bucket fill or paste.
- Lasso/polygon are mouse-driven; keyboard selection is marquee (`v`) + magic
  wand (`w`).
- Magic wand is exact-match, active layer only (no tolerance, no all-layers).
- Selection is single-layer for copy/cut/move (not merged across layers).
- `.ase` export is depth-32 RGBA only (indexed import is supported).
- No selection add/subtract/intersect yet, no selection transforms.
- Rendering is CPU-composited; no custom shaders (kept backend-agnostic on
  purpose for the future glow→wgpu switch).

## Not verified

Interaction paths are compiled + unit-tested and the app launches, but **not
hand-tested on a live display** in this environment. First manual pass should
cover: draw/erase, `v` marquee → `m` move, `w` wand, `L` lasso drag, `P` polygon
+ `Enter`, `s` symmetry painting, `:w x.ase` then reopen.
