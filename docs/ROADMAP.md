# Roadmap

**This is the backlog.** P0-P4 are closed and kept for the record; **P5 is the
open set — full capability parity with Aseprite**. `docs/PROGRESS.md` has the
immediate next step and `docs/ASEPRITE_PARITY.md` is the live capability matrix
(every Aseprite capability, its status, and the task that closes it).

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

## P4 — closed 2026-10-28

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

Everything actionable in this group is done. Tileset/tilemap editing is now
P5-29..P5-33 in the parity programme below.

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

## P5 — full Aseprite parity — **open**

Goal: **nothing Aseprite can do is missing**. Different or better UI and
implementations are allowed; capability gaps are not. `docs/ASEPRITE_PARITY.md`
is the live matrix; this list is the ordered work.

Working rules: one task per loop iteration on `feat/aseprite-parity`; one
Conventional Commit per task (`Refs: P5-xx`); update the matrix and
`docs/PROGRESS.md`; run `cargo fmt --check`, `cargo test`, and
`cargo clippy --all-targets -- -D warnings`; UI tasks use the
`docs/TESTING.md` Xvfb/twm gate or record a manual check. Sizes: S ≈ one
iteration, M ≈ 1-2, L ≈ 2-4. XL work is split before starting. No `main` merge
or push without explicit approval. A task is done only with a test/gate.

Dependencies matter: the loop picks the first unchecked task whose deps are
met; blocked tasks are marked with the reason.

### Phase 0 — land P4 and set up the programme

- [x] **P5-00 — Review and merge P4; create `feat/aseprite-parity`** (M) —
  deps: none. Landed 2026-10-28.
  Review stayed clean; the gate was 97 tests + clippy + fmt + wgpu build. The
  Xvfb/twm smoke pass (text stamp, mouse paint, dirty close guard, save/export,
  recents, argv open, autosave recovery) found **one real bug**: eframe sleeps
  while idle, so the 30 s autosave only ran when input arrived. Fixed in
  `cc25a29` with `request_repaint_after`, then re-verified: recovery writes
  while idle and `:q!` clears it. Drag-and-drop was not exercised (xdotool
  cannot XDND). Fast-forward merged into `main`; this branch was created from
  it.
- [x] **P5-01 — Parity matrix + backlog wiring** (S) — deps: none.
  `docs/ASEPRITE_PARITY.md` (one row per Aseprite capability), this P5 section,
  PROGRESS/README/AGENTS pointers and the matrix-update convention. Landed
  2026-10-28; P5-00 will carry these docs to `main` with the P4 merge.

### Epic 1 — Correctness foundation

- [x] **P5-02 — Selection clips every paint path** (M) — deps: none.
  Landed 2026-10-28. `paint_pixel` now enforces bounds + the current selection
  (checked without cloning; the live marquee resolves via `current_selection`).
  `paint_pixel_unclipped` is the named exemption for paste, selection-transform
  writes and contour. `FillOptions` gained a clip mask, so bucket fills stop at
  the selection edge in both contiguous and global mode, and a seed outside the
  selection does nothing. 7 new tests (104 total): pencil/line/brush+symmetry,
  bucket, outside seed, empty stroke, text+bounds, paste/contour,
  `Op::Px`/`Op::Fill`.
- [x] **P5-07 — `.ase` unknown-chunk pass-through + harness** (M) — deps: none.
  Landed 2026-10-28. `Sprite.raw_chunks` keeps every chunk vixel does not parse
  (slices, user data, ICC, ...) with its frame and a `ChunkAnchor` (frame start,
  layer/cel/palette/tags/tileset); `save` emits each group after the same
  object and falls back to the frame end when that object was not written.
  Frame header old/new chunk counts are now real (they were zero). 3 new tests
  (107 total): header counts, anchored unknown-chunk order, and open→save→open
  on all four real fixtures (composites + raw chunk bodies). Group order can
  shift relative to other groups; frame, kind, body and object attachment
  survive.
- [x] **P5-05 — All 19 blend modes** (M) — deps: none.
  Landed 2026-10-28. Added dodge, burn, hard/soft light, difference, exclusion,
  hue, saturation, color, luminosity and divide. `blend_fn` now takes the RGB
  triple so the W3C HSL modes work; the 15 separable modes use W3C math plus
  Aseprite's Divide (`backdrop / source`, clamped, 0 stays 0). `.ase` words
  6-15 and 18 map both ways and `ALL_BLENDS` has 19 entries. 2 new tests
  (109 total): an independently computed reference table for all 19 at α=255
  and α=128, and a save→load round trip of every word. UI: the layer combo is
  generated from `ALL_BLENDS`; not screenshotted this iteration (manual check:
  open the layer panel and count the 19 entries).

### Epic 2 — Colour depth

- [ ] **P5-03 — Real indexed colour** (L) — deps: P5-01.
  Palette on `Sprite` + transparent index; quantize on `:mode indexed`; resolve
  paint to palette entries; read/write header byte 28; indexed `.ase` round
  trip.
  Gate: indexed fixture open→edit→save→open keeps indices/transparency; >256
  colours pick a palette; palette survives `.vpx`/`.ase`.
- [ ] **P5-04 — Dithering engine** (M) — deps: P5-03.
  Bayer 2/4/8 ordered + Floyd–Steinberg; RGB→indexed and gradients; `:dither`.
  Gate: deterministic matrix tests; indexed export honours the mode.
- [ ] **P5-06 — Palette editor and picker depth** (M) — deps: P5-03.
  Presets/defaults, insert/delete at index, sort, names round-trip, picker
  wheel + recent colours.
  Gate: palette names in `.ase`; editor/picker tests.
- [ ] **P5-08 — Header and small-chunk fidelity** (M) — deps: P5-03, P5-07.
  Grid origin + size (`.vpx` migration), transparent index, palette names, tag
  direction 3.
  Gate: header round trip; direction 3 in `.ase`.

### Epic 3 — Brush, ink, tools

- [ ] **P5-18 — Ink engine** (L) — deps: P5-02, P5-03.
  `Ink` (simple, lock-alpha, replace, shading, erase), per-tool ink, alpha-lock
  flag, indexed dab resolution.
- [ ] **P5-19 — Custom brushes** (L) — deps: P5-18.
  Brush image masks, square/circle built-ins, spacing, library save/load,
  cursor preview, `for_each_stamp` for pencil/eraser/shapes.
- [ ] **P5-20 — Pressure/tablet** (M) — deps: P5-19.
  Spike `egui::Event::Touch.force`/eframe mapping; thread `pressure` through
  `paint_at`/`paint_with_tool`; if the backend cannot deliver it, ship a manual
  pressure control and document the limit.
- [ ] **P5-21 — Tool depth and consistency** (M) — deps: P5-19.
  Brush 1-64 everywhere, shape thickness, spray rate/radius, shade
  strength/ramp, blur radius unified with `box_blur`, contour width/position.
- [ ] **P5-22 — Gradient depth** (M) — deps: P5-04.
  Multi-stop gradients, presets, alpha, dithering, selection-aware; UI.
- [ ] **P5-23 — Filters breadth** (L) — deps: none.
  Curves, levels, colour balance, desaturate, median, convolution/emboss,
  outline options; selection-aware, one undo step.
- [ ] **P5-24 — Transform depth** (L) — deps: P5-10.
  RotSprite rotation; interactive move/scale/rotate/skew handles with pivot;
  command/panel equivalents stay.
- [ ] **P5-25 — Selection extras** (M) — deps: none.
  Stroke/fill selection, colour-range, selection from layer alpha, selection
  transform handles.
- [ ] **P5-26 — Wide pixels rendering** (S) — deps: none.
  Render the canvas with `pixel_aspect`, plus a preview toggle.
- [ ] **P5-27 — Tiled-mode options** (S) — deps: none.
  Configurable X/Y repeats and preview options; seam wrapping stays.

### Epic 4 — `.ase` document fidelity

- [ ] **P5-09 — Colour profile chunk 0x2007** (M) — deps: P5-07.
  Read type/gamma/ICC; write sRGB; retain ICC; document linear's limit.
- [ ] **P5-10 — Cel bounds (offset cels)** (L) — deps: P5-07.
  `Cel` bounds; stop baking into canvas-sized cels; update paint/transform/
  undo/export/composite.
- [ ] **P5-11 — Cel z-index** (M) — deps: P5-10.
  Read/write cel z; order siblings in `composite_range`; timeline control.
- [ ] **P5-12 — Linked cels authoring** (L) — deps: P5-10.
  Linked cel content, copy-on-write on paint, write cel type 1, prefer-linked
  flag, timeline toggle.
- [ ] **P5-13 — Layer flags and UUIDs** (M) — deps: P5-07.
  Lock movement, prefer linked cels, collapsed group, layer UUID + header bit.
- [ ] **P5-14 — User data 0x2020** (M) — deps: P5-07.
  Text/color/properties on layer/cel/tag/slice/tileset; implement the
  “attaches to the previous chunk” ordering rule.
- [ ] **P5-15 — Slices 0x2022** (L) — deps: P5-07, P5-14.
  Slice model (name, bounds, 9-patch, pivot, color), read/write, list UI,
  export by slice, JSON metadata.
- [ ] **P5-16 — Text object model** (L) — deps: P5-14.
  Text objects stored in user data and rasterized; `.ase` text-property round
  trip.
- [ ] **P5-17 — External files + deprecated chunks** (S) — deps: P5-07.
  Pass through 0x2008; fix external-tileset skip/flag; document deprecated
  chunks left unsupported.

### Epic 5 — Text UI

- [ ] **P5-28 — Text tool** (L) — deps: P5-16.
  `Mode`/`Tool::Text`, placement, live preview, fonts (bitmap + TTF if
  accepted), size, alignment, colour, edit/re-rasterize, one undo step.

### Epic 6 — Tilesets and tilemaps

- [ ] **P5-29 — Tileset model + retained import** (L) — deps: P5-07.
  `Sprite.tilesets` (id/name/tile size/base index/flags/tiles); fix ID, base
  index, external skip and flip order; persist in `.vpx`.
- [ ] **P5-30 — Tilemap layer model** (L) — deps: P5-29, P5-10.
  Layer kind, cel tile refs + flips, retain/rasterize, composite.
- [ ] **P5-31 — Tilemap editing** (L) — deps: P5-30.
  Draw tiles, picker, empty tile, manual mode, pixel-edit mode, tile undo, UI.
- [ ] **P5-32 — Write tileset/tilemap chunks** (M) — deps: P5-30.
  0x2023 + cel type 3; `open→save→open` tilemap fixture.
- [ ] **P5-33 — Tileset depth** (M) — deps: P5-29, P5-32.
  Edit tile pixels, `--export-tileset`, external tileset read, auto/stack modes.

### Epic 7 — Timeline and animation

- [ ] **P5-34 — Multi-cel selection** (L) — deps: none.
  Layer×frame selection, paint/transform/export fan-out, timeline modifiers.
- [ ] **P5-35 — Cel operations** (M) — deps: P5-11, P5-12, P5-34.
  Copy/paste cels, empty cel vs frame, z/link controls in the timeline.
- [ ] **P5-36 — Preview window** (M) — deps: none.
  In-window preview + optional OS viewport; independent playback/zoom/loop,
  fullscreen.
- [ ] **P5-37 — Timeline UX** (M) — deps: P5-34.
  Layer×frame grid, onion controls, tag editor, drag multi-select, per-cel
  thumbnails.
- [ ] **P5-38 — Undo history** (M) — deps: none.
  History panel; evaluate/implement non-linear undo; otherwise document why
  the linear stack is kept.

### Epic 8 — Export/import pipeline

- [ ] **P5-39 — Export config core** (M) — deps: none.
  Frame range/tag/layer filters, ignore-empty, merge duplicates, scale, trim,
  padding, meta/GIF switches; command + dialog.
- [ ] **P5-40 — PNG sequence + filename templates** (M) — deps: P5-39.
  `{frame}`, `{layer}`, `{tag}`, `{title}`, `{duration}`; per-frame files.
- [ ] **P5-41 — Sheet layouts** (L) — deps: P5-39.
  horizontal/vertical/rows/columns/packed, width/height/columns/rows,
  border/shape/inner padding, extrude, trim/crop.
- [ ] **P5-42 — Atlas metadata + GIF options** (M) — deps: P5-41.
  json-hash/json-array, layers/tags/slices, filename formats; GIF loop, palette
  and dither.
- [ ] **P5-43 — Extra image formats** (M) — deps: P5-40.
  JPG/BMP/TGA/WebP; evaluate FLC/FLI/PCX and document any deliberate gap.
- [ ] **P5-44 — Import parity** (L) — deps: P5-39.
  Aseprite JSON/XML atlas, image sequence, sheet layouts, layers/tags.
- [ ] **P5-45 — In-app file browser** (M) — deps: none.
  Keyboard open/save browser with recents/favourites; no native dialog dep.

### Epic 9 — CLI and automation

- [ ] **P5-46 — CLI batch core** (L) — deps: P5-39, P5-40, P5-41, P5-42, P5-03,
  P5-04.
  `-b`, save-as/sheet/data, scale, colour-mode, dithering, frame range/tag/
  layer filters, trim/crop, list-*; headless tests.
- [ ] **P5-47 — CLI batch advanced** (L) — deps: P5-46, P5-15, P5-33.
  split-layers/tags/slices/grid, filename templates, padding/extrude/
  ignore-empty/merge, export-tileset, palette.
- [ ] **P5-48 — Scripting engine decision** (S) — deps: none.
  `docs/SCRIPTING.md` comparing Lua/mlua, Rhai, QuickJS/WASM, Python and JSON
  `Op`; default **Lua/mlua** (Aseprite script compatibility); check offline
  dependency availability.
- [ ] **P5-49 — Scripting MVP** (L) — deps: P5-48.
  Sandboxed engine, core API, `:script`/`--script`.
- [ ] **P5-50 — Scripting API coverage** (L) — deps: P5-49.
  Aseprite API compatibility matrix; run representative scripts.
- [ ] **P5-51 — Extensions** (L) — deps: P5-49, P5-50, P5-53.
  `.aseprite-extension` zip/manifest, install/remove, scripts/themes.

### Epic 10 — App shell and UX

- [ ] **P5-52 — Multi-document/tabs** (L) — deps: none.
  `Vec<Doc>` + active index, per-doc state, session persistence, aggregated
  dirty close, tab keys.
- [ ] **P5-53 — Themes** (M) — deps: none.
  Theme file format + built-ins; font customization.
- [ ] **P5-54 — Workspace layouts** (M) — deps: P5-52.
  Saved/shared panel layouts; reset.
- [ ] **P5-55 — Keymap customization** (M) — deps: none.
  Action layer, key/wheel bindings, conflicts, persisted map.
- [ ] **P5-56 — Localization infrastructure** (M) — deps: none.
  String extraction, language files, runtime switch.
- [ ] **P5-57 — Preferences dialog** (M) — deps: none.
  One dialog for editor/grid/export/theme/keys settings.
- [ ] **P5-58 — Render cache / dirty rects** (L) — deps: none.
  Incremental composite cache, dirty rects, texture reuse; benchmark.
- [ ] **P5-59 — Undo memory + filter threading** (M) — deps: P5-38.
  Compact structural undo, bounded history, filters off the UI thread where
  safe.

### P5 definition of done

`docs/ASEPRITE_PARITY.md` has no unexplained Missing/Partial row; every P5 task
is checked or marked blocked with a reason; the full gate is green; and the
`.ase` round-trip fixtures in `docs/TESTING.md`/`docs/ASEPRITE_PARITY.md` pass.

## Explicitly out of scope (for now)

- Web/Wasm build (Aseprite has no web build; product choice).
- Custom GPU shaders (a vixel extra, not Aseprite parity; P3 deferral stays).

Verification: `docs/TESTING.md` drives the real GUI under Xvfb with a window
manager and records what each pass proved.

P0-P4 are closed. **P5 is the open set**: full Aseprite capability parity,
tracked in `docs/ASEPRITE_PARITY.md` and listed above. The P3 sub-items keep
their re-open conditions; keymap customization is P5-55 and ICC embedding is
P5-09.
