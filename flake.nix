{
  description = "vixel — a keyboard-first, vim-style pixel art editor";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      # winit/glow dlopen these at runtime; keep them on LD_LIBRARY_PATH so
      # `cargo run` works without steam-run.
      runtimeLibs = with pkgs; [
        wayland
        libxkbcommon
        libGL
        libx11
        libxcb
        libxcursor
        libxrandr
        libxi
        libxext
        libxrender
      ];
      libPath = pkgs.lib.makeLibraryPath runtimeLibs;
    in {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [ cargo rustc rustfmt clippy pkg-config ];
        buildInputs = runtimeLibs;
        LD_LIBRARY_PATH = libPath;
        shellHook = ''
          # Prefer the host's Rust when it exists so the existing target/
          # cache (same compiler) stays valid; fall back to nixpkgs Rust.
          for p in "/etc/profiles/per-user/$USER/bin" /run/current-system/sw/bin; do
            if [ -x "$p/cargo" ]; then
              export PATH="$p:$PATH"
              break
            fi
          done
          echo "vixel devShell — run: cargo run --release"
        '';
      };

      formatter.${system} = pkgs.nixfmt-rfc-style;
    };
}
