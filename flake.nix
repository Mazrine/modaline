{
  description = "Modaline — a keyboard-first, modal pixel art editor";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      # winit/glow dlopen these at runtime; keep them on LD_LIBRARY_PATH so
      # both `nix run` and `cargo run` work without steam-run.
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
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "modaline";
        version = "0.1.0";
        src = self;
        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [ makeWrapper pkg-config ];
        buildInputs = runtimeLibs;

        postInstall = ''
          wrapProgram $out/bin/modaline \
            --prefix LD_LIBRARY_PATH : "${libPath}"
        '';

        meta = with pkgs.lib; {
          description = "A keyboard-first, modal pixel art editor";
          homepage = "https://github.com/Mazrine/modaline";
          license = licenses.gpl3Plus;
          mainProgram = "modaline";
          platforms = [ "x86_64-linux" ];
        };
      };

      apps.${system}.default = {
        type = "app";
        program = "${self.packages.${system}.default}/bin/modaline";
      };

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
          echo "modaline devShell — run: cargo run --release"
        '';
      };

      formatter.${system} = pkgs.nixfmt-rfc-style;
    };
}
