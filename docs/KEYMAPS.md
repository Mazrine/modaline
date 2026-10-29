# Keymaps: vim now, Helix later

vixel ships a vim-style keymap because that is what pixel-art and code people
already know. This note records why Helix feels friendlier, what would actually
translate to a canvas, and the phased plan (tracked as P7-01/P7-02 in
`docs/ROADMAP.md`).

## Why Helix is easier for newcomers

- **Selection first, then verb.** You select what you mean, see it, then press
  `d`/`y`/`c`. vim's `dw` requires knowing the motion and verb order first.
- **One mode fewer in practice.** Insert is the odd mode; normal/replace/visual
  read the same way. In vixel the canvas is already "normal" and drawing is a
  momentary mode (`i`), so the mental model is close.
- **Discoverable menus.** `space` opens a which-key-style menu instead of
  requiring memorised `:` commands.
- **Multiple cursors/selections** are first-class. On a canvas this maps to
  multi-cel selection and repeated stamps, not text words.

For a pixel editor the text motions (`w`, `b`, `e`, `f`, `t`) have no direct
meaning. Helix's real wins here are the **selection-first verbs**, the leader
menu and fewer mode switches — not the text objects.

## Mapping proposal (canvas)

| Helix idea | vixel today | Proposed Helix preset |
|---|---|---|
| `v` extends selection, then `y/x/p` | same (`v`, `y`, `x`, `p`) | keep |
| `u` undo, `U` redo | `u`, `Ctrl-r` | `u`, `U` |
| `i` / `a` insert | `i` draw mode | keep `i`, add `a` as draw |
| `Esc` normal | `Esc` normal | keep |
| `space` leader | none | `space f` files, `space p` panels, `space t` tools, `space ?` help |
| `g` goto menu | `gg`, `G`, `0`, `$` | `g g`, `g e` (end), `g h` (start line) |
| `x` select line | `x` erase pixel | in selection mode `x` selects the cel row; normal `x` stays erase (documented override) |
| `:w`, `:q` | same | same |
| `Ctrl-hjkl` window pan | added | same (Helix uses `C-w` for splits; no splits yet) |

The preset must not silently change tool keys; it changes undo/redo, leader
menus and selection verbs, and lists every override in `:tips` so users are not
surprised.

## Phases

1. **P7-01 — preset switch.** `keymap.preset = "vim" | "helix"` in config and
   `:keymap vim|helix`; applies the table above to the rebindable `Action` layer.
   Keep explicit `:bind` overrides on top.
2. **P7-02 — leader / which-key.** Hold `space`, see available groups with
   descriptions; same data as the help overlay, generated from `Action::ALL`.
3. **P7-03 — pen extras.** Tilt/rotation/ExpressKey mapping (separate task).
4. **P7-04 — UX follow-ups.** Collapsible side sections, fuzzy command palette,
   focus ring (see `docs/UX_AUDIT.md`).

Until the preset lands, `:bind` can approximate it today:

```vim
:bind undo u
:bind redo U
:bind pan-left ctrl-h
:bind pan-right ctrl-l
```

Only the actions in `:bind list` are rebindable; fixed modal keys (tools,
selection) are documented in `?`.
