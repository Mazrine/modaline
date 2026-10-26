# Roadmap

**This is the complete backlog.** `docs/PROGRESS.md` has the immediate next step;
everything still to do should be listed here. "Full Aseprite parity" is the north
star, but the project is already a usable editor — the rest is depth and polish.

## P0 — next up

Done: **layer groups & masks** (nested groups, per-layer alpha masks, lock /
editable flags) and **Aseprite format fidelity** (linked cels, compressed
tilemaps with embedded tilesets, cel opacity, indexed/grayscale export, real
fixtures), **tile mode** (3x3 seamless preview + wrapping cursor) and **frame
tags + reorder**.

Nothing left in P0 — promote from P1.

## P1 — editing depth — **done 2026-10-26**

- **Onion-skin settings** (count 1-3, opacity, both tints) and **loop /
  ping-pong playback** — `870947c`. `advance_frame` now returns whether playback
  continues, so a loop-less clip stops instead of wrapping.
- **Layer panel drag-to-reorder** — `870947c`. Rows are drag sources and drop
  targets; the move uses `sprite.move_layer`, so subtrees stay intact.
- **Text tool** — `f35381c`. 8x8 bitmap font (`font8x8`), integer scale 1-8,
  `:text <string>` + a text field and stamp button, one undo step.
- **Palette tools** — `688ea49`. Hex field and H/S/V drag values for fg/bg,
  swatches drag into order with fg/bg following their colour, `:fg`, `:bg`,
  `:pal move`. Palette-from-sprite and the colour modes were already done.
- **Symmetry settings** — `03a9f1a`. Radial N-fold (2-16 axes) and a centre
  that moves to the cursor or explicit coordinates; guides follow the centre.
- **Grid settings** — `696c653`. Design grid 1-64 px with its own tone, snap to
  grid for the cursor and every canvas pick, `:grid`, `:snap`.

## P2 — export, filters & transform — **done 2026-10-26**

- **Export options** — `a6d11f9` adds slice export (`:slices <dir> [cols rows]`,
  one PNG per grid cell plus a `.slices.json`); sheet/GIF/JSON already shipped.
- **Filters** — `0937562`. invert, brightness, contrast, hue, saturation,
  outline, blur, sharpen; selection-aware, one undo step, alpha-weighted blur.
- **Selection ops** — `52daa86`. expand, contract, border, feather (dithered
  edge on a 1-bit mask) and invert.
- **Free transform** — `f0c896e`. Arbitrary rotate, scale and skew about the
  selection centre, nearest-neighbour, one undo step.
- **Sprite properties** — `23123fa`. Colour mode (auto/rgba/grayscale/indexed,
  the declared mode drives the .ase depth), pixel ratio (round-trips through the
  .ase header) and the sprite grid.

## P3 — rendering & polish

- **Colour profiles** — `faabab4`. sRGB and linear-light as a declared, real
  property: converting rewrites the pixels and the palette together, and the
  export sidecar states the profile. **Deferred: ICC embedding.** Aseprite's
  0x2007 chunk is an opaque ICC blob we can neither author nor verify here;
  half-parsing it would silently corrupt profiles. Re-open when a real ICC
  fixture exists.
- **wgpu backend** — done. `--features wgpu` plus `--wgpu` (or `renderer: wgpu`
  in the config) swaps the renderer; nothing else changes because all drawing
  goes through `egui::TextureHandle`. Verified headlessly under Xvfb (EGL
  software path) at 1280x800 with the same frame.
- **Custom shaders** — **deferred, on purpose.** A real implementation is a
  backend-specific pass (glow or wgpu) behind a `Renderer` trait, and there is
  no GPU in the verification loop (Xvfb runs llvmpipe) and nothing in the
  product that needs a live effect yet. The CPU filter pipeline already covers
  the colour work. Re-open when a shader can be verified against a screenshot
  test on real hardware.
- **Config/theme persistence** — `0d4fa93`. Window size, zoom, pixel/design
  grid, snap, onion settings, loop/ping-pong and the theme, in
  `$XDG_CONFIG_HOME/vixel/config.json`. **Deferred: keymap customization** —
  `input.rs` dispatches a flat match on `Key`; remapping needs an action layer
  in front of it, and nothing has asked for a remap yet. The config already has
  an `extra` map so a keymap can be added without breaking old files.

## Explicitly out of scope (for now)

- Web/Wasm build.
- Multi-document / tabbed interface.
- Plugins / scripting (Aseprite's Lua API).

Nothing in P1-P3 is open except the three explicitly deferred sub-items
(ICC embedding, custom shaders, keymap customization), each with the condition
that would re-open it.
