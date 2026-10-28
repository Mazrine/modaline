# Roadmap

**This is the backlog.** P0-P3 are closed and kept for the record; **P4 is the
open set**. `docs/PROGRESS.md` has the immediate next step. "Full Aseprite
parity" is the north star, but the project is already a usable editor — the rest
is depth and correctness.

## P0 — foundation — **done**

Done: **layer groups & masks** (nested groups, per-layer alpha masks, lock /
editable flags) and **Aseprite format fidelity** (linked cels, compressed
tilemaps with embedded tilesets, cel opacity, indexed/grayscale export, real
fixtures), **tile mode** (3x3 seamless preview + wrapping cursor) and **frame
tags + reorder**.

Nothing left in P0.

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

## P3 — rendering & polish — **done 2026-10-26**

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

## P4 — open work — **audited 2026-10-28**

### Correctness & data loss — **done 2026-10-28**

- **Unsaved-changes guard on window close** — `CancelClose` plus a status
  warning; a window-manager close cannot drop a dirty sprite.
- **`:open!` / `:new!` / `:import!`** — a dirty sprite is no longer replaced
  without a prompt.
- **`:wq` dispatch by extension** — a `.ase` name no longer receives `.vpx`
  JSON, and a failed save no longer quits.
- **Autosave / crash recovery** — a dirty sprite is snapshotted to
  `$XDG_CONFIG_HOME/vixel/recovery.vpx` every 30 s; `:recover` loads it (dirty,
  no project path, ready for `:w`); a successful save or a forced quit removes
  it.

### Known limitations, promoted from the hand-off note

Done 2026-10-28: **structural undo** — layer add/delete/move/indent/rename/lock,
frame add/delete/duplicate/move, and tag create/delete each take one
`UndoStep::Structure` snapshot, and the active layer/frame/tag travel with both
sides. Panel visibility/opacity/blend/lock are cheap `LayerProps` steps (one
opacity drag = one step), and mask ops were already snapshots. **Merged copy**
also landed (`Y` / `:copy merged` flattens the visible layers inside the
selection); `y` stays layer-only, and cut/move act on the active layer as in
Aseprite.

Everything actionable in this group is done. Tileset/tilemap editing moved to
"Explicitly out of scope" below, with a plan and a re-open condition.

Format limit, not a deferred feature: **masks stay `.vpx`-only**. The official
`.ase` spec has no per-layer alpha mask (chunk 0x2016 is a deprecated,
sprite-level 1-bit mask), so there is nothing faithful to write. Verified
against the spec on 2026-10-28.

### Aseprite parity gaps (previously untracked)

Done 2026-10-28: **crop / trim** and **layer duplicate / merge down / flatten**
— `:crop [x y w h]` (no args = the selection) and `:trim` (non-transparent
content) resize every cel and layer mask; `:duplayer`, `:merge` (bakes opacity,
blend and mask into the layer below) and `:flatten` (visible layers to one) do
the layer work. All are single undo steps and have panel buttons. **Contiguous
toggle + fill tolerance** landed too: `:contig on|off` switches the wand and
bucket between the seed's region and every matching pixel, and the shared
`:wand <0-255>` tolerance now drives both. **Export scale** also landed:
`:export <target> [scale]` upscales PNG, sheet and GIF with nearest-neighbour,
and the sheet meta reports the scaled frame size. **Sheet import** also landed:
`:importsheet <sheet.png> [cols rows]` reads the sidecar `:export` writes (or an
explicit grid) and rebuilds the frames with their durations. **Reference /
background layer kinds** also landed: `:reference` marks a tracing layer (drawn
but never painted or exported), `:background` marks the opaque bottom layer,
and both flags round-trip in `.ase` (bits 64 and 8). **Pixel-perfect freehand**
also landed: `:perfect on|off` (1 px, no symmetry) joins pointer steps with a
line and drops the L-shaped corner pixel; the whole stroke is still one undo.
**File entry** also landed: drop a file on the window to open it (dirty-guarded),
and the last 10 opened/saved files live in the config (`:recent`,
`:recent <n>`). A native file dialog is deliberately not added — path entry,
drag-and-drop and `:recent` cover it in a keyboard-first editor.

## Explicitly out of scope (for now)

- Web/Wasm build.
- Multi-document / tabbed interface.
- Plugins / scripting (Aseprite's Lua API).
- **Tilesets / tilemap editing.** The `.ase` reader already parses compressed
  tilemap cels and embedded tilesets (rasterizing them, flips included), which
  is the natural base. Real editing needs a tileset/tile model, a tile picker,
  tile-aware paint/undo, and tileset chunk writing — a feature-sized batch —
  and there is no fixture or consumer asking for it. Re-open when a real
  external-tileset file must round-trip or tile editing is requested.

Verification: `docs/TESTING.md` drives the real GUI under Xvfb with a window
manager and records what each pass proved.

P0-P3 are closed, with the three deferred sub-items under their stated
conditions (ICC embedding, custom shaders, keymap customization). **P4 is
closed.** Its last item, tileset/tilemap editing, moved to "Explicitly out of
scope" with a plan and a re-open condition; everything else in P4 shipped.
