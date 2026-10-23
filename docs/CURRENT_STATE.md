# Current State

Last updated: 2026-10-22 — after the filled-shapes + wand/selection-ops batch.

## Summary

`vixel` is a working, keyboard-first pixel-art editor in Rust with a native GUI
window (`eframe` 0.37, OpenGL/`glow`, wgpu-swappable). It has true 24-bit color,
real alpha transparency, zoom, onion skinning, per-layer blend modes, animation
frames, a full vim keymap, and `.ase`/`.aseprite` + PNG + `.vpx` file support.

The architecture keeps the **model GUI-agnostic** and the view/input as thin
shells (see `AGENTS.md` for the module map and invariants).

## Editing modes

| Mode | Purpose |
|------|---------|
| normal | move cursor, issue commands, select tools |
| draw (`i`) | paint as you move (pencil = fg, eraser = transparent) |
| visual (`v`) | marquee selection, then act on it |
| paste (`p`) | float the clipboard, reposition, commit |
| command (`:`) | type a command |
| help (`?`) | keymap overlay |

## Tools

pencil, eraser, bucket (flood fill), picker (eyedropper), line, rectangle,
ellipse (outline or **filled**) — all with live preview where applicable — plus
**symmetry painting** (mirror across horizontal/vertical/both axes).

## Selection & clipboard

- **Marquee** (`v`, `V` = whole canvas), **magic wand** (`w`, contiguous region,
  with **tolerance** and optional **all-layers** matching), **lasso** (`L`,
  freehand mouse drag), and **polygon** (`P`, click vertices, `Enter` closes).
- **Selection ops** (`:selop` or the toolbar): replace / add / subtract / intersect
  with the existing selection.
- **Mask-based**: selection is a bounding box + per-pixel mask, so copy/cut/
  move/paste preserve arbitrary shapes.
- Operators: `y` copy · `x` cut · `m` move (cut + float) · `p` paste. `Esc`
  clears the selection. One undo step per operation.

## Layers

- visibility toggle, opacity (0–255), 8 **blend modes** (normal, multiply,
  screen, overlay, darken, lighten, add, subtract — W3C source-over math),
  reorder (↑/↓), rename (`:layername`), add/remove.

## Frames & animation

- add / duplicate / remove frames, per-frame duration, playback with onion
  skinning (previous frame tinted red, next green), and **timeline thumbnails**
  (click a thumbnail to switch frames).

## Canvas

- integer zoom (fit + `+`/`-`/scroll), pan (middle-drag), pixel grid overlay,
  transparency checkerboard, cursor + selection + floating-paste previews.

## Color

- default 16-color palette, fg/bg, swap, full RGB picker, eyedropper.

## Undo/redo

- stroke-grouped (a drag = one undo step), `u` / `Ctrl-r`.

## File formats

- **`.ase` / `.aseprite`** — native Aseprite format, read + write. Reads raw and
  zlib-compressed cels, RGBA (depth 32) and indexed/grayscale (depth 8/16 via
  palette chunks), layer names/visibility/blend/opacity, frame durations.
  Writes depth-32 RGBA.
- **`.vpx`** — JSON project file (size, layers, frames, all cels).
- **PNG** — single-frame import/export with alpha.

`:w` / `:export` / `:open` pick the format by file extension.

## Keymap (normal mode)

| Key | Action |
|-----|--------|
| `h` `j` `k` `l` / arrows | move cursor |
| `H` `J` `K` `L` | move 8px |
| `0` `$` `gg` `G` | left / right / top / bottom |
| `i` | draw mode |
| `x` | erase pixel (or cut selection) |
| `r` / `space` | paint fg |
| `b` / `e` / `f` / `c` | pencil / eraser / bucket / picker |
| `d` / `R` / `o` | line / rect / ellipse |
| `v` / `V` | visual select / select all |
| `w` | magic wand |
| `L` / `P` | lasso / polygon selection |
| `y` / `m` / `p` | copy / move / paste |
| `s` | cycle symmetry |
| `t` | toggle filled shapes |
| `,` `.` | prev / next color |
| `<` `>` | prev / next layer |
| `[` `]` | prev / next frame |
| `X` | swap fg/bg |
| `u` / `Ctrl-r` | undo / redo |
| `+` `-` | zoom |
| `:` `?` | command / help |
| `Ctrl-c` / `Ctrl-x` / `Ctrl-v` | copy / cut / paste (OS clipboard) |
| `Ctrl-s` / `Ctrl-q` | save / quit |

Mouse: left = paint fg · right = paint bg · middle = pick · wheel = zoom ·
middle-drag = pan.

## Commands (`:`)

`w`/`save` · `q`/`q!`/`wq` · `new [w h]` · `resize w h` · `newframe`/`dupframe`/
`delframe` · `newlayer`/`dellayer`/`layerup`/`layerdown`/`layername <name>` ·
`fliph`/`flipv`/`rot90` · `sym <mode>` · `wand <0-255>` / `wandall on|off` ·
`selop <op>` · `export <file>` · `import <png>` · `open <file>` · `help`.

## Test status

`cargo test` → 15 tests green; `cargo clippy --all-targets` → clean. Verified to
launch on NixOS via `steam-run`. (Interaction paths are compiled + unit-tested
but not yet hand-tested on a live display.)
