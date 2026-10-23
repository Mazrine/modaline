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
| `y` / `x` / `m` / `p` | copy / cut / move the selection · `p` pastes the clipboard |
| `s` | cycle symmetry (none / horizontal / vertical / both) |
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
same-color region around the cursor (active layer). Then `y` copies, `x` cuts,
`m` cuts-and-floats (move). `p` floats the clipboard for pasting — `hjkl`
repositions it, `Enter` commits, `Esc` cancels. Copy/cut/move operate on the
active layer's cel. `Esc` in normal mode clears the selection.

### Mouse

Left-drag paint fg · right-drag paint bg · middle-click pick · scroll zoom ·
middle-drag pan. (The editor is keyboard-first, but the mouse is fully wired.)

## Commands (`:`)

| Command | Effect |
|---------|--------|
| `:w [file]` / `:save` | save project (JSON `.vpx`) |
| `:q` / `:q!` / `:wq` | quit / force quit / save + quit |
| `:new [w h]` | new sprite (default 32×32) |
| `:resize w h` | resize canvas (keeps overlap) |
| `:newframe` / `:delframe` | add / remove animation frame |
| `:dupframe` | duplicate the current frame (cels + duration) |
| `:newlayer` / `:dellayer` | add / remove layer |
| `:layerup` / `:layerdown` | move the active layer up / down |
| `:layername <name>` | rename the active layer |
| `:newgroup` | add a layer group |
| `:indent` / `:outdent` | nest the active layer into the group above / un-nest |
| `:lock` / `:unlock` | lock / unlock the active layer (blocks painting) |
| `:mask add\|apply\|del` | alpha mask from the selection (or full) / bake / remove |
| `:tile [on\|off]` | seamless 3x3 preview; cursor wraps at the edges |
| `:fliph` / `:flipv` / `:rot90` | mirror H / V / rotate 90° (selection if any, else whole sprite) |
| `:sym none\|h\|v\|both` | set symmetry (mirror painting) |
| `:wand <0-255>` / `:wandall on\|off` | magic-wand tolerance / match all layers |
| `:selop replace\|add\|subtract\|intersect` | how new selections combine |
| `:grad linear\|radial` | gradient mode |
| `:pal load\|save <file>` | load/save palette (`.gpl`, `.hex`, `.pal`) |
| `:export out.png\|out.ase` | export current frame as PNG, or the sprite as `.ase` |
| `:import in.png` | import a PNG as a new sprite |
| `:open proj.vpx\|proj.ase\|in.png` | load a project / `.ase` / PNG (by extension) |

## UI

- **Center** — the canvas: checkerboard transparency, real alpha, onion skin
  (prev frame tinted red, next green), optional pixel grid, and a 3x3
  seamless-tile preview with wrapping cursor.
- **Right** — layer list (visibility, opacity, blend mode, reorder, rename,
  add/remove, groups + indent/outdent, lock flag, alpha mask) + palette swatches
  + full RGB picker for fg/bg.
- **Bottom** — frame timeline (numbered cells, add/duplicate/remove),
  play/pause, per-frame duration.
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
uses: `layers[0]` is the topmost layer, each cel is a row-major `Vec<Rgba>`.
Editing writes to the active layer's cel on the active frame; the canvas shows
the flattened composite of all visible layers.

## File formats

- **`.ase` / `.aseprite`** — the native Aseprite format (read + write): raw,
  zlib, linked and compressed-tilemap cels; RGBA, grayscale and indexed pixels;
  layer groups, cel opacity and frame durations. Export picks the smallest
  lossless depth (gray, indexed, or 32-bit RGBA).
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
  io.rs       PNG + JSON (.vpx) save/load
  main.rs     window + run_native
```

The model (`sprite.rs`, `editor.rs`, `io.rs`, `color.rs`) is independent of any
GUI library — the view and input layers are thin shells over it.

## Implemented vs. full Aseprite

Implemented: truecolor canvas with real alpha + checkerboard, zoom + pixel grid,
onion skinning, animation playback with timeline thumbnails, layers
(visibility/opacity/**blend modes**/reorder/rename/add/remove, **nested
groups**, **lock flag**, **alpha masks**), palette + RGB
picker, pencil/eraser/bucket/picker/line/rect/ellipse with **filled shapes**,
**gradient (linear/radial), spray, shade, blur, contour**,
**marquee + magic-wand (tolerance, all-layers) + lasso + polygon selection**
with **add/subtract/intersect** and copy/cut/paste/move, **symmetry painting**,
flip H/V + rotate 90° (**selection-aware**), stroke-grouped undo/redo,
**`.ase`/`.aseprite` + PNG + `.vpx` import/export**, **custom palettes**
(`.gpl`/`.hex`/`.pal`), **OS clipboard**, **tile mode** (3x3 seamless preview +
wrapping cursor), full vim keymap (normal/draw/visual/paste/command modes).

Not yet (Aseprite parity gaps): external tileset files, color profiles,
shaders.
