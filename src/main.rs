mod app;
mod config;
mod gui;
mod input;

use std::path::PathBuf;

use eframe::egui;
// The model lives in the library crate now. These names are what the shell
// modules above reach through their `crate::…` paths.
use vixel::{ase, color, editor, io, ops, palette, sprite};

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
        eprintln!("vixel was built without the wgpu feature; using glow");
    }
    let _ = want_wgpu;
    eframe::Renderer::Glow
}

fn main() -> eframe::Result {
    // Optional first argument: the file to open, so another tool can launch
    // `vixel assets/fixes/<look>.vpx` instead of making the user press `:open`.
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
        || saved.extra.get("renderer").is_some_and(|r| r == "wgpu");
    let renderer = pick_renderer(want_wgpu);
    let size = (
        saved.window.0.clamp(640.0, 7680.0),
        saved.window.1.clamp(480.0, 4320.0),
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([size.0, size.1])
            .with_title("vixel"),
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "vixel",
        options,
        Box::new(move |cc| Ok(Box::new(app::VixelApp::new(cc, path)))),
    )
}
