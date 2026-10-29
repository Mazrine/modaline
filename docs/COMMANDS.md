# Keys and commands

The in-app `?` help and `:tips` window are the same reference; this file is the
offline copy. Every action below is keyboard-first; the mouse path is at the
end, and the pen is described in [`PEN.md`](PEN.md).

Modal, like vim. Start in **normal** mode.

## Normal mode

| Key | Action |
|-----|--------|
| `h` `j` `k` `l` / arrows | move cursor |
| `H` `J` `K` `L` | move 8px |
| `0` `$` `gg` `G` | left / right / top / bottom edge |
| `Ctrl-hjkl` | pan the canvas |
| `{` / `}` | smaller / bigger brush |
| `Ctrl-K` | switch studio / overlay layout |
| `i` | **draw** mode — paint as you move |
| `x` | erase pixel (vim's delete-char) |
| `r` / `space` | paint one pixel with fg |
| `b` / `e` | pencil / eraser tool |
| `f` | bucket fill |
| `c` | color picker (eyedropper) |
| `d` / `R` / `o` | line / rectangle / ellipse — move, then `Enter` commits, `Esc` cancels |
| `z` / `a` / `q` | gradient / spray (airbrush) / shade (darken; right-click lightens) |
| `n` / `C` | blur / contour (outline the selection or region under the cursor) |
| `v` | **visual select** (marquee) — `V` selects the whole canvas |
| `w` | **magic wand** — select the same-color region under the cursor |
| `L` / `P` | **lasso** (freehand) / **polygon** (click vertices) selection |
| `y` / `Y` / `x` / `m` / `p` | copy (layer) / copy merged / cut / move · `p` pastes the clipboard |
| `s` | cycle symmetry (none / horizontal / vertical / both / radial) |
| `t` | toggle filled shapes (rect / ellipse) |
| `,` / `.` | previous / next color |
| `<` / `>` | previous / next layer |
| `[` / `]` | previous / next frame |
| `X` | swap fg / bg |
| `u` / `Ctrl-r` | undo / redo |
| `+` / `-` | zoom in / out |
| `:` | command line |
| `?` | help |
| `Ctrl-c` / `Ctrl-x` / `Ctrl-v` | copy / cut / paste (OS clipboard; no selection copies the whole cel) |
| `Ctrl-z` | undo (alias of `u`) |
| `Ctrl-s` / `Ctrl-q` | save / quit |
| `Ctrl-Tab` / `Ctrl-Shift-Tab` | next / previous document tab |
| `Ctrl-t` / `Ctrl-w` | new / close document tab |

## Draw mode (`i`)

`hjkl`/arrows move **and** paint with the active tool (pencil = fg, eraser =
transparent). `Esc` or `q` returns to normal. A whole drag is one undo step.

## Visual mode (`v`), magic wand (`w`) & paste

`v` starts a marquee selection; `hjkl` extends it. `w` selects the contiguous
same-color region around the cursor (active layer). Then `y` copies the active
layer, `Y` copies the flattened selection (all visible layers), `x` cuts and
`m` cuts-and-floats (move). `p` floats the clipboard for pasting — `hjkl`
repositions it, `Enter` commits, `Esc` cancels. Cut and move act on the active
layer's cel; `:copy merged` is the same as `Y`. Every paint tool is clipped to
the selection; paste replaces it, and contour draws its ring just outside it.
`Esc` in normal mode clears the selection.

## Mouse

Left-drag paint fg · right-drag paint bg · middle-click pick · scroll zoom ·
middle-drag pan · drop a file on the window to open it. (The editor is
keyboard-first, but the mouse is fully wired.)

## Commands (`:`)

| Command | Effect |
|---------|--------|
| `:w [file]` / `:save` | save project (JSON `.vpx`) |
| `:q` / `:q!` / `:wq` | quit / force quit / save + quit |
| `:new[!] [w h]` | new sprite (default 32×32); `!` drops unsaved work |
| `:resize w h` | resize canvas (keeps overlap) |
| `:crop [x y w h]` | crop to a rectangle (no args: the selection) |
| `:trim` | crop to the non-transparent content |
| `:newframe` / `:delframe` | add / remove animation frame |
| `:dupframe` | duplicate the current frame (cels + duration) |
| `:framemove <from> <to>` | move a frame (or drag a timeline thumbnail) |
| `:celz <n>` | active cel z-index (+ moves towards the front) |
| `:link <frame>` / `:unlink` / `:preferlink on\|off` | linked cels in the active layer |
| `:movelock on\|off` / `:collapse on\|off` / `:layeruuid [new\|clear]` | layer movement lock, group collapse, UUID |
| `:tag <name> [from to]` | create/update a named frame tag |
| `:tagdel [name]` | delete a frame tag (default: the active one) |
| `:tagdir forward\|reverse\|pingpong\|pingpong-reverse` | play direction of the active tag |
| `:tagrepeat <n>` | repeat count of the active tag (0 = forever) |
| `:newlayer` / `:dellayer` | add / remove layer |
| `:duplayer` | duplicate the active layer (with its cels) |
| `:merge` | merge the active layer into the one below |
| `:flatten` | flatten every visible layer into one |
| `:layerup` / `:layerdown` | move the active layer up / down |
| `:layername <name>` | rename the active layer |
| `:newgroup` | add a layer group |
| `:indent` / `:outdent` | nest the active layer into the group above / un-nest |
| `:lock` / `:unlock` | lock / unlock the active layer (blocks painting) |
| `:reference on\|off` | tracing layer: drawn, never painted or exported |
| `:background on\|off` | the opaque bottom layer |
| `:mask add\|apply\|del` | alpha mask from the selection (or full) / bake / remove |
| `:tile [on\|off]` / `:tile x\|y <1-8>` / `:tile repeats <x> <y>` | seamless tiled preview with configurable repeats; cursor and shapes wrap |
| `:tile opacity <0-255>` / `:tile preview on\|off` | dim or hide neighbour copies |
| `:tileid <n>` / `:tileflip x\|y\|diag\|none` / `:tilemode manual\|auto\|stack\|pixel` | tile brush ID/flips and editing mode |
| `:exporttileset [out.png]` | export the retained tileset atlas |
| `:exportcfg <stem> [--frames a b] [--tag name] [--layers ...] [--merge] [--trim] [--padding n]` | configured sheet/GIF/meta export |
| `:exportcfg <stem> --sequence [--per-layer] [--pattern <tpl>]` | PNG sequence with `{frame}/{layer}/{tag}/{title}/{duration}` |
| `:exportcfg <stem> --layout h\|v\|rows\|columns\|packed [--columns n] [--rows n] [--border n] [--shape n] [--inner n] [--extrude n]` | sheet layout, padding and extrusion |
| `:exportcfg <stem> --atlas modaline\|hash\|array [--gif-loop n] [--gif-colors n] [--gif-dither]` | atlas metadata and GIF options |
| `:celselect <layer> <frame>` / `:celclear` / `:exportcels <dir>` | multi-cel selection and per-cel PNG export |
| `:celcopy` / `:celcut` / `:celpaste` / `:celdel` / `:celinfo` | whole-cel clipboard, empty cel vs frame, cel status |
| `modaline --export-tileset in.ase [out.png]` | headless atlas export |
| `:tilepick` / `:tileerase` | pick the tile under the cursor / place the empty tile |
| `:fliph` / `:flipv` / `:rot90` | mirror H / V / rotate 90° (selection if any, else whole sprite) |
| `:sym none\|h\|v\|both\|radial [axes]` | set symmetry (mirror painting) |
| `:symcenter cursor\|reset\|<x> <y>` | radial symmetry centre |
| `:wand <0-255>` / `:wandall on\|off` | wand + bucket tolerance / match all layers |
| `:contig on\|off` | wand and bucket: contiguous region or every matching pixel |
| `:selop replace\|add\|subtract\|intersect` | how new selections combine |
| `:sel expand\|contract\|border\|feather <n>` / `:sel invert\|none\|all` | selection shape ops |
| `:sel color [#hex] [tol]` / `:sel alpha [all]` | colour-range or opaque-pixel selection |
| `:sel rotate\|skew <deg>` / `:sel scale <sx> [sy]` / `:sel offset <dx> <dy>` | transform the selection mask only |
| `:fill [#hex]` / `:stroke [width] [inside\|outside] [#hex]` | fill the selection or paint its edge |
| `:grad linear\|radial` | gradient mode |
| `:filter <name> [args]` | invert, brightness, contrast, hue, saturation, desaturate, curves, levels, balance, posterize, outline, blur, sharpen, median, convolve, emboss |
| `:rotate <deg>` / `:scale <sx> [sy]` / `:skew <deg>` | free-transform the selection (or the cel) |
| `:rotsprite on\|off` / `:pivot <x> <y>\|clear` | supersampled rotation and a custom transform pivot |
| `:text <string>` | stamp 8x8 bitmap text at the cursor |
| `:textobj <string>` | create/edit an editable text object (stored in cel user data) |
| `:textfont modaline` / `:textsize <8-64>` / `:textalign left\|center\|right` | text tool font, size and alignment |
| `:grid <px>` / `:snap [on\|off]` | design grid and snapping |
| `:brush <1-64>` / `:brush square\|circle <n>` / `:brush spacing <pct>` | brush size, built-in shape, stamp spacing |
| `:brush load\|save\|use\|list [name]` | custom PNG brush and the `modaline/brushes` library |
| `:pressure <0-100>` / `:pressure auto` | manual pressure override; `auto` follows pen force |
| `:spray <dots> [radius]` / `:shade <pct>` / `:blur <radius>` | airbrush, shade, and blur-tool depth |
| `:contour [width] [inside\|outside]` | contour the cursor region with the chosen ring |
| `:grad preset <name>` / `:grad stop <pct> [#hex]` / `:grad stops` / `:grad clear` | multi-stop gradient presets and stops |
| `:grad dither none\|bayer2\|bayer4\|bayer8` | ordered dithering while painting a gradient |
| `:ink simple\|lockalpha\|replace\|shading\|erase` / `:alphalock on\|off` | per-tool ink and alpha-locked painting |
| `:perfect on\|off` | pixel-perfect freehand: drop L-shaped corner pixels |
| `:mode auto\|rgba\|grayscale\|indexed` / `:aspect <x> <y>` / `:sgrid [x y] <w> <h>` | sprite colour mode, pixel ratio, grid size + origin |
| `:dither none\|bayer2\|bayer4\|bayer8\|floyd` | dithering for indexed conversion |
| `:profile srgb\|linear` / `:profile icc <file>` / `:profile gamma <g>` / `:profile clear` | working space / embedded `.ase` colour profile |
| `:renderer glow\|wgpu` | renderer (applied on next launch) |
| `:theme [arcane\|dark\|light\|midnight\|paper\|file.json]` / `:theme save <file>` | apply/list/save a JSON theme |
| `:uifont <font.ttf\|silkscreen\|default> [8-32]` | UI font and size (saved in the config) |
| `:layout [default\|drawing\|animation\|compact\|overlay\|file.json]` / `:layout reset\|save <file>` | workspace panels and sizes |
| `:bind <action> <chord\|wheel-up\|wheel-down\|none>` / `:bind list\|reset` | customise action keys and wheel |
| `:lang [en\|pt-BR\|file.json]` / `:lang list\|check\|save <file>` | runtime language and JSON catalogs |
| `:prefs` | preferences window (editor, grid, export, appearance, keys, workspace) |
| `:motion on\|off` | hover lift and short transitions |
| `:bgcolor [#rrggbb\|clear]` | sprite-properties background colour (user data) |
| `:script <file.lua>` / `:lua <code>` | run sandboxed Lua with the `app` API |
| `:ext install\|list\|remove\|run\|themes\|theme` | `.aseprite-extension` packages, scripts and themes |
| `:fg #rrggbb` / `:bg #rrggbb` | set fg / bg exactly |
| `:pal load\|save <file>` / `:pal move <from> <to>` / `:pal from-sprite` / `:pal lock on\|off` | palette I/O, reorder, derive from the frame, lock |
| `:pal preset <name>` / `:pal insert <i> [#hex]` / `:pal delete <i>` / `:pal sort [rgb\|luma\|hue]` / `:pal name <i> <name>` | presets, index edits, sort, names |
| `:slices [<dir> [cols rows\|named]]` | list slices, export a grid, or one PNG per named slice + `.slices.json` |
| `:ops <file.json>` | replay a JSON op list as one undo step |
| `:export out.png\|out.ase\|<stem> [scale]` | PNG frame · `.ase` sprite · bare stem = sheet + GIF + meta JSON; `[scale 1-16]` upscales rasters |
| `:import[!] in.png` | import a PNG as a new sprite; `!` drops unsaved work |
| `:importsheet[!] sheet.png [cols rows]` | rebuild frames from a sheet + its `.json` sidecar |
| `:importatlas sheet.png [meta.json\|xml]` | Aseprite/TexturePacker JSON or XML atlas |
| `:importseq <dir>` | image folder as frames in natural order |
| `:open[!] proj.vpx\|proj.ase\|in.png` | load a project / `.ase` / PNG (by extension); `!` drops unsaved work |
| `:recover [file]` | load the autosave snapshot (`recovery.vpx`) as unsaved work |
| `:recent [n]` | list the last 10 opened/saved files, or open the nth |

## Pen and tablet

Wacom tablets and OpenTabletDriver virtual tablets add tip pressure through
Linux evdev. Position and clicks still come from the OS pointer, so the same
actions work with the pen as with the mouse. `:pen` shows the detected device,
`:pen off` ignores it, and the side-panel checkbox toggles it. Setup and
troubleshooting: [`PEN.md`](PEN.md).
