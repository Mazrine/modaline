# Aseprite Parity Matrix

**Source of truth for P5/P6.** One row per Aseprite capability. A row is closed
when its status is Covered (or an approved deferral) and its task is checked in
`docs/ROADMAP.md`. Update this file in the same commit that changes a
capability, and add `Refs: P5-xx`/`P6-xx` to that commit.

Legend: **Covered** = capability present and tested · **Partial** = some depth
missing · **Missing** = no equivalent · **Blocked** = task cannot proceed
(reason given) · **Out of scope** = deliberate, user-approved.

Last audited: 2026-10-29 (after P6; every capability row is Covered or explicitly deferred).
Last smoke (P5-00): 2026-10-28 — text stamp, mouse paint, dirty close guard,
save/export, recents, argv open, idle autosave recovery; drag-and-drop not
exercised. The smoke found and fixed the idle-autosave wakeup (`cc25a29`).

## Timeline and animation

| Capability | Status | Task |
|---|---|---|
| Frames: add, duplicate, delete, reorder | Covered | — |
| Per-frame duration | Covered | — |
| Tags: name, range, color, playback range | Covered | — |
| Tag direction + repeat | Covered | — |
| Tag direction 3 (ping-pong reverse) | Covered | P5-08 |
| Tag editor UI (create/edit/range/color) | Covered | P5-37 |
| Loop / ping-pong playback | Covered | — |
| Onion skin: count, opacity, tints | Covered | — |
| Timeline thumbnails | Covered | — |
| Multi-cel selection (layer×frame) | Covered | P5-34 |
| Cel copy/paste across frames/layers | Covered | P5-35 |
| Empty cel vs empty frame | Covered | P5-35 |
| Linked cels (authoring + write) | Covered | P5-12 |
| Cel z-index | Covered | P5-11 |
| Timeline layer×frame grid + per-cel cells | Covered | P5-37 |
| Preview window / fullscreen | Covered | P5-36 |

## Layers

| Capability | Status | Task |
|---|---|---|
| Normal, group, background, reference layers | Covered | — |
| Tilemap layers | Covered (retained TileRefs, editing, tile seam preview, `.ase` chunks) | P5-30/P5-31 |
| Visibility, opacity, rename, add/remove, reorder | Covered | — |
| Nested groups + group opacity/blend | Covered | — |
| Blend modes | Covered (19) | P5-05 |
| Editable lock | Covered | — |
| Lock movement | Covered | P5-13 |
| Prefer linked cels | Covered | P5-12 |
| Collapsed group | Covered | P5-13 |
| Alpha masks | Covered (Modaline extension) | — |
| Duplicate / merge down / flatten | Covered | — |
| Layer UUID | Covered | P5-13 |
| Layer user data | Covered | P5-14 |
| Cel / tag / sprite user data | Covered | P5-14 |

## Colour

| Capability | Status | Task |
|---|---|---|
| RGBA mode | Covered | — |
| Grayscale mode + conversion | Covered | — |
| Indexed mode (real indices, conversion) | Covered | P5-03 |
| Transparent palette index | Covered | P5-03 |
| RGB→indexed dithering | Covered | P5-04 |
| Palette import/export (`.gpl`, `.hex`, `.pal`) | Covered | — |
| Palette editor: insert/delete/sort/presets/names | Covered | P5-06 |
| Colour picker wheel + recent colours | Covered | P5-06 |
| sRGB/linear working profiles | Covered (Modaline) | — |
| ICC embedding (`.ase` 0x2007) | Covered | P5-09 |
| Background colour sprite property | Covered (sprite user-data colour; `:bgcolor`, side picker, `.ase`/`.vpx`) | P5-60 |
| Wide pixels rendering (pixel ratio) | Covered | P5-26 |

## Drawing and tools

| Capability | Status | Task |
|---|---|---|
| Pencil, eraser, bucket, picker | Covered | — |
| Line, rectangle, ellipse (outline/filled) | Covered | — |
| Gradient (linear/radial) | Covered (multi-stop, presets, alpha, dither) | P5-22 |
| Spray, shade, blur, contour | Covered (configurable depth) | P5-21 |
| Filters (curves, levels, balance, desaturate, posterize, median, convolve/emboss) | Covered | P5-23 |
| Pixel-perfect freehand | Covered | — |
| Symmetry (H/V/both/radial, movable centre) | Covered | — |
| **Selection clips every paint tool** | Covered | P5-02 |
| Selection stroke / fill commands | Covered | P5-25 |
| Colour-range selection | Covered | P5-25 |
| Selection from layer alpha | Covered | P5-25 |
| Selection transform handles | Covered | P5-25 |
| Custom brushes (images, spacing, library) | Covered | P5-19 |
| Brush cursor preview | Covered | P5-19 |
| Ink modes (simple, lock-alpha, replace, shading, erase) | Covered | P5-18 |
| Alpha-lock painting flag | Covered | P5-18 |
| Pressure/tablet sensitivity | Covered (manual + Wacom/OpenTabletDriver evdev pressure) | P5-20, P7-00 |
| Tiled mode | Covered (configurable repeats, opacity, preview) | P5-27 |
| Move layer/cel content tool | Covered (M move tool drags the active cel content) | P6-00 |
| Text tool (editable, fonts, sizes) | Covered (bitmap font) | P5-28 |

## Transformations

| Capability | Status | Task |
|---|---|---|
| Flip H/V, rotate 90° | Covered | — |
| Resize, crop, trim | Covered | — |
| Free rotate/scale/skew (nearest) | Covered | — |
| RotSprite rotation | Covered (supersampled mode sampling) | P5-24 |
| Interactive transform handles + pivot | Covered | P5-24 |
| Transform multiple layers/frames/cels | Covered | P5-34 |
| Canvas transforms | Covered | — |

## Tilemap

| Capability | Status | Task |
|---|---|---|
| Embedded tileset read/rasterize | Covered | P5-29 |
| Tileset model (id, name, base index, flags) + `.vpx` persistence | Covered | P5-29 |
| Tilemap layer model + tile references | Covered | P5-30 |
| Draw tiles / draw pixels in tiles | Covered | P5-31 |
| Manual / auto / stack modes | Covered | P5-33 |
| Empty tile | Covered | P5-31 |
| Tile flips X/Y/diagonal | Covered (diagonal-first order) | P5-29 |
| Write tileset/tilemap chunks (`.ase`) | Covered | P5-32 |
| Tileset editing + export | Covered | P5-33 |
| External tileset files | Covered (PNG/`.aseprite` loaded; refs written) | P5-33 |

## Slices

| Capability | Status | Task |
|---|---|---|
| Slice objects (name, bounds, color) | Covered | P5-15 |
| 9-patch / 9-slice metadata | Covered | P5-15 |
| Pivot metadata | Covered | P5-15 |
| Slice read/write in `.ase` | Covered | P5-15 |
| Export by slice + JSON metadata | Covered | P5-15 |

## Text

| Capability | Status | Task |
|---|---|---|
| Editable text objects | Covered | P5-16 |
| Fonts + sizes + alignment | Covered (bitmap; TTF not accepted) | P5-28 |
| `.ase` text properties round-trip | Covered | P5-16 |
| Bitmap text stamp (`:text`) | Covered (Modaline shortcut) | — |

## Files and formats

| Capability | Status | Task |
|---|---|---|
| `.ase`/`.aseprite` core read/write | Covered | — |
| `.ase` unknown-chunk preservation | Covered | P5-07 |
| `.ase` grid origin/size | Covered | P5-08 |
| `.ase` cel bounds / offsets | Covered | P5-10 |
| `.ase` cel z-index | Covered | P5-11 |
| `.ase` linked-cel write | Covered | P5-12 |
| `.ase` layer flags + UUIDs | Covered | P5-13 |
| `.ase` user data (text/color/properties) | Covered | P5-14 |
| `.ase` slices | Covered | P5-15 |
| `.ase` color profile / ICC | Covered | P5-09 |
| `.ase` external files chunk | Covered | P5-17 |
| Deprecated chunks (0x2006/0x2016/0x2017) | Preserved (raw) | P5-17 |
| `.vpx` project JSON | Covered | — |
| PNG import/export | Covered | — |
| Sheet PNG + meta import/export | Covered (Modaline format) | — |
| Image sequence export | Covered | P5-40 |
| Filename templates (`{frame}`, `{layer}`, `{tag}`, `{title}`, `{duration}`) | Covered | P5-40 |
| JPG / BMP / TGA / WebP | Covered (static read/write) | P5-43 |
| PCX import | Covered (24-bit and paletted; opaque, via the `pcx` crate) | P6-06 |
| FLC / FLI import | Covered (all frames + playback speed, via the `flic` crate) | P6-06 |
| Crash recovery | Covered | — |
| Reopen closed files | Covered (`:reopen`/`:closed`, clean docs only) | P6-01 |
| File browser dialog | Covered (in-app, no native dep) | P5-45 |
| Drag-and-drop open | Covered | — |
| Recent files | Covered | — |

## Export and import

| Capability | Status | Task |
|---|---|---|
| PNG frame / sprite export | Covered | — |
| Sprite sheet export | Covered | P5-41 |
| Sheet layouts + padding/trim/extrude/crop | Covered (packed = row grid for uniform frames) | P5-41 |
| JSON atlas metadata (hash/array) | Covered | P5-42 |
| GIF export | Covered (loop count, palette size, dither) | P5-42 |
| Tag / layer / frame-range filters on export | Covered | P5-39 |
| Export config (ignore empty, merge, trim, padding, toggles) | Covered | P5-39 |
| Export scale | Covered | — |
| Import sheet (own sidecar/grid) | Covered | — |
| Import Aseprite JSON/XML atlas | Covered | P5-44 |
| Import image sequence | Covered (natural filename order) | P5-44 |

## CLI

| Capability | Status | Task |
|---|---|---|
| Headless batch mode (`-b`) | Covered | P5-46 |
| `--save-as`, `--sheet`, `--data`, `--scale` | Covered | P5-46 |
| Colour mode/dithering options | Covered | P5-46 |
| Frame/tag/layer filters | Covered | P5-46 |
| `--list-*` (layers/tags/frames/slices/tilesets/palette) | Covered | P5-46 |
| Split layers/tags/slices/grid, filename templates | Covered | P5-47 |
| `--export-tileset`, `--palette`, `--list-*` | Covered | P5-46/P5-47 |

## Scripting and extensions

| Capability | Status | Task |
|---|---|---|
| Scripting engine decision | Covered (Lua/mlua; `docs/SCRIPTING.md`) | P5-48 |
| Lua scripting API | Covered (sandboxed engine; matrix in `docs/SCRIPTING.md`; Image/Cel/Selection/Dialog/`app.fs` landed in P6-02, dialogs headless) | P5-49, P5-50, P6-02 |
| Script from CLI / `:script` | Covered | P5-49 |
| `.aseprite-extension` packages | Covered (zip/manifest install/remove, scripts run sandboxed, themes listed/applied) | P5-51 |
| JSON `Op` replay (Modaline-specific automation) | Covered | — |

## Customization

| Capability | Status | Task |
|---|---|---|
| Preferences dialog | Covered (one window: editor, grid, export, appearance, keys, workspace) | P5-57 |
| Themes + font customization | Covered (5 built-ins incl. Arcane, JSON themes, bundled pixel font/UI size) | P5-53, P7 |
| Keyboard shortcut customization | Covered (actions, rebinding, conflicts, persisted) | P5-55 |
| Mouse-wheel customization | Covered (wheel-up/down actions) | P5-55 |
| Workspace layouts | Covered (4 built-ins, JSON layouts, reset, drag sizes) | P5-54 |
| Localization | Covered (runtime catalogs for every UI label, status/command message, enum value, help and window) | P5-56, P6-03 |

## Editors, workspace, undo, performance

| Capability | Status | Task |
|---|---|---|
| Multiple documents / tabs | Covered (tab strip, per-doc state, session restore) | P5-52 |
| Preview window / fullscreen | Covered | P5-36 |
| Linear undo/redo (strokes, structural, properties) | Covered | — |
| Undo history panel | Covered | P5-38 |
| Non-linear undo | Covered (jump within the linear stack; branching documented) | P5-38 |
| Compact/bounded undo memory | Covered (count limit + 256 MB byte budget; oldest trimmed) | P5-59 |
| Incremental rendering / large-canvas performance | Covered (revision/frame cache, dirty-rect texture uploads and dirty-rect compositing) | P6-04 |
| Filter threading | Covered (filters over 512×512 run on a worker with progress/cancel; UI stays responsive) | P6-05 |

## Modaline extras (not parity gaps)

These are deliberate differences, not rows to close:

- Modal vim keymap + `:` command line.
- GUI-free model crate + JSON `Op` replay API.
- Merged copy (`Y` / `:copy merged`).
- Palette lock and palette-from-sprite.
- Radial symmetry with movable centre.
- Design grid + snap.
- Per-layer alpha masks (`.vpx` only; Aseprite has no per-layer mask in `.ase`).
- Autosave / crash recovery.
- `wgpu` renderer switch.

## Blocked / deferrals

| Item | Reason | Re-open condition |
|---|---|---|
| PCX/FLC/FLI export | Import is covered (P6-06); the formats are obsolete and the import crates do not write them | A consumer requires export |
| Custom GPU shaders | Not an Aseprite capability (Modaline extra) | A shader can be verified on real hardware |
| Web/Wasm build | Aseprite has no web build; product choice | Product requirement changes |
