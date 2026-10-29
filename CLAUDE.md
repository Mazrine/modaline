# CLAUDE.md

Claude Code entry point. The project guidance lives in [`AGENTS.md`](AGENTS.md);
keep that file as the single source of truth (architecture, invariants, egui
0.37 gotchas, commands).

Quick commands:

```sh
nix develop                                  # NixOS: runtime libs
cargo test && cargo clippy --all-targets
cargo run --release
nix run github:Mazrine/modaline -- art.ase
```
