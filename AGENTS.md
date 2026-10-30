# AGENTS.md

Contributor notes for **Modaline** — a keyboard-first, vim-style pixel
art editor in Rust (native GUI, not a terminal).

## What this is

- Aseprite-grade rendering (true 24-bit color, real alpha, checkerboard, onion
  skin, blend modes) driven entirely by vim keys.
- GUI is `eframe` 0.37 on the **glow** (OpenGL) backend, designed to switch to
  `wgpu` later with no render-code changes.

## Commands

```sh
nix run .                         # build + run the flake package
nix develop                       # NixOS: LD_LIBRARY_PATH for wayland/xkbcommon
cargo build
cargo test
cargo clippy --all-targets        # must be clean before committing
cargo run --release
steam-run ./target/release/modaline  # fallback without the devShell
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
| `io.rs` | PNG + sheet/GIF/meta + `.vpx` (JSON) save/load |
| `ase.rs` | `.ase`/`.aseprite` import/export |
| `palette.rs` | `.gpl` / `.hex` / `.pal` palette import/export |
| `ops.rs` | an `Op` list replayed through `Editor::apply` (one undo step) |
| `config.rs` | `config.json` load/save + the autosave `recovery.vpx` path |
| `input.rs` | vim keymap + `:` command line (reads `egui::Event`) |
| `gui.rs` | `View` (zoom/pan), canvas drawing, panels, preferences/export/help windows |
| `app.rs` | `eframe::App` + `Doc` tabs: event loop, per-doc view/textures, playback, session persistence |
| `pen.rs` (lib) | Linux evdev pressure for Wacom/OTD tablets; position/clicks stay on the OS pointer |
| `theme.rs` | JSON themes + built-ins, egui visuals application, UI font/size loading |
| `keymap.rs` | action layer, chord parsing, conflicts, wheel actions, default suppression |
| `i18n.rs` (lib) | JSON language catalogs, English fallback, translator and missing-key checks |
| `script.rs` (lib) | sandboxed Lua (mlua) + Aseprite-style `app` API; scripts commit one snapshot undo |
| `extension.rs` | `.aseprite-extension` zip install/list/remove, contributed scripts/themes |
| `layout.rs` (lib) | JSON workspace layouts, built-ins and size clamping |
| `lib.rs` | The **model** half as a library (`ase`, `color`, `editor`, `io`, `ops`, `palette`, `sprite`) with no GUI dependency, so another tool can read/composite/write a `.vpx` without launching the editor |
| `main.rs` | `run_native` + shell module wiring; takes an optional file path argument |

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
   sibling that records), never `sprite.set` directly. Structural edits
   (layers, frames, tags) use `UndoStep::Structure` snapshots that carry the
   active indices; panel property edits use a cheap `LayerProps` step.
   `paint_pixel` also enforces canvas bounds and the selection clip;
   `paint_pixel_unclipped` is reserved for paste, selection-transform writes
   and contour.
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
- `:` commands live in `input.rs::exec_command`. Opening a file dispatches on
  the extension in **one** place, `io::load_any` (`.ase`, `.png`, else `.vpx`),
  shared by `:open` and the command-line argument.
- Bump `Editor::revision` (via `touch`) on every sprite mutation; the timeline
  caches thumbnails by it, so a missed bump shows a stale frame.
- Opaque `.ase` chunks live in `Sprite.raw_chunks` with a `ChunkAnchor`; keep
  them when adding or removing chunks so open→save never destroys data this
  build does not parse yet.
- Indexed sprites keep every pixel on `Sprite.palette`; write through
  `Sprite::set` or `indexed_color` so nearest-entry snapping happens, and keep
  `transparent_index` pointing at the transparent entry.
- `palette` and `palette_names` are parallel vectors in both `Editor` and
  `Sprite`; edits must move both, and indexed edits mirror into the sprite.
- `.ase` chunk 0x2007 is parsed into `Sprite.ase_profile`; do not leave it in
  `raw_chunks`, or a save would write the profile twice.
- Cel pixels are cel-sized, not canvas-sized. Use `Cel::get`/`set_extend` or
  `Sprite::get`/`set`; never index `pixels` by the canvas width. Transform and
  composite code must honour cel bounds.
- Composite order is `-layer_index + cel.z_index` (ties by z) because our layer
  list is top-first; positive z moves a cel towards the front.
- A linked cel has `link: Some(frame)` and empty pixels; `Sprite::get`/composite
  resolve it, `Sprite::set` materialises it first (copy-on-write), and frame
  inserts/removes/moves must remap or materialise links.
- Layer flags map to `.ase` bits: editable 2, lock movement 4, background 8,
  prefer-linked 16, collapsed 32, reference 64. UUIDs are written only when at
  least one layer has one (header bit 4).
- `UserData.properties` is the raw 0x2020 property payload after flags/text/
  colour; keep it verbatim. Tag user data is positional: write one chunk per
  tag (empty placeholder included) so later tags stay attached correctly.
- Slice chunks carry their own user data immediately after 0x2022; slice keys
  are valid from `frame` onward, so exports use the last key <= the frame.
- Text objects live in the cel's user-data property map key 0 (`text`, `font`,
  `size`, `align`, `antialias`, `x`, `y`); use `properties::parse/write` so
  unknown sibling properties survive edits. A cel with user data but no pixels
  still needs its cel chunk written.
- External files (0x2008) are parsed into `Sprite.external_files`; external
  tilesets store their file/tileset IDs and deliberately skip pixel decoding
  until P5-33. Deprecated chunks stay in `raw_chunks` on purpose.
- Pen pressure comes from `pen::PenReader` (evdev `ABS_PRESSURE`/`BTN_TOUCH`)
  and is applied by the app only while the tip is down; never grab the device
  (the OS pointer must keep working). See `docs/PEN.md`.
- Canvas pan is bindable `pan-*` actions (`Ctrl-hjkl` by default); brush size has
  `{`/`}` and every panel slider has a command (`:celopacity`,
  `:layeropacity`, `:frameduration`, `:visible`, `:blend`, `:onion`). Keep that
  keyboard/mouse/pen parity when adding controls (`docs/UX_AUDIT.md`).
- Brush paths go through `apply_ink`; operations that own their pixels (cut,
  transform, paste, contour, filters) must use the exact put path so the active
  ink cannot alter them. Inks are stored per tool in `tool_inks`.
- Brush masks are `u8` coverage (0-255). Every stamp path calls
  `stamp_brush_symmetric`, which modulates the paint alpha by coverage and
  `Editor::pressure()`; keep freehand, shapes and the cursor preview on
  `current_brush()`. Pressure precedence is manual → device → 1.0. Shade,
  blur and spray iterate that same mask, so brush size stays consistent across
  tools; blur radius must keep using `box_blur`. Gradient stops live in
  `gradient_stops` (empty = fg→bg) and must stay sorted; dithering is
  gradient-local (`gradient_dither`) and must not touch conversion settings.
- Filters are `Filter` values parsed by `parse_args` and applied by
  `apply_filter`; keep them selection-clipped through `Editor::filter` so each
  remains one undo step.
- Free transforms read `transform_pivot` (None = bbox centre) and
  `transform_rotsprite`; handle drags in `gui.rs` apply exactly one transform on
  release. Keep new canvas drag modes out of the paint path.
- Canvas geometry goes through `View::cell()` (zoom × aspect); never multiply
  overlay coordinates by `zoom` directly, or wide-pixel sprites drift.
- Preview state is independent (`View::preview_tick`, `preview_frame`,
  `preview_zoom`); keep it out of the editor's active-frame playback.
- Timeline grid thumbnails are cached in `View::cel_thumbs` and invalidated by
  `Editor::revision`; rectangle selection uses `select_cel_rect`.
- Undo remains linear; `goto_history` navigates by replaying undo/redo. Keep
  every `UndoStep` variant's `label()` meaningful for the history panel.
- Exports go through `ExportConfig`/`configured_sprite`; derive a filtered
  sprite rather than re-implementing frame/layer selection in each exporter.
  Sequence filenames go through `render_filename` (sanitize, `.png`, dedupe).
  Sheet layouts use `export_sheet_layout`; keep border/shape/inner padding and
  extrusion distinct, and record them in `Sheet`/meta. Static formats dispatch
  through `is_image`/`export_image_scaled`; add new codecs to Cargo features.
  Atlas/sequence imports live in `import_atlas`/`import_sequence_dir`; keep
  metadata-only layers documented as merged on import. Save dispatch is
  `Editor::save_path`; browser favourites persist in Config.
- Theme files are JSON (`src/theme.rs`); keep `Default` literal (never call
  `builtin` from it) and map new colors through `apply`.
- UI strings go through `View::tr(key)` with keys in `lang/en.json`; add the
  translation to `lang/pt-BR.json` or it falls back to English.
- Status messages are `Status` values: build one with `crate::status!` in the
  library or `modaline::status!` in the shell so the `format!` template is
  recorded (plain `.into()` literals translate by exact text). Add the English
  template to `lang/en.json` `messages` and its translation to
  `lang/pt-BR.json`; enum names embedded in messages go in both `values` maps;
  the i18n coverage test fails on a missing entry. Static labels use
  `View::tr`/`View::value` (value keys are `kind.name`).
- Keymap actions run before the fixed modal handlers. When a char key is
  consumed, set `view.suppress_text` so the following `Event::Text` does not run
  the old handler again; `blocks_default` handles moved defaults.
- The sprite background colour is the sprite user-data colour; keep
  `Sprite::set_background_color` preserving text/property payload.
- Undo pushes go through `Editor::push_undo`/`push_redo` so the count and byte
  budgets are enforced; add new step kinds to `UndoStep::bytes`.
- Pixel writes use `Editor::record` (merged dirty rect); structural edits use
  `Editor::touch` (full rebuild). The canvas consumes dirty state with
  `clear_dirty`; do not bypass `record` for pixel edits or caching goes stale.
- Timeline/side panel content must fill `ui.max_rect()`; the unified egui Panel
  sizes to content otherwise. Loading a layout seeds `PanelState` via
  `force_panel_size` so remembered sizes do not win.
- Scripting engine decisions live in `docs/SCRIPTING.md` (Lua/mlua chosen;
  P5-49/50/51 blocked on offline dependency). Do not substitute another engine
  without updating that document.
- Headless CLI lives in `src/cli.rs` (`-b`), parsed before any eframe setup;
  keep new batch flags there and cover them in `tests/cli.rs`. Splits reuse
  `export_sequence`/`export_slices`/`export_named_slices`.
- Tile preview extents use `Editor::tile_offsets()`/`tile_x`/`tile_y`; the
  canvas extent and hit area must match or seam clicks land outside.
- Tilesets live in `Sprite.tilesets` keyed by `Tileset.id`; layers reference
  that ID (`Layer.tileset`). Tile IDs map through `base_index`, and flips apply
  diagonal → X → Y. Tilemap cels keep `Cel.tilemap` refs beside `pixels`; after
  changing refs call `Sprite::rasterize_tilemap` so composite stays current.
  Tile edits go through `paint_tile_at` (snapshot + rerasterize); pixel mode is
  a cache edit and does not change refs. `.ase` writing emits `0x2023`, tilemap
  layer kind 2, and type-3 cels; keep the layer tileset ID before the UUID.
- Multi-cel writes go through `cel_targets` (active always included; empty set =
  active only). Transform loops suspend fan-out and collapse to one snapshot.
- Cel copy resolves link chains into `cel_clipboard`; paste clears the link.
  `delete_cel` empties the cel (keeps the frame); `del_frame` removes frames.
- Text editing snapshots once in `text_edit_before`; `preview_text_object`
  re-rasterises without undo, and only `commit_text_edit` pushes the step.
- Selection masks are sprite-sized grids (`selection_grid`/`grid_to_selection`);
  fill/stroke and mask transforms go through the paint/put helpers so clipping
  and undo stay correct. `transform_selection_mask` is mask-only; the pixel
  transform is `free_transform`.
- `docs/ASEPRITE_PARITY.md` is the source of truth for Aseprite parity: when a
  capability lands, update its row and the matching roadmap item in the same
  commit.

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
plain shell, so `cargo run` fails with `NoWaylandLib`. Use `nix develop` (the
flake devShell sets `LD_LIBRARY_PATH`), or `steam-run` as a fallback.
