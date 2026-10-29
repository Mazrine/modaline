# Progress Log

> **Resume here.** This file is the hand-off note: where the project stands, what
> was done, and the exact next step.

## Status snapshot

- `cargo test` → **261 tests green** (224 lib + 31 bin + 6 CLI; 1 ignored benchmark)
- `cargo clippy --all-targets` → **clean**
- `cargo build --features wgpu` → **clean**; runs headless under Xvfb
- App launches on NixOS via `steam-run` (the plain loader can't find the
  wayland/xkbcommon libs).
- Last completed batch: **P6 backlog opened** (see batch 98).

## How to resume

```sh
cd ~/projects/vixel
cargo test
cargo clippy --all-targets
steam-run cargo run --release      # or: cargo build --release && steam-run ./target/release/vixel
```

Read, in order:
0. `docs/TESTING.md` — how to drive the GUI headlessly and what the last pass
   proved (twm is required: without a window manager, keys never arrive).
1. `AGENTS.md` — module map + invariants (must not break these).
2. `docs/CURRENT_STATE.md` — what is implemented today.
3. `docs/ROADMAP.md` — planned work, ordered by priority.

## Batch history

1. **TUI v1** (ratatui) — built, then superseded by the GUI port.
2. **GUI port** — `eframe` 0.37 / glow; canvas as a single `TextureHandle`;
   true alpha, checkerboard, zoom, grid, onion skin, playback. Model reused as-is.
3. **Editing batch** — mask selection + copy/cut/move/paste, 8 per-layer blend
   modes, layer reorder/rename, duplicate frame, flip H/V + rotate 90°, ellipse.
4. **Selection/file batch** — magic wand, generalized mask-based selection,
   `.ase`/`.aseprite` import/export (raw + zlib cels, RGBA + indexed), extension-
   aware `:w`/`:export`/`:open`.
5. **Docs** — `README.md`, `AGENTS.md`, `CLAUDE.md`, `docs/CURRENT_STATE.md`,
   `docs/ROADMAP.md`.
6. **Draw/select batch** — symmetry painting (`s`) + lasso (`L`) / polygonal
   (`P`) selection. 3 new tests. Docs updated.
7. **Clipboard + thumbnails batch** — OS clipboard (Ctrl-C/X/V, image
   copy/paste through egui), timeline thumbnails, and the Ctrl-C→copy /
   Ctrl-Q→quit / Ctrl-Z→undo remap.
8. **Shapes + selection-quality batch** — filled rect/ellipse (`t`), magic-wand
   tolerance + all-layers, and selection ops (replace/add/subtract/intersect).
   2 new tests.
9. **Toolset batch** — gradient (linear/radial, `z`), spray (`a`), shade (`q`,
   right-click lightens). 1 new test.
10. **Transforms + blur/contour batch** — `:fliph`/`:flipv`/`:rot90` now transform
    the selection's content (single cel, undoable) when a selection exists;
    blur (`n`) and contour (`C`) tools. 2 new tests.
11. **Palettes + transform-undo batch** — `.gpl`/`.hex`/`.pal` palette
    import/export (`:pal`), palette add/remove UI, and whole-sprite transforms
    are now undoable via a sprite-snapshot undo step. 3 new tests.
12. **Layer groups & masks batch** — nested groups via a flat `child_level`
    tree (Aseprite-style): group compositing with group visibility / opacity /
    blend / mask, subtree-aware add/remove/move, `:newgroup` / `:indent` /
    `:outdent`, per-layer `editable` lock (`:lock`/`:unlock`) enforced on all
    paint paths, per-layer alpha masks (`:mask add|apply|del`) that follow
    flips/rotate/resize and persist in `.vpx`. `.ase` now reads/writes layer
    groups, child levels and the editable flag. 5 new tests.
13. **Aseprite fidelity batch** — fixed chunk framing (Aseprite's chunk size
    **includes** the 6-byte header; the old writer was malformed), read linked
    cels (type 1) and compressed tilemap cels (type 3) with embedded tilesets
    (X/Y/diagonal flips), per-cel opacity (read, write, timeline slider), correct
    legacy palette chunks, and lossless depth selection on export (grayscale
    16-bit, indexed 8-bit with palette chunk, else RGBA). Real Aseprite-saved
    fixtures added under `tests/fixtures/ase/`. 7 new tests.
14. **Tile mode batch** — `:tile on|off` (or the side-panel checkbox) previews
    the sprite repeated 3x3 with the center tile outlined; the cursor wraps at
    the edges, so edge strokes stay seamless across the tile boundary. Clicks
    in the neighbouring copies fold back into the sprite. 1 new test.
15. **Frame tags & reorder batch** — named inclusive frame ranges (`:tag
    <name> [from to]`, `:tagdel`), shown as colored chips above the timeline;
    clicking one restricts playback to its range. Tags round-trip in `.vpx`
    and `.ase` (Tags chunk: name/range/color) and shift with frame
    insert/remove. Frames reorder by dragging a thumbnail or `:framemove`.
    4 new tests.
16. **P1-P3 batch** — onion-skin settings (count/opacity/tints) and loop +
    ping-pong playback; layer drag-to-reorder; bitmap-font text tool; palette
    hex/HSV entry and swatch reordering; radial symmetry with a movable centre;
    design grid + snap; selection shape ops (expand/contract/border/feather/
    invert); eight filters; free rotate/scale/skew; sprite properties (colour
    mode, pixel ratio, grid) with .ase round trips for the depth and the ratio;
    slice export; sRGB/linear profile conversion; config persistence; wgpu
    behind a feature flag. Deferred with reasons in `docs/ROADMAP.md`: ICC
    embedding, custom shaders, keymap customization. **+35 tests.**
17. **Backlog reconciliation** — audited the code against the docs and filed the
    open set as `docs/ROADMAP.md` P4: close-without-save and the missing dirty
    guards (`:open` / `:new` / `:import`), `:wq *.ase` writing JSON, no autosave,
    structural undo, every "known limitation", and the untracked Aseprite parity
    gaps (brush size, layer merge/flatten, crop, contiguous toggle, sheet import,
    export scale, file dialog). Fixed doc drift: README's "colour profiles not
    implemented", its command/tool tables, AGENTS' module map, the dead `VI-2`
    reference, and this file's stale P1 next step. Source comments only; no
    behaviour change, 73 tests unchanged.
18. **Data-loss guards** — closing the window no longer discards a dirty sprite:
    `app.rs` cancels the close (`CancelClose`) and says so; `:open` / `:new` /
    `:import` refuse unsaved work unless forced with `!` (`:open!` etc.); `:wq`
    picks the format by extension through the shared `save_to` (a `.ase` name
    used to receive the `.vpx` JSON) and only quits after a successful save,
    reporting a failed one instead of exiting. 2 new tests (75 total).
19. **Autosave / crash recovery** — a dirty sprite is written to
    `$XDG_CONFIG_HOME/vixel/recovery.vpx` every 30 s; a successful save or a
    forced quit removes it, so a clean session never resurrects a stale
    snapshot. `:recover [file]` loads it (dirty, with no project path) so `:w`
    names the destination, and startup offers it in the status line instead of
    loading silently. 1 new test (76 total).
20. **Structural undo** — layer add/delete/move/indent/rename/lock, frame
    add/delete/duplicate/move and tag create/delete are one `Structure` undo
    step each; the snapshot carries the active layer/frame/tag on both sides,
    so undo/redo never leaves an index past the end of a shorter list. Refused
    edits (last layer/frame, same name, no-op move) record nothing. `:lock` now
    goes through `set_layer_editable`, so locking is undoable too. 2 new tests
    (78 total).
21. **Panel property undo** — layer visibility, lock, opacity and blend mode
    record a small `LayerProps` step (before/after values, no sprite clone), so
    toggling from the panel is undoable; an opacity drag is one step from
    drag-start to release. `:lock` now uses the same step instead of a full
    sprite snapshot. 1 new test (79 total).
22. **Symmetry reaches fill and paste** — a bucket fill now floods the seed and
    every mirrored/radial seed as one undo stroke, and a committed paste writes
    each pixel at its symmetry points, so the pasted block is mirrored too.
    2 new tests (81 total).
23. **Tag direction and repeat** — each tag has a `PlayDirection`
    (forward/reverse/ping-pong) and a repeat count (0 = forever), set with
    `:tagdir` / `:tagrepeat`; playback honours them inside the tag while the
    global loop/ping-pong flags still drive untagged playback. Both fields
    round-trip in `.vpx` and in the `.ase` Tags chunk (direction byte, repeat
    u16). Playing restarts the pass counter. 1 new test (82 total).
24. **Timeline thumbnail caching** — `Editor::revision` bumps on every edit
    (`record`, undo/redo, structural and property steps, snapshots, resize,
    sprite replacement); the timeline compares it and rebuilds only stale
    thumbnails instead of every frame. The cel-opacity slider bumps it too.
    1 new test (83 total).
25. **Tile-mode seams** — in tile mode, line/rect/ellipse and gradient take the
    shorter way round the canvas: the shape is generated past the seam and each
    pixel is wrapped, so a line from the right edge continues on the left
    instead of crossing the whole sprite. 2 new tests (85 total).
26. **Merged copy** — `Y` (normal or visual mode) and `:copy merged` flatten the
    visible layers of the active frame inside the selection into the clipboard;
    `y` / `:copy layer` keep the active-layer behaviour. Cut and move stay on
    the active layer, matching Aseprite. 1 new test (86 total).
27. **.ase mask check + brush size** — checked the official `.ase` spec: there
    is no per-layer alpha mask (0x2016 is a deprecated, sprite-level 1-bit
    mask), so masks stay `.vpx`-only by format. That item is closed rather than
    deferred. Added a square brush for pencil/eraser (1-64 px, `:brush` and a
    side-panel slider), clipped at the canvas edges and mirrored by symmetry;
    shapes, paste and text keep their own sizing. 1 new test (87 total).
28. **Crop / trim** — `:crop [x y w h]` (no args = the selection's bbox) and
    `:trim` (the non-transparent bbox across every visible frame) crop every
    cel and layer mask, clamp the cursor, clear the selection, and undo as one
    snapshot. 1 new test (88 total).
29. **Layer duplicate / merge / flatten** — `:duplayer` clones a normal layer
    (with its cels) above itself; `:merge` composites it into the layer below
    through the same `composite_range` the canvas uses, bakes opacity/blend/mask
    and removes it; `:flatten` replaces the visible layers with one per-frame
    composite. All undoable as one structure step, with panel buttons. Groups
    are refused for duplicate/merge (flatten includes them). 1 new test
    (89 total).
30. **Contiguous toggle + fill tolerance** — `:contig on|off` (panel checkbox)
    switches the wand and bucket between the seed's contiguous region and every
    matching pixel; the shared `:wand <0-255>` tolerance now drives the bucket
    too (`flood_fill_tol` with a `FillOptions`). 2 new tests (91 total).
31. **Export scale** — `:export <target> [scale 1-16]` upscales PNG, the sheet
    and the GIF with nearest-neighbour; the sheet meta reports the scaled frame
    size so it stays sliceable. `.ase` export refuses a scale. 1 new test
    (92 total).
32. **Sheet import** — `:importsheet <sheet.png> [cols rows]` reads the sidecar
    `:export` writes (`<stem>.json`) or an explicit grid, validates the image
    against the meta, and rebuilds the frames with their durations; a dirty
    sprite is protected like `:import`. Closes the G6 round trip. 1 new test
    (93 total).
33. **Reference / background layer kinds** — `Layer` gains Aseprite's
    `reference` and `background` flags. Reference layers draw on canvas but are
    refused by every paint path and skipped by `export_composite`, so PNG/sheet/
    GIF/flatten leave them out; background layers are forced opaque. Both flags
    round-trip in `.vpx` and `.ase` (bits 64 and 8), are undoable `LayerProps`,
    and have `ref` / `bg` chips in the panel. 2 new tests (95 total).
34. **Pixel-perfect freehand** — `:perfect on|off` (panel checkbox) routes the
    1 px pencil/eraser through `paint_freehand`: it joins pointer steps with a
    line and drops the middle pixel of an L-shaped corner, restoring that
    pixel's `before` and removing its stroke record. Brushes and symmetry fall
    back to a plain stamp. 1 new test (96 total).
35. **Drag-and-drop + recent files** — dropping a file on the window opens it
    through `open_path`, with the dirty guard (a drop never discards a stroke).
    `Editor::recent` keeps the last 10 opened/saved files, deduped and newest
    first; it persists in `config.json` and drives `:recent` /
    `:recent <n>`. A native file dialog was deliberately not added. 1 new test
    (97 total).
36. **Backlog close-out** — tileset/tilemap editing was the last P4 item. A
    faithful editor needs a tileset/tile model, a tile picker, tile-aware
    paint/undo and tileset chunk writing; the reader already parses tilemaps
    and embedded tilesets but there is no fixture or consumer for editing. It
    moved to `docs/ROADMAP.md` → "Explicitly out of scope" with that plan and
    the re-open condition. P4 is otherwise empty. No code changed; 97 tests.
37. **Parity programme opened** — the capability review found vixel at roughly
    60% of Aseprite's feature surface (~80–85% of the core hand-drawn loop). P5
    (full capability parity) is now in `docs/ROADMAP.md`; the living matrix is
    `docs/ASEPRITE_PARITY.md`; the detailed rationale/architecture is in
    `docs/ROADMAP.md` P5 and the plan file. No capability changed yet; the loop
    starts at P5-00 (review + merge P4, create `feat/aseprite-parity`).
38. **P5-00 — P4 review + merge** — reviewed the five P4/backlog commits and ran
    the full gate: 97 tests, clippy, fmt, wgpu build. The Xvfb/twm smoke pass
    (text stamp, mouse paint, dirty close guard, save/export, recents, argv
    open, autosave recovery) found one real bug: eframe sleeps while idle, so
    the 30 s autosave only ran when input arrived; fixed with
    `request_repaint_after` (`cc25a29`), then re-verified that recovery writes
    while idle and `:q!` clears it. Fast-forward merged
    `fix/data-loss-guards` into `main` and created `feat/aseprite-parity`.
    Drag-and-drop was not exercised (xdotool cannot XDND).
39. **P5-02 — Selection clips paint** — `Editor::paint_pixel` now enforces
    bounds and the current selection, with `paint_pixel_unclipped` as the
    explicit exemption used by paste, selection transforms and contour.
    `FillOptions` gained a clip mask, so bucket fills (contiguous and global)
    stop at the selection edge and a seed outside it does nothing. 7 new tests
    (104 total).
40. **P5-07 — `.ase` unknown chunks** — `Sprite.raw_chunks` + `ChunkAnchor`
    retain every unparsed chunk with its frame and the object it followed;
    `save` re-emits each group after the same layer/cel/palette/tags anchor and
    falls back to the frame end if that object was not written. Frame headers
    now carry real chunk counts. 3 new tests (107 total), including
    open→save→open on all four real fixtures; composites, chunk bodies and
    frame/kind survive (group order may differ).
41. **P5-05 — all 19 blend modes** — added dodge, burn, hard/soft light,
    difference, exclusion, hue, saturation, color, luminosity and divide;
    `blend_fn` takes the RGB triple so the W3C HSL modes work, and divide uses
    Aseprite's `backdrop / source` clamp. `.ase` words 6-15/18 map both ways;
    the layer combo has 19 entries. 2 new tests (109 total): an independent
    reference table at α=255 and α=128 for every mode, plus a save→load round
    trip of every word. UI not screenshotted; the combo is generated from
    `ALL_BLENDS` (manual check: count the 19 entries).
42. **P5-03 — real indexed colour** — `Sprite.palette` and
    `transparent_index`; `:mode indexed` quantizes (exact ≤255, median cut
    above) with a transparent slot as one undo step and snaps paint/fill to
    entries. `.ase` writes/reads header byte 28 and the palette; undeclared
    slots pad opaque black and indices match by RGBA. `.vpx` keeps the palette;
    opening an indexed file syncs the picker. 6 new tests (115 total), including
    a fixture edit→save→open that asserts the exact cel index bytes and
    transparency.
43. **P5-04 — dithering engine** — `DitherMode` (none, Bayer 2/4/8,
    Floyd–Steinberg), `Sprite::dither_to_palette` and `:dither`; `:mode indexed`
    applies the current mode. Bayer matrices are recursive; FS diffuses
    7/16/3/16/5/16/1/16. 5 new tests (120 total): exact 2×2 Bayer pattern, an
    FS `B B W W` ramp, determinism across all four modes, editor conversion +
    undo/palette sync, and dithered `.ase` indices surviving save→load.
    Gradients reuse the same engine in P5-22.
44. **P5-06 — palette editor and picker depth** — presets (default, gray,
    pico8, dawnbringer16), `:pal insert/delete/sort/name`, parallel colour and
    name vectors mirrored into the document when indexed, and `.ase` UTF-16
    palette names (flag bit 1) + `.vpx` persistence. Picker gained a hue wheel
    and the last 12 recent colours; the palette panel has a name field and
    preset combo. 5 new tests (125 total). UI not screenshotted; manual check:
    drag the wheel, type a name, click a recent swatch.
45. **P5-08 — header and small-chunk fidelity** — `Sprite.grid_origin`
    (serde default so old `.vpx` files migrate), `.ase` header SHORT origin +
    WORD size, `:sgrid [x y] <w> <h>` and panel origin drags. Added
    `PlayDirection::PingPongReverse` for direction byte 3; playback starts at
    the tag's last frame and bounces. 5 new tests (130 total): header bytes and
    round trip, `.vpx` origin + missing-field default, direction byte mapping,
    direction-3 tags chunk + round trip, reverse ping-pong playback.
46. **P5-09 — ICC colour-profile embedding** — `AseProfile { kind, flags,
    gamma, icc }` parses 0x2007 type/gamma/ICC and writes it in frame 0; an
    imported profile wins, else saving writes sRGB or linear (fixed gamma 1.0).
    `:profile` shows it; `:profile icc <file>|gamma <g>|clear` authors it;
    `.vpx` stores it. 4 new tests (134 total), including `prop.ase`'s 536-byte
    ICC surviving open→save→open. ICC pixels are read as-is in sRGB.
47. **P5-10 — cel bounds / offset cels** — `Cel` gained `x/y/w/h` and stores
    cel-sized pixels. `.ase` import/export preserves offsets and dimensions;
    composite, get/set, flips, rotate, masks, dithering, merge and flatten
    respect bounds, while painting outside grows a cel. Crop/resize
    canonicalize; old `.vpx` cels normalize on load. 5 new tests (139 total):
    clipping/growth, transform bounds, `.ase` offset round trip, `.vpx`
    migration, and offset-cel paint/undo.
48. **P5-11 — cel z-index** — `Cel.z_index` (serde default 0); `.ase` reads and
    writes the SHORT z field, composite orders siblings by `-layerIndex + z`
    (ties by z), and `:celz <n>` / the timeline z drag set it. `UndoStep::CelZ`
    makes it undoable. 3 new tests (142 total): composite ordering, `.ase` z
    bytes + round trip, undo/redo.
49. **P5-12 — linked-cel authoring** — `Cel.link` with copy-on-write on paint;
    `.ase` reads/writes cel type 1; `Layer.prefer_linked` is flag bit 16.
    `:link <frame>`, `:unlink`, `:preferlink` and a timeline toggle author
    links; frame insert/duplicate/remove/move remap or materialise links, and
    global operations materialise first. 5 new tests (147 total): sharing/COW,
    prefer-linked frames, frame-op remapping, type-1 round trip, link undo.
50. **P5-13 — layer flags and UUIDs** — `Layer.lock_movement` (flag 4),
    `collapsed` (flag 32) and `uuid` (16 bytes); `.ase` reads/writes UUIDs and
    header bit 4, fixing the tileset/UUID order. New layers/duplicates mint
    UUIDs when the sprite uses them; cut/move/free-transform refuse locked
    layers. `:movelock`, `:collapse`, `:layeruuid` and panel `mv`/`col`
    toggles; collapsed groups hide children. 4 new tests (151 total): flag and
    UUID bytes/round trip, fixture UUIDs, movement lock, UUID minting.
51. **P5-14 — user data 0x2020** — `UserData { text, color, properties,
    flags }` on layers, cels, tags and the sprite; properties are retained
    verbatim. The reader applies the “attaches to the previous object” rules
    (layer, cel, one per tag, sprite after the palette) and the writer re-emits
    them in place; `.vpx` stores the data. 3 new tests (154 total): attachment
    and round trip, property payload + sprite data, `prop.ase`
    open→save→open. Tileset/slice user data stays raw until P5-15/31.
52. **P5-15 — slices 0x2022** — `Slice`/`SliceKey` model (name, bounds,
    per-key 9-patch centre, pivot, frame ranges, user-data colour); 0x2022
    read/write with user-data attachment. `:slices` lists, `:slices <dir>
    named` exports per-slice PNGs + `<stem>.slices.json`, and the side panel
    lists them. 3 new tests (157 total): synthetic chunk bytes/round trip,
    fixture slice round trip, named PNG+JSON export.
53. **P5-16 — text object model** — typed property-map codec for every
    Aseprite value type; `TextObject { text, font, size, align, antialias, x,
    y }` stored in cel user-data map key 0 (colour in the user-data colour),
    `:textobj` creates/edits and rasterises the bitmap font as one undo step.
    Empty cels with user data now write a cel chunk so text objects survive.
    3 new tests (160 total): property codec, `.ase` text object, editor
    rasterise/update/undo. Aseprite main still rasterises text; this is the
    forward-compatible user-data layout.
54. **P5-17 — external files + deprecated chunks** — `ExternalFile { id, kind,
    name }` parsed/written from 0x2008; tileset flag 1 records external
    file/tileset IDs and skips external pixels without treating them as
    corruption. Deprecated 0x2006/0x2016/0x2017 stay raw. 3 new tests (163
    total): external chunk round trip, external tileset flag, deprecated chunk
    preservation. External pixel resolution remains P5-33.
55. **P5-18 — ink engine** — `Ink` (simple, lock-alpha, replace, shading,
    erase) with one remembered per tool and an alpha-lock flag. Brush paths go
    through `apply_ink`; cut/transform/paste/contour/filters use an exact put
    path. Indexed dabs still snap via `Sprite::set`. 2 new tests (165 total):
    ink math and per-tool/alpha-lock behaviour. `:ink`/`:alphalock` and a panel
    selector.
56. **P5-19 — custom brushes** — `Brush` coverage masks with square/circle
    built-ins and PNG customs; percentage spacing; the `vixel/brushes` library
    (`:brush load|save|use|list`); freehand, shapes and symmetry stamp the
    mask; cursor previews its coverage. 4 new tests (169 total): builtins, PNG
    round trip, custom-mask limits, circle/shape/spacing.
57. **P5-20 — pressure/tablet** — touch/pen `force` is captured and touch paint
    is mapped to sprite pixels; pressure scales brush coverage. The manual
    slider and `:pressure <0-100>|auto` cover mice (no force on X11) and override
    the device until `auto`. 1 new test (170 total): pressure scaling and
    manual/device precedence.
58. **P5-21 — tool depth/consistency** — shade/blur/spray stamp the active
    brush mask; spray rate/radius, shade strength, blur radius through
    `box_blur`, contour width + inside/outside. Commands and a "tool depth"
    panel group. 4 new tests (174 total): spray bounds, shade strength/coverage,
    blur sampling, contour width/side.
59. **P5-22 — gradient depth** — multi-stop gradients with alpha, seven presets,
    Bayer 2/4/8 dithering, selection clipping; `:grad preset|stop|stops|clear|
    dither` and toolbar combos. 3 new tests (177 total): interpolation/presets,
    dither level reduction, selection + alpha.
60. **P5-23 — filters breadth** — curves, levels, colour balance, desaturate,
    posterize, median, 3×3 convolution (custom kernel/divisor/bias) and emboss;
    selection-clipped and one undo step. 4 new tests (181 total): colour remaps,
    median/convolution/emboss, selection + undo, parser coverage.
61. **P5-24 — transform depth** — RotSprite-style supersampling, custom pivot,
    corner/edge/rotate handles with inside-drag move, applied on release.
    `:rotsprite`/`:pivot`; "handles" panel toggle. 2 new tests (183 total):
    RotSprite vs nearest + pivot rotation, handle hit-testing/pivot mapping.
62. **P5-25 — selection extras** — fill/stroke commands, colour-range and
    layer-alpha selection, and mask-only rotate/scale/skew/offset (plus "sel
    only" handles). 2 new tests (185 total): selection sources + fill/stroke,
    mask transform/offset.
63. **P5-26 — wide pixels** — every canvas overlay/hit-test/fit/zoom uses the
    aspect-aware `View::cell()`; "wide px" toggles preview. 1 new test (186
    total): 2:1 cell sizing, hit-testing and fit.
64. **P5-27 — tiled options** — configurable X/Y repeats, neighbour opacity and
    preview toggle; hit extent follows the grid, seam wrapping unchanged. 1 new
    test (187 total): offset counts and clamping.
65. **P5-28 — text tool** — `Tool::Text` placement, multi-line live preview with
    one-undo commit, bitmap font/size/alignment/colour, `:textfont|size|align`.
    2 new tests (189 total): preview undo grouping and placement/settings.
66. **P5-29 — tileset model** — retained `Sprite.tilesets`/`Layer.tileset`,
    ID-based lookup, base-index tile mapping, diagonal-first flips, `.vpx`
    persistence. 3 new tests (191 total): model/ID/base/flip render, flip order,
    and `.vpx` round-trip.
67. **P5-30 — tilemap model** — `Layer.tilemap` plus per-cel `TilemapCel`/
    `TileRef` (id + flips) kept alongside the render cache; `render_tilemap`
    and `Sprite::rasterize_tilemap` rebuild pixels/bounds; `.ase` retains refs,
    `.vpx` round-trips them. 2 new tests (192 total): retained refs/flag and
    re-rasterization.
68. **P5-31 — tilemap editing** — `Tool::Tile` paint/pick/empty with flips,
  per-edit rasterize + one undo, pixel-edit mode, `:tileid|flip|mode|pick|erase`
  and a tilemap panel. 2 new tests (194 total): paint/pick/erase/undo and pixel
  mode.
69. **P5-32 — write tileset/tilemap chunks** — `0x2023` embedded/external
  tilesets, tilemap layer kind 2 + tileset ID, type-3 cels with compressed tile
  words and flips; shared pixel encoder. 2 new tests (195 total): fixture
  round-trip and external reference write.
70. **P5-33 — tileset depth** — atlas pixel editing with flip mapping and
  all-cel rerasterization; `--export-tileset` + `:exporttileset`; external
  PNG/`.aseprite` tileset resolution; manual/auto/stack/pixel modes. 3 new tests
  (198 total): atlas edit + modes, external PNG, atlas export.
71. **P5-34 — multi-cel selection** — layer×frame set + active cel, Ctrl/Shift
  timeline modifiers, paint/exact-write fan-out, per-cel transform loop with one
  undo, `:exportcels` PNG fan-out, `:celselect|:celclear`. 2 new tests (200
  total): fan-out and range selection.
72. **P5-35 — cel operations** — whole-cel clipboard with link-resolving
  copy/cut/paste, `:celdel` empty-cel vs `:delframe`, `:celinfo`, and timeline
  copy/cut/paste/empty buttons beside z/link controls. 1 new test (201 total):
  copy/paste/cut and empty-cel-vs-frame.
73. **P5-36 — preview window** — floating preview with independent frame clock,
  play/pause/loop/zoom/scrubber, optional OS viewport and fullscreen. 1 new test
  (202 total): independent playback/loop/stop.
74. **P5-37 — timeline UX** — layer×frame grid with per-cel thumbnails, drag
  rectangle selection, compact onion controls, and a tag editor
  (name/range/colour/create/delete). 2 new tests (204 total): rectangle+tag
  mutations and thumbnail sizing.
75. **P5-38 — undo history** — labelled history window with click/jump to any
  undo/redo position; linear stack retained and branching rationale documented.
  1 new test (205 total): labels and jump/redo navigation.
76. **P5-39 — export config core** — ExportConfig with frame/tag/layer filters,
  ignore-empty, merge dup, scale, trim, padding, sheet/GIF/meta toggles;
  configured sprite derivation, padded sheets, `:exportcfg` and export dialog.
  2 new tests (207 total): derivation and bundle output.
77. **P5-40 — PNG sequence** — per-frame/per-layer sequence export with scale,
  token templates, sanitizing, dedupe and `.png` enforcement. 2 new tests (209
  total): token rendering and 2x2 sequence output.
78. **P5-41 — sheet layouts** — horizontal/vertical/rows/columns/packed grids,
  border/shape/inner padding, extrusion, trim via config; metadata records the
  layout. 2 new tests (211 total): padding/extrude placement and grid choices.
79. **P5-42 — atlas + GIF options** — JSON-hash/array metadata with tags/layers/
  slices and templated filenames; GIF repeat count, palette cube and dither.
  2 new tests (213 total): atlas metadata and GIF quantization/output.
80. **P5-43 — extra image formats** — JPEG/BMP/TGA/WebP export and import
  through extension dispatch; JPEG alpha flattened, FLC/FLI/PCX documented as
  unsupported. 1 new test (214 total): export/scale/classify/reload.
81. **P5-44 — import parity** — JSON hash/array + XML atlas import with
  durations/tags/slices, auto-detected by `:importsheet`; natural-order image
  sequence import; `:importatlas`/`:importseq`. 3 new tests (217 total).
82. **P5-45 — file browser** — in-app files window with filtering, navigation,
  recents and persistent favourites; Ctrl/Cmd+O and Ctrl/Cmd+Shift+S; save
  dispatch via `save_path`. 2 new tests (219 total): listing/filter and save
  formats.
83. **P5-46 — CLI batch core** — headless `-b` with save-as/sheet/data/gif,
  scale, colour mode/dither, frame/tag/layer filters, trim/crop, layout/atlas/GIF
  options and list-* output. 3 integration tests (222 total).
84. **P5-47 — CLI batch advanced** — split layers/tags/slices/grid, filename
  templates, ignore/merge, tileset and palette export. 2 integration tests (224
  total).
85. **P5-48 — scripting decision** — `docs/SCRIPTING.md` compares Lua/mlua,
  Rhai, QuickJS/WASM, Python and JSON Op; chose Lua/mlua 0.10 for Aseprite
  compatibility, with sandbox/limits/API plan. Offline check found no mlua
  source/cache, so P5-49/P5-50/P5-51 are Blocked (dependency, not design).
86. **P5-52 — multi-document tabs** — `Doc` vector with per-doc editor/view/
  textures/playback, tab strip, Ctrl-Tab/Shift-Tab/T/W, dirty close guards,
  session restore (paths + active). 4 unit tests (228 total); Xvfb smoke showed
  two tabs and tab switching. Fixed per-event chord modifiers.
87. **P5-53 — themes and UI font** — JSON theme format with `dark`, `light`,
  `midnight`, `paper`; `:theme` apply/list/save, per-tab restore; `:uifont`
  loads a TTF/OTF and size, persisted in config. 2 tests (230 total); Xvfb
  smoke verified the midnight palette, Noto at 20 pt, and theme export.
  Fixed a default/builtin recursion that overflowed the stack.
88. **P5-54 — workspace layouts** — JSON layouts with `default`, `drawing`,
  `animation`, `compact`; `:layout` apply/reset/save; per-tab + config persist,
  dragged sizes. Panels fill their allocation and loaded layouts seed
  `PanelState`. 3 tests (233 total); Xvfb smoke verified animation/reset/export.
89. **P5-55 — keymap customization** — 28 actions with default chords,
  `:bind` rebind/steal/unbind/list/reset, wheel-up/down actions, default
  suppression, global across tabs and persisted. 5 tests (238 total); Xvfb
  verified rebinding, blocking, help and wheel zoom.
90. **P5-56 — localization** — JSON catalogs (`en`, `pt-BR`), English fallback,
  `:lang` switch/list/check/save, global across tabs and persisted; localized
  core labels. 4 tests (242 total); Xvfb smoke verified pt-BR labels and export.
  Status/help strings remain English (documented).
91. **P5-57 — preferences dialog** — `:prefs` window with editor, grid/canvas,
  export, appearance, keys and workspace sections using live config fields.
  Xvfb smoke verified the dialog and sections; 242 tests unchanged (UI-only).
92. **P5-58 — render cache / dirty rects** — canvas texture skipped when
  revision/frame/size unchanged; onion cache keyed by revision/frame/count;
  pixel dirty rectangles patch via `set_partial`, structural edits force full.
  Composite still full-frame when dirty. 3 tests (245 total) + debug benchmark;
  Xvfb paint/undo smoke.
93. **P5-59 — undo memory + filter threading** — undo count limit (default 100,
  prefs/config) + 256 MB byte budget trimming oldest; pointwise filters run on
  scoped workers (≥64 rows), neighborhood filters synchronous. 2 tests (247
  total). Remaining gap: async/off-UI-thread filtering (documented Partial).
94. **P5-60 — sprite background colour** — sprite user-data colour with undo,
  `:bgcolor`, side picker/swatch, `.ase`/`.vpx` round-trip. 3 tests (250 total);
  Xvfb verified the swatch/status.
95. **P5-49 — scripting MVP** — `mlua` 0.12 (Lua 5.4 vendored), sandboxed
  execution, core `app`/sprite/layer/pixel API, one-step script undo,
  `:script`/`:lua`/`--script`. 5 unit + 1 CLI test (256 total).
96. **P5-50 — scripting API coverage** — pixelColor/Color, transaction,
  command Undo/Redo/NewLayer/NewFrame, flatten/crop/newTag/durations; full
  compatibility matrix in `docs/SCRIPTING.md`. 2 tests (258 total).
97. **P5-51 — extensions** — zip/manifest install/list/remove with traversal and
  size limits; `:ext` commands; sandboxed contributed scripts; theme listing/
  application. 3 tests (261 total); Xvfb installed, listed and ran `demo`.
98. **P6 backlog opened** — added seven tasks (P6-00…P6-06) for the documented
  Partial/Not-supported matrix rows: layer-content move, reopen closed docs,
  Lua object API/fs, full string extraction, dirty-rect composition, async
  filtering, PCX/FLC/FLI codecs. Docs-only; 261 tests unchanged.
99. **P6-00 — move layer/cel content tool** — new `Tool::Move` (`M`, toolbar)
  detaches the active cel into a floating preview and bakes it back clipped to
  the canvas as one undo step; Esc cancels, tool switch commits, locked/hidden/
  reference layers refuse. 2 tests (263 total). Xvfb smoke: preview
  followed the drag, commit moved the block, `u` restored it, Esc cancelled.
100. **P6-01 — reopen closed documents** — bounded deduplicated stack of clean,
  named closed tabs (persisted in `config.closed`); `:closed` lists and
  `:reopen` restores the newest in a new tab. Dirty forced closes are never
  recorded. 2 tests (265 total). Xvfb smoke: `Ctrl-W` then `:closed`/`:reopen`
  restored `b.png` as the active tab.
101. **P6-02 — Lua object API** — `Image` tables (getPixel/setPixel/clear/
  bounds), `sprite:cel` + `sprite:setCelImage`, `app.selection`
  (bounds/isEmpty/add/subtract/clear), headless `Dialog` + `app.confirm`/
  `app.prompt`, and `app.fs.read/write/exists` sandboxed to `:scriptroot` or the
  open file's folder (absolute/`..`/symlink escapes denied). 3 tests (268
  total). Xvfb smoke: root, fs write, cel image, selection and a denial all
  visible in the status line.
102. **P6-04 — dirty-rect compositing** — `Sprite::composite_rect` shares the
  full compositor's core; partial canvas uploads now composite only the dirty
  rect instead of the whole frame. 2 tests (269 total); debug benchmark:
  256×256×8 full 71.3 ms vs 16×16 dirty 0.55 ms. Xvfb paint/drag/undo smoke.
103. **P6-05 — async filters** — `:filter` on canvases ≥512×512 runs in a
  `FilterJob` worker; status shows progress, `poll_filter` applies one undo
  step on the UI thread, and cancelled jobs are dropped. Neighborhood filters
  chunk with radius context so async == inline. 5 tests (274 total). Xvfb:
  512² `blur 4` showed 50% while the help overlay opened; completion reported.
104. **P6-06 — PCX / FLC / FLI import** — added `pcx 0.2` and `flic 0.1`
  (pure Rust); `load_any` opens `.pcx` (paletted → opaque RGBA) and
  `.flc`/`.fli` (all frames + speed), sequence import accepts both, and saving
  to a legacy extension is refused. 4 tests (278 total) + checked-in fixtures
  (`sample.pcx`, `sample.flc`). CLI `-b sample.flc --save-as` round-trips to
  2 frames at 120 ms.
105. **P6-03 — localization extraction** — `Editor::msg` is a `Status` (English
  text + `format!` template); `status!` records the template and the status line
  re-renders translated values in place, with a value dictionary for enum names.
  427 message templates + help overlay + window titles + preferences + side
  panel/timeline chrome + tool labels translated to pt-BR; tests scan
  `status!`/`.msg`/`.tr()` usage and check every key/message/value (3 tests, 286
  total). Xvfb: `:lang pt-BR` gave `tinta: simples`, `grade: 12 px`, Portuguese
  tools/panels/prefs; `:lang en` switched back cleanly.
106. **P7-00 + UX pass — pen, keyboard parity, tips** — Linux evdev pressure
  for Wacom/OpenTabletDriver (never grabs, rescans, `:pen`, side row, config);
  Ctrl-hjkl pan; `{`/`}` brush size; `:celopacity`, `:layeropacity`,
  `:frameduration`, `:visible`, `:blend`, `:onion`; Esc closes floating
  windows; contextual status hints, toolbar key tooltips and `:tips` with a
  vim-vs-Helix note. 6 new tests (292 total). Xvfb: hint line, pan, tips, pen
  status and cel opacity checked. Docs: `PEN.md`, `UX_AUDIT.md`, `KEYMAPS.md`,
  ROADMAP P7. Hardware pressure path is untested in CI.

## Next step (exact)

**P6 is complete (7/7); P7 is open (1/5).** P7-00 (pen pressure) landed with
the input/UX pass. The next loop picks the first unchecked P7 task whose deps
are met: **P7-01 Helix keymap preset** (plan in `docs/KEYMAPS.md`), then
P7-02 leader overlay, P7-03 pen tilt/ExpressKeys, P7-04 UX follow-ups from
`docs/UX_AUDIT.md`.

### P7 landed (2026-10-29)

- **P7-00** — Wacom / OpenTabletDriver pen pressure (Linux evdev).
- Keyboard/pen parity pass: pan, brush size, opacity/blend/visibility/duration
  commands, Esc-closes-windows.
- Contextual status hints, tool tooltips, `:tips`; UX audit and Helix plan.

### P6 landed (2026-10-29)

- **P6-00** — move layer/cel content tool.
- **P6-01** — reopen closed documents.
- **P6-02** — Lua Image/Cel/Selection/Dialog/`app.fs` objects.
- **P6-03** — localization string extraction.
- **P6-04** — incremental dirty compositing.
- **P6-05** — async filter pipeline.
- **P6-06** — PCX/FLC/FLI import.

## Known limitations (do not forget)

The matrix is the authoritative list; the entries below are the design limits
that remain after P7. Anything marked Missing/Partial in
`docs/ASEPRITE_PARITY.md` is planned work, not a permanent limitation.

- Pen pressure is read from Linux evdev only (Wacom/OTD); tilt, rotation and
  ExpressKeys are P7-03, and real hardware is not exercised in CI.
- Mask pixels cannot be painted directly: masks are created from the selection
  (or fully opaque), baked, or removed.
- New layers are added as siblings; nesting is an explicit `:indent`.
- New groups start empty and do not adopt the active layer.
- Groups cannot be duplicated or merged down (flatten includes them).
- `.ase` has no per-layer alpha mask (0x2016 is deprecated and sprite-level);
  masks stay `.vpx`-only by format.
- Tilemap layers are rasterized on `.ase` import; tilesets are not kept (tile
  editing is out of scope, see ROADMAP).
- `.ase` import ignores external tileset files (flag 1) and cel z-index.
- `.ase` files exported by vixel before this batch have non-conformant chunk
  sizes and may not load; re-export them with the current build.
- Lasso/polygon are mouse-driven; keyboard selection is marquee (`v`) + magic
  wand (`w`).
- Cut and move act on the active layer; `Y` copies the flattened selection.
- OS paste prefers clipboard *text* when present (egui behavior); image paste is
  used when the clipboard has no usable text.
- Rendering is CPU-composited; no custom shaders (kept backend-agnostic on
  purpose for the future glow→wgpu switch).

## Not yet exercised by the headless pass

The 2026-10-26 pass (`docs/TESTING.md`) covers ten paths; these are still
unexercised: draw/erase, `v` marquee → `m` move, `w` wand, `L` lasso drag,
`P` polygon + `Enter`, `s` symmetry painting, `z` gradient, `a` spray,
`q` shade, `n` blur, `C` contour, `:fliph` with a selection, `Ctrl-c` in another
app → `Ctrl-v` here, `:w x.ase` then reopen.
