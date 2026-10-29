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

- [x] **P5-03 — Real indexed colour** (L) — deps: P5-01.
  Landed 2026-10-28. `Sprite.palette` + `transparent_index`; `:mode indexed`
  quantizes to ≤255 opaque colours (median cut above that) plus a transparent
  slot, as one undo step; `Sprite::set`, paint and fill snap to the nearest
  entry. `.ase` reads/writes header byte 28 and the palette, matches indices by
  RGBA and pads undeclared slots with opaque black; `.vpx` stores the palette.
  6 new tests (115 total): >256 quantize + determinism, paint snapping, editor
  mode/undo/palette sync, opening an indexed fixture syncs the picker, `.vpx`
  palette round trip, and fixture edit→save→open checking header byte 28 and
  exact cel indices. Dithering stays P5-04; palette editing/names stay P5-06.
- [x] **P5-04 — Dithering engine** (M) — deps: P5-03.
  Landed 2026-10-28. `DitherMode` (none, Bayer 2/4/8, Floyd–Steinberg),
  `Sprite::dither_to_palette` and `:dither`; `:mode indexed` applies the current
  choice. Bayer matrices are generated recursively; Floyd–Steinberg carries
  7/16, 3/16, 5/16 and 1/16 of the error. 5 new tests (120 total): the exact
  2×2 Bayer pattern, an FS `B B W W` ramp, determinism across all four modes,
  editor conversion + undo/palette sync, and dithered `.ase` indices surviving
  save→load. P5-22 gradients can reuse `dither_to_palette`.
- [x] **P5-06 — Palette editor and picker depth** (M) — deps: P5-03.
  Landed 2026-10-28. Presets `default`, `gray`, `pico8`, `dawnbringer16`;
  `:pal insert/delete/sort/name` with colours and names kept parallel and
  mirrored into the document when indexed. `.ase` reads/writes UTF-16 palette
  names (flag bit 1) and `.vpx` stores them. The picker gained a hue wheel and
  the last 12 recent colours; the side panel has a name field and preset combo.
  5 new tests (125 total): presets, index edits/name/sprite sync, recent dedup
  and cap, `.ase` name flag/length + round trip, and wheel angle mapping. UI:
  wheel/name field not screenshotted (manual check: drag the ring, type a name,
  click a recent swatch).
- [x] **P5-08 — Header and small-chunk fidelity** (M) — deps: P5-03, P5-07.
  Landed 2026-10-28. `Sprite.grid_origin` (serde default for old `.vpx`), header
  SHORT x/y + WORD w/h read/write, `:sgrid [x y] <w> <h>` and panel origin
  drags. `PlayDirection::PingPongReverse` maps Aseprite's byte 3 and playback
  starts at the tag's last frame. Transparent index and palette names were
  already covered by P5-03/P5-06. 5 new tests (130 total): header bytes +
  round trip, `.vpx` origin + missing-field default, direction byte mapping,
  direction-3 tag chunk + round trip, and reverse ping-pong playback. UI origin
  drags not screenshotted (manual check: change origin, save, reopen).

### Epic 3 — Brush, ink, tools

- [x] **P5-18 — Ink engine** (L) — deps: P5-02, P5-03.
  Landed 2026-10-28. `Ink` (simple, lock-alpha, replace, shading, erase), one
  remembered per tool; an alpha-lock flag blocks painting on transparent
  pixels. All brush paths run through `apply_ink`; cut/transform/paste/contour/
  filters use an exact put path so they stay ink-independent. Indexed dabs still
  resolve through `Sprite::set`. `:ink`, `:alphalock` and a panel selector expose
  it. 2 new tests (165 total): every ink mode's math and per-tool ink/alpha-lock
  behaviour. UI selector not screenshotted (manual check: switch ink and paint).
- [x] **P5-19 — Custom brushes** (L) — deps: P5-18.
  Landed 2026-10-28. `Brush` coverage masks: square/circle built-ins and PNG
  custom brushes loaded through `:brush load`, saved to/loaded from the
  `vixel/brushes` library, with percentage spacing. Freehand interpolation,
  shapes and symmetry all stamp the mask; the canvas cursor previews its
  coverage. 4 new tests (169 total): built-in coverage, PNG round trip, custom
  mask limits, and circle/shape/spacing. UI preview not screenshotted (manual
  check: load a brush and hover the canvas).
- [x] **P5-20 — Pressure/tablet** (M) — deps: P5-19.
  Landed 2026-10-28. `egui::Event::Touch.force` is captured and touch/pen events
  paint at the mapped sprite pixel; pressure scales brush-mask coverage (so it
  affects freehand, pixel-perfect and shapes). Mice carry no force on X11, so a
  manual slider plus `:pressure <0-100>|auto` is the fallback and takes over
  device force until `auto`. 1 new test (170 total): pressure scaling and the
  manual/device override order. Tablet hardware unavailable: manual fallback
  smoke-checked in the UI only.
- [x] **P5-21 — Tool depth and consistency** (M) — deps: P5-19.
  Landed 2026-10-28. Shade, blur and spray now stamp the active brush mask
  (size 1-64), honouring pressure. Spray has rate/radius, shade has strength,
  the blur tool shares `box_blur`'s radius, and contour takes a width plus
  inside/outside. Commands: `:spray <dots> [r]`, `:shade <pct>`, `:blur <r>`,
  `:contour [1-64] [inside|outside]`; all four have a "tool depth" panel group.
  4 new tests (174 total): spray scatter bounds, shade strength/brush coverage,
  blur radius sampling, and contour width/side.
- [x] **P5-22 — Gradient depth** (M) — deps: P5-04.
  Landed 2026-10-28. Multi-stop gradients with interpolated alpha, seven presets
  (fg-bg, fg-transparent, bg-transparent, black-white, white-black, rainbow,
  fire), optional Bayer 2/4/8 ordered dithering, and selection clipping through
  `paint_pixel`. Commands: `:grad preset <name>`, `:grad stop <pct> [#hex]`,
  `:grad stops`, `:grad clear`, `:grad dither <mode>`; toolbar preset/dither
  combos. 3 new tests (177 total): stop interpolation/presets, dither level
  reduction, selection + alpha. Combos not screenshotted (manual check).
- [x] **P5-23 — Filters breadth** (L) — deps: none.
  Landed 2026-10-28. Added curves (3-point LUT), levels (input/output + gamma),
  colour balance, desaturate, posterize, median, 3×3 convolution with a custom
  kernel/divisor/bias, and emboss. Every filter stays selection-clipped and is
  one undo step. `:filter` now takes full arguments
  (`curves s m h`, `levels in_b in_w gamma [out_b] [out_w]`, `balance r g b`,
  `median r`, `convolve k0..k8 [div] [bias]`, `posterize n`, `emboss`).
  4 new tests (181 total): colour remaps, median/convolution/emboss, selection +
  single undo, and parser coverage.
- [x] **P5-24 — Transform depth** (L) — deps: P5-10.
  Landed 2026-10-28. RotSprite-style rotation/scale/skew samples a 4×4 grid per
  destination pixel and keeps the most frequent source colour (`:rotsprite
  on|off`). Free transforms can pivot on any point (`:pivot x y|clear`), drawn
  as a cross. With "handles" enabled, the canvas shows corner scale, edge skew
  and rotate handles plus an inside-drag move; drags apply one free transform
  on release. 2 new tests (183 total: 173 lib + 10 bin): RotSprite vs nearest
  and pivot rotation; handle hit-testing/pivot mapping. Handles are UI-only
  and were not screenshotted (manual check: enable handles and drag a corner).
- [x] **P5-25 — Selection extras** (M) — deps: none.
  Landed 2026-10-28. `:fill [#hex]` fills the selection (or cel) and `:stroke
  [width] [inside|outside] [#hex]` paints its edge, both one undo step.
  `:sel color [#hex] [tol]` selects matching pixels and `:sel alpha [all]`
  selects opaque pixels. `:sel rotate|skew <deg>`, `:sel scale <sx> [sy]` and
  `:sel offset <dx> <dy>` transform/move the mask without pixels; the same
  happens with handles when "sel only" is checked. 2 new tests (185 total):
  colour/alpha selection + fill/stroke, and mask transform/offset. "sel only"
  is UI-only and was not screenshotted (manual check).
- [x] **P5-26 — Wide pixels rendering** (S) — deps: none.
  Landed 2026-10-28. The canvas, checker, pixel/design grids, selection,
  floating paste, lasso, symmetry guides, cursor and transform handles all use
  `View::cell()` (zoom × `pixel_aspect`), as do hit-testing, fit and zoom
  anchoring. A "wide px" checkbox toggles aspect preview. 1 new test (186
  total): cell sizing, hit-testing and fit with a 2:1 ratio. Toggle is UI-only
  and was not screenshotted (manual check: set `:aspect 2 1` and toggle).
- [x] **P5-27 — Tiled-mode options** (S) — deps: none.
  Landed 2026-10-28. X/Y repeats (1-8 copies per side, default 3×3), neighbour
  opacity, and a preview toggle; the canvas extent and hit area follow the
  repeats while seam wrapping is unchanged. Commands: `:tile x|y <1-8>`,
  `:tile repeats <x> <y>`, `:tile opacity <0-255>`, `:tile preview on|off`;
  panel controls appear while tile mode is on. 1 new test (187 total): offset
  counts/clamping. Controls are UI-only and were not screenshotted (manual
  check: turn on tile mode and change the repeats).

### Epic 4 — `.ase` document fidelity

- [x] **P5-09 — Colour profile chunk 0x2007** (M) — deps: P5-07.
  Landed 2026-10-28. `AseProfile { kind, flags, gamma, icc }`; 0x2007 type,
  gamma and ICC bytes are parsed and written in frame 0. An imported profile
  wins; otherwise saving writes sRGB, or linear sRGB as fixed gamma 1.0.
  `:profile` shows the embedded profile; `:profile icc <file>`, `gamma <g>` and
  `clear` author it; `.vpx` stores it. 4 new tests (134 total): chunk bytes +
  ICC/gamma round trip, `prop.ase`'s 536-byte ICC open→save→open, linear
  fixed-gamma encode/decode, and summary + `.vpx`. ICC pixels load as-is into
  the sRGB working space (no colour-managed conversion).
- [x] **P5-10 — Cel bounds (offset cels)** (L) — deps: P5-07.
  Landed 2026-10-28. `Cel { x, y, w, h }` with cel-sized pixels. `.ase` import
  keeps offsets and the writer emits them; linked and tilemap cels carry real
  bounds. `Sprite::get/set`, composite, flips, rotate, masks, dithering, merge
  and flatten respect bounds; painting outside grows the cel. Crop/resize
  canonicalize to the new canvas. Old `.vpx` cels (no bounds) normalize on
  load. 5 new tests (139 total): clipping/growing, flip/rotate bounds, `.ase`
  offset round trip, `.vpx` migration, offset-cel paint + undo.
- [x] **P5-11 — Cel z-index** (M) — deps: P5-10.
  Landed 2026-10-28. `Cel.z_index` (serde default 0). `.ase` reads/writes the
  SHORT z field and composite sorts siblings by `-layerIndex + z` (ties by z),
  matching Aseprite's front ordering with our top-first list. `:celz <n>` and a
  timeline z drag set it; `UndoStep::CelZ` makes the change undoable. 3 new
  tests (142 total): composite ordering, `.ase` z bytes + round trip, and
  undo/redo. UI drag not screenshotted (manual check: change z, see the stack
  reorder).
- [x] **P5-12 — Linked cels authoring** (L) — deps: P5-10.
  Landed 2026-10-28. `Cel.link` resolves content by frame; painting materialises
  it (copy-on-write). `.ase` reads and writes cel type 1, and `Layer.prefer_linked`
  maps flag bit 16. `:link <frame>`, `:unlink`, `:preferlink on|off` and a
  timeline toggle author links; frame insert/duplicate/remove/move remap or
  materialise affected links. Global operations materialise links first. 5 new
  tests (147 total): sharing/COW, prefer-linked frames, frame-op remapping,
  type-1 round trip, and link/unlink undo. UI toggle not screenshotted (manual
  check: link a cel, paint it, see only that frame change).
- [x] **P5-13 — Layer flags and UUIDs** (M) — deps: P5-07.
  Landed 2026-10-28. `Layer.lock_movement` (flag 4), `collapsed` (flag 32) and
  `uuid` (16 bytes). `.ase` parses/writes UUIDs plus header bit 4 (and fixes
  the tileset/UUID field order). New layers and duplicates mint UUIDs when the
  sprite already uses them; cut, move and free transform refuse a movement-
  locked layer. `:movelock`, `:collapse`, `:layeruuid [new|clear]` and the
  panel `mv`/`col` toggles expose them; collapsed groups hide their children.
  4 new tests (151 total): flag+UUID bytes/round trip, fixture UUID
  preservation, movement lock, and UUID minting. Collapsed UI not screenshotted
  (manual check: collapse a group and reopen the file).
- [x] **P5-14 — User data 0x2020** (M) — deps: P5-07.
  Landed 2026-10-28. `UserData { text, color, properties, flags }` on layers,
  cels, tags and the sprite; the property payload is kept verbatim. The reader
  applies the “attaches to the previous object” rules (layer, cel, one per tag,
  sprite after the palette) and the writer re-emits them in the same positions.
  `.vpx` stores the data. 3 new tests (154 total): layer/cel/tag attachment and
  round trip, property payload + sprite data, and `prop.ase` open→save→open.
  Tileset/slice user data stays in the raw-chunk pass-through until P5-15/31.
- [x] **P5-15 — Slices 0x2022** (L) — deps: P5-07, P5-14.
  Landed 2026-10-28. `Slice`/`SliceKey` model (name, bounds, per-key 9-patch
  centre, pivot, frame ranges, user-data colour). 0x2022 is read/written and
  user data attaches to its slice. `:slices` lists slices, `:slices <dir>
  named` exports one PNG per slice plus `<stem>.slices.json`, and the side
  panel lists them. 3 new tests (157 total): synthetic chunk bytes + round
  trip, `test_grayscale.ase` slice round trip, and named PNG+JSON export. UI
  list not screenshotted (manual check: open a file with slices).
- [x] **P5-16 — Text object model** (L) — deps: P5-14.
  Landed 2026-10-28. A typed property-map codec covers every Aseprite value
  type. `TextObject { text, font, size, align, antialias, x, y }` is stored in
  the cel's user-data map key 0 (colour in the user-data colour field).
  `:textobj <text>` creates/edits and rasterises the bitmap font as one undo
  snapshot; empty cels with user data now write a cel chunk so the object
  survives. `.ase` text properties round-trip. 3 new tests (160 total):
  property codec, `.ase` text object, and editor rasterise/update/undo.
  Aseprite main still rasterises text (text layers are on its roadmap), so the
  property layout is vixel's forward-compatible representation.
- [x] **P5-17 — External files + deprecated chunks** (S) — deps: P5-07.
  Landed 2026-10-28. `ExternalFile { id, kind, name }` is parsed from and
  written to 0x2008. Tileset flag 1 now records the external file/tileset IDs
  and skips external pixels without treating the tileset as corrupt. Deprecated
  0x2006 cel-extra, 0x2016 mask and 0x2017 path chunks stay in the raw
  pass-through and are documented as such. 3 new tests (163 total): external
  files round trip, external tileset flag, and deprecated chunk preservation.
  Resolving external tileset pixels remains P5-33.

### Epic 5 — Text UI

- [x] **P5-28 — Text tool** (L) — deps: P5-16.
  Landed 2026-10-28. `Tool::Text` (`T`): click places a text object at the
  cursor; the panel edits multi-line text with live preview, and previews commit
  as exactly one undo step (`commit`). Font is the 8x8 bitmap ("vixel" only —
  TTF is not accepted), size 8-64, alignment left/centre/right, colour from fg
  or the cel's stored text colour. Commands: `:textfont`, `:textsize`,
  `:textalign`; `:textobj` still edits/queries. 2 new tests (189 total): live
  preview = one undo, and placement/size/alignment. UI preview not
  screenshotted (manual check: `T`, type in the panel, click the canvas).

### Epic 6 — Tilesets and tilemaps

- [x] **P5-29 — Tileset model + retained import** (L) — deps: P5-07.
  Landed 2026-10-28. `Sprite.tilesets: Vec<Tileset>` carries id/name/tile
  size/base index/flags/tile count/atlas/external refs, and `Layer.tileset`
  keeps the referenced ID. `.ase` import now looks tilesets up by ID (not list
  index), maps tile IDs through the base index, and applies flips in Aseprite's
  diagonal→X→Y order. Tilesets persist in `.vpx`. 3 new tests (191 total):
  retained model/ID/base/flip rendering, flip-order helper, and `.vpx`
  round-trip.
- [x] **P5-30 — Tilemap layer model** (L) — deps: P5-29, P5-10.
  Landed 2026-10-28. `Layer.tilemap` marks tilemap layers (with their tileset
  ID); `Cel.tilemap: Option<TilemapCel>` retains row-major `TileRef`s (id +
  X/Y/diagonal flips) while `pixels` stays the render cache. `render_tilemap`
  and `Sprite::rasterize_tilemap` rebuild bounds/pixels from refs; `.ase` import
  now fills both. Composite keeps using the cached pixels. Refs persist in
  `.vpx`. 2 new tests (192 total): retained refs/layer flag from `.ase`, and
  re-rasterization after edits (plus `.vpx` refs).
- [x] **P5-31 — Tilemap editing** (L) — deps: P5-30.
  Landed 2026-10-28. `Tool::Tile` (`K`) places the brush tile at the cursor
  (with X/Y/diagonal flips), right-click picks a tile into the brush, and tile
  0 / `:tileerase` places the empty tile. Every edit updates the retained refs,
  re-rasterises the cel and is one undo step. "pixel edit" routes the tile tool
  through the pencil so pixels can be edited directly (documented as a cache
  edit). Commands: `:tileid`, `:tileflip x|y|diag|none`, `:tilemode manual|pixel`,
  `:tilepick`, `:tileerase`. 2 new tests (194 total): paint/pick/erase/undo and
  pixel mode. Panel is UI-only and not screenshotted (manual check).
- [x] **P5-32 — Write tileset/tilemap chunks** (M) — deps: P5-30.
  Landed 2026-10-28. Frame 0 emits `0x2023` for every retained tileset
  (embedded atlas compressed at the chosen depth, or external file/tileset IDs);
  tilemap layers are written as kind 2 with their tileset ID, and tilemap cels
  as type 3 with zlib-compressed tile words (ID + X/Y/diagonal flips). Raw cel
  pixel encoding is shared with the tileset writer. 2 new tests (195 total):
  open→save→open tilemap fixture (refs, flips, bytes) and external tileset
  reference write.
- [x] **P5-33 — Tileset depth** (M) — deps: P5-29, P5-32.
  Landed 2026-10-28. Pixel-edit mode now maps the clicked cel pixel back through
  the tile's flips into the tileset atlas, then rerasterizes every cel using that
  tileset (one undo step). `--export-tileset in.ase [out.png]` and
  `:exporttileset [path]` write the atlas. External tilesets resolve from
  sibling PNG or `.aseprite` files (max depth 3) and rerasterize tilemaps.
  `:tilemode manual|auto|stack|pixel` plus the panel combo: auto fills empty
  orthogonal neighbours, stack advances variants. 3 new tests (198 total):
  atlas pixel edit + auto/stack, external PNG resolution, atlas export.

### Epic 7 — Timeline and animation

- [x] **P5-34 — Multi-cel selection** (L) — deps: none.
  Landed 2026-10-28. `Editor.cel_selection` stores explicit layer×frame pairs;
  the active cel always participates. Ctrl/Cmd-click toggles a frame or layer,
  Shift-click selects ranges; timeline thumbs/layers mark extra cels. Paint and
  exact writes fan out through `cel_targets`, transforms loop each selected cel
  and collapse to one snapshot, and `:exportcels <dir>` writes
  `l<layer>_f<frame>.png` per selected cel. Commands: `:celselect`,
  `:celclear`, `:exportcels`. 2 new tests (200 total): paint/transform/export
  fan-out and range selection. Timeline modifiers are UI-only (manual check).
- [x] **P5-35 — Cel operations** (M) — deps: P5-11, P5-12, P5-34.
  Landed 2026-10-28. Whole-cel clipboard (`Cel`) with copy/cut/paste that
  resolves links and works across layers/frames; cut/paste are one undo step.
  `:celdel` empties the active cel but keeps the frame (distinct from
  `:delframe`), with `:celinfo` reporting emptiness. Timeline cel controls keep
  opacity, z-index and link/unlink plus copy/cut/paste/empty buttons. Commands:
  `:celcopy`, `:celcut`, `:celpaste`, `:celdel`, `:celinfo`. 1 new test (201
  total): copy/paste/cut and empty-cel-vs-frame. Timeline buttons are UI-only
  (manual check).
- [x] **P5-36 — Preview window** (M) — deps: none.
  Landed 2026-10-28. "preview" opens a floating window; "preview OS" mirrors it
  in a separate egui viewport with a fullscreen toggle. The preview has its own
  frame/clock (`View::preview_tick`), play/pause, loop, zoom 1-16 and a frame
  scrubber, independent of the editor's active frame. Textures are cached per
  preview context/revision. 1 new test (202 total): independent playback and
  loop/stop behaviour. Both viewport modes are UI-only and were not
  screenshotted (manual check).
- [x] **P5-37 — Timeline UX** (M) — deps: P5-34.
  Landed 2026-10-28. Timeline has a "grid" mode with one row per layer and one
  cell per frame, per-cel thumbnails cached by revision, and a frame strip as
  the alternative. Drag across cells selects a rectangle (Ctrl toggles, Shift
  ranges); onion count/opacity sit in the timeline header. A tag editor edits
  name/range/colour, deletes the active tag and creates "new tag" at the
  playhead. 2 new tests (204 total): rectangle selection + tag mutations, and
  cel thumbnail sizing. Grid/tag UI is manual-check only.
- [x] **P5-38 — Undo history** (M) — deps: none.
  Landed 2026-10-28. A history window lists labelled undo steps, redo steps and
  the base state; clicking any row jumps there via repeated undo/redo
  (`goto_history`). Labels cover strokes (unique pixel count), snapshots,
  structure edits, layer properties and cel z. Branching undo is deliberately
  not implemented: steps mix coalesced cell patches with whole-sprite snapshots
  and active-index travel, so a tree would need per-node inverse patches and
  redo-branch semantics for little gain; jump navigation keeps every state in
  the linear history reachable. 1 new test (205 total): labels and
  jump/redo navigation.

### Epic 8 — Export/import pipeline

- [x] **P5-39 — Export config core** (M) — deps: none.
  Landed 2026-10-28. `ExportConfig` (frame range, tag, visible/all/indices
  layers, ignore-empty, merge duplicates, scale, global trim, padding,
  sheet/GIF/meta toggles) with `configured_sprite` deriving the export view and
  `export_config` writing the bundle. Padded sheets report `padding` in the
  meta. `:exportcfg <stem> [--frames a b] [--tag name] [--layers ...]
  [--ignore-empty] [--merge] [--trim] [--scale n] [--padding n] [--no-*]` plus
  a side-panel export dialog. 2 new tests (207 total): filter/merge/trim/layer
  derivation and the sheet/GIF/meta bundle. Dialog is manual-check only.
- [x] **P5-40 — PNG sequence + filename templates** (M) — deps: P5-39.
  Landed 2026-10-28. `ExportConfig.sequence` writes one PNG per filtered frame
  (or per layer with `per_layer`/`{layer}`), honoring scale. Templates support
  `{frame}` (1-based, 3 digits), `{layer}`, `{tag}`, `{title}` and `{duration}`,
  sanitize unsafe characters, auto-append `.png` and de-duplicate collisions.
  `:exportcfg ... --sequence [--per-layer] [--pattern <tpl>]` and dialog fields.
  2 new tests (209 total): token rendering/sanitizing and a 2-frame × 2-layer
  sequence with scaling.
- [x] **P5-41 — Sheet layouts** (L) — deps: P5-39.
  Landed 2026-10-28. `SheetLayout` supports horizontal, vertical, rows,
  columns and packed (row grid, equivalent for uniform frame sizes) with
  optional column/row overrides. Sheet padding is split into border, shape
  spacing and inner margin; `extrude` replicates edge pixels. Trim/crop comes
  from `ExportConfig.trim`. Metadata records layout and all paddings. Commands:
  `:exportcfg ... --layout h|v|rows|columns|packed --columns n --rows n
  --border n --shape n --inner n --extrude n`; dialog exposes the same.
  2 new tests (211 total): horizontal padding/extrude placement and
  vertical/columns grids.
- [x] **P5-42 — Atlas metadata + GIF options** (M) — deps: P5-41.
  Landed 2026-10-28. Atlas metadata supports vixel, JSON-hash and JSON-array
  formats with per-frame rects/timing, `frameTags`, layers and slices; filenames
  reuse the sequence template. GIF export takes a repeat count (0 = forever),
  palette-size (RGB cube) and ordered-dither toggle via
  `export_gif_configured`. Commands: `--atlas vixel|hash|array`,
  `--gif-loop n`, `--gif-colors n`, `--gif-dither`; dialog controls too.
  2 new tests (213 total): hash/array metadata with tags/layers/slices and GIF
  quantization/alpha/loop output.
- [x] **P5-43 — Extra image formats** (M) — deps: P5-40.
  Landed 2026-10-28. Enabled image-crate JPEG/BMP/TGA/WebP codecs; `:export
  file.ext` dispatches by extension and `:open`/`load_any` imports all of
  them. JPEG flattens transparency onto white at quality 92; other formats keep
  RGBA. GIF/PNG unchanged. FLC/FLI/PCX are a deliberate gap (no codec in
  `image`); documented in the matrix. 1 new test (214 total): all four formats
  export, scale, classify and BMP reloads exactly.
- [x] **P5-44 — Import parity** (L) — deps: P5-39.
  Landed 2026-10-28. `import_atlas` reads Aseprite/TexturePacker JSON
  hash/array and XML `SubTexture` metadata, reconstructing frames, durations,
  tags (direction/repeat/colour) and slices; `:importsheet` auto-detects atlas
  sidecars. `import_sequence_dir` loads a folder of PNG/JPEG/BMP/TGA/WebP frames
  in natural numeric order (explicit file lists supported). Layers in atlas
  metadata are merged into one pixel layer (metadata cannot un-composite them).
  Commands: `:importatlas`, `:importseq`. 3 new tests (217 total): hash
  round-trip, XML atlas, natural-order sequence.
- [x] **P5-45 — In-app file browser** (M) — deps: none.
  Landed 2026-10-28. A `files` window lists directories/files with filtering,
  parent/home navigation, recent files and persistent favourites; Ctrl/Cmd+O
  opens it, Ctrl/Cmd+Shift+S opens save mode. Dirty open is guarded unless
  "force" is ticked. Save dispatches by extension via `Editor::save_path`
  (`.ase`, image formats, else `.vpx`), defaulting to `.vpx`. Favourites persist
  in `config.json`. 2 new tests (219 total): directory listing/filtering and
  save-path format dispatch. Browser UI is manual-check only (not screenshotted).

### Epic 9 — CLI and automation

- [x] **P5-46 — CLI batch core** (L) — deps: P5-39, P5-40, P5-41, P5-42, P5-03, P5-04.
  Landed 2026-10-28. `vixel -b input [options]` runs fully headless:
  `--save-as`, `--sheet`, `--data`, `--gif`, `--scale`, colour-mode/dither,
  frame/tag/layer filters, trim/crop, sheet layout/padding/extrude, atlas and GIF
  options, and `--list-layers/tags/frames/slices/tilesets/palette`. Output paths
  are explicit; `--data` references `--sheet`. 3 integration tests (222 total):
  filtered/trimmed sheet+atlas, list output, and indexed conversion save.
- [x] **P5-47 — CLI batch advanced** (L) — deps: P5-46, P5-15, P5-33.
  Landed 2026-10-28. Batch mode gains `--split-layers <dir>`,
  `--split-tags <dir>`, `--split-slices <dir>`, `--split-grid <dir>
  [--columns/--rows]`, `--filename <tpl>`, `--ignore-empty`, `--merge`,
  `--export-tileset <png> [--tileset-id]` and `--palette <gpl|hex|pal>`.
  Splits use the configured filters/scale and filename templates; slices/grid
  export the first selected frame. 2 integration tests (224 total): layer/tag
  splits + palette and grid/slices/tileset.
- [x] **P5-48 — Scripting engine decision** (S) — deps: none.
  Landed 2026-10-28. `docs/SCRIPTING.md` compares Lua/mlua, Rhai,
  QuickJS/WASM, Python and JSON `Op`; decision is **Lua via mlua 0.10**
  (`lua54`, vendored) because Aseprite scripts must run unchanged. Sandbox,
  memory/instruction limits, API surface and residual risks are documented.
  Offline check found no mlua/rhai/quickjs/pyo3 sources or cache entries, so the
  dependency cannot be added here. No tests (decision/documentation).
- [x] **P5-49 — Scripting MVP** (L) — deps: P5-48.
  Landed 2026-10-29. `mlua 0.12` (Lua 5.4, vendored) is now a real dependency.
  `src/script.rs` runs scripts sandboxed (no io/os/package/debug/dofile/
  loadfile/require, 64 MiB memory cap, 50M-instruction budget) and exposes
  `app.version`, `app.alert`, `app.activeSprite` (width/height/frameCount/
  layerCount, resize/getPixel/setPixel/newLayer/newFrame/save/saveAs),
  `app.activeLayer` (name/visible/editable/opacity/isBackground/isReference)
  and `app.activeFrame`. A whole script is one snapshot undo step.
  `:script <file.lua>`, `:lua <code>` and CLI `--script` are wired. 5 unit tests
  + CLI integration (256 total). Xvfb/headless verified.
- [x] **P5-50 — Scripting API coverage** (L) — deps: P5-49.
  Landed 2026-10-29. Added `app.pixelColor` (rgba/rgb/gray/grayA/channel
  getters), `Color`, `app.transaction`, `app.command` (Undo/Redo/NewLayer/
  NewFrame), sprite `flatten`/`crop`/`newTag`/frame-duration API. The full
  Aseprite compatibility matrix (supported/partial/missing) is in
  `docs/SCRIPTING.md`; representative scripts run in unit tests and the CLI.
  Remaining gaps are documented (Image/Cel/Selection objects, Dialog/events/
  fs, newLayer/newFrame return indices). 2 new tests (258 total).
- [x] **P5-51 — Extensions** (L) — deps: P5-49, P5-50, P5-53.
  Landed 2026-10-29. `src/extension.rs` installs `.aseprite-extension` zips
  (`package.json` name/displayName/version/author/contributes.scripts/themes)
  under the config directory with path-traversal, file-count and 50 MB size
  limits; list/remove/get. `:ext install|list|remove|run|themes|theme`; scripts
  run through the sandboxed Lua engine and extension themes can be applied as
  vixel JSON themes. 3 tests (261 total). Xvfb smoke installed `demo`, listed it
  and ran its script (pixel painted).

### Epic 10 — App shell and UX

- [x] **P5-52 — Multi-document/tabs** (L) — deps: none.
  Landed 2026-10-28. `Vec<Doc>` (editor, view, textures, playback timer) with an
  active index; tab strip with dirty markers, close/new buttons; Ctrl/Cmd-Tab
  cycles, Ctrl/Cmd-Shift-Tab reverses, Ctrl/Cmd-T opens, Ctrl/Cmd-W closes.
  Session paths + active index persist in `config.json` and restore on launch;
  drops open a new tab. Dirty tabs refuse close, window close checks every tab,
  and `:q` refuses while another tab is dirty (only active `:q!` discards all).
  4 unit tests + Xvfb/twm smoke (2 tabs, Ctrl+T, Ctrl+Tab): 228 total.
  Fixed chord detection to read per-event modifiers (frame-end modifiers lost
  fast Ctrl+T).
- [x] **P5-53 — Themes** (M) — deps: none.
  Landed 2026-10-28. `src/theme.rs` defines a serde JSON theme (bg, panel,
  text, accent, grid, checker, dark flag) with four built-ins (`dark`,
  `light`, `midnight`, `paper`); `:theme <name|file.json>` applies and
  `:theme save <file>` exports. `:uifont <ttf|otf|default> [8-32]` installs a
  UI font and size, persisted in `config.json`; themes/fonts are per-tab and
  restored on switch. 2 theme tests + old-config defaults; 230 total. Xvfb
  smoke: midnight theme rendered, Noto font at 20 pt, saved JSON re-read.
- [x] **P5-54 — Workspace layouts** (M) — deps: P5-52.
  Landed 2026-10-28. `src/layout.rs` (library) defines a serde JSON layout
  (toolbar/timeline/side visibility, side width, timeline height) with four
  built-ins (`default`, `drawing`, `animation`, `compact`). `:layout <name|json>`
  applies, `:layout reset` restores defaults, `:layout save <file>` exports;
  current layout and dragged sizes persist in `config.json` and are per-tab.
  Panels now fill their allocated rect so saved sizes and drags hold; loading a
  layout seeds egui's `PanelState` to override remembered sizes. 3 tests + old
  config defaults (233 total). Xvfb smoke: animation (side hidden, 210 px
  timeline), JSON export, reset.
- [x] **P5-55 — Keymap customization** (M) — deps: none.
  Landed 2026-10-28. `src/keymap.rs` defines 28 actions (undo/save/quit/tools/
  frame/layer/zoom/grid/help/command) with default chords matching the modal
  keys. `:bind <action> <chord|wheel-up|wheel-down|none>`, `:bind list`,
  `:bind reset`; chords parse ctrl/alt/shift and named keys. Rebinding steals a
  conflicting chord (reported) and suppresses the old hardcoded default;
  keymaps are global across tabs and persist in `config.json`. Wheel actions
  consume scroll before canvas zoom. 5 tests (238 total). Xvfb smoke: `u`
  blocked after undo→ctrl-z, ctrl-z undid, `h`/`?` opened help, `wheel-down`
  zoomed.
- [x] **P5-56 — Localization infrastructure** (M) — deps: none.
  Landed 2026-10-28. `src/i18n.rs` + `lang/en.json`/`lang/pt-BR.json` define a
  JSON catalog with English fallback and missing-key checks. `:lang
  <code|file.json>` switches at runtime, `:lang list|check|save <file>` support
  translation; catalogs are global across tabs and persisted in `config.json`.
  Core labels are localized (theme, brush, text, symmetry, untitled, play/pause,
  frame buttons); status/help strings remain English. 4 tests (242 total). Xvfb
  smoke: `pt-BR` rendered `sem título`/`pincel`/`texto`/`simetria` and localized
  frame buttons, `check` reported complete, save/export re-read.
- [x] **P5-57 — Preferences dialog** (M) — deps: none.
  Landed 2026-10-28. `:prefs` opens one `preferences` window with collapsible
  sections: editor (brush/shape/spacing/pixel-perfect/snap/aspect), grid and
  canvas (pixel grid, design grid, snap cell, reset zoom/pan), export
  (sheet/gif/meta/sequence, trim/ignore/merge, scale, open export panel),
  appearance (theme, UI font size, language), keys (binding/conflict counts,
  reset) and workspace (layout name, reset). All values are the live
  editor/view/config fields, so existing persistence applies. UI-only task:
  Xvfb smoke opened the dialog and showed all sections; 242 tests unchanged.
- [x] **P5-58 — Render cache / dirty rects** (L) — deps: none.
  Landed 2026-10-28. The canvas texture is keyed by revision/frame/size and
  skipped entirely when unchanged; onion textures refresh only when
  revision/frame/count changes. Pixel writes (`Editor::record`) accumulate a
  merged dirty rectangle (structural edits set `dirty_full`), and the canvas
  patches the texture with `set_partial` instead of uploading the whole frame.
  Composite CPU work still runs full-frame when dirty (documented). 3 tests
  (245 total) cover upload plans, dirty merging and sub-image extraction; an
  ignored benchmark on a debug build measured 164.8 ms for a 256×256×8-layer
  composite vs 24 ns for a cache decision (`cargo test benchmark_canvas
  -- --ignored --nocapture`). Xvfb paint/undo smoke showed changes rendering.
- [x] **P5-59 — Undo memory + filter threading** (M) — deps: P5-38.
  Landed 2026-10-28. Undo/redo history is capped by `undo_limit` (default 100,
  configurable in preferences/config) and a 256 MB approximate byte budget;
  oldest steps are dropped, with `UndoStep::bytes` charging snapshots per
  layer/frame. `apply_filter_parallel` runs pointwise filters (invert,
  brightness, contrast, hue, saturation, curves, levels, balance, desaturate,
  posterize) on scoped worker threads for images ≥64 rows; neighborhood
  filters (blur/sharpen/outline/median/convolve/emboss) stay synchronous, and
  the editor still waits for the result (no async preview). 2 tests (247 total)
  cover bounded history and parallel/sequential equivalence for every pointwise
  filter. Honest remaining gap: true off-UI-thread/async filtering.
- [x] **P5-60 — Sprite background colour property** (S) — deps: P5-14.
  Landed 2026-10-28. Stored as the sprite user-data colour
  (`Sprite::background_color`/`set_background_color`), preserving text/property
  payloads. `:bgcolor [#rrggbb|clear]` sets/queries it as one undo step; the side
  panel shows a colour swatch with clear. Round-trips through `.ase` user data
  and `.vpx` JSON. 3 tests (250 total): editor undo/preserve, ase round-trip, vpx
  round-trip. Xvfb smoke showed `#ff8800` in the swatch and status.

### P5 definition of done

`docs/ASEPRITE_PARITY.md` has no unexplained Missing/Partial row; every P5 task
is checked or marked blocked with a reason; the full gate is green; and the
`.ase` round-trip fixtures in `docs/TESTING.md`/`docs/ASEPRITE_PARITY.md` pass.

## P6 — documented parity gaps — **open**

P5 shipped every planned task. P6 tracks the remaining matrix rows that are
Partial or Not supported, so "no unexplained gap" is true again.

- [x] **P6-00 — Move layer/cel content tool** (M) — deps: P5-24.
  Landed 2026-10-29. `Tool::Move` (`M`, toolbar) detaches the active cel into a
  floating preview (`Editor::start_cel_move`/`move_cel_to`/`commit_cel_move`);
  the source is emptied while dragging, commits clip at the canvas edges, and
  the whole drag is one `UndoStep::Snapshot` (zero move restores untouched).
  `Esc` cancels, switching tools commits. Locked movement, hidden/locked and
  reference layers refuse to start. 2 tests (226 total): translate + alpha
  preservation + clipping + single undo, and locked/editable refusal + cancel.
  Xvfb smoke: `M`, drag, content moved; `Esc` put it back; `u` undid a commit.
- [x] **P6-01 — Reopen closed documents** (S) — deps: P5-52.
  Landed 2026-10-29. Closing a clean, named tab pushes its path on a bounded
  (10, deduplicated) stack carried with the session (`config.closed`);
  `:closed` lists it, `:reopen` opens the newest in a new tab and pops it.
  Forced closes of dirty work never record a path, so discarded content is not
  offered back. 2 tests (265 total): stack order/dedup/bound and
  clean-vs-dirty close plus reopen. Xvfb smoke: opened `b.png` in a second tab,
  `Ctrl-W`, `:closed` listed it, `:reopen` restored the tab as active.
- [x] **P6-02 — Lua object API coverage** (L) — deps: P5-49, P5-50.
  Landed 2026-10-29. `Image(w,h)` tables (packed pixels) with get/set/clear/
  bounds; `sprite:cel(layer, frame)` returns layer/frame/bounds/image/isEmpty
  and `sprite:setCelImage` writes a detached image back; `app.selection` exposes
  bounds/isEmpty and `add`/`subtract`/`clear` rectangles; `Dialog{...}` with
  label/entry/check/button plus `app.confirm`/`app.prompt` run headless and
  return declared defaults; `app.fs.read/write/exists` is confined to
  `:scriptroot <dir>` or the open file's folder, rejecting absolute paths, `..`
  and symlink escapes. `docs/SCRIPTING.md` documents the matrix and limits.
  3 tests (268 total): objects, fs denials, headless dialogs. Xvfb smoke:
  `:scriptroot`, fs write, `Image`+cel write, selection add, absolute-path
  denial all reported in the status line.
- [ ] **P6-03 — Localization string extraction** (M) — deps: P5-56.
  Move status, help-overlay, window and command messages to catalog keys;
  complete `pt-BR`; add a test that the catalog covers every key used by the
  UI (or an explicit allowlist). Runtime switch must leave no mixed language.
- [x] **P6-04 — Incremental dirty compositing** (L) — deps: P5-58.
  Landed 2026-10-29. `Sprite::composite_rect` composites only a clipped
  rectangle (groups, masks, cel z and blend modes stay per-pixel), and the
  canvas partial upload path now builds a `ColorImage` from that rect instead
  of compositing the full frame and cropping it. `composite_range` delegates to
  the rect-aware core so full and partial share one implementation. 2 tests
  (269 total): the dirty patch matches the full image, and a 200-edit random
  case proves rect == full composite. Ignored benchmark (debug): 256x256 x8
  layers full 71.3 ms vs 16x16 dirty 0.55 ms vs cache hit 12 ns. Xvfb smoke:
  paint, drag and undo updated the canvas through the partial path.
- [x] **P6-05 — Async filter pipeline** (M) — deps: P5-59.
  Landed 2026-10-29. Canvases ≥512×512 run `:filter` and the filter panel on a
  `FilterJob` worker; the status line shows its progress, the UI keeps taking
  input, and `poll_filter` applies the result on the UI thread as one undo step
  (cancelled jobs are discarded). Neighborhood filters process 128-row chunks
  with `r` rows of context and keep only the inner rows, so async output equals
  the inline result while progress and cancel still work between chunks.
  Pointwise chunks still use the scoped parallel workers. 5 tests (274 total):
  inline small path, completion + undo, neighborhood apply, async == inline,
  and cancellation without undo. Xvfb smoke: `:new 512 512`, `:filter blur 4`
  showed 50% progress, `?` opened help while the job ran, and completion
  reported `(0 px)` on the empty canvas.
- [ ] **P6-06 — PCX / FLC / FLI import** (M) — deps: none.
  Evaluate/implement codecs for PCX and FLC/FLI (frames + durations), integrate
  with `load_any` and sequence import; add fixtures and docs. If no maintainable
  crate/decoder is available offline, mark Blocked with the exact missing
  dependency instead of silently dropping the formats.

### P6 definition of done

Every row in `docs/ASEPRITE_PARITY.md` is Covered or explicitly Out of scope; no
Partial/Not supported row remains without a checked task or a Blocked reason.
The full gate is green and each UI task has an Xvfb/headless check.

## Explicitly out of scope (for now)

- Web/Wasm build (Aseprite has no web build; product choice).
- Custom GPU shaders (a vixel extra, not Aseprite parity; P3 deferral stays).

Verification: `docs/TESTING.md` drives the real GUI under Xvfb with a window
manager and records what each pass proved.

P0-P5 are closed. **P6 is the open set**: closing the documented Partial and
Not-supported rows in `docs/ASEPRITE_PARITY.md`. The P3 sub-items keep their
re-open conditions.
