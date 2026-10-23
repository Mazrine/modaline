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

/// One undoable change: a set of pixel writes, each `(x, y, before, after)`.
pub struct UndoStep {
    pub layer: usize,
    pub frame: usize,
    pub cells: Vec<(u32, u32, Rgba, Rgba)>,
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
    pub symmetry: Symmetry,
    pub lasso_points: Vec<(u32, u32)>,
    pub selection_op: SelectionOp,
    pub filled_shapes: bool,
    pub wand_tolerance: u8,
    pub wand_all_layers: bool,
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
            symmetry: Symmetry::None,
            lasso_points: Vec::new(),
            selection_op: SelectionOp::Replace,
            filled_shapes: false,
            wand_tolerance: 0,
            wand_all_layers: false,
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
        let w = self.sprite.width as i32;
        let h = self.sprite.height as i32;
        let nx = (self.cursor.0 as i32 + dx).clamp(0, w.saturating_sub(1));
        let ny = (self.cursor.1 as i32 + dy).clamp(0, h.saturating_sub(1));
        self.cursor = (nx as u32, ny as u32);
    }

    // ---- drawing ----

    fn record(&mut self, layer: usize, frame: usize, x: u32, y: u32, old: Rgba, new: Rgba) {
        match &mut self.stroke {
            Some(s) => s.cells.push((x, y, old, new)),
            None => {
                self.undo.push(UndoStep {
                    layer,
                    frame,
                    cells: vec![(x, y, old, new)],
                });
                self.redo.clear();
            }
        }
    }

    pub fn begin_stroke(&mut self) {
        self.stroke = Some(UndoStep {
            layer: self.active_layer,
            frame: self.active_frame,
            cells: Vec::new(),
        });
    }

    pub fn end_stroke(&mut self) {
        if let Some(s) = self.stroke.take()
            && !s.cells.is_empty() {
                self.undo.push(s);
                self.redo.clear();
            }
    }

    pub fn paint_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        let (l, f) = (self.active_layer, self.active_frame);
        let old = self.sprite.get(l, f, x, y);
        if old == color {
            return;
        }
        self.sprite.set(l, f, x, y, color);
        self.dirty = true;
        self.record(l, f, x, y, old, color);
    }

    /// What the active tool does to a pixel when dragged/clicked.
    pub fn paint_with_tool(&mut self, x: u32, y: u32) {
        match self.tool {
            Tool::Eraser => self.paint_symmetric(x, y, Rgba::TRANSPARENT),
            _ => self.paint_symmetric(x, y, self.fg),
        }
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
        let color = self.fg;
        self.begin_stroke();
        let changes = flood_fill(&mut self.sprite, l, f, x, y, color);
        let n = changes.len();
        if let Some(s) = &mut self.stroke {
            s.cells.extend(changes);
        }
        if n > 0 {
            self.dirty = true;
        }
        self.end_stroke();
    }

    pub fn commit_shape(&mut self, color: Rgba) {
        if let Some(p) = self.pending.take() {
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
            };
            self.begin_stroke();
            for (x, y) in pts {
                self.paint_symmetric(x, y, color);
            }
            self.end_stroke();
        }
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
        if let Some(s) = self.undo.pop() {
            for &(x, y, before, _) in &s.cells {
                self.sprite.set(s.layer, s.frame, x, y, before);
            }
            self.redo.push(s);
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            for &(x, y, _, after) in &s.cells {
                self.sprite.set(s.layer, s.frame, x, y, after);
            }
            self.undo.push(s);
            self.dirty = true;
        }
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
            self.dirty = true;
        } else {
            self.msg = "cannot delete last frame".into();
        }
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
        let to = (from as i32 + dir).clamp(0, self.sprite.layers.len() as i32 - 1) as usize;
        if self.sprite.move_layer(from, to) {
            self.active_layer = to;
            self.dirty = true;
        }
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
        self.sprite.flip_h();
        self.dirty = true;
    }

    pub fn flip_v(&mut self) {
        self.sprite.flip_v();
        self.dirty = true;
    }

    pub fn rotate_cw(&mut self) {
        self.sprite.rotate_cw();
        self.cursor = (
            self.cursor.0.min(self.sprite.width - 1),
            self.cursor.1.min(self.sprite.height - 1),
        );
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
}
