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
                    editor.paint_brush_symmetric(x, y, editor.fg);
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
        "tile" => match arg {
            Some("on") => editor.set_tile_mode(Some(true)),
            Some("off") => editor.set_tile_mode(Some(false)),
            _ => editor.set_tile_mode(None),
        },
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
        "brush" => match arg.and_then(|v| v.parse::<u32>().ok()) {
            Some(n) => {
                editor.brush_size = n.clamp(1, 64);
                editor.msg = format!("brush: {} px", editor.brush_size);
            }
            None => editor.msg = "usage: :brush <1-64>".into(),
        },
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
        "filter" | "fx" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let name = it.next().unwrap_or("");
            let amount = it.next();
            match crate::editor::Filter::parse(name, amount) {
                Some(f) => editor.filter(f),
                None => {
                    editor.msg =
                        "usage: :filter invert|brightness|contrast|hue|saturation|outline|blur|sharpen [amount]"
                            .into()
                }
            }
        }
        "sel" => {
            let mut it = arg.unwrap_or("").split_whitespace();
            let op = it.next().unwrap_or("");
            let n: u32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(1);
            match op {
                "expand" | "grow" => editor.selection_expand(n),
                "contract" | "shrink" => editor.selection_contract(n),
                "border" | "ring" => editor.selection_border(n),
                "feather" | "soft" => editor.selection_feather(n),
                "invert" | "inv" => editor.selection_invert(),
                "none" | "off" => editor.selection_none(),
                "all" => editor.select_all(),
                _ => {
                    editor.msg =
                        "usage: :sel expand|contract|border|feather <n> | invert | none | all"
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
            let (x, y) = editor.cursor;
            editor.contour_at(x, y);
        }
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
            let cols: u32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let rows: u32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if dir.is_empty() {
                editor.msg = "usage: :slices <dir> [cols rows]".into();
            } else {
                let stem = editor
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "sprite".into());
                match io::export_slices(
                    &editor.sprite,
                    editor.active_frame,
                    std::path::Path::new(&dir),
                    &stem,
                    cols,
                    rows,
                ) {
                    Ok(paths) => editor.msg = format!("slices: {} files in {dir}", paths.len()),
                    Err(e) => editor.msg = format!("slices failed: {e}"),
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
                } else if io::is_png(&path) {
                    io::export_png_scaled(&editor.sprite, editor.active_frame, &path, scale)
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
        "" => {}
        _ => editor.msg = format!("unknown command: {name}"),
    }
}

/// Write the sprite to `path`, picking the format by extension (`.ase` →
/// Aseprite, else the `.vpx` JSON project). Returns whether it was written;
/// `:wq` only quits on success. Shared so `:w` and `:wq` cannot drift.
fn save_to(editor: &mut Editor, path: PathBuf) -> bool {
    let res = if io::is_ase(&path) {
        ase::save(&editor.sprite, &path)
    } else {
        io::save_project(&editor.sprite, &path)
    };
    match res {
        Ok(()) => {
            editor.note_recent(path.clone());
            editor.path = Some(path);
            editor.dirty = false;
            editor.msg = editor.saved_note();
            true
        }
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
