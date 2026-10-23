use std::path::PathBuf;

use crate::color::Rgba;
use crate::sprite::{Sprite, ellipse_points, flood_fill, line_points, rect_filled_points, rect_points};

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
}

impl Symmetry {
    pub fn name(self) -> &'static str {
        match self {
            Symmetry::None => "none",
            Symmetry::Horizontal => "horizontal",
            Symmetry::Vertical => "vertical",
            Symmetry::Both => "both",
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

/// One undoable change.
pub enum UndoStep {
    /// Per-pixel writes, each `(layer, frame, x, y, before, after)`.
    Cells {
        cells: Vec<(usize, usize, u32, u32, Rgba, Rgba)>,
    },
    /// Whole-sprite snapshot (for whole-sprite transforms).
    Snapshot { before: Sprite, after: Sprite },
}

/// The sprite model plus the interaction state (modes, tools, undo, colors).
/// All view state (zoom/pan) lives in the GUI layer, not here.
pub struct Editor {
    pub sprite: Sprite,
    pub cursor: (u32, u32),
    pub mode: Mode,
    pub tool: Tool,
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
    pub lasso_points: Vec<(u32, u32)>,
    pub selection_op: SelectionOp,
    pub filled_shapes: bool,
    pub gradient_radial: bool,
    pub spray_seed: u64,
    pub wand_tolerance: u8,
    pub wand_all_layers: bool,
    /// Show the sprite tiled 3x3 and wrap the cursor across its edges.
    pub tile_mode: bool,
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
    pub should_quit: bool,
    pub g_prefix: bool,
    pub path: Option<PathBuf>,
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
            fg,
            bg,
            palette,
            fg_idx,
            bg_idx: 0,
            active_layer: 0,
            active_frame: 0,
            active_tag: None,
            symmetry: Symmetry::None,
            lasso_points: Vec::new(),
            selection_op: SelectionOp::Replace,
            filled_shapes: false,
            gradient_radial: false,
            spray_seed: 0x9E37_79B9_7F4A_7C15,
            wand_tolerance: 0,
            wand_all_layers: false,
            tile_mode: false,
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
            should_quit: false,
            g_prefix: false,
            path: None,
        }
    }

    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        let w = (self.sprite.width as i32).max(1);
        let h = (self.sprite.height as i32).max(1);
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

    fn record(&mut self, layer: usize, frame: usize, x: u32, y: u32, old: Rgba, new: Rgba) {
        if let Some(UndoStep::Cells { cells }) = &mut self.stroke {
            cells.push((layer, frame, x, y, old, new));
        } else {
            self.undo.push(UndoStep::Cells {
                cells: vec![(layer, frame, x, y, old, new)],
            });
            self.redo.clear();
        }
    }

    pub fn begin_stroke(&mut self) {
        self.stroke = Some(UndoStep::Cells { cells: Vec::new() });
    }

    pub fn end_stroke(&mut self) {
        if let Some(s) = self.stroke.take() {
            let empty = match &s {
                UndoStep::Cells { cells } => cells.is_empty(),
                UndoStep::Snapshot { .. } => false,
            };
            if !empty {
                self.undo.push(s);
                self.redo.clear();
            }
        }
    }

    pub fn paint_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        let (l, f) = (self.active_layer, self.active_frame);
        if !self.sprite.layer_editable(l) {
            self.msg = "layer is locked or hidden".into();
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
            Tool::Eraser => self.paint_symmetric(x, y, Rgba::TRANSPARENT),
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
            if px >= 0 && py >= 0 && (px as u32) < self.sprite.width && (py as u32) < self.sprite.height
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
                    self.paint_pixel(x, y, color);
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
        let (bx, by) = (self.cursor.0 as f32 + 0.5, self.cursor.1 as f32 + 0.5);
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
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
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

    /// Paint `color` at `(x, y)` and at every mirrored symmetry point.
    pub fn paint_symmetric(&mut self, x: u32, y: u32, color: Rgba) {
        for (px, py) in self.symmetry_points(x, y) {
            self.paint_pixel(px, py, color);
        }
    }

    fn symmetry_points(&self, x: u32, y: u32) -> Vec<(u32, u32)> {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let mut pts = vec![(x, y)];
        match self.symmetry {
            Symmetry::None => {}
            Symmetry::Horizontal => pts.push((w - 1 - x, y)),
            Symmetry::Vertical => pts.push((x, h - 1 - y)),
            Symmetry::Both => {
                pts.push((w - 1 - x, y));
                pts.push((x, h - 1 - y));
                pts.push((w - 1 - x, h - 1 - y));
            }
        }
        pts
    }

    pub fn cycle_symmetry(&mut self) {
        self.symmetry = match self.symmetry {
            Symmetry::None => Symmetry::Horizontal,
            Symmetry::Horizontal => Symmetry::Vertical,
            Symmetry::Vertical => Symmetry::Both,
            Symmetry::Both => Symmetry::None,
        };
    }

    pub fn set_symmetry(&mut self, name: &str) -> bool {
        self.symmetry = match name {
            "none" | "off" => Symmetry::None,
            "horizontal" | "h" | "x" => Symmetry::Horizontal,
            "vertical" | "v" | "y" => Symmetry::Vertical,
            "both" | "b" => Symmetry::Both,
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
        self.begin_stroke();
        let changes = flood_fill(&mut self.sprite, l, f, x, y, color);
        let n = changes.len();
        if let Some(UndoStep::Cells { cells }) = &mut self.stroke {
            cells.extend(changes.into_iter().map(|(x, y, old, new)| (l, f, x, y, old, new)));
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
        let pts = match p.shape {
            Shape::Line => line_points(
                p.start.0 as i32,
                p.start.1 as i32,
                self.cursor.0 as i32,
                self.cursor.1 as i32,
            ),
            Shape::Rect if filled => rect_filled_points(
                p.start.0 as i32,
                p.start.1 as i32,
                self.cursor.0 as i32,
                self.cursor.1 as i32,
            ),
            Shape::Rect => rect_points(
                p.start.0 as i32,
                p.start.1 as i32,
                self.cursor.0 as i32,
                self.cursor.1 as i32,
            ),
            Shape::Ellipse => ellipse_points(
                p.start.0 as i32,
                p.start.1 as i32,
                self.cursor.0 as i32,
                self.cursor.1 as i32,
                filled,
            ),
            Shape::Gradient => unreachable!(),
        };
        self.begin_stroke();
        for (x, y) in pts {
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
                    for &(l, f, x, y, before, _) in cells {
                        self.sprite.set(l, f, x, y, before);
                    }
                }
                UndoStep::Snapshot { before, .. } => {
                    self.sprite = before.clone();
                    self.clamp_cursor();
                }
            }
            self.redo.push(step);
            self.dirty = true;
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
                    self.sprite = after.clone();
                    self.clamp_cursor();
                }
            }
            self.undo.push(step);
            self.dirty = true;
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

    pub fn load_palette(&mut self, colors: Vec<Rgba>) {
        if colors.is_empty() {
            return;
        }
        self.palette = colors;
        self.fg_idx = self.fg_idx.min(self.palette.len() - 1);
        self.bg_idx = self.bg_idx.min(self.palette.len() - 1);
        self.fg = self.palette[self.fg_idx];
        self.bg = self.palette[self.bg_idx];
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
        let idx = self.active_layer;
        self.sprite.add_layer_after(idx);
        self.active_layer = idx + 1;
        self.dirty = true;
    }

    pub fn del_layer(&mut self) {
        if self.sprite.remove_layer(self.active_layer) {
            if self.active_layer >= self.sprite.layers.len() {
                self.active_layer = self.sprite.layers.len() - 1;
            }
            self.dirty = true;
        } else {
            self.msg = "cannot delete last layer".into();
        }
    }

    pub fn add_frame(&mut self) {
        let idx = self.active_frame;
        self.sprite.add_frame_after(idx);
        self.active_frame = idx + 1;
        self.dirty = true;
    }

    pub fn del_frame(&mut self) {
        if self.sprite.remove_frame(self.active_frame) {
            if self.active_frame >= self.sprite.frames.len() {
                self.active_frame = self.sprite.frames.len() - 1;
            }
            if self.active_tag.is_some_and(|i| i >= self.sprite.tags.len()) {
                self.active_tag = None;
            }
            self.dirty = true;
        } else {
            self.msg = "cannot delete last frame".into();
        }
    }

    /// Move a frame (drag-and-drop in the timeline).
    pub fn move_frame(&mut self, from: usize, to: usize) {
        if self.sprite.move_frame(from, to) {
            if self.active_frame == from {
                self.active_frame = to;
            }
            self.dirty = true;
        }
    }

    /// Create or update a tag over the inclusive frame range.
    pub fn set_tag(&mut self, name: &str, from: Option<usize>, to: Option<usize>) {
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
                self.sprite
                    .tags
                    .push(crate::sprite::Tag {
                        name: name.to_string(),
                        from: a,
                        to: b,
                        color: Rgba(120, 170, 255, 255),
                    });
                self.sprite.tags.len() - 1
            }
        };
        self.active_tag = Some(idx);
        self.msg = format!("tag '{name}': frames {a}-{b}");
        self.dirty = true;
    }

    /// Delete a tag by name, or the active tag when no name is given.
    pub fn del_tag(&mut self, name: Option<&str>) {
        let idx = match name {
            Some(n) => self.sprite.tags.iter().position(|t| t.name == n),
            None => self.active_tag,
        };
        if let Some(i) = idx {
            self.sprite.tags.remove(i);
            self.active_tag = None;
            self.dirty = true;
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

    /// Step to the next playback frame, wrapping inside the active tag.
    pub fn advance_frame(&mut self) {
        let (a, b) = self.play_range();
        self.active_frame = if self.active_frame < a || self.active_frame >= b {
            a
        } else {
            self.active_frame + 1
        };
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.sprite.resize(w, h);
        self.cursor = (self.cursor.0.min(w - 1), self.cursor.1.min(h - 1));
        self.dirty = true;
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

    /// Select the contiguous region around `(x, y)` matching within `wand_tolerance`.
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

    /// Copy the selection to the clipboard (active layer).
    pub fn yank(&mut self) {
        if let Some(sel) = self.current_selection() {
            self.clipboard = Some(self.selection_to_clipboard(&sel));
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
                            self.paint_pixel(pos.0 + xx, pos.1 + yy, color);
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
        let from = self.active_layer;
        let to = if dir > 0 {
            self.sprite.subtree_end(from)
        } else {
            from.saturating_sub(1)
        };
        if let Some(at) = self.sprite.move_layer(from, to) {
            self.active_layer = at;
            self.dirty = true;
        }
    }

    pub fn add_group(&mut self) {
        let idx = self.active_layer;
        self.sprite.add_group_after(idx);
        self.active_layer = idx + 1;
        self.dirty = true;
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
        let end = self.sprite.subtree_end(i);
        for layer in &mut self.sprite.layers[i..end] {
            layer.child_level = (layer.child_level as i32 + delta) as u16;
        }
        self.dirty = true;
    }

    /// Mask ops are whole-sprite snapshots so they undo as one step.
    fn snapshot(&mut self, before: Sprite) {
        self.undo.push(UndoStep::Snapshot {
            before,
            after: self.sprite.clone(),
        });
        self.redo.clear();
        self.dirty = true;
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
        self.sprite.layers[self.active_layer].name = name.to_string();
        self.dirty = true;
    }

    pub fn duplicate_frame(&mut self) {
        self.sprite.duplicate_frame(self.active_frame);
        self.active_frame += 1;
        self.dirty = true;
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
        self.undo.push(UndoStep::Snapshot { before, after });
        self.redo.clear();
        self.clamp_cursor();
        self.selection = None;
        self.select_anchor = None;
        self.dirty = true;
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
                    self.paint_pixel(ox + xx, oy + yy, final_grid[i]);
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
    }
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

/// Whether two colors are within `tol` on every channel (transparent == transparent).
fn color_close(a: Rgba, b: Rgba, tol: u8) -> bool {
    if a.3 == 0 && b.3 == 0 {
        return true;
    }
    let t = tol as i16;
    (a.0 as i16 - b.0 as i16).abs() <= t
        && (a.1 as i16 - b.1 as i16).abs() <= t
        && (a.2 as i16 - b.2 as i16).abs() <= t
        && (a.3 as i16 - b.3 as i16).abs() <= t
}

/// Darken (`f = 0.8`) or lighten (`f = 1.25`) a color, keeping alpha.
fn shade_color(c: Rgba, darken: bool) -> Rgba {
    let f = if darken { 0.8 } else { 1.25 };
    let ch = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
    Rgba(ch(c.0), ch(c.1), ch(c.2), c.3)
}

/// Linear interpolation between two colors, `t` in 0..1.
fn lerp_rgba(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Rgba(l(a.0, b.0), l(a.1, b.1), l(a.2, b.2), l(a.3, b.3))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(color_close(Rgba(100, 100, 100, 255), Rgba(105, 98, 100, 255), 8));
        assert!(!color_close(Rgba(100, 100, 100, 255), Rgba(120, 100, 100, 255), 8));
        assert!(color_close(Rgba::TRANSPARENT, Rgba::TRANSPARENT, 0));
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
}
