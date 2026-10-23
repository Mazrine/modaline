use std::path::PathBuf;

use eframe::egui::{self, Event, Key, Modifiers};

use crate::color::Rgba;
use crate::editor::{Clipboard, Editor, Mode, Pending, Shape, Tool};
use crate::gui::View;
use crate::{ase, io};

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
            Event::Text(text) => handle_text(editor, text),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => handle_key(editor, view, *key, *modifiers),
            Event::Copy => clipboard_copy(editor, ui.ctx()),
            Event::Cut => clipboard_cut(editor, ui.ctx()),
            Event::PasteImage(img) => clipboard_paste_image(editor, img),
            _ => {}
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

fn handle_key(editor: &mut Editor, view: &mut View, key: Key, mods: Modifiers) {
    use Key::*;

    if mods.command {
        match key {
            R => editor.redo(),
            Z => editor.undo(),
            S => save(editor),
            Q => editor.should_quit = true,
            _ => {}
        }
        return;
    }

    match editor.mode {
        Mode::Command => match key {
            Enter => submit(editor),
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
                    editor.paint_symmetric(x, y, editor.fg);
                    editor.end_stroke();
                }
            }
            Escape => {
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

fn paint_cursor(editor: &mut Editor) {
    let (x, y) = editor.cursor;
    editor.paint_with_tool(x, y);
}

fn handle_text(editor: &mut Editor, text: &str) {
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
                editor.paint_symmetric(x, y, Rgba::TRANSPARENT);
                editor.end_stroke();
            }
        }
        'r' => {
            editor.begin_stroke();
            let (x, y) = editor.cursor;
            editor.paint_symmetric(x, y, editor.fg);
            editor.end_stroke();
        }
        's' => editor.cycle_symmetry(),
        't' => editor.toggle_filled_shapes(),
        'w' => {
            let (x, y) = editor.cursor;
            editor.magic_wand(x, y);
        }
        'y' => editor.yank(),
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

fn submit(editor: &mut Editor) {
    let cmd = std::mem::take(&mut editor.cmdline);
    editor.mode = Mode::Normal;
    exec_command(editor, &cmd);
}

fn save(editor: &mut Editor) {
    if let Some(p) = editor.path.clone() {
        match io::save_project(&editor.sprite, &p) {
            Ok(()) => {
                editor.dirty = false;
                editor.msg = "saved".into();
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
                    let res = if is_ase(&p) {
                        ase::save(&editor.sprite, &p)
                    } else {
                        io::save_project(&editor.sprite, &p)
                    };
                    match res {
                        Ok(()) => {
                            editor.path = Some(p);
                            editor.dirty = false;
                            editor.msg = "saved".into();
                        }
                        Err(err) => editor.msg = format!("save failed: {err}"),
                    }
                }
                None => editor.msg = "no filename (use :w <file>)".into(),
            }
        }
        "q" | "quit" => {
            if editor.dirty {
                editor.msg = "unsaved changes (use :q! to force)".into();
            } else {
                editor.should_quit = true;
            }
        }
        "q!" => editor.should_quit = true,
        "wq" => {
            if let Some(p) = arg.map(PathBuf::from).or_else(|| editor.path.clone())
                && io::save_project(&editor.sprite, &p).is_ok() {
                    editor.path = Some(p);
                    editor.dirty = false;
                }
            editor.should_quit = true;
        }
        "new" => {
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
        "newframe" | "nf" => editor.add_frame(),
        "delframe" | "df" => editor.del_frame(),
        "dupframe" | "dupf" => editor.duplicate_frame(),
        "newlayer" | "nl" => editor.add_layer(),
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
        "fliph" => editor.flip_h(),
        "flipv" => editor.flip_v(),
        "rot90" => editor.rotate_cw(),
        "sym" | "symmetry" => {
            if let Some(name) = arg
                && editor.set_symmetry(name) {
                    editor.msg = format!("symmetry: {}", editor.symmetry.name());
                } else {
                    editor.msg = "usage: :sym none|horizontal|vertical|both".into();
                }
        }
        "wand" | "tolerance" => {
            if let Some(n) = arg.and_then(|s| s.parse::<u8>().ok()) {
                editor.wand_tolerance = n;
                editor.msg = format!("wand tolerance: {n}");
            } else {
                editor.msg = "usage: :wand <0-255>".into();
            }
        }
        "wandall" | "wandlayers" => {
            let on = matches!(arg, Some("on" | "true" | "1" | "all"));
            if arg.is_some() {
                editor.wand_all_layers = on;
                editor.msg = format!("wand all layers: {}", if on { "on" } else { "off" });
            } else {
                editor.msg = "usage: :wandall on|off".into();
            }
        }
        "selop" | "selection" => {
            if let Some(name) = arg
                && editor.set_selection_op(name) {
                    editor.msg = format!("selection: {}", editor.selection_op.name());
                } else {
                    editor.msg = "usage: :selop replace|add|subtract|intersect".into();
                }
        }
        "grad" | "gradient" => match arg {
            Some("radial") | Some("rad") => {
                editor.gradient_radial = true;
                editor.msg = "gradient: radial".into();
            }
            Some("linear") | Some("lin") => {
                editor.gradient_radial = false;
                editor.msg = "gradient: linear".into();
            }
            _ => editor.msg = "usage: :grad linear|radial".into(),
        },
        "contour" | "outline" => {
            let (x, y) = editor.cursor;
            editor.contour_at(x, y);
        }
        "export" => {
            if let Some(p) = arg {
                let path = PathBuf::from(p);
                let res = if is_ase(&path) {
                    ase::save(&editor.sprite, &path)
                } else {
                    io::export_png(&editor.sprite, editor.active_frame, &path)
                };
                match res {
                    Ok(()) => editor.msg = "exported".into(),
                    Err(err) => editor.msg = format!("export failed: {err}"),
                }
            } else {
                editor.msg = "usage: :export <file.png|file.ase>".into();
            }
        }
        "import" => {
            if let Some(p) = arg {
                match io::import_png(PathBuf::from(p).as_path()) {
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
        "open" | "load" => {
            if let Some(p) = arg {
                let path = PathBuf::from(p);
                let res = if is_ase(&path) {
                    ase::load(&path)
                } else if is_png(&path) {
                    io::import_png(&path)
                } else {
                    io::load_project(&path)
                };
                match res {
                    Ok(s) => {
                        editor.replace_sprite(s);
                        editor.msg = "loaded".into();
                    }
                    Err(err) => editor.msg = format!("load failed: {err}"),
                }
            } else {
                editor.msg = "usage: :open <file.vpx|file.ase|file.png>".into();
            }
        }
        "help" => editor.mode = Mode::Help,
        "" => {}
        _ => editor.msg = format!("unknown command: {name}"),
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

fn is_ase(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ase") || e.eq_ignore_ascii_case("aseprite"))
}

fn is_png(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
}
