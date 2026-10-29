# Arcane UI — plan

Port the Arcane Pixel look and game-like shell from `another tool` into Modaline
without changing editing behaviour or the vim keymap. Work lands on
`feat/arcane-ui`; each step gets a screenshot and a keyboard/mouse/pen pass.

Source of truth: `~/projects/another tool/src/viewer/ui/` (`tokens.rs`, `theme.rs`,
`frame.rs`, `widgets.rs`, `shell.rs`, `icons.rs`) and its bundled fonts.

## Status on `feat/arcane-ui`

- Step 1 landed: palette, bundled fonts, `arcane` theme (new default), flat
  visuals.
- Step 2 landed: panel frames, dot grain, wells/raised helpers, canvas corner
  brackets.
- Step 3 started: tool groups, mode chip with per-mode colours, side-panel
  section headers, wrapped HUD at 1280–1600 px. Icons and the overlay layout
  remain for the next pass.
- Review screenshots: `docs/screenshots/arcane-editor.png`,
  `arcane-selection.png`, `arcane-help.png`.
- Defaults for new configs are now `theme=arcane`, `ui_font=silkscreen`,
  `ui_font_size=12`; existing configs keep their theme and font.

## Design rules

- Ink-and-parchment palette, violet accent, gold focus; 1 px straight borders,
  no rounded corners.
- Silkscreen for UI text, Noto Sans Symbols 2 as fallback; 8/10/12/14/18 pt
  scale, uppercase labels.
- Panels use a dot grain and a lighter top edge; wells are sunken; buttons are
  raised and press in; selected tabs get an accent underline.
- The shell should read as a game HUD: grouped tool strip, mode chip, status
  band, workspace layout switch, visible focus.
- Keep the existing JSON themes and `:theme`; Arcane becomes a built-in theme
  (default for new configs) and the custom chrome only paints when it is active.

## Steps

1. **Tokens + fonts + theme** (this branch, foundation)
   - Copy the Arcane palette into `src/arcane.rs` (void, panel, panel_hi,
     sunken, line, line_hi, text, dim, faint, accent, accent_hi, accent_deep,
     focus, warn, good, danger) and the spacing/type scales.
   - Bundle `Silkscreen-Regular.ttf` + `NotoSansSymbols2-Regular.otf` under
     `assets/fonts/` with OFL notices; load `silkscreen` as a built-in UI font
     and make it the default for new configs.
   - Add `arcane` to `theme.rs` and apply the Arcane `Visuals` (flat corners,
     borders, accent selection, Silkscreen text styles).
   - Acceptance: `:theme arcane` switches live, `:uifont silkscreen` works,
     all text styles use the pixel font, `cargo test`/clippy green, screenshot.

2. **Pixel chrome primitives**
   - `frame` (panel fill + dots + hairline + top highlight), `well`, `tab`,
     `raised`, `mode_chip`, dotted texture helpers.
   - Paint the toolbar, side panel, timeline and status band with the chrome;
     use the sunken style for the command/status line.
   - Acceptance: no rounded corners anywhere; panels have grain and borders;
     screenshot at default layout.

3. **Game HUD flow**
   - Group the tool strip (paint / shape / select / transform / view) with
     separators and key hints; mode chip shows NORMAL/DRAW/VISUAL with accent.
   - Side panel becomes titled sections (Brush, Animation, Appearance, Layers)
     with carved headers; timeline gets a dock header.
   - Keep every label command-backed and localized.
   - Acceptance: same actions reachable, keyboard-only pass, screenshot.

4. **Pixel icons**
   - Port/redraw the another tool icon set for tools and panel actions (pencil,
     eraser, bucket, picker, shapes, select, layers, frames, play).
   - Icons are painted primitives (no external image dependency), with text
     tooltips and key hints.
   - Acceptance: every toolbar button identifiable without text at 1280 px,
     tooltips still name the key.

5. **Motion, focus and feedback**
   - One-pixel hover lift, pressed inset, 120 ms accent fades, focus ring on
     the canvas and command line, toast for destructive actions.
   - Respect `prefers-reduced-motion`-style config (`motion: off`).
   - Acceptance: visible focus while tabbing; no layout shift on hover.

6. **Canvas-first overlay layout**
   - Add a second layout (`overlay`): compact top HUD, tool rail on the left,
     floating inspector right, canvas fills the window; switch with Ctrl+K or
     `:layout overlay`.
   - Keep the docked `studio` layout as default until parity is reviewed.
   - Acceptance: pan/zoom/hit-testing unchanged; layout round-trips in config.

7. **Contrast, tests and docs**
   - Add WCAG contrast tests for the Arcane palette (text/dim/faint/accent),
     a font-loading test, and a theme-round-trip test.
   - Update README screenshots, `docs/COMMANDS.md` (`:theme arcane`,
     `:uifont silkscreen`, `:layout overlay`), and `THIRD_PARTY.md` for OFL
     fonts.
   - Acceptance: full gate green, Xvfb screenshots for both themes/layouts.

## Non-goals

- No keymap changes; Helix stays P7-01.
- No new editor features or file-format changes.
- No bitmap 9-slice theme images; the pixel look is geometry + tokens.
