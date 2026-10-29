# Current State

Last updated: 2026-10-28 — P1-P3 shipped; the open set is `docs/ROADMAP.md` P4.

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

pencil, eraser, bucket (flood fill, contiguous or global, with tolerance),
picker (eyedropper), line, rectangle,
ellipse (outline or **filled**), **gradient (linear/radial)**, **spray**,
**shade** (darken; right-click lightens), **blur**, and **contour** (outline) —
all with live preview where applicable — plus **symmetry painting** (mirror or
radial, including bucket fill and paste). Pencil and eraser share a square
brush, 1-64 px (`:brush` or the panel slider), clipped at the canvas edges.
**Pixel-perfect** freehand (`:perfect on|off`) joins pointer steps and drops
L-shaped corners for the 1 px pencil/eraser.

## Selection & clipboard

- **Marquee** (`v`, `V` = whole canvas), **magic wand** (`w`, contiguous or
  global via `:contig`, with **tolerance** and optional **all-layers**
  matching), **lasso** (`L`, freehand mouse drag), and **polygon** (`P`, click
  vertices, `Enter` closes).
- **Selection ops** (`:selop` or the toolbar): replace / add / subtract / intersect
  with the existing selection.
- **Shape ops** (`:sel`, or the toolbar chips): `expand` / `contract` /
  `border` / `feather <n>`, `invert`, `none`, `all`. They work on a sprite-sized
  grid, so they can grow past the selection's own bbox; feather is a
  deterministic dither ramp (a mask is one bit per pixel).
- **Mask-based**: selection is a bounding box + per-pixel mask, so copy/cut/
  move/paste preserve arbitrary shapes.
- **Paint clip**: every paint tool — pencil, brush, eraser, shapes, text,
  bucket, spray, shade, blur, symmetry mirrors and `Op` replay — is clipped to
  the selection. Paste replaces the selection instead of being clipped, and
  contour draws its ring just outside it.
- Operators: `y` copy (active layer) · `Y` copy merged (flattened visible
  layers) · `x` cut · `m` move (cut + float) · `p` paste; `:copy merged|layer`
  does the same from the command line. `Esc` clears the selection. One undo
  step per operation.

## Layers

- visibility toggle, opacity (0–255), 19 **blend modes** (normal, multiply,
  screen, overlay, darken, lighten, color dodge/burn, hard/soft light,
  difference, exclusion, hue, saturation, color, luminosity, add, subtract,
  divide — W3C source-over math, Aseprite's divide),
  reorder (↑/↓), rename (`:layername`), add/remove, **duplicate**
  (`:duplayer`), **merge down** (`:merge`, bakes opacity/blend/mask) and
  **flatten** (`:flatten`, visible layers to one). Groups can be flattened but
  not duplicated or merged down.
- **nested groups**: `:newgroup` adds a group, `:indent` / `:outdent` (or the
  panel's →/←) nest layers inside it. A group's visibility, opacity, blend
  mode and mask apply to the composite of its children; removing a group
  removes its whole subtree; moving a group moves its subtree.
- **lock / editable flag** per layer: painting, bucket fill and group-level
  locks are refused (`:lock` / `:unlock`, or the `edit` checkbox).
- **alpha masks** per layer: `:mask add` (from the current selection, or full
  opacity), `:mask apply` (bake into the layer's alpha and drop the mask), and
  `:mask del`. Masks are composited as an alpha multiplier and follow flips,
  rotation and resize. They persist in `.vpx`, not in `.ase`.
- **layer kinds**: `:reference on|off` marks a tracing layer (visible on canvas,
  never painted, excluded from exports and flatten; `ref` chip) and
  `:background on|off` marks the opaque bottom layer (`bg` chip). Both flags
  round-trip in `.vpx` and `.ase` and undo with the layer properties.

## Frames & animation

- add / duplicate / remove frames, per-frame duration, playback with onion
  skinning (1-3 frames each side, fading, with configurable tints and opacity),
  and **timeline thumbnails** (click a thumbnail to switch frames; they rebuild
  only when the sprite revision changes). Playback can
  loop or stop at the end, and bounce (**ping-pong**). The timeline also exposes the active
  cel's **opacity** (per layer + frame, as in Aseprite).
- **frame tags** (named inclusive ranges): `:tag <name> [from to]`,
  `:tagdel [name]`; tags are listed above the timeline, clicking one restricts
  playback to its range (click again to clear). Tags carry a play direction
  (forward/reverse/ping-pong, `:tagdir`) and a repeat count (`:tagrepeat`,
  0 = forever); both round-trip in `.vpx` and `.ase` (name, range, color,
  direction, repeat) and follow frame insert/remove.
- **frame reorder**: drag a timeline thumbnail onto another slot, or
  `:framemove <from> <to>`.

## Canvas

- integer zoom (fit + `+`/`-`/scroll), pan (middle-drag), pixel grid overlay,
  transparency checkerboard, cursor + selection + floating-paste previews.
- **design grid** (1-64 px, its own tone) with **snap**: the cursor steps one
  cell and every canvas pick rounds down to the cell.
- **filters** (panel + `:filter`): invert, brightness, contrast, hue,
  saturation, outline, blur, sharpen — selection-aware, one undo step.
- **free transform** (panel + `:rotate`/`:scale`/`:skew`): rotate, scale and
  skew the selection (or the cel) about its centre, nearest-neighbour.
- **crop / trim**: `:crop [x y w h]` (no args = the selection) and `:trim`
  (non-transparent content) resize every cel and layer mask, one undo step.
- **text**: an 8x8 bitmap font at scale 1-8, stamped at the cursor as one undo
  step (`:text <string>`).
- **slice export** (`:slices <dir> [cols rows]`): one PNG per grid cell plus a
  `.slices.json` naming every rect.
- **tile mode** (`:tile on|off`, side-panel checkbox): the sprite is previewed
  tiled 3x3 with a highlighted center, and the cursor wraps across the edges
  (moving right from the last column lands on column 0), so strokes stay
  seamless across the tile boundary. Line, rect, ellipse and gradient take the
  short way round too: geometry past the seam lands on the opposite edge.

## Color

- default 16-color palette, fg/bg, swap, full RGB picker, eyedropper, plus a
  **hex field** and **HSV** drag values for both.
- palette management: add the current fg, remove the selected swatch, drag a
  swatch to reorder (`:pal move <from> <to>`), palette-from-sprite, lock, and
  import/export **`.gpl` (GIMP), `.hex`, and `.pal` (JASC)** via `:pal`.
- **sprite properties**: colour mode (auto / rgba / grayscale / indexed — the
  declared mode drives the .ase depth; grayscale also converts the pixels),
  pixel ratio (written to and read from the .ase header) and the sprite grid.
- **indexed mode**: `:mode indexed` builds a document palette from the pixels
  (exact up to 255 opaque colours, median cut above that), keeps a transparent
  slot, and snaps painting/fills to palette entries. The palette and
  transparent index survive `.vpx` and `.ase`; entries may carry alpha.
- **dithering**: `:dither none|bayer2|bayer4|bayer8|floyd` converts to the
  document palette with an ordered Bayer matrix or Floyd–Steinberg error
  diffusion; `:mode indexed` uses the last choice. Gradients reuse the engine.
- **colour profile**: sRGB or linear light. Converting rewrites every pixel and
  the palette together with the real transfer function (`:profile`).

## Undo/redo

- stroke-grouped (a drag = one undo step), `u` / `Ctrl-r`.
- structural edits are one step each too: add/delete/move/indent/rename/lock
  layers, add/delete/duplicate/move frames, and create/delete tags. The active
  layer/frame/tag are restored with them.
- panel properties (visible, lock, opacity, blend) undo as small `LayerProps`
  steps; an opacity drag is one step.

## File formats

- **`.ase` / `.aseprite`** — native Aseprite format, read + write.
  - Reads raw (0), zlib-compressed (2), **linked (1)** and **compressed tilemap
    (3)** cels, including embedded tileset images with X/Y/diagonal flips.
  - Reads RGBA (32), grayscale (16) and indexed (8) pixels, new and legacy
    palette chunks, layer names/visibility/blend/opacity, layer groups (type +
    child level), the editable flag, **per-cel opacity** and frame durations.
  - Writes the declared colour mode when one is set (`:mode`), else the
    automatically chosen **lossless** depth: grayscale when every pixel is
    gray, indexed when the sprite uses <= 255 colors, else 32-bit RGBA. The
    pixel ratio is written to and read from the header. Unknown chunks
    (slices, user data, ICC, external files, ...) are retained verbatim with
    their frame and anchor, and frame chunk counts are written, so an
    open→save keeps data this build does not interpret yet.
    Layer masks are `.vpx`-only (the `.ase` format has no per-layer mask);
    tilemap layers are rasterized on import (tile editing is out of scope,
    see ROADMAP).
- **`.vpx`** — JSON project file (size, layers, frames, all cels).
- **PNG** — single-frame import/export with alpha; a sheet PNG plus the
  `:export` sidecar (or an explicit grid) imports back as frames
  (`:importsheet`).

`:w` / `:export` / `:open` pick the format by file extension. `:export <target>
[scale 1-16]` upscales PNG, sheet and GIF with nearest-neighbour; `.ase` export
has no scale.

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
| `z` / `a` / `q` | gradient / spray / shade |
| `n` / `C` | blur / contour |
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

`w`/`save` · `q`/`q!`/`wq` · `new [w h]` · `resize w h` · `crop [x y w h]` ·
`trim` · `newframe`/`dupframe`/
`delframe` · `newlayer`/`dellayer`/`layerup`/`layerdown`/`layername <name>` ·
`fliph`/`flipv`/`rot90` (selection-aware) · `sym <mode>` · `wand <0-255>` /
`wandall on|off` · `contig on|off` · `selop <op>` · `grad linear|radial` ·
`contour` ·
`pal load|save <file>` · `newgroup` · `indent`/`outdent` · `lock`/`unlock` ·
`background on|off` · `reference on|off` ·
`mask add|apply|del` · `tile [on|off]` · `tag <name> [from to]` · `tagdel` ·
`framemove <from> <to>` · `export <file> [scale]` · `import <png>` ·
`importsheet <sheet.png> [cols rows]` · `open <file>` · `recent [n]` ·
`filter <name> [amount]` · `rotate <deg>` · `scale <sx> [sy]` · `skew <deg>` ·
`sel expand|contract|border|feather <n>` / `sel invert|none|all` ·
`text <string>` · `grid <px>` · `snap [on|off]` · `brush <1-64>` ·
`perfect on|off` · `sym radial [axes]` ·
`symcenter cursor|reset|<x> <y>` · `fg|bg #rrggbb` · `pal move <from> <to>` ·
`mode auto|rgba|grayscale|indexed` · `dither none|bayer2|bayer4|bayer8|floyd` ·
`aspect <x> <y>` · `sgrid <w> <h>` ·
`slices <dir> [cols rows]` · `export <file> [scale]` · `profile srgb|linear` ·
`renderer glow|wgpu` ·
`ops <file.json>` · `recover [file]` · `help`.

## Settings

`$XDG_CONFIG_HOME/vixel/config.json` (or `~/.config/vixel`): window size, zoom,
pixel grid, design grid, snap, onion settings, loop/ping-pong, the theme and the
last 10 opened/saved files (`:recent`). Dropping a file on the window opens it,
with the same dirty guard as `:open`.
Written on quit and on window close; a corrupt or missing file falls back to
defaults. Keymap customization is not implemented (see ROADMAP).

A dirty sprite is protected: `:q`, `:open`, `:new`, `:import` and the window
close button refuse to drop it (`!` or `:q!` forces). While it is dirty, a
`recovery.vpx` snapshot is written beside the config every 30 s; `:recover
[file]` loads it as unsaved work, and a successful save or forced quit removes
it.

## Test status

`cargo test` → **97 tests green** (89 lib + 8 bin); `cargo clippy --all-targets`
→ clean; `cargo build --features wgpu` → clean. On NixOS the plain loader needs
the wayland/xkbcommon libs, so `steam-run` or the LD_LIBRARY_PATH recipe is
still the way in.

The GUI itself is now functionally tested headlessly — `docs/TESTING.md` has the
rig (Xvfb + twm, because without a window manager nothing gets keyboard focus)
and the ten checks of the 2026-10-26 pass: text stamp, radial symmetry, rotate,
outline, selection contract, slice export, layer drag, onion ghosts, playback,
and the renderer config round trip with both builds.
