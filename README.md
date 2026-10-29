# vixel

A keyboard-first, **vim-style pixel art editor** with Aseprite-grade rendering —
a native GUI window (not a terminal) in Rust.

Vim muscle memory: modal editing (normal / draw / command / help), `hjkl`
movement, `i` to paint, `x` to erase, `u`/`Ctrl-r` undo, `:` commands. Full
24-bit color, real alpha transparency, zoom, pixel grid, onion skinning,
layers, and animation frames.

## Run

```sh
cargo run --release
```

On NixOS, the winit/wayland client libraries aren't on the default linker path,
so run through the FHS wrapper (or provide them yourself):

```sh
steam-run ./target/release/vixel
```

## Keymap

Modal, like vim. Start in **normal** mode.

### Normal mode

| Key | Action |
|-----|--------|
| `h` `j` `k` `l` / arrows | move cursor |
| `H` `J` `K` `L` | move 8px |
| `0` `$` `gg` `G` | left / right / top / bottom edge |
| `i` | **draw** mode — paint as you move |
| `x` | erase pixel (vim's delete-char) |
| `r` / `space` | paint one pixel with fg |
| `b` / `e` | pencil / eraser tool |
| `f` | bucket fill |
| `c` | color picker (eyedropper) |
| `d` / `R` / `o` | line / rectangle / ellipse — move, then `Enter` commits, `Esc` cancels |
| `z` / `a` / `q` | gradient / spray (airbrush) / shade (darken; right-click lightens) |
| `n` / `C` | blur / contour (outline the selection or region under the cursor) |
| `v` | **visual select** (marquee) — `V` selects the whole canvas |
| `w` | **magic wand** — select the same-color region under the cursor |
| `L` / `P` | **lasso** (freehand) / **polygon** (click vertices) selection |
| `y` / `Y` / `x` / `m` / `p` | copy (layer) / copy merged / cut / move · `p` pastes the clipboard |
| `s` | cycle symmetry (none / horizontal / vertical / both / radial) |
| `t` | toggle filled shapes (rect / ellipse) |
| `,` / `.` | previous / next color |
| `<` / `>` | previous / next layer |
| `[` / `]` | previous / next frame |
| `X` | swap fg / bg |
| `u` / `Ctrl-r` | undo / redo |
| `+` / `-` | zoom in / out |
| `:` | command line |
| `?` | help |
| `Ctrl-c` / `Ctrl-x` / `Ctrl-v` | copy / cut / paste (OS clipboard; no selection copies the whole cel) |
| `Ctrl-z` | undo (alias of `u`) |
| `Ctrl-s` / `Ctrl-q` | save / quit |

### Draw mode (`i`)

`hjkl`/arrows move **and** paint with the active tool (pencil = fg, eraser =
transparent). `Esc` or `q` returns to normal. A whole drag is one undo step.

### Visual mode (`v`), magic wand (`w`) & paste

`v` starts a marquee selection; `hjkl` extends it. `w` selects the contiguous
same-color region around the cursor (active layer). Then `y` copies the active
layer, `Y` copies the flattened selection (all visible layers), `x` cuts and
`m` cuts-and-floats (move). `p` floats the clipboard for pasting — `hjkl`
repositions it, `Enter` commits, `Esc` cancels. Cut and move act on the active
layer's cel; `:copy merged` is the same as `Y`. Every paint tool is clipped to
the selection; paste replaces it, and contour draws its ring just outside it.
`Esc` in normal mode clears the selection.

### Mouse

Left-drag paint fg · right-drag paint bg · middle-click pick · scroll zoom ·
middle-drag pan · drop a file on the window to open it. (The editor is
keyboard-first, but the mouse is fully wired.)

## Commands (`:`)

| Command | Effect |
|---------|--------|
| `:w [file]` / `:save` | save project (JSON `.vpx`) |
| `:q` / `:q!` / `:wq` | quit / force quit / save + quit |
| `:new[!] [w h]` | new sprite (default 32×32); `!` drops unsaved work |
| `:resize w h` | resize canvas (keeps overlap) |
| `:crop [x y w h]` | crop to a rectangle (no args: the selection) |
| `:trim` | crop to the non-transparent content |
| `:newframe` / `:delframe` | add / remove animation frame |
| `:dupframe` | duplicate the current frame (cels + duration) |
| `:framemove <from> <to>` | move a frame (or drag a timeline thumbnail) |
| `:celz <n>` | active cel z-index (+ moves towards the front) |
| `:link <frame>` / `:unlink` / `:preferlink on\|off` | linked cels in the active layer |
| `:movelock on\|off` / `:collapse on\|off` / `:layeruuid [new\|clear]` | layer movement lock, group collapse, UUID |
| `:tag <name> [from to]` | create/update a named frame tag |
| `:tagdel [name]` | delete a frame tag (default: the active one) |
| `:tagdir forward\|reverse\|pingpong\|pingpong-reverse` | play direction of the active tag |
| `:tagrepeat <n>` | repeat count of the active tag (0 = forever) |
| `:newlayer` / `:dellayer` | add / remove layer |
| `:duplayer` | duplicate the active layer (with its cels) |
| `:merge` | merge the active layer into the one below |
| `:flatten` | flatten every visible layer into one |
| `:layerup` / `:layerdown` | move the active layer up / down |
| `:layername <name>` | rename the active layer |
| `:newgroup` | add a layer group |
| `:indent` / `:outdent` | nest the active layer into the group above / un-nest |
| `:lock` / `:unlock` | lock / unlock the active layer (blocks painting) |
| `:reference on\|off` | tracing layer: drawn, never painted or exported |
| `:background on\|off` | the opaque bottom layer |
| `:mask add\|apply\|del` | alpha mask from the selection (or full) / bake / remove |
| `:tile [on\|off]` | seamless 3x3 preview; cursor and shapes wrap across the seam |
| `:fliph` / `:flipv` / `:rot90` | mirror H / V / rotate 90° (selection if any, else whole sprite) |
| `:sym none\|h\|v\|both\|radial [axes]` | set symmetry (mirror painting) |
| `:symcenter cursor\|reset\|<x> <y>` | radial symmetry centre |
| `:wand <0-255>` / `:wandall on\|off` | wand + bucket tolerance / match all layers |
| `:contig on\|off` | wand and bucket: contiguous region or every matching pixel |
| `:selop replace\|add\|subtract\|intersect` | how new selections combine |
| `:sel expand\|contract\|border\|feather <n>` / `:sel invert\|none\|all` | selection shape ops |
| `:sel color [#hex] [tol]` / `:sel alpha [all]` | colour-range or opaque-pixel selection |
| `:sel rotate\|skew <deg>` / `:sel scale <sx> [sy]` / `:sel offset <dx> <dy>` | transform the selection mask only |
| `:fill [#hex]` / `:stroke [width] [inside\|outside] [#hex]` | fill the selection or paint its edge |
| `:grad linear\|radial` | gradient mode |
| `:filter <name> [args]` | invert, brightness, contrast, hue, saturation, desaturate, curves, levels, balance, posterize, outline, blur, sharpen, median, convolve, emboss |
| `:rotate <deg>` / `:scale <sx> [sy]` / `:skew <deg>` | free-transform the selection (or the cel) |
| `:rotsprite on\|off` / `:pivot <x> <y>\|clear` | supersampled rotation and a custom transform pivot |
| `:text <string>` | stamp 8x8 bitmap text at the cursor |
| `:textobj <string>` | create/edit an editable text object (stored in cel user data) |
| `:grid <px>` / `:snap [on\|off]` | design grid and snapping |
| `:brush <1-64>` / `:brush square\|circle <n>` / `:brush spacing <pct>` | brush size, built-in shape, stamp spacing |
| `:brush load\|save\|use\|list [name]` | custom PNG brush and the `vixel/brushes` library |
| `:pressure <0-100>` / `:pressure auto` | manual pressure override; `auto` follows pen force |
| `:spray <dots> [radius]` / `:shade <pct>` / `:blur <radius>` | airbrush, shade, and blur-tool depth |
| `:contour [width] [inside\|outside]` | contour the cursor region with the chosen ring |
| `:grad preset <name>` / `:grad stop <pct> [#hex]` / `:grad stops` / `:grad clear` | multi-stop gradient presets and stops |
| `:grad dither none\|bayer2\|bayer4\|bayer8` | ordered dithering while painting a gradient |
| `:ink simple\|lockalpha\|replace\|shading\|erase` / `:alphalock on\|off` | per-tool ink and alpha-locked painting |
| `:perfect on\|off` | pixel-perfect freehand: drop L-shaped corner pixels |
| `:mode auto\|rgba\|grayscale\|indexed` / `:aspect <x> <y>` / `:sgrid [x y] <w> <h>` | sprite colour mode, pixel ratio, grid size + origin |
| `:dither none\|bayer2\|bayer4\|bayer8\|floyd` | dithering for indexed conversion |
| `:profile srgb\|linear` / `:profile icc <file>` / `:profile gamma <g>` / `:profile clear` | working space / embedded `.ase` colour profile |
| `:renderer glow\|wgpu` | renderer (applied on next launch) |
| `:fg #rrggbb` / `:bg #rrggbb` | set fg / bg exactly |
| `:pal load\|save <file>` / `:pal move <from> <to>` / `:pal from-sprite` / `:pal lock on\|off` | palette I/O, reorder, derive from the frame, lock |
| `:pal preset <name>` / `:pal insert <i> [#hex]` / `:pal delete <i>` / `:pal sort [rgb\|luma\|hue]` / `:pal name <i> <name>` | presets, index edits, sort, names |
| `:slices [<dir> [cols rows\|named]]` | list slices, export a grid, or one PNG per named slice + `.slices.json` |
| `:ops <file.json>` | replay a JSON op list as one undo step |
| `:export out.png\|out.ase\|<stem> [scale]` | PNG frame · `.ase` sprite · bare stem = sheet + GIF + meta JSON; `[scale 1-16]` upscales rasters |
| `:import[!] in.png` | import a PNG as a new sprite; `!` drops unsaved work |
| `:importsheet[!] sheet.png [cols rows]` | rebuild frames from a sheet + its `.json` sidecar |
| `:open[!] proj.vpx\|proj.ase\|in.png` | load a project / `.ase` / PNG (by extension); `!` drops unsaved work |
| `:recover [file]` | load the autosave snapshot (`recovery.vpx`) as unsaved work |
| `:recent [n]` | list the last 10 opened/saved files, or open the nth |

## UI

- **Center** — the canvas: checkerboard transparency, real alpha, onion skin
  (prev frame tinted red, next green), optional pixel grid, and a 3x3
  seamless-tile preview with wrapping cursor.
- **Right** — layer list (visibility, opacity, blend mode, reorder, rename,
  add/remove, groups + indent/outdent, lock flag, alpha mask) + palette swatches
  + full RGB picker for fg/bg.
- **Bottom** — frame timeline (numbered cells, add/duplicate/remove),
  play/pause, per-frame duration, drag-to-reorder, and tag chips that restrict
  playback to a named range.
- **Top** — mode, tool strip, fg/bg swatches, zoom, cursor/status, and the `:`
  command line.

## Rendering & the glow → wgpu plan

The canvas is a single RGBA8 texture (`egui::TextureHandle`, nearest-neighbor at
integer zoom). Compositing, blend modes, and onion skin all happen on the CPU,
producing plain pixels. `egui`'s texture API is identical on the `glow` (OpenGL)
and `wgpu` backends, so switching renderers later is:

1. flip `eframe` features from `glow` → `wgpu` in `Cargo.toml`, and
2. set `renderer: eframe::Renderer::Wgpu` in `main.rs`.

No render-code changes. (Custom GPU shaders would be the one thing needing
per-backend code — deliberately avoided for now.)

## Data model

A sprite is a grid of **cels** indexed `[layer][frame]`, the same shape Aseprite
uses: `layers[0]` is the topmost layer, each cel is a row-major `Vec<Rgba>`
with its own `x/y/w/h` bounds, z-index and optional link to another frame.
Editing writes to the active layer's cel on the active frame; the canvas shows
the flattened composite of all visible layers, with cel z-index reordering them.
Linked cels share pixels and painting one copies them first.
Offset cels from `.ase` keep their bounds; painting outside grows a cel, while
crop/resize canonicalize to the canvas.

## File formats

- **`.ase` / `.aseprite`** — the native Aseprite format (read + write): raw,
  zlib, linked (read + write) and compressed-tilemap cels; RGBA, grayscale and indexed pixels;
  layer groups, cel opacity and frame durations. Export picks the smallest
  lossless depth (gray, indexed, or 32-bit RGBA). Indexed sprites carry a
  document palette and transparent index (header byte 28), and paint snaps to
  palette entries. Embedded sRGB/ICC/gamma profiles are read and written;
  user data (text/colour/properties) round-trips on layers, cels, tags, slices
  and the sprite; slices (name, bounds, 9-patch, pivot) round-trip and export
  by name; external-file entries round-trip and deprecated chunks are preserved
  raw; unknown chunks are preserved on save.
- **`.vpx`** — project file: JSON with size, layers, frames, and every cel.
- **PNG** — single-frame import/export with full alpha.

`:` commands pick the format by file extension (`.ase` → Aseprite, `.png` →
PNG, otherwise `.vpx`).

## Layout

```
src/
  color.rs    Rgba color + default palette
  sprite.rs   Sprite/Layer/Frame/Cel, composite, fill/line/rect, resize
  editor.rs   Editor state: modes, tools, undo, navigation (GUI-agnostic)
  input.rs    vim keymap + command line (egui events)
  gui.rs      View (zoom/pan), canvas, panels, help overlay
  app.rs      eframe App: event loop, playback, texture management
  io.rs       PNG + `.vpx` + sheet/GIF/meta save/load
  ase.rs      `.ase` / `.aseprite` import + export
  palette.rs  `.gpl` / `.hex` / `.pal` palette I/O
  ops.rs      an `Op` list replayed through `Editor::apply`
  config.rs   `$XDG_CONFIG_HOME/vixel/config.json`
  lib.rs      the model half as a library (no GUI dependency)
  main.rs     window + run_native
```

The model (`sprite.rs`, `editor.rs`, `io.rs`, `ase.rs`, `color.rs`,
`palette.rs`, `ops.rs`) is independent of any GUI library — the view and input
layers are thin shells over it.

## Implemented vs. full Aseprite

Implemented: truecolor canvas with real alpha + checkerboard, zoom + pixel grid,
onion skinning, animation playback with timeline thumbnails, layers
(visibility/opacity/**blend modes**/reorder/rename/add/remove, **nested
groups**, **lock flag**, **alpha masks**, **reference/background layers**),
palette + RGB
picker, pencil/eraser/bucket/picker/line/rect/ellipse with **filled shapes**,
**gradient (linear/radial), spray, shade, blur, contour**,
**marquee + magic-wand (tolerance, contiguous/global, all-layers) + lasso +
polygon selection**
with **add/subtract/intersect**, **expand/contract/border/feather/invert** and
copy/cut/paste/move, **sixteen filters** (curves, levels, balance, median, convolve, emboss…), **free rotate/scale/skew + RotSprite + interactive handles/pivot**, **8x8 bitmap
text**, **brush size 1-64 + custom masks/spacing + pressure**, **pixel-perfect freehand**, **crop/trim**,
**per-tool inks + alpha lock**, **design grid + snap**,
**multi-stop gradients** (presets, alpha, Bayer dither), **wide-pixel preview**, **symmetry painting**
(mirror or radial,
including bucket fill and paste), flip H/V + rotate 90° (**selection-aware**),
stroke-grouped undo/redo (**structural layer/frame/tag edits and panel
properties undo too**),
**`.ase`/`.aseprite` + PNG + `.vpx` import/export**, **sheet + GIF + meta JSON
export**, **sprite properties** (colour mode, pixel ratio, grid size + origin) and
**layer flags** (movement lock, collapsed groups, UUIDs),
**sRGB/linear colour profiles**, **custom palettes** (`.gpl`/`.hex`/`.pal`),
**OS clipboard**, **tile mode** (3x3 seamless preview + wrapping cursor),
**frame tags + drag reorder**, full vim keymap (normal/draw/visual/paste/command
modes).

Aseprite-parity programme in progress: `docs/ASEPRITE_PARITY.md` tracks every
Aseprite capability against the task that closes it, and `docs/ROADMAP.md` P5
is the ordered backlog. Out-of-scope by product choice: Web/Wasm and custom GPU
shaders.
