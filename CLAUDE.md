# CLAUDE.md

Context for Claude Code (and other agents) working on **Modaline**.

## Project

A keyboard-first, vim-style pixel art editor in Rust (native GUI via `eframe`
0.37 / glow, wgpu-swappable). Aseprite-grade rendering: truecolor, real alpha +
checkerboard, onion skin, per-layer blend modes — all driven by vim keys.

## Commands

```sh
cargo build && cargo test && cargo clippy --all-targets
cargo run --release
steam-run ./target/release/modaline   # NixOS: needed for wayland/xkbcommon libs
```

## Rules

- **Model is GUI-free.** `color.rs`, `sprite.rs`, `editor.rs`, `io.rs`, `ase.rs`
  must not depend on egui. `gui.rs`/`app.rs` (view) and `input.rs` are shells.
- Cels are `[layer][frame]`; `layers[0]` = topmost; each cel is a row-major
  `Vec<Rgba>` of `width * height`.
- Editing hits the **active layer + active frame** only. Mutate pixels via
  `paint_pixel` (stroke-recording undo), never `sprite.set` directly.
- Rendering goes through `egui::TextureHandle` only (backend-agnostic). No raw
  GL. This is what makes the future glow→wgpu switch a config change.
- Selection/clipboard are per-pixel **masks**, not rectangles.
- Persisted structs derive serde; new fields use `#[serde(default)]`.
- Add unit tests for non-trivial model/parser logic (`sprite.rs`, `ase.rs`).

## egui 0.37 specifics

- `App::ui(&mut self, ui, frame)` (not `update(ctx, …)`); context via `ui.ctx()`.
- Panels: `egui::Panel::top/right/bottom(…)` (no `TopBottomPanel`/`SidePanel`).
- Keyboard guard: `ui.ctx().egui_wants_keyboard_input()`.

See `AGENTS.md` for the full module map and gotchas.
