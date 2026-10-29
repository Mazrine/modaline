# Modaline

**A keyboard-first pixel art editor.** Modal like vim, native in Rust, with
layers, animation and Aseprite-compatible files. The mouse and a drawing tablet
work everywhere, but the keyboard is the fast path.

[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Rust: edition 2024](https://img.shields.io/badge/rust-edition%202024-orange.svg)](Cargo.toml)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-lightgrey.svg)](#requirements)

![The editor with the demo project](assets/screenshots/editor.png)

*The demo project (`examples/demo.vpx`): timeline grid, layers, animation tag,
pixel-perfect zoom and a contextual hint on the status line.*

## Why modal?

Pixel editors usually make you travel to a toolbar for every action. Modaline
treats the canvas like text: a small set of modes, one key per operation, and
the same keys everywhere.

| Mode | What it is for | Enter |
|---|---|---|
| **Normal** | navigate, pick tools, run commands | start here |
| **Draw** | paint while moving the cursor | `i` |
| **Visual** | grow a selection before acting on it | `v` |
| **Paste** | position a floating clipboard | `p` |
| **Command** | everything not worth a key | `:` |
| **Help** | the full key list | `?` |

Select first, then act: `v` + `hjkl`, then `y` copy, `x` cut, `p` paste,
`m` cut-and-float. The status line always tells you what the current state
accepts, so you do not have to memorise the whole map on day one.

![Selection with contextual hints](assets/screenshots/selection.png)

*Selection-first editing: the marquee, the mode, and the next actions
(`y` copy · `x` cut · `p` paste · `Esc` clear) in the status line.*

**Ten keys to start:** `hjkl` move · `i` draw · `b` pencil · `f` bucket ·
`v` select · `y`/`x`/`p` copy/cut/paste · `u` undo · `:` command · `?` help.

The complete map is in [`docs/COMMANDS.md`](docs/COMMANDS.md), and `:tips`
opens a short cheat sheet with the keyboard, mouse and pen workflows.

![Tips window](assets/screenshots/tips.png)

*`:tips` — the built-in cheat sheet. `Esc` closes windows; nothing needs the
mouse.*

## Features

**Canvas & rendering**
- True 24-bit colour, real alpha, checkerboard transparency, nearest-neighbour
  zoom, pixel grid and a design grid with snapping.
- Onion skinning (previous/next frame), tiled 3×3 seamless preview with edge
  wrapping, wide-pixel aspect preview, independent preview window/OS viewport.
- CPU compositing (blend modes, groups, masks, cel z-order) drawn through egui;
  a `wgpu` build exists behind a feature flag with no render-code changes.

**Tools**
- Pencil, eraser, bucket, eyedropper, line, rectangle, ellipse, gradient
  (linear/radial, multi-stop, dithered), spray, shade, blur, contour and an
  8×8 bitmap text tool.
- Brush sizes 1–64, square/circle/custom masks, spacing, pressure, pixel-perfect
  freehand, per-tool inks and alpha lock.
- Horizontal, vertical and N-fold radial symmetry (including bucket and paste).
- Selection masks with marquee, magic wand (tolerance, contiguous/global,
  all layers), lasso and polygon; expand/contract/border/feather/invert and
  add/subtract/intersect.
- Free rotate/scale/skew with interactive handles, custom pivot and a RotSprite
  mode; flip/rotate 90° respects the selection.
- Sixteen filters: invert, brightness, contrast, hue, saturation, desaturate,
  curves, levels, balance, posterize, outline, blur, sharpen, median, convolve
  and emboss — all clipped to the selection and one undo step.

**Layers & animation**
- Layer groups, blend modes, opacity, visibility, movement lock, reference and
  background layers, alpha masks and linked cels.
- Frame timeline with a layer×frame grid, thumbnails, drag reorder, per-frame
  duration, tags with direction and repeat, onion skin and playback.
- Multi-cel edits, whole-cel clipboard and per-cel export.

**Colour & files**
- Sweetie 16 default palette, presets, `.gpl`/`.hex`/`.pal` import/export,
  palette names, sorting and palette lock; indexed mode with dithering.
- sRGB/linear working spaces and embedded `.ase` profiles.
- Read/write `.ase`/`.aseprite` (RGBA, grayscale, indexed, linked and tilemap
  cels, user data, slices, groups, profiles), `.vpx` JSON projects, PNG, JPEG,
  BMP, TGA, WebP; import PCX and FLC/FLI animations.
- Sheet, GIF, PNG sequence, atlas metadata and slices; atlas/sheet/sequence
  import.

**Made for the keyboard**
- Panel controls have `:` equivalents where a keyboard path matters, so the
  mouse is never required.
- `Ctrl-hjkl` pans, `{`/`}` change brush size, `[`/`]` change frames,
  `<`/`>` change layers; all pan and action keys are rebindable.
- Command-specific messages and contextual hints in English **and** Portuguese,
  switchable at runtime with `:lang`.
- `:bind` for custom chords, four workspace layouts, JSON themes and fonts.
- Sandboxed Lua 5.4 scripting with an Aseprite-style `app` API, plus
  `.aseprite-extension` packages.
- Headless `-b` CLI for batch export.

## Pen and tablet

Wacom Intuos Pro and **OpenTabletDriver** tablets are supported on Linux: tip
pressure is read from evdev while position and clicks continue through the OS
pointer, so every action works with the pen. `:pen` shows the device and
pressure; `:pen off` disables it; the setting persists.

```sh
:pen          # device + live pressure
:pen off      # ignore the tablet
```

Setup (udev/input group, NixOS, OTD) and troubleshooting are in
[`docs/PEN.md`](docs/PEN.md).

## Animation

![Three-frame demo animation](assets/demo.gif)

*A three-frame demo exported by Modaline itself. The project is
`examples/demo.vpx`; `examples/build_demo.lua` generates it with the scripting
API, and the GIF comes from the headless CLI.*

## Quick start

**NixOS (flake devShell):**

```sh
nix develop
cargo run --release
```

**Plain Rust** (needs the usual winit/X11/Wayland build libraries):

```sh
cargo build --release
./target/release/modaline
```

On NixOS without the devShell, `steam-run ./target/release/modaline` is the
fallback. Open a file directly with `modaline art.ase`, `modaline art.vpx` or
`modaline art.png`.

**First five minutes:** `?` (help) · `:tips` (workflow cheat sheet) ·
`:prefs` (settings) · `:layout animation` (hide the side panel) · `:lang pt-BR`
(Portuguese UI).

## Requirements

- Linux with X11 or Wayland (developed and tested here).
- Rust 1.85+ (edition 2024), or the Nix flake.
- A pen needs permission to read `/dev/input/event*` (see `docs/PEN.md`).

## Keyboard quick reference

| Keys | Action |
|---|---|
| `hjkl` / arrows | move the cursor |
| `H` `J` `K` `L` | move 8 px |
| `0` `$` `gg` `G` | left / right / top / bottom |
| `i` | draw mode: paint while moving |
| `b` / `e` / `f` / `c` | pencil / eraser / bucket / picker |
| `d` / `R` / `o` | line / rect / ellipse (`Enter` commits, `Esc` cancels) |
| `v` / `V` | marquee selection / select all |
| `w` / `L` / `P` | magic wand / lasso / polygon |
| `y` / `Y` / `x` / `m` / `p` | copy / copy merged / cut / move / paste |
| `u` / `Ctrl-r` | undo / redo |
| `Ctrl-hjkl` | pan the canvas |
| `{` / `}` | smaller / bigger brush |
| `[` / `]` | previous / next frame |
| `<` / `>` | previous / next layer |
| `+` / `-` | zoom in / out |
| `:` / `?` | command line / help |
| `Ctrl-s` / `Ctrl-q` | save / quit |

Full keymap, modes and every `:` command: [`docs/COMMANDS.md`](docs/COMMANDS.md).

## Examples

**Batch export** — no window:

```sh
modaline -b demo.vpx --sheet out.png --data out.json --gif out.gif
modaline -b demo.vpx --split-layers layers/ --split-tags tags/
modaline -b demo.vpx --color-mode indexed --dither bayer4 --save-as out.ase
modaline -b demo.vpx --list-tags --list-layers
```

**Scripting** — draw into a cel and add a frame:

```lua
local s = app.activeSprite
s:resize(32, 32)
local img = Image(32, 32)
for x = 0, 31 do
  img:setPixel(x, x, { r = 255, g = 205, b = 117, a = 255 })
end
s:setCelImage(1, 1, img)
s:newFrame()
s:setFrameDuration(2, 120)
```

The full API and sandbox rules are in [`docs/SCRIPTING.md`](docs/SCRIPTING.md);
`examples/build_demo.lua` builds the scene above.

## Project

The model is a GUI-free Rust library: sprites, cels, layers, frames,
compositing, undo, selections, `.ase`/`.vpx` I/O. The binary is a thin shell
over it (`input`, `gui`, `app`). Another tool can read or write a project
without opening a window.

```sh
cargo test                    # 292 tests
cargo clippy --all-targets    # clean
nix develop -c cargo run --release
```

- [`docs/COMMANDS.md`](docs/COMMANDS.md) — full keymap and commands
- [`docs/SCRIPTING.md`](docs/SCRIPTING.md) — Lua API and sandbox
- [`docs/PEN.md`](docs/PEN.md) — Wacom / OpenTabletDriver
- [`docs/KEYMAPS.md`](docs/KEYMAPS.md) — vim now, Helix preset plan
- [`docs/UX_AUDIT.md`](docs/UX_AUDIT.md) — what works and what is next
- [`docs/ASEPRITE_PARITY.md`](docs/ASEPRITE_PARITY.md) — capability matrix
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — P7 input/UX backlog

Status: pre-1.0. The feature surface is stable enough to use daily; the open
work is pen tilt/ExpressKeys, a Helix keymap preset and the UX follow-ups in
the audit.

## License

Modaline is free software under the **GNU General Public License v3.0 or
later** (`GPL-3.0-or-later`). See [`LICENSE`](LICENSE). No warranty; use at your
own risk.

- Test fixtures in `tests/fixtures/ase/` are MIT-licensed third-party data:
  [`tests/fixtures/README.md`](tests/fixtures/README.md).
- Palette, font and dependency notices: [`THIRD_PARTY.md`](THIRD_PARTY.md).
- Aseprite is a trademark of Igara Studio S.A. Modaline is an independent
  project, not affiliated with or endorsed by Aseprite.
