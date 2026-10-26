use std::time::{Duration, Instant};

use eframe::egui;

use crate::config::Config;
use crate::editor::Editor;
use crate::gui::{self, View};
use crate::input;

pub struct VixelApp {
    editor: Editor,
    view: View,
    texture: Option<egui::TextureHandle>,
    onion_prev: Option<egui::TextureHandle>,
    onion_next: Option<egui::TextureHandle>,
    playing: bool,
    next_frame: Instant,
    /// Last title sent to the window manager, so a fix bundle names its part.
    title: String,
    /// Settings loaded at start; written back on exit and on `:wq`.
    config: Config,
}

impl VixelApp {
    /// `path` is the optional file from the command line. A file that will not
    /// load is reported in the status line instead of blocking startup: a bad
    /// argument must still give you an editor.
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<std::path::PathBuf>) -> Self {
        let config = Config::load();
        cc.egui_ctx.set_visuals(if config.theme == "light" {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        });
        let mut editor = Editor::new(32, 32);
        editor.grid = config.grid.max(1);
        editor.snap = config.snap;
        editor.play_loop = config.play_loop;
        editor.play_pingpong = config.play_pingpong;
        editor.renderer = config.renderer.clone();
        let mut view = View::new();
        view.zoom = config.zoom.clamp(1, 64);
        view.show_grid = config.show_grid;
        view.onion = config.onion.into();
        view.theme = config.theme.clone();
        if let Some(path) = path
            && let Err(e) = editor.open_path(&path)
        {
            editor.msg = format!("open failed: {e}");
        }
        Self {
            editor,
            view,
            texture: None,
            onion_prev: None,
            onion_next: None,
            playing: false,
            next_frame: Instant::now(),
            title: "vixel".into(),
            config,
        }
    }

    /// Snapshot the live settings into the config and write it. Called on quit
    /// and whenever the window is closed by the manager.
    fn save_config(&mut self) {
        self.config.window = self.view.window;
        self.config.zoom = self.view.zoom;
        self.config.show_grid = self.view.show_grid;
        self.config.grid = self.editor.grid;
        self.config.snap = self.editor.snap;
        self.config.onion = self.view.onion.into();
        self.config.play_loop = self.editor.play_loop;
        self.config.play_pingpong = self.editor.play_pingpong;
        self.config.theme = self.view.theme.clone();
        self.config.renderer = self.editor.renderer.clone();
        let _ = self.config.save();
    }
}

impl eframe::App for VixelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let title = match &self.editor.bundle_stage {
            Some(stage) => format!("vixel — {stage} fix"),
            None => "vixel".to_string(),
        };
        if title != self.title {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        input::handle_input(&mut self.editor, &mut self.view, ui);

        if self.playing {
            let now = Instant::now();
            if now >= self.next_frame {
                let dur = self.editor.sprite.frames[self.editor.active_frame].duration_ms as u64;
                if !self.editor.advance_frame() {
                    self.playing = false;
                }
                self.next_frame = now + Duration::from_millis(dur.max(16));
            }
            ui.ctx().request_repaint();
        }

        gui::draw(
            ui,
            &mut self.editor,
            &mut self.view,
            &mut self.texture,
            &mut self.onion_prev,
            &mut self.onion_next,
            &mut self.playing,
        );

        if self.editor.should_quit {
            self.save_config();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
    }
}
