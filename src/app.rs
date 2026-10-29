use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::config::Config;
use crate::editor::Editor;
use crate::gui::{self, View};
use crate::input;
use crate::io;

/// How often a dirty sprite is snapshotted to the recovery file.
const AUTOSAVE: Duration = Duration::from_secs(30);

struct Doc {
    editor: Editor,
    view: View,
    texture: Option<egui::TextureHandle>,
    onion_prev: Option<egui::TextureHandle>,
    onion_next: Option<egui::TextureHandle>,
    playing: bool,
    next_frame: Instant,
    was_dirty: bool,
}

impl Doc {
    fn new(config: &Config, path: Option<PathBuf>) -> Self {
        let mut editor = Editor::new(32, 32);
        editor.grid = config.grid.max(1);
        editor.snap = config.snap;
        editor.play_loop = config.play_loop;
        editor.play_pingpong = config.play_pingpong;
        editor.renderer = config.renderer.clone();
        editor.recent = config.recent.iter().map(PathBuf::from).collect();
        let mut view = View::new();
        view.zoom = config.zoom.clamp(1, 64);
        view.show_grid = config.show_grid;
        view.onion = config.onion.into();
        view.theme = config.theme.clone();
        view.ui_font = config.ui_font.clone();
        view.ui_font_size = config.ui_font_size;
        view.layout = config.layout.clone();
        view.keymap = config.keymap.clone();
        view.language = config.language.clone();
        match crate::i18n::resolve(&config.language) {
            Ok(catalog) => view.i18n = crate::i18n::Translator::new(catalog),
            Err(e) => editor.msg = format!("language: {e}"),
        }
        view.browser_favourites = config.favourites.iter().map(PathBuf::from).collect();
        if let Some(path) = path
            && let Err(e) = editor.open_path(&path)
        {
            editor.msg = format!("open failed: {e}");
        }
        Doc {
            editor,
            view,
            texture: None,
            onion_prev: None,
            onion_next: None,
            playing: false,
            next_frame: Instant::now(),
            was_dirty: false,
        }
    }

    fn title(&self) -> String {
        self.editor
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.view.tr("untitled"))
    }
}

pub struct VixelApp {
    docs: Vec<Doc>,
    active: usize,
    /// Last title sent to the window manager, so a fix bundle names its part.
    title: String,
    /// Settings loaded at start; written back on exit and on `:wq`.
    config: Config,
    /// Next periodic autosave while the active sprite is dirty.
    next_autosave: Instant,
    /// Theme/font currently applied, so switching tabs can restore per-doc UI.
    theme_applied: String,
    font_applied: (String, f32),
}

impl VixelApp {
    /// `path` is the optional file from the command line. A file that will not
    /// load is reported in the status line instead of blocking startup: a bad
    /// argument must still give you an editor.
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let config = Config::load();
        match crate::theme::resolve(&config.theme) {
            Ok(theme) => crate::theme::apply(&cc.egui_ctx, &theme),
            Err(e) => {
                crate::theme::apply(&cc.egui_ctx, &crate::theme::Theme::default());
                eprintln!("theme: {e}");
            }
        }
        if let Err(e) =
            crate::theme::install_font(&cc.egui_ctx, &config.ui_font, config.ui_font_size)
        {
            eprintln!("font: {e}");
        }
        let mut docs = Vec::new();
        if let Some(path) = path {
            docs.push(Doc::new(&config, Some(path)));
        } else {
            for path in &config.session {
                if !path.is_empty() {
                    docs.push(Doc::new(&config, Some(PathBuf::from(path))));
                }
            }
            if docs.is_empty() {
                docs.push(Doc::new(&config, None));
            }
        }
        let active = config.session_active.min(docs.len().saturating_sub(1));
        // A snapshot from a crashed session is offered, never loaded silently:
        // it may be older than the file the user just opened.
        if crate::config::recovery_path().is_some_and(|p| p.exists())
            && !docs[active].editor.msg.starts_with("open failed")
        {
            docs[active].editor.msg = "recovery from a previous session (:recover)".into();
        }
        Self {
            docs,
            active,
            title: "vixel".into(),
            theme_applied: config.theme.clone(),
            font_applied: (config.ui_font.clone(), config.ui_font_size),
            config,
            next_autosave: Instant::now() + AUTOSAVE,
        }
    }

    fn new_doc(&mut self) {
        let keymap = self.docs[self.active].view.keymap.clone();
        let language = self.docs[self.active].view.language.clone();
        let i18n = self.docs[self.active].view.i18n.clone();
        let mut doc = Doc::new(&self.config, None);
        doc.view.keymap = keymap;
        doc.view.language = language;
        doc.view.i18n = i18n;
        self.docs.push(doc);
        self.active = self.docs.len() - 1;
    }

    /// Switch tabs, carrying the global keymap and language with the selection.
    fn activate(&mut self, index: usize) {
        let keymap = self.docs[self.active].view.keymap.clone();
        let language = self.docs[self.active].view.language.clone();
        let i18n = self.docs[self.active].view.i18n.clone();
        let target = &mut self.docs[index].view;
        target.keymap = keymap;
        target.language = language;
        target.i18n = i18n;
        target.layout_pending = true;
        self.active = index;
    }

    /// Close one tab. Dirty documents need an explicit save or force.
    fn close_doc(&mut self, index: usize, force: bool) {
        if self.docs.len() == 1 {
            self.docs[index].editor.msg = "cannot close the last tab".into();
            return;
        }
        if self.docs[index].editor.dirty && !force {
            self.docs[index].editor.msg = "unsaved changes (use :w or close with force)".into();
            return;
        }
        self.docs.remove(index);
        if self.active >= self.docs.len() {
            self.active = self.docs.len() - 1;
            self.docs[self.active].view.layout_pending = true;
        }
    }

    fn any_dirty(&self) -> bool {
        self.docs.iter().any(|d| d.editor.dirty)
    }

    fn session_paths(&self) -> Vec<String> {
        self.docs
            .iter()
            .filter_map(|d| {
                d.editor
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .collect()
    }

    /// Snapshot the live settings into the config and write it. Called on quit
    /// and whenever the window is closed by the manager.
    fn save_config(&mut self) {
        let doc = &self.docs[self.active];
        self.config.window = doc.view.window;
        self.config.zoom = doc.view.zoom;
        self.config.show_grid = doc.view.show_grid;
        self.config.grid = doc.editor.grid;
        self.config.snap = doc.editor.snap;
        self.config.onion = doc.view.onion.into();
        self.config.play_loop = doc.editor.play_loop;
        self.config.play_pingpong = doc.editor.play_pingpong;
        self.config.theme = doc.view.theme.clone();
        self.config.ui_font = doc.view.ui_font.clone();
        self.config.ui_font_size = doc.view.ui_font_size;
        self.config.layout = doc.view.layout.clone();
        self.config.keymap = doc.view.keymap.clone();
        self.config.language = doc.view.language.clone();
        self.config.renderer = doc.editor.renderer.clone();
        self.config.recent = doc
            .editor
            .recent
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        self.config.favourites = doc
            .view
            .browser_favourites
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        self.config.session = self.session_paths();
        self.config.session_active = self.active;
        let _ = self.config.save();
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut switch = None;
        let mut close = None;
        let mut add = false;
        ui.horizontal_wrapped(|ui| {
            for i in 0..self.docs.len() {
                let label = format!(
                    "{}{}",
                    self.docs[i].title(),
                    if self.docs[i].editor.dirty { "*" } else { "" }
                );
                if ui.selectable_label(i == self.active, label).clicked() {
                    switch = Some(i);
                }
                if ui.small_button("×").on_hover_text("close tab").clicked() {
                    close = Some(i);
                }
            }
            if ui.small_button("+").on_hover_text("new tab").clicked() {
                add = true;
            }
        });
        if let Some(i) = switch {
            self.activate(i);
        }
        if add {
            self.new_doc();
        }
        if let Some(i) = close {
            self.close_doc(i, false);
        }
    }
}

/// Write the dirty sprite to the recovery file. Best-effort: a read-only
/// config directory must not break the editor.
fn snapshot(editor: &mut Editor) {
    let Some(path) = crate::config::recovery_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    editor.msg = match io::save_project(&editor.sprite, &path) {
        Ok(()) => format!("autosaved ({})", path.display()),
        Err(e) => format!("autosave failed: {e}"),
    };
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
        // Tab navigation: Ctrl/Cmd-Tab cycles, Ctrl/Cmd-T new, Ctrl/Cmd-W close.
        let (mut next_tab, mut prev_tab, mut new_tab, mut close_tab) = (false, false, false, false);
        ui.ctx().input(|i| {
            for ev in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = ev
                    && modifiers.command
                {
                    match key {
                        egui::Key::Tab if modifiers.shift => prev_tab = true,
                        egui::Key::Tab => next_tab = true,
                        egui::Key::T => new_tab = true,
                        egui::Key::W => close_tab = true,
                        _ => {}
                    }
                }
            }
        });
        if next_tab {
            self.activate((self.active + 1) % self.docs.len());
        }
        if prev_tab {
            self.activate((self.active + self.docs.len() - 1) % self.docs.len());
        }
        if new_tab {
            self.new_doc();
        }
        if close_tab {
            self.close_doc(self.active, false);
        }

        egui::Panel::top("tabs").show(ui, |ui| self.tab_bar(ui));

        // Each tab keeps its own theme/font; restore the active one on switch.
        let (theme_spec, font_spec, font_size) = {
            let view = &self.docs[self.active].view;
            (view.theme.clone(), view.ui_font.clone(), view.ui_font_size)
        };
        if theme_spec != self.theme_applied {
            match crate::theme::resolve(&theme_spec) {
                Ok(theme) => crate::theme::apply(ui.ctx(), &theme),
                Err(e) => self.docs[self.active].editor.msg = format!("theme: {e}"),
            }
            self.theme_applied = theme_spec;
        }
        if (font_spec.clone(), font_size) != self.font_applied {
            if let Err(e) = crate::theme::install_font(ui.ctx(), &font_spec, font_size) {
                self.docs[self.active].editor.msg = format!("font: {e}");
            }
            self.font_applied = (font_spec, font_size);
        }

        // Dropping a file onto the window opens it in a new tab.
        let dropped = ui
            .ctx()
            .input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            let mut doc = Doc::new(&self.config, Some(path));
            if doc.editor.msg.starts_with("open failed") {
                self.docs[self.active].editor.msg = doc.editor.msg;
            } else if doc.editor.path.is_none() {
                doc.editor.msg = "unsupported drop".into();
                self.docs[self.active].editor.msg = doc.editor.msg;
            } else {
                self.docs.push(doc);
                self.active = self.docs.len() - 1;
            }
        }

        // A window-manager close bypasses `:q`, so hold the window open while
        // any tab has unsaved edits (unless the active tab forced a quit).
        let forced =
            self.docs[self.active].editor.should_quit && self.docs[self.active].editor.dirty;
        if ui.ctx().input(|i| i.viewport().close_requested()) && self.any_dirty() && !forced {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.docs[self.active].editor.msg =
                "unsaved changes in a tab (save them or :q!)".into();
        }

        let other_dirty = self
            .docs
            .iter()
            .enumerate()
            .any(|(i, d)| i != self.active && d.editor.dirty);
        let doc = &mut self.docs[self.active];
        let title = match &doc.editor.bundle_stage {
            Some(stage) => format!("vixel — {stage} fix"),
            None => format!("vixel — {}", doc.title()),
        };
        if title != self.title {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        // A configured wheel binding consumes the scroll before the canvas can
        // treat it as zoom.
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0
            && let Some(action) = doc.view.keymap.wheel_action(scroll > 0.0)
        {
            doc.view.wheel_consumed = true;
            action.apply(&mut doc.editor, &mut doc.view, ui.ctx());
        }

        input::handle_input(&mut doc.editor, &mut doc.view, ui);

        // Snapshot the active dirty sprite periodically. A successful save or a
        // forced quit drops the snapshot.
        let now = Instant::now();
        if doc.editor.dirty && now >= self.next_autosave {
            self.next_autosave = now + AUTOSAVE;
            snapshot(&mut doc.editor);
        }
        if doc.was_dirty && !doc.editor.dirty {
            clear_recovery();
        }
        doc.was_dirty = doc.editor.dirty;
        // eframe sleeps while idle; request a wakeup so autosave still runs.
        if doc.editor.dirty {
            let wait = self.next_autosave.saturating_duration_since(now);
            ui.ctx().request_repaint_after(wait);
        }

        if doc.playing {
            let now = Instant::now();
            if now >= doc.next_frame {
                let dur = doc.editor.sprite.frames[doc.editor.active_frame].duration_ms as u64;
                if !doc.editor.advance_frame() {
                    doc.playing = false;
                }
                doc.next_frame = now + Duration::from_millis(dur.max(16));
            }
            ui.ctx().request_repaint();
        }

        gui::draw(
            ui,
            &mut doc.editor,
            &mut doc.view,
            &mut doc.texture,
            &mut doc.onion_prev,
            &mut doc.onion_next,
            &mut doc.playing,
        );

        let quit = doc.editor.should_quit;
        let quit_forced = doc.editor.dirty;
        if quit {
            if !quit_forced && other_dirty {
                doc.editor.should_quit = false;
                doc.editor.msg = "another tab has unsaved changes (switch and save)".into();
            } else {
                clear_recovery();
                self.save_config();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(count: usize) -> VixelApp {
        let config = Config::default();
        VixelApp {
            docs: (0..count).map(|_| Doc::new(&config, None)).collect(),
            active: 0,
            title: "vixel".into(),
            theme_applied: config.theme.clone(),
            font_applied: (config.ui_font.clone(), config.ui_font_size),
            config,
            next_autosave: Instant::now(),
        }
    }

    #[test]
    fn a_dirty_tab_needs_force_to_close() {
        let mut app = app_with(3);
        app.docs[0].editor.dirty = true;
        app.close_doc(0, false);
        assert_eq!(app.docs.len(), 3);
        assert!(app.docs[0].editor.msg.contains("unsaved"));
        app.close_doc(0, true);
        assert_eq!(app.docs.len(), 2);
    }

    #[test]
    fn closing_the_active_last_tab_moves_the_selection() {
        let mut app = app_with(3);
        app.active = 2;
        app.close_doc(2, true);
        assert_eq!(app.docs.len(), 2);
        assert_eq!(app.active, 1);
        app.close_doc(0, true);
        app.close_doc(0, true);
        assert_eq!(app.docs.len(), 1);
        assert!(app.docs[0].editor.msg.contains("last tab"));
    }

    #[test]
    fn session_records_named_documents_and_the_active_index() {
        let mut app = app_with(3);
        app.docs[0].editor.path = Some(PathBuf::from("/tmp/a.vpx"));
        app.docs[2].editor.path = Some(PathBuf::from("/tmp/c.ase"));
        assert_eq!(
            app.session_paths(),
            vec!["/tmp/a.vpx".to_string(), "/tmp/c.ase".to_string()]
        );
        app.active = 2;
        app.save_config();
        assert_eq!(app.config.session_active, 2);
        assert_eq!(app.config.session, app.session_paths());
    }

    #[test]
    fn tab_titles_follow_the_file_or_stay_untitled() {
        let config = Config::default();
        let mut doc = Doc::new(&config, None);
        assert_eq!(doc.title(), "untitled");
        doc.editor.path = Some(PathBuf::from("/tmp/walk.ase"));
        assert_eq!(doc.title(), "walk.ase");
    }
}
