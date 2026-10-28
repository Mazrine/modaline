use std::time::{Duration, Instant};

use eframe::egui;

use crate::config::Config;
use crate::editor::Editor;
use crate::gui::{self, View};
use crate::input;
use crate::io;

/// How often a dirty sprite is snapshotted to the recovery file.
const AUTOSAVE: Duration = Duration::from_secs(30);

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
    /// Next periodic autosave while the sprite is dirty.
    next_autosave: Instant,
    /// Previous frame's dirty flag, so a successful save can drop the snapshot.
    was_dirty: bool,
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
        editor.recent = config.recent.iter().map(std::path::PathBuf::from).collect();
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
        // A snapshot from a crashed session is offered, never loaded silently:
        // it may be older than the file the user just opened.
        if crate::config::recovery_path().is_some_and(|p| p.exists())
            && !editor.msg.starts_with("open failed")
        {
            editor.msg = "recovery from a previous session (:recover)".into();
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
            next_autosave: Instant::now() + AUTOSAVE,
            was_dirty: false,
        }
    }

    /// Write the dirty sprite to the recovery file. Best-effort: a read-only
    /// config directory must not break the editor.
    fn snapshot(&mut self) {
        let Some(path) = crate::config::recovery_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        self.editor.msg = match io::save_project(&self.editor.sprite, &path) {
            Ok(()) => format!("autosaved ({})", path.display()),
            Err(e) => format!("autosave failed: {e}"),
        };
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
        self.config.recent = self
            .editor
            .recent
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let _ = self.config.save();
    }
}

/// Drop the crash-recovery snapshot. Called when the work is safely saved or
/// explicitly discarded.
fn clear_recovery() {
    if let Some(path) = crate::config::recovery_path() {
        let _ = std::fs::remove_file(path);
    }
}

impl eframe::App for VixelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Dropping a file onto the window opens it, with the same dirty guard
        // as `:open`: a drop must not throw away a stroke.
        let dropped = ui
            .ctx()
            .input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            if self.editor.dirty {
                self.editor.msg = "unsaved changes — drop ignored (use :w or :q!)".into();
            } else if let Err(e) = self.editor.open_path(&path) {
                self.editor.msg = format!("open failed: {e}");
            }
        }
        // A window-manager close bypasses `:q`, so hold the window open while
        // there are unsaved edits. `:q!` / `:wq` set `should_quit`, which lets
        // the close through.
        if ui.ctx().input(|i| i.viewport().close_requested())
            && self.editor.dirty
            && !self.editor.should_quit
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.editor.msg = "unsaved changes (use :q! to force or :w to save)".into();
        }
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

        // Snapshot a dirty sprite periodically. A successful save or a forced
        // quit drops the snapshot, so a clean session never resurrects it.
        if self.editor.dirty && Instant::now() >= self.next_autosave {
            self.next_autosave = Instant::now() + AUTOSAVE;
            self.snapshot();
        }
        if self.was_dirty && !self.editor.dirty {
            clear_recovery();
        }
        self.was_dirty = self.editor.dirty;
        // eframe sleeps while idle. Without an explicit wakeup the snapshot
        // above would only run when input arrives, so a dirty sprite could sit
        // unsnapshotted indefinitely.
        if self.editor.dirty {
            ui.ctx().request_repaint_after(
                self.next_autosave.saturating_duration_since(Instant::now()),
            );
        }

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
            if self.editor.dirty {
                clear_recovery();
            }
            self.save_config();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
    }
}
