# Scripting engine decision (P5-48)

Status: **implemented MVP** — Lua 5.4 via `mlua` (vendored); P5-49 landed
2026-10-29. P5-50 grows API coverage.

## Requirement

Aseprite's value is not just "some scripting" — it is the **Aseprite Lua API and
the existing scripts people wrote for it** (`app`, `Sprite`, `Layer`, `Cel`,
`Image`, `Color`, `Selection`, commands, dialogs, palettes, tags, slices,
tilesets). Any engine that cannot run those scripts solves a different problem.

Secondary requirements:

- sandboxable (no ambient filesystem/network access unless explicitly granted),
- bounded memory and execution time so a bad script cannot hang the editor,
- cross-platform, with no runtime service to install,
- buildable in this repository without requiring a GUI/toolchain not already
  present.

## Comparison

| Option | Aseprite compatibility | Sandbox | Build/cost | Offline here | Verdict |
|---|---|---|---|---|---|
| **Lua / `mlua` (Lua 5.4, vendored)** | **Direct** — same language and API surface; port the Aseprite API | Good: no `io`/`os`/`package` by default, memory limit + instruction hook, register only permitted functions | One small C library (`vendored` builds it with the existing C toolchain); no runtime service | **No crate/source cached**, Cargo cannot resolve it offline | **Chosen** |
| Rhai | None — Rust-native language; every Aseprite script must be rewritten | Excellent (pure Rust, no unsafe) | Easy to build | Not cached either | Fallback **only if** script compatibility is abandoned |
| QuickJS / WASM | None — JS/WASM engines; incompatible | Excellent (explicit imports only) | Heavy: embeds a JS engine + WASM toolchain | Not cached | Reject for compatibility |
| Python (PyO3 / RustPython) | None — Aseprite has no Python API | PyO3 exposes a large host surface; RustPython is incomplete | PyO3 needs CPython; RustPython is large | Not cached | Reject |
| JSON `Op` replay (current) | Not an Aseprite API | Complete (data-only) | Already shipped | n/a | Keep as deterministic automation, not a scripting API |

## Decision

- **Engine:** `mlua = "0.10"` with `features = ["lua54", "vendored", "serialize"]`.
  Aseprite 1.3.x uses Lua 5.4; `vendored` avoids depending on a system Lua.
- **Why not Rhai:** it would be pleasant, but a Rhai script cannot run an
  Aseprite script. The backlog's acceptance criterion is Aseprite compatibility.
- **Sandbox plan for P5-49:**
  - fresh `Lua` per script; open only base/table/string/math/utf8;
  - explicitly omit `io`, `os`, `package`, `debug` and `dofile`/`loadfile`;
  - `set_memory_limit` (default 64 MiB) and a `set_hook` instruction budget
    (default ~50 M instructions) per run;
  - host API registered as plain Rust functions returning errors, no raw
    pointers;
  - file access only through explicit `app.fs`-style helpers that resolve paths
    against an allow-listed project directory (not the process CWD).
- **API plan for P5-49/P5-50:** expose `app` (version, active sprite, commands,
  alert), `Sprite` (size, layers, frames, cels, tags, slices, palette,
  selection), `Layer`/`Cel` properties, `Image` pixel get/set + drawing
  primitives, and `Color`. Build the compatibility matrix in
  `docs/ASEPRITE_PARITY.md` by running representative Aseprite scripts
  (palette cycling, layer export, tilemap inspection, selection fill) against
  every documented method.

## Availability check (2026-10-28)

`mlua`, `rhai`, `rlua`, `quickjs`, `pyo3` and RustPython are absent from both
`~/.cargo/registry/src` and `~/.cargo/registry/cache`; there is no `mlua` entry
in `Cargo.lock`. `cargo` therefore cannot add the chosen dependency in this
offline environment.

**Consequence:** P5-49 (scripting MVP) and P5-50 (API coverage) are **Blocked**
until the crate can be fetched (network/cache available) or a vendored copy is
added. This is a dependency availability block, not a design change: the engine
decision stands, and no alternative engine was selected merely because it could
build offline.

## Residual risks

- Aseprite scripts use host-specific globals (`Dialog`, `app.transaction`,
  `Image:drawPixel`). A subset may never map cleanly; document per-call gaps in
  the parity matrix rather than claiming full compatibility.
- Lua's C ABI adds a build dependency; `vendored` keeps it reproducible but
  increases compile time.
- Sandbox limits can break legitimate long-running scripts; expose limits as
  configurable (with conservative defaults) in P5-49.
