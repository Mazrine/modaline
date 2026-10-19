mod app;
mod ase;
mod color;
mod editor;
mod gui;
mod input;
mod io;
mod sprite;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("vixel"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "vixel",
        options,
        Box::new(|cc| Ok(Box::new(app::VixelApp::new(cc)))),
    )
}
