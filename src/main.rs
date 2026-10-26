mod app;
mod config;
mod gui;
mod input;

use std::path::PathBuf;

use eframe::egui;
// The model lives in the library crate now. These names are what the shell
// modules above reach through their `crate::…` paths.
use vixel::{ase, color, editor, io, ops, palette, sprite};

fn main() -> eframe::Result {
    // Optional first argument: the file to open, so another tool can launch
    // `vixel assets/fixes/<look>.vpx` instead of making the user press `:open`.
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    // The window opens at the size it was last left at.
    let saved = config::Config::load();
    let size = (
        saved.window.0.clamp(640.0, 7680.0),
        saved.window.1.clamp(480.0, 4320.0),
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([size.0, size.1])
            .with_title("vixel"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "vixel",
        options,
        Box::new(move |cc| Ok(Box::new(app::VixelApp::new(cc, path)))),
    )
}
