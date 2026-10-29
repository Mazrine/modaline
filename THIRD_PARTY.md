# Third-party notices

Modaline is distributed under the GNU General Public License v3.0 or later (see
`LICENSE`). This file lists the third-party material that is **included in the
repository or the built binary**, separate from normal Cargo dependencies
(which are fetched by the build and keep their own licenses).

## Included test fixtures

`tests/fixtures/ase/*.ase` — MIT, Copyright (c) 2026 Barış, from
<https://github.com/setanarut/aseprite>. The full MIT notice is in
`tests/fixtures/README.md`. Test data only.

## Included data

| Item | Where | License / attribution |
|---|---|---|
| Sweetie 16 palette | `src/color.rs` default palette | CC0 by GrafxKid — <https://lospec.com/palette-list/sweetie-16> |
| `font8x8` glyph data | bitmap text tool | MIT, Copyright (c) 2017 Joaquin Rosales |
| egui default fonts | UI, via the `epaint_default_fonts` crate | OFL-1.1 and Ubuntu-font-1.0 (font data), MIT/Apache-2.0 (code) |

## Direct dependencies

All are permissive and GPL-3.0-compatible. The exact versions are in
`Cargo.lock`; each crate ships its own license text.

| Crate | License |
|---|---|
| anyhow, eframe, flate2, image, serde, serde_json | MIT OR Apache-2.0 |
| evdev | Apache-2.0 OR MIT |
| flic, mlua, quick-xml, zip | MIT |
| pcx | MIT OR Apache-2.0 OR WTFPL |
| font8x8 | MIT |

Transitive dependencies (432 crates at the time of writing) are likewise
permissive: MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, OFL-1.1
and Ubuntu-font-1.0. No GPL-only or AGPL dependency is used.

For a binary release, generate a complete notice file with `cargo about` or
`cargo deny` from the lockfile and ship it next to the executable.

## Trademarks

Aseprite is a trademark of its respective owner (Igara Studio S.A.). Modaline is
an independent project, not affiliated with or endorsed by Aseprite. The name
is used only to describe file-format and feature compatibility.
