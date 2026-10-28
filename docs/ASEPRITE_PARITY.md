# Aseprite Parity Matrix

**Source of truth for P5.** One row per Aseprite capability. A row is closed
when its status is Covered (or an approved deferral) and its task is checked in
`docs/ROADMAP.md`. Update this file in the same commit that changes a
capability, and add `Refs: P5-xx` to that commit.

Legend: **Covered** = capability present and tested · **Partial** = some depth
missing · **Missing** = no equivalent · **Blocked** = task cannot proceed
(reason given) · **Out of scope** = deliberate, user-approved.

Last audited: 2026-10-28 (after P4; P5 just opened).
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
| Tag direction 3 (ping-pong reverse) | Partial | P5-08 |
| Tag editor UI (create/edit/range/color) | Partial | P5-37 |
| Loop / ping-pong playback | Covered | — |
| Onion skin: count, opacity, tints | Covered | — |
| Timeline thumbnails | Covered | — |
| Multi-cel selection (layer×frame) | Missing | P5-34 |
| Cel copy/paste across frames/layers | Missing | P5-35 |
| Empty cel vs empty frame | Missing | P5-35 |
| Linked cels (authoring; read exists) | Partial | P5-12 |
| Cel z-index | Missing | P5-11 |
| Timeline layer×frame grid + per-cel cells | Missing | P5-37 |
| Preview window / fullscreen | Missing | P5-36 |

## Layers

| Capability | Status | Task |
|---|---|---|
| Normal, group, background, reference layers | Covered | — |
| Tilemap layers | Missing | P5-30 |
| Visibility, opacity, rename, add/remove, reorder | Covered | — |
| Nested groups + group opacity/blend | Covered | — |
| Blend modes | Partial (8/19) | P5-05 |
| Editable lock | Covered | — |
| Lock movement | Missing | P5-13 |
| Prefer linked cels | Missing | P5-13 |
| Collapsed group | Missing | P5-13 |
| Alpha masks | Covered (vixel extension) | — |
| Duplicate / merge down / flatten | Covered | — |
| Layer UUID | Missing | P5-13 |
| Layer user data | Missing | P5-14 |

## Colour

| Capability | Status | Task |
|---|---|---|
| RGBA mode | Covered | — |
| Grayscale mode + conversion | Covered | — |
| Indexed mode (real indices, conversion) | Partial | P5-03 |
| Transparent palette index | Missing | P5-03 |
| RGB→indexed dithering | Missing | P5-04 |
| Palette import/export (`.gpl`, `.hex`, `.pal`) | Covered | — |
| Palette editor: insert/delete/sort/presets/names | Partial | P5-06 |
| Colour picker wheel + recent colours | Missing | P5-06 |
| sRGB/linear working profiles | Covered (vixel) | — |
| ICC embedding (`.ase` 0x2007) | Missing | P5-09 |
| Background colour / transparent-index sprite properties | Missing | P5-03/P5-08 |
| Wide pixels rendering (pixel ratio) | Partial (metadata) | P5-26 |

## Drawing and tools

| Capability | Status | Task |
|---|---|---|
| Pencil, eraser, bucket, picker | Covered | — |
| Line, rectangle, ellipse (outline/filled) | Covered | — |
| Gradient (linear/radial) | Partial (2 colours) | P5-22 |
| Spray, shade, blur, contour | Partial (fixed depth) | P5-21 |
| Pixel-perfect freehand | Covered | — |
| Symmetry (H/V/both/radial, movable centre) | Covered | — |
| **Selection clips every paint tool** | **Missing** | **P5-02** |
| Selection stroke / fill commands | Missing | P5-25 |
| Colour-range selection | Missing | P5-25 |
| Selection from layer alpha | Missing | P5-25 |
| Custom brushes (images, spacing, library) | Missing | P5-19 |
| Ink modes (simple, lock-alpha, replace, shading, erase) | Missing | P5-18 |
| Pressure/tablet sensitivity | Missing | P5-20 |
| Tiled mode | Covered (fixed 3×3) | P5-27 |
| Move layer/cel content tool | Partial | P5-24 |
| Text tool (editable, fonts, sizes) | Missing | P5-16, P5-28 |

## Transformations

| Capability | Status | Task |
|---|---|---|
| Flip H/V, rotate 90° | Covered | — |
| Resize, crop, trim | Covered | — |
| Free rotate/scale/skew (nearest) | Covered | — |
| RotSprite rotation | Missing | P5-24 |
| Interactive transform handles + pivot | Missing | P5-24 |
| Transform multiple layers/frames/cels | Missing | P5-24, P5-34 |
| Canvas transforms | Covered | — |

## Tilemap

| Capability | Status | Task |
|---|---|---|
| Embedded tileset read/rasterize | Partial | P5-29 |
| Tileset model (id, name, base index, flags) | Missing | P5-29 |
| Tilemap layer model + tile references | Missing | P5-30 |
| Draw tiles / draw pixels in tiles | Missing | P5-31 |
| Manual / auto / stack modes | Missing | P5-33 |
| Empty tile | Missing | P5-31 |
| Tile flips X/Y/diagonal | Partial (import) | P5-29 |
| Write tileset/tilemap chunks (`.ase`) | Missing | P5-32 |
| Tileset editing + export | Missing | P5-33 |
| External tileset files | Missing | P5-17, P5-33 |

## Slices

| Capability | Status | Task |
|---|---|---|
| Slice objects (name, bounds, color) | Missing | P5-15 |
| 9-patch / 9-slice metadata | Missing | P5-15 |
| Pivot metadata | Missing | P5-15 |
| Slice read/write in `.ase` | Missing | P5-15 |
| Export by slice + JSON metadata | Partial (grid only) | P5-15 |

## Text

| Capability | Status | Task |
|---|---|---|
| Editable text objects | Missing | P5-16 |
| Fonts + sizes + alignment | Missing | P5-28 |
| `.ase` text properties round-trip | Missing | P5-16 |
| Bitmap text stamp (`:text`) | Covered (vixel shortcut) | — |

## Files and formats

| Capability | Status | Task |
|---|---|---|
| `.ase`/`.aseprite` core read/write | Covered | — |
| `.ase` unknown-chunk preservation | Missing | P5-07 |
| `.ase` grid origin/size | Missing | P5-08 |
| `.ase` cel bounds + z-index | Missing | P5-10, P5-11 |
| `.ase` linked-cel write | Missing | P5-12 |
| `.ase` layer flags + UUIDs | Partial | P5-13 |
| `.ase` user data (text/color/properties) | Missing | P5-14 |
| `.ase` slices | Missing | P5-15 |
| `.ase` color profile / ICC | Missing | P5-09 |
| `.ase` external files chunk | Missing | P5-17 |
| `.vpx` project JSON | Covered | — |
| PNG import/export | Covered | — |
| Sheet PNG + meta import/export | Covered (vixel format) | — |
| Image sequence export | Missing | P5-40 |
| JPG / BMP / TGA / WebP | Missing | P5-43 |
| PCX / FLC / FLI | Missing | P5-43 (evaluate) |
| Crash recovery | Covered | — |
| Reopen closed files | Partial (recent list) | P5-52 |
| File browser dialog | Missing | P5-45 |
| Drag-and-drop open | Covered | — |
| Recent files | Covered | — |

## Export and import

| Capability | Status | Task |
|---|---|---|
| PNG frame / sprite export | Covered | — |
| Sprite sheet export | Partial (fixed grid) | P5-41 |
| Sheet layouts + padding/trim/extrude/crop | Missing | P5-41 |
| JSON atlas metadata (hash/array) | Partial (vixel meta) | P5-42 |
| GIF export | Partial (loop always infinite) | P5-42 |
| Tag / layer / frame-range filters on export | Missing | P5-39 |
| Export scale | Covered | — |
| Import sheet (own sidecar/grid) | Covered | — |
| Import Aseprite JSON/XML atlas | Missing | P5-44 |
| Import image sequence | Missing | P5-44 |

## CLI

| Capability | Status | Task |
|---|---|---|
| Headless batch mode (`-b`) | Missing | P5-46 |
| `--save-as`, `--sheet`, `--data`, `--scale` | Missing | P5-46 |
| Colour mode/dithering options | Missing | P5-46 |
| Frame/tag/layer filters | Missing | P5-46 |
| Split layers/tags/slices/grid, filename templates | Missing | P5-47 |
| `--export-tileset`, `--palette`, `--list-*` | Missing | P5-47 |

## Scripting and extensions

| Capability | Status | Task |
|---|---|---|
| Lua scripting API | Missing | P5-48, P5-49, P5-50 |
| Script from CLI / `:script` | Missing | P5-49 |
| `.aseprite-extension` packages | Missing | P5-51 |
| JSON `Op` replay (vixel-specific automation) | Covered | — |

## Customization

| Capability | Status | Task |
|---|---|---|
| Preferences dialog | Partial (side panel + config) | P5-57 |
| Themes + font customization | Partial (light/dark) | P5-53 |
| Keyboard shortcut customization | Missing | P5-55 |
| Mouse-wheel customization | Missing | P5-55 |
| Workspace layouts | Missing | P5-54 |
| Localization | Missing | P5-56 |

## Editors, workspace, undo, performance

| Capability | Status | Task |
|---|---|---|
| Multiple documents / tabs | Missing | P5-52 |
| Preview window / fullscreen | Missing | P5-36 |
| Linear undo/redo (strokes, structural, properties) | Covered | — |
| Undo history panel | Missing | P5-38 |
| Non-linear undo | Missing | P5-38 |
| Compact/bounded undo memory | Partial | P5-59 |
| Incremental rendering / large-canvas performance | Partial (CPU full-frame) | P5-58 |
| Filter threading | Missing | P5-59 |

## vixel extras (not parity gaps)

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
| Scripting engine | No Lua/scripting crate cached; `mlua` needs network or a vendored source | P5-48 decision confirms the dependency can be added |
| FLC/FLI/PCX export | Obsolete formats; `image` has no codecs | A consumer requires them |
| ICC authoring | No fixture/verifier; `linear` has no native `.ase` encoding | A real ICC fixture exists; P5-09 |
| Custom GPU shaders | Not an Aseprite capability (vixel extra) | A shader can be verified on real hardware |
| Web/Wasm build | Aseprite has no web build; product choice | Product requirement changes |
