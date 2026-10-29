mod app;
mod cli;
mod config;
mod extension;
mod gui;
mod input;
mod keymap;
mod theme;

use std::path::PathBuf;

use eframe::egui;
// The model lives in the library crate now. These names are what the shell
// modules above reach through their `crate::…` paths.
use modaline::{ase, brush, color, editor, i18n, io, layout, ops, palette, script, sprite};

/// The backend this build can actually run. A `wgpu` request on a glow-only
/// build is a warning, not a crash: losing the setting is worse than losing the
/// editor.
fn pick_renderer(want_wgpu: bool) -> eframe::Renderer {
    #[cfg(feature = "wgpu")]
    {
        if want_wgpu {
            return eframe::Renderer::Wgpu;
        }
    }
    #[cfg(not(feature = "wgpu"))]
    if want_wgpu {
        eprintln!("modaline was built without the wgpu feature; using glow");
    }
    let _ = want_wgpu;
    eframe::Renderer::Glow
}

fn main() -> eframe::Result {
    // Headless tileset export: `modaline --export-tileset in.ase [out.png]`.
    let argv: Vec<String> = std::env::args().collect();
    if argv.iter().any(|a| a == "-b" || a == "--batch") {
        if let Err(e) = cli::run(&argv) {
            eprintln!("modaline: {e:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(pos) = argv.iter().position(|a| a == "--export-tileset") {
        let Some(input) = argv.get(pos + 1).map(PathBuf::from) else {
            eprintln!("usage: modaline --export-tileset <file.ase|.vpx> [out.png]");
            std::process::exit(2);
        };
        let sprite = match input.extension().and_then(|e| e.to_str()) {
            Some("vpx") => io::load_project(&input),
            _ => ase::load(&input),
        };
        let sprite = match sprite {
            Ok(s) => s,
            Err(e) => {
                eprintln!("export-tileset: {e:#}");
                std::process::exit(1);
            }
        };
        let output = argv
            .get(pos + 2)
            .map(PathBuf::from)
            .unwrap_or_else(|| input.with_extension("tileset.png"));
        match io::export_tileset(&sprite, None, &output) {
            Ok(()) => {
                println!("{}", output.display());
                return Ok(());
            }
            Err(e) => {
                eprintln!("export-tileset: {e:#}");
                std::process::exit(1);
            }
        }
    }
    // Optional first argument: the file to open, so another tool can launch
    // `modaline assets/fixes/<look>.vpx` instead of making the user press `:open`.
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .filter(|a| !a.to_string_lossy().starts_with("--"))
        .map(PathBuf::from);
    // The window opens at the size it was last left at.
    let saved = config::Config::load();
    // `--wgpu` or `renderer: wgpu` in the config picks the backend. The drawing
    // goes through egui::TextureHandle only, so nothing else changes.
    let want_wgpu = std::env::args().any(|a| a == "--wgpu")
        || saved.renderer.eq_ignore_ascii_case("wgpu")
        // A hand-edited `extra` entry from before the field existed.
        || saved.extra.get("renderer").is_some_and(|r| r == "wgpu");
    let renderer = pick_renderer(want_wgpu);
    let size = (
        saved.window.0.clamp(640.0, 7680.0),
        saved.window.1.clamp(480.0, 4320.0),
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([size.0, size.1])
            .with_title("modaline"),
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "modaline",
        options,
        Box::new(move |cc| Ok(Box::new(app::ModalineApp::new(cc, path)))),
    )
}
