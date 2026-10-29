# Examples

- `demo.vpx` — the three-frame sunset scene used by the README screenshots and
  GIF. Open it with `modaline examples/demo.vpx`.
- `build_demo.lua` — regenerates the scene through the scripting API.

Rebuild the project from scratch (any input image works; the script resizes the
canvas):

```sh
nix shell nixpkgs#imagemagick -c \
  magick -size 96x64 xc:none /tmp/blank.png
cargo run --release -- -b /tmp/blank.png \
  --script examples/build_demo.lua --save-as examples/demo.vpx
```

The README GIF is exported by Modaline itself:

```sh
cargo run --release -- -b examples/demo.vpx --gif assets/demo.gif \
  --gif-loop 0 --gif-colors 16 --scale 4
```

The screenshots are Xvfb captures of the app with this project open; see
`docs/TESTING.md` for the rig.
