# Roadmap

Modaline is usable today. This file lists the work that is still worth doing;
the capability matrix in [`ASEPRITE_PARITY.md`](ASEPRITE_PARITY.md) is the
source of truth for what is covered.

## Next

- **Helix keymap preset** — a selectable preset on top of the existing action
  layer, keeping the canvas verbs and documenting the overrides. See
  [`KEYMAPS.md`](KEYMAPS.md).
- **Leader / which-key overlay** — hold the leader to browse actions with
  descriptions instead of memorising `:` commands.
- **Pen depth** — tilt, rotation and ExpressKey mapping on top of the current
  pressure support. See [`PEN.md`](PEN.md).
- **UX follow-ups** — collapsible side-panel sections, a fuzzy command palette,
  a keyboard-accessible colour stepper, canvas focus treatment. See
  [`UX_AUDIT.md`](UX_AUDIT.md).

## Deferred

- PCX/FLC/FLI export: import is supported, but the formats are obsolete and the
  import crates do not write them.
- Custom GPU shaders: a vixel extra, not an Aseprite feature. Everything renders
  through `egui::TextureHandle`.
- Web/WASM build: product choice, not planned.

## Done

P0–P6 (foundation, editing depth, Aseprite fidelity, brushes/ink, text,
tilesets, timeline, export/import, CLI/scripting, app shell, parity gaps and
accessibility) and the Arcane UI pass are complete. The commit history and the
capability matrix carry the details; this file stays short.
