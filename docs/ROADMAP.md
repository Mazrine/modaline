# Roadmap

Planned work, roughly ordered by value vs. effort. "Full Aseprite parity" is the
north star but is a large, ongoing effort; this lists the concrete next steps.

## P0 — next batch candidates

*(Done: selection tools + add/subtract/intersect, magic-wand tolerance &
all-layers, filled shapes, symmetry, gradient/spray/shade, OS clipboard,
timeline thumbnails.)*

- **Selection transforms** — flip/rotate the selected content (currently
  transforms are whole-sprite only).
- **Blur + contour tools** (the remaining toolset items).
- **Layer groups & masks.**
- **Aseprite format fidelity** — linked cels, cel opacity, indexed-color export.

## P1 — fill out the toolset & selection

- More tools: gradient, spray, shade, blur, contour, and a **fillable** ellipse/
  rectangle (filled vs. outline toggle).
- Magic-wand **tolerance** (color-distance threshold) and an "all layers" option.
- Selection **modifiers** — add / subtract / intersect with an existing
  selection (e.g. shift/alt while selecting).
- Selection transforms: flip / rotate **the selection** (currently transforms
  are whole-sprite only).
- Contiguous + bounding-box selection export.

## P1 — layers & animation

- **Layer groups** and per-layer **lock** / editable flags.
- **Mask layers** (per-layer alpha mask).
- Frame **tags** (named ranges) and frame reorder.
- Onion-skin settings (tint color, count, opacity).
- Layer reorder by drag in the panel (currently ↑/↓ + `:layerup`/`down`).

## P2 — Aseprite format fidelity

- Read/write **linked cels**, **compressed tilesets** (type 3), cel **opacity**
  (currently ignored), layer **groups**.
- **Indexed-color export** (depth 8/16) in addition to the current depth-32
  RGBA write.
- Round-trip tests against real Aseprite-saved files (fixtures).

## P2 — palettes & color

- Load/save **custom palettes** (`.gpl`, Aseprite `.pal`/`.hex`).
- Palette management UI: add/remove/reorder swatches, import/export.
- HSV/hex input in the picker (egui has this built in — surface it).

## P3 — rendering & polish

- **Custom shaders** (GPU blend modes / live effects) — the one thing that will
  need per-backend (glow vs wgpu) code; isolate behind a small `Renderer` trait.
- **wgpu backend** switch (feature flag + `renderer:` line — no code change
  expected with the current `TextureHandle`-only rendering).
- **Color profiles** (sRGB handling, ICC).
- Config/theme persistence (window size, grid/onion defaults, keymap
  customization).

## Explicitly out of scope (for now)

- Web/Wasm build.
- Multi-document/tabbed interface.
- Plugins/scripting (Aseprite's Lua API).

## Suggested order

1. OS clipboard + timeline thumbnails (daily-use ergonomics).
2. Toolset/tolerance/modifiers, then layer groups & masks.
3. Aseprite format fidelity, then palettes, then shaders/wgpu.
