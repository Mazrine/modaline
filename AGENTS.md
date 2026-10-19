# AGENTS.md

Guidance for AI agents working on **vixel** — a keyboard-first, vim-style pixel
art editor in Rust (native GUI, not a terminal).

## What this is

- Aseprite-grade rendering (true 24-bit color, real alpha, checkerboard, onion
  skin, blend modes) driven entirely by vim keys.
- GUI is `eframe` 0.37 on the **glow** (OpenGL) backend, designed to switch to
  `wgpu` later with no render-code changes.

## Commands

```sh
cargo build
cargo test
cargo clippy --all-targets        # must be clean before committing
cargo run --release
steam-run ./target/release/vixel  # NixOS: wayland/xkbcommon libs aren't on the
                                  # default linker path, so run via steam-run
```

The GUI can't run headless; verify with `cargo test` (model logic) and, on a
desktop, a manual run.

## Architecture — read before touching anything

The code is split so the **model has no GUI dependency** and the view/input are
thin shells:

| Module | Role |
|--------|------|
| `color.rs` | `Rgba` (tuple struct, serde as `[r,g,b,a]`) + default palette |
| `sprite.rs` | Model: `Sprite`/`Layer`/`Frame`/`Cel`, compositing + blend modes, tool geometry (fill/line/rect/ellipse), transforms, tests |
| `editor.rs` | `Editor` state: modes, tools, undo, selection/clipboard. No view state here. |
| `io.rs` | PNG + `.vpx` (JSON) save/load |
| `ase.rs` | `.ase`/`.aseprite` import/export |
| `input.rs` | vim keymap + `:` command line (reads `egui::Event`) |
| `gui.rs` | `View` (zoom/pan), canvas drawing, panels, help overlay |
| `app.rs` | `eframe::App`: event loop, playback timer, texture upload |
| `main.rs` | `run_native` + module wiring |

**Core invariants (do not break):**

1. A sprite is a grid of **cels** indexed `[layer][frame]`. `layers[0]` is the
   **topmost** layer (drawn last). Each cel is a row-major `Vec<Rgba>` of
   `width * height`.
2. **Editing always targets the active layer + active frame** (`active_layer`,
   `active_frame` in `Editor`). The canvas shows the composite of all visible
   layers of the active frame.
3. **Undo** works by recording `(x, y, before, after)` into a *stroke*
   (`begin_stroke` / `paint_pixel` / `end_stroke`); a whole drag is one undo
   step. Every mutation of cel pixels must go through `paint_pixel` (or a
   sibling that records), never `sprite.set` directly.
4. **Rendering is backend-agnostic**: everything goes through
   `egui::TextureHandle` (composite → `ColorImage::from_rgba_unmultiplied` →
   `load_texture`/`set`, nearest-neighbor). No raw GL calls. Custom shaders are
   the *only* thing that would need per-backend code — deliberately avoided.
5. **Selection/clipboard are masks**, not rectangles: `Selection { x, y, w, h,
   mask }`. Copy/cut/move/paste operate on the active layer's cel.

## Conventions

- New model logic (branches, loops, parsers) gets a unit test in the same file
  (`#[cfg(test)]`). `sprite.rs` and `ase.rs` are the pattern to follow.
- Persisted structs derive `Serialize`/`Deserialize`; new fields use
  `#[serde(default)]` so old `.vpx` files still load.
- vim keys go in `input.rs`; tool/shape names go through `Tool`/`Shape` enums in
  `editor.rs`. Add new tools to the enums, `input.rs`, and the toolbar in
  `gui.rs` together.
- `:` commands live in `input.rs::exec_command`, dispatched by extension where
  relevant (`.ase`, `.png`, else `.vpx`).

## egui 0.37 gotchas (this version changed things)

- `eframe::App` trait method is `fn ui(&mut self, ui: &mut egui::Ui, frame)`,
  **not** `update(ctx, …)`. Get the context via `ui.ctx()`.
- Panels are a single `egui::Panel` type: `Panel::top("id")`, `Panel::right(…)`,
  `Panel::bottom(…)` — there is no `TopBottomPanel`/`SidePanel` anymore.
  `Window::show` still takes `&Context`.
- Keyboard guard is `ui.ctx().egui_wants_keyboard_input()` (note the `egui_`
  prefix).
- `Id::new` / `Ui::id` are deprecated → `ui.scope_id()` / `IdSalt`.

## NixOS runtime note

The wayland/xkbcommon client libraries are not on the default loader path in a
plain shell. `cargo run` will fail with `NoWaylandLib`; use `steam-run` (see
Commands). A proper `flake.nix` devShell is a welcome future addition.
