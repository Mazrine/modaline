# Progress Log

> **Resume here.** This file is the hand-off note: where the project stands, what
> was done, and the exact next step.

## Status snapshot

- `cargo test` → **147 tests green** (138 lib + 9 bin)
- `cargo clippy --all-targets` → **clean**
- `cargo build --features wgpu` → **clean**; runs headless under Xvfb
- App launches on NixOS via `steam-run` (the plain loader can't find the
  wayland/xkbcommon libs).
- Last completed batch: **P5-12 linked-cel authoring** (see batch 49).

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

## Next step (exact)

**P5 is open.** Read `docs/ASEPRITE_PARITY.md` and `docs/ROADMAP.md` P5; the
loop picks the first unchecked task whose dependencies are met.

### Next (P5)

- **P5-13** — layer flags and UUIDs (first open task), then continue down the
  P5 roadmap in dependency order.

## Known limitations (do not forget)

The matrix is the authoritative list; the entries below are the design limits
that remain after P5. Anything marked Missing/Partial in
`docs/ASEPRITE_PARITY.md` is planned work, not a permanent limitation.

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
