use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Vec2};

use crate::color::Rgba;
use crate::editor::{Editor, LayerProps, Mode, SelectionOp, Shape, Symmetry, Tool};
use crate::sprite::{BlendMode, ellipse_points, line_points, rect_points};

/// Onion-skin settings: how many frames each side, how strong, and the two
/// tints. Aseprite's default is one red behind and one green ahead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Onion {
    pub enabled: bool,
    pub count: u8,
    pub opacity: u8,
    pub prev: [u8; 3],
    pub next: [u8; 3],
}

impl Default for Onion {
    fn default() -> Self {
        Onion {
            enabled: true,
            count: 1,
            opacity: 110,
            prev: [255, 90, 90],
            next: [90, 210, 90],
        }
    }
}

/// The canvas view: zoom (screen px per sprite px) and pan (screen pos of the
/// sprite's top-left pixel).
pub struct View {
    pub zoom: u32,
    pub pan: Pos2,
    pub initialized: bool,
    pub show_grid: bool,
    /// The window's inner size as of the last frame, persisted on exit.
    pub window: (f32, f32),
    /// "dark" or "light"; applied to the whole context when it changes.
    pub theme: String,
    /// Mirrored from the editor each frame: 0 = no snapping.
    pub snap_grid: u32,
    pub onion: Onion,
    /// Frame index being dragged in the timeline.
    drag_frame: Option<usize>,
    /// Cached per-frame timeline thumbnails, rebuilt only when the sprite's
    /// revision changes.
    thumbs: Vec<Option<TextureHandle>>,
    thumbs_revision: u64,
    /// Hex text fields for fg/bg, and the colour they were last filled from,
    /// so typing is not overwritten by the next frame's echo.
    fg_hex: (String, Rgba),
    bg_hex: (String, Rgba),
    /// Index into `crate::editor::FILTERS`, and the amount its button applies.
    filter: usize,
    filter_amount: f32,
    /// Free-transform fields: degrees, uniform scale, degrees of skew.
    rot: f32,
    scale: f32,
    skew: f32,
    /// Opacity slider drag in progress: the layer and its properties at drag
    /// start, so the whole drag is one undo step.
    layer_prop_drag: Option<(usize, LayerProps)>,
}

impl View {
    pub fn new() -> Self {
        Self {
            zoom: 16,
            pan: Pos2::ZERO,
            initialized: false,
            show_grid: true,
            window: (1280.0, 800.0),
            theme: "dark".into(),
            snap_grid: 0,
            onion: Onion::default(),
            drag_frame: None,
            thumbs: Vec::new(),
            thumbs_revision: u64::MAX,
            fg_hex: (String::new(), Rgba(0, 0, 0, 0)),
            bg_hex: (String::new(), Rgba(0, 0, 0, 0)),
            filter: 0,
            filter_amount: 24.0,
            rot: 0.0,
            scale: 1.0,
            skew: 0.0,
            layer_prop_drag: None,
        }
    }

    pub fn zoom_in(&mut self) {
        self.set_zoom(self.zoom.saturating_add(1));
    }

    pub fn zoom_out(&mut self) {
        self.set_zoom(self.zoom.saturating_sub(1).max(1));
    }

    fn set_zoom(&mut self, z: u32) {
        self.zoom = z.clamp(1, 64);
    }

    /// Zoom by one step, anchoring the pixel currently under `pos`.
    pub fn zoom_at(&mut self, pos: Pos2, dir: i32) {
        let new_zoom = if dir >= 0 {
            self.zoom.saturating_add(1)
        } else {
            self.zoom.saturating_sub(1).max(1)
        }
        .clamp(1, 64);
        if new_zoom == self.zoom {
            return;
        }
        let f = (pos - self.pan) / self.zoom as f32;
        self.pan = pos - f * new_zoom as f32;
        self.zoom = new_zoom;
    }

    fn fit(&mut self, panel: Rect, w: u32, h: u32) {
        let z = ((panel.width() / w as f32).min(panel.height() / h as f32) * 0.85)
            .floor()
            .max(1.0) as u32;
        self.zoom = z.clamp(1, 64);
        let size = Vec2::new((w * self.zoom) as f32, (h * self.zoom) as f32);
        self.pan = panel.center() - size * 0.5;
    }

    fn screen_to_pixel(&self, pos: Pos2, w: u32, h: u32) -> Option<(u32, u32)> {
        let f = (pos - self.pan) / self.zoom as f32;
        let (x, y) = (f.x.floor(), f.y.floor());
        if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
            None
        } else {
            let (mut x, mut y) = (x as u32, y as u32);
            if self.snap_grid > 1 {
                x -= x % self.snap_grid;
                y -= y % self.snap_grid;
            }
            Some((x, y))
        }
    }

    /// Like `screen_to_pixel` but accepts the 3x3 tile preview around the
    /// canvas and folds those clicks back into the sprite.
    fn screen_to_pixel_wrapped(&self, pos: Pos2, w: u32, h: u32) -> Option<(u32, u32)> {
        let f = (pos - self.pan) / self.zoom as f32;
        let (x, y) = (f.x.floor(), f.y.floor());
        let (wf, hf) = (w.max(1) as f32, h.max(1) as f32);
        if x < -wf || y < -hf || x >= 2.0 * wf || y >= 2.0 * hf {
            None
        } else {
            Some((x.rem_euclid(wf) as u32, y.rem_euclid(hf) as u32))
        }
    }
}

pub fn draw(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    view: &mut View,
    texture: &mut Option<TextureHandle>,
    onion_prev: &mut Option<TextureHandle>,
    onion_next: &mut Option<TextureHandle>,
    playing: &mut bool,
) {
    // Remember the window size for the config file.
    let screen = ui.ctx().content_rect().size();
    view.window = (screen.x, screen.y);
    egui::Panel::top("toolbar").show(ui, |ui| toolbar(ui, editor, view));
    egui::Panel::bottom("timeline").show(ui, |ui| timeline(ui, editor, playing, view));
    egui::Panel::right("side").show(ui, |ui| side(ui, editor, view));
    egui::CentralPanel::default().show(ui, |ui| {
        canvas(ui, editor, view, texture, onion_prev, onion_next)
    });

    if editor.mode == Mode::Help {
        help(ui.ctx());
    }
}

// ---- top toolbar ----

fn toolbar(ui: &mut egui::Ui, editor: &mut Editor, view: &mut View) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("vixel").strong());
        ui.separator();
        ui.label(format!("[{}]", mode_name(editor.mode)));
        ui.separator();

        for (tool, name) in [
            (Tool::Pencil, "P"),
            (Tool::Eraser, "E"),
            (Tool::Bucket, "B"),
            (Tool::Picker, "Pick"),
            (Tool::Line, "Line"),
            (Tool::Rect, "Rect"),
            (Tool::Ellipse, "Ellip"),
            (Tool::Gradient, "Grad"),
            (Tool::Shade, "Shade"),
            (Tool::Spray, "Spray"),
            (Tool::Blur, "Blur"),
            (Tool::Lasso, "Lasso"),
            (Tool::Polygon, "Poly"),
        ] {
            if ui.selectable_label(editor.tool == tool, name).clicked() {
                editor.set_tool(tool);
            }
        }

        ui.separator();
        if ui
            .selectable_label(
                editor.symmetry != Symmetry::None,
                format!("sym:{}", editor.symmetry.name()),
            )
            .clicked()
        {
            editor.cycle_symmetry();
        }
        if ui.selectable_label(editor.filled_shapes, "fill").clicked() {
            editor.toggle_filled_shapes();
        }
        let gmode = if editor.gradient_radial {
            "grad:rad"
        } else {
            "grad:lin"
        };
        if ui.selectable_label(editor.gradient_radial, gmode).clicked() {
            editor.gradient_radial = !editor.gradient_radial;
        }
        if ui
            .selectable_label(
                editor.selection_op != SelectionOp::Replace,
                format!("sel:{}", editor.selection_op.name()),
            )
            .clicked()
        {
            editor.cycle_selection_op();
        }

        ui.horizontal(|ui| {
            if ui.small_button("sel+").clicked() {
                editor.selection_expand(1);
            }
            if ui.small_button("sel-").clicked() {
                editor.selection_contract(1);
            }
            if ui.small_button("ring").clicked() {
                editor.selection_border(1);
            }
            if ui.small_button("soft").clicked() {
                editor.selection_feather(2);
            }
            if ui.small_button("inv").clicked() {
                editor.selection_invert();
            }
            if ui.small_button("none").clicked() {
                editor.selection_none();
            }
        });

        ui.separator();
        swatch(ui, editor.fg, true);
        swatch(ui, editor.bg, false);
        if ui.button("swap").clicked() {
            editor.swap_colors();
        }

        ui.separator();
        if ui.button("-").clicked() {
            view.zoom_out();
        }
        ui.label(format!("{}x", view.zoom));
        if ui.button("+").clicked() {
            view.zoom_in();
        }

        ui.separator();
        ui.label(format!(
            "{}x{} ({},{})",
            editor.sprite.width, editor.sprite.height, editor.cursor.0, editor.cursor.1
        ));

        ui.separator();
        if editor.dirty {
            ui.colored_label(Color32::YELLOW, "●");
        }
    });

    // The status line gets its own row: appended to the tool row it was pushed
    // off the right edge at any normal window width, so every message (palette
    // lock refusals included) was invisible.
    ui.horizontal(|ui| {
        if editor.mode == Mode::Command {
            ui.label(egui::RichText::new(format!(":{}", editor.cmdline)).strong());
            ui.label(egui::RichText::new("▏").weak());
        } else {
            ui.label(&editor.msg);
        }
    });
}

// ---- bottom timeline ----

fn timeline(ui: &mut egui::Ui, editor: &mut Editor, playing: &mut bool, view: &mut View) {
    ui.horizontal(|ui| {
        let play_label = if *playing { "pause" } else { "play" };
        if ui.button(play_label).clicked() {
            *playing = !*playing;
            if *playing {
                editor.start_playback();
            }
        }
        if ui.button("+frame").clicked() {
            editor.add_frame();
        }
        if ui.button("dup").clicked() {
            editor.duplicate_frame();
        }
        if ui.button("-frame").clicked() {
            editor.del_frame();
        }
        ui.separator();

        let tags: Vec<(String, usize, bool, [u8; 3])> = editor
            .sprite
            .tags
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    format!("{} {}-{}", t.name, t.from, t.to),
                    t.from,
                    editor.active_tag == Some(i),
                    [t.color.0, t.color.1, t.color.2],
                )
            })
            .collect();
        for (i, (label, from, active, rgb)) in tags.into_iter().enumerate() {
            let text = egui::RichText::new(label).color(Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
            if ui
                .selectable_label(active, text)
                .on_hover_text("play this tag (click again to clear)")
                .clicked()
            {
                editor.active_tag = if active { None } else { Some(i) };
                if !active {
                    editor.active_frame = from.min(editor.sprite.frames.len() - 1);
                    editor.start_playback();
                }
            }
        }
        ui.separator();

        let count = editor.sprite.frames.len();
        if view.thumbs.len() < count {
            view.thumbs.resize_with(count, || None);
        } else if view.thumbs.len() > count {
            view.thumbs.truncate(count);
        }
        if view.thumbs_revision != editor.revision {
            view.thumbs_revision = editor.revision;
            view.thumbs.iter_mut().for_each(|t| *t = None);
        }

        let mut selected = editor.active_frame;
        let th = 40.0;
        let aspect = editor.sprite.width as f32 / editor.sprite.height.max(1) as f32;
        let tw = (th * aspect).clamp(10.0, 120.0);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

        for i in 0..count {
            if view.thumbs[i].is_none() {
                let comp = frame_composite(editor, i);
                upload(ui, &mut view.thumbs[i], &format!("thumb{i}"), comp);
            }
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(tw, th), Sense::click_and_drag());
            draw_checker(ui.painter(), rect, 4.0);
            if let Some(t) = &view.thumbs[i] {
                ui.painter().image(t.id(), rect, uv, Color32::WHITE);
            }
            if view.drag_frame.is_some() && resp.contains_pointer() {
                ui.painter().rect_stroke(
                    rect,
                    1.0,
                    Stroke::new(2.0, Color32::from_rgb(120, 200, 255)),
                    StrokeKind::Inside,
                );
            }
            let stroke = if i == selected {
                Stroke::new(2.0, Color32::WHITE)
            } else {
                Stroke::new(1.0, Color32::from_gray(80))
            };
            ui.painter()
                .rect_stroke(rect, 1.0, stroke, StrokeKind::Inside);
            if resp.clicked() {
                selected = i;
            }
            if resp.drag_started() {
                view.drag_frame = Some(i);
            }
            if let Some(from) = view.drag_frame
                && from != i
                && resp.contains_pointer()
                && ui.input(|inp| inp.pointer.any_released())
            {
                editor.move_frame(from, i);
                view.drag_frame = None;
                selected = i;
            }
            if resp.drag_stopped() && view.drag_frame.is_some() {
                view.drag_frame = None;
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
        editor.active_frame = selected;

        ui.separator();
        ui.label("ms:");
        let d = &mut editor.sprite.frames[editor.active_frame].duration_ms;
        ui.add(egui::DragValue::new(d).range(1..=10_000));
        ui.separator();
        ui.label("cel op:");
        let cel = &mut editor.sprite.cels[editor.active_layer][editor.active_frame];
        let mut opacity = cel.opacity;
        if ui.add(egui::Slider::new(&mut opacity, 0..=255)).changed() {
            cel.opacity = opacity;
            editor.touch();
        }
        ui.separator();
        ui.label("z:");
        let mut z = editor.sprite.cels[editor.active_layer][editor.active_frame].z_index;
        let resp = ui.add(egui::DragValue::new(&mut z).range(i16::MIN..=i16::MAX));
        if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
            editor.set_cel_z(z);
        }
    });
}

// ---- right side panel ----

/// The side panel scrolls: the settings, the palette, the layers and the sprite
/// tools together are taller than any window, and a layer list below the fold
/// is a layer you cannot click.
fn side(ui: &mut egui::Ui, editor: &mut Editor, view: &mut View) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| side_body(ui, editor, view));
}

fn side_body(ui: &mut egui::Ui, editor: &mut Editor, view: &mut View) {
    ui.horizontal(|ui| {
        let light = view.theme == "light";
        if ui
            .selectable_label(light, "light")
            .on_hover_text("theme (saved in the config)")
            .clicked()
        {
            view.theme = if light { "dark".into() } else { "light".into() };
            ui.ctx().set_visuals(if light {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label("brush");
        ui.add(egui::Slider::new(&mut editor.brush_size, 1..=16));
        ui.checkbox(&mut editor.pixel_perfect, "perfect")
            .on_hover_text("drop L-shaped corner pixels while drawing");
    });
    ui.checkbox(&mut view.show_grid, "pixel grid");
    ui.checkbox(&mut view.onion.enabled, "onion skin");
    if view.onion.enabled {
        ui.horizontal(|ui| {
            ui.label("count");
            ui.add(egui::Slider::new(&mut view.onion.count, 1..=3));
            ui.label("opacity");
            ui.add(egui::Slider::new(&mut view.onion.opacity, 0..=255));
        });
        ui.horizontal(|ui| {
            ui.label("before");
            let _ = ui.color_edit_button_srgb(&mut view.onion.prev);
            ui.label("after");
            let _ = ui.color_edit_button_srgb(&mut view.onion.next);
        });
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut editor.play_loop, "loop playback");
        ui.checkbox(&mut editor.play_pingpong, "ping-pong");
    });
    ui.checkbox(&mut editor.tile_mode, "tile mode (3x3, wraps)");
    ui.horizontal(|ui| {
        ui.label("text");
        ui.add(egui::TextEdit::singleline(&mut editor.text).desired_width(120.0));
        ui.add(
            egui::DragValue::new(&mut editor.text_scale)
                .range(1..=8)
                .prefix("x"),
        );
        if ui
            .button("stamp")
            .on_hover_text("stamp at the cursor (:text)")
            .clicked()
        {
            let (x, y) = editor.cursor;
            let text = editor.text.clone();
            editor.stamp_text(&text, x, y);
        }
    });
    ui.horizontal(|ui| {
        let mut profile = editor.sprite.color_profile;
        egui::ComboBox::from_id_salt("profile")
            .selected_text(profile.name())
            .show_ui(ui, |ui| {
                for p in [
                    crate::sprite::ColorProfile::Srgb,
                    crate::sprite::ColorProfile::Linear,
                ] {
                    ui.selectable_value(&mut profile, p, p.name());
                }
            });
        if profile != editor.sprite.color_profile {
            editor.set_color_profile(profile);
        }
        ui.label("grid");
        ui.add(egui::Slider::new(&mut editor.grid, 1..=64));
        ui.checkbox(&mut editor.snap, "snap");
    });
    ui.horizontal(|ui| {
        ui.label("symmetry");
        for (label, mode) in [
            ("off", Symmetry::None),
            ("h", Symmetry::Horizontal),
            ("v", Symmetry::Vertical),
            ("both", Symmetry::Both),
            ("radial", Symmetry::Radial),
        ] {
            if ui
                .selectable_label(editor.symmetry == mode, label)
                .clicked()
            {
                editor.symmetry = mode;
            }
        }
    });
    if editor.symmetry == Symmetry::Radial {
        ui.add(egui::Slider::new(&mut editor.sym_axes, 2..=16).text("axes"));
    }
    ui.horizontal(|ui| {
        let (cx, cy) = editor.symmetry_center();
        ui.label(format!("centre {cx:.0},{cy:.0}"));
        if ui
            .small_button("cursor")
            .on_hover_text("mirror about the cursor")
            .clicked()
        {
            editor.sym_center = Some(editor.cursor);
        }
        if ui.small_button("reset").clicked() {
            editor.sym_center = None;
        }
    });
    ui.horizontal(|ui| {
        ui.label("tol (wand/fill)");
        ui.add(egui::Slider::new(&mut editor.wand_tolerance, 0..=255));
    });
    ui.checkbox(&mut editor.contiguous, "contiguous (wand/fill)");
    ui.checkbox(&mut editor.wand_all_layers, "wand: all layers");
    ui.separator();

    let mut fg = editor.fg;
    color_editor(ui, "fg", &mut fg, &mut view.fg_hex);
    if fg != editor.fg {
        editor.set_fg(fg);
    }
    let mut bg = editor.bg;
    color_editor(ui, "bg", &mut bg, &mut view.bg_hex);
    if bg != editor.bg {
        editor.set_bg(bg);
    }
    if !editor.recent_colors.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label("recent");
            for c in editor.recent_colors.clone() {
                let col = Color32::from_rgb(c.0, c.1, c.2);
                if ui
                    .add(egui::Button::new("").fill(col).min_size(Vec2::splat(16.0)))
                    .on_hover_text(format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2))
                    .clicked()
                {
                    editor.set_fg(c);
                }
            }
        });
    }
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Palette").strong());
        if ui.button("+fg").clicked() {
            editor.add_palette_color();
        }
        if ui.button("-").clicked() {
            editor.remove_palette_color();
        }
        egui::ComboBox::from_id_salt("palette_preset")
            .selected_text("preset")
            .show_ui(ui, |ui| {
                for p in crate::palette::PRESETS {
                    if ui.selectable_label(false, *p).clicked() {
                        editor.load_palette_preset(p);
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("name");
        let mut name = editor.palette_name(editor.fg_idx).to_owned();
        if ui
            .add(egui::TextEdit::singleline(&mut name).desired_width(100.0))
            .changed()
        {
            editor.set_palette_name(editor.fg_idx, &name);
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("palette")
        .max_height(180.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut drop: Option<(usize, usize)> = None;
                let names = editor.palette_names.clone();
                for (i, c) in editor.palette.clone().into_iter().enumerate() {
                    let col = Color32::from_rgba_unmultiplied(c.0, c.1, c.2, 255);
                    let mut btn = egui::Button::new("").fill(col).min_size(Vec2::splat(20.0));
                    if i == editor.fg_idx {
                        btn = btn.stroke(Stroke::new(2.0, Color32::WHITE));
                    }
                    let name = names.get(i).map(String::as_str).unwrap_or("");
                    let hint = if name.is_empty() {
                        format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
                    } else {
                        format!("{name}\n#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
                    };
                    // A swatch drags onto another to reorder; plain clicks still
                    // pick fg (left) and bg (right).
                    let src = ui.dnd_drag_source(egui::Id::unique(("swatch", i)), i, |ui| {
                        ui.add(btn).on_hover_text(hint)
                    });
                    let resp = src.inner;
                    if resp.clicked() {
                        editor.set_fg(c);
                        editor.fg_idx = i;
                    }
                    if resp.secondary_clicked() {
                        editor.bg = c;
                        editor.bg_idx = i;
                    }
                    if let Some(payload) = src.response.dnd_hover_payload::<usize>()
                        && src.response.dnd_release_payload::<usize>().is_some()
                    {
                        drop = Some((*payload, i));
                    }
                }
                if let Some((from, to)) = drop {
                    editor.move_palette(from, to);
                }
            });
        });
    ui.horizontal(|ui| {
        if ui
            .small_button("from sprite")
            .on_hover_text("palette = the frame's own colours (:pal from-sprite)")
            .clicked()
        {
            editor.palette_from_sprite();
        }
        if ui
            .selectable_label(editor.palette_lock, "lock")
            .on_hover_text("refuse colours that are not in the palette (:pal lock)")
            .clicked()
        {
            editor.set_palette_lock(!editor.palette_lock);
        }
    });
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Layers").strong());
        if ui.button("^").clicked() {
            editor.move_layer_active(-1);
        }
        if ui.button("v").clicked() {
            editor.move_layer_active(1);
        }
        if ui.button("+").clicked() {
            editor.add_layer();
        }
        if ui.button("-").clicked() {
            editor.del_layer();
        }
        if ui
            .button("dup")
            .on_hover_text("duplicate the active layer")
            .clicked()
        {
            editor.duplicate_layer();
        }
        if ui
            .button("merge")
            .on_hover_text("merge into the layer below")
            .clicked()
        {
            editor.merge_down();
        }
        if ui
            .button("flat")
            .on_hover_text("flatten every visible layer")
            .clicked()
        {
            editor.flatten();
        }
        if ui.button("grp").on_hover_text("new group").clicked() {
            editor.add_group();
        }
        if ui
            .button(">")
            .on_hover_text("indent into group above")
            .clicked()
        {
            editor.indent_layer(1);
        }
        if ui.button("<").on_hover_text("outdent").clicked() {
            editor.indent_layer(-1);
        }
    });
    let count = editor.sprite.layers.len();
    let mut selected = editor.active_layer;
    let mut toggle_mask = None;
    let mut toggle_ref = None;
    let mut toggle_bg = None;
    let mut drop_to: Option<(usize, usize)> = None;
    let mut prop_edits: Vec<(usize, LayerProps)> = Vec::new();
    let mut prop_drag = view.layer_prop_drag;
    egui::ScrollArea::vertical()
        .id_salt("layers")
        .show(ui, |ui| {
            for i in 0..count {
                let before = editor.layer_props(i);
                // Each row is a drag source carrying its own index, and every
                // row is a drop target: drag a layer onto another to move it
                // (with its children) in front of that one.
                let source = ui.dnd_drag_source(egui::Id::unique(("layer", i)), i, |ui| {
                    let layer = &mut editor.sprite.layers[i];
                    let indent = layer.child_level as f32 * 14.0;
                    ui.horizontal(|ui| {
                        ui.add_space(indent);
                        let label = if layer.group {
                            format!("▾ {}", layer.name)
                        } else {
                            layer.name.clone()
                        };
                        if ui.selectable_label(i == selected, label).clicked() {
                            selected = i;
                        }
                        if ui
                            .checkbox(&mut layer.visible, "")
                            .on_hover_text("visible")
                            .changed()
                        {
                            prop_edits.push((i, before));
                        }
                        if ui
                            .checkbox(&mut layer.editable, "edit")
                            .on_hover_text("editable (lock)")
                            .changed()
                        {
                            prop_edits.push((i, before));
                        }
                        if ui
                            .selectable_label(layer.reference, "ref")
                            .on_hover_text("reference layer: traced, not painted or exported")
                            .clicked()
                        {
                            toggle_ref = Some(i);
                        }
                        if ui
                            .selectable_label(layer.background, "bg")
                            .on_hover_text("background layer (opaque)")
                            .clicked()
                        {
                            toggle_bg = Some(i);
                        }
                        if !layer.group
                            && ui
                                .selectable_label(layer.mask.is_some(), "M")
                                .on_hover_text(
                                    "alpha mask: from selection, or full; click to add/remove",
                                )
                                .clicked()
                        {
                            toggle_mask = Some(i);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(indent);
                        if blend_combo(ui, i, &mut layer.blend) {
                            prop_edits.push((i, before));
                        }
                        let opacity =
                            ui.add(egui::Slider::new(&mut layer.opacity, 0..=255).text("op"));
                        if opacity.drag_started() {
                            prop_drag = Some((i, before));
                        }
                        if opacity.drag_stopped() {
                            if let Some((l, values)) = prop_drag.take() {
                                prop_edits.push((l, values));
                            }
                        } else if opacity.changed() && !opacity.dragged() {
                            prop_edits.push((i, before));
                        }
                    });
                });
                let resp = source.response;
                if let Some(payload) = resp.dnd_hover_payload::<usize>() {
                    ui.painter().rect_stroke(
                        resp.rect,
                        2.0,
                        Stroke::new(1.0, Color32::from_rgb(120, 190, 255)),
                        StrokeKind::Inside,
                    );
                    if resp.dnd_release_payload::<usize>().is_some() {
                        drop_to = Some((*payload, i));
                    }
                }
            }
        });
    view.layer_prop_drag = prop_drag;
    for (layer, before) in prop_edits {
        editor.record_layer_props(layer, before);
    }
    if let Some((from, to)) = drop_to {
        editor.move_layer_to(from, to);
    }
    if let Some(i) = toggle_ref {
        editor.active_layer = i;
        let on = !editor.sprite.layers[i].reference;
        editor.set_layer_reference(on);
    }
    if let Some(i) = toggle_bg {
        editor.active_layer = i;
        let on = !editor.sprite.layers[i].background;
        editor.set_layer_background(on);
    }
    if let Some(i) = toggle_mask {
        editor.active_layer = i;
        if editor.sprite.layers[i].mask.is_some() {
            editor.mask_del();
        } else {
            editor.mask_add();
        }
    }
    editor.active_layer = selected;

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Sprite").strong());
        let mut mode = editor.sprite.color_mode;
        egui::ComboBox::from_id_salt("colormode")
            .selected_text(mode.name())
            .show_ui(ui, |ui| {
                for m in [
                    crate::sprite::ColorMode::Auto,
                    crate::sprite::ColorMode::Rgba,
                    crate::sprite::ColorMode::Grayscale,
                    crate::sprite::ColorMode::Indexed,
                ] {
                    ui.selectable_value(&mut mode, m, m.name());
                }
            });
        if mode != editor.sprite.color_mode {
            editor.set_color_mode(mode);
        }
    });
    ui.horizontal(|ui| {
        ui.label("px ratio");
        let (mut ax, mut ay) = editor.sprite.pixel_aspect;
        let cx = ui.add(egui::DragValue::new(&mut ax).range(1..=255));
        let cy = ui.add(egui::DragValue::new(&mut ay).range(1..=255));
        if (cx.changed() || cy.changed()) && (ax, ay) != editor.sprite.pixel_aspect {
            editor.set_pixel_aspect(ax, ay);
        }
        ui.label("grid");
        let (mut gx, mut gy) = editor.sprite.grid;
        let g1 = ui.add(egui::DragValue::new(&mut gx).range(0..=1024));
        let g2 = ui.add(egui::DragValue::new(&mut gy).range(0..=1024));
        if (g1.changed() || g2.changed()) && (gx, gy) != editor.sprite.grid {
            editor.set_sprite_grid(gx, gy);
        }
        ui.label("origin");
        let (mut ox, mut oy) = editor.sprite.grid_origin;
        let o1 = ui.add(egui::DragValue::new(&mut ox).range(-32768..=32767));
        let o2 = ui.add(egui::DragValue::new(&mut oy).range(-32768..=32767));
        if (o1.changed() || o2.changed()) && (ox, oy) != editor.sprite.grid_origin {
            editor.set_sprite_grid_origin(ox, oy);
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Transform").strong());
    });
    ui.horizontal(|ui| {
        ui.label("rot");
        ui.add(egui::DragValue::new(&mut view.rot).speed(1.0).suffix("°"));
        ui.label("scale");
        ui.add(
            egui::DragValue::new(&mut view.scale)
                .speed(0.05)
                .range(0.05..=8.0),
        );
        ui.label("skew");
        ui.add(egui::DragValue::new(&mut view.skew).speed(1.0).suffix("°"));
    });
    if ui
        .button("apply transform")
        .on_hover_text("rotate / scale / skew the selection (or the layer) about its centre")
        .clicked()
    {
        editor.free_transform(view.rot, view.scale, view.scale, view.skew);
    }

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter").strong());
        egui::ComboBox::from_id_salt("filter")
            .selected_text(crate::editor::FILTERS[view.filter])
            .show_ui(ui, |ui| {
                for (i, name) in crate::editor::FILTERS.iter().enumerate() {
                    ui.selectable_value(&mut view.filter, i, *name);
                }
            });
        ui.add(egui::DragValue::new(&mut view.filter_amount).speed(1.0));
        if ui
            .button("apply")
            .on_hover_text("apply to the selection, or the whole layer (:filter)")
            .clicked()
        {
            let name = crate::editor::FILTERS[view.filter];
            let amount = format!("{:.2}", view.filter_amount);
            if let Some(f) = crate::editor::Filter::parse(name, Some(&amount)) {
                editor.filter(f);
            }
        }
    });
}

/// A colour row: the swatch button, a hex field, and HSV drag values. The hex
/// buffer only refills when the colour changed elsewhere, so a half-typed value
/// survives the frame.
fn color_editor(ui: &mut egui::Ui, label: &str, color: &mut Rgba, hex: &mut (String, Rgba)) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut srgb = [color.0, color.1, color.2];
        if ui.color_edit_button_srgb(&mut srgb).changed() {
            *color = Rgba(srgb[0], srgb[1], srgb[2], 255);
        }
        if hex.1 != *color {
            hex.0 = format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2);
            hex.1 = *color;
        }
        if ui
            .add(egui::TextEdit::singleline(&mut hex.0).desired_width(70.0))
            .changed()
            && let Some(c) = crate::color::parse_hex(&hex.0)
        {
            *color = c;
            hex.1 = c;
        }
    });
    let (h, s, v) = crate::color::to_hsv(*color);
    let (mut nh, mut ns, mut nv) = (h, s * 100.0, v * 100.0);
    ui.horizontal(|ui| {
        ui.label("hsv");
        let a = ui.add(egui::DragValue::new(&mut nh).range(0.0..=360.0).speed(1.0));
        let b = ui.add(egui::DragValue::new(&mut ns).range(0.0..=100.0).speed(1.0));
        let c = ui.add(egui::DragValue::new(&mut nv).range(0.0..=100.0).speed(1.0));
        if a.changed() || b.changed() || c.changed() {
            *color = crate::color::from_hsv(nh, ns / 100.0, nv / 100.0);
        }
    });

    let (resp, painter) = ui.allocate_painter(Vec2::splat(88.0), Sense::click_and_drag());
    let center = resp.rect.center();
    let outer = resp.rect.width().min(resp.rect.height()) * 0.46;
    let inner = outer * 0.70;
    for i in 0..72 {
        let a0 = (i as f32 * 5.0).to_radians();
        let a1 = ((i + 1) as f32 * 5.0).to_radians();
        let hue = crate::color::from_hsv(i as f32 * 5.0, 1.0, 1.0);
        let color = Color32::from_rgb(hue.0, hue.1, hue.2);
        let points = vec![
            center + Vec2::new(a0.cos(), a0.sin()) * inner,
            center + Vec2::new(a0.cos(), a0.sin()) * outer,
            center + Vec2::new(a1.cos(), a1.sin()) * outer,
            center + Vec2::new(a1.cos(), a1.sin()) * inner,
        ];
        painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    }
    if (resp.clicked() || resp.dragged())
        && let Some(pos) = resp.interact_pointer_pos()
        && (pos - center).length() >= inner * 0.9
    {
        let (_, s, v) = crate::color::to_hsv(*color);
        *color = crate::color::from_hsv(wheel_hue(center, pos), s, v);
    }
    let (h, _, _) = crate::color::to_hsv(*color);
    let marker =
        center + Vec2::new(h.to_radians().cos(), h.to_radians().sin()) * ((inner + outer) / 2.0);
    painter.circle_filled(marker, 3.0, Color32::WHITE);
    painter.circle_stroke(marker, 3.0, Stroke::new(1.0, Color32::BLACK));
}

/// Angle of `pos` around `center`, 0° at +x and 90° at +y (screen down).
fn wheel_hue(center: Pos2, pos: Pos2) -> f32 {
    let d = pos - center;
    d.y.atan2(d.x).to_degrees().rem_euclid(360.0)
}

/// Blend-mode dropdown; returns whether the mode changed.
fn blend_combo(ui: &mut egui::Ui, i: usize, blend: &mut BlendMode) -> bool {
    let was = *blend;
    egui::ComboBox::from_id_salt(("blend", i))
        .selected_text(blend_name(*blend))
        .show_ui(ui, |ui| {
            for b in ALL_BLENDS {
                ui.selectable_value(blend, b, blend_name(b));
            }
        });
    *blend != was
}

fn blend_name(b: BlendMode) -> &'static str {
    match b {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color dodge",
        BlendMode::ColorBurn => "color burn",
        BlendMode::HardLight => "hard light",
        BlendMode::SoftLight => "soft light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
        BlendMode::Add => "add",
        BlendMode::Subtract => "subtract",
        BlendMode::Divide => "divide",
    }
}

const ALL_BLENDS: [BlendMode; 19] = [
    BlendMode::Normal,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::HardLight,
    BlendMode::SoftLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
    BlendMode::Add,
    BlendMode::Subtract,
    BlendMode::Divide,
];

// ---- central canvas ----

fn canvas(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    view: &mut View,
    texture: &mut Option<TextureHandle>,
    onion_prev: &mut Option<TextureHandle>,
    onion_next: &mut Option<TextureHandle>,
) {
    let (w, h) = (editor.sprite.width, editor.sprite.height);
    let panel = ui.max_rect();

    upload(
        ui,
        texture,
        "canvas",
        frame_composite(editor, editor.active_frame),
    );

    if !view.initialized {
        view.fit(panel, w, h);
        view.initialized = true;
    }
    reveal_cursor(editor, view, panel);

    let tex = texture.as_ref().unwrap();
    let size = Vec2::new((w * view.zoom) as f32, (h * view.zoom) as f32);
    let rect = Rect::from_min_size(view.pan, size);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

    ui.painter().rect_filled(panel, 0.0, Color32::from_gray(18));
    draw_checker(ui.painter(), rect, view.zoom as f32);
    if editor.tile_mode {
        // Seamless preview: the sprite repeated 3x3, neighbours dimmed.
        let full = Rect::from_min_size(view.pan - size, size * 3.0);
        draw_checker(ui.painter(), full, view.zoom as f32);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let r = rect.translate(Vec2::new(dx as f32 * size.x, dy as f32 * size.y));
                ui.painter()
                    .image(tex.id(), r, uv, Color32::from_white_alpha(120));
            }
        }
    }

    let n = editor.sprite.frames.len();
    let onion = view.onion;
    if onion.enabled && n > 1 {
        // Each step back/forward is dimmer than the one before it, so three
        // frames of history read as history and not as a crowd.
        for k in 1..=(onion.count as usize).min(n - 1) {
            let alpha = (onion.opacity as u32 / k as u32) as u8;
            let [r, g, b] = onion.prev;
            let frame = (editor.active_frame + n - k) % n;
            upload(ui, onion_prev, "onion_prev", frame_composite(editor, frame));
            ui.painter().image(
                onion_prev.as_ref().unwrap().id(),
                rect,
                uv,
                Color32::from_rgba_unmultiplied(r, g, b, alpha),
            );
            let [r, g, b] = onion.next;
            let frame = (editor.active_frame + k) % n;
            upload(ui, onion_next, "onion_next", frame_composite(editor, frame));
            ui.painter().image(
                onion_next.as_ref().unwrap().id(),
                rect,
                uv,
                Color32::from_rgba_unmultiplied(r, g, b, alpha),
            );
        }
    }

    ui.painter().image(tex.id(), rect, uv, Color32::WHITE);
    if editor.tile_mode {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            0.0,
            Stroke::new(1.0, Color32::from_gray(190)),
            StrokeKind::Outside,
        );
    }

    view.snap_grid = if editor.snap { editor.grid } else { 0 };
    if view.show_grid && view.zoom >= 6 {
        draw_grid(ui.painter(), rect, view.zoom as f32, w, h);
    }
    if editor.grid > 1 && view.zoom * editor.grid >= 4 {
        draw_design_grid(ui.painter(), rect, view.zoom as f32, w, h, editor.grid);
    }
    draw_pending(ui.painter(), editor, view);
    draw_selection(ui.painter(), editor, view);
    draw_floating_paste(ui.painter(), editor, view);
    draw_lasso(ui.painter(), editor, view);
    draw_symmetry(ui.painter(), editor, view, rect);
    draw_cursor(ui.painter(), editor, view);

    let hit = if editor.tile_mode {
        rect.expand2(size)
    } else {
        rect
    };
    let resp = ui.interact(
        hit.intersect(panel),
        ui.scope_id().with("canvas"),
        Sense::click_and_drag(),
    );
    canvas_input(ui, editor, view, resp, w, h);
}

fn upload(ui: &egui::Ui, slot: &mut Option<TextureHandle>, name: &str, img: egui::ColorImage) {
    match slot {
        Some(t) => t.set(img, egui::TextureOptions::NEAREST),
        None => {
            *slot = Some(
                ui.ctx()
                    .load_texture(name, img, egui::TextureOptions::NEAREST),
            );
        }
    }
}

fn frame_composite(editor: &Editor, frame: usize) -> egui::ColorImage {
    let comp = editor.sprite.composite(frame);
    let (w, h) = (editor.sprite.width as usize, editor.sprite.height as usize);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for c in &comp {
        rgba.extend_from_slice(&[c.0, c.1, c.2, c.3]);
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

fn draw_checker(painter: &egui::Painter, rect: Rect, zoom: f32) {
    let tile = zoom.max(4.0);
    let c1 = Color32::from_gray(56);
    let c2 = Color32::from_gray(44);
    let mut y = rect.min.y;
    let mut row = 0i64;
    while y < rect.max.y {
        let mut x = rect.min.x;
        let mut col = 0i64;
        while x < rect.max.x {
            let color = if (row + col) % 2 == 0 { c1 } else { c2 };
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(x, y),
                    Pos2::new((x + tile).min(rect.max.x), (y + tile).min(rect.max.y)),
                ),
                0.0,
                color,
            );
            x += tile;
            col += 1;
        }
        y += tile;
        row += 1;
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, zoom: f32, w: u32, h: u32) {
    let stroke = Stroke::new(1.0, Color32::from_white_alpha(40));
    for x in 0..=w {
        let sx = rect.min.x + x as f32 * zoom;
        painter.line_segment(
            [Pos2::new(sx, rect.min.y), Pos2::new(sx, rect.max.y)],
            stroke,
        );
    }
    for y in 0..=h {
        let sy = rect.min.y + y as f32 * zoom;
        painter.line_segment(
            [Pos2::new(rect.min.x, sy), Pos2::new(rect.max.x, sy)],
            stroke,
        );
    }
}

/// The design grid: every `step` sprite pixels, brighter than the pixel grid.
fn draw_design_grid(painter: &egui::Painter, rect: Rect, zoom: f32, w: u32, h: u32, step: u32) {
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 190, 255, 70));
    let mut x = 0;
    while x <= w {
        let sx = rect.min.x + x as f32 * zoom;
        painter.line_segment(
            [Pos2::new(sx, rect.min.y), Pos2::new(sx, rect.max.y)],
            stroke,
        );
        x += step;
    }
    let mut y = 0;
    while y <= h {
        let sy = rect.min.y + y as f32 * zoom;
        painter.line_segment(
            [Pos2::new(rect.min.x, sy), Pos2::new(rect.max.x, sy)],
            stroke,
        );
        y += step;
    }
}

fn draw_pending(painter: &egui::Painter, editor: &Editor, view: &View) {
    if let Some(p) = editor.pending {
        let pts = match p.shape {
            Shape::Line => line_points(
                p.start.0 as i32,
                p.start.1 as i32,
                editor.cursor.0 as i32,
                editor.cursor.1 as i32,
            ),
            Shape::Rect => rect_points(
                p.start.0 as i32,
                p.start.1 as i32,
                editor.cursor.0 as i32,
                editor.cursor.1 as i32,
            ),
            Shape::Ellipse => ellipse_points(
                p.start.0 as i32,
                p.start.1 as i32,
                editor.cursor.0 as i32,
                editor.cursor.1 as i32,
                false,
            ),
            Shape::Gradient => line_points(
                p.start.0 as i32,
                p.start.1 as i32,
                editor.cursor.0 as i32,
                editor.cursor.1 as i32,
            ),
        };
        let z = view.zoom as f32;
        let color = Color32::from_rgba_unmultiplied(255, 255, 255, 130);
        for (x, y) in pts {
            painter.rect_filled(
                Rect::from_min_size(view.pan + Vec2::new(x as f32, y as f32) * z, Vec2::splat(z)),
                0.0,
                color,
            );
        }
    }
}

fn draw_cursor(painter: &egui::Painter, editor: &Editor, view: &View) {
    let z = view.zoom as f32;
    let c = editor.cursor;
    let rect = Rect::from_min_size(
        view.pan + Vec2::new(c.0 as f32, c.1 as f32) * z,
        Vec2::splat(z),
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Inside,
    );
}

fn draw_lasso(painter: &egui::Painter, editor: &Editor, view: &View) {
    if editor.lasso_points.len() < 2 {
        return;
    }
    let z = view.zoom as f32;
    let pts: Vec<Pos2> = editor
        .lasso_points
        .iter()
        .map(|&(x, y)| view.pan + Vec2::new((x as f32 + 0.5) * z, (y as f32 + 0.5) * z))
        .collect();
    let stroke = Stroke::new(1.5, Color32::WHITE);
    for w in pts.windows(2) {
        painter.line_segment([w[0], w[1]], stroke);
    }
    if pts.len() >= 3 {
        painter.line_segment(
            [pts[pts.len() - 1], pts[0]],
            Stroke::new(1.5, Color32::from_white_alpha(150)),
        );
    }
}

fn draw_symmetry(painter: &egui::Painter, editor: &Editor, view: &View, rect: Rect) {
    let z = view.zoom as f32;
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 220, 0, 90));
    let (cx, cy) = editor.symmetry_center();
    let at = |x: f32, y: f32| view.pan + Vec2::new(x, y) * z;
    if matches!(editor.symmetry, Symmetry::Vertical | Symmetry::Both) {
        let x = at(cx + 0.5, 0.0).x;
        painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], stroke);
    }
    if matches!(editor.symmetry, Symmetry::Horizontal | Symmetry::Both) {
        let y = at(0.0, cy + 0.5).y;
        painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], stroke);
    }
    if matches!(editor.symmetry, Symmetry::Radial) {
        let center = at(cx + 0.5, cy + 0.5);
        let n = editor.sym_axes.max(2) as f32;
        let r = rect.width().max(rect.height());
        for k in 0..n as u32 {
            let a = k as f32 * std::f32::consts::TAU / n;
            let d = Vec2::new(a.cos(), a.sin()) * r;
            painter.line_segment([center - d, center + d], stroke);
        }
    }
}

fn draw_selection(painter: &egui::Painter, editor: &Editor, view: &View) {
    if let Some(sel) = editor.current_selection() {
        let z = view.zoom as f32;
        let fill = Color32::from_rgba_unmultiplied(120, 160, 255, 60);
        for yy in 0..sel.h {
            for xx in 0..sel.w {
                if sel.mask[(yy * sel.w + xx) as usize] {
                    painter.rect_filled(
                        Rect::from_min_size(
                            view.pan + Vec2::new((sel.x + xx) as f32, (sel.y + yy) as f32) * z,
                            Vec2::splat(z),
                        ),
                        0.0,
                        fill,
                    );
                }
            }
        }
        let r = Rect::from_min_size(
            view.pan + Vec2::new(sel.x as f32, sel.y as f32) * z,
            Vec2::new(sel.w as f32 * z, sel.h as f32 * z),
        );
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.5, Color32::from_rgba_unmultiplied(120, 160, 255, 255)),
            StrokeKind::Inside,
        );
    }
}

fn draw_floating_paste(painter: &egui::Painter, editor: &Editor, view: &View) {
    if let (Some(buf), Some(pos)) = (&editor.clipboard, editor.paste_pos) {
        let z = view.zoom as f32;
        for yy in 0..buf.h {
            for xx in 0..buf.w {
                let c = buf.pixels[(yy * buf.w + xx) as usize];
                if c.is_transparent() {
                    continue;
                }
                let r = Rect::from_min_size(
                    view.pan + Vec2::new((pos.0 + xx) as f32, (pos.1 + yy) as f32) * z,
                    Vec2::splat(z),
                );
                painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(c.0, c.1, c.2, 160));
            }
        }
        let r = Rect::from_min_size(
            view.pan + Vec2::new(pos.0 as f32, pos.1 as f32) * z,
            Vec2::new(buf.w as f32 * z, buf.h as f32 * z),
        );
        painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
    }
}

fn reveal_cursor(editor: &Editor, view: &mut View, panel: Rect) {
    let z = view.zoom as f32;
    let c = editor.cursor;
    let min = view.pan + Vec2::new(c.0 as f32, c.1 as f32) * z;
    let max = min + Vec2::splat(z);
    let mut pan = view.pan;
    if max.x > panel.max.x {
        pan.x -= max.x - panel.max.x;
    }
    if min.x < panel.min.x {
        pan.x += panel.min.x - min.x;
    }
    if max.y > panel.max.y {
        pan.y -= max.y - panel.max.y;
    }
    if min.y < panel.min.y {
        pan.y += panel.min.y - min.y;
    }
    view.pan = pan;
}

fn canvas_input(
    ui: &egui::Ui,
    editor: &mut Editor,
    view: &mut View,
    resp: egui::Response,
    w: u32,
    h: u32,
) {
    use egui::PointerButton as PB;

    if resp.dragged_by(PB::Middle) {
        view.pan += resp.drag_delta();
        return;
    }

    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll != 0.0
        && let Some(pos) = resp.hover_pos()
    {
        view.zoom_at(pos, if scroll > 0.0 { 1 } else { -1 });
    }

    // lasso / polygon selection tools
    let pos = resp.interact_pointer_pos().unwrap_or_default();
    match editor.tool {
        Tool::Lasso => {
            if resp.drag_started_by(PB::Primary)
                && let Some((x, y)) = view.screen_to_pixel(pos, w, h)
            {
                editor.lasso_start(x, y);
            }
            if resp.dragged_by(PB::Primary)
                && let Some((x, y)) = view.screen_to_pixel(pos, w, h)
            {
                editor.lasso_add(x, y);
            }
            if resp.drag_stopped_by(PB::Primary) {
                editor.lasso_finish();
            }
            return;
        }
        Tool::Polygon => {
            if resp.clicked_by(PB::Primary)
                && let Some((x, y)) = view.screen_to_pixel(pos, w, h)
            {
                editor.lasso_add(x, y);
            }
            return;
        }
        _ => {}
    }

    // primary: paint with the active tool
    if resp.drag_started_by(PB::Primary) {
        editor.begin_stroke();
    }
    if resp.dragged_by(PB::Primary) {
        paint_at(editor, view, resp.interact_pointer_pos(), w, h, false);
    }
    if resp.clicked_by(PB::Primary) {
        editor.begin_stroke();
        paint_at(editor, view, resp.interact_pointer_pos(), w, h, false);
        editor.end_stroke();
    }
    if resp.drag_stopped_by(PB::Primary) {
        editor.end_stroke();
    }

    // secondary: paint with the bg color
    if resp.drag_started_by(PB::Secondary) {
        editor.begin_stroke();
    }
    if resp.dragged_by(PB::Secondary) {
        paint_at(editor, view, resp.interact_pointer_pos(), w, h, true);
    }
    if resp.clicked_by(PB::Secondary) {
        editor.begin_stroke();
        paint_at(editor, view, resp.interact_pointer_pos(), w, h, true);
        editor.end_stroke();
    }
    if resp.drag_stopped_by(PB::Secondary) {
        editor.end_stroke();
    }

    // middle click: pick color
    if resp.clicked_by(PB::Middle)
        && let Some((x, y)) = pixel_at(
            editor,
            view,
            resp.interact_pointer_pos().unwrap_or_default(),
            w,
            h,
        )
    {
        editor.pick_color(x, y);
    }
}

fn paint_at(editor: &mut Editor, view: &View, pos: Option<Pos2>, w: u32, h: u32, use_bg: bool) {
    if let Some(pos) = pos
        && let Some((x, y)) = pixel_at(editor, view, pos, w, h)
    {
        editor.cursor = (x, y);
        if use_bg {
            editor.paint_secondary(x, y);
        } else {
            editor.paint_with_tool(x, y);
        }
    }
}

fn pixel_at(editor: &Editor, view: &View, pos: Pos2, w: u32, h: u32) -> Option<(u32, u32)> {
    if editor.tile_mode {
        view.screen_to_pixel_wrapped(pos, w, h)
    } else {
        view.screen_to_pixel(pos, w, h)
    }
}

// ---- help overlay ----

fn help(ctx: &egui::Context) {
    egui::Window::new("vixel — vim keys")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            for line in HELP {
                ui.label(*line);
            }
        });
}

fn swatch(ui: &mut egui::Ui, color: Rgba, _selected: bool) -> egui::Response {
    let c = Color32::from_rgba_unmultiplied(color.0, color.1, color.2, 255);
    ui.add(
        egui::Button::new("")
            .fill(c)
            .min_size(Vec2::new(18.0, 18.0))
            .stroke(Stroke::new(1.0, Color32::from_gray(90))),
    )
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Normal => "normal",
        Mode::Draw => "draw",
        Mode::Visual => "visual",
        Mode::Paste => "paste",
        Mode::Command => "command",
        Mode::Help => "help",
    }
}

const HELP: &[&str] = &[
    "normal mode:",
    "  hjkl / arrows      move          H J K L   move 8px",
    "  0 / $ / gg / G     left / right / top / bottom",
    "  i                  draw mode (paint as you move)",
    "  x                  erase pixel (vim delete)",
    "  r / space          paint pixel (fg)",
    "  b / e / f / c      pencil / eraser / bucket / picker",
    "  d / R / o          line / rect / ellipse (Enter commits, Esc cancels)",
    "  z / a / q          gradient / spray / shade",
    "  n / C              blur / contour outline",
    "  v                  visual select (V = all)   then:",
    "    y                yank (copy)      Y   copy merged",
    "    x                cut              m   move (cut + float)   p   paste",
    "  w                  magic wand (select same-color region)",
    "  L / P              lasso / polygon selection (mouse)",
    "  s                  cycle symmetry (none / H / V / both)",
    "  :tile [on|off]     seamless 3x3 preview; cursor wraps at the edges",
    "  t                  toggle filled shapes (rect / ellipse)",
    "  , / .              prev / next color",
    "  < / >              prev / next layer",
    "  [ / ]              prev / next frame",
    "  X                  swap fg / bg",
    "  u / Ctrl-r         undo / redo",
    "  + / -              zoom in / out",
    "  Ctrl-c / x / v     copy / cut / paste (OS clipboard)",
    "  Ctrl-s / Ctrl-q    save / quit",
    "  :                  command line",
    "",
    "mouse: left=paint fg  right=paint bg  middle=pick  wheel=zoom  middle-drag=pan",
    "",
    "commands:",
    "  :w [file]   :q   :q!   :wq   :new [w h]   :resize w h",
    "  :newframe / :dupframe / :delframe   (drag a thumbnail to reorder)",
    "  :tag <name> [from to] / :tagdel [name]   (named frame ranges)",
    "  :tagdir forward|reverse|pingpong|pingpong-reverse   :tagrepeat <n>   (0 = forever)",
    "  :framemove <from> <to>   :celz <n>   (cel z-index; + = towards the front)",
    "  :newlayer / :dellayer / :layerup / :layerdown / :layername <name>",
    "  :duplayer   :merge   :flatten   (duplicate / merge down / flatten)",
    "  :newgroup / :indent / :outdent / :lock / :unlock   (layer groups)",
    "  :background on|off   :reference on|off   (layer kinds)",
    "  :mask add|apply|del   (alpha mask: selection if any, else full)",
    "  :fliph   :flipv   :rot90   :sym none|horizontal|vertical|both",
    "  :wand <0-255>   :wandall on|off   :contig on|off (wand + bucket)",
    "  :sel expand|contract|border|feather <n>   :sel invert|none|all",
    "  :filter invert|brightness|contrast|hue|saturation|outline|blur|sharpen [amount]",
    "  :rotate <deg>   :scale <sx> [sy]   :skew <deg>   (free transform)",
    "  :brush <1-64>   :perfect on|off   :grid <px>   :snap on|off   (brush, grid)",
    "  :crop [x y w h]   :trim   (crop to a rect/selection, or to content)",
    "  :mode auto|rgba|grayscale|indexed   :dither none|bayer2|bayer4|bayer8|floyd",
    "  :aspect <x> <y>   :sgrid [x y] <w> <h>",
    "  :pal preset <name>|insert <i> [#hex]|delete <i>|sort [rgb|luma|hue]|name <i> <name>",
    "  :profile [srgb|linear|icc <file>|gamma <g>|clear]   (working space / .ase profile)",
    "  :renderer glow|wgpu    (wgpu also needs a build with --features wgpu)",
    "  :slices <dir> [cols rows]   (one png per grid cell + a slices.json)",
    "  :grad linear|radial   :contour",
    "  :export out.png|.ase|stem [scale 1-16]   :import in.png   :open proj.vpx|.ase|.png",
    "  :importsheet sheet.png [cols rows]   (frames from a sheet + its .json)",
    "  :recent [n]   (last 10 files; drop a file on the window to open it)",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_maps_screen_angles() {
        let c = Pos2::new(50.0, 50.0);
        assert!((wheel_hue(c, Pos2::new(100.0, 50.0)) - 0.0).abs() < 0.01);
        assert!((wheel_hue(c, Pos2::new(50.0, 100.0)) - 90.0).abs() < 0.01);
        assert!((wheel_hue(c, Pos2::new(0.0, 50.0)) - 180.0).abs() < 0.01);
        assert!((wheel_hue(c, Pos2::new(50.0, 0.0)) - 270.0).abs() < 0.01);
    }
}
