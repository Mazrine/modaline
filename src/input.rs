use std::path::PathBuf;

use eframe::egui::{self, Event, Key, Modifiers};

use crate::color::Rgba;
use crate::editor::{Clipboard, Editor, Mode, Pending, Shape, Tool};
use crate::gui::View;
use crate::{ase, io, ops, palette};

/// Read this frame's input events and translate them into editor/view actions.
pub fn handle_input(editor: &mut Editor, view: &mut View, ui: &egui::Ui) {
    // Let egui's own widgets (text fields, popup search, color picker) keep the
    // keyboard; otherwise our vim keys would hijack typing into them.
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let events: Vec<Event> = ui.input(|i| i.events.clone());
    for event in &events {
        match event {
            Event::Text(text) => handle_text(editor, view, text),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => handle_key(editor, view, *key, *modifiers, ui.ctx()),
            Event::Copy => clipboard_copy(editor, ui.ctx()),
            Event::Cut => clipboard_cut(editor, ui.ctx()),
            Event::PasteImage(img) => clipboard_paste_image(editor, img),
            Event::Touch {
                phase, pos, force, ..
            } => handle_touch(editor, view, *phase, *pos, *force),
            _ => {}
        }
    }
}

/// Paint from a tablet/touch event, taking its pressure when the backend
/// reports one. Mice never carry force, so the manual pressure control covers
/// those devices.
fn handle_touch(
    editor: &mut Editor,
    view: &View,
    phase: egui::TouchPhase,
    pos: egui::Pos2,
    force: Option<f32>,
) {
    match phase {
        egui::TouchPhase::Start | egui::TouchPhase::Move => {
            if force.is_some() {
                editor.set_device_pressure(force);
            }
            let z = view.zoom as f32;
            if z <= 0.0 {
                return;
            }
            let local = (pos - view.pan) / z;
            let x = local.x.floor();
            let y = local.y.floor();
            if x < 0.0
                || y < 0.0
                || x as u32 >= editor.sprite.width
                || y as u32 >= editor.sprite.height
            {
                return;
            }
            if phase == egui::TouchPhase::Start || editor.stroke.is_none() {
                editor.begin_stroke();
            }
            editor.cursor = (x as u32, y as u32);
            editor.paint_with_tool(x as u32, y as u32);
        }
        egui::TouchPhase::End | egui::TouchPhase::Cancel => {
            editor.end_stroke();
            editor.set_device_pressure(None);
        }
    }
}

/// Ctrl/Cmd-C: yank the selection (or whole cel) to the clipboard and OS clipboard.
fn clipboard_copy(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.has_selection() {
        editor.select_all();
    }
    editor.yank();
    if let Some(cb) = &editor.clipboard
        && let Some(img) = clipboard_to_image(cb)
    {
        ctx.copy_image(img);
    }
}

/// Ctrl/Cmd-X: cut the selection (or whole cel) and put it on the OS clipboard.
fn clipboard_cut(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.has_selection() {
        editor.select_all();
    }
    editor.cut();
    if let Some(cb) = &editor.clipboard
        && let Some(img) = clipboard_to_image(cb)
    {
        ctx.copy_image(img);
    }
}

/// Ctrl/Cmd-V with an image on the OS clipboard → float it for pasting.
fn clipboard_paste_image(editor: &mut Editor, img: &std::sync::Arc<egui::ColorImage>) {
    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
    if w == 0 || h == 0 {
        return;
    }
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for c in &img.pixels {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        pixels.push(Rgba(r, g, b, a));
    }
    editor.clipboard = Some(Clipboard {
        w,
        h,
        pixels,
        mask: vec![true; (w * h) as usize],
    });
    editor.start_paste();
    editor.msg = "pasted from clipboard".into();
}

fn clipboard_to_image(cb: &Clipboard) -> Option<egui::ColorImage> {
    if cb.w == 0 || cb.h == 0 {
        return None;
    }
    let mut rgba = Vec::with_capacity((cb.w * cb.h * 4) as usize);
    for i in 0..(cb.w * cb.h) as usize {
        let c = if cb.mask[i] {
            cb.pixels[i]
        } else {
            Rgba::TRANSPARENT
        };
        rgba.extend_from_slice(&[c.0, c.1, c.2, c.3]);
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [cb.w as usize, cb.h as usize],
        &rgba,
    ))
}

fn handle_key(
    editor: &mut Editor,
    view: &mut View,
    key: Key,
    mods: Modifiers,
    ctx: &egui::Context,
) {
    use Key::*;

    // Custom action bindings are checked first so they can override the fixed
    // modal keymap; a moved default is swallowed rather than falling through.
    if editor.mode == Mode::Normal {
        if let Some(action) = view.keymap.action_for(key, mods) {
            action.apply(editor, view, ctx);
            view.suppress_text = key_produces_text(key, mods);
            return;
        }
        if view.keymap.blocks_default(key, mods) {
            view.suppress_text = key_produces_text(key, mods);
            return;
        }
    }

    if mods.command {
        match key {
            R => editor.redo(),
            Z => editor.undo(),
            S if mods.shift => {
                view.browser_mode = crate::gui::BrowserMode::Save;
                view.browser_open = true;
                if let Some(dir) = editor.path.as_ref().and_then(|p| p.parent()) {
                    view.browser_dir = dir.to_path_buf();
                }
            }
            O => {
                view.browser_mode = crate::gui::BrowserMode::Open;
                view.browser_open = true;
                if let Some(dir) = editor.path.as_ref().and_then(|p| p.parent()) {
                    view.browser_dir = dir.to_path_buf();
                }
            }
            S => save(editor),
            Q => editor.should_quit = true,
            _ => {}
        }
        return;
    }

    match editor.mode {
        Mode::Command => match key {
            Enter => submit(editor, view, ctx),
            Escape => {
                editor.mode = Mode::Normal;
                editor.cmdline.clear();
            }
            Backspace => {
                editor.cmdline.pop();
            }
            _ => {}
        },
        Mode::Help => match key {
            Escape | Q | H => editor.mode = Mode::Normal,
            _ => {}
        },
        Mode::Visual => match key {
            H => editor.move_cursor(if mods.shift { -8 } else { -1 }, 0),
            J => editor.move_cursor(0, if mods.shift { 8 } else { 1 }),
            K => editor.move_cursor(0, if mods.shift { -8 } else { -1 }),
            L => editor.move_cursor(if mods.shift { 8 } else { 1 }, 0),
            ArrowLeft => editor.move_cursor(-1, 0),
            ArrowDown => editor.move_cursor(0, 1),
            ArrowUp => editor.move_cursor(0, -1),
            ArrowRight => editor.move_cursor(1, 0),
            Escape => {
                editor.commit_marquee();
            }
            _ => {}
        },
        Mode::Paste => match key {
            H | ArrowLeft => editor.move_paste(-1, 0),
            J | ArrowDown => editor.move_paste(0, 1),
            K | ArrowUp => editor.move_paste(0, -1),
            L | ArrowRight => editor.move_paste(1, 0),
            Enter => editor.commit_paste(),
            Escape | Q => editor.cancel_paste(),
            _ => {}
        },
        Mode::Draw => match key {
            H | ArrowLeft => {
                editor.move_cursor(-1, 0);
                paint_cursor(editor);
            }
            J | ArrowDown => {
                editor.move_cursor(0, 1);
                paint_cursor(editor);
            }
            K | ArrowUp => {
                editor.move_cursor(0, -1);
                paint_cursor(editor);
            }
            L | ArrowRight => {
                editor.move_cursor(1, 0);
                paint_cursor(editor);
            }
            Escape | Q => {
                editor.end_stroke();
                editor.mode = Mode::Normal;
            }
            _ => {}
        },
        Mode::Normal => match key {
            H => editor.move_cursor(if mods.shift { -8 } else { -1 }, 0),
            J => editor.move_cursor(0, if mods.shift { 8 } else { 1 }),
            K => editor.move_cursor(0, if mods.shift { -8 } else { -1 }),
            L => editor.move_cursor(if mods.shift { 8 } else { 1 }, 0),
            ArrowLeft => editor.move_cursor(-1, 0),
            ArrowDown => editor.move_cursor(0, 1),
            ArrowUp => editor.move_cursor(0, -1),
            ArrowRight => editor.move_cursor(1, 0),
            Enter => {
                if editor.pending.is_some() {
                    editor.commit_shape(editor.fg);
                } else if editor.has_lasso() {
                    editor.lasso_finish();
                }
            }
            Space => {
                if editor.pending.is_some() {
                    editor.commit_shape(editor.fg);
                } else {
                    editor.begin_stroke();
                    let (x, y) = editor.cursor;
                    editor.paint_brush_symmetric(x, y, editor.fg);
                    editor.end_stroke();
                }
            }
            Escape => {
                if editor.cel_move.is_some() {
                    editor.cancel_cel_move();
                }
                if editor.has_lasso() {
                    editor.lasso_cancel();
                }
                editor.pending = None;
                editor.g_prefix = false;
                editor.clear_selection();
            }
            Plus | Equals => view.zoom_in(),
            Minus => view.zoom_out(),
            _ => {}
        },
    }
}

/// Keys that also emit an `Event::Text`, so a keymap action must suppress the
/// follow-up character handler.
fn key_produces_text(key: Key, mods: Modifiers) -> bool {
    if mods.ctrl || mods.alt {
        return false;
    }
    !matches!(
        key,
        Key::ArrowLeft
            | Key::ArrowRight
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::Enter
            | Key::Escape
            | Key::Tab
            | Key::Backspace
            | Key::Delete
            | Key::Insert
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
    )
}

fn paint_cursor(editor: &mut Editor) {
    let (x, y) = editor.cursor;
    editor.paint_with_tool(x, y);
}

fn handle_text(editor: &mut Editor, view: &mut View, text: &str) {
    if view.suppress_text {
        view.suppress_text = false;
        return;
    }
    for ch in text.chars() {
        match editor.mode {
            Mode::Command => {
                if !ch.is_control() {
                    editor.cmdline.push(ch);
                }
            }
            Mode::Normal => handle_char(editor, ch),
            Mode::Visual => handle_visual_char(editor, ch),
            _ => {}
        }
    }
}

fn handle_visual_char(editor: &mut Editor, ch: char) {
    match ch {
        'y' => editor.yank(),
        'Y' => editor.yank_merged(),
        'x' | 'd' => editor.cut(),
        'm' => editor.start_move(),
        _ => {}
    }
}

fn handle_char(editor: &mut Editor, ch: char) {
    if editor.g_prefix {
        editor.g_prefix = false;
        if ch == 'g' {
            editor.move_cursor(0, -(editor.sprite.height as i32));
            return;
        }
        // otherwise fall through and handle `ch` normally
    }

    match ch {
        'i' => {
            editor.begin_stroke();
            editor.mode = Mode::Draw;
        }
        'x' => {
            if editor.has_selection() {
                editor.cut();
            } else {
                editor.begin_stroke();
                let (x, y) = editor.cursor;
                editor.paint_brush_symmetric(x, y, Rgba::TRANSPARENT);
                editor.end_stroke();
            }
        }
        'r' => {
            editor.begin_stroke();
            let (x, y) = editor.cursor;
            editor.paint_brush_symmetric(x, y, editor.fg);
            editor.end_stroke();
        }
        's' => editor.cycle_symmetry(),
        't' => editor.toggle_filled_shapes(),
        'w' => {
            let (x, y) = editor.cursor;
            editor.magic_wand(x, y);
        }
        'y' => editor.yank(),
        'Y' => editor.yank_merged(),
        'm' => editor.start_move(),
        'v' => editor.start_select(),
        'V' => {
            editor.cursor = (editor.sprite.width - 1, editor.sprite.height - 1);
            editor.select_anchor = Some((0, 0));
            editor.pending = None;
            editor.mode = Mode::Visual;
        }
        'p' => editor.start_paste(),
        'b' => editor.set_tool(Tool::Pencil),
        'e' => editor.set_tool(Tool::Eraser),
        'f' => {
            editor.set_tool(Tool::Bucket);
            let (x, y) = editor.cursor;
            editor.apply_bucket(x, y);
        }
        'c' => {
            editor.set_tool(Tool::Picker);
            let (x, y) = editor.cursor;
            editor.pick_color(x, y);
        }
        'd' => {
            editor.set_tool(Tool::Line);
            editor.pending = Some(Pending {
                shape: Shape::Line,
                start: editor.cursor,
            });
        }
        'R' => {
            editor.set_tool(Tool::Rect);
            editor.pending = Some(Pending {
                shape: Shape::Rect,
                start: editor.cursor,
            });
        }
        'o' => {
            editor.set_tool(Tool::Ellipse);
            editor.pending = Some(Pending {
                shape: Shape::Ellipse,
                start: editor.cursor,
            });
        }
        'L' => editor.set_tool(Tool::Lasso),
        'P' => editor.set_tool(Tool::Polygon),
        'z' => {
            editor.set_tool(Tool::Gradient);
            editor.pending = Some(Pending {
                shape: Shape::Gradient,
                start: editor.cursor,
            });
        }
        'a' => editor.set_tool(Tool::Spray),
        'q' => editor.set_tool(Tool::Shade),
        'n' => editor.set_tool(Tool::Blur),
        'T' => editor.set_tool(Tool::Text),
        'K' => editor.set_tool(Tool::Tile),
        'M' => editor.set_tool(Tool::Move),
        'C' => {
            let (x, y) = editor.cursor;
            editor.contour_at(x, y);
        }
        ',' => editor.cycle_color(-1),
        '.' => editor.cycle_color(1),
        '<' => editor.cycle_layer(-1),
        '>' => editor.cycle_layer(1),
        '[' => editor.cycle_frame(-1),
        ']' => editor.cycle_frame(1),
        'X' => editor.swap_colors(),
        'u' => editor.undo(),
        'g' => editor.g_prefix = true,
        'G' => editor.move_cursor(0, editor.sprite.height as i32),
        '0' => editor.cursor.0 = 0,
        '$' => editor.cursor.0 = editor.sprite.width - 1,
        ':' => {
            editor.cmdline.clear();
            editor.mode = Mode::Command;
        }
        '?' => editor.mode = Mode::Help,
        _ => {}
    }
}

fn submit(editor: &mut Editor, view: &mut View, ctx: &egui::Context) {
    let cmd = std::mem::take(&mut editor.cmdline);
    editor.mode = Mode::Normal;
    // Theme/font are view state, so they are handled where the UI context and
    // `View` are available; `exec_command` stays model-only.
    if let Some(rest) = cmd.strip_prefix("theme")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        let rest = rest.trim();
        if let Some(path) = rest.strip_prefix("save ") {
            match crate::theme::resolve(&view.theme)
                .and_then(|t| crate::theme::save(&t, std::path::Path::new(path.trim())))
            {
                Ok(()) => editor.msg = format!("theme saved: {}", path.trim()),
                Err(e) => editor.msg = format!("theme save failed: {e}"),
            }
        } else if rest.is_empty() {
            editor.msg = format!(
                "theme: {} (built-ins: {}; use :theme <name|file.json>)",
                view.theme,
                crate::theme::names().join(", ")
            );
        } else {
            match crate::theme::resolve(rest) {
                Ok(theme) => {
                    crate::theme::apply(ctx, &theme);
                    view.theme = rest.to_string();
                    editor.msg = format!("theme: {}", theme.name);
                }
                Err(e) => editor.msg = format!("theme: {e}"),
            }
        }
        return;
    }
    if let Some(rest) = cmd.strip_prefix("layout")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        let rest = rest.trim();
        if let Some(path) = rest.strip_prefix("save ") {
            match crate::layout::save(&view.layout, std::path::Path::new(path.trim())) {
                Ok(()) => editor.msg = format!("layout saved: {}", path.trim()),
                Err(e) => editor.msg = format!("layout save failed: {e}"),
            }
        } else if rest == "reset" {
            view.layout = crate::layout::Layout::default();
            view.layout_pending = true;
            editor.msg = "layout: default (reset)".into();
        } else if rest.is_empty() {
            editor.msg = format!(
                "layout: {} (built-ins: {}; use :layout <name|file.json> | reset)",
                view.layout.name,
                crate::layout::names().join(", ")
            );
        } else {
            match crate::layout::resolve(rest) {
                Ok(mut layout) => {
                    crate::layout::clamp_sizes(&mut layout);
                    view.layout = layout;
                    view.layout_pending = true;
                    editor.msg = format!("layout: {}", view.layout.name);
                }
                Err(e) => editor.msg = format!("layout: {e}"),
            }
        }
        return;
    }
    if cmd == "prefs" || cmd == "preferences" {
        view.prefs_open = true;
        editor.msg = "preferences".into();
        return;
    }
    if let Some(rest) = cmd.strip_prefix("ext")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        let rest = rest.trim();
        let root = match crate::extension::default_root() {
            Ok(root) => root,
            Err(e) => {
                editor.msg = format!("extensions: {e}");
                return;
            }
        };
        let mut it = rest.split_whitespace();
        let sub = it.next().unwrap_or("");
        match sub {
            "" | "list" => match crate::extension::list(&root) {
                Ok(list) if list.is_empty() => editor.msg = "extensions: none installed".into(),
                Ok(list) => {
                    editor.msg = list
                        .iter()
                        .map(|e| {
                            let author = e.author();
                            format!(
                                "{} {}{} ({})",
                                e.id(),
                                e.manifest.version,
                                if author.is_empty() {
                                    String::new()
                                } else {
                                    format!(" by {author}")
                                },
                                e.label()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                }
                Err(e) => editor.msg = format!("extensions: {e}"),
            },
            "install" => match it.next() {
                Some(path) => match crate::extension::install_default(std::path::Path::new(path)) {
                    Ok(ext) => {
                        editor.msg = format!(
                            "installed {} {} ({} scripts, {} themes)",
                            ext.id(),
                            ext.manifest.version,
                            ext.scripts.len(),
                            ext.themes.len()
                        )
                    }
                    Err(e) => editor.msg = format!("extension install: {e}"),
                },
                None => editor.msg = "usage: :ext install <file.aseprite-extension>".into(),
            },
            "remove" => match it.next() {
                Some(id) => match crate::extension::remove(&root, id) {
                    Ok(()) => editor.msg = format!("removed extension {id}"),
                    Err(e) => editor.msg = format!("extension remove: {e}"),
                },
                None => editor.msg = "usage: :ext remove <id>".into(),
            },
            "run" => match it
                .next()
                .and_then(|id| crate::extension::get(&root, id).ok())
            {
                Some(ext) => match ext.run_scripts(editor) {
                    Ok(out) if out.is_empty() => {
                        editor.msg = format!("{}: scripts finished", ext.id())
                    }
                    Ok(out) => {
                        editor.msg = out.lines().last().unwrap_or("scripts finished").to_string()
                    }
                    Err(e) => editor.msg = format!("{}: {e}", ext.id()),
                },
                None => editor.msg = "usage: :ext run <id>".into(),
            },
            "themes" => match it
                .next()
                .and_then(|id| crate::extension::get(&root, id).ok())
            {
                Some(ext) if ext.themes.is_empty() => {
                    editor.msg = format!("{}: no themes", ext.id())
                }
                Some(ext) => {
                    editor.msg = ext
                        .themes
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            format!(
                                "{}= {}",
                                i + 1,
                                p.file_name().unwrap_or_default().to_string_lossy()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                }
                None => editor.msg = "usage: :ext themes <id>".into(),
            },
            "theme" => {
                let id = it.next();
                let index = it.next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(1);
                match id.and_then(|id| crate::extension::get(&root, id).ok()) {
                    Some(ext) => match ext.themes.get(index.saturating_sub(1)) {
                        Some(path) => match crate::theme::resolve(&path.to_string_lossy()) {
                            Ok(theme) => {
                                crate::theme::apply(ctx, &theme);
                                view.theme = path.to_string_lossy().to_string();
                                editor.msg = format!("theme: {}", theme.name);
                            }
                            Err(e) => editor.msg = format!("extension theme: {e}"),
                        },
                        None => editor.msg = format!("{}: no theme {index}", ext.id()),
                    },
                    None => editor.msg = "usage: :ext theme <id> [n]".into(),
                }
            }
            other => {
                editor.msg =
                    format!("usage: :ext list|install|remove|run|themes|theme (got {other})")
            }
        }
        return;
    }
    if let Some(rest) = cmd.strip_prefix("lang")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        let rest = rest.trim();
        if let Some(path) = rest.strip_prefix("save ") {
            match view.i18n.catalog.save(std::path::Path::new(path.trim())) {
                Ok(()) => editor.msg = format!("language saved: {}", path.trim()),
                Err(e) => editor.msg = format!("language save failed: {e}"),
            }
        } else if rest == "check" {
            let missing = view.i18n.missing();
            editor.msg = if missing.is_empty() {
                format!("language {}: complete", view.language)
            } else {
                format!(
                    "language {}: {} missing ({})",
                    view.language,
                    missing.len(),
                    missing.join(", ")
                )
            };
        } else if rest.is_empty() || rest == "list" {
            let mut text = format!("language: {} (", view.language);
            for code in crate::i18n::BUILTINS {
                let name = crate::i18n::builtin(code)
                    .map(|c| c.name)
                    .unwrap_or_else(|| code.to_string());
                text.push_str(&format!("{code}={name} "));
            }
            text.push_str("; :lang <code|file.json>)");
            editor.msg = text;
        } else {
            match crate::i18n::resolve(rest) {
                Ok(catalog) => {
                    let name = catalog.name.clone();
                    view.language = rest.to_string();
                    view.i18n = crate::i18n::Translator::new(catalog);
                    editor.msg = format!("language: {name}");
                }
                Err(e) => editor.msg = format!("language: {e}"),
            }
        }
        return;
    }
    if let Some(rest) = cmd.strip_prefix("bind")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        let rest = rest.trim();
        if rest.is_empty() || rest == "list" {
            let mut text = format!("bindings ({} actions):", view.keymap.bindings.len());
            for (action, chord) in &view.keymap.bindings {
                text.push_str(&format!(" {}={}", action.name(), chord.label()));
            }
            if let Some(a) = view.keymap.wheel_up {
                text.push_str(&format!(" wheel-up={}", a.name()));
            }
            if let Some(a) = view.keymap.wheel_down {
                text.push_str(&format!(" wheel-down={}", a.name()));
            }
            let conflicts = view.keymap.conflicts();
            if !conflicts.is_empty() {
                text.push_str(&format!(" CONFLICTS: {}", conflicts.len()));
            }
            editor.msg = text;
        } else if rest == "reset" {
            view.keymap = crate::keymap::Keymap::default();
            editor.msg = "bindings: defaults restored".into();
        } else {
            let mut it = rest.split_whitespace();
            let name = it.next().unwrap_or("");
            let target = it.next();
            let Some(action) = crate::keymap::Action::parse(name) else {
                editor.msg = format!("unknown action '{name}' (use :bind list)");
                return;
            };
            match target {
                None => {
                    editor.msg = format!(
                        "usage: :bind {} <chord|wheel-up|wheel-down|none>",
                        action.name()
                    )
                }
                Some("none") => {
                    view.keymap.unbind(action);
                    editor.msg = format!("{}: unbound", action.name());
                }
                Some("wheel-up") | Some("wheel-down") => {
                    view.keymap.set_wheel(action, target == Some("wheel-up"));
                    editor.msg = format!("{}: {} (scroll)", action.name(), target.unwrap());
                }
                Some(chord) => match crate::keymap::Chord::parse(chord) {
                    None => {
                        editor.msg = format!("bad chord '{chord}' (e.g. ctrl-shift-z, f1, space)")
                    }
                    Some(chord) => {
                        let stolen = view.keymap.set(action, chord.clone());
                        editor.msg = match stolen {
                            Some(old) => format!(
                                "{} = {} (stole from {})",
                                action.name(),
                                chord.label(),
                                old.name()
                            ),
                            None => format!("{} = {}", action.name(), chord.label()),
                        };
                    }
                },
            }
        }
        return;
    }
    if let Some(rest) = cmd.strip_prefix("uifont")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        let rest = rest.trim();
        let mut it = rest.split_whitespace();
        let path = it.next().unwrap_or("");
        if path.is_empty() {
            editor.msg = "usage: :uifont <file.ttf|default> [8-32]".into();
            return;
        }
        let size = it
            .next()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(view.ui_font_size);
        match crate::theme::install_font(ctx, path, size) {
            Ok(()) => {
                view.ui_font = path.to_string();
                view.ui_font_size = size.clamp(8.0, 32.0);
                editor.msg = format!("ui font: {path} @ {:.0}", view.ui_font_size);
            }
            Err(e) => editor.msg = format!("ui font failed: {e}"),
        }
        return;
    }
    exec_command(editor, &cmd);
}

pub fn save_active(editor: &mut Editor) {
    save(editor);
}

fn save(editor: &mut Editor) {
    if let Some(p) = editor.path.clone() {
        match io::save_project(&editor.sprite, &p) {
            Ok(()) => {
                editor.dirty = false;
                editor.msg = editor.saved_note();
            }
            Err(err) => editor.msg = format!("save failed: {err}"),
        }
    } else {
        editor.msg = "no filename (use :w <file>)".into();
    }
}

fn exec_command(editor: &mut Editor, cmd: &str) {
    let trimmed = cmd.trim();
    let (name, arg) = match trimmed.find(char::is_whitespace) {
        Some(i) => (&trimmed[..i], Some(trimmed[i..].trim_start())),
        None => (trimmed, None),
    };
    match name {
        "w" | "write" | "save" => {
            let path = arg.map(PathBuf::from).or_else(|| editor.path.clone());
            match path {
                Some(p) => {
                    save_to(editor, p);
                }
                None => editor.msg = "no filename (use :w <file>)".into(),
            }
        }
        "q" | "quit" => {
            if editor.dirty {
                editor.msg = "unsaved changes (use :q! to force or :w to save)".into();
            } else {
                editor.should_quit = true;
            }
        }
        "q!" => editor.should_quit = true,
        "wq" => {
            let path = arg.map(PathBuf::from).or_else(|| editor.path.clone());
            match path {
                Some(p) => {
                    if save_to(editor, p) {
                        editor.should_quit = true;
                    }
                }
                None => editor.msg = "no filename (use :wq <file>)".into(),
            }
        }
        "new" | "new!" => {
            if !replace_ok(editor, name == "new!", "new") {
                return;
            }
            let (w, h) = parse_dims(arg, 32, 32);
            editor.new_sprite(w, h);
            editor.msg = format!("new {w}x{h}");
        }
        "resize" => {
            let (w, h) = parse_dims(arg, editor.sprite.width, editor.sprite.height);
            if w < 1 || h < 1 {
                editor.msg = "resize needs width/height >= 1".into();
            } else {
                editor.resize(w, h);
                editor.msg = format!("resized to {w}x{h}");
            }
        }
        "crop" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            match (
                it.next().and_then(|s| s.parse::<u32>().ok()),
                it.next().and_then(|s| s.parse::<u32>().ok()),
                it.next().and_then(|s| s.parse::<u32>().ok()),
                it.next().and_then(|s| s.parse::<u32>().ok()),
            ) {
                (Some(x), Some(y), Some(w), Some(h)) => editor.crop_to(x, y, w, h),
                _ if arg.is_none() => editor.crop_to_selection(),
                _ => editor.msg = "usage: :crop [x y w h] (no args = selection)".into(),
            }
        }
        "trim" => editor.trim(),
        "newframe" | "nf" => editor.add_frame(),
        "delframe" | "df" => editor.del_frame(),
        "dupframe" | "dupf" => editor.duplicate_frame(),
        "newlayer" | "nl" => editor.add_layer(),
        "duplayer" | "dupl" => editor.duplicate_layer(),
        "merge" | "mergedown" => editor.merge_down(),
        "flatten" | "flat" => editor.flatten(),
        "dellayer" | "dl" => editor.del_layer(),
        "layerup" | "lup" => editor.move_layer_active(-1),
        "layerdown" | "ldown" => editor.move_layer_active(1),
        "layername" | "renamelayer" => {
            if let Some(name) = arg {
                editor.rename_layer(name);
            } else {
                editor.msg = "usage: :layername <name>".into();
            }
        }
        "newgroup" | "ng" => editor.add_group(),
        "indent" => editor.indent_layer(1),
        "outdent" => editor.indent_layer(-1),
        "background" | "bgnd" => match arg {
            Some("on" | "true" | "1") => editor.set_layer_background(true),
            Some("off" | "false" | "0") => editor.set_layer_background(false),
            _ => editor.msg = "usage: :background on|off".into(),
        },
        "reference" | "ref" => match arg {
            Some("on" | "true" | "1") => editor.set_layer_reference(true),
            Some("off" | "false" | "0") => editor.set_layer_reference(false),
            _ => editor.msg = "usage: :reference on|off".into(),
        },
        "lock" | "unlock" => {
            let on = name == "lock";
            editor.set_layer_editable(!on);
            editor.msg = if on { "layer locked" } else { "layer unlocked" }.into();
        }
        "mask" => match arg {
            Some("add" | "new") => editor.mask_add(),
            Some("apply" | "bake") => editor.mask_apply(),
            Some("del" | "delete" | "remove") => editor.mask_del(),
            _ => editor.msg = "usage: :mask add|apply|del".into(),
        },
        "tile" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            match it.next() {
                Some("on") => editor.set_tile_mode(Some(true)),
                Some("off") => editor.set_tile_mode(Some(false)),
                Some("x") => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                    Some(n) => {
                        editor.tile_x = n.clamp(1, 8);
                        editor.msg = format!("tile: {}x per side", editor.tile_x);
                    }
                    None => editor.msg = "usage: :tile x <1-8>".into(),
                },
                Some("y") => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                    Some(n) => {
                        editor.tile_y = n.clamp(1, 8);
                        editor.msg = format!("tile: {}y per side", editor.tile_y);
                    }
                    None => editor.msg = "usage: :tile y <1-8>".into(),
                },
                Some("repeats") => {
                    let x = it.next().and_then(|v| v.parse::<u32>().ok());
                    let y = it.next().and_then(|v| v.parse::<u32>().ok());
                    match (x, y) {
                        (Some(x), Some(y)) => {
                            editor.tile_x = x.clamp(1, 8);
                            editor.tile_y = y.clamp(1, 8);
                            editor.msg =
                                format!("tile: {}x {}y per side", editor.tile_x, editor.tile_y);
                        }
                        _ => editor.msg = "usage: :tile repeats <x> <y>".into(),
                    }
                }
                Some("opacity") => match it.next().and_then(|v| v.parse::<u8>().ok()) {
                    Some(n) => {
                        editor.tile_opacity = n;
                        editor.msg = format!("tile opacity: {n}");
                    }
                    None => editor.msg = "usage: :tile opacity <0-255>".into(),
                },
                Some("preview") => match it.next() {
                    Some("on") => {
                        editor.tile_preview = true;
                        editor.msg = "tile preview: on".into();
                    }
                    Some("off") => {
                        editor.tile_preview = false;
                        editor.msg = "tile preview: off".into();
                    }
                    _ => editor.msg = "usage: :tile preview on|off".into(),
                },
                None => editor.set_tile_mode(None),
                _ => {
                    editor.msg = "usage: :tile [on|off] | x|y <1-8> | repeats <x> <y> | \
                                  opacity <0-255> | preview on|off"
                        .into()
                }
            }
        }
        "tag" | "tags" => match arg {
            Some(rest) => {
                let mut parts = rest.split_whitespace();
                let name = parts.next().unwrap_or("");
                if name.is_empty() {
                    editor.msg = "usage: :tag <name> [from to]".into();
                } else {
                    let from = parts.next().and_then(|s| s.parse::<usize>().ok());
                    let to = parts.next().and_then(|s| s.parse::<usize>().ok());
                    editor.set_tag(name, from, to);
                }
            }
            None => {
                editor.msg = if editor.sprite.tags.is_empty() {
                    "no tags (use :tag <name> [from to])".into()
                } else {
                    editor
                        .sprite
                        .tags
                        .iter()
                        .map(|t| format!("{} {}-{}", t.name, t.from, t.to))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
            }
        },
        "tagdel" | "untag" => editor.del_tag(arg),
        "copy" | "yank" => match arg {
            Some("merged" | "all") => editor.yank_merged(),
            _ => editor.yank(),
        },
        "tagdir" => match arg.and_then(crate::sprite::PlayDirection::parse) {
            Some(dir) => editor.set_tag_direction(dir),
            None => {
                editor.msg = "usage: :tagdir forward|reverse|pingpong|pingpong-reverse".into();
            }
        },
        "celz" | "z" => match arg.and_then(|s| s.parse::<i16>().ok()) {
            Some(z) => editor.set_cel_z(z),
            None => editor.msg = "usage: :celz <n>".into(),
        },
        "link" => match arg.and_then(|s| s.parse::<usize>().ok()) {
            Some(frame) => editor.link_cel(frame),
            None => editor.msg = "usage: :link <frame>".into(),
        },
        "unlink" => editor.unlink_cel(),
        "preferlink" | "preferlinked" => match arg {
            Some("on") | Some("true") | Some("1") => editor.set_prefer_linked(true),
            Some("off") | Some("false") | Some("0") => editor.set_prefer_linked(false),
            _ => editor.msg = "usage: :preferlink on|off".into(),
        },
        "movelock" | "lockmove" => match arg {
            Some("on") | Some("true") | Some("1") => editor.set_layer_lock_movement(true),
            Some("off") | Some("false") | Some("0") => editor.set_layer_lock_movement(false),
            _ => editor.msg = "usage: :movelock on|off".into(),
        },
        "collapse" | "collapsed" => match arg {
            Some("on") | Some("true") | Some("1") => editor.set_layer_collapsed(true),
            Some("off") | Some("false") | Some("0") => editor.set_layer_collapsed(false),
            _ => editor.msg = "usage: :collapse on|off".into(),
        },
        "layeruuid" | "uuid" => match arg {
            Some("new") => editor.new_layer_uuid(),
            Some("clear") => editor.clear_layer_uuid(),
            None => editor.msg = format!("layer uuid: {}", editor.layer_uuid_hex()),
            _ => editor.msg = "usage: :layeruuid [new|clear]".into(),
        },
        "tagrepeat" => match arg.and_then(|s| s.parse::<u16>().ok()) {
            Some(n) => editor.set_tag_repeat(n),
            None => editor.msg = "usage: :tagrepeat <n> (0 = forever)".into(),
        },
        "framemove" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            match (
                it.next().and_then(|s| s.parse::<usize>().ok()),
                it.next().and_then(|s| s.parse::<usize>().ok()),
            ) {
                (Some(from), Some(to)) => editor.move_frame(from, to),
                _ => editor.msg = "usage: :framemove <from> <to>".into(),
            }
        }
        "fliph" => editor.flip_h(),
        "flipv" => editor.flip_v(),
        "rot90" => editor.rotate_cw(),
        "sym" | "symmetry" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let name = it.next().unwrap_or("");
            if editor.set_symmetry(name) {
                if let Some(n) = it.next().and_then(|v| v.parse::<u8>().ok()) {
                    editor.sym_axes = n.clamp(2, 16);
                }
                editor.msg = format!("symmetry: {}", editor.symmetry.name());
            } else {
                editor.msg = "usage: :sym none|horizontal|vertical|both|radial [axes]".into();
            }
        }
        "symcenter" | "symorigin" => {
            let arg = arg.unwrap_or("");
            let parts: Vec<&str> = arg.split_whitespace().collect();
            match parts.as_slice() {
                ["cursor"] | [] => {
                    editor.sym_center = Some(editor.cursor);
                    let (x, y) = editor.sym_center.unwrap();
                    editor.msg = format!("symmetry centre: {x},{y}");
                }
                ["reset"] => {
                    editor.sym_center = None;
                    editor.msg = "symmetry centre: sprite centre".into();
                }
                [x, y] => match (x.parse::<u32>(), y.parse::<u32>()) {
                    (Ok(x), Ok(y)) => {
                        editor.sym_center = Some((x, y));
                        editor.msg = format!("symmetry centre: {x},{y}");
                    }
                    _ => editor.msg = "usage: :symcenter cursor|reset|<x> <y>".into(),
                },
                _ => editor.msg = "usage: :symcenter cursor|reset|<x> <y>".into(),
            }
        }
        "celselect" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let layer = it.next().and_then(|v| v.parse::<usize>().ok());
            let frame = it.next().and_then(|v| v.parse::<usize>().ok());
            match (layer, frame) {
                (Some(layer), Some(frame))
                    if layer < editor.sprite.layers.len() && frame < editor.sprite.frames.len() =>
                {
                    editor.toggle_cel_selection(layer, frame);
                }
                _ => editor.msg = "usage: :celselect <layer> <frame>".into(),
            }
        }
        "celclear" => {
            editor.clear_cel_selection();
            editor.msg = "cel selection cleared".into();
        }
        "exportcels" => {
            let Some(dir) = arg.filter(|p| !p.is_empty()) else {
                editor.msg = "usage: :exportcels <dir>".into();
                return;
            };
            match editor.export_cels(std::path::Path::new(dir)) {
                Ok(n) => editor.msg = format!("cels exported: {n} -> {dir}"),
                Err(e) => editor.msg = format!("cel export failed: {e:#}"),
            }
        }
        "exporttileset" | "tilesetexport" => {
            let path = arg
                .filter(|p| !p.is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    editor
                        .path
                        .as_ref()
                        .map(|p| p.with_extension("tileset.png"))
                })
                .unwrap_or_else(|| std::path::PathBuf::from("tileset.png"));
            match crate::io::export_tileset(&editor.sprite, None, &path) {
                Ok(()) => editor.msg = format!("tileset exported: {}", path.display()),
                Err(e) => editor.msg = format!("tileset export failed: {e:#}"),
            }
        }
        "tileid" => match arg.and_then(|v| v.parse::<u32>().ok()) {
            Some(id) => {
                editor.tile_brush = id;
                editor.msg = format!("tile brush: #{id}");
            }
            None => editor.msg = "usage: :tileid <0-1024>".into(),
        },
        "tileflip" => match arg {
            Some("none" | "off" | "clear") => {
                editor.tile_flip_x = false;
                editor.tile_flip_y = false;
                editor.tile_flip_diagonal = false;
                editor.msg = "tile flips: none".into();
            }
            Some("x") => {
                editor.tile_flip_x = !editor.tile_flip_x;
                editor.msg = format!("tile flip x: {}", editor.tile_flip_x);
            }
            Some("y") => {
                editor.tile_flip_y = !editor.tile_flip_y;
                editor.msg = format!("tile flip y: {}", editor.tile_flip_y);
            }
            Some("diag" | "d" | "diagonal") => {
                editor.tile_flip_diagonal = !editor.tile_flip_diagonal;
                editor.msg = format!("tile flip diag: {}", editor.tile_flip_diagonal);
            }
            _ => editor.msg = "usage: :tileflip x|y|diag|none".into(),
        },
        "tilemode" => match arg {
            Some("manual" | "tile") => {
                editor.tile_pixel_mode = false;
                editor.tile_edit_mode = crate::editor::TileMode::Manual;
                editor.msg = "tile mode: manual".into();
            }
            Some("auto") => {
                editor.tile_pixel_mode = false;
                editor.tile_edit_mode = crate::editor::TileMode::Auto;
                editor.msg = "tile mode: auto".into();
            }
            Some("stack") => {
                editor.tile_pixel_mode = false;
                editor.tile_edit_mode = crate::editor::TileMode::Stack;
                editor.msg = "tile mode: stack".into();
            }
            Some("pixel" | "pixels") => {
                editor.tile_pixel_mode = true;
                editor.msg = "tile mode: pixel".into();
            }
            _ => editor.msg = "usage: :tilemode manual|auto|stack|pixel".into(),
        },
        "tilepick" => {
            let (x, y) = editor.cursor;
            editor.pick_tile_at(x, y);
        }
        "tileerase" | "tileclear" => {
            let (x, y) = editor.cursor;
            editor.erase_tile_at(x, y);
        }
        "textfont" | "font" => match arg {
            Some("vixel" | "bitmap" | "8x8") => {
                editor.text_font = "vixel".into();
                editor.msg = "text font: vixel (bitmap 8x8)".into();
            }
            Some(other) => {
                editor.msg = format!("text font: {other} unsupported (bitmap 'vixel' only)");
            }
            None => editor.msg = "usage: :textfont vixel".into(),
        },
        "textsize" => match arg.and_then(|v| v.parse::<u32>().ok()) {
            Some(n) => {
                editor.text_scale = (n / 8).clamp(1, 8);
                editor.msg = format!("text size: {} px", editor.text_scale * 8);
            }
            None => editor.msg = "usage: :textsize <8-64>".into(),
        },
        "textalign" | "textalignment" => match arg {
            Some("left" | "l") => {
                editor.text_align = 0;
                editor.msg = "text align: left".into();
            }
            Some("center" | "centre" | "c") => {
                editor.text_align = 1;
                editor.msg = "text align: center".into();
            }
            Some("right" | "r") => {
                editor.text_align = 2;
                editor.msg = "text align: right".into();
            }
            _ => editor.msg = "usage: :textalign left|center|right".into(),
        },
        "textobj" | "textobject" => match arg.filter(|t| !t.is_empty()) {
            Some(text) => editor.set_text_object(text),
            None => editor.msg = editor.text_object_summary(),
        },
        "text" | "t" => {
            let Some(text) = arg.filter(|t| !t.is_empty()) else {
                editor.msg = "usage: :text <string> (stamps at the cursor)".into();
                return;
            };
            let (x, y) = editor.cursor;
            editor.text = text.to_string();
            editor.stamp_text(text, x, y);
        }
        "perfect" | "pixelperfect" => match arg {
            Some("on" | "true" | "1") => {
                editor.pixel_perfect = true;
                editor.msg = "pixel-perfect: on".into();
            }
            Some("off" | "false" | "0") => {
                editor.pixel_perfect = false;
                editor.msg = "pixel-perfect: off".into();
            }
            _ => editor.msg = "usage: :perfect on|off".into(),
        },
        "pressure" => match arg {
            Some("auto") => editor.pressure_auto(),
            Some(v) => match v.parse::<f32>() {
                Ok(p) => editor.set_pressure(p / 100.0),
                Err(_) => editor.msg = "usage: :pressure <0-100>|auto".into(),
            },
            None => {
                editor.msg = format!(
                    "pressure: {:.0}% ({})",
                    editor.pressure() * 100.0,
                    editor.pressure_source()
                );
            }
        },
        "brush" => {
            let rest = arg.unwrap_or("").trim();
            let mut it = rest.split_whitespace();
            match it.next() {
                Some("square") | Some("circle") => {
                    let kind = if rest.starts_with("square") {
                        crate::brush::BrushKind::Square
                    } else {
                        crate::brush::BrushKind::Circle
                    };
                    match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => {
                            editor.brush_size = n.clamp(1, 64);
                            editor.set_brush_kind(kind);
                            editor.msg = format!("brush: {} {} px", kind.name(), editor.brush_size);
                        }
                        None => editor.msg = "usage: :brush square|circle <1-64>".into(),
                    }
                }
                Some("custom") => {
                    if editor.custom_brush.is_some() {
                        editor.set_brush_kind(crate::brush::BrushKind::Custom);
                    } else {
                        editor.msg = "no custom brush; use :brush load <file.png>".into();
                    }
                }
                Some("spacing") => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                    Some(pct) => editor.set_brush_spacing(pct),
                    None => editor.msg = "usage: :brush spacing <1-200>".into(),
                },
                Some("load") => {
                    let path = it.collect::<Vec<_>>().join(" ");
                    if path.is_empty() {
                        editor.msg = "usage: :brush load <file.png>".into();
                    } else {
                        if let Err(e) = editor.load_brush(std::path::Path::new(&path)) {
                            editor.msg = format!("brush load failed: {e}");
                        }
                    }
                }
                Some("save") => {
                    let name = it.next().unwrap_or("brush");
                    if let Err(e) = editor.save_brush(name) {
                        editor.msg = format!("brush save failed: {e}");
                    }
                }
                Some("use") => match it.next() {
                    Some(name) => {
                        if let Err(e) = editor.use_brush(name) {
                            editor.msg = format!("brush: {e}");
                        }
                    }
                    None => editor.msg = "usage: :brush use <name>".into(),
                },
                Some("list") => {
                    let names = crate::brush::list();
                    editor.msg = if names.is_empty() {
                        "brush library: empty".into()
                    } else {
                        format!("brushes: {}", names.join(", "))
                    };
                }
                Some(n) => match n.parse::<u32>() {
                    Ok(n) => {
                        editor.brush_size = n.clamp(1, 64);
                        editor.set_brush_kind(crate::brush::BrushKind::Square);
                        editor.msg = format!("brush: {} px", editor.brush_size);
                    }
                    Err(_) => {
                        editor.msg = "usage: :brush <1-64> | square|circle <n> | \
                                      spacing <pct> | load <png> | save <name> | use <name> | list"
                            .into();
                    }
                },
                None => editor.msg = "usage: :brush <1-64>".into(),
            }
        }
        "grid" => match arg.and_then(|v| v.parse::<u32>().ok()) {
            Some(n) => {
                editor.grid = n.max(1);
                editor.msg = format!("grid: {} px", editor.grid);
            }
            None => editor.msg = "usage: :grid <px>".into(),
        },
        "snap" => match arg {
            Some("on" | "true" | "1") => {
                editor.snap = true;
                editor.msg = "snap: on".into();
            }
            Some("off" | "false" | "0") => {
                editor.snap = false;
                editor.msg = "snap: off".into();
            }
            None => {
                editor.snap = !editor.snap;
                editor.msg = format!("snap: {}", if editor.snap { "on" } else { "off" });
            }
            _ => editor.msg = "usage: :snap on|off".into(),
        },
        "wand" | "tolerance" => {
            if let Some(n) = arg.and_then(|s| s.parse::<u8>().ok()) {
                editor.wand_tolerance = n;
                editor.msg = format!("wand tolerance: {n}");
            } else {
                editor.msg = "usage: :wand <0-255>".into();
            }
        }
        "contig" | "contiguous" => match arg {
            Some("on" | "true" | "1") => {
                editor.contiguous = true;
                editor.msg = "contiguous: on".into();
            }
            Some("off" | "false" | "0") => {
                editor.contiguous = false;
                editor.msg = "contiguous: off".into();
            }
            _ => editor.msg = "usage: :contig on|off".into(),
        },
        "wandall" | "wandlayers" => {
            let on = matches!(arg, Some("on" | "true" | "1" | "all"));
            if arg.is_some() {
                editor.wand_all_layers = on;
                editor.msg = format!("wand all layers: {}", if on { "on" } else { "off" });
            } else {
                editor.msg = "usage: :wandall on|off".into();
            }
        }
        "renderer" => {
            let name = arg.unwrap_or("").to_lowercase();
            match name.as_str() {
                "glow" | "opengl" | "gl" => {
                    editor.renderer = "glow".into();
                    editor.msg = "renderer: glow (applies on restart)".into();
                }
                "wgpu" => {
                    editor.renderer = "wgpu".into();
                    editor.msg =
                        "renderer: wgpu (applies on restart; needs a build with --features wgpu)"
                            .into();
                }
                _ => editor.msg =
                    "usage: :renderer glow|wgpu   (wgpu also needs --features wgpu at build time)"
                        .into(),
            }
        }
        "profile" | "colorspace" => {
            let rest = arg.unwrap_or("");
            let mut it = rest.split_whitespace();
            let sub = it.next().unwrap_or("");
            match sub {
                "" => editor.msg = editor.profile_summary(),
                "srgb" | "linear" => {
                    if let Some(p) = crate::sprite::ColorProfile::parse(sub) {
                        editor.set_color_profile(p);
                    }
                }
                "icc" => match it.next() {
                    Some(path) => match std::fs::read(path) {
                        Ok(bytes) => editor.set_ase_profile(Some(crate::sprite::AseProfile {
                            kind: 2,
                            flags: 0,
                            gamma: 0.0,
                            icc: bytes,
                        })),
                        Err(e) => editor.msg = format!("profile: cannot read {path}: {e}"),
                    },
                    None => editor.msg = "usage: :profile icc <file>".into(),
                },
                "clear" | "none" => editor.set_ase_profile(None),
                "gamma" => match it.next().and_then(|v| v.parse::<f32>().ok()) {
                    Some(g) => {
                        let mut p = editor.sprite.ase_profile.clone().unwrap_or(
                            crate::sprite::AseProfile {
                                kind: 1,
                                ..Default::default()
                            },
                        );
                        if p.kind == 0 {
                            p.kind = 1;
                        }
                        p.flags |= 1;
                        p.gamma = g.max(0.0);
                        editor.set_ase_profile(Some(p));
                    }
                    None => editor.msg = "usage: :profile gamma <g>".into(),
                },
                _ => {
                    editor.msg =
                        "usage: :profile [srgb|linear | icc <file> | gamma <g> | clear]".into();
                }
            }
        }
        "ink" => match arg.and_then(crate::editor::Ink::parse) {
            Some(ink) => editor.set_ink(ink),
            None => editor.msg = "usage: :ink simple|lockalpha|replace|shading|erase".into(),
        },
        "alphalock" | "alphlock" => match arg {
            Some("on") | Some("true") | Some("1") => editor.set_alpha_lock(true),
            Some("off") | Some("false") | Some("0") => editor.set_alpha_lock(false),
            _ => editor.msg = "usage: :alphalock on|off".into(),
        },
        "mode" | "colormode" => match arg.and_then(crate::sprite::ColorMode::parse) {
            Some(m) => editor.set_color_mode(m),
            None => editor.msg = "usage: :mode auto|rgba|grayscale|indexed".into(),
        },
        "dither" => match arg {
            None => editor.msg = format!("dither: {}", editor.dither.name()),
            Some(a) => match crate::sprite::DitherMode::parse(a) {
                Some(m) => editor.set_dither(m),
                None => {
                    editor.msg = "usage: :dither none|bayer2|bayer4|bayer8|floyd".into();
                }
            },
        },
        "aspect" | "ratio" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let x = it.next().and_then(|v| v.parse::<u16>().ok());
            let y = it.next().and_then(|v| v.parse::<u16>().ok());
            match (x, y) {
                (Some(x), Some(y)) => editor.set_pixel_aspect(x, y),
                _ => editor.msg = "usage: :aspect <x> <y>".into(),
            }
        }
        "sgrid" => {
            let v: Vec<i64> = arg
                .unwrap_or("")
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            match v.as_slice() {
                [w, h] => editor.set_sprite_grid(
                    (*w).clamp(0, u32::MAX as i64) as u32,
                    (*h).clamp(0, u32::MAX as i64) as u32,
                ),
                [x, y, w, h] => editor.set_sprite_grid_bounds(
                    (*x).clamp(i16::MIN as i64, i16::MAX as i64) as i32,
                    (*y).clamp(i16::MIN as i64, i16::MAX as i64) as i32,
                    (*w).clamp(0, u32::MAX as i64) as u32,
                    (*h).clamp(0, u32::MAX as i64) as u32,
                ),
                _ => editor.msg = "usage: :sgrid [x y] <w> <h>".into(),
            }
        }
        "rotate" => match arg.and_then(|v| v.parse::<f32>().ok()) {
            Some(deg) => editor.free_transform(deg, 1.0, 1.0, 0.0),
            None => editor.msg = "usage: :rotate <degrees>".into(),
        },
        "scale" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let sx = it.next().and_then(|v| v.parse::<f32>().ok());
            let sy = it.next().and_then(|v| v.parse::<f32>().ok());
            match (sx, sy) {
                (Some(sx), sy) => editor.free_transform(0.0, sx, sy.unwrap_or(sx), 0.0),
                _ => editor.msg = "usage: :scale <sx> [sy]".into(),
            }
        }
        "skew" => match arg.and_then(|v| v.parse::<f32>().ok()) {
            Some(deg) => editor.free_transform(0.0, 1.0, 1.0, deg),
            None => editor.msg = "usage: :skew <degrees>".into(),
        },
        "fill" => {
            let color = arg.and_then(crate::color::parse_hex).unwrap_or(editor.fg);
            editor.fill_selection(color);
        }
        "stroke" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let width = it.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
            let mut inside = true;
            let mut color = editor.fg;
            for tok in it {
                match tok {
                    "outside" | "out" => inside = false,
                    "inside" | "in" => inside = true,
                    _ => {
                        if let Some(c) = crate::color::parse_hex(tok) {
                            color = c;
                        }
                    }
                }
            }
            editor.stroke_selection(width, inside, color);
        }
        "rotsprite" => match arg {
            Some("on") | Some("1") => {
                editor.transform_rotsprite = true;
                editor.msg = "rotsprite: on".into();
            }
            Some("off") | Some("0") => {
                editor.transform_rotsprite = false;
                editor.msg = "rotsprite: off".into();
            }
            _ => {
                editor.transform_rotsprite = !editor.transform_rotsprite;
                editor.msg = format!(
                    "rotsprite: {}",
                    if editor.transform_rotsprite {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
        },
        "pivot" => match arg {
            Some("clear") | Some("none") => {
                editor.transform_pivot = None;
                editor.msg = "transform pivot: bbox centre".into();
            }
            Some(v) => {
                let mut it = v.split_whitespace();
                let x = it.next().and_then(|n| n.parse::<f32>().ok());
                let y = it.next().and_then(|n| n.parse::<f32>().ok());
                match (x, y) {
                    (Some(x), Some(y)) => {
                        editor.transform_pivot = Some((x, y));
                        editor.msg = format!("transform pivot: {x:.1},{y:.1}");
                    }
                    _ => editor.msg = "usage: :pivot <x> <y>|clear".into(),
                }
            }
            None => {
                editor.msg = format!(
                    "transform pivot: {}",
                    editor
                        .transform_pivot
                        .map(|(x, y)| format!("{x:.1},{y:.1}"))
                        .unwrap_or_else(|| "bbox centre".into())
                )
            }
        },
        "filter" | "fx" => {
            let args: Vec<&str> = arg.unwrap_or("").split_whitespace().collect();
            match crate::editor::Filter::parse_args(&args) {
                Some(f) => editor.filter(f),
                None => {
                    editor.msg =
                        "usage: :filter invert|brightness|contrast|hue|saturation|desaturate|\
                         curves <s> <m> <h>|levels <in_b> <in_w> <gamma> [out_b] [out_w]|\
                         balance <r> <g> <b>|posterize <levels>|outline <r>|blur <r>|\
                         sharpen <k>|median <r>|convolve <k0..k8> [div] [bias]|emboss"
                            .into()
                }
            }
        }
        "sel" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let op = it.next().unwrap_or("");
            let rest: Vec<&str> = it.collect();
            let n: u32 = rest.first().and_then(|v| v.parse().ok()).unwrap_or(1);
            match op {
                "expand" | "grow" => editor.selection_expand(n),
                "contract" | "shrink" => editor.selection_contract(n),
                "border" | "ring" => editor.selection_border(n),
                "feather" | "soft" => editor.selection_feather(n),
                "invert" | "inv" => editor.selection_invert(),
                "none" | "off" => editor.selection_none(),
                "all" => editor.select_all(),
                "color" | "colour" | "range" => {
                    let mut color = editor.fg;
                    let mut tolerance = editor.wand_tolerance;
                    for tok in &rest {
                        if let Some(c) = crate::color::parse_hex(tok) {
                            color = c;
                        } else if let Ok(t) = tok.parse::<u8>() {
                            tolerance = t;
                        }
                    }
                    editor.selection_from_color(color, tolerance, false);
                }
                "alpha" => editor.selection_from_alpha(rest.first() == Some(&"all")),
                "rotate" => match rest.first().and_then(|v| v.parse::<f32>().ok()) {
                    Some(deg) => editor.transform_selection_mask(deg, 1.0, 1.0, 0.0),
                    None => editor.msg = "usage: :sel rotate <deg>".into(),
                },
                "scale" => {
                    let sx = rest.first().and_then(|v| v.parse::<f32>().ok());
                    let sy = rest.get(1).and_then(|v| v.parse::<f32>().ok());
                    match sx {
                        Some(sx) => editor.transform_selection_mask(0.0, sx, sy.unwrap_or(sx), 0.0),
                        None => editor.msg = "usage: :sel scale <sx> [sy]".into(),
                    }
                }
                "skew" => match rest.first().and_then(|v| v.parse::<f32>().ok()) {
                    Some(deg) => editor.transform_selection_mask(0.0, 1.0, 1.0, deg),
                    None => editor.msg = "usage: :sel skew <deg>".into(),
                },
                "offset" => {
                    let dx = rest.first().and_then(|v| v.parse::<i32>().ok());
                    let dy = rest.get(1).and_then(|v| v.parse::<i32>().ok());
                    match (dx, dy) {
                        (Some(dx), Some(dy)) => editor.offset_selection(dx, dy),
                        _ => editor.msg = "usage: :sel offset <dx> <dy>".into(),
                    }
                }
                _ => {
                    editor.msg =
                        "usage: :sel expand|contract|border|feather <n> | invert | none | all | \
                         color [#hex] [tol] | alpha [all] | rotate|skew <deg> | scale <sx> [sy] | \
                         offset <dx> <dy>"
                            .into()
                }
            }
        }
        "selop" | "selection" => {
            if let Some(name) = arg
                && editor.set_selection_op(name)
            {
                editor.msg = format!("selection: {}", editor.selection_op.name());
            } else {
                editor.msg = "usage: :selop replace|add|subtract|intersect".into();
            }
        }
        "grad" | "gradient" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            match it.next() {
                Some("radial") | Some("rad") => {
                    editor.gradient_radial = true;
                    editor.msg = "gradient: radial".into();
                }
                Some("linear") | Some("lin") => {
                    editor.gradient_radial = false;
                    editor.msg = "gradient: linear".into();
                }
                Some("clear") => {
                    editor.gradient_stops.clear();
                    editor.msg = "gradient: fg -> bg".into();
                }
                Some("stops") => {
                    let list: Vec<String> = editor
                        .gradient_stops
                        .iter()
                        .map(|(p, c)| {
                            format!(
                                "{:.0}% #{:02x}{:02x}{:02x}{:02x}",
                                p * 100.0,
                                c.0,
                                c.1,
                                c.2,
                                c.3
                            )
                        })
                        .collect();
                    editor.msg = if list.is_empty() {
                        "gradient: fg -> bg".into()
                    } else {
                        list.join("  ")
                    };
                }
                Some("dither") => match it.next().and_then(crate::sprite::DitherMode::parse) {
                    Some(mode) => {
                        editor.gradient_dither = mode;
                        editor.msg = format!("gradient dither: {}", mode.name());
                    }
                    None => editor.msg = "usage: :grad dither none|bayer2|bayer4|bayer8".into(),
                },
                Some("preset") => match it.next() {
                    Some(name) => {
                        if !editor.gradient_preset(name) {
                            editor.msg =
                                "presets: fg-bg, fg-transparent, bg-transparent, black-white, \
                                 white-black, rainbow, fire"
                                    .into();
                        }
                    }
                    None => editor.msg = "usage: :grad preset <name>".into(),
                },
                Some("stop") => match it.next().and_then(|v| v.parse::<f32>().ok()) {
                    Some(pos) => {
                        let color = it
                            .next()
                            .and_then(crate::color::parse_hex)
                            .unwrap_or(editor.fg);
                        editor.gradient_set_stop(pos / 100.0, color);
                    }
                    None => editor.msg = "usage: :grad stop <0-100> [#rrggbb]".into(),
                },
                None => {
                    editor.msg = format!(
                        "gradient: {} · {} stops · dither {}",
                        if editor.gradient_radial {
                            "radial"
                        } else {
                            "linear"
                        },
                        editor.gradient_stops.len(),
                        editor.gradient_dither.name()
                    );
                }
                _ => {
                    editor.msg = "usage: :grad linear|radial|dither <mode>|preset <name>|\
                                  stop <pct> [#rrggbb]|stops|clear"
                        .into();
                }
            }
        }
        "script" => match arg {
            Some(path) => match crate::script::run_file(editor, std::path::Path::new(path)) {
                Ok(out) => {
                    editor.msg = if out.is_empty() {
                        format!("script: {path}")
                    } else {
                        out.lines().last().unwrap_or("script finished").to_string()
                    }
                }
                Err(e) => editor.msg = format!("script: {e}"),
            },
            None => editor.msg = "usage: :script <file.lua>".into(),
        },
        "lua" => match arg {
            Some(code) => match crate::script::run(editor, code, "inline") {
                Ok(out) => {
                    editor.msg = if out.is_empty() {
                        "lua: ok".into()
                    } else {
                        out.lines().last().unwrap_or("lua: ok").to_string()
                    }
                }
                Err(e) => editor.msg = format!("lua: {e}"),
            },
            None => editor.msg = "usage: :lua <code>".into(),
        },
        "bgcolor" | "backgroundcolor" => match arg {
            Some("clear" | "none") => {
                editor.set_background_color(None);
                editor.msg = "background colour: none".into();
            }
            Some(spec) => match crate::color::parse_hex(spec) {
                Some(c) => {
                    editor.set_background_color(Some(c));
                    editor.msg = format!("background: #{:02x}{:02x}{:02x}", c.0, c.1, c.2);
                }
                None => editor.msg = "usage: :bgcolor #rrggbb|clear".into(),
            },
            None => {
                editor.msg = match editor.sprite.background_color() {
                    Some(c) => format!("background: #{:02x}{:02x}{:02x}", c.0, c.1, c.2),
                    None => "background: none".into(),
                };
            }
        },
        "fg" | "bg" => match arg.and_then(crate::color::parse_hex) {
            Some(c) => {
                if name == "fg" {
                    editor.set_fg(c);
                } else {
                    editor.set_bg(c);
                }
                editor.msg = format!("{name}: #{:02x}{:02x}{:02x}", c.0, c.1, c.2);
            }
            None => editor.msg = format!("usage: :{name} #rrggbb"),
        },
        "contour" | "outline" => {
            for tok in arg.unwrap_or("").split_whitespace() {
                match tok {
                    "inside" => editor.contour_inside = true,
                    "outside" => editor.contour_inside = false,
                    _ => match tok.parse::<u32>() {
                        Ok(n) => editor.contour_width = n.clamp(1, 64),
                        Err(_) => {
                            editor.msg = "usage: :contour [1-64] [inside|outside]".into();
                            return;
                        }
                    },
                }
            }
            let (x, y) = editor.cursor;
            editor.contour_at(x, y);
            editor.msg = format!(
                "contour: {} px {}",
                editor.contour_width,
                if editor.contour_inside {
                    "inside"
                } else {
                    "outside"
                }
            );
        }
        "spray" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            match it.next().and_then(|v| v.parse::<u32>().ok()) {
                Some(rate) => {
                    editor.spray_rate = rate.clamp(1, 64);
                    if let Some(radius) = it.next().and_then(|v| v.parse::<u32>().ok()) {
                        editor.spray_radius = radius.clamp(1, 64);
                    }
                    editor.msg = format!(
                        "spray: {} dots, radius {}",
                        editor.spray_rate, editor.spray_radius
                    );
                }
                None => editor.msg = "usage: :spray <1-64> [radius 1-64]".into(),
            }
        }
        "shade" => match arg.and_then(|v| v.parse::<u32>().ok()) {
            Some(n) => {
                editor.shade_strength = n.clamp(1, 100);
                editor.msg = format!("shade strength: {}%", editor.shade_strength);
            }
            None => editor.msg = "usage: :shade <1-100>".into(),
        },
        "blur" => match arg.and_then(|v| v.parse::<u32>().ok()) {
            Some(n) => {
                editor.blur_radius = n.clamp(1, 32);
                editor.msg = format!("blur radius: {} px", editor.blur_radius);
            }
            None => editor.msg = "usage: :blur <1-32>".into(),
        },
        "ops" => {
            let Some(path) = arg.filter(|p| !p.is_empty()) else {
                editor.msg = "usage: :ops <file.json>".into();
                return;
            };
            let parsed = std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    serde_json::from_str::<Vec<ops::Op>>(&text).map_err(|e| e.to_string())
                });
            match parsed {
                Ok(list) => {
                    let changed = editor.apply(&list);
                    editor.msg = format!("ops: {} applied, {changed} px", list.len());
                }
                Err(e) => editor.msg = format!("ops failed: {e}"),
            }
        }
        "pal" | "palette" => {
            let rest = arg.unwrap_or("");
            let (sub, path) = match rest.find(char::is_whitespace) {
                Some(i) => (&rest[..i], rest[i..].trim_start()),
                None => (rest, ""),
            };
            match sub {
                "from-sprite" | "sprite" => {
                    editor.palette_from_sprite();
                }
                "lock" | "unlock" => {
                    let on = match path {
                        "" => sub == "lock",
                        "on" | "true" | "1" => true,
                        "off" | "false" | "0" => false,
                        _ => {
                            editor.msg = "usage: :pal lock on|off".into();
                            return;
                        }
                    };
                    editor.set_palette_lock(on);
                }
                "move" if !path.is_empty() => {
                    let mut it = path
                        .split_whitespace()
                        .filter_map(|v| v.parse::<usize>().ok());
                    match (it.next(), it.next()) {
                        (Some(from), Some(to)) => editor.move_palette(from, to),
                        _ => editor.msg = "usage: :pal move <from> <to>".into(),
                    }
                }
                "preset" if !path.is_empty() => {
                    editor.load_palette_preset(path);
                }
                "insert" => {
                    let mut it = path.split_whitespace();
                    let idx = it.next().and_then(|v| v.parse::<usize>().ok());
                    let color = it
                        .next()
                        .and_then(crate::color::parse_hex)
                        .unwrap_or(editor.fg);
                    match idx {
                        Some(i) => editor.insert_palette_at(i, color),
                        None => editor.msg = "usage: :pal insert <index> [#rrggbb]".into(),
                    }
                }
                "delete" | "remove" => {
                    match path
                        .split_whitespace()
                        .next()
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        Some(i) => editor.remove_palette_at(i),
                        None => editor.msg = "usage: :pal delete <index>".into(),
                    }
                }
                "sort" if !path.is_empty() => {
                    editor.sort_palette(path);
                }
                "sort" => editor.sort_palette("rgb"),
                "name" => {
                    let mut it = path.splitn(2, char::is_whitespace);
                    let idx = it.next().and_then(|v| v.parse::<usize>().ok());
                    let name = it.next().unwrap_or("").trim();
                    match idx {
                        Some(i) => editor.set_palette_name(i, name),
                        None => editor.msg = "usage: :pal name <index> [name]".into(),
                    }
                }
                "load" | "save" if !path.is_empty() => {
                    let p = std::path::Path::new(path);
                    if sub == "load" {
                        match palette::load(p) {
                            Ok(colors) => {
                                let n = colors.len();
                                editor.load_palette(colors);
                                editor.msg = format!("palette: {n} colors");
                            }
                            Err(e) => editor.msg = format!("palette load failed: {e}"),
                        }
                    } else {
                        match palette::save(&editor.palette, p) {
                            Ok(()) => editor.msg = "palette saved".into(),
                            Err(e) => editor.msg = format!("palette save failed: {e}"),
                        }
                    }
                }
                _ => {
                    editor.msg = "usage: :pal from-sprite | lock on|off | move <from> <to> | \
                                  insert <i> [#hex] | delete <i> | sort [rgb|luma|hue] | \
                                  name <i> <name> | preset <name> | load|save <file>"
                        .into();
                }
            }
        }
        "slices" | "slice" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let dir = it.next().unwrap_or("").to_string();
            if dir.is_empty() {
                if editor.sprite.slices.is_empty() {
                    editor.msg = "slices: none".into();
                } else {
                    let names: Vec<String> = editor
                        .sprite
                        .slices
                        .iter()
                        .map(|s| format!("{} ({} keys)", s.name, s.keys.len()))
                        .collect();
                    editor.msg = format!("slices: {}", names.join(", "));
                }
            } else {
                let mode = it.next();
                let named = matches!(mode, Some("named") | Some("slices"));
                let cols = if named {
                    0
                } else {
                    mode.and_then(|v| v.parse().ok()).unwrap_or(0)
                };
                let rows = if named {
                    0
                } else {
                    it.next().and_then(|v| v.parse().ok()).unwrap_or(0)
                };
                let stem = editor
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "sprite".into());
                let exported = if named {
                    io::export_named_slices(
                        &editor.sprite,
                        editor.active_frame,
                        std::path::Path::new(&dir),
                        &stem,
                    )
                } else {
                    io::export_slices(
                        &editor.sprite,
                        editor.active_frame,
                        std::path::Path::new(&dir),
                        &stem,
                        cols,
                        rows,
                    )
                };
                match exported {
                    Ok(paths) => editor.msg = format!("slices: {} files in {dir}", paths.len()),
                    Err(e) => editor.msg = format!("slices failed: {e}"),
                }
            }
        }
        "exportcfg" | "exportconfig" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let stem = it.next().map(std::path::PathBuf::from);
            let mut cfg = crate::io::ExportConfig::default();
            let mut bad = false;
            while let Some(tok) = it.next() {
                match tok {
                    "--frames" => {
                        let a = it.next().and_then(|v| v.parse::<usize>().ok());
                        let b = it.next().and_then(|v| v.parse::<usize>().ok());
                        match (a, b) {
                            (Some(a), Some(b)) if a > 0 && b > 0 => {
                                cfg.frames = Some((a - 1, b - 1));
                            }
                            _ => bad = true,
                        }
                    }
                    "--tag" => cfg.tag = it.next().map(str::to_string),
                    "--layers" => match it.next() {
                        Some("all") => cfg.layers = crate::io::ExportLayers::All,
                        Some("visible") => cfg.layers = crate::io::ExportLayers::Visible,
                        Some("indices") => {
                            cfg.layers = crate::io::ExportLayers::Indices;
                            cfg.layer_indices = it
                                .next()
                                .unwrap_or("")
                                .split(',')
                                .filter_map(|v| v.parse::<usize>().ok())
                                .map(|v| v.saturating_sub(1))
                                .collect();
                        }
                        _ => bad = true,
                    },
                    "--ignore-empty" => cfg.ignore_empty = true,
                    "--merge" => cfg.merge_duplicates = true,
                    "--trim" => cfg.trim = true,
                    "--scale" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.scale = n.clamp(1, 16),
                        None => bad = true,
                    },
                    "--padding" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.padding = n.min(64),
                        None => bad = true,
                    },
                    "--sheet" => cfg.sheet = true,
                    "--no-sheet" => cfg.sheet = false,
                    "--gif" => cfg.gif = true,
                    "--no-gif" => cfg.gif = false,
                    "--meta" => cfg.meta = true,
                    "--no-meta" => cfg.meta = false,
                    "--sequence" => cfg.sequence = true,
                    "--per-layer" => {
                        cfg.per_layer = true;
                        cfg.sequence = true;
                    }
                    "--pattern" => match it.next() {
                        Some(p) => cfg.filename = p.to_string(),
                        None => bad = true,
                    },
                    "--layout" => match it.next().and_then(crate::io::SheetLayout::parse) {
                        Some(l) => cfg.sheet_layout = Some(l),
                        None => bad = true,
                    },
                    "--columns" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.columns = Some(n.max(1)),
                        None => bad = true,
                    },
                    "--rows" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.rows = Some(n.max(1)),
                        None => bad = true,
                    },
                    "--border" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.border_padding = n.min(64),
                        None => bad = true,
                    },
                    "--shape" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.shape_padding = n.min(64),
                        None => bad = true,
                    },
                    "--inner" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.inner_padding = n.min(64),
                        None => bad = true,
                    },
                    "--extrude" => match it.next().and_then(|v| v.parse::<u32>().ok()) {
                        Some(n) => cfg.extrude = n.min(64),
                        None => bad = true,
                    },
                    "--atlas" => match it.next().and_then(crate::io::AtlasFormat::parse) {
                        Some(a) => cfg.atlas = a,
                        None => bad = true,
                    },
                    "--gif-loop" => match it.next().and_then(|v| v.parse::<u16>().ok()) {
                        Some(n) => cfg.gif_loop = n,
                        None => bad = true,
                    },
                    "--gif-dither" => cfg.gif_dither = true,
                    "--gif-colors" => match it.next().and_then(|v| v.parse::<u16>().ok()) {
                        Some(n) => cfg.gif_colors = n.clamp(2, 256),
                        None => bad = true,
                    },
                    _ => bad = true,
                }
            }
            if cfg.sheet_layout.is_none()
                && (cfg.border_padding > 0
                    || cfg.shape_padding > 0
                    || cfg.inner_padding > 0
                    || cfg.extrude > 0)
            {
                cfg.sheet_layout = Some(crate::io::SheetLayout::Rows);
            }
            match (bad, stem) {
                (false, Some(stem)) => {
                    editor.export_config = cfg.clone();
                    match crate::io::export_config(&editor.sprite, &cfg, &stem) {
                        Ok(paths) => {
                            editor.msg = format!(
                                "exported {}",
                                paths
                                    .iter()
                                    .filter_map(|p| p.file_name())
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                        }
                        Err(e) => editor.msg = format!("export failed: {e:#}"),
                    }
                }
                _ => {
                    editor.msg = "usage: :exportcfg <stem> [--frames a b] [--tag name] \
                        [--layers all|visible|indices i,j] [--ignore-empty] [--merge] \
                        [--trim] [--scale n] [--padding n] [--no-sheet] [--no-gif] \
                        [--no-meta] [--sequence] [--per-layer] [--pattern <tpl>] \
                        [--layout h|v|rows|columns|packed] [--columns n] [--rows n] \
                        [--border n] [--shape n] [--inner n] [--extrude n] \
                        [--atlas vixel|hash|array] [--gif-loop n] [--gif-dither] [--gif-colors n]"
                        .into();
                }
            }
        }
        "export" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let target = it.next().unwrap_or("");
            let scale = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
            if target.is_empty() || it.next().is_some() || !(1..=16).contains(&scale) {
                editor.msg = "usage: :export <stem | file.png | file.ase> [scale 1-16]".into();
            } else {
                let path = PathBuf::from(target);
                // `.png` = this frame, `.ase` = the whole sprite, a bare stem =
                // the animation bundle (sheet + gif + json), which is what a
                // paperdoll pipeline actually needs. `scale` upscales the
                // raster outputs with nearest-neighbour.
                let res = if io::is_ase(&path) {
                    if scale != 1 {
                        Err(anyhow::anyhow!(".ase export has no scale"))
                    } else {
                        ase::save(&editor.sprite, &path).map(|_| "exported".to_string())
                    }
                } else if io::is_image(&path) {
                    io::export_image_scaled(&editor.sprite, editor.active_frame, &path, scale)
                        .map(|_| "exported".to_string())
                } else {
                    io::export_animation_scaled(&editor.sprite, &path, None, scale).map(|paths| {
                        format!(
                            "exported {}",
                            paths
                                .iter()
                                .filter_map(|p| p.file_name())
                                .map(|n| n.to_string_lossy().into_owned())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                };
                match res {
                    Ok(what) => editor.msg = what,
                    Err(err) => editor.msg = format!("export failed: {err}"),
                }
            }
        }
        "importatlas" | "atlasimport" => {
            let mut parts = arg.unwrap_or("").split_whitespace();
            let image = parts.next().map(std::path::PathBuf::from);
            let meta = parts.next().map(std::path::PathBuf::from);
            match image {
                Some(image) => {
                    let meta = meta.unwrap_or_else(|| image.with_extension("json"));
                    match crate::io::import_atlas(&image, &meta) {
                        Ok(sprite) => {
                            editor.replace_sprite(sprite);
                            editor.msg = "atlas imported".into();
                        }
                        Err(e) => editor.msg = format!("atlas import failed: {e:#}"),
                    }
                }
                None => editor.msg = "usage: :importatlas <sheet.png> [meta.json|xml]".into(),
            }
        }
        "importseq" | "importsequence" => {
            let Some(dir) = arg.filter(|p| !p.is_empty()) else {
                editor.msg = "usage: :importseq <dir>".into();
                return;
            };
            match crate::io::import_sequence_dir(std::path::Path::new(dir)) {
                Ok(sprite) => {
                    editor.replace_sprite(sprite);
                    editor.msg =
                        format!("sequence imported: {} frames", editor.sprite.frames.len());
                }
                Err(e) => editor.msg = format!("sequence import failed: {e:#}"),
            }
        }
        "importsheet" | "importsheet!" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let p = it.next().unwrap_or("");
            let cols = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            let rows = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            if p.is_empty() {
                editor.msg = "usage: :importsheet <sheet.png> [cols rows]".into();
            } else if replace_ok(editor, name.ends_with('!'), "importsheet") {
                match io::import_sheet(PathBuf::from(p).as_path(), cols, rows) {
                    Ok(s) => {
                        let n = s.frames.len();
                        editor.replace_sprite(s);
                        editor.path = None;
                        editor.msg = format!("imported sheet: {n} frames");
                    }
                    Err(e) => editor.msg = format!("import sheet failed: {e}"),
                }
            }
        }
        "import" | "import!" => {
            if let Some(p) = arg {
                if !replace_ok(editor, name == "import!", "import") {
                    return;
                }
                match io::import_image(PathBuf::from(p).as_path()) {
                    Ok(s) => {
                        editor.replace_sprite(s);
                        editor.msg = "imported".into();
                    }
                    Err(err) => editor.msg = format!("import failed: {err}"),
                }
            } else {
                editor.msg = "usage: :import <file.png>".into();
            }
        }
        "recent" | "recent!" => match arg.and_then(|s| s.parse::<usize>().ok()) {
            Some(n) if (1..=editor.recent.len()).contains(&n) => {
                if !replace_ok(editor, name.ends_with('!'), "recent") {
                    return;
                }
                let path = editor.recent[n - 1].clone();
                if let Err(e) = editor.open_path(&path) {
                    editor.msg = format!("open failed: {e}");
                }
            }
            Some(_) => editor.msg = "no such recent entry".into(),
            None => {
                editor.msg = if editor.recent.is_empty() {
                    "no recent files".into()
                } else {
                    editor
                        .recent
                        .iter()
                        .enumerate()
                        .map(|(i, p)| format!("{}: {}", i + 1, p.display()))
                        .collect::<Vec<_>>()
                        .join(" · ")
                };
            }
        },
        "recover" | "recover!" => {
            if !replace_ok(editor, name.ends_with('!'), "recover") {
                return;
            }
            let path = arg.map(PathBuf::from).or_else(crate::config::recovery_path);
            match path {
                Some(p) => match io::load_project(&p) {
                    Ok(s) => {
                        editor.replace_sprite(s);
                        editor.path = None;
                        editor.dirty = true;
                        editor.msg = "recovered (use :w <file>)".into();
                    }
                    Err(e) => editor.msg = format!("recover failed: {e}"),
                },
                None => editor.msg = "no recovery path".into(),
            }
        }
        "open" | "load" | "open!" | "load!" => {
            if let Some(p) = arg {
                if !replace_ok(editor, name.ends_with('!'), name.trim_end_matches('!')) {
                    return;
                }
                match editor.open_path(PathBuf::from(p).as_path()) {
                    Ok(()) => {}
                    Err(err) => editor.msg = format!("load failed: {err}"),
                }
            } else {
                editor.msg = "usage: :open <file.vpx|file.ase|file.png>".into();
            }
        }
        "help" => editor.mode = Mode::Help,
        "closed" => {
            editor.msg = if editor.closed.is_empty() {
                "no closed documents".into()
            } else {
                let list: Vec<String> = editor
                    .closed
                    .iter()
                    .rev()
                    .map(|p| p.display().to_string())
                    .collect();
                format!("closed ({}): {}", list.len(), list.join(", "))
            };
        }
        "reopen" => editor.request_reopen(),
        "" => {}
        _ => editor.msg = format!("unknown command: {name}"),
    }
}

/// Write the sprite to `path`, picking the format by extension (`.ase` →
/// Aseprite, else the `.vpx` JSON project). Returns whether it was written;
/// `:wq` only quits on success. Shared so `:w` and `:wq` cannot drift.
fn save_to(editor: &mut Editor, path: PathBuf) -> bool {
    match editor.save_path(&path) {
        Ok(()) => true,
        Err(err) => {
            editor.msg = format!("save failed: {err}");
            false
        }
    }
}

/// Destructive commands (`:new`, `:open`, `:import`) refuse to drop unsaved
/// edits unless forced with `!`, matching `:q`.
fn replace_ok(editor: &mut Editor, force: bool, cmd: &str) -> bool {
    if editor.dirty && !force {
        editor.msg = format!("unsaved changes (use :{cmd}! to force or :w to save)");
        false
    } else {
        true
    }
}

fn parse_dims(arg: Option<&str>, dw: u32, dh: u32) -> (u32, u32) {
    match arg {
        Some(s) => {
            let mut it = s.split_whitespace();
            let w = it.next().and_then(|v| v.parse().ok()).unwrap_or(dw);
            let h = it.next().and_then(|v| v.parse().ok()).unwrap_or(dh);
            (w, h)
        }
        None => (dw, dh),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops_file(tag: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vixel-ops-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ops.json");
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn the_ops_command_applies_a_json_list() {
        let path = ops_file("dots", r#"[{"op":"px","x":2,"y":3,"color":[9,9,9,255]}]"#);
        let mut editor = Editor::new(8, 8);
        exec_command(&mut editor, &format!("ops {}", path.display()));
        assert_eq!(editor.sprite.get(0, 0, 2, 3), Rgba::rgb(9, 9, 9));
        assert!(editor.msg.contains("1 applied"), "{}", editor.msg);

        exec_command(&mut editor, "ops");
        assert!(editor.msg.contains("usage"), "{}", editor.msg);

        exec_command(&mut editor, "ops /nonexistent/vixel-ops.json");
        assert!(editor.msg.contains("ops failed"), "{}", editor.msg);
    }

    #[test]
    fn the_new_commands_reach_their_editor_op() {
        let mut e = Editor::new(16, 16);
        // text -> pixels at the cursor
        exec_command(&mut e, "text A");
        assert!(
            (0..16).any(|x| !e.sprite.get(0, 0, x, 0).is_transparent()),
            "text stamped nothing"
        );
        // filter invert round-trips: two inverts restore the pixel
        let (l, f) = (e.active_layer, e.active_frame);
        let painted = (0..16)
            .find_map(|x| {
                (0..16)
                    .find(|y| !e.sprite.get(l, f, x, *y).is_transparent())
                    .map(|y| (x, y))
            })
            .expect("the text stamp left a pixel");
        let before = e.sprite.get(l, f, painted.0, painted.1);
        exec_command(&mut e, "filter invert");
        assert_ne!(e.sprite.get(l, f, painted.0, painted.1), before);
        exec_command(&mut e, "filter invert");
        assert_eq!(e.sprite.get(l, f, painted.0, painted.1), before);
        // selection ops on a freshly made selection
        exec_command(&mut e, "sel all");
        exec_command(&mut e, "sel contract 2");
        assert!(e.current_selection().is_some());
        exec_command(&mut e, "sel none");
        assert!(e.current_selection().is_none());
        // sprite properties and the profile
        exec_command(&mut e, "mode grayscale");
        assert_eq!(e.sprite.color_mode.name(), "grayscale");
        exec_command(&mut e, "profile linear");
        assert_eq!(e.sprite.color_profile.name(), "linear");
        exec_command(&mut e, "aspect 2 1");
        assert_eq!(e.sprite.pixel_aspect, (2, 1));
        exec_command(&mut e, "sgrid 4 4");
        assert_eq!(e.sprite.grid, (4, 4));
        // grid/snap and the free transform
        exec_command(&mut e, "renderer wgpu");
        assert_eq!(e.renderer, "wgpu");
        assert!(e.msg.contains("restart"), "{}", e.msg);
        exec_command(&mut e, "renderer nope");
        assert!(e.msg.contains("usage"), "{}", e.msg);
        exec_command(&mut e, "grid 4");
        exec_command(&mut e, "snap on");
        assert!(e.snap && e.grid == 4);
        exec_command(&mut e, "rotate 90");
        // usage strings, not panics, on bad arguments
        for bad in [
            "text",
            "filter nope",
            "rotate",
            "scale",
            "skew x",
            "sel",
            "aspect 1",
        ] {
            exec_command(&mut e, bad);
            assert!(
                e.msg.contains("usage") || e.msg.starts_with("text:"),
                "{bad} -> {}",
                e.msg
            );
        }
    }

    #[test]
    fn wq_dispatches_by_extension_and_only_quits_on_success() {
        let dir = std::env::temp_dir().join(format!("vixel-wq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let ase_path = dir.join("x.ase");
        let mut e = Editor::new(4, 4);
        exec_command(&mut e, &format!("wq {}", ase_path.display()));
        assert!(e.should_quit, "wq quits after a successful save");
        assert!(!e.dirty);
        assert!(ase::load(&ase_path).is_ok(), "`.ase` must not hold JSON");

        let missing = dir.join("missing").join("x.ase");
        let mut e = Editor::new(4, 4);
        exec_command(&mut e, &format!("wq {}", missing.display()));
        assert!(!e.should_quit, "a failed save keeps the editor open");
        assert!(e.msg.contains("save failed"), "{}", e.msg);

        exec_command(&mut e, "wq");
        assert!(!e.should_quit, "wq without a filename does not quit");
        assert!(e.msg.contains("no filename"), "{}", e.msg);
    }

    #[test]
    fn destructive_commands_refuse_unsaved_work_unless_forced() {
        let missing = std::env::temp_dir()
            .join(format!("vixel-missing-{}", std::process::id()))
            .join("x");

        let mut e = Editor::new(8, 8);
        e.dirty = true;
        exec_command(&mut e, "new 4 4");
        assert_eq!((e.sprite.width, e.sprite.height), (8, 8), "refused");
        assert!(e.msg.contains("unsaved changes"), "{}", e.msg);
        exec_command(&mut e, "new! 4 4");
        assert_eq!((e.sprite.width, e.sprite.height), (4, 4), "forced");

        e.dirty = true;
        exec_command(&mut e, &format!("open {}.vpx", missing.display()));
        assert!(e.msg.contains("unsaved changes"), "{}", e.msg);
        exec_command(&mut e, &format!("open! {}.vpx", missing.display()));
        assert!(e.msg.contains("load failed"), "{}", e.msg);

        e.dirty = true;
        exec_command(&mut e, &format!("import {}.png", missing.display()));
        assert!(e.msg.contains("unsaved changes"), "{}", e.msg);
        exec_command(&mut e, &format!("import! {}.png", missing.display()));
        assert!(e.msg.contains("import failed"), "{}", e.msg);

        e.dirty = true;
        exec_command(&mut e, &format!("importsheet {}.png", missing.display()));
        assert!(e.msg.contains("unsaved changes"), "{}", e.msg);
        exec_command(&mut e, &format!("importsheet! {}.png", missing.display()));
        assert!(e.msg.contains("import sheet failed"), "{}", e.msg);

        e.dirty = true;
        exec_command(&mut e, &format!("recover {}.vpx", missing.display()));
        assert!(e.msg.contains("unsaved changes"), "{}", e.msg);
        exec_command(&mut e, &format!("recover! {}.vpx", missing.display()));
        assert!(e.msg.contains("recover failed"), "{}", e.msg);
    }

    #[test]
    fn recover_loads_a_snapshot_and_leaves_it_dirty() {
        let dir = std::env::temp_dir().join(format!("vixel-recover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("recovery.vpx");

        let mut saved = crate::sprite::Sprite::new(4, 4);
        saved.set(0, 0, 1, 1, Rgba::rgb(7, 8, 9));
        io::save_project(&saved, &path).unwrap();

        let mut e = Editor::new(8, 8);
        exec_command(&mut e, &format!("recover {}", path.display()));
        assert_eq!((e.sprite.width, e.sprite.height), (4, 4));
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::rgb(7, 8, 9));
        assert!(e.dirty, "recovered work is unsaved");
        assert!(e.path.is_none(), "a snapshot has no project path");
        assert!(e.msg.contains("recovered"), "{}", e.msg);
    }
}
