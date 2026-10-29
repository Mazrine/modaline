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
and gradients support multiple stops, seven presets, interpolated alpha and
Bayer 2/4/8 ordered dithering, clipped by the selection —
all with live preview where applicable — plus **symmetry painting** (mirror or
radial, including bucket fill and paste). Pencil and eraser share a square
brush, 1-64 px (`:brush` or the panel slider), clipped at the canvas edges.
Brushes come as **square**, **circle** or a **custom PNG mask** (`:brush
load|save|use|list`), with percentage **spacing**; the mask previews under the
cursor and is stamped by freehand, shapes and symmetry. **Pressure** comes from
touch/pen `force` when the backend reports it, otherwise from the manual slider
(`:pressure <0-100>|auto`); it scales mask coverage. **Shade**, **blur** and
**spray** stamp the same brush mask; their depth is configurable — shade
strength, blur radius (shared with `box_blur`), spray rate/radius, and contour
width with inside/outside placement. Each tool remembers its own **ink** (`simple`, `lock-alpha`, `replace`,
`shading`, `erase`; `:ink` or the panel selector), and **alpha lock**
(`:alphalock`) restricts painting to pixels that are already opaque.
**Pixel-perfect** freehand (`:perfect on|off`) joins pointer steps and drops
L-shaped corners for the 1 px pencil/eraser.

## Selection & clipboard

- **Marquee** (`v`, `V` = whole canvas), **magic wand** (`w`, contiguous or
  global via `:contig`, with **tolerance** and optional **all-layers**
  matching), **lasso** (`L`, freehand mouse drag), and **polygon** (`P`, click
  vertices, `Enter` closes).
- **Cel clipboard**: `:celcopy` / `:celcut` / `:celpaste` move whole cels
  across layers and frames (links resolved; one undo step), `:celdel` empties a
  cel while keeping the frame, and `:celinfo` reports emptiness. Timeline cel
  row exposes opacity, z-index, link/unlink and copy/cut/paste/empty.
- **Multi-cel selection**: Ctrl/Cmd-click toggles a timeline frame or layer,
  Shift-click selects a range (`:celselect`, `:celclear`). Paint and exact
  writes fan out to every selected cel; transforms run per cel as one undo;
  `:exportcels <dir>` writes each selected cel as PNG.
- **Selection ops** (`:selop` or the toolbar): replace / add / subtract / intersect
  with the existing selection.
- **Shape ops** (`:sel`, or the toolbar chips): `expand` / `contract` /
  `border` / `feather <n>`, `invert`, `none`, `all`, plus `color [#hex] [tol]`,
  `alpha [all]`, and mask-only `rotate`/`scale`/`skew`/`offset`. They work on a
  sprite-sized grid, so they can grow past the selection's own bbox; feather is
  a deterministic dither ramp (a mask is one bit per pixel).
- **Fill / stroke** (`:fill [#hex]`, `:stroke [width] [inside|outside] [#hex]`)
  paint the selection interior or its edge as one undo step.
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

- visibility toggle, **lock movement** (`:movelock`, `mv`), **collapsed groups**
  (`:collapse`, `col`), **layer UUIDs** (`:layeruuid [new|clear]`), opacity
  (0–255), 19 **blend modes** (normal, multiply,
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
  cel's **opacity** and **z-index** (per layer + frame, as in Aseprite; `:celz`).
  Cels can also be **linked** to another frame (`:link`, `:unlink`, timeline
  toggle); linked cels share pixels and painting one copies them first. Layers
  with `:preferlink on` link new frames to the previous one. Cels keep their own
  **bounds/offset** (`x/y/w/h`), so partial and off-canvas cels survive;
  painting outside a cel grows it, while crop/resize canonicalize to the canvas.
- **frame tags** (named inclusive ranges): `:tag <name> [from to]`,
  `:tagdel [name]`; tags are listed above the timeline, clicking one restricts
  playback to its range (click again to clear). Tags carry a play direction
  (forward/reverse/ping-pong/ping-pong-reverse, `:tagdir`) and a repeat count (`:tagrepeat`,
  0 = forever); both round-trip in `.vpx` and `.ase` (name, range, color,
  direction, repeat) and follow frame insert/remove.
- **frame reorder**: drag a timeline thumbnail onto another slot, or
  `:framemove <from> <to>`.

## Canvas

- integer zoom (fit + `+`/`-`/scroll), pan (middle-drag), pixel grid overlay,
  transparency checkerboard, cursor + selection + floating-paste previews.
  **Wide pixels** render at the sprite's `pixel_aspect` (checkbox "wide px");
  grids, hit-testing, fit, zoom anchoring and handles all follow the cell.
- **file browser**: in-app `files` window (Ctrl/Cmd+O, Ctrl/Cmd+Shift+S) with
  filtering, parent/home navigation, recents and persistent favourites; save
  dispatches `.ase`/image/`.vpx` by extension.
- **undo history**: a history window lists labelled steps and redo entries;
  clicking any row jumps to that state within the linear stack (branching is
  deliberately not implemented).
- **timeline**: switchable layer×frame grid with per-cel thumbnails and drag
  rectangle selection, plus the compact frame strip. Tag chips show playback
  ranges and the tag editor edits name/range/colour or creates/deletes tags;
  onion count/opacity are in the header.
- **preview**: a floating window (or separate OS viewport) with independent
  play/pause, loop, zoom 1-16 and frame scrubber; fullscreen is available on the
  OS viewport.
- **design grid** (1-64 px, its own tone) with **snap**: the cursor steps one
  cell and every canvas pick rounds down to the cell.
- **export config**: frame range/tag/layer filters, ignore-empty, merge
  duplicates, scale, global trim, padding and sheet/GIF/meta switches, shared by
  `:exportcfg` and the export dialog; padded sheets record `padding` in metadata.
  PNG sequences export per frame or per layer with `{frame}`, `{layer}`, `{tag}`,
  `{title}` and `{duration}` filename templates. Sheet layouts: horizontal,
  vertical, rows, columns, packed; border/shape/inner padding, extrusion and
  trim, recorded in metadata. Atlas metadata can be vixel, JSON-hash or
  JSON-array (tags/layers/slices); GIF supports repeat count, palette size and
  ordered dithering. Static PNG/JPEG/BMP/TGA/WebP import and export dispatch by
  extension; JPEG flattens alpha. PCX/FLC/FLI remain unsupported. Atlas import
  understands JSON hash/array and XML `SubTexture` metadata (frames, durations,
  tags, slices); image folders import as natural-order sequences
  (`:importatlas`, `:importseq`).
- **filters** (panel + `:filter`): invert, brightness, contrast, hue,
  saturation, desaturate, curves, levels, colour balance, posterize, outline,
  blur, sharpen, median, 3×3 convolution (custom kernel/divisor/bias) and
  emboss — selection-aware, one undo step.
- **free transform** (panel + `:rotate`/`:scale`/`:skew`): rotate, scale and
  skew the selection (or the cel) about any pivot (`:pivot`), nearest-neighbour
  or **RotSprite-style supersampling** (`:rotsprite`). With "handles" enabled,
  corner/edge/rotate handles and inside-drag move run on the canvas.
- **crop / trim**: `:crop [x y w h]` (no args = the selection) and `:trim`
  (non-transparent content) resize every cel and layer mask, one undo step.
- **text**: an 8x8 bitmap font at scale 1-8, stamped at the cursor as one undo
  step (`:text <string>`). The **Text tool** (`T`) places text objects on click;
  the panel edits multi-line text with live preview and commits one undo step
  (`commit`), with font (bitmap "vixel"), size 8-64 and left/centre/right
  alignment. Editable **text objects** store text, font, size,
  alignment, antialias flag and position in the cel's user-data properties and
  re-rasterise on edit (`:textobj <string>`); `.ase` text properties round-trip.
- **tilesets**: embedded/external tilesets are retained on the sprite
  (`id`, name, tile size, base index, flags, atlas) with layer tilemap IDs;
  `.vpx` preserves them and `.ase` import honours base-index and
  diagonal→X→Y flip order. Tilemap layers keep their per-cel tile refs/flips
  (`TilemapCel`) beside the rendered pixel cache; `Sprite::rasterize_tilemap`
  rebuilds pixels after edits. The **tile tool** (`K`) places/picks tiles with
  flips, places the empty tile, and has a pixel-edit mode; each edit is one undo
  step (`:tileid`, `:tileflip`, `:tilemode`, `:tilepick`, `:tileerase`).
  `.ase` saving writes `0x2023` tilesets and type-3 tilemap cels (ID + flips);
  external references are written without pixels and resolved on load from
  sibling PNG/`.aseprite` files. Pixel-edit mode edits the atlas through the
  tile's flips; `--export-tileset`/`:exporttileset` write it out. Manual, auto
  (fills empty neighbours) and stack (variant advance) tile modes.
- **slices**: named 9-patch/pivot slice objects with per-frame keys, read and
  written from `.ase` 0x2022 (colour from user data) and listed in the side
  panel. `:slices` lists them; `:slices <dir> named` exports one PNG per slice
  plus `<stem>.slices.json`; `:slices <dir> [cols rows]` still exports a grid.
- **tile mode** (`:tile on|off`, side-panel checkbox): the sprite is previewed
  tiled with configurable X/Y repeats, neighbour opacity and a preview toggle
  (default 3x3), and the cursor wraps across the edges
  (moving right from the last column lands on column 0), so strokes stay
  seamless across the tile boundary. Line, rect, ellipse and gradient take the
  short way round too: geometry past the seam lands on the opposite edge.

## Color

- default 16-color palette, fg/bg, swap, full RGB picker, a **hue wheel**, the
  last 12 **recent colours**, eyedropper, plus a **hex field** and **HSV** drag
  values for both.
- palette management: add the current fg, remove/insert at an index, sort by
  RGB/luma/hue, per-swatch **names** (written to `.ase`), presets (default,
  gray, pico8, dawnbringer16), drag a swatch to reorder (`:pal move`),
  palette-from-sprite, lock, and import/export **`.gpl` (GIMP), `.hex`, and
  `.pal` (JASC)** via `:pal`.
- **sprite properties**: colour mode (auto / rgba / grayscale / indexed — the
  declared mode drives the .ase depth; grayscale also converts the pixels),
  pixel ratio and the sprite grid size + origin (all written to and read from
  the .ase header).
- **indexed mode**: `:mode indexed` builds a document palette from the pixels
  (exact up to 255 opaque colours, median cut above that), keeps a transparent
  slot, and snaps painting/fills to palette entries. The palette and
  transparent index survive `.vpx` and `.ase`; entries may carry alpha.
- **dithering**: `:dither none|bayer2|bayer4|bayer8|floyd` converts to the
  document palette with an ordered Bayer matrix or Floyd–Steinberg error
  diffusion; `:mode indexed` uses the last choice. Gradients reuse the engine.
- **colour profile**: sRGB or linear light. Converting rewrites every pixel and
  the palette together with the real transfer function (`:profile srgb|linear`).
  The `.ase` 0x2007 chunk is read and written: `:profile` shows it, `:profile
  icc <file>` embeds ICC bytes, `gamma <g>` sets fixed gamma and `clear` drops
  it. Saving without an imported profile writes sRGB (linear writes fixed gamma
  1.0); ICC pixels load as-is in sRGB.

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
    Cel offsets, dimensions, z-index and linked-cel references are preserved
    on import and written back (type 1), so partial/off-canvas cels, stacking
    order and links round-trip instead of being rasterized into the canvas.
    Layer flag bits (editable, background, reference, prefer-linked, lock
    movement, collapsed) and layer UUIDs (header bit 4) round-trip.
  - Reads and writes **user data** (0x2020) on layers, cels, tags, slices and
    the sprite: text, colour and the property payload (kept verbatim). Tileset
    user data stays in the raw-chunk pass-through until those models land.
  - Reads and writes **slices** (0x2022): names, frame keys, bounds, 9-patch
    centre and pivot.
  - Reads/writes the **external files** chunk (0x2008) as `ExternalFile`
    entries and records external tileset references; external pixels are not
    resolved yet. Deprecated 0x2006/0x2016/0x2017 chunks are preserved raw.
  - Reads RGBA (32), grayscale (16) and indexed (8) pixels, new and legacy
    palette chunks, layer names/visibility/blend/opacity, layer groups (type +
    child level), the editable flag, **per-cel opacity** and frame durations.
  - Writes the declared colour mode when one is set (`:mode`), else the
    automatically chosen **lossless** depth: grayscale when every pixel is
    gray, indexed when the sprite uses <= 255 colors, else 32-bit RGBA. The
    pixel ratio is written to and read from the header. Unknown chunks
    (slices, user data, external files, ...) are retained verbatim with
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
`text <string>` · `textobj <string>` · `grid <px>` · `snap [on|off]` · `brush <1-64>` ·
`ink simple|lockalpha|replace|shading|erase` · `alphalock on|off` ·
`perfect on|off` · `sym radial [axes]` ·
`symcenter cursor|reset|<x> <y>` · `fg|bg #rrggbb` · `pal move <from> <to>` ·
`pal preset <name>` · `pal insert <i> [#hex]` · `pal delete <i>` ·
`pal sort [rgb|luma|hue]` · `pal name <i> <name>` ·
`mode auto|rgba|grayscale|indexed` · `dither none|bayer2|bayer4|bayer8|floyd` ·
`aspect <x> <y>` · `sgrid [x y] <w> <h>` · `celz <n>` ·
`link <frame>` · `unlink` · `preferlink on|off` · `movelock on|off` ·
`collapse on|off` · `layeruuid [new|clear]` ·
`slices [<dir> [cols rows|named]]` · `export <file> [scale]` ·
`profile [srgb|linear|icc <file>|gamma <g>|clear]` ·
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
