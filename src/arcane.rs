//! Arcane Pixel UI: palette, spacing, flat chrome and the dotted surface.
//!
//! The Arcane shell: geometry is the pixel look: straight
//! edges, 1 px borders, a lighter top edge, sunken wells and raised buttons.
//!
//! Tokens and helpers are the design system for the later port steps too, so
//! some entries are intentionally unused for now.
#![allow(dead_code)]

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Stroke, StrokeKind,
    TextStyle, TextureHandle, TextureOptions,
};

// --- palette -------------------------------------------------------------

pub const VOID: Color32 = Color32::from_rgb(8, 6, 12);
pub const PANEL: Color32 = Color32::from_rgb(18, 13, 27);
pub const PANEL_HI: Color32 = Color32::from_rgb(26, 19, 39);
pub const SUNKEN: Color32 = Color32::from_rgb(7, 5, 10);
pub const LINE: Color32 = Color32::from_rgb(43, 32, 56);
pub const LINE_HI: Color32 = Color32::from_rgb(74, 53, 104);
pub const TEXT: Color32 = Color32::from_rgb(236, 228, 246);
pub const DIM: Color32 = Color32::from_rgb(162, 148, 187);
pub const FAINT: Color32 = Color32::from_rgb(132, 122, 156);
pub const ACCENT: Color32 = Color32::from_rgb(181, 108, 255);
pub const ACCENT_HI: Color32 = Color32::from_rgb(215, 165, 255);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(85, 32, 127);
pub const FOCUS: Color32 = Color32::from_rgb(255, 204, 102);
pub const WARN: Color32 = Color32::from_rgb(227, 176, 75);
pub const GOOD: Color32 = Color32::from_rgb(121, 201, 154);
pub const DANGER: Color32 = Color32::from_rgb(226, 106, 131);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub void: Color32,
    pub panel: Color32,
    pub panel_hi: Color32,
    pub sunken: Color32,
    pub line: Color32,
    pub line_hi: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_hi: Color32,
    pub accent_deep: Color32,
    pub focus: Color32,
    pub warn: Color32,
    pub good: Color32,
    pub danger: Color32,
}

pub const ARCANE: Palette = Palette {
    void: VOID,
    panel: PANEL,
    panel_hi: PANEL_HI,
    sunken: SUNKEN,
    line: LINE,
    line_hi: LINE_HI,
    text: TEXT,
    dim: DIM,
    faint: FAINT,
    accent: ACCENT,
    accent_hi: ACCENT_HI,
    accent_deep: ACCENT_DEEP,
    focus: FOCUS,
    warn: WARN,
    good: GOOD,
    danger: DANGER,
};

pub fn palette() -> Palette {
    ARCANE
}

// --- spacing and type ----------------------------------------------------

pub const SPACE_2: f32 = 2.0;
pub const SPACE_4: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_8: f32 = 8.0;
pub const SPACE_12: f32 = 12.0;
pub const SPACE_16: f32 = 16.0;

pub const TYPE_TITLE: f32 = 18.0;
pub const TYPE_HEADING: f32 = 14.0;
pub const TYPE_BODY: f32 = 12.0;
pub const TYPE_LABEL: f32 = 10.0;
pub const TYPE_MICRO: f32 = 8.0;

/// Width reserved for the floating inspector in the overlay layout.
pub const OVERLAY_INSPECTOR_W: f32 = 380.0;

/// Apply the Arcane visuals. Fonts are installed by `theme::install_font`.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    let p = palette();
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = p.void;
    visuals.window_fill = p.panel;
    visuals.extreme_bg_color = p.sunken;
    visuals.faint_bg_color = p.panel_hi;
    visuals.override_text_color = Some(p.text);
    visuals.selection.bg_fill = p.accent;
    visuals.selection.stroke = Stroke::new(1.0, p.text);
    visuals.hyperlink_color = p.accent;
    visuals.window_stroke = Stroke::new(1.0, p.line);
    visuals.widgets.noninteractive.bg_fill = p.panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    visuals.widgets.inactive.bg_fill = p.panel;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.dim);
    visuals.widgets.hovered.bg_fill = p.panel_hi;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.line_hi);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
    visuals.widgets.active.bg_fill = p.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, p.sunken);
    visuals.widgets.open.bg_fill = p.panel_hi;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, p.line);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::ZERO;
    }
    visuals.menu_corner_radius = CornerRadius::ZERO;
    visuals.window_corner_radius = CornerRadius::ZERO;
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(SPACE_6, SPACE_4);
    style.spacing.button_padding = egui::vec2(SPACE_8, SPACE_4);
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    for (text, size) in [
        (TextStyle::Heading, TYPE_HEADING),
        (TextStyle::Body, TYPE_BODY),
        (TextStyle::Button, TYPE_BODY),
        (TextStyle::Small, TYPE_LABEL),
        (TextStyle::Monospace, TYPE_LABEL),
    ] {
        style
            .text_styles
            .insert(text, FontId::new(size, FontFamily::Monospace));
    }
    ctx.set_style_of(egui::Theme::Dark, style);
}

// --- chrome --------------------------------------------------------------

pub fn fill(painter: &Painter, rect: Rect, color: Color32) {
    painter.rect_filled(rect, 0.0, color);
}

pub fn border(painter: &Painter, rect: Rect, color: Color32, width: f32) {
    painter.rect_stroke(rect, 0.0, Stroke::new(width, color), StrokeKind::Inside);
}

/// A panel surface: ink, dot grain, hairline border and a lighter top edge.
pub fn panel(painter: &Painter, rect: Rect, dots: Option<&TextureHandle>, p: Palette) {
    fill(painter, rect, p.panel);
    if let Some(texture) = dots {
        paint_dots(painter, rect, texture);
    }
    border(painter, rect, p.line, 1.0);
    painter.line_segment(
        [
            Pos2::new(rect.left() + 1.0, rect.top() + 1.5),
            Pos2::new(rect.right() - 1.0, rect.top() + 1.5),
        ],
        Stroke::new(1.0, p.line_hi),
    );
}

/// A sunken surface, for the canvas well and the status/command line.
pub fn well(painter: &Painter, rect: Rect, p: Palette) {
    fill(painter, rect, p.sunken);
    border(painter, rect, p.line, 1.0);
    painter.line_segment(
        [
            rect.left_top() + egui::vec2(1.0, 1.0),
            rect.right_top() + egui::vec2(-1.0, 1.0),
        ],
        Stroke::new(1.0, Color32::BLACK),
    );
    painter.line_segment(
        [
            rect.left_top() + egui::vec2(1.0, 1.0),
            rect.left_bottom() + egui::vec2(1.0, -1.0),
        ],
        Stroke::new(1.0, Color32::BLACK),
    );
}

pub fn tab(painter: &Painter, rect: Rect, selected: bool, p: Palette) {
    fill(painter, rect, if selected { p.panel_hi } else { p.sunken });
    border(
        painter,
        rect,
        if selected { p.line_hi } else { p.line },
        1.0,
    );
    if selected {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + 1.0, rect.bottom() - 2.0),
                egui::vec2((rect.width() - 2.0).max(0.0), 2.0),
            ),
            0.0,
            p.accent,
        );
    }
}

pub fn raised(
    painter: &Painter,
    rect: Rect,
    fill_color: Color32,
    border_color: Color32,
    pressed: bool,
    p: Palette,
) {
    fill(painter, rect, fill_color);
    border(painter, rect, border_color, 1.0);
    let edge = if pressed { Color32::BLACK } else { p.line_hi };
    painter.line_segment(
        [
            rect.left_top() + egui::vec2(1.0, 1.0),
            rect.right_top() + egui::vec2(-1.0, 1.0),
        ],
        Stroke::new(1.0, edge),
    );
    if !pressed {
        painter.line_segment(
            [
                rect.left_top() + egui::vec2(1.0, 1.0),
                rect.left_bottom() + egui::vec2(1.0, -1.0),
            ],
            Stroke::new(1.0, edge),
        );
    }
}

/// A small uppercase mode badge (NORMAL / DRAW / VISUAL ...).
/// The frame used by the docked panels. Straight corners, 1 px border.
pub fn panel_frame(margin: f32) -> egui::Frame {
    let p = palette();
    egui::Frame::NONE
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::ZERO)
        .inner_margin(margin)
}

/// Corner brackets around the canvas, the game-HUD cue.
/// Paint a 12×12 ASCII pixel icon scaled into `rect`, centred.
pub fn paint_icon(painter: &Painter, rect: Rect, rows: &[&str], color: Color32) {
    if rows.is_empty() {
        return;
    }
    let cell = (rect.width().min(rect.height()) / 12.0).floor().max(1.0);
    let w = cell * 12.0;
    let origin = rect.center() - egui::vec2(w / 2.0, w / 2.0);
    for (y, row) in rows.iter().enumerate().take(12) {
        for (x, ch) in row.chars().enumerate().take(12) {
            if ch == '#' {
                painter.rect_filled(
                    Rect::from_min_size(
                        origin + egui::vec2(x as f32 * cell, y as f32 * cell),
                        egui::vec2(cell, cell),
                    ),
                    0.0,
                    color,
                );
            }
        }
    }
}

pub fn brackets(painter: &Painter, rect: Rect, p: Palette) {
    let color = Color32::from_rgba_unmultiplied(p.accent.r(), p.accent.g(), p.accent.b(), 150);
    let len = 12.0;
    let s = Stroke::new(1.5, color);
    for (corner, dx, dy) in [
        (rect.left_top(), 1.0, 1.0),
        (rect.right_top(), -1.0, 1.0),
        (rect.left_bottom(), 1.0, -1.0),
        (rect.right_bottom(), -1.0, -1.0),
    ] {
        painter.line_segment([corner, corner + egui::vec2(len * dx, 0.0)], s);
        painter.line_segment([corner, corner + egui::vec2(0.0, len * dy)], s);
    }
}

pub fn mode_chip(ui: &mut egui::Ui, label: &str, color: Color32) -> egui::Response {
    let p = palette();
    let font = FontId::monospace(TYPE_LABEL);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), p.text);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + SPACE_12, 20.0),
        egui::Sense::hover(),
    );
    fill(ui.painter(), rect, p.accent_deep);
    border(ui.painter(), rect, color, 1.0);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        color,
    );
    response
}

// --- dotted surface ------------------------------------------------------

pub fn dot_image() -> egui::ColorImage {
    const SIDE: usize = 16;
    let mut bytes = vec![0u8; SIDE * SIDE * 4];
    for (x, y) in [(3, 3), (11, 9), (7, 13)] {
        let index = (y * SIDE + x) * 4;
        bytes[index..index + 4].copy_from_slice(&[236, 228, 246, 18]);
    }
    egui::ColorImage::from_rgba_unmultiplied([SIDE, SIDE], &bytes)
}

pub fn dot_texture(ctx: &egui::Context) -> TextureHandle {
    ctx.load_texture("arcane-dots", dot_image(), TextureOptions::NEAREST_REPEAT)
}

pub fn paint_dots(painter: &Painter, rect: Rect, texture: &TextureHandle) {
    let uv = Rect::from_min_max(
        Pos2::ZERO,
        Pos2::new(rect.width() / 16.0, rect.height() / 16.0),
    );
    painter.image(texture.id(), rect, uv, Color32::WHITE);
}

/// WCAG contrast ratio, used by the palette tests.
pub fn contrast(fg: Color32, bg: Color32) -> f32 {
    fn luminance(color: Color32) -> f32 {
        let channel = |value: u8| {
            let c = value as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }
    let (a, b) = (luminance(fg) + 0.05, luminance(bg) + 0.05);
    if a > b { a / b } else { b / a }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcane_text_meets_contrast_targets() {
        assert!(contrast(TEXT, PANEL) >= 7.0);
        assert!(contrast(TEXT, VOID) >= 7.0);
        assert!(contrast(DIM, PANEL) >= 4.5);
        assert!(contrast(FAINT, PANEL) >= 3.0);
        assert!(contrast(ACCENT_HI, PANEL) >= 4.5);
        assert!(contrast(DANGER, PANEL) >= 4.5);
        assert!(contrast(GOOD, PANEL) >= 4.5);
    }

    #[test]
    fn the_dot_texture_is_a_repeating_tile() {
        let image = dot_image();
        assert_eq!(image.size, [16, 16]);
        assert_eq!(image.pixels.iter().filter(|p| p.a() > 0).count(), 3);
        assert!(image.pixels.iter().all(|p| p.a() <= 18));
    }
}
