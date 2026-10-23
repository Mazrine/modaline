use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Vec2};

use crate::color::Rgba;
use crate::editor::{Editor, Mode, SelectionOp, Shape, Symmetry, Tool};
use crate::sprite::{BlendMode, ellipse_points, line_points, rect_points};

/// The canvas view: zoom (screen px per sprite px) and pan (screen pos of the
/// sprite's top-left pixel).
pub struct View {
    pub zoom: u32,
    pub pan: Pos2,
    pub initialized: bool,
    pub show_grid: bool,
    pub onion: bool,
    /// Cached per-frame timeline thumbnails (rebuilt each UI pass).
    thumbs: Vec<Option<TextureHandle>>,
}

impl View {
    pub fn new() -> Self {
        Self {
            zoom: 16,
            pan: Pos2::ZERO,
            initialized: false,
            show_grid: true,
            onion: true,
            thumbs: Vec::new(),
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
            Some((x as u32, y as u32))
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
            .selectable_label(editor.symmetry != Symmetry::None, format!("sym:{}", editor.symmetry.name()))
            .clicked()
        {
            editor.cycle_symmetry();
        }
        if ui.selectable_label(editor.filled_shapes, "fill").clicked() {
            editor.toggle_filled_shapes();
        }
        let gmode = if editor.gradient_radial { "grad:rad" } else { "grad:lin" };
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
        ui.label(format!("{}x{} ({},{})", editor.sprite.width, editor.sprite.height, editor.cursor.0, editor.cursor.1));

        ui.separator();
        if editor.dirty {
            ui.colored_label(Color32::YELLOW, "●");
        }
        if !editor.msg.is_empty() {
            ui.label(&editor.msg);
        }
    });

    if editor.mode == Mode::Command {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!(":{}", editor.cmdline)).strong());
            ui.label(egui::RichText::new("▏").weak());
        });
    }
}

// ---- bottom timeline ----

fn timeline(ui: &mut egui::Ui, editor: &mut Editor, playing: &mut bool, view: &mut View) {
    ui.horizontal(|ui| {
        let play_label = if *playing { "⏸" } else { "⏵" };
        if ui.button(play_label).clicked() {
            *playing = !*playing;
        }
        if ui.button("＋frame").clicked() {
            editor.add_frame();
        }
        if ui.button("⧉frame").clicked() {
            editor.duplicate_frame();
        }
        if ui.button("－frame").clicked() {
            editor.del_frame();
        }
        ui.separator();

        let count = editor.sprite.frames.len();
        if view.thumbs.len() < count {
            view.thumbs.resize_with(count, || None);
        } else if view.thumbs.len() > count {
            view.thumbs.truncate(count);
        }

        let mut selected = editor.active_frame;
        let th = 40.0;
        let aspect = editor.sprite.width as f32 / editor.sprite.height.max(1) as f32;
        let tw = (th * aspect).clamp(10.0, 120.0);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

        for i in 0..count {
            let comp = frame_composite(editor, i);
            upload(ui, &mut view.thumbs[i], &format!("thumb{i}"), comp);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(tw, th), Sense::click());
            draw_checker(ui.painter(), rect, 4.0);
            if let Some(t) = &view.thumbs[i] {
                ui.painter().image(t.id(), rect, uv, Color32::WHITE);
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
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
        editor.active_frame = selected;

        ui.separator();
        ui.label("ms:");
        let d = &mut editor.sprite.frames[editor.active_frame].duration_ms;
        ui.add(egui::DragValue::new(d).range(1..=10_000));
    });
}

// ---- right side panel ----

fn side(ui: &mut egui::Ui, editor: &mut Editor, view: &mut View) {
    ui.checkbox(&mut view.show_grid, "pixel grid");
    ui.checkbox(&mut view.onion, "onion skin");
    ui.horizontal(|ui| {
        ui.label("wand tol");
        ui.add(egui::Slider::new(&mut editor.wand_tolerance, 0..=255));
    });
    ui.checkbox(&mut editor.wand_all_layers, "wand: all layers");
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("fg");
        let mut srgb = [editor.fg.0, editor.fg.1, editor.fg.2];
        if ui.color_edit_button_srgb(&mut srgb).changed() {
            editor.fg = Rgba(srgb[0], srgb[1], srgb[2], 255);
        }
    });
    ui.horizontal(|ui| {
        ui.label("bg");
        let mut srgb = [editor.bg.0, editor.bg.1, editor.bg.2];
        if ui.color_edit_button_srgb(&mut srgb).changed() {
            editor.bg = Rgba(srgb[0], srgb[1], srgb[2], 255);
        }
    });
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Palette").strong());
        if ui.button("＋fg").clicked() {
            editor.add_palette_color();
        }
        if ui.button("－").clicked() {
            editor.remove_palette_color();
        }
    });
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (i, c) in editor.palette.iter().enumerate() {
                let col = Color32::from_rgba_unmultiplied(c.0, c.1, c.2, 255);
                let mut btn = egui::Button::new("").fill(col).min_size(Vec2::splat(20.0));
                if i == editor.fg_idx {
                    btn = btn.stroke(Stroke::new(2.0, Color32::WHITE));
                }
                let resp = ui
                    .add(btn)
                    .on_hover_text(format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2));
                if resp.clicked() {
                    editor.fg = *c;
                    editor.fg_idx = i;
                }
                if resp.secondary_clicked() {
                    editor.bg = *c;
                    editor.bg_idx = i;
                }
            }
        });
    });
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Layers").strong());
        if ui.button("↑").clicked() {
            editor.move_layer_active(-1);
        }
        if ui.button("↓").clicked() {
            editor.move_layer_active(1);
        }
        if ui.button("＋").clicked() {
            editor.add_layer();
        }
        if ui.button("－").clicked() {
            editor.del_layer();
        }
    });
    let count = editor.sprite.layers.len();
    let mut selected = editor.active_layer;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for i in 0..count {
            let layer = &mut editor.sprite.layers[i];
            ui.horizontal(|ui| {
                if ui.selectable_label(i == selected, &layer.name).clicked() {
                    selected = i;
                }
                ui.checkbox(&mut layer.visible, "");
                blend_combo(ui, i, &mut layer.blend);
            });
            ui.add(egui::Slider::new(&mut layer.opacity, 0..=255).text("op"));
        }
    });
    editor.active_layer = selected;
}

fn blend_combo(ui: &mut egui::Ui, i: usize, blend: &mut BlendMode) {
    egui::ComboBox::from_id_salt(("blend", i))
        .selected_text(blend_name(*blend))
        .show_ui(ui, |ui| {
            for b in ALL_BLENDS {
                ui.selectable_value(blend, b, blend_name(b));
            }
        });
}

fn blend_name(b: BlendMode) -> &'static str {
    match b {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::Add => "add",
        BlendMode::Subtract => "subtract",
    }
}

const ALL_BLENDS: [BlendMode; 8] = [
    BlendMode::Normal,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::Add,
    BlendMode::Subtract,
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

    upload(ui, texture, "canvas", frame_composite(editor, editor.active_frame));

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

    if view.onion && editor.sprite.frames.len() > 1 {
        let n = editor.sprite.frames.len();
        let prev = (editor.active_frame + n - 1) % n;
        let next = (editor.active_frame + 1) % n;
        upload(ui, onion_prev, "onion_prev", frame_composite(editor, prev));
        upload(ui, onion_next, "onion_next", frame_composite(editor, next));
        ui.painter().image(
            onion_prev.as_ref().unwrap().id(),
            rect,
            uv,
            Color32::from_rgba_unmultiplied(255, 90, 90, 110),
        );
        ui.painter().image(
            onion_next.as_ref().unwrap().id(),
            rect,
            uv,
            Color32::from_rgba_unmultiplied(90, 210, 90, 110),
        );
    }

    ui.painter().image(tex.id(), rect, uv, Color32::WHITE);

    if view.show_grid && view.zoom >= 6 {
        draw_grid(ui.painter(), rect, view.zoom as f32, w, h);
    }
    draw_pending(ui.painter(), editor, view);
    draw_selection(ui.painter(), editor, view);
    draw_floating_paste(ui.painter(), editor, view);
    draw_lasso(ui.painter(), editor, view);
    draw_symmetry(ui.painter(), editor, view, rect);
    draw_cursor(ui.painter(), editor, view);

    let resp = ui.interact(rect.intersect(panel), ui.scope_id().with("canvas"), Sense::click_and_drag());
    canvas_input(ui, editor, view, resp, w, h);
}

fn upload(ui: &egui::Ui, slot: &mut Option<TextureHandle>, name: &str, img: egui::ColorImage) {
    match slot {
        Some(t) => t.set(img, egui::TextureOptions::NEAREST),
        None => {
            *slot = Some(ui.ctx().load_texture(name, img, egui::TextureOptions::NEAREST));
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
                Rect::from_min_size(
                    view.pan + Vec2::new(x as f32, y as f32) * z,
                    Vec2::splat(z),
                ),
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
    painter.rect_stroke(rect, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
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
    if matches!(editor.symmetry, Symmetry::Vertical | Symmetry::Both) {
        let cx = view.pan.x + (editor.sprite.width as f32 / 2.0) * z;
        painter.line_segment([Pos2::new(cx, rect.min.y), Pos2::new(cx, rect.max.y)], stroke);
    }
    if matches!(editor.symmetry, Symmetry::Horizontal | Symmetry::Both) {
        let cy = view.pan.y + (editor.sprite.height as f32 / 2.0) * z;
        painter.line_segment([Pos2::new(rect.min.x, cy), Pos2::new(rect.max.x, cy)], stroke);
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
        && let Some(pos) = resp.hover_pos() {
            view.zoom_at(pos, if scroll > 0.0 { 1 } else { -1 });
        }

    // lasso / polygon selection tools
    let pos = resp.interact_pointer_pos().unwrap_or_default();
    match editor.tool {
        Tool::Lasso => {
            if resp.drag_started_by(PB::Primary)
                && let Some((x, y)) = view.screen_to_pixel(pos, w, h) {
                    editor.lasso_start(x, y);
                }
            if resp.dragged_by(PB::Primary)
                && let Some((x, y)) = view.screen_to_pixel(pos, w, h) {
                    editor.lasso_add(x, y);
                }
            if resp.drag_stopped_by(PB::Primary) {
                editor.lasso_finish();
            }
            return;
        }
        Tool::Polygon => {
            if resp.clicked_by(PB::Primary)
                && let Some((x, y)) = view.screen_to_pixel(pos, w, h) {
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
        && let Some((x, y)) = view.screen_to_pixel(resp.interact_pointer_pos().unwrap_or_default(), w, h) {
            editor.pick_color(x, y);
        }
}

fn paint_at(
    editor: &mut Editor,
    view: &View,
    pos: Option<Pos2>,
    w: u32,
    h: u32,
    use_bg: bool,
) {
    if let Some(pos) = pos
        && let Some((x, y)) = view.screen_to_pixel(pos, w, h)
    {
        editor.cursor = (x, y);
        if use_bg {
            editor.paint_secondary(x, y);
        } else {
            editor.paint_with_tool(x, y);
        }
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
    "    y                yank (copy)      x   cut",
    "    m                move (cut + float)   p   paste",
    "  w                  magic wand (select same-color region)",
    "  L / P              lasso / polygon selection (mouse)",
    "  s                  cycle symmetry (none / H / V / both)",
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
    "  :newframe / :dupframe / :delframe",
    "  :newlayer / :dellayer / :layerup / :layerdown / :layername <name>",
    "  :fliph   :flipv   :rot90   :sym none|horizontal|vertical|both",
    "  :wand <0-255>   :wandall on|off   :selop replace|add|subtract|intersect",
    "  :grad linear|radial   :contour",
    "  :export out.png|.ase   :import in.png   :open proj.vpx|.ase|.png",
];
