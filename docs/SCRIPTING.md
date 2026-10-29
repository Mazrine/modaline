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

## API compatibility matrix (P5-50, 2026-10-29)

Representative scripts are covered by `src/script.rs` tests and the CLI
integration test.

| Area | Supported | Status / gaps |
|---|---|---|
| Lua language | Lua 5.4, base/table/string/math/utf8 | Covered; no io/os/package/debug/dofile/loadfile/require (sandbox) |
| `app` | `version`, `alert`, `activeSprite`, `activeLayer`, `activeFrame` | Covered (active objects are live views where documented) || `app.pixelColor` | `rgba`, `rgb`, `gray`, `grayA`, `rgbaR/G/B/A` | Covered; color tables (integer packed also accepted by channel getters) |
| `app.transaction(fn)` | Runs the callback and returns its value | Covered (undo is still one script step) |
| `app.command` | `Undo`, `Redo`, `NewLayer`, `NewFrame` | Covered; broader command set missing |
| `Sprite` | width/height/frameCount/layerCount/activeLayer/activeFrame | Covered (read-only) |
| `Sprite` methods | `resize`, `newLayer`, `newFrame`, `getPixel`, `setPixel`, `save`, `saveAs`, `flatten`, `crop`, `newTag`, `setFrameDuration`, `frameDuration`, `cel`, `setCelImage` | Partial: `newLayer`/`newFrame` return a 1-based index, not Aseprite userdata; pixel methods are a Modaline extension to the sprite |
| `Layer` | name/visible/opacity/editable/isBackground/isReference | Partial: no `cel`, `newCel`, groups or blend setters |
| `Image` | `Image(w,h)`, `getPixel`, `setPixel`, `clear([color])`, `width`, `height`, `bounds` | Covered (detached pixel buffer) |
| `Cel` | `layer`, `frame`, `bounds`, `image`, `isEmpty` via `sprite:cel(l, f)` | Covered as a snapshot: `image` is a copy; write back with `sprite:setCelImage(l, f, img)` |
| `Selection` | `app.selection.bounds`, `.isEmpty`, `:add{...}`, `:subtract{...}`, `:clear()` | Covered (axis-aligned rectangles over the sprite mask) |
| `Dialog` | `Dialog{title=...}` with `:label/:entry/:check/:button/:show`; `app.confirm`, `app.prompt` | Covered headless: no window opens; `show` returns declared entry/check defaults, confirm is always false, prompt echoes its default |
| `app.fs` | `read`, `write`, `exists` under the project root | Covered sandbox: root is `:scriptroot <dir>` or the open file's folder; absolute paths, `..` and symlink escapes are rejected; no root denies all access |

## File access and dialogs (P6-02)

`app.fs` is the only filesystem surface. The root is `:scriptroot <dir>` when
set, otherwise the folder of the open document; a document with no path (a new
sprite) and no explicit root denies every call. Paths must be relative and stay
inside the root: absolute paths, `..` components and symlinks that resolve
outside it are rejected. `:scriptroot clear` removes an explicit root (the open
document's folder still applies).

Scripts execute synchronously on the UI thread, so dialogs are headless:
`app.alert` prints, `app.confirm` records the message and returns `false`,
`app.prompt` records the message and returns its default, and
`Dialog{...}:show()` returns the `:entry`/`:check` defaults while recording the
title. A script that needs a real answer must be driven from a configured value,
not from a modal.

## Extensions (P5-51)

`.aseprite-extension` zips are installed with `:ext install <file>`; the manifest
supports `name`, `displayName`, `version`, `author` (string or `{name}`),
`description` and `contributes.scripts`/`contributes.themes`. Installs reject
absolute/`..` paths, more than 500 files, files over 10 MiB, or more than 50 MiB
total. `:ext run <id>` executes contributed scripts in the sandbox; `:ext theme
<id> [n]` applies a Modaline JSON theme from the package.

## Availability check (2026-10-28; resolved 2026-10-29)

`mlua`, `rhai`, `rlua`, `quickjs`, `pyo3` and RustPython are absent from both
`~/.cargo/registry/src` and `~/.cargo/registry/cache`; there is no `mlua` entry
in `Cargo.lock`. `cargo` therefore cannot add the chosen dependency in this
offline environment.

**Resolved:** the registry became reachable on 2026-10-29; `mlua 0.12` (Lua 5.4,
vendored) was added, and P5-49/P5-50 landed. `zip 9` was added for P5-51.

## Residual risks

- Aseprite scripts use host-specific globals (`Dialog`, `app.transaction`,
  `Image:drawPixel`). A subset may never map cleanly; document per-call gaps in
  the parity matrix rather than claiming full compatibility.
- Lua's C ABI adds a build dependency; `vendored` keeps it reproducible but
  increases compile time.
- Sandbox limits can break legitimate long-running scripts; expose limits as
  configurable (with conservative defaults) in P5-49.
