use std::path::PathBuf;

use crate::color::Rgba;
use crate::sprite::{
    FillOptions, Sprite, color_close, ellipse_points, flood_fill_tol, line_points,
    rect_filled_points, rect_points,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Draw,
    Visual,
    Paste,
    Command,
    Help,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pencil,
    Eraser,
    Bucket,
    Picker,
    Line,
    Rect,
    Ellipse,
    Gradient,
    Shade,
    Spray,
    Blur,
    Lasso,
    Polygon,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Symmetry {
    None,
    Horizontal,
    Vertical,
    Both,
    /// N-fold rotation about the symmetry centre.
    Radial,
}

impl Symmetry {
    pub fn name(self) -> &'static str {
        match self {
            Symmetry::None => "none",
            Symmetry::Horizontal => "horizontal",
            Symmetry::Vertical => "vertical",
            Symmetry::Both => "both",
            Symmetry::Radial => "radial",
        }
    }
}

/// How a new selection combines with the existing one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectionOp {
    Replace,
    Add,
    Subtract,
    Intersect,
}

impl SelectionOp {
    pub fn name(self) -> &'static str {
        match self {
            SelectionOp::Replace => "replace",
            SelectionOp::Add => "add",
            SelectionOp::Subtract => "subtract",
            SelectionOp::Intersect => "intersect",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SelectionOp::Replace => SelectionOp::Add,
            SelectionOp::Add => SelectionOp::Subtract,
            SelectionOp::Subtract => SelectionOp::Intersect,
            SelectionOp::Intersect => SelectionOp::Replace,
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "replace" | "r" => Some(SelectionOp::Replace),
            "add" | "a" => Some(SelectionOp::Add),
            "subtract" | "sub" | "s" => Some(SelectionOp::Subtract),
            "intersect" | "and" | "i" => Some(SelectionOp::Intersect),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Line,
    Rect,
    Ellipse,
    Gradient,
}

/// A region transform for the selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Xform {
    FlipH,
    FlipV,
    Rot90,
}

/// A selected region of the sprite: a bounding box plus a per-pixel mask.
#[derive(Clone, Debug)]
pub struct Selection {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Row-major `w * h`, `true` = part of the selection.
    pub mask: Vec<bool>,
}

impl Selection {
    /// An axis-aligned rectangle selection.
    pub fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let (x, xw) = (x0.min(x1), x0.max(x1));
        let (y, yh) = (y0.min(y1), y0.max(y1));
        let w = (xw - x + 1) as u32;
        let h = (yh - y + 1) as u32;
        Selection {
            x: x as u32,
            y: y as u32,
            w,
            h,
            mask: vec![true; (w * h) as usize],
        }
    }

    fn selected(&self, dx: u32, dy: u32) -> bool {
        self.mask[(dy * self.w + dx) as usize]
    }

    /// Whether sprite pixel `(px, py)` is part of this selection.
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.w
            && py < self.y + self.h
            && self.mask[((py - self.y) * self.w + (px - self.x)) as usize]
    }
}

/// A copied region of pixels (from the active layer's cel), with its shape.
#[derive(Clone, Debug)]
pub struct Clipboard {
    pub w: u32,
    pub h: u32,
    pub pixels: Vec<Rgba>,
    pub mask: Vec<bool>,
}

/// A pending two-point shape: `start` is fixed, the cursor is the live end.
#[derive(Clone, Copy, Debug)]
pub struct Pending {
    pub shape: Shape,
    pub start: (u32, u32),
}

/// Active indices at one side of a structural edit.
#[derive(Clone, Copy, Debug)]
pub struct EditPoint {
    layer: usize,
    frame: usize,
    tag: Option<usize>,
}

/// One layer's display properties, as recorded by the layer panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerProps {
    pub visible: bool,
    pub editable: bool,
    pub opacity: u8,
    pub blend: crate::sprite::BlendMode,
    pub background: bool,
    pub reference: bool,
}

/// One undoable change.
pub enum UndoStep {
    /// Per-pixel writes, each `(layer, frame, x, y, before, after)`.
    Cells {
        cells: Vec<(usize, usize, u32, u32, Rgba, Rgba)>,
    },
    /// Whole-sprite snapshot (for whole-sprite transforms and mode changes).
    /// Boxed: two sprites are much larger than a cell list, and the enum is
    /// moved around on every undo push.
    Snapshot {
        before: Box<Sprite>,
        after: Box<Sprite>,
    },
    /// Whole-sprite snapshot for edits that change the layer/frame tree (add,
    /// delete, move, rename, lock, tags). The active indices travel with each
    /// side so undo/redo never leave them past the end of a shorter list.
    Structure {
        before: Box<Sprite>,
        after: Box<Sprite>,
        before_at: EditPoint,
        after_at: EditPoint,
    },
    /// One layer's visibility/opacity/blend/lock. Cheap enough to record on
    /// every panel click; the layer index does not move.
    LayerProps {
        layer: usize,
        before: LayerProps,
        after: LayerProps,
    },
}

/// The sprite model plus the interaction state (modes, tools, undo, colors).
/// All view state (zoom/pan) lives in the GUI layer, not here.
pub struct Editor {
    pub sprite: Sprite,
    pub cursor: (u32, u32),
    pub mode: Mode,
    pub tool: Tool,
    /// Pencil/eraser brush width in pixels (a square, clipped at the edges).
    pub brush_size: u32,
    /// Drop L-shaped corner pixels while freehand drawing.
    pub pixel_perfect: bool,
    /// Last points of the current freehand stroke, for the corner filter.
    pp: Vec<(u32, u32)>,
    pub fg: Rgba,
    pub bg: Rgba,
    pub palette: Vec<Rgba>,
    pub fg_idx: usize,
    pub bg_idx: usize,
    pub active_layer: usize,
    pub active_frame: usize,
    /// Index into `sprite.tags`; playback is restricted to it when set.
    pub active_tag: Option<usize>,
    pub symmetry: Symmetry,
    /// Axes for `Symmetry::Radial`.
    pub sym_axes: u8,
    /// Mirror/rotation origin. `None` is the sprite centre, which is what the
    /// plain horizontal/vertical/both modes have always used.
    pub sym_center: Option<(u32, u32)>,
    pub lasso_points: Vec<(u32, u32)>,
    pub selection_op: SelectionOp,
    pub filled_shapes: bool,
    pub gradient_radial: bool,
    pub spray_seed: u64,
    pub wand_tolerance: u8,
    pub wand_all_layers: bool,
    /// Wand and bucket restrict themselves to the seed's contiguous region.
    pub contiguous: bool,
    /// Show the sprite tiled 3x3 and wrap the cursor across its edges.
    pub tile_mode: bool,
    /// Design grid in sprite pixels (1 = off) and whether the cursor snaps to
    /// it. The pixel grid is a separate view flag.
    pub grid: u32,
    pub snap: bool,
    /// Bitmap-font text stamp: the scale (1 = 8x8 px) and the last string
    /// typed, so the panel keeps it between stamps.
    pub text_scale: u32,
    pub text: String,
    /// Playback: wrap at the end of the range, bounce instead of wrapping, and
    /// the current direction for the bounce.
    pub play_loop: bool,
    pub play_pingpong: bool,
    pub play_dir: i32,
    /// Completed passes in the current playback session, for a tag's repeat.
    pub play_passes: u32,
    pub undo: Vec<UndoStep>,
    pub redo: Vec<UndoStep>,
    pub stroke: Option<UndoStep>,
    pub pending: Option<Pending>,
    pub select_anchor: Option<(u32, u32)>,
    pub selection: Option<Selection>,
    pub clipboard: Option<Clipboard>,
    pub paste_pos: Option<(u32, u32)>,
    pub cmdline: String,
    pub msg: String,
    pub dirty: bool,
    /// Monotonic counter for cache invalidation (timeline thumbnails). Every
    /// edit bumps it; the view compares it instead of rebuilding each frame.
    pub revision: u64,
    pub should_quit: bool,
    pub g_prefix: bool,
    pub path: Option<PathBuf>,
    /// Recently opened or saved files, newest first (max 10).
    pub recent: Vec<PathBuf>,
    /// Set when the opened file is a another tool fix bundle: the part the strokes
    /// are for, from the sidecar. Also marks the file as one whose `ref` layer
    /// must survive a save.
    pub bundle_stage: Option<String>,
    /// Refuse to paint a colour that is not in the palette. Off by default: a
    /// plain sprite has no ramp to protect.
    pub palette_lock: bool,
    /// The renderer the user asked for. Applied at startup, so changing it here
    /// only takes effect on the next launch.
    pub renderer: String,
}

impl Editor {
    pub fn new(width: u32, height: u32) -> Self {
        let palette = crate::color::default_palette();
        let fg_idx = 12; // white
        let fg = palette[fg_idx];
        let bg = palette[0];
        Self {
            sprite: Sprite::new(width, height),
            cursor: (0, 0),
            mode: Mode::Normal,
            tool: Tool::Pencil,
            brush_size: 1,
            pixel_perfect: false,
            pp: Vec::new(),
            fg,
            bg,
            palette,
            fg_idx,
            bg_idx: 0,
            active_layer: 0,
            active_frame: 0,
            active_tag: None,
            symmetry: Symmetry::None,
            sym_axes: 6,
            sym_center: None,
            lasso_points: Vec::new(),
            selection_op: SelectionOp::Replace,
            filled_shapes: false,
            gradient_radial: false,
            spray_seed: 0x9E37_79B9_7F4A_7C15,
            wand_tolerance: 0,
            wand_all_layers: false,
            contiguous: true,
            tile_mode: false,
            grid: 8,
            snap: false,
            text_scale: 1,
            text: String::new(),
            play_loop: true,
            play_pingpong: false,
            play_dir: 1,
            play_passes: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            stroke: None,
            pending: None,
            select_anchor: None,
            selection: None,
            clipboard: None,
            paste_pos: None,
            cmdline: String::new(),
            msg: "press ? for help".into(),
            dirty: false,
            revision: 0,
            should_quit: false,
            g_prefix: false,
            path: None,
            recent: Vec::new(),
            bundle_stage: None,
            palette_lock: false,
            renderer: "glow".into(),
        }
    }

    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        let w = (self.sprite.width as i32).max(1);
        let h = (self.sprite.height as i32).max(1);
        let step = if self.snap && self.grid > 1 {
            self.grid as i32
        } else {
            1
        };
        let (dx, dy) = (dx * step, dy * step);
        let nx = self.cursor.0 as i32 + dx;
        let ny = self.cursor.1 as i32 + dy;
        let (nx, ny) = if self.tile_mode {
            (nx.rem_euclid(w), ny.rem_euclid(h))
        } else {
            (nx.clamp(0, w - 1), ny.clamp(0, h - 1))
        };
        self.cursor = (nx as u32, ny as u32);
    }

    /// Toggle (or set) seamless-tile preview + wrapping cursor.
    pub fn set_tile_mode(&mut self, on: Option<bool>) {
        self.tile_mode = on.unwrap_or(!self.tile_mode);
        self.msg = if self.tile_mode {
            "tile mode: on (cursor wraps at the edges)".into()
        } else {
            "tile mode: off".into()
        };
    }

    // ---- drawing ----

    /// Bump the cache revision. Every edit calls this; the view compares it so
    /// timeline thumbnails are only rebuilt when the sprite changed.
    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn record(
        &mut self,
        layer: usize,
        frame: usize,
        x: u32,
        y: u32,
        old: Rgba,
        new: Rgba,
    ) {
        if let Some(UndoStep::Cells { cells }) = &mut self.stroke {
            cells.push((layer, frame, x, y, old, new));
        } else {
            self.undo.push(UndoStep::Cells {
                cells: vec![(layer, frame, x, y, old, new)],
            });
            self.redo.clear();
        }
        self.touch();
    }

    pub fn begin_stroke(&mut self) {
        self.stroke = Some(UndoStep::Cells { cells: Vec::new() });
        self.pp.clear();
    }

    pub fn end_stroke(&mut self) {
        self.pp.clear();
        if let Some(s) = self.stroke.take() {
            let empty = match &s {
                UndoStep::Cells { cells } => cells.is_empty(),
                UndoStep::Snapshot { .. }
                | UndoStep::Structure { .. }
                | UndoStep::LayerProps { .. } => false,
            };
            if !empty {
                self.undo.push(s);
                self.redo.clear();
            }
        }
    }

    /// Does the current selection allow a write at `(x, y)`? No selection
    /// means the whole canvas. The persistent selection is checked without
    /// cloning; a live marquee (visual mode) resolves its combined mask.
    fn selection_allows(&self, x: u32, y: u32) -> bool {
        if self.select_anchor.is_some() {
            return self.current_selection().is_none_or(|s| s.contains(x, y));
        }
        self.selection.as_ref().is_none_or(|s| s.contains(x, y))
    }

    pub fn paint_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        if x >= self.sprite.width || y >= self.sprite.height || !self.selection_allows(x, y) {
            return;
        }
        self.paint_pixel_unclipped(x, y, color);
    }

    /// Write one pixel without the selection clip. Only for operations that
    /// own their mask (paste, transform writes, contour); recorded truth
    /// (undo/redo) uses `sprite.set` directly. Everything else must go through
    /// `paint_pixel`, which is where the selection clips.
    pub(crate) fn paint_pixel_unclipped(&mut self, x: u32, y: u32, color: Rgba) {
        if x >= self.sprite.width || y >= self.sprite.height {
            return;
        }
        let (l, f) = (self.active_layer, self.active_frame);
        if !self.sprite.layer_editable(l) {
            self.msg = "layer is locked or hidden".into();
            return;
        }
        let color = if self.sprite.is_indexed() {
            self.sprite.indexed_color(color)
        } else {
            color
        };
        if self.palette_lock && !color.is_transparent() && !self.palette.contains(&color) {
            self.msg = "palette locked: that colour is not in it".into();
            return;
        }
        let old = self.sprite.get(l, f, x, y);
        if old == color {
            return;
        }
        self.sprite.set(l, f, x, y, color);
        self.dirty = true;
        self.record(l, f, x, y, old, color);
    }

    /// What the active tool does to a pixel when dragged/clicked (primary).
    pub fn paint_with_tool(&mut self, x: u32, y: u32) {
        match self.tool {
            Tool::Eraser => self.paint_freehand(x, y, Rgba::TRANSPARENT),
            Tool::Pencil => self.paint_freehand(x, y, self.fg),
            Tool::Shade => self.shade_at(x, y, true),
            Tool::Spray => self.spray_at(x, y),
            Tool::Blur => self.blur_at(x, y),
            _ => self.paint_symmetric(x, y, self.fg),
        }
    }

    /// Secondary action (right mouse button): bg color, or lighten for Shade.
    pub fn paint_secondary(&mut self, x: u32, y: u32) {
        match self.tool {
            Tool::Shade => self.shade_at(x, y, false),
            Tool::Pencil => self.paint_freehand(x, y, self.bg),
            _ => self.paint_symmetric(x, y, self.bg),
        }
    }

    /// Darken (or lighten) the existing pixel(s) under the cursor, keeping alpha.
    pub fn shade_at(&mut self, x: u32, y: u32, darken: bool) {
        for (px, py) in self.symmetry_points(x, y) {
            let (l, f) = (self.active_layer, self.active_frame);
            let old = self.sprite.get(l, f, px, py);
            if old.is_transparent() {
                continue;
            }
            self.paint_pixel(px, py, shade_color(old, darken));
        }
    }

    /// Scatter a few random dots around the cursor (airbrush).
    pub fn spray_at(&mut self, x: u32, y: u32) {
        let r = 3i32;
        for _ in 0..6 {
            let dx = self.next_rand() % (2 * r + 1) - r;
            let dy = self.next_rand() % (2 * r + 1) - r;
            let px = x as i32 + dx;
            let py = y as i32 + dy;
            if px >= 0
                && py >= 0
                && (px as u32) < self.sprite.width
                && (py as u32) < self.sprite.height
            {
                self.paint_symmetric(px as u32, py as u32, self.fg);
            }
        }
    }

    fn next_rand(&mut self) -> i32 {
        self.spray_seed = self
            .spray_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.spray_seed >> 33) as u32 & 0x7fff_ffff) as i32
    }

    /// 3x3 alpha-weighted box blur of the pixel(s) under the cursor.
    pub fn blur_at(&mut self, x: u32, y: u32) {
        for (px, py) in self.symmetry_points(x, y) {
            let (l, f) = (self.active_layer, self.active_frame);
            let (w, h) = (self.sprite.width, self.sprite.height);
            let (mut sr, mut sg, mut sb, mut sa, mut count) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for dy in -1..=1i32 {
                for dx in -1..=1i32 {
                    let nx = px as i32 + dx;
                    let ny = py as i32 + dy;
                    if nx >= 0 && ny >= 0 && (nx as u32) < w && (ny as u32) < h {
                        let c = self.sprite.get(l, f, nx as u32, ny as u32);
                        let a = c.3 as u32;
                        sr += c.0 as u32 * a;
                        sg += c.1 as u32 * a;
                        sb += c.2 as u32 * a;
                        sa += a;
                        count += 1;
                    }
                }
            }
            if sa == 0 {
                continue;
            }
            let out = Rgba(
                (sr / sa) as u8,
                (sg / sa) as u8,
                (sb / sa) as u8,
                (sa / count.max(1)) as u8,
            );
            self.paint_pixel(px, py, out);
        }
    }

    /// Draw an outline (in fg) around the selection, or around the contiguous
    /// opaque region under the cursor when there is no selection.
    pub fn contour_at(&mut self, x: u32, y: u32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let region = self.region_mask(x, y);
        let color = self.fg;
        self.begin_stroke();
        for y in 0..h {
            for x in 0..w {
                if region[(y * w + x) as usize] {
                    continue;
                }
                let neighbors = [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ];
                if neighbors
                    .iter()
                    .any(|&(nx, ny)| nx < w && ny < h && region[(ny * w + nx) as usize])
                {
                    // The ring is just outside the selection by construction.
                    self.paint_pixel_unclipped(x, y, color);
                }
            }
        }
        self.end_stroke();
    }

    /// Region used by `contour_at`: the selection, else the contiguous opaque
    /// region (composite) around `(x, y)`.
    fn region_mask(&self, x: u32, y: u32) -> Vec<bool> {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let mut mask = vec![false; (w * h) as usize];
        if let Some(sel) = self.current_selection() {
            for yy in sel.y..(sel.y + sel.h).min(h) {
                for xx in sel.x..(sel.x + sel.w).min(w) {
                    if sel.contains(xx, yy) {
                        mask[(yy * w + xx) as usize] = true;
                    }
                }
            }
            return mask;
        }
        let comp = self.sprite.composite(self.active_frame);
        if comp[(y * w + x) as usize].is_transparent() {
            return mask;
        }
        let mut seen = vec![false; (w * h) as usize];
        let mut stack = vec![(x, y)];
        seen[(y * w + x) as usize] = true;
        while let Some((cx, cy)) = stack.pop() {
            mask[(cy * w + cx) as usize] = true;
            for (nx, ny) in [
                (cx.wrapping_sub(1), cy),
                (cx + 1, cy),
                (cx, cy.wrapping_sub(1)),
                (cx, cy + 1),
            ] {
                if nx < w && ny < h {
                    let i = (ny * w + nx) as usize;
                    if !seen[i] && !comp[i].is_transparent() {
                        seen[i] = true;
                        stack.push((nx, ny));
                    }
                }
            }
        }
        mask
    }

    /// Fill the selection (or whole cel) with a gradient from `start` to the cursor.
    pub fn paint_gradient(&mut self, start: (u32, u32)) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let sel = self.current_selection();
        let (ax, ay) = (start.0 as f32 + 0.5, start.1 as f32 + 0.5);
        let (bx, by) = if self.tile_mode {
            (
                ax + Self::wrap_delta(start.0, self.cursor.0, w) as f32,
                ay + Self::wrap_delta(start.1, self.cursor.1, h) as f32,
            )
        } else {
            (self.cursor.0 as f32 + 0.5, self.cursor.1 as f32 + 0.5)
        };
        let dx = bx - ax;
        let dy = by - ay;
        let len2 = dx * dx + dy * dy;
        let len = len2.sqrt();
        let radial = self.gradient_radial;
        let (fg, bg) = (self.fg, self.bg);
        self.begin_stroke();
        for y in 0..h {
            for x in 0..w {
                if let Some(s) = &sel
                    && !s.contains(x, y)
                {
                    continue;
                }
                // In tile mode each pixel is measured from the start along the
                // shorter way round, so a gradient drawn across the seam wraps.
                let px = if self.tile_mode {
                    ax + Self::wrap_delta(start.0, x, w) as f32
                } else {
                    x as f32 + 0.5
                };
                let py = if self.tile_mode {
                    ay + Self::wrap_delta(start.1, y, h) as f32
                } else {
                    y as f32 + 0.5
                };
                let t = if radial {
                    if len < 0.001 {
                        0.0
                    } else {
                        (((px - ax).powi(2) + (py - ay).powi(2)).sqrt() / len).clamp(0.0, 1.0)
                    }
                } else if len2 < 0.001 {
                    0.0
                } else {
                    (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
                };
                self.paint_pixel(x, y, lerp_rgba(fg, bg, t));
            }
        }
        self.end_stroke();
    }

    /// Shortest signed delta from `from` to `to` on an axis of `size` pixels:
    /// tile-mode geometry takes the seam rather than crossing the canvas.
    fn wrap_delta(from: u32, to: u32, size: u32) -> i32 {
        let d = to as i32 - from as i32;
        let s = size as i32;
        if d > s / 2 {
            d - s
        } else if d < -(s / 2) {
            d + s
        } else {
            d
        }
    }

    /// Wrap a (possibly out-of-range) coordinate into the sprite.
    fn wrap_point(&self, x: i32, y: i32) -> (u32, u32) {
        (
            x.rem_euclid(self.sprite.width as i32) as u32,
            y.rem_euclid(self.sprite.height as i32) as u32,
        )
    }

    /// Paint `color` at `(x, y)` and at every mirrored symmetry point.
    pub fn paint_symmetric(&mut self, x: u32, y: u32, color: Rgba) {
        for (px, py) in self.symmetry_points(x, y) {
            self.paint_pixel(px, py, color);
        }
    }

    /// Symmetric paint without the selection clip, for paste: Aseprite's paste
    /// replaces the selection rather than being limited by it.
    fn paint_symmetric_unclipped(&mut self, x: u32, y: u32, color: Rgba) {
        for (px, py) in self.symmetry_points(x, y) {
            self.paint_pixel_unclipped(px, py, color);
        }
    }

    /// Freehand paint: joins the last point to `(x, y)` with a line, and in
    /// pixel-perfect mode drops the middle pixel of an L-shaped corner. Only
    /// the plain 1 px, no-symmetry path gets the filter; brushes and symmetry
    /// fall back to a single stamp.
    pub fn paint_freehand(&mut self, x: u32, y: u32, color: Rgba) {
        let plain = self.brush_size == 1 && self.symmetry == Symmetry::None;
        if !self.pixel_perfect || !plain {
            self.pp.clear();
            self.paint_brush_symmetric(x, y, color);
            return;
        }
        let start = self.pp.last().copied().unwrap_or((x, y));
        for (px, py) in line_points(start.0 as i32, start.1 as i32, x as i32, y as i32) {
            if self.pp.last() == Some(&(px, py)) {
                continue;
            }
            self.paint_symmetric(px, py, color);
            self.pp.push((px, py));
            let n = self.pp.len();
            if n >= 3 {
                let (a, b, c) = (self.pp[n - 3], self.pp[n - 2], self.pp[n - 1]);
                if is_corner(a, b, c) {
                    self.unpaint(b);
                    self.pp.remove(n - 2);
                }
            }
            if self.pp.len() > 4 {
                self.pp.remove(0);
            }
        }
    }

    /// Undo one pixel of the current stroke (the pixel-perfect corner). Only
    /// safe when the stroke wrote that pixel once; otherwise it is left alone.
    fn unpaint(&mut self, (x, y): (u32, u32)) {
        let (l, f) = (self.active_layer, self.active_frame);
        let same =
            |c: &(usize, usize, u32, u32, Rgba, Rgba)| c.0 == l && c.1 == f && c.2 == x && c.3 == y;
        let Some(UndoStep::Cells { cells }) = &mut self.stroke else {
            return;
        };
        if cells.iter().filter(|c| same(c)).count() != 1 {
            return;
        }
        if let Some(pos) = cells.iter().rposition(same) {
            let before = cells.remove(pos).4;
            self.sprite.set(l, f, x, y, before);
            self.touch();
        }
    }

    /// Paint a square brush of `brush_size` pixels centred on the cursor,
    /// mirrored by the active symmetry and clipped at the canvas edges.
    pub fn paint_brush_symmetric(&mut self, x: u32, y: u32, color: Rgba) {
        let n = self.brush_size.max(1) as i32;
        let lo = -(n / 2);
        let hi = n - n / 2;
        let (w, h) = (self.sprite.width as i32, self.sprite.height as i32);
        for dy in lo..hi {
            for dx in lo..hi {
                let (px, py) = (x as i32 + dx, y as i32 + dy);
                if (0..w).contains(&px) && (0..h).contains(&py) {
                    self.paint_symmetric(px as u32, py as u32, color);
                }
            }
        }
    }

    /// Stamp `text` with the 8x8 bitmap font, top-left at `(x, y)`, in the
    /// current fg, as one undoable stroke. Glyphs advance 8 px (plus one space)
    /// and scale by an integer factor, so the pixels stay on grid.
    pub fn stamp_text(&mut self, text: &str, x: u32, y: u32) {
        if text.is_empty() {
            return;
        }
        let scale = self.text_scale.clamp(1, 8) as usize;
        self.begin_stroke();
        let mut pen = x;
        let mut line = y;
        let mut any = false;
        for ch in text.chars() {
            if ch == '\n' {
                pen = x;
                line += 9 * scale as u32;
                continue;
            }
            use font8x8::UnicodeFonts;
            let glyph = font8x8::BASIC_FONTS.get(ch);
            if let Some(bits) = glyph {
                for (col, rows) in bits.iter().enumerate() {
                    for row in 0..8 {
                        if rows & (1 << row) == 0 {
                            continue;
                        }
                        for sy in 0..scale as u32 {
                            for sx in 0..scale as u32 {
                                let px = pen + col as u32 * scale as u32 + sx;
                                let py = line + row as u32 * scale as u32 + sy;
                                if px < self.sprite.width && py < self.sprite.height {
                                    self.paint_pixel(px, py, self.fg);
                                    any = true;
                                }
                            }
                        }
                    }
                }
            }
            pen += 9 * scale as u32;
            if pen >= self.sprite.width {
                break;
            }
        }
        self.end_stroke();
        self.msg = format!(
            "text: {text:?} ({} px)",
            if any { "stamped" } else { "empty" }
        );
    }

    /// Round a sprite pixel down to the design grid, when snapping is on.
    pub fn snap_to_grid(&self, x: u32, y: u32) -> (u32, u32) {
        if !self.snap || self.grid < 2 {
            return (x, y);
        }
        (x - x % self.grid, y - y % self.grid)
    }

    /// The mirror/rotation origin. `None` is the sprite centre, so the plain
    /// modes behave exactly as before.
    pub fn symmetry_center(&self) -> (f32, f32) {
        match self.sym_center {
            Some((x, y)) => (x as f32, y as f32),
            None => (
                (self.sprite.width as f32 - 1.0) * 0.5,
                (self.sprite.height as f32 - 1.0) * 0.5,
            ),
        }
    }

    fn symmetry_points(&self, x: u32, y: u32) -> Vec<(u32, u32)> {
        let (w, h) = (self.sprite.width as f32, self.sprite.height as f32);
        let (cx, cy) = self.symmetry_center();
        let (x, y) = (x as f32, y as f32);
        let mut pts: Vec<(u32, u32)> = Vec::new();
        let mut push = |px: f32, py: f32| {
            let (px, py) = (px.round(), py.round());
            if px >= 0.0 && py >= 0.0 && px < w && py < h {
                let p = (px as u32, py as u32);
                if !pts.contains(&p) {
                    pts.push(p);
                }
            }
        };
        push(x, y);
        match self.symmetry {
            Symmetry::None => {}
            Symmetry::Horizontal => push(2.0 * cx - x, y),
            Symmetry::Vertical => push(x, 2.0 * cy - y),
            Symmetry::Both => {
                push(2.0 * cx - x, y);
                push(x, 2.0 * cy - y);
                push(2.0 * cx - x, 2.0 * cy - y);
            }
            Symmetry::Radial => {
                let n = self.sym_axes.max(2) as u32;
                let (dx, dy) = (x - cx, y - cy);
                for k in 1..n {
                    let a = k as f32 * std::f32::consts::TAU / n as f32;
                    let (s, c) = a.sin_cos();
                    push(cx + dx * c - dy * s, cy + dx * s + dy * c);
                }
            }
        }
        pts
    }

    pub fn cycle_symmetry(&mut self) {
        self.symmetry = match self.symmetry {
            Symmetry::None => Symmetry::Horizontal,
            Symmetry::Horizontal => Symmetry::Vertical,
            Symmetry::Vertical => Symmetry::Both,
            Symmetry::Both => Symmetry::Radial,
            Symmetry::Radial => Symmetry::None,
        };
    }

    pub fn set_symmetry(&mut self, name: &str) -> bool {
        self.symmetry = match name {
            "none" | "off" => Symmetry::None,
            "horizontal" | "h" | "x" => Symmetry::Horizontal,
            "vertical" | "v" | "y" => Symmetry::Vertical,
            "both" | "b" => Symmetry::Both,
            "radial" | "r" => Symmetry::Radial,
            _ => return false,
        };
        true
    }

    pub fn apply_bucket(&mut self, x: u32, y: u32) {
        let (l, f) = (self.active_layer, self.active_frame);
        if !self.sprite.layer_editable(l) {
            self.msg = "layer is locked or hidden".into();
            return;
        }
        let color = self.fg;
        let clip = self.has_selection().then(|| self.selection_grid());
        let opts = FillOptions {
            tol: self.wand_tolerance,
            contiguous: self.contiguous,
            clip: clip.as_deref(),
        };
        self.begin_stroke();
        let mut n = 0;
        // Fill the seed and every mirrored seed: a region on the other side of
        // the axis is a different flood, not a mirrored copy of the first.
        for (sx, sy) in self.symmetry_points(x, y) {
            let changes = flood_fill_tol(&mut self.sprite, l, f, sx, sy, color, opts);
            n += changes.len();
            if let Some(UndoStep::Cells { cells }) = &mut self.stroke {
                cells.extend(
                    changes
                        .into_iter()
                        .map(|(x, y, old, new)| (l, f, x, y, old, new)),
                );
            }
        }
        if n > 0 {
            self.dirty = true;
        }
        self.end_stroke();
    }

    pub fn commit_shape(&mut self, color: Rgba) {
        let Some(p) = self.pending.take() else {
            return;
        };
        if p.shape == Shape::Gradient {
            self.paint_gradient(p.start);
            return;
        }
        let filled = self.filled_shapes;
        let (w, h) = (self.sprite.width as i32, self.sprite.height as i32);
        let (sx, sy) = (p.start.0 as i32, p.start.1 as i32);
        let (mut ex, mut ey) = (self.cursor.0 as i32, self.cursor.1 as i32);
        if self.tile_mode {
            ex = sx + Self::wrap_delta(p.start.0, self.cursor.0, w as u32);
            ey = sy + Self::wrap_delta(p.start.1, self.cursor.1, h as u32);
        }
        // The helpers cast to u32, so shift both corners clear of zero when
        // the seam took the end (or start) negative; the modulo is unchanged.
        let (sx, sy, ex, ey) = if sx < 0 || sy < 0 || ex < 0 || ey < 0 || ex >= w || ey >= h {
            (sx + w, sy + h, ex + w, ey + h)
        } else {
            (sx, sy, ex, ey)
        };
        let pts = match p.shape {
            Shape::Line => line_points(sx, sy, ex, ey),
            Shape::Rect if filled => rect_filled_points(sx, sy, ex, ey),
            Shape::Rect => rect_points(sx, sy, ex, ey),
            Shape::Ellipse => ellipse_points(sx, sy, ex, ey, filled),
            Shape::Gradient => unreachable!(),
        };
        self.begin_stroke();
        for (x, y) in pts {
            let (x, y) = if self.tile_mode {
                self.wrap_point(x as i32, y as i32)
            } else {
                (x, y)
            };
            self.paint_symmetric(x, y, color);
        }
        self.end_stroke();
    }

    pub fn pick_color(&mut self, x: u32, y: u32) {
        let comp = self.sprite.composite(self.active_frame);
        let c = comp[(y * self.sprite.width + x) as usize];
        if !c.is_transparent() {
            self.fg = c;
            if let Some(i) = self.palette.iter().position(|&p| p == c) {
                self.fg_idx = i;
            }
        }
    }

    // ---- undo/redo ----

    pub fn undo(&mut self) {
        if let Some(step) = self.undo.pop() {
            match &step {
                UndoStep::Cells { cells } => {
                    // Reverse order: one stroke can write a pixel twice (drag
                    // back over it), and only the first `before` is the truth.
                    for &(l, f, x, y, before, _) in cells.iter().rev() {
                        self.sprite.set(l, f, x, y, before);
                    }
                }
                UndoStep::Snapshot { before, .. } => {
                    self.sprite = (**before).clone();
                    self.clamp_cursor();
                }
                UndoStep::Structure {
                    before, before_at, ..
                } => {
                    self.sprite = (**before).clone();
                    self.restore_point(*before_at);
                }
                UndoStep::LayerProps { layer, before, .. } => {
                    self.apply_layer_props(*layer, *before);
                }
            }
            self.redo.push(step);
            self.dirty = true;
            self.touch();
        }
    }

    pub fn redo(&mut self) {
        if let Some(step) = self.redo.pop() {
            match &step {
                UndoStep::Cells { cells } => {
                    for &(l, f, x, y, _, after) in cells {
                        self.sprite.set(l, f, x, y, after);
                    }
                }
                UndoStep::Snapshot { after, .. } => {
                    self.sprite = (**after).clone();
                    self.clamp_cursor();
                }
                UndoStep::Structure {
                    after, after_at, ..
                } => {
                    self.sprite = (**after).clone();
                    self.restore_point(*after_at);
                }
                UndoStep::LayerProps { layer, after, .. } => {
                    self.apply_layer_props(*layer, *after);
                }
            }
            self.undo.push(step);
            self.dirty = true;
            self.touch();
        }
    }

    fn clamp_cursor(&mut self) {
        self.cursor = (
            self.cursor.0.min(self.sprite.width - 1),
            self.cursor.1.min(self.sprite.height - 1),
        );
    }

    // ---- color / layer / frame ----

    pub fn cycle_color(&mut self, dir: i32) {
        let n = self.palette.len() as i32;
        let mut i = self.fg_idx as i32 + dir;
        if i < 0 {
            i = n - 1;
        } else if i >= n {
            i = 0;
        }
        self.fg_idx = i as usize;
        self.fg = self.palette[self.fg_idx];
    }

    pub fn swap_colors(&mut self) {
        std::mem::swap(&mut self.fg, &mut self.bg);
        std::mem::swap(&mut self.fg_idx, &mut self.bg_idx);
    }

    // ---- palette ----

    pub fn load_palette(&mut self, mut colors: Vec<Rgba>) {
        if colors.is_empty() {
            return;
        }
        if self.sprite.is_indexed() {
            if !colors.iter().any(|c| c.3 == 0) {
                colors.insert(0, Rgba::TRANSPARENT);
            }
            colors.truncate(256);
            self.sprite.palette = colors.clone();
            let ti = self.sprite.transparent_slot();
            self.sprite.transparent_index = ti;
            self.sprite.remap_to_palette();
            self.dirty = true;
            self.touch();
        }
        self.palette = colors;
        self.fg_idx = self.fg_idx.min(self.palette.len() - 1);
        self.bg_idx = self.bg_idx.min(self.palette.len() - 1);
        self.fg = self.palette[self.fg_idx];
        self.bg = self.palette[self.bg_idx];
    }

    /// Replace the palette with the exact distinct opaque colours of the active
    /// frame, most used first. A fix painted on the generator's own ramp cannot
    /// drift off it.
    pub fn palette_from_sprite(&mut self) -> usize {
        let comp = self.sprite.composite(self.active_frame);
        let mut counts: std::collections::HashMap<Rgba, usize> = std::collections::HashMap::new();
        for c in comp {
            if !c.is_transparent() {
                *counts.entry(c).or_default() += 1;
            }
        }
        let mut list: Vec<(Rgba, usize)> = counts.into_iter().collect();
        list.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(a.0.0.cmp(&b.0.0))
                .then(a.0.1.cmp(&b.0.1))
        });
        let n = list.len();
        self.load_palette(list.into_iter().map(|(c, _)| c).collect());
        self.msg = format!("palette: {n} colours from the frame");
        n
    }

    /// Turn the lock on/off, reporting what it means.
    pub fn set_palette_lock(&mut self, on: bool) {
        self.palette_lock = on;
        self.msg = if on {
            "palette locked: only these colours (and eraser)".into()
        } else {
            "palette unlocked".into()
        };
    }

    pub fn add_palette_color(&mut self) {
        if !self.palette.contains(&self.fg) {
            self.palette.push(self.fg);
            self.fg_idx = self.palette.len() - 1;
        }
    }

    pub fn remove_palette_color(&mut self) {
        if self.palette.len() > 1 {
            self.palette.remove(self.fg_idx);
            if self.fg_idx >= self.palette.len() {
                self.fg_idx = self.palette.len() - 1;
            }
            self.bg_idx = self.bg_idx.min(self.palette.len() - 1);
            self.fg = self.palette[self.fg_idx];
            self.bg = self.palette[self.bg_idx];
        }
    }

    /// Move a palette entry, keeping fg/bg pointing at the same colour.
    pub fn move_palette(&mut self, from: usize, to: usize) {
        if from >= self.palette.len() || to >= self.palette.len() || from == to {
            return;
        }
        let c = self.palette.remove(from);
        self.palette.insert(to, c);
        let track = |idx: usize| {
            if idx == from {
                to
            } else if from < idx && idx <= to {
                idx - 1
            } else if to <= idx && idx < from {
                idx + 1
            } else {
                idx
            }
        };
        self.fg_idx = track(self.fg_idx);
        self.bg_idx = track(self.bg_idx);
        self.msg = format!("palette {from} -> {to}");
    }

    /// Set fg/bg from `#rrggbb`, as the picker's hex field and `:fg` do.
    pub fn set_fg(&mut self, colour: Rgba) {
        self.fg = colour;
        if let Some(i) = self.palette.iter().position(|c| *c == colour) {
            self.fg_idx = i;
        }
    }

    pub fn set_bg(&mut self, colour: Rgba) {
        self.bg = colour;
        if let Some(i) = self.palette.iter().position(|c| *c == colour) {
            self.bg_idx = i;
        }
    }

    pub fn cycle_layer(&mut self, dir: i32) {
        let n = self.sprite.layers.len() as i32;
        let mut i = self.active_layer as i32 + dir;
        if i < 0 {
            i = n - 1;
        } else if i >= n {
            i = 0;
        }
        self.active_layer = i as usize;
    }

    pub fn cycle_frame(&mut self, dir: i32) {
        let n = self.sprite.frames.len() as i32;
        let mut i = self.active_frame as i32 + dir;
        if i < 0 {
            i = n - 1;
        } else if i >= n {
            i = 0;
        }
        self.active_frame = i as usize;
    }

    pub fn add_layer(&mut self) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        let idx = self.active_layer;
        self.sprite.add_layer_after(idx);
        self.active_layer = idx + 1;
        self.record_structure(before, at);
    }

    pub fn del_layer(&mut self) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        if self.sprite.remove_layer(self.active_layer) {
            if self.active_layer >= self.sprite.layers.len() {
                self.active_layer = self.sprite.layers.len() - 1;
            }
            self.record_structure(before, at);
        } else {
            self.msg = "cannot delete last layer".into();
        }
    }

    pub fn add_frame(&mut self) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        let idx = self.active_frame;
        self.sprite.add_frame_after(idx);
        self.active_frame = idx + 1;
        self.record_structure(before, at);
    }

    pub fn del_frame(&mut self) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        if self.sprite.remove_frame(self.active_frame) {
            if self.active_frame >= self.sprite.frames.len() {
                self.active_frame = self.sprite.frames.len() - 1;
            }
            if self.active_tag.is_some_and(|i| i >= self.sprite.tags.len()) {
                self.active_tag = None;
            }
            self.record_structure(before, at);
        } else {
            self.msg = "cannot delete last frame".into();
        }
    }

    /// Move a frame (drag-and-drop in the timeline).
    pub fn move_frame(&mut self, from: usize, to: usize) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        if self.sprite.move_frame(from, to) {
            if self.active_frame == from {
                self.active_frame = to;
            }
            self.record_structure(before, at);
        }
    }

    /// Create or update a tag over the inclusive frame range.
    pub fn set_tag(&mut self, name: &str, from: Option<usize>, to: Option<usize>) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        let last = self.sprite.frames.len().saturating_sub(1);
        let a = from.unwrap_or(self.active_frame).min(last);
        let b = to.unwrap_or(a).min(last);
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let idx = match self.sprite.tags.iter().position(|t| t.name == name) {
            Some(i) => {
                self.sprite.tags[i].from = a;
                self.sprite.tags[i].to = b;
                i
            }
            None => {
                self.sprite.tags.push(crate::sprite::Tag {
                    name: name.to_string(),
                    from: a,
                    to: b,
                    color: Rgba(120, 170, 255, 255),
                    direction: crate::sprite::PlayDirection::Forward,
                    repeat: 0,
                });
                self.sprite.tags.len() - 1
            }
        };
        self.active_tag = Some(idx);
        self.msg = format!("tag '{name}': frames {a}-{b}");
        self.record_structure(before, at);
    }

    /// Delete a tag by name, or the active tag when no name is given.
    pub fn del_tag(&mut self, name: Option<&str>) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        let idx = match name {
            Some(n) => self.sprite.tags.iter().position(|t| t.name == n),
            None => self.active_tag,
        };
        if let Some(i) = idx {
            self.sprite.tags.remove(i);
            self.active_tag = None;
            self.record_structure(before, at);
        } else {
            self.msg = "no such tag".into();
        }
    }

    /// Inclusive frame range for playback: the active tag, else everything.
    pub fn play_range(&self) -> (usize, usize) {
        let last = self.sprite.frames.len().saturating_sub(1);
        match self.active_tag.and_then(|i| self.sprite.tags.get(i)) {
            Some(t) => (t.from.min(last), t.to.min(last)),
            None => (0, last),
        }
    }

    /// Reset the pass counter when playback starts (or the tag changes).
    pub fn start_playback(&mut self) {
        self.play_passes = 0;
        self.play_dir = 1;
    }

    /// Step to the next playback frame inside the active tag. Returns whether
    /// playback should continue. A tag carries its own direction and repeat
    /// count; without one, the global loop/ping-pong flags apply.
    pub fn advance_frame(&mut self) -> bool {
        let (a, b) = self.play_range();
        if b <= a {
            return false;
        }
        let tag = self.active_tag.and_then(|i| self.sprite.tags.get(i));
        let direction = match tag {
            Some(t) => t.direction,
            None if self.play_pingpong => crate::sprite::PlayDirection::PingPong,
            None => crate::sprite::PlayDirection::Forward,
        };
        let repeat = tag.map_or(0, |t| t.repeat);
        use crate::sprite::PlayDirection;
        match direction {
            PlayDirection::PingPong => {
                // `b` is the last frame, so the turn happens when the step
                // would leave the range, and the endpoint is shown once.
                let mut f = self.active_frame as i32 + self.play_dir;
                let mut full_cycle = false;
                if f > b as i32 {
                    self.play_dir = -1;
                    f = b as i32 - 1;
                } else if f < a as i32 {
                    self.play_dir = 1;
                    f = a as i32 + 1;
                    full_cycle = true;
                }
                self.active_frame = f.clamp(a as i32, b as i32) as usize;
                if full_cycle && !self.pass_complete(repeat) {
                    self.active_frame = a;
                    return false;
                }
                true
            }
            PlayDirection::Reverse => {
                if self.active_frame <= a || self.active_frame > b {
                    if !self.pass_complete(repeat) {
                        return false;
                    }
                    self.active_frame = b;
                } else {
                    self.active_frame -= 1;
                }
                true
            }
            PlayDirection::Forward => {
                let looping = tag.is_some() || self.play_loop;
                if self.active_frame >= b || self.active_frame < a {
                    if !looping || !self.pass_complete(repeat) {
                        return false;
                    }
                    self.active_frame = a;
                } else {
                    self.active_frame += 1;
                }
                true
            }
        }
    }

    /// Count a completed pass; returns whether playback may continue. `repeat`
    /// of 0 means forever.
    fn pass_complete(&mut self, repeat: u16) -> bool {
        if repeat == 0 {
            return true;
        }
        self.play_passes += 1;
        self.play_passes < repeat as u32
    }

    /// Direction of the active tag's playback.
    pub fn set_tag_direction(&mut self, direction: crate::sprite::PlayDirection) {
        let Some(i) = self.active_tag else {
            self.msg = "no active tag (click one or :tag <name>)".into();
            return;
        };
        if self.sprite.tags[i].direction == direction {
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        self.sprite.tags[i].direction = direction;
        self.msg = format!("tag '{}': {}", self.sprite.tags[i].name, direction.name());
        self.record_structure(before, at);
    }

    /// Repeat count of the active tag (0 = forever).
    pub fn set_tag_repeat(&mut self, repeat: u16) {
        let Some(i) = self.active_tag else {
            self.msg = "no active tag (click one or :tag <name>)".into();
            return;
        };
        if self.sprite.tags[i].repeat == repeat {
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        self.sprite.tags[i].repeat = repeat;
        self.msg = format!(
            "tag '{}': {}",
            self.sprite.tags[i].name,
            if repeat == 0 {
                "forever".to_string()
            } else {
                format!("{repeat}×")
            }
        );
        self.record_structure(before, at);
    }

    /// Crop the canvas to a rectangle (clamped), as one undo step.
    pub fn crop_to(&mut self, x: u32, y: u32, w: u32, h: u32) {
        let (sw, sh) = (self.sprite.width, self.sprite.height);
        let x = x.min(sw.saturating_sub(1));
        let y = y.min(sh.saturating_sub(1));
        let w = w.clamp(1, sw - x);
        let h = h.clamp(1, sh - y);
        if (x, y, w, h) == (0, 0, sw, sh) {
            self.msg = "crop: no change".into();
            return;
        }
        let before = self.sprite.clone();
        self.sprite.crop(x, y, w, h);
        self.cursor = (
            self.cursor.0.saturating_sub(x).min(w - 1),
            self.cursor.1.saturating_sub(y).min(h - 1),
        );
        self.selection = None;
        self.select_anchor = None;
        self.pending = None;
        self.paste_pos = None;
        self.snapshot(before);
        self.msg = format!("cropped to {w}x{h}");
    }

    /// Crop to the current selection's bounding box.
    pub fn crop_to_selection(&mut self) {
        match self.current_selection() {
            Some(sel) => self.crop_to(sel.x, sel.y, sel.w, sel.h),
            None => self.msg = "no selection to crop to".into(),
        }
    }

    /// Crop to the non-transparent content of every visible frame.
    pub fn trim(&mut self) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let (mut minx, mut miny, mut maxx, mut maxy) = (w, h, 0, 0);
        let mut any = false;
        for f in 0..self.sprite.frames.len() {
            let comp = self.sprite.composite(f);
            for y in 0..h {
                for x in 0..w {
                    if !comp[(y * w + x) as usize].is_transparent() {
                        any = true;
                        minx = minx.min(x);
                        miny = miny.min(y);
                        maxx = maxx.max(x);
                        maxy = maxy.max(y);
                    }
                }
            }
        }
        if !any {
            self.msg = "nothing to trim".into();
            return;
        }
        self.crop_to(minx, miny, maxx - minx + 1, maxy - miny + 1);
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.sprite.resize(w, h);
        self.cursor = (self.cursor.0.min(w - 1), self.cursor.1.min(h - 1));
        self.dirty = true;
        self.touch();
    }

    // ---- selection / clipboard ----

    pub fn start_select(&mut self) {
        self.select_anchor = Some(self.cursor);
        self.pending = None;
        self.mode = Mode::Visual;
    }

    /// Persistent selection of the whole canvas.
    pub fn select_all(&mut self) {
        self.selection = Some(Selection::rect(
            0,
            0,
            self.sprite.width as i32 - 1,
            self.sprite.height as i32 - 1,
        ));
        self.select_anchor = None;
    }

    pub fn has_selection(&self) -> bool {
        self.select_anchor.is_some() || self.selection.is_some()
    }

    /// Convert the sprite between sRGB and linear light. The palette, fg and bg
    /// convert with it: painting an sRGB palette colour into a linear sprite
    /// would mix spaces.
    pub fn set_color_profile(&mut self, to: crate::sprite::ColorProfile) {
        use crate::sprite::ColorProfile;
        if to == self.sprite.color_profile {
            self.msg = format!("profile: {}", to.name());
            return;
        }
        let before = self.sprite.clone();
        self.sprite.convert_profile(to);
        let after = self.sprite.clone();
        self.undo.push(UndoStep::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        });
        self.redo.clear();
        let f = match to {
            ColorProfile::Linear => crate::color::srgb_to_linear,
            ColorProfile::Srgb => crate::color::linear_to_srgb,
        };
        for c in self.palette.iter_mut() {
            *c = Rgba(f(c.0), f(c.1), f(c.2), c.3);
        }
        self.fg = Rgba(f(self.fg.0), f(self.fg.1), f(self.fg.2), self.fg.3);
        self.bg = Rgba(f(self.bg.0), f(self.bg.1), f(self.bg.2), self.bg.3);
        self.dirty = true;
        self.touch();
        self.msg = format!("profile: {}", to.name());
    }

    // ---- sprite properties ----

    /// Set the declared colour mode. `Grayscale` converts the pixels and
    /// `Indexed` quantizes to a document palette; both are one undo step.
    /// Indexed painting snaps to the nearest palette entry.
    pub fn set_color_mode(&mut self, mode: crate::sprite::ColorMode) {
        use crate::sprite::ColorMode;
        let needs_palette = mode == ColorMode::Indexed && self.sprite.palette.is_empty();
        if mode == self.sprite.color_mode && !needs_palette {
            self.msg = format!("mode: {}", mode.name());
            return;
        }
        match mode {
            ColorMode::Grayscale => {
                let before = self.sprite.clone();
                self.sprite.to_grayscale();
                let after = self.sprite.clone();
                self.undo.push(UndoStep::Snapshot {
                    before: Box::new(before),
                    after: Box::new(after),
                });
                self.redo.clear();
                self.palette_lock = true;
            }
            ColorMode::Rgba | ColorMode::Auto => {
                self.sprite.color_mode = mode;
                self.palette_lock = false;
            }
            ColorMode::Indexed => {
                let before = self.sprite.clone();
                self.sprite.quantize_to_palette();
                let after = self.sprite.clone();
                self.undo.push(UndoStep::Snapshot {
                    before: Box::new(before),
                    after: Box::new(after),
                });
                self.redo.clear();
                self.palette_lock = false;
                self.load_palette(self.sprite.palette.clone());
            }
        }
        self.dirty = true;
        self.touch();
        self.msg = format!("mode: {}", mode.name());
    }

    pub fn set_pixel_aspect(&mut self, x: u16, y: u16) {
        self.sprite.pixel_aspect = (x.max(1), y.max(1));
        self.dirty = true;
        self.msg = format!(
            "pixel aspect: {}:{}",
            self.sprite.pixel_aspect.0, self.sprite.pixel_aspect.1
        );
    }

    /// The sprite's own grid bounds, which also drive the design grid.
    pub fn set_sprite_grid(&mut self, w: u32, h: u32) {
        self.sprite.grid = (w, h);
        if w > 0 {
            self.grid = w;
        }
        self.dirty = true;
        self.msg = format!("sprite grid: {}x{}", w, h);
    }

    // ---- free transform ----

    /// Rotate, scale and skew the selection (or the whole cel) about its own
    /// centre, nearest-neighbour, clipped to the sprite. One undo step. All
    /// three are one affine map, so they compose in the order rotate, scale,
    /// skew; the inverse map is what samples the source.
    pub fn free_transform(&mut self, angle_deg: f32, sx: f32, sy: f32, skew_deg: f32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let (l, f) = (self.active_layer, self.active_frame);
        let sel = self.current_selection();
        let (rx, ry, rw, rh) = match &sel {
            Some(s) => (s.x, s.y, s.w, s.h),
            None => (0, 0, w, h),
        };
        let (sx, sy) = (sx.abs().max(0.01), sy.abs().max(0.01));
        let (cx, cy) = (rx as f32 + rw as f32 / 2.0, ry as f32 + rh as f32 / 2.0);
        let (sin, cos) = (-angle_deg).to_radians().sin_cos();
        let tan = (-skew_deg).to_radians().tan();
        let src: Vec<Rgba> = (0..w * h)
            .map(|i| self.sprite.get(l, f, i % w, i / w))
            .collect();
        let inside = |x: u32, y: u32| match &sel {
            Some(s) => s.contains(x, y),
            None => true,
        };
        self.begin_stroke();
        // Vacate the region first: a rotated shape must not leave the old one.
        for y in ry..ry + rh {
            for x in rx..rx + rw {
                if inside(x, y) && y < h && x < w {
                    self.paint_pixel(x, y, Rgba::TRANSPARENT);
                }
            }
        }
        for y in ry..ry + rh {
            for x in rx..rx + rw {
                if !inside(x, y) || x >= w || y >= h {
                    continue;
                }
                let (mut dx, mut dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                dx -= tan * dy;
                dx /= sx;
                dy /= sy;
                let (sxf, syf) = (dx * cos - dy * sin, dx * sin + dy * cos);
                let (px, py) = ((cx + sxf).floor(), (cy + syf).floor());
                if px < 0.0 || py < 0.0 || px >= w as f32 || py >= h as f32 {
                    continue;
                }
                let (px, py) = (px as u32, py as u32);
                let c = src[(py * w + px) as usize];
                if c.3 > 0 && inside(px, py) {
                    self.paint_pixel(x, y, c);
                }
            }
        }
        self.end_stroke();
        self.dirty = true;
        self.msg = format!(
            "transform: rotate {angle_deg:.0} deg, scale {sx:.2}x{sy:.2}, skew {skew_deg:.0} deg"
        );
    }

    // ---- filters ----

    /// One filter over the selection (or the whole cel when there is none), as
    /// a single undo step. Transparent pixels stay transparent: a filter is for
    /// the art, not for the empty space around it. `Outline` is the exception,
    /// because its whole job is to grow into that space.
    pub fn filter(&mut self, f: Filter) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let (l, frame) = (self.active_layer, self.active_frame);
        let src: Vec<Rgba> = (0..w * h)
            .map(|i| self.sprite.get(l, frame, i % w, i / w))
            .collect();
        let dst = apply_filter(&src, w, h, f, self.fg);
        let sel = self.current_selection();
        self.begin_stroke();
        let mut changed = 0;
        for i in 0..src.len() {
            if dst[i] == src[i] {
                continue;
            }
            let (x, y) = (i as u32 % w, i as u32 / w);
            if let Some(s) = &sel
                && !s.contains(x, y)
            {
                continue;
            }
            self.paint_pixel(x, y, dst[i]);
            changed += 1;
        }
        self.end_stroke();
        self.msg = format!("filter: {} ({changed} px)", f.name());
    }

    // ---- selection shape ----

    /// The selection as a sprite-sized grid. Shape operations work here rather
    /// than inside the bbox, because every one of them can grow past it.
    pub(crate) fn selection_grid(&self) -> Vec<bool> {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let mut g = vec![false; (w * h) as usize];
        if let Some(sel) = self.current_selection() {
            for y in 0..sel.h {
                for x in 0..sel.w {
                    let (sx, sy) = (sel.x + x, sel.y + y);
                    if sel.selected(x, y) && sx < w && sy < h {
                        g[(sy * w + sx) as usize] = true;
                    }
                }
            }
        }
        g
    }

    /// Store a sprite-sized grid back as the tightest bbox selection.
    fn grid_to_selection(&mut self, g: &[bool]) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if g[(y * w + x) as usize] {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x0 == u32::MAX {
            self.selection = None;
            self.select_anchor = None;
            return;
        }
        let (sw, sh) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut mask = vec![false; (sw * sh) as usize];
        for y in 0..sh {
            for x in 0..sw {
                mask[(y * sw + x) as usize] = g[((y0 + y) * w + x0 + x) as usize];
            }
        }
        self.selection = Some(Selection {
            x: x0,
            y: y0,
            w: sw,
            h: sh,
            mask,
        });
        self.select_anchor = None;
    }

    /// Grow the selection by `n` pixels on every side (4-neighbour).
    pub fn selection_expand(&mut self, n: u32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let mut g = self.selection_grid();
        for _ in 0..n.min(64) {
            g = dilate(&g, w, h);
        }
        self.grid_to_selection(&g);
        self.msg = format!("selection: expand {n}");
    }

    /// Shrink the selection by `n` pixels. A selection thinner than `2n` goes.
    pub fn selection_contract(&mut self, n: u32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let mut g = self.selection_grid();
        for _ in 0..n.min(64) {
            g = erode(&g, w, h);
        }
        self.grid_to_selection(&g);
        self.msg = format!("selection: contract {n}");
    }

    /// Keep only the `n`-pixel ring along the selection's edge.
    pub fn selection_border(&mut self, n: u32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let n = n.clamp(1, 64);
        let mut outer = self.selection_grid();
        let mut inner = outer.clone();
        for _ in 0..n {
            outer = dilate(&outer, w, h);
            inner = erode(&inner, w, h);
        }
        let ring: Vec<bool> = outer
            .iter()
            .zip(inner.iter())
            .map(|(o, i)| *o && !i)
            .collect();
        self.grid_to_selection(&ring);
        self.msg = format!("selection: border {n}");
    }

    /// Soften the edge. A selection mask is one bit per pixel, so the ramp is a
    /// dither: a pixel `d` steps inside the edge survives with probability
    /// `d / n`. Deterministic, so undoing and redoing lands on the same pixels.
    pub fn selection_feather(&mut self, n: u32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let n = n.clamp(1, 32);
        // depth[i] is how many erosions the pixel survives, capped at n.
        let mut depth = vec![0u32; (w * h) as usize];
        let mut alive = self.selection_grid();
        for step in 1..=n {
            let next = erode(&alive, w, h);
            for i in 0..depth.len() {
                if alive[i] && !next[i] {
                    depth[i] = step;
                }
            }
            alive = next;
        }
        let src = self.selection_grid();
        let mut out = src.clone();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if !src[i] {
                    continue;
                }
                let d = depth[i];
                if d > 0 && d < n {
                    let hsh = (x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263)) % n;
                    if hsh >= d {
                        out[i] = false;
                    }
                }
            }
        }
        self.grid_to_selection(&out);
        self.msg = format!("selection: feather {n}");
    }

    /// Everything the selection is not, inside the sprite.
    pub fn selection_invert(&mut self) {
        let g = self.selection_grid();
        let inverted: Vec<bool> = g.iter().map(|v| !v).collect();
        self.grid_to_selection(&inverted);
        self.msg = "selection: invert".into();
    }

    pub fn selection_none(&mut self) {
        self.selection = None;
        self.select_anchor = None;
        self.msg = "selection: none".into();
    }

    /// The selection to operate on: the live marquee while in visual mode, else
    /// the persistent selection (magic wand, etc.).
    pub fn current_selection(&self) -> Option<Selection> {
        if let Some((ax, ay)) = self.select_anchor {
            let marquee = Selection::rect(
                ax as i32,
                ay as i32,
                self.cursor.0 as i32,
                self.cursor.1 as i32,
            );
            self.combined_selection(marquee)
        } else {
            self.selection.clone()
        }
    }

    /// Combine `new_sel` with the persistent selection according to `selection_op`.
    fn combined_selection(&self, new_sel: Selection) -> Option<Selection> {
        let combined = match (&self.selection, self.selection_op) {
            (Some(existing), SelectionOp::Add) => {
                combine_masks(existing, &new_sel, SelectionOp::Add)
            }
            (Some(existing), SelectionOp::Subtract) => {
                combine_masks(existing, &new_sel, SelectionOp::Subtract)
            }
            (Some(existing), SelectionOp::Intersect) => {
                combine_masks(existing, &new_sel, SelectionOp::Intersect)
            }
            _ => new_sel,
        };
        if combined.mask.iter().any(|&v| v) {
            Some(combined)
        } else {
            None
        }
    }

    /// Commit a selection created by a wand / lasso / polygon.
    fn apply_selection(&mut self, new_sel: Selection) {
        let combined = self.combined_selection(new_sel);
        self.selection = combined;
        self.select_anchor = None;
    }

    /// Commit the live marquee selection and leave visual mode.
    pub fn commit_marquee(&mut self) {
        let combined = self.current_selection();
        self.selection = combined;
        self.select_anchor = None;
        self.mode = Mode::Normal;
    }

    pub fn clear_selection(&mut self) {
        self.select_anchor = None;
        self.selection = None;
    }

    pub fn cycle_selection_op(&mut self) {
        self.selection_op = self.selection_op.next();
    }

    pub fn set_selection_op(&mut self, name: &str) -> bool {
        match SelectionOp::parse(name) {
            Some(op) => {
                self.selection_op = op;
                true
            }
            None => false,
        }
    }

    pub fn toggle_filled_shapes(&mut self) {
        self.filled_shapes = !self.filled_shapes;
    }

    /// Select the region around `(x, y)` matching within `wand_tolerance`:
    /// contiguous, or every matching pixel when `contiguous` is off.
    pub fn magic_wand(&mut self, x: u32, y: u32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let source: Vec<Rgba> = if self.wand_all_layers {
            self.sprite.composite(self.active_frame)
        } else {
            self.sprite.cels[self.active_layer][self.active_frame]
                .pixels
                .clone()
        };
        let target = source[(y * w + x) as usize];
        let tol = self.wand_tolerance;
        let mut mask = vec![false; (w * h) as usize];
        if self.contiguous {
            let mut seen = vec![false; (w * h) as usize];
            let mut stack = vec![(x, y)];
            seen[(y * w + x) as usize] = true;
            while let Some((cx, cy)) = stack.pop() {
                mask[(cy * w + cx) as usize] = true;
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx < w && ny < h {
                        let i = (ny * w + nx) as usize;
                        if !seen[i] && color_close(source[i], target, tol) {
                            seen[i] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
        } else {
            for (i, &c) in source.iter().enumerate() {
                if color_close(c, target, tol) {
                    mask[i] = true;
                }
            }
        }
        // Crop to the bounding box of the selected pixels.
        let (mut minx, mut miny) = (u32::MAX, u32::MAX);
        let (mut maxx, mut maxy) = (0, 0);
        for (i, &m) in mask.iter().enumerate() {
            if m {
                let (px, py) = (i as u32 % w, i as u32 / w);
                minx = minx.min(px);
                miny = miny.min(py);
                maxx = maxx.max(px);
                maxy = maxy.max(py);
            }
        }
        let sw = maxx - minx + 1;
        let sh = maxy - miny + 1;
        let mut cropped = vec![false; (sw * sh) as usize];
        for yy in miny..=maxy {
            for xx in minx..=maxx {
                cropped[((yy - miny) * sw + (xx - minx)) as usize] = mask[(yy * w + xx) as usize];
            }
        }
        self.apply_selection(Selection {
            x: minx,
            y: miny,
            w: sw,
            h: sh,
            mask: cropped,
        });
    }

    pub fn has_lasso(&self) -> bool {
        !self.lasso_points.is_empty()
    }

    pub fn lasso_start(&mut self, x: u32, y: u32) {
        self.lasso_points.clear();
        self.lasso_points.push((x, y));
    }

    pub fn lasso_add(&mut self, x: u32, y: u32) {
        if self.lasso_points.last().copied() != Some((x, y)) {
            self.lasso_points.push((x, y));
        }
    }

    /// Close the in-progress lasso/polygon into a selection.
    pub fn lasso_finish(&mut self) {
        if self.lasso_points.len() >= 3 {
            let poly: Vec<(i32, i32)> = self
                .lasso_points
                .iter()
                .map(|&(x, y)| (x as i32, y as i32))
                .collect();
            if let Some(sel) = self.selection_from_polygon(&poly) {
                self.apply_selection(sel);
            }
        }
        self.lasso_points.clear();
        self.select_anchor = None;
    }

    pub fn lasso_cancel(&mut self) {
        self.lasso_points.clear();
    }

    fn selection_from_polygon(&self, poly: &[(i32, i32)]) -> Option<Selection> {
        let minx = poly.iter().map(|p| p.0).min()?;
        let maxx = poly.iter().map(|p| p.0).max()?;
        let miny = poly.iter().map(|p| p.1).min()?;
        let maxy = poly.iter().map(|p| p.1).max()?;
        let sw = (maxx - minx + 1) as u32;
        let sh = (maxy - miny + 1) as u32;
        let mut mask = vec![false; (sw * sh) as usize];
        for yy in miny..=maxy {
            for xx in minx..=maxx {
                if point_in_polygon(xx as f32 + 0.5, yy as f32 + 0.5, poly) {
                    mask[((yy - miny) as u32 * sw + (xx - minx) as u32) as usize] = true;
                }
            }
        }
        Some(Selection {
            x: minx as u32,
            y: miny as u32,
            w: sw,
            h: sh,
            mask,
        })
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.lasso_points.clear();
        self.pending = None;
    }

    fn selection_to_clipboard(&self, sel: &Selection) -> Clipboard {
        let (l, f) = (self.active_layer, self.active_frame);
        let mut pixels = Vec::with_capacity((sel.w * sel.h) as usize);
        for yy in 0..sel.h {
            for xx in 0..sel.w {
                if sel.selected(xx, yy) {
                    pixels.push(self.sprite.get(l, f, sel.x + xx, sel.y + yy));
                } else {
                    pixels.push(Rgba::TRANSPARENT);
                }
            }
        }
        Clipboard {
            w: sel.w,
            h: sel.h,
            pixels,
            mask: sel.mask.clone(),
        }
    }

    /// Flattened copy: the composite of every visible layer in the active
    /// frame, restricted to the selection. Aseprite calls this Copy Merged.
    fn selection_to_clipboard_merged(&self, sel: &Selection) -> Clipboard {
        let comp = self.sprite.composite(self.active_frame);
        let (w, h) = (self.sprite.width, self.sprite.height);
        let mut pixels = Vec::with_capacity((sel.w * sel.h) as usize);
        for yy in 0..sel.h {
            for xx in 0..sel.w {
                let (x, y) = (sel.x + xx, sel.y + yy);
                if sel.selected(xx, yy) && x < w && y < h {
                    pixels.push(comp[(y * w + x) as usize]);
                } else {
                    pixels.push(Rgba::TRANSPARENT);
                }
            }
        }
        Clipboard {
            w: sel.w,
            h: sel.h,
            pixels,
            mask: sel.mask.clone(),
        }
    }

    /// Copy the selection to the clipboard (active layer).
    pub fn yank(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.clipboard = Some(self.selection_to_clipboard(&sel));
        }
        self.clear_selection();
        self.mode = Mode::Normal;
    }

    /// Copy the flattened selection (all visible layers) to the clipboard.
    pub fn yank_merged(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.clipboard = Some(self.selection_to_clipboard_merged(&sel));
        }
        self.clear_selection();
        self.mode = Mode::Normal;
    }

    /// Cut the selection: copy it, then clear to transparent (one undo step).
    pub fn cut(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.clipboard = Some(self.selection_to_clipboard(&sel));
            self.clear_pixels(&sel);
        }
        self.clear_selection();
        self.mode = Mode::Normal;
    }

    /// Cut the selection and float it (move), entering paste mode.
    pub fn start_move(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.clipboard = Some(self.selection_to_clipboard(&sel));
            self.clear_pixels(&sel);
            self.clear_selection();
            self.paste_pos = Some((sel.x, sel.y));
            self.mode = Mode::Paste;
        } else {
            self.clear_selection();
            self.mode = Mode::Normal;
        }
    }

    fn clear_pixels(&mut self, sel: &Selection) {
        self.begin_stroke();
        for yy in 0..sel.h {
            for xx in 0..sel.w {
                if sel.selected(xx, yy) {
                    self.paint_pixel(sel.x + xx, sel.y + yy, Rgba::TRANSPARENT);
                }
            }
        }
        self.end_stroke();
    }

    /// Float the clipboard for pasting at the cursor.
    pub fn start_paste(&mut self) {
        if self.clipboard.is_some() {
            self.paste_pos = Some(self.cursor);
            self.mode = Mode::Paste;
        }
    }

    pub fn move_paste(&mut self, dx: i32, dy: i32) {
        if let (Some(buf), Some(pos)) = (&self.clipboard, &mut self.paste_pos) {
            let w = self.sprite.width as i32;
            let h = self.sprite.height as i32;
            let nx = (pos.0 as i32 + dx).clamp(0, (w - buf.w as i32).max(0));
            let ny = (pos.1 as i32 + dy).clamp(0, (h - buf.h as i32).max(0));
            *pos = (nx as u32, ny as u32);
        }
    }

    /// Bake the floating clipboard into the active cel (one undo step).
    pub fn commit_paste(&mut self) {
        if let (Some(buf), Some(pos)) = (self.clipboard.clone(), self.paste_pos) {
            self.begin_stroke();
            for yy in 0..buf.h {
                for xx in 0..buf.w {
                    if buf.mask[(yy * buf.w + xx) as usize] {
                        let color = buf.pixels[(yy * buf.w + xx) as usize];
                        if !color.is_transparent() {
                            self.paint_symmetric_unclipped(pos.0 + xx, pos.1 + yy, color);
                        }
                    }
                }
            }
            self.end_stroke();
        }
        self.paste_pos = None;
        self.mode = Mode::Normal;
    }

    pub fn cancel_paste(&mut self) {
        self.paste_pos = None;
        self.mode = Mode::Normal;
    }

    // ---- layer / frame / transform ----

    pub fn move_layer_active(&mut self, dir: i32) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        let from = self.active_layer;
        let to = if dir > 0 {
            self.sprite.subtree_end(from)
        } else {
            from.saturating_sub(1)
        };
        if let Some(to) = self.sprite.move_layer(from, to) {
            self.active_layer = to;
            self.record_structure(before, at);
        }
    }

    /// Move a layer (with its subtree) so it lands at `to`. Drag and drop from
    /// the layer panel; `move_layer_active` is the keyboard path of the same.
    pub fn move_layer_to(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        if let Some(to) = self.sprite.move_layer(from, to) {
            self.active_layer = to;
            self.msg = format!("layer {from} -> {to}");
            self.record_structure(before, at);
        }
    }

    pub fn duplicate_layer(&mut self) {
        let i = self.active_layer;
        if self.sprite.layers[i].group {
            self.msg = "groups cannot be duplicated yet".into();
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        self.sprite.duplicate_layer(i);
        self.active_layer = i;
        self.record_structure(before, at);
        self.msg = "layer duplicated".into();
    }

    pub fn merge_down(&mut self) {
        let up = self.active_layer;
        let down = up + 1;
        if down >= self.sprite.layers.len() {
            self.msg = "no layer below".into();
            return;
        }
        if self.sprite.layers[up].group || self.sprite.layers[down].group {
            self.msg = "groups cannot be merged".into();
            return;
        }
        if self.sprite.layers[up].reference || self.sprite.layers[down].reference {
            self.msg = "reference layers cannot be merged".into();
            return;
        }
        if self.sprite.layers[up].child_level != self.sprite.layers[down].child_level {
            self.msg = "merge needs sibling layers".into();
            return;
        }
        if !self.sprite.layers[up].visible || !self.sprite.layers[down].visible {
            self.msg = "merge needs both layers visible".into();
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        self.sprite.merge_down(up);
        self.active_layer = up.min(self.sprite.layers.len() - 1);
        self.record_structure(before, at);
        self.msg = "merged down".into();
    }

    pub fn flatten(&mut self) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        if !self.sprite.flatten() {
            self.msg = "already flat".into();
            return;
        }
        self.active_layer = 0;
        self.active_frame = self.active_frame.min(self.sprite.frames.len() - 1);
        self.selection = None;
        self.select_anchor = None;
        self.pending = None;
        self.paste_pos = None;
        self.record_structure(before, at);
        self.msg = "flattened".into();
    }

    pub fn add_group(&mut self) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        let idx = self.active_layer;
        self.sprite.add_group_after(idx);
        self.active_layer = idx + 1;
        self.record_structure(before, at);
    }

    /// Indent (`dir > 0`) or outdent the active layer and its children.
    pub fn indent_layer(&mut self, dir: i32) {
        let i = self.active_layer;
        let cur = self.sprite.layers[i].child_level as i32;
        let max = if i == 0 {
            0
        } else {
            let prev = &self.sprite.layers[i - 1];
            prev.child_level as i32 + prev.group as i32
        };
        let delta = (cur + dir).clamp(0, max) - cur;
        if delta == 0 {
            self.msg = if dir > 0 {
                "no group above to indent into".into()
            } else {
                "already at top level".into()
            };
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        let end = self.sprite.subtree_end(i);
        for layer in &mut self.sprite.layers[i..end] {
            layer.child_level = (layer.child_level as i32 + delta) as u16;
        }
        self.record_structure(before, at);
    }

    pub fn layer_props(&self, layer: usize) -> LayerProps {
        let l = &self.sprite.layers[layer];
        LayerProps {
            visible: l.visible,
            editable: l.editable,
            opacity: l.opacity,
            blend: l.blend,
            background: l.background,
            reference: l.reference,
        }
    }

    fn apply_layer_props(&mut self, layer: usize, p: LayerProps) {
        if let Some(l) = self.sprite.layers.get_mut(layer) {
            l.visible = p.visible;
            l.editable = p.editable;
            l.opacity = p.opacity;
            l.blend = p.blend;
            l.background = p.background;
            l.reference = p.reference;
        }
    }

    /// Record a panel property change as one undo step. The widget has already
    /// written the new value; `before` is what the row looked like before the
    /// interaction.
    pub fn record_layer_props(&mut self, layer: usize, before: LayerProps) {
        if layer >= self.sprite.layers.len() {
            return;
        }
        let after = self.layer_props(layer);
        if after == before {
            return;
        }
        self.undo.push(UndoStep::LayerProps {
            layer,
            before,
            after,
        });
        self.redo.clear();
        self.dirty = true;
        self.touch();
    }

    fn edit_point(&self) -> EditPoint {
        EditPoint {
            layer: self.active_layer,
            frame: self.active_frame,
            tag: self.active_tag,
        }
    }

    /// Restore indices from a snapshot, clamped: a snapshot taken before a
    /// delete cannot point past the end of the restored (shorter) list.
    fn restore_point(&mut self, at: EditPoint) {
        self.active_layer = at.layer.min(self.sprite.layers.len().saturating_sub(1));
        self.active_frame = at.frame.min(self.sprite.frames.len().saturating_sub(1));
        self.active_tag = at.tag.filter(|&i| i < self.sprite.tags.len());
        self.clamp_cursor();
    }

    /// Record a structural edit (layers, frames, tags) as one undo step.
    fn record_structure(&mut self, before: Sprite, before_at: EditPoint) {
        let after = self.sprite.clone();
        let after_at = self.edit_point();
        self.undo.push(UndoStep::Structure {
            before: Box::new(before),
            after: Box::new(after),
            before_at,
            after_at,
        });
        self.redo.clear();
        self.dirty = true;
        self.touch();
    }

    /// Mask ops are whole-sprite snapshots so they undo as one step.
    fn snapshot(&mut self, before: Sprite) {
        self.undo.push(UndoStep::Snapshot {
            before: Box::new(before),
            after: Box::new(self.sprite.clone()),
        });
        self.redo.clear();
        self.dirty = true;
        self.touch();
    }

    /// Add an alpha mask on the active layer: the selection if there is one,
    /// otherwise fully opaque. Replaces an existing mask.
    pub fn mask_add(&mut self) {
        let (l, w, h) = (self.active_layer, self.sprite.width, self.sprite.height);
        if self.sprite.layers[l].group {
            self.msg = "groups have no mask".into();
            return;
        }
        let mut pixels = vec![255u8; (w * h) as usize];
        if let Some(sel) = self.current_selection() {
            pixels.fill(0);
            for y in sel.y..(sel.y + sel.h).min(h) {
                for x in sel.x..(sel.x + sel.w).min(w) {
                    if sel.contains(x, y) {
                        pixels[(y * w + x) as usize] = 255;
                    }
                }
            }
        }
        let before = self.sprite.clone();
        self.sprite.layers[l].mask = Some(crate::sprite::Mask { pixels });
        self.snapshot(before);
        self.msg = "mask added".into();
    }

    /// Multiply the mask into the layer's pixels (all frames), then drop it.
    pub fn mask_apply(&mut self) {
        let l = self.active_layer;
        if self.sprite.layers[l].mask.is_none() {
            self.msg = "no mask on this layer".into();
            return;
        }
        let before = self.sprite.clone();
        let mask = self.sprite.layers[l].mask.take().unwrap();
        for cel in &mut self.sprite.cels[l] {
            for (i, px) in cel.pixels.iter_mut().enumerate() {
                let m = *mask.pixels.get(i).unwrap_or(&255) as u32;
                px.3 = (px.3 as u32 * m / 255) as u8;
            }
        }
        self.snapshot(before);
        self.msg = "mask applied".into();
    }

    pub fn mask_del(&mut self) {
        let l = self.active_layer;
        if self.sprite.layers[l].mask.is_none() {
            self.msg = "no mask on this layer".into();
            return;
        }
        let before = self.sprite.clone();
        self.sprite.layers[l].mask = None;
        self.snapshot(before);
        self.msg = "mask removed".into();
    }

    pub fn rename_layer(&mut self, name: &str) {
        if self.sprite.layers[self.active_layer].name == name {
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        self.sprite.layers[self.active_layer].name = name.to_string();
        self.record_structure(before, at);
    }

    /// Mark the active layer as Aseprite's background layer. A background is
    /// opaque, so the opacity is forced to 255.
    pub fn set_layer_background(&mut self, on: bool) {
        let layer = self.active_layer;
        if self.sprite.layers[layer].group {
            self.msg = "groups cannot be a background".into();
            return;
        }
        if self.sprite.layers[layer].background == on {
            return;
        }
        let before = self.layer_props(layer);
        self.sprite.layers[layer].background = on;
        if on {
            self.sprite.layers[layer].opacity = 255;
        }
        self.record_layer_props(layer, before);
        self.msg = if on {
            "background layer".into()
        } else {
            "normal layer".into()
        };
    }

    /// Mark the active layer as a reference layer: drawn for tracing, never
    /// painted and excluded from exports.
    pub fn set_layer_reference(&mut self, on: bool) {
        let layer = self.active_layer;
        if self.sprite.layers[layer].group {
            self.msg = "groups cannot be reference layers".into();
            return;
        }
        if self.sprite.layers[layer].reference == on {
            return;
        }
        let before = self.layer_props(layer);
        self.sprite.layers[layer].reference = on;
        self.record_layer_props(layer, before);
        self.msg = if on {
            "reference layer (not exported)".into()
        } else {
            "normal layer".into()
        };
    }

    /// Lock or unlock the active layer. Blocked painting is enforced elsewhere;
    /// this only flips the flag so the change is undoable.
    pub fn set_layer_editable(&mut self, editable: bool) {
        let layer = self.active_layer;
        if self.sprite.layers[layer].editable == editable {
            return;
        }
        let before = self.layer_props(layer);
        self.sprite.layers[layer].editable = editable;
        self.record_layer_props(layer, before);
    }

    pub fn duplicate_frame(&mut self) {
        let before = self.sprite.clone();
        let at = self.edit_point();
        self.sprite.duplicate_frame(self.active_frame);
        self.active_frame += 1;
        self.record_structure(before, at);
    }

    pub fn flip_h(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.transform_selection(&sel, Xform::FlipH);
        } else {
            self.transform_whole(Xform::FlipH);
        }
    }

    pub fn flip_v(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.transform_selection(&sel, Xform::FlipV);
        } else {
            self.transform_whole(Xform::FlipV);
        }
    }

    pub fn rotate_cw(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.transform_selection(&sel, Xform::Rot90);
        } else {
            self.transform_whole(Xform::Rot90);
        }
    }

    /// Transform the whole sprite (all cels) as one snapshot undo step.
    fn transform_whole(&mut self, kind: Xform) {
        let before = self.sprite.clone();
        match kind {
            Xform::FlipH => self.sprite.flip_h(),
            Xform::FlipV => self.sprite.flip_v(),
            Xform::Rot90 => self.sprite.rotate_cw(),
        }
        let after = self.sprite.clone();
        self.undo.push(UndoStep::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        });
        self.redo.clear();
        self.clamp_cursor();
        self.selection = None;
        self.select_anchor = None;
        self.dirty = true;
        self.touch();
    }

    /// Transform the selected pixels inside the active cel (one undo step).
    fn transform_selection(&mut self, sel: &Selection, kind: Xform) {
        let (ox, oy) = (sel.x, sel.y);
        let (w, h) = (sel.w, sel.h);
        let (nw, nh) = match kind {
            Xform::Rot90 => (h, w),
            _ => (w, h),
        };
        // Union region (shared origin) holding both the old and new bboxes.
        let uw = w.max(nw).min(self.sprite.width.saturating_sub(ox));
        let uh = h.max(nh).min(self.sprite.height.saturating_sub(oy));
        if uw == 0 || uh == 0 {
            return;
        }
        // Clamped new-bbox size for the updated selection.
        let snw = nw.min(self.sprite.width.saturating_sub(ox));
        let snh = nh.min(self.sprite.height.saturating_sub(oy));

        let (l, f) = (self.active_layer, self.active_frame);
        let mut orig = vec![Rgba::TRANSPARENT; (uw * uh) as usize];
        for yy in 0..uh {
            for xx in 0..uw {
                orig[(yy * uw + xx) as usize] = self.sprite.get(l, f, ox + xx, oy + yy);
            }
        }
        let mut final_grid = orig.clone();

        let map = |xx: u32, yy: u32| -> (u32, u32) {
            match kind {
                Xform::FlipH => (w - 1 - xx, yy),
                Xform::FlipV => (xx, h - 1 - yy),
                Xform::Rot90 => (h - 1 - yy, xx),
            }
        };

        // Vacate the selected pixels, then place them at their mapped positions.
        for yy in 0..h {
            for xx in 0..w {
                if sel.selected(xx, yy) {
                    final_grid[(yy * uw + xx) as usize] = Rgba::TRANSPARENT;
                }
            }
        }
        for yy in 0..h {
            for xx in 0..w {
                if sel.selected(xx, yy) {
                    let (nx, ny) = map(xx, yy);
                    if nx < uw && ny < uh {
                        final_grid[(ny * uw + nx) as usize] = orig[(yy * uw + xx) as usize];
                    }
                }
            }
        }

        self.begin_stroke();
        for yy in 0..uh {
            for xx in 0..uw {
                let i = (yy * uw + xx) as usize;
                if final_grid[i] != orig[i] {
                    // The mapped pixels leave the old mask; the selection is
                    // replaced with the transformed shape below. Clip against
                    // the old one and the move would delete its own content.
                    self.paint_pixel_unclipped(ox + xx, oy + yy, final_grid[i]);
                }
            }
        }
        self.end_stroke();

        // Update the selection mask to the transformed shape.
        let mut nmask = vec![false; (snw * snh) as usize];
        for yy in 0..h {
            for xx in 0..w {
                if sel.selected(xx, yy) {
                    let (nx, ny) = map(xx, yy);
                    if nx < snw && ny < snh {
                        nmask[(ny * snw + nx) as usize] = true;
                    }
                }
            }
        }
        if nmask.iter().any(|&v| v) {
            self.selection = Some(Selection {
                x: ox,
                y: oy,
                w: snw,
                h: snh,
                mask: nmask,
            });
        } else {
            self.selection = None;
        }
        self.select_anchor = None;
        self.dirty = true;
    }

    pub fn new_sprite(&mut self, w: u32, h: u32) {
        self.replace_sprite(Sprite::new(w, h));
    }

    /// Load a sprite by extension (`.ase`/`.aseprite`, `.png`, else the JSON
    /// project) and reset transient editor state. Shared by `:open` and the
    /// command line, so `vixel foo.vpx` and `:open foo.vpx` are one code path.
    /// The path is remembered, so `:w` with no argument writes it back.
    pub fn open_path(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let sprite = crate::io::load_any(path)?;
        self.replace_sprite(sprite);
        self.adopt_bundle(path);
        self.path = Some(path.to_path_buf());
        self.note_recent(path.to_path_buf());
        self.dirty = false;
        self.msg = "loaded".into();
        Ok(())
    }

    /// Remember a file as recently opened, newest first and capped at 10.
    pub fn note_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(10);
    }

    /// Swap in an already-built sprite and reset transient editor state.
    pub fn replace_sprite(&mut self, s: Sprite) {
        self.sprite = s;
        self.cursor = (0, 0);
        self.active_layer = 0;
        self.active_frame = 0;
        self.undo.clear();
        self.redo.clear();
        self.stroke = None;
        self.pending = None;
        self.select_anchor = None;
        self.selection = None;
        self.clipboard = None;
        self.paste_pos = None;
        self.lasso_points.clear();
        self.dirty = false;
        self.bundle_stage = None;
        self.palette_lock = false;
        if self.sprite.is_indexed() && !self.sprite.palette.is_empty() {
            self.load_palette(self.sprite.palette.clone());
        }
        self.dirty = false;
        self.touch();
    }

    /// A fix bundle has a locked `base` and an editable `fix` layer. Land on
    /// `fix` in draw mode, so the first keypress paints; a plain project is
    /// left exactly as vixel has always opened it.
    fn adopt_bundle(&mut self, path: &std::path::Path) {
        let fix = self
            .sprite
            .layers
            .iter()
            .position(|l| l.name == "fix" && l.editable);
        let base = self
            .sprite
            .layers
            .iter()
            .position(|l| l.name == "base" && !l.editable);
        let (Some(fix), Some(_)) = (fix, base) else {
            return;
        };
        let stage = sidecar_stage(path).unwrap_or_else(|| "fix".to_string());
        self.active_layer = fix;
        self.mode = Mode::Draw;
        self.msg = format!("{stage} fix — painting on the `fix` layer");
        self.bundle_stage = Some(stage);
    }

    /// What to report after a write. Losing `ref` is not fatal — the strokes
    /// still composite — but the alignment frame is gone, so say it every time.
    pub fn saved_note(&self) -> String {
        let lost_ref = self.bundle_stage.is_some()
            && !self.sprite.layers.iter().any(|layer| layer.name == "ref");
        if lost_ref {
            "saved — warning: ref layer removed, alignment lost".into()
        } else {
            "saved".into()
        }
    }
}

/// The `stage` field of a bundle's sidecar (`<name>.json` next to `<name>.vpx`).
fn sidecar_stage(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path.with_extension("json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("stage")?.as_str().map(str::to_string)
}

/// Even-odd point-in-polygon test (ray cast along +x). `poly` is implicitly closed.
fn point_in_polygon(px: f32, py: f32, poly: &[(i32, i32)]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (x1, y1) = (poly[i].0 as f32, poly[i].1 as f32);
        let (x2, y2) = (poly[(i + 1) % n].0 as f32, poly[(i + 1) % n].1 as f32);
        if (y1 > py) != (y2 > py) {
            let xint = (x2 - x1) * (py - y1) / (y2 - y1) + x1;
            if px < xint {
                inside = !inside;
            }
        }
    }
    inside
}

/// Combine two mask shapes (`Add` / `Subtract` / `Intersect`).
fn combine_masks(a: &Selection, b: &Selection, op: SelectionOp) -> Selection {
    let minx = a.x.min(b.x);
    let miny = a.y.min(b.y);
    let maxx = (a.x + a.w).max(b.x + b.w); // exclusive
    let maxy = (a.y + a.h).max(b.y + b.h);
    let w = maxx - minx;
    let h = maxy - miny;
    let mut mask = vec![false; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (minx + x, miny + y);
            let ia = a.contains(px, py);
            let ib = b.contains(px, py);
            mask[(y * w + x) as usize] = match op {
                SelectionOp::Add => ia || ib,
                SelectionOp::Subtract => ia && !ib,
                SelectionOp::Intersect => ia && ib,
                SelectionOp::Replace => ib,
            };
        }
    }
    Selection {
        x: minx,
        y: miny,
        w,
        h,
        mask,
    }
}

/// Is `b` the L-shaped middle between the diagonal neighbours `a` and `c`?
fn is_corner(a: (u32, u32), b: (u32, u32), c: (u32, u32)) -> bool {
    let diagonal = (a.0 as i32 - c.0 as i32).abs() == 1 && (a.1 as i32 - c.1 as i32).abs() == 1;
    diagonal && ((b.0 == a.0 && b.1 == c.1) || (b.0 == c.0 && b.1 == a.1))
}

/// Darken (`f = 0.8`) or lighten (`f = 1.25`) a color, keeping alpha.
fn shade_color(c: Rgba, darken: bool) -> Rgba {
    let f = if darken { 0.8 } else { 1.25 };
    let ch = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
    Rgba(ch(c.0), ch(c.1), ch(c.2), c.3)
}

/// Linear interpolation between two colors, `t` in 0..1.
fn lerp_rgba(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let l = |x: u8, y: u8| {
        (x as f32 + (y as f32 - x as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba(l(a.0, b.0), l(a.1, b.1), l(a.2, b.2), l(a.3, b.3))
}

/// A one-shot colour filter. `amount` is per filter: brightness -255..255,
/// contrast 0..3 as a factor, hue in degrees, saturation as a factor, outline
/// and blur as a pixel radius, sharpen as a strength.
/// The filter names, in the order the combo box lists them.
pub const FILTERS: [&str; 8] = [
    "invert",
    "brightness",
    "contrast",
    "hue",
    "saturation",
    "outline",
    "blur",
    "sharpen",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Invert,
    Brightness(i32),
    Contrast(f32),
    Hue(f32),
    Saturation(f32),
    Outline(u32),
    Blur(u32),
    Sharpen(f32),
}

impl Filter {
    pub fn name(self) -> String {
        match self {
            Filter::Invert => "invert".into(),
            Filter::Brightness(v) => format!("brightness {v}"),
            Filter::Contrast(v) => format!("contrast {v:.2}"),
            Filter::Hue(v) => format!("hue {v:.0}"),
            Filter::Saturation(v) => format!("saturation {v:.2}"),
            Filter::Outline(v) => format!("outline {v}"),
            Filter::Blur(v) => format!("blur {v}"),
            Filter::Sharpen(v) => format!("sharpen {v:.2}"),
        }
    }

    /// Parse `name [amount]`, the same table the `:filter` command uses.
    pub fn parse(name: &str, amount: Option<&str>) -> Option<Filter> {
        let a = amount.and_then(|v| v.parse::<f32>().ok());
        Some(match name {
            "invert" | "inv" => Filter::Invert,
            "brightness" | "bri" => Filter::Brightness(a.unwrap_or(24.0) as i32),
            "contrast" | "con" => Filter::Contrast(a.unwrap_or(1.2)),
            "hue" => Filter::Hue(a.unwrap_or(30.0)),
            "saturation" | "sat" => Filter::Saturation(a.unwrap_or(1.3)),
            "outline" | "out" => Filter::Outline(a.unwrap_or(1.0).max(1.0) as u32),
            "blur" => Filter::Blur(a.unwrap_or(1.0).max(1.0) as u32),
            "sharpen" | "sharp" => Filter::Sharpen(a.unwrap_or(1.0)),
            _ => return None,
        })
    }
}

fn apply_filter(src: &[Rgba], w: u32, h: u32, f: Filter, fg: Rgba) -> Vec<Rgba> {
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let mut out = src.to_vec();
    match f {
        Filter::Invert => {
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    out[i] = Rgba(255 - p.0, 255 - p.1, 255 - p.2, p.3);
                }
            }
        }
        Filter::Brightness(d) => {
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    let add = |c: u8| (c as i32 + d).clamp(0, 255) as u8;
                    out[i] = Rgba(add(p.0), add(p.1), add(p.2), p.3);
                }
            }
        }
        Filter::Contrast(k) => {
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    let f = |c: u8| ((c as f32 - 128.0) * k + 128.0).clamp(0.0, 255.0) as u8;
                    out[i] = Rgba(f(p.0), f(p.1), f(p.2), p.3);
                }
            }
        }
        Filter::Hue(deg) => {
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    let (hh, s, v) = crate::color::to_hsv(*p);
                    let c = crate::color::from_hsv(hh + deg, s, v);
                    out[i] = Rgba(c.0, c.1, c.2, p.3);
                }
            }
        }
        Filter::Saturation(k) => {
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    let (hh, s, v) = crate::color::to_hsv(*p);
                    let c = crate::color::from_hsv(hh, s * k, v);
                    out[i] = Rgba(c.0, c.1, c.2, p.3);
                }
            }
        }
        Filter::Outline(n) => {
            // Grow the opaque silhouette by `n` into the transparent pixels.
            let mut solid: Vec<bool> = src.iter().map(|p| p.3 > 0).collect();
            for _ in 0..n {
                solid = dilate(&solid, w, h);
            }
            for i in 0..src.len() {
                if solid[i] && src[i].3 == 0 {
                    out[i] = fg;
                }
            }
        }
        Filter::Blur(n) => out = box_blur(src, w, h, n),
        Filter::Sharpen(k) => {
            let soft = box_blur(src, w, h, 1);
            for i in 0..src.len() {
                let (p, b) = (src[i], soft[i]);
                if p.3 == 0 {
                    continue;
                }
                let f =
                    |c: u8, bc: u8| (c as f32 + k * (c as f32 - bc as f32)).clamp(0.0, 255.0) as u8;
                out[i] = Rgba(f(p.0, b.0), f(p.1, b.1), f(p.2, b.2), p.3);
            }
        }
    }
    let _ = idx;
    out
}

/// Box blur over alpha-weighted colour, so a blur softens the edge instead of
/// leaving a hard one: transparent neighbours pull alpha down, they do not just
/// get skipped.
fn box_blur(src: &[Rgba], w: u32, h: u32, r: u32) -> Vec<Rgba> {
    let r = r.clamp(1, 32) as i32;
    let mut out = src.to_vec();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (mut sr, mut sg, mut sb, mut sa, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let p = src[(ny as u32 * w + nx as u32) as usize];
                    sr += p.0 as u32 * p.3 as u32;
                    sg += p.1 as u32 * p.3 as u32;
                    sb += p.2 as u32 * p.3 as u32;
                    sa += p.3 as u32;
                    n += 1;
                }
            }
            let i = (y as u32 * w + x as u32) as usize;
            if src[i].3 == 0 || sa == 0 || n == 0 {
                continue;
            }
            out[i] = Rgba(
                (sr / sa) as u8,
                (sg / sa) as u8,
                (sb / sa) as u8,
                (sa / n) as u8,
            );
        }
    }
    out
}

/// One 4-neighbour dilation step on a sprite-sized grid.
fn dilate(g: &[bool], w: u32, h: u32) -> Vec<bool> {
    let mut out = g.to_vec();
    for y in 0..h {
        for x in 0..w {
            if !g[(y * w + x) as usize] {
                continue;
            }
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx >= 0 && ny >= 0 && (nx as u32) < w && (ny as u32) < h {
                    out[(ny as u32 * w + nx as u32) as usize] = true;
                }
            }
        }
    }
    out
}

/// One 4-neighbour erosion step. The border erodes away.
fn erode(g: &[bool], w: u32, h: u32) -> Vec<bool> {
    let mut out = g.to_vec();
    for y in 0..h {
        for x in 0..w {
            if !g[(y * w + x) as usize] {
                continue;
            }
            let keep = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .all(|(dx, dy)| {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    nx >= 0
                        && ny >= 0
                        && (nx as u32) < w
                        && (ny as u32) < h
                        && g[(ny as u32 * w + nx as u32) as usize]
                });
            if !keep {
                out[(y * w + x) as usize] = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn filters_are_undoable_and_keep_alpha() {
        let mut e = Editor::new(4, 4);
        let (l, f) = (e.active_layer, e.active_frame);
        e.fg = Rgba::rgb(10, 20, 30);
        // A hard vertical edge: two columns transparent, two opaque.
        for y in 0..4 {
            for x in 2..4 {
                e.paint_pixel(x, y, Rgba::rgb(200, 60, 40));
            }
        }
        e.undo.clear();

        e.filter(Filter::Invert);
        assert_eq!(e.sprite.get(l, f, 3, 0), Rgba::rgb(55, 195, 215));
        assert_eq!(
            e.sprite.get(l, f, 0, 0),
            Rgba::TRANSPARENT,
            "clear stays clear"
        );
        e.undo();
        assert_eq!(e.sprite.get(l, f, 3, 0), Rgba::rgb(200, 60, 40), "one undo");

        // Outline grows into the transparent pixels in fg and is clipped at the edge.
        e.filter(Filter::Outline(1));
        assert_eq!(e.sprite.get(l, f, 1, 0), Rgba::rgb(10, 20, 30));
        e.undo();

        // Brightness clamps instead of wrapping.
        e.filter(Filter::Brightness(100));
        assert_eq!(e.sprite.get(l, f, 3, 0), Rgba::rgb(255, 160, 140));
        e.undo();

        // Contrast about 128, hue and saturation round-trip by identity values.
        let before = e.sprite.get(l, f, 3, 1);
        e.filter(Filter::Contrast(1.0));
        assert_eq!(e.sprite.get(l, f, 3, 1), before);
        e.filter(Filter::Hue(360.0));
        let after = e.sprite.get(l, f, 3, 1);
        assert!(after.0.abs_diff(before.0) <= 2 && after.1.abs_diff(before.1) <= 2);
        e.filter(Filter::Saturation(1.0));

        // Blur softens the edge: the pixel next to it changed, alpha kept.
        e.filter(Filter::Blur(1));
        let p = e.sprite.get(l, f, 2, 0);
        assert!(p.3 < 255, "the blurred edge softens into the clear side");
        assert!(p.3 > 0);
        e.undo();
        e.filter(Filter::Sharpen(1.0));
    }

    #[test]
    fn free_transform_rotates_scales_and_skews_about_the_centre() {
        let mut e = Editor::new(16, 16);
        let (l, f) = (e.active_layer, e.active_frame);
        // A 9x9 selection centred on pixel 8,8 with one marker left of centre.
        e.selection = Some(Selection::rect(4, 4, 12, 12));
        e.paint_pixel(7, 8, Rgba::rgb(200, 0, 0));
        e.paint_pixel(8, 8, Rgba::rgb(0, 200, 0));

        // Identity leaves everything alone.
        e.free_transform(0.0, 1.0, 1.0, 0.0);
        assert_eq!(e.sprite.get(l, f, 7, 8), Rgba::rgb(200, 0, 0));

        // 180 degrees about the region centre (8.5, 8.5): pixel 7 -> 9.
        e.free_transform(180.0, 1.0, 1.0, 0.0);
        assert_eq!(e.sprite.get(l, f, 7, 8), Rgba::TRANSPARENT);
        assert_eq!(e.sprite.get(l, f, 9, 8), Rgba::rgb(200, 0, 0));
        e.undo();
        assert_eq!(e.sprite.get(l, f, 7, 8), Rgba::rgb(200, 0, 0), "one undo");

        // Scale 2 pushes the marker twice as far out from the centre.
        e.free_transform(0.0, 2.0, 2.0, 0.0);
        assert_eq!(e.sprite.get(l, f, 6, 8), Rgba::rgb(200, 0, 0));
        e.undo();

        // Skew 45 degrees: a pixel 3 rows above the centre slides 3 px right.
        e.paint_pixel(8, 5, Rgba::rgb(0, 0, 200));
        e.free_transform(0.0, 1.0, 1.0, 45.0);
        assert_eq!(
            e.sprite.get(l, f, 8, 5),
            Rgba::TRANSPARENT,
            "the pixel moved"
        );
        assert_eq!(e.sprite.get(l, f, 11, 5), Rgba::rgb(0, 0, 200));
        // Outside the selection nothing is touched.
        assert_eq!(e.sprite.get(l, f, 0, 0), Rgba::TRANSPARENT);
    }

    #[test]
    fn the_colour_profile_converts_the_sprite_and_the_palette_together() {
        use crate::sprite::ColorProfile;
        let mut e = Editor::new(2, 2);
        let (l, f) = (e.active_layer, e.active_frame);
        e.paint_pixel(0, 0, Rgba::rgb(128, 128, 128));
        let palette_before = e.palette.clone();

        e.set_color_profile(ColorProfile::Linear);
        let p = e.sprite.get(l, f, 0, 0);
        assert!((54..=57).contains(&p.0), "128 sRGB -> {} linear", p.0);
        assert_eq!(e.sprite.color_profile, ColorProfile::Linear);
        assert_ne!(e.palette[0], palette_before[0], "the palette converted too");

        e.undo();
        assert_eq!(
            e.sprite.get(l, f, 0, 0),
            Rgba::rgb(128, 128, 128),
            "one undo"
        );
        assert_eq!(e.sprite.color_profile, ColorProfile::Srgb);

        // Converting twice is a no-op, not a second conversion.
        e.set_color_profile(ColorProfile::Linear);
        let once = e.sprite.get(l, f, 0, 0);
        e.set_color_profile(ColorProfile::Linear);
        assert_eq!(e.sprite.get(l, f, 0, 0), once);
    }

    #[test]
    fn indexed_mode_quantizes_and_snaps_painting() {
        use crate::sprite::ColorMode;
        let mut e = Editor::new(2, 1);
        e.paint_pixel(0, 0, Rgba(255, 0, 0, 255));
        e.paint_pixel(1, 0, Rgba(0, 0, 255, 255));

        e.set_color_mode(ColorMode::Indexed);
        assert!(e.sprite.is_indexed());
        assert!(!e.palette_lock, "indexed mode snaps instead of refusing");
        assert_eq!(e.palette, e.sprite.palette);
        let red = e.sprite.palette[1..]
            .iter()
            .copied()
            .find(|c| (c.0, c.1, c.2) == (255, 0, 0))
            .expect("red in palette");
        let blue = e.sprite.palette[1..]
            .iter()
            .copied()
            .find(|c| (c.0, c.1, c.2) == (0, 0, 255))
            .expect("blue in palette");

        e.paint_pixel(0, 0, Rgba(2, 3, 250, 255));
        assert_eq!(e.sprite.get(0, 0, 0, 0), blue, "paint resolves to entry");
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 0, 0), red);

        let (l, f) = (e.active_layer, e.active_frame);
        e.undo(); // undo the mode switch
        assert_eq!(e.sprite.color_mode, ColorMode::Auto);
        assert_eq!(e.sprite.get(l, f, 0, 0), Rgba(255, 0, 0, 255));
        assert_eq!(e.sprite.get(l, f, 1, 0), Rgba(0, 0, 255, 255));
    }

    #[test]
    fn opening_an_indexed_sprite_syncs_its_palette() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ase/test_paletted.ase");
        let mut e = Editor::new(1, 1);
        e.open_path(&path).unwrap();
        assert!(e.sprite.is_indexed());
        assert_eq!(e.palette, e.sprite.palette);
        assert!(!e.palette_lock);
    }

    #[test]
    fn sprite_properties_convert_and_persist() {
        use crate::sprite::ColorMode;
        let mut e = Editor::new(4, 4);
        let (l, f) = (e.active_layer, e.active_frame);
        e.paint_pixel(1, 1, Rgba::rgb(200, 60, 40));

        e.set_color_mode(ColorMode::Grayscale);
        let p = e.sprite.get(l, f, 1, 1);
        assert!(p.0 == p.1 && p.1 == p.2, "grayscale conversion: {p:?}");
        assert_eq!(p.3, 255);
        assert!(e.palette_lock, "indexed painting is palette-locked");
        e.undo();
        assert_eq!(e.sprite.get(l, f, 1, 1), Rgba::rgb(200, 60, 40), "one undo");

        e.set_pixel_aspect(2, 1);
        e.set_sprite_grid(8, 8);
        assert_eq!(e.sprite.pixel_aspect, (2, 1));
        assert_eq!(e.sprite.grid, (8, 8));
        assert_eq!(e.grid, 8, "the design grid follows the sprite grid");

        // The three properties survive a .vpx round trip.
        let path = std::env::temp_dir().join(format!("vixel-props-{}.vpx", std::process::id()));
        crate::io::save_project(&e.sprite, &path).expect("save");
        let back = crate::io::load_project(&path).expect("load");
        assert_eq!(back.pixel_aspect, (2, 1));
        assert_eq!(back.grid, (8, 8));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn filters_only_touch_the_selection() {
        let mut e = Editor::new(4, 1);
        let (l, f) = (e.active_layer, e.active_frame);
        for x in 0..4 {
            e.paint_pixel(x, 0, Rgba::rgb(10, 10, 10));
        }
        e.selection = Some(Selection::rect(0, 0, 1, 0));
        e.filter(Filter::Invert);
        assert_eq!(e.sprite.get(l, f, 0, 0), Rgba::rgb(245, 245, 245));
        assert_eq!(
            e.sprite.get(l, f, 2, 0),
            Rgba::rgb(10, 10, 10),
            "outside is untouched"
        );
    }

    #[test]
    fn selection_shape_ops_grow_shrink_ring_and_flip() {
        let mut e = Editor::new(16, 16);
        e.selection = Some(Selection::rect(4, 4, 7, 7));
        e.selection_expand(2);
        let s = e.selection.clone().expect("expanded");
        assert_eq!((s.x, s.y, s.w, s.h), (2, 2, 8, 8));
        // 4-neighbour dilation chamfers the corners; the diamond-ish ring is
        // 52 px against the 64 of a square.
        assert_eq!(s.mask.iter().filter(|v| **v).count(), 52);

        e.selection = Some(Selection::rect(4, 4, 7, 7));
        e.selection_contract(1);
        let s = e.selection.clone().expect("contracted");
        assert_eq!((s.x, s.y, s.w, s.h), (5, 5, 2, 2));

        e.selection = Some(Selection::rect(4, 4, 7, 7));
        e.selection_border(1);
        let s = e.selection.clone().expect("ring");
        let n = s.mask.iter().filter(|v| **v).count();
        // One pixel outside the edge plus the edge itself: 32 (the chamfered
        // dilated square) minus the 4-pixel interior.
        assert_eq!(n, 28, "ring width 1: {n}");
        assert!(!s.contains(5, 5), "the middle is not part of a ring");

        e.selection = Some(Selection::rect(4, 4, 7, 7));
        e.selection_invert();
        let s = e.selection.clone().expect("inverted");
        assert_eq!((s.x, s.y, s.w, s.h), (0, 0, 16, 16));
        assert_eq!(s.mask.iter().filter(|v| **v).count(), 256 - 16);

        // Feather is a dither ramp: the edge thins out but the core stays.
        e.selection = Some(Selection::rect(2, 2, 13, 13));
        e.selection_feather(3);
        let s = e.selection.clone().expect("feathered");
        let n = s.mask.iter().filter(|v| **v).count();
        assert!(n < 144 && n > 60, "feather keeps {n} of 144");
        assert!(
            s.contains(8, 8),
            "the middle of a feathered selection must survive"
        );

        // Expand past the sprite edge clips instead of panicking.
        e.selection = Some(Selection::rect(0, 0, 2, 2));
        e.selection_expand(64);
        let s = e.selection.clone().expect("expanded to the edge");
        assert_eq!((s.x, s.y, s.w, s.h), (0, 0, 16, 16));
    }

    use super::*;

    fn bundle_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vixel-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_bundle(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        use crate::sprite::{Cel, Layer};
        let path = dir.join(format!("{name}.vpx"));
        let mut sprite = Sprite::new(4, 4);
        sprite.layers = vec![
            Layer {
                name: "fix".into(),
                ..Default::default()
            },
            Layer {
                name: "base".into(),
                editable: false,
                ..Default::default()
            },
            Layer {
                name: "ref".into(),
                editable: false,
                ..Default::default()
            },
        ];
        sprite.cels = (0..3).map(|_| vec![Cel::blank(16)]).collect();
        crate::io::save_project(&sprite, &path).unwrap();
        std::fs::write(dir.join(format!("{name}.json")), r#"{"stage":"face"}"#).unwrap();
        path
    }

    #[test]
    fn a_fix_bundle_opens_on_the_fix_layer_in_draw_mode() {
        let dir = bundle_dir("bundle");
        let path = write_bundle(&dir, "soft");
        let mut e = Editor::new(8, 8);
        e.open_path(&path).unwrap();
        assert_eq!(e.active_layer, 0);
        assert_eq!(e.mode, Mode::Draw);
        assert_eq!(e.bundle_stage.as_deref(), Some("face"));
        assert_eq!(e.saved_note(), "saved");
        // A save after deleting `ref` still works, but says what was lost.
        e.sprite.layers.retain(|layer| layer.name != "ref");
        assert!(e.saved_note().contains("ref layer removed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plain_project_opens_the_way_it_always_did() {
        let dir = bundle_dir("plain");
        let path = dir.join("plain.vpx");
        crate::io::save_project(&Sprite::new(4, 4), &path).unwrap();
        let mut e = Editor::new(8, 8);
        e.open_path(&path).unwrap();
        assert_eq!(e.mode, Mode::Normal);
        assert!(e.bundle_stage.is_none());
        // A `fix` layer alone is not a bundle: no locked base, no adoption.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn point_in_polygon_basic() {
        let tri = [(0, 0), (4, 0), (0, 4)];
        assert!(point_in_polygon(1.0, 1.0, &tri));
        assert!(!point_in_polygon(3.0, 3.0, &tri));
    }

    #[test]
    fn polygon_selection_shapes_mask() {
        let e = Editor::new(6, 6);
        let sel = e.selection_from_polygon(&[(1, 1), (5, 1), (1, 5)]).unwrap();
        assert_eq!((sel.x, sel.y, sel.w, sel.h), (1, 1, 5, 5));
        assert!(sel.selected(0, 0)); // (1,1) inside
        assert!(!sel.selected(4, 4)); // (5,5) outside the triangle
    }

    #[test]
    fn symmetry_points_mirror() {
        let mut e = Editor::new(4, 4);
        e.symmetry = Symmetry::Both;
        let pts = e.symmetry_points(0, 0);
        assert_eq!(pts.len(), 4);
        assert!(pts.contains(&(3, 3)));
        assert!(pts.contains(&(0, 0)));
    }

    #[test]
    fn selection_ops() {
        let mut e = Editor::new(10, 10);
        e.selection = Some(Selection::rect(0, 0, 4, 4));
        e.selection_op = SelectionOp::Add;
        e.apply_selection(Selection::rect(6, 6, 9, 9));
        let sel = e.selection.clone().unwrap();
        assert!(sel.contains(0, 0));
        assert!(sel.contains(9, 9));

        e.selection_op = SelectionOp::Subtract;
        e.apply_selection(Selection::rect(0, 0, 9, 9));
        assert!(e.selection.is_none());
    }

    #[test]
    fn wand_tolerance_matches_close_colors() {
        assert!(color_close(
            Rgba(100, 100, 100, 255),
            Rgba(105, 98, 100, 255),
            8
        ));
        assert!(!color_close(
            Rgba(100, 100, 100, 255),
            Rgba(120, 100, 100, 255),
            8
        ));
        assert!(color_close(Rgba::TRANSPARENT, Rgba::TRANSPARENT, 0));
    }

    #[test]
    fn bucket_fill_can_be_global_and_tolerant() {
        let mut e = Editor::new(4, 1);
        let ink = Rgba::rgb(10, 10, 10);
        e.sprite.set(0, 0, 0, 0, ink);
        e.sprite.set(0, 0, 2, 0, ink);
        e.sprite.set(0, 0, 3, 0, Rgba::rgb(12, 10, 10));
        e.fg = Rgba::rgb(255, 0, 0);
        e.wand_tolerance = 3;
        e.contiguous = false;
        e.apply_bucket(0, 0);
        assert_eq!(e.sprite.get(0, 0, 2, 0), e.fg);
        assert_eq!(e.sprite.get(0, 0, 3, 0), e.fg, "within tolerance");
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba::TRANSPARENT);

        // Contiguous (the default) stops at the transparent gap.
        let mut e = Editor::new(4, 1);
        e.sprite.set(0, 0, 0, 0, ink);
        e.sprite.set(0, 0, 2, 0, ink);
        e.fg = Rgba::rgb(255, 0, 0);
        e.apply_bucket(0, 0);
        assert_eq!(e.sprite.get(0, 0, 0, 0), e.fg);
        assert_eq!(e.sprite.get(0, 0, 2, 0), ink, "not reached");
    }

    #[test]
    fn wand_can_select_non_contiguous_pixels() {
        let mut e = Editor::new(4, 1);
        let ink = Rgba::rgb(10, 10, 10);
        e.sprite.set(0, 0, 0, 0, ink);
        e.sprite.set(0, 0, 2, 0, ink);
        e.contiguous = false;
        e.magic_wand(0, 0);
        let sel = e.current_selection().unwrap();
        assert_eq!((sel.x, sel.w), (0, 3), "bbox spans the gap");
        assert_eq!(sel.mask.iter().filter(|&&m| m).count(), 2);
    }

    #[test]
    fn gradient_and_shade_helpers() {
        assert_eq!(
            lerp_rgba(Rgba(0, 0, 0, 255), Rgba(255, 255, 255, 255), 0.0),
            Rgba(0, 0, 0, 255)
        );
        assert_eq!(
            lerp_rgba(Rgba(0, 0, 0, 255), Rgba(255, 255, 255, 255), 1.0),
            Rgba(255, 255, 255, 255)
        );
        let mid = lerp_rgba(Rgba(0, 0, 0, 255), Rgba(255, 255, 255, 255), 0.5);
        assert!((mid.0 as i32 - 128).abs() <= 1);
        assert!(shade_color(Rgba(100, 100, 100, 255), true).0 < 100);
        assert!(shade_color(Rgba(100, 100, 100, 255), false).0 > 100);
    }

    #[test]
    fn selection_flip_h_moves_and_undoes() {
        let mut e = Editor::new(4, 4);
        e.sprite.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        e.sprite.set(0, 0, 1, 0, Rgba(0, 255, 0, 255));
        e.selection = Some(Selection::rect(0, 0, 1, 0)); // 2x1
        e.flip_h();
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba(0, 255, 0, 255));
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba(255, 0, 0, 255));
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba(0, 255, 0, 255));
    }

    #[test]
    fn selection_rotate_swaps_dims() {
        let mut e = Editor::new(4, 4);
        e.sprite.set(0, 0, 0, 0, Rgba(1, 2, 3, 255));
        e.selection = Some(Selection::rect(0, 0, 1, 2)); // 2w x 3h
        e.rotate_cw();
        assert_eq!(e.sprite.get(0, 0, 2, 0), Rgba(1, 2, 3, 255));
        let sel = e.selection.as_ref().unwrap();
        assert_eq!((sel.w, sel.h), (3, 2));
    }

    #[test]
    fn whole_sprite_transform_undoes() {
        let mut e = Editor::new(3, 2);
        e.sprite.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        e.flip_h();
        assert_eq!(e.sprite.get(0, 0, 2, 0), Rgba(255, 0, 0, 255));
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
        e.redo();
        assert_eq!(e.sprite.get(0, 0, 2, 0), Rgba(255, 0, 0, 255));
    }

    #[test]
    fn whole_sprite_rotate_undo_restores_dims() {
        let mut e = Editor::new(2, 3);
        e.rotate_cw();
        assert_eq!((e.sprite.width, e.sprite.height), (3, 2));
        e.undo();
        assert_eq!((e.sprite.width, e.sprite.height), (2, 3));
    }

    #[test]
    fn tag_restricts_playback() {
        let mut e = Editor::new(1, 1);
        e.add_frame();
        e.add_frame(); // 3 frames
        e.set_tag("walk", Some(1), Some(2));
        assert_eq!(e.play_range(), (1, 2));
        e.active_frame = 1;
        e.advance_frame();
        assert_eq!(e.active_frame, 2);
        e.advance_frame();
        assert_eq!(e.active_frame, 1, "wraps at the tag end");
        e.del_tag(Some("walk"));
        assert_eq!(e.play_range(), (0, 2));
        assert!(e.active_tag.is_none());
    }

    #[test]
    fn text_stamps_the_bitmap_font_inside_the_sprite() {
        let mut e = Editor::new(24, 16);
        e.fg = Rgba::rgb(255, 255, 255);
        e.stamp_text("A", 0, 0);
        let (l, f) = (e.active_layer, e.active_frame);
        let painted = (0..16)
            .flat_map(|y| (0..24).map(move |x| (x, y)))
            .filter(|(x, y)| !e.sprite.get(l, f, *x, *y).is_transparent())
            .count();
        assert!(painted > 10, "A painted {painted} px");
        assert!(
            (0..16).all(|y| (8..24).all(|x| e.sprite.get(l, f, x, y).is_transparent())),
            "the glyph is 8 px wide"
        );
        // Undo takes the whole string out as one step.
        e.undo();
        assert!(
            (0..24).all(|x| e.sprite.get(l, f, x, 0).is_transparent()),
            "one undo removes the stamp"
        );
        // Multi-line and a scale that would run past the edge must not panic.
        e.text_scale = 2;
        e.stamp_text("ab\ncd", 20, 12);
        e.text_scale = 4;
        e.stamp_text("wide", 22, 14);
    }

    #[test]
    fn the_grid_snaps_the_cursor_and_the_pick() {
        let mut e = Editor::new(32, 32);
        e.grid = 8;
        assert_eq!(e.snap_to_grid(13, 21), (13, 21), "snapping is off");
        e.snap = true;
        assert_eq!(e.snap_to_grid(13, 21), (8, 16));
        e.cursor = (8, 16);
        e.move_cursor(1, 0);
        assert_eq!(e.cursor, (16, 16), "a step is one grid cell");
        e.move_cursor(-1, -1);
        assert_eq!(e.cursor, (8, 8));
        e.snap = false;
        e.move_cursor(1, 0);
        assert_eq!(e.cursor, (9, 8), "one pixel when snapping is off");
    }

    #[test]
    fn symmetry_mirrors_about_the_centre_and_rotates_n_fold() {
        let mut e = Editor::new(7, 7);
        // The default centre is the sprite centre: w-1-x, as it always was.
        e.symmetry = Symmetry::Horizontal;
        let mut p = e.symmetry_points(1, 2);
        p.sort();
        assert_eq!(p, vec![(1, 2), (5, 2)]);

        // Moved centre: mirror about x=3.
        e.sym_center = Some((3, 3));
        let mut p = e.symmetry_points(1, 2);
        p.sort();
        assert_eq!(p, vec![(1, 2), (5, 2)]);

        // Radial, 4 axes, one point right of the centre: four points, no
        // duplicates from the rounding.
        e.symmetry = Symmetry::Radial;
        e.sym_axes = 4;
        let mut p = e.symmetry_points(4, 3);
        p.sort();
        assert_eq!(p, vec![(2, 3), (3, 2), (3, 4), (4, 3)]);

        // A 8-axis rotation is still deduplicated, and stays in bounds.
        e.sym_axes = 8;
        for x in 0..7 {
            for y in 0..7 {
                let pts = e.symmetry_points(x, y);
                assert!(pts.len() <= 8);
                assert!(pts.iter().all(|(x, y)| *x < 7 && *y < 7));
            }
        }
    }

    #[test]
    fn palette_reorder_keeps_fg_and_bg_on_their_colours() {
        let mut e = Editor::new(1, 1);
        let (a, b, c) = (Rgba::rgb(1, 2, 3), Rgba::rgb(4, 5, 6), Rgba::rgb(7, 8, 9));
        e.load_palette(vec![a, b, c]);
        e.fg_idx = 2;
        e.bg_idx = 0;
        e.move_palette(2, 0);
        assert_eq!(e.palette, vec![c, a, b]);
        assert_eq!(e.palette[e.fg_idx], c, "fg follows its colour");
        assert_eq!(e.palette[e.bg_idx], a, "bg follows its colour");
        e.move_palette(0, 1);
        assert_eq!(e.palette, vec![a, c, b]);
        assert_eq!(e.palette[e.fg_idx], c);
    }

    #[test]
    fn layers_move_by_drag() {
        let mut e = Editor::new(1, 1);
        e.add_layer();
        e.add_layer();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            e.sprite.layers[i].name = name.to_string();
        }
        // Dropping a row onto a later row lands it after that row.
        e.move_layer_to(0, 2);
        assert_eq!(names(&e), ["b", "c", "a"]);
        assert_eq!(e.active_layer, 2);
        // Dropping onto an earlier row lands it at that row.
        e.move_layer_to(2, 0);
        assert_eq!(names(&e), ["a", "b", "c"]);
        assert_eq!(e.active_layer, 0);
    }

    fn names(e: &Editor) -> Vec<&str> {
        e.sprite.layers.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn playback_can_stop_or_bounce() {
        let mut e = Editor::new(1, 1);
        e.add_frame();
        e.add_frame(); // 3 frames: 0,1,2
        assert_eq!(e.play_range(), (0, 2));

        // Loop (the default) wraps; no-loop stops on the last frame.
        e.play_loop = false;
        e.active_frame = 1;
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 2);
        assert!(!e.advance_frame(), "a loop-less clip ends");
        assert_eq!(e.active_frame, 2);

        // Ping-pong: 2 is the last frame, so the next step goes back to 1.
        e.play_pingpong = true;
        e.play_dir = 1;
        e.active_frame = 1;
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 2);
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 1, "bounces off the end");
        assert_eq!(e.play_dir, -1);
        e.active_frame = 0;
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 1, "bounces off the start");
        assert_eq!(e.play_dir, 1);
    }

    #[test]
    fn tile_mode_wraps_cursor_and_paints_across_edges() {
        let mut e = Editor::new(4, 3);
        e.set_tile_mode(Some(true));
        e.move_cursor(-1, -1);
        assert_eq!(e.cursor, (3, 2));
        e.move_cursor(1, 0);
        assert_eq!(e.cursor, (0, 2));
        // Painting at the left edge is a normal in-bounds write.
        e.paint_pixel(0, 2, Rgba(9, 9, 9, 255));
        assert_eq!(e.sprite.get(0, 0, 0, 2), Rgba(9, 9, 9, 255));

        e.set_tile_mode(Some(false));
        e.cursor = (0, 0);
        e.move_cursor(-1, -1);
        assert_eq!(e.cursor, (0, 0), "clamped when tile mode is off");
    }

    #[test]
    fn palette_from_sprite_takes_the_frame_colours_most_used_first() {
        let mut e = Editor::new(3, 2);
        let a = Rgba(10, 20, 30, 255);
        let b = Rgba(200, 100, 50, 255);
        for x in 0..3 {
            e.sprite.set(0, 0, x, 0, a);
        }
        e.sprite.set(0, 0, 0, 1, b);
        assert_eq!(e.palette_from_sprite(), 2);
        assert_eq!(e.palette, vec![a, b]);
        assert!(e.palette.contains(&e.fg), "fg stays inside the palette");
        // A frame with nothing on it leaves the palette alone rather than
        // emptying it (paint_pixel needs at least one colour).
        let mut blank = Editor::new(2, 2);
        let before = blank.palette.clone();
        assert_eq!(blank.palette_from_sprite(), 0);
        assert_eq!(blank.palette, before);
    }

    #[test]
    fn a_locked_palette_refuses_a_stray_colour_but_still_erases() {
        let mut e = Editor::new(2, 2);
        let in_ramp = Rgba(10, 20, 30, 255);
        let stray = Rgba(1, 2, 3, 255);
        e.load_palette(vec![in_ramp]);
        e.sprite.set(0, 0, 0, 0, in_ramp);
        e.set_palette_lock(true);

        e.paint_pixel(1, 1, stray);
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::TRANSPARENT);
        assert!(e.msg.contains("palette locked"));

        e.paint_pixel(1, 1, in_ramp);
        assert_eq!(e.sprite.get(0, 0, 1, 1), in_ramp);
        e.paint_pixel(0, 0, Rgba::TRANSPARENT);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::TRANSPARENT, "eraser wins");

        e.set_palette_lock(false);
        e.paint_pixel(1, 2 - 1, stray);
        assert_eq!(e.sprite.get(0, 0, 1, 1), stray);
    }

    #[test]
    fn structural_edits_undo_and_redo_the_layer_and_frame_tree() {
        let mut e = Editor::new(4, 4);
        let layers = e.sprite.layers.len();

        e.add_layer();
        assert_eq!(e.sprite.layers.len(), layers + 1);
        assert_eq!(e.active_layer, 1);
        e.undo();
        assert_eq!(e.sprite.layers.len(), layers);
        assert_eq!(e.active_layer, 0, "the active layer travels with the tree");
        e.redo();
        assert_eq!(e.sprite.layers.len(), layers + 1);
        assert_eq!(e.active_layer, 1);

        // Deleting a frame clamps the active index; undo brings it back.
        e.add_frame();
        assert_eq!(e.active_frame, 1);
        e.del_frame();
        assert_eq!(e.sprite.frames.len(), 1);
        e.undo();
        assert_eq!(e.sprite.frames.len(), 2);
        assert_eq!(e.active_frame, 1, "the deleted frame's index is restored");
        e.redo();
        assert_eq!(e.sprite.frames.len(), 1);
        assert_eq!(e.active_frame, 0);

        // A refused delete is not an undo step.
        let steps = e.undo.len();
        e.del_frame();
        assert_eq!(e.undo.len(), steps, "deleting the last frame is a no-op");
    }

    #[test]
    fn structural_edits_cover_tags_renames_moves_and_locks() {
        let mut e = Editor::new(4, 4);
        e.add_frame();

        e.set_tag("walk", Some(0), Some(1));
        assert_eq!(e.sprite.tags.len(), 1);
        assert_eq!(e.active_tag, Some(0));
        e.undo();
        assert!(e.sprite.tags.is_empty());
        assert_eq!(e.active_tag, None);
        e.redo();
        assert_eq!(e.sprite.tags.len(), 1);
        assert_eq!(e.active_tag, Some(0));

        e.del_tag(Some("walk"));
        assert!(e.sprite.tags.is_empty());
        e.undo();
        assert_eq!(e.sprite.tags.len(), 1, "the tag comes back");
        assert_eq!(e.active_tag, Some(0));

        let name = e.sprite.layers[0].name.clone();
        e.rename_layer("ink");
        assert_eq!(e.sprite.layers[0].name, "ink");
        e.undo();
        assert_eq!(e.sprite.layers[0].name, name);

        e.add_layer();
        e.active_layer = 0;
        let first = e.sprite.layers[0].name.clone();
        e.move_layer_active(1);
        assert_eq!(e.active_layer, 1);
        assert_eq!(e.sprite.layers[1].name, first);
        e.undo();
        assert_eq!(e.active_layer, 0);
        assert_eq!(e.sprite.layers[0].name, first);

        e.set_layer_editable(false);
        assert!(!e.sprite.layers[e.active_layer].editable);
        e.undo();
        assert!(e.sprite.layers[e.active_layer].editable);
    }

    #[test]
    fn layer_properties_are_one_undo_step_each() {
        use crate::sprite::BlendMode;
        let mut e = Editor::new(4, 4);
        let before = e.layer_props(0);
        e.sprite.layers[0].visible = false;
        e.sprite.layers[0].opacity = 90;
        e.sprite.layers[0].blend = BlendMode::Multiply;
        e.record_layer_props(0, before);
        assert_eq!(e.undo.len(), 1);

        e.undo();
        let p = e.layer_props(0);
        assert!(p.visible && p.opacity == 255 && p.blend == BlendMode::Normal);
        e.redo();
        let p = e.layer_props(0);
        assert!(!p.visible);
        assert_eq!(p.opacity, 90);
        assert_eq!(p.blend, BlendMode::Multiply);

        // Recording the values that are already there is not an undo step.
        let steps = e.undo.len();
        let now = e.layer_props(0);
        e.record_layer_props(0, now);
        assert_eq!(e.undo.len(), steps);
    }

    #[test]
    fn bucket_fill_reaches_the_mirrored_seed() {
        let mut e = Editor::new(8, 8);
        // A wall at x=4 splits the canvas into two regions.
        let wall = Rgba::rgb(0, 0, 0);
        for y in 0..8 {
            e.sprite.set(0, 0, 4, y, wall);
        }
        e.set_symmetry("h");
        e.fg = Rgba::rgb(200, 10, 10);
        e.apply_bucket(0, 0);
        assert_eq!(e.sprite.get(0, 0, 0, 0), e.fg, "the seed region fills");
        assert_eq!(
            e.sprite.get(0, 0, 7, 7),
            e.fg,
            "the mirrored region fills too"
        );
        assert_eq!(e.sprite.get(0, 0, 4, 0), wall, "the wall stays");
    }

    #[test]
    fn tag_direction_and_repeat_drive_playback() {
        use crate::sprite::PlayDirection;
        let mut e = Editor::new(1, 1);
        e.add_frame();
        e.add_frame();
        e.set_tag("walk", Some(0), Some(2));

        e.set_tag_direction(PlayDirection::Reverse);
        e.active_frame = 2;
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 1);
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 0);
        assert!(e.advance_frame(), "reverse wraps to the far end");
        assert_eq!(e.active_frame, 2);

        e.set_tag_direction(PlayDirection::Forward);
        e.set_tag_repeat(1);
        e.start_playback();
        e.active_frame = 1;
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 2);
        assert!(!e.advance_frame(), "repeat 1 stops after one pass");

        e.set_tag_repeat(0);
        e.set_tag_direction(PlayDirection::PingPong);
        e.start_playback();
        e.active_frame = 2;
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 1);
        assert_eq!(e.play_dir, -1);
    }

    #[test]
    fn recent_files_move_to_the_front_and_cap() {
        let mut e = Editor::new(1, 1);
        for i in 0..12 {
            e.note_recent(PathBuf::from(format!("/tmp/f{i}.vpx")));
        }
        assert_eq!(e.recent.len(), 10);
        assert_eq!(e.recent[0], PathBuf::from("/tmp/f11.vpx"));
        e.note_recent(PathBuf::from("/tmp/f5.vpx"));
        assert_eq!(e.recent[0], PathBuf::from("/tmp/f5.vpx"));
        assert_eq!(e.recent.iter().filter(|p| p.ends_with("f5.vpx")).count(), 1);
    }

    #[test]
    fn pixel_perfect_drops_the_corner_pixel() {
        let mut e = Editor::new(4, 4);
        e.pixel_perfect = true;
        e.begin_stroke();
        e.paint_with_tool(0, 0);
        e.paint_with_tool(1, 0);
        e.paint_with_tool(1, 1);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 0, 0), e.fg);
        assert_eq!(e.sprite.get(0, 0, 1, 1), e.fg);
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba::TRANSPARENT, "corner");
        e.undo();
        assert!(e.sprite.get(0, 0, 0, 0).is_transparent());
        assert!(e.sprite.get(0, 0, 1, 1).is_transparent());

        // A true diagonal keeps every pixel.
        let mut e = Editor::new(4, 4);
        e.pixel_perfect = true;
        e.begin_stroke();
        e.paint_with_tool(0, 0);
        e.paint_with_tool(1, 1);
        e.paint_with_tool(2, 2);
        e.end_stroke();
        for i in 0..3 {
            assert_eq!(e.sprite.get(0, 0, i, i), e.fg, "({i},{i})");
        }
    }

    #[test]
    fn reference_layers_are_not_exported_or_painted() {
        let mut e = Editor::new(1, 1);
        e.paint_pixel(0, 0, Rgba::rgb(255, 0, 0));
        e.add_layer();
        e.active_layer = 1;
        e.sprite.set(1, 0, 0, 0, Rgba::rgb(0, 0, 255));
        e.set_layer_reference(true);
        assert!(e.sprite.layers[1].reference);

        // The canvas shows the reference; an export does not.
        assert_eq!(e.sprite.composite(0)[0], Rgba::rgb(255, 0, 0));
        e.sprite.layers[0].visible = false;
        assert_eq!(e.sprite.composite(0)[0], Rgba::rgb(0, 0, 255));
        assert_eq!(e.sprite.export_composite(0)[0], Rgba::TRANSPARENT);

        // Reference layers refuse paint.
        e.paint_pixel(0, 0, Rgba::rgb(9, 9, 9));
        assert_eq!(e.sprite.get(1, 0, 0, 0), Rgba::rgb(0, 0, 255));
        assert!(e.msg.contains("locked or hidden"), "{}", e.msg);
    }

    #[test]
    fn background_layers_are_opaque_and_undoable() {
        let mut e = Editor::new(1, 1);
        e.sprite.layers[0].opacity = 40;
        e.set_layer_background(true);
        assert!(e.sprite.layers[0].background);
        assert_eq!(e.sprite.layers[0].opacity, 255);
        e.undo();
        assert!(!e.sprite.layers[0].background);
        assert_eq!(e.sprite.layers[0].opacity, 40);
    }

    #[test]
    fn layer_duplicate_merge_and_flatten() {
        // Duplicate copies the pixels, and the two cels are independent.
        let mut e = Editor::new(2, 2);
        e.paint_pixel(0, 0, Rgba::rgb(10, 20, 30));
        e.duplicate_layer();
        assert_eq!(e.sprite.layers.len(), 2);
        assert_eq!(e.active_layer, 0);
        assert_eq!(e.sprite.get(1, 0, 0, 0), Rgba::rgb(10, 20, 30));
        e.paint_pixel(1, 1, Rgba::rgb(9, 9, 9));
        assert_eq!(e.sprite.get(1, 0, 1, 1), Rgba::TRANSPARENT);
        e.undo(); // the paint
        e.undo(); // the duplicate
        assert_eq!(e.sprite.layers.len(), 1);

        // Merge down bakes the upper layer into the lower one.
        let mut e = Editor::new(1, 1);
        e.paint_pixel(0, 0, Rgba::rgb(255, 0, 0));
        e.add_layer();
        e.sprite.set(1, 0, 0, 0, Rgba::rgb(0, 0, 255));
        e.active_layer = 0;
        e.merge_down();
        assert_eq!(e.sprite.layers.len(), 1);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        e.undo();
        assert_eq!(e.sprite.layers.len(), 2);

        // Flatten collapses the visible layers into one.
        let mut e = Editor::new(1, 1);
        e.paint_pixel(0, 0, Rgba::rgb(255, 0, 0));
        e.add_layer();
        e.sprite.set(1, 0, 0, 0, Rgba::rgb(0, 0, 255));
        e.flatten();
        assert_eq!(e.sprite.layers.len(), 1);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        e.undo();
        assert_eq!(e.sprite.layers.len(), 2);
    }

    #[test]
    fn crop_and_trim_resize_every_cel() {
        let mut e = Editor::new(6, 6);
        e.paint_pixel(2, 3, Rgba::rgb(9, 9, 9));
        e.paint_pixel(4, 4, Rgba::rgb(8, 8, 8));
        e.trim();
        assert_eq!((e.sprite.width, e.sprite.height), (3, 2));
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(9, 9, 9));
        assert_eq!(e.sprite.get(0, 0, 2, 1), Rgba::rgb(8, 8, 8));
        e.undo();
        assert_eq!((e.sprite.width, e.sprite.height), (6, 6));
        assert_eq!(e.sprite.get(0, 0, 2, 3), Rgba::rgb(9, 9, 9));

        // Selection crop, and the layer mask follows the same offset.
        e.selection = Some(Selection::rect(0, 0, 2, 2));
        e.mask_add();
        e.selection = Some(Selection::rect(1, 2, 3, 4));
        e.crop_to_selection();
        assert_eq!((e.sprite.width, e.sprite.height), (3, 3));
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::rgb(9, 9, 9), "2,3 -> 1,1");
        assert_eq!(e.sprite.layers[0].mask.as_ref().unwrap().pixels.len(), 9);
    }

    #[test]
    fn the_brush_paints_a_square_and_clips_at_the_edge() {
        let mut e = Editor::new(5, 5);
        e.brush_size = 3;
        e.tool = Tool::Pencil;
        e.begin_stroke();
        e.paint_with_tool(2, 2);
        e.end_stroke();
        for y in 1..=3 {
            for x in 1..=3 {
                assert_eq!(e.sprite.get(0, 0, x, y), e.fg, "({x},{y})");
            }
        }
        assert!(e.sprite.get(0, 0, 0, 0).is_transparent());

        // At the corner the square is clipped, not wrapped or panicked.
        e.begin_stroke();
        e.paint_with_tool(0, 0);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 0, 0), e.fg);
        assert_eq!(e.sprite.get(0, 0, 1, 1), e.fg);
    }

    #[test]
    fn merged_copy_flattens_the_visible_layers() {
        let mut e = Editor::new(4, 4);
        e.add_layer(); // inserts below (index 1) and activates it
        e.paint_pixel(1, 1, Rgba::rgb(200, 10, 10));
        e.active_layer = 0; // top layer
        e.paint_pixel(2, 2, Rgba::rgb(10, 200, 10));
        e.selection = Some(Selection::rect(1, 1, 2, 2));

        e.yank_merged();
        let cb = e.clipboard.clone().unwrap();
        assert_eq!((cb.w, cb.h), (2, 2));
        assert_eq!(
            cb.pixels[0],
            Rgba::rgb(200, 10, 10),
            "the lower layer shows through"
        );
        assert_eq!(cb.pixels[3], Rgba::rgb(10, 200, 10), "the top layer wins");
        assert_eq!(cb.pixels[1], Rgba::TRANSPARENT);

        // A plain copy still takes only the active layer.
        e.selection = Some(Selection::rect(1, 1, 2, 2));
        e.yank();
        let cb = e.clipboard.unwrap();
        assert_eq!(cb.pixels[0], Rgba::TRANSPARENT);
    }

    #[test]
    fn tile_mode_wraps_a_line_across_the_seam() {
        let mut e = Editor::new(8, 4);
        e.set_tile_mode(Some(true));
        e.pending = Some(Pending {
            shape: Shape::Line,
            start: (6, 1),
        });
        e.cursor = (1, 1);
        let ink = Rgba::rgb(9, 9, 9);
        e.commit_shape(ink);
        for x in [6, 7, 0, 1] {
            assert_eq!(e.sprite.get(0, 0, x, 1), ink, "x={x}");
        }
        assert!(
            e.sprite.get(0, 0, 3, 1).is_transparent(),
            "the seam is the short way, not across the canvas"
        );
    }

    #[test]
    fn tile_mode_gradient_follows_the_short_way() {
        let mut e = Editor::new(8, 1);
        e.set_tile_mode(Some(true));
        e.fg = Rgba::rgb(255, 255, 255);
        e.bg = Rgba::rgb(0, 0, 0);
        e.cursor = (1, 0);
        e.paint_gradient((6, 0));
        // The ramp runs 6→7→0→1, so x=4 is still at the fg end.
        assert_eq!(e.sprite.get(0, 0, 4, 0), e.fg);
        assert_eq!(e.sprite.get(0, 0, 1, 0), e.bg);
    }

    #[test]
    fn edits_bump_the_thumbnail_revision() {
        let mut e = Editor::new(2, 2);
        let r = e.revision;
        e.paint_pixel(0, 0, Rgba::rgb(1, 2, 3));
        assert!(e.revision > r, "painting invalidates the thumbnails");
        let r = e.revision;
        e.undo();
        assert!(e.revision > r, "undo invalidates them too");
        let r = e.revision;
        e.add_layer();
        assert!(e.revision > r);
    }

    #[test]
    fn paint_tools_are_clipped_to_the_selection() {
        let mut e = Editor::new(8, 8);
        e.selection = Some(Selection::rect(2, 2, 5, 5));

        e.begin_stroke();
        e.paint_with_tool(0, 0);
        e.paint_with_tool(3, 3);
        e.end_stroke();
        assert!(e.sprite.get(0, 0, 0, 0).is_transparent());
        assert_eq!(e.sprite.get(0, 0, 3, 3), e.fg);

        // A line crossing the selection only paints inside it.
        let ink = Rgba::rgb(9, 9, 9);
        e.pending = Some(Pending {
            shape: Shape::Line,
            start: (0, 0),
        });
        e.cursor = (7, 7);
        e.commit_shape(ink);
        for (x, y) in [(0, 0), (1, 1), (6, 6), (7, 7)] {
            assert!(e.sprite.get(0, 0, x, y).is_transparent(), "({x},{y})");
        }
        assert_eq!(e.sprite.get(0, 0, 3, 3), ink);

        // The brush mirrors the clip per stamped pixel.
        let mut b = Editor::new(8, 8);
        b.selection = Some(Selection::rect(0, 0, 3, 7));
        b.brush_size = 3;
        b.symmetry = Symmetry::Horizontal;
        b.begin_stroke();
        b.paint_with_tool(1, 1);
        b.end_stroke();
        assert_eq!(b.sprite.get(0, 0, 1, 1), b.fg);
        assert!(b.sprite.get(0, 0, 6, 1).is_transparent(), "mirror clipped");
    }

    #[test]
    fn bucket_fill_stops_at_the_selection() {
        let ink = Rgba::rgb(10, 10, 10);
        let mut e = Editor::new(8, 1);
        for x in 0..8 {
            e.sprite.set(0, 0, x, 0, ink);
        }
        e.selection = Some(Selection::rect(2, 0, 4, 0));
        e.fg = Rgba::rgb(200, 0, 0);
        e.apply_bucket(2, 0);
        for x in 2..=4 {
            assert_eq!(e.sprite.get(0, 0, x, 0), e.fg, "x{x}");
        }
        for x in [0, 1, 5, 6, 7] {
            assert_eq!(e.sprite.get(0, 0, x, 0), ink, "x{x}");
        }

        // Global mode still honours the clip.
        let mut g = Editor::new(8, 1);
        for x in 0..8 {
            g.sprite.set(0, 0, x, 0, ink);
        }
        g.selection = Some(Selection::rect(2, 0, 4, 0));
        g.fg = Rgba::rgb(0, 0, 200);
        g.contiguous = false;
        g.apply_bucket(2, 0);
        for x in 2..=4 {
            assert_eq!(g.sprite.get(0, 0, x, 0), g.fg, "x{x}");
        }
        assert_eq!(g.sprite.get(0, 0, 0, 0), ink);
        assert_eq!(g.sprite.get(0, 0, 7, 0), ink);
    }

    #[test]
    fn bucket_seed_outside_the_selection_does_nothing() {
        let ink = Rgba::rgb(10, 10, 10);
        let mut e = Editor::new(4, 1);
        for x in 0..4 {
            e.sprite.set(0, 0, x, 0, ink);
        }
        e.selection = Some(Selection::rect(1, 0, 2, 0));
        e.fg = Rgba::rgb(1, 2, 3);
        e.apply_bucket(0, 0);
        for x in 0..4 {
            assert_eq!(e.sprite.get(0, 0, x, 0), ink);
        }
    }

    #[test]
    fn a_fully_clipped_stroke_records_no_undo_step() {
        let mut e = Editor::new(8, 8);
        e.selection = Some(Selection::rect(4, 4, 7, 7));
        e.begin_stroke();
        e.paint_with_tool(0, 0);
        e.paint_with_tool(1, 1);
        e.end_stroke();
        assert!(e.undo.is_empty(), "nothing changed, nothing to undo");
    }

    #[test]
    fn text_and_bounds_respect_the_selection() {
        let mut e = Editor::new(16, 16);
        e.selection = Some(Selection::rect(12, 12, 15, 15));
        e.stamp_text("A", 0, 0);
        for x in 0..8 {
            for y in 0..8 {
                assert!(e.sprite.get(0, 0, x, y).is_transparent(), "({x},{y})");
            }
        }
        // Out-of-bounds paint is a no-op instead of a panic.
        e.paint_pixel(16, 0, Rgba::rgb(1, 2, 3));
        e.paint_pixel(0, 16, Rgba::rgb(1, 2, 3));
    }

    #[test]
    fn paste_and_contour_own_their_mask() {
        let mut e = Editor::new(8, 8);
        e.selection = Some(Selection::rect(0, 0, 1, 1));
        e.clipboard = Some(Clipboard {
            w: 1,
            h: 1,
            pixels: vec![Rgba::rgb(5, 6, 7)],
            mask: vec![true],
        });
        e.paste_pos = Some((4, 4));
        e.commit_paste();
        assert_eq!(
            e.sprite.get(0, 0, 4, 4),
            Rgba::rgb(5, 6, 7),
            "paste replaces the selection"
        );

        let mut c = Editor::new(8, 8);
        c.selection = Some(Selection::rect(2, 2, 4, 4));
        c.fg = Rgba::rgb(9, 9, 9);
        c.contour_at(2, 2);
        assert_eq!(
            c.sprite.get(0, 0, 1, 2),
            c.fg,
            "the ring is outside the selection"
        );
        assert!(c.sprite.get(0, 0, 2, 2).is_transparent());
    }

    #[test]
    fn paste_mirrors_each_pixel() {
        let mut e = Editor::new(8, 8);
        e.set_symmetry("h");
        e.clipboard = Some(Clipboard {
            w: 1,
            h: 1,
            pixels: vec![Rgba::rgb(5, 6, 7)],
            mask: vec![true],
        });
        e.paste_pos = Some((0, 0));
        e.commit_paste();
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(5, 6, 7));
        assert_eq!(e.sprite.get(0, 0, 7, 0), Rgba::rgb(5, 6, 7));
    }
}
