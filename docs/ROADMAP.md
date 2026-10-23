# Roadmap

**This is the complete backlog.** `docs/PROGRESS.md` has the immediate next step;
everything still to do should be listed here. "Full Aseprite parity" is the north
star, but the project is already a usable editor — the rest is depth and polish.

## P0 — next up

- **Layer groups & masks** — nested layer groups, per-layer alpha masks, and
  per-layer **lock / editable** flags (affects compositing + the layer panel).
- **Aseprite format fidelity** — read **linked cels** (type 1), **compressed
  tilesets** (type 3), **cel opacity** (currently ignored) and layer **groups**;
  write **indexed-color** (depth 8/16) in addition to depth-32 RGBA; add
  round-trip tests against real Aseprite-saved fixtures.
- **Tile mode** — tile grid with wrapping paint for seamless textures.

## P1 — editing depth

- **Frame tags** (named ranges) and **frame reorder** (drag in the timeline).
- **Onion-skin settings** (tint color, count, opacity) and loop/playback options.
- **Layer panel**: drag-to-reorder (currently ↑/↓ + `:layerup`/`:layerdown`).
- **Text tool** — bitmap-font text placement (a larger feature).
- **Palette tools**: reorder swatches, HSV/hex input in the picker,
  palette-from-sprite, and sprite **color modes** (RGB / indexed / grayscale).
- **Symmetry settings** — radial symmetry and an adjustable center/origin.
- **Grid settings** — grid size and snap-to-grid.

## P2 — export, filters & transform

- **Export options** — sprite sheet, GIF, JSON metadata, and slice export.
- **Filters** — brightness/contrast, hue/saturation, invert, outline, blur/sharpen.
- **Selection ops** — expand/contract, border, feather, invert.
- **Free transform** — arbitrary-angle rotate, scale, skew.
- **Sprite properties** — color mode, pixel aspect ratio, grid.

## P3 — rendering & polish

- **Color profiles** (sRGB handling, ICC).
- **Custom shaders** (GPU blend modes / live effects) — isolated behind a small
  `Renderer` trait, since this is the one thing needing per-backend code.
- **wgpu backend** switch (feature flag + `renderer:` line; no code change
  expected with the current `TextureHandle`-only rendering).
- **Config/theme persistence** — window size, grid/onion defaults, keymap
  customization.

## Explicitly out of scope (for now)

- Web/Wasm build.
- Multi-document / tabbed interface.
- Plugins / scripting (Aseprite's Lua API).

## Suggested order

1. Layer groups & masks → tile mode.
2. Aseprite format fidelity + export options.
3. Text tool, filters, free transform.
4. Color profiles, shaders/wgpu, persistence.
