# UX / UI audit (2026-10-29)

Scope: the real GUI under Xvfb (screenshots in the session log), the control
inventory in `src/gui.rs`, and a keyboard/mouse/pen pass over every action.
Findings are ordered by impact. "Landed" means fixed in this pass; the rest is
tracked in `docs/ROADMAP.md` P7.

## What already works

- The canvas is keyboard-first without hiding the mouse: tools on single keys,
  `hjkl`/`HJKL` cursor movement, `0/$/gg/G`, visual mode, command line.
- Undo is consistent and visible (history window), including panel edits.
- Every destructive command asks or has a `!` form; dirty tabs refuse to close.
- The status line always reports what happened.
- Palettes, layers, timeline, tags, slices, Chinese/Portuguese catalogs and the
  command line are reachable without the file browser.
- Pen pressure now reaches the brush on X11 (see `docs/PEN.md`).

## Problems found, and what was done

| Problem | Why it hurt | Fix |
|---|---|---|
| No keyboard pan | Mouse-only middle drag broke the "keyboard-only" promise | `Ctrl-h/j/k/l` (rebindable `pan-*` actions) + hint |
| Brush size was slider/`:brush` only | Slow for the most common adjustment | `{` / `}` grow/shrink, clamped 1–64 |
| Cel opacity, layer opacity, frame duration only in panels | Not scriptable, not undoable from the keyboard (the cel slider bypassed undo) | `:celopacity`, `:layeropacity`, `:frameduration`; all one undo step |
| Layer visibility and blend had no command | Keyboard users could not toggle/hide or change blending | `:visible [on/off]`, `:blend <name>` (parser unit-tested) |
| Onion skin only via checkbox | No keyboard path | `:onion [on/off|count N|opacity N]` |
| Playback and tag colour were mouse-only | Timeline buttons/pickers had no commands | `:play [on/off]`, `:tagsel <name>`, `:tagcolor #rrggbb` |
| Floating windows needed the × button | Esc did not close prefs/export/history/files/tips | Esc closes the topmost window first |
| No next-step guidance | New users see a dense toolbar and a status string | Contextual hint on the status line; `:tips` window |
| Toolbar tools had no key hints | Names are discoverable, shortcuts were not | Hover tooltip `pencil (b)`, etc. |
| Pen invisible on X11 | Pressure silently missing | evdev reader, pen row in the side panel, `:pen` |
| Status/help/panels were English-only | Locale switch felt half done | Full catalog pass (P6-03) |

## Still weak (planned)

1. **Side-panel length**: the scroll is long and flat. Proposal: collapsible
   brush / animation / appearance sections with remembered state (`P7-04`).
2. **No command palette**: `:` + tab completion exists only for command names in
   the help text. A fuzzy palette over commands and tools would be the single
   biggest speed win (`P7-04`).
3. **Color picking depth**: `:fg #hex` is precise but slow for eyeballing; a
   keyboard-accessible HSV stepper is missing.
4. **Export dialog** still needs typed paths for some workflows; acceptable
   because `:exportcfg` covers it, but a focused path field + tab order would
   help.
5. **Dense toolbar**: 16 text buttons at small widths. Grouping with separators
   (paint / shape / select / transform) and overflow is planned with the panel
   work.
6. **Modal discoverability**: `?` help is long; `:tips` helps, but a first-run
   two-line coach mark is not implemented.
7. **No visual focus ring** on the canvas vs panels; keyboard users can lose
   where input goes. Track with the palette work.

## Keyboard / mouse / pen parity matrix

Legend: K = keyboard, M = mouse, P = pen tip (buttons via OTD/driver).

| Action | K | M | P |
|---|---|---|---|
| Move cursor | hjkl/HJKL, 0/$/gg/G | click | hover/click |
| Pan canvas | Ctrl-hjkl | middle-drag | middle button (mapped) |
| Zoom | + / - , wheel | wheel | wheel/ExpressKey (mapped) |
| Paint / erase | r, x, i mode | drag | drag with pressure |
| Pick colour | c at cursor, `:fg #hex` | right-click with picker / `c` | barrel button mapped to right click |
| Shapes | d/R/o + Enter | drag | drag |
| Selection | v + hjkl, w, L/P | drag | drag |
| Copy/cut/paste | y/x/p | — | — |
| Undo/redo | u / Ctrl-r | — | ExpressKey (mapped) |
| Brush size | { / }, `:brush n` | slider | slider |
| Layers | `:newlayer`, `:visible`, `:blend`, `:layeropacity`, `:lock` | panel clicks | panel clicks |
| Frames | `:newframe`, `:frameduration`, `[`/`]` | timeline | timeline |
| Files | `:w`, `:open`, `:recent`, Ctrl-Cmd-O | browser | browser |
| Dialogs | Esc closes, `:prefs`/`:tips` toggle | × button | × button |
