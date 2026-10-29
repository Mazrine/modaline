# Functional tests, headless

How to drive the real GUI and check what it did, without a display of your own.
Everything below was run on 2026-10-26 and passed; the screenshots it produced
are the evidence.

## Why this works (and what made it fail before)

X11 with **no window manager** never sets input focus, so `xdotool key` and
`xdotool type` go nowhere: clicks paint, keys vanish. The fix is one line — run
`twm` on the display. With it, `xdotool windowfocus` + `xdotool type` reach the
app and the whole keyboard surface (commands, tools, `:text`) is testable.

## The rig

```sh
LIBS=$(nix build --print-out-paths --no-link nixpkgs#libx11 nixpkgs#libxcb \
  nixpkgs#libXcursor nixpkgs#libXrandr nixpkgs#libXi nixpkgs#libXext \
  nixpkgs#libXrender nixpkgs#libGL nixpkgs#libxkbcommon nixpkgs#wayland \
  2>/dev/null | grep -v -- '-man$' | sed 's|$|/lib|' | tr '\n' ':')

cat > drive.sh <<'SH'
export DISPLAY=:95 WINIT_UNIX_BACKEND=x11 WAYLAND_DISPLAY=
export HOME=/tmp/opencode/vhome          # a throwaway config dir
Xvfb :95 -screen 0 1400x900x24 >/dev/null 2>&1 &
sleep 1
nix run nixpkgs#twm >/dev/null 2>&1 &    # gives the window keyboard focus
sleep 1
target/debug/vixel > log.txt 2>&1 &
sleep 9
WID=$(xdotool search --name '^vixel$' | head -1)
xdotool windowfocus $WID; xdotool windowraise $WID
sleep 1

xdotool key colon; xdotool type 'text HI'; xdotool key Return
sleep 1
import -window root shot.png
SH

nix shell nixpkgs#xdotool -c sh -c "export LD_LIBRARY_PATH=$LIBS; bash drive.sh"
```

Rules that cost me a run each:

- **Never** reuse a display that already has an Xvfb on it: the new client joins
  the old one, whose window manager is gone, and every key silently dies.
- `windowactivate` is EWMH; `twm` does not implement it. Use `windowfocus`.
- One `mousemove` then `mousedown`/`mouseup` does not drag in egui. Move, sleep
  ~0.3 s, move again, then release.
- A slider needs a drag with intermediate positions; a single click on the track
  does nothing.
- A freshly added layer list grows downwards; scroll the side panel (it scrolls
  now, see `feat(ui): the side panel scrolls`) before clicking a row.

## What the suite checks, and what "pass" looked like

| step | command / gesture | evidence |
|---|---|---|
| text tool | `:text HI` | glyphs appear at the cursor; the panel's text field shows `HI` |
| radial symmetry | `:sym radial 6`, then click off-centre | six dots around the centre, six guide lines |
| free transform | `:rotate 90` | the glyphs stand on their side |
| filters | `:filter outline 1` | status `filter: outline 1 (69 px)`, each dot grows into a plus |
| selection shape | `:sel all`, `:sel contract 3` | status `selection: contract 3`, marquee inset by 3 |
| slice export | `:slices /tmp/opencode/sl 2 2` | status `slices: 5 files`, four 16x16 PNGs + `sprite.slices.json` |
| layer drag | drag row 1 onto row 3 | status `layer 0 -> 2`, the row order changed |
| onion skin | 3 frames, different glyph each, frame 0 active | neighbours show as red/green ghosts |
| playback | click `play` | the canvas content changes between screenshots at 0.4 s and 0.9 s |
| config | `:renderer wgpu`, quit, relaunch | `config.json` has `"renderer": "wgpu"`; a glow-only build logs `built without the wgpu feature; using glow`, a `--features wgpu` build initialises EGL instead |
| tabs | `Ctrl-t`, screenshot; `Ctrl-Tab`, screenshot | two `untitled` tabs, active marker moves; the strip shows `×` and `+`; `Ctrl-w` closes (dirty tabs refuse) |
| layouts | `:layout animation`, screenshot; `:layout reset`, screenshot | animation hides the side panel and gives a 210 px timeline; reset restores side + 120 px; `:layout save` writes JSON |
| keymap | `:bind undo ctrl-z`, draw + `u` + `ctrl-z`; `:bind help h`, `h`; `:bind zoom-in wheel-down`, scroll | `u` does nothing, ctrl-z undoes; help opens on `h`; scroll zooms once; config shows the bindings |

## Checking a screenshot

Crop the part you care about and look at it: the status line is at `y=28`,
width ~700; the side panel starts at `x=820` on a 1280-wide window. Messages
there are the cheapest assertion — every command writes what it did.
