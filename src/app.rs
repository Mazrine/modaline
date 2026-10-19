use std::time::{Duration, Instant};

use eframe::egui;

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
}

impl VixelApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Self {
            editor: Editor::new(32, 32),
            view: View::new(),
            texture: None,
            onion_prev: None,
            onion_next: None,
            playing: false,
            next_frame: Instant::now(),
        }
    }
}

impl eframe::App for VixelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        input::handle_input(&mut self.editor, &mut self.view, ui);

        if self.playing {
            let now = Instant::now();
            if now >= self.next_frame {
                let n = self.editor.sprite.frames.len();
                let dur = self.editor.sprite.frames[self.editor.active_frame].duration_ms as u64;
                self.editor.active_frame = (self.editor.active_frame + 1) % n.max(1);
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
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
