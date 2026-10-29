use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::brush::{self, Brush, BrushKind};
use crate::color::Rgba;
use crate::sprite::{
    AseProfile, Cel, DitherMode, FillOptions, Sprite, TextObject, color_close, ellipse_points,
    flood_fill_tol, line_points, rect_filled_points, rect_points,
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
    Text,
    Tile,
    Move,
}

/// How a brush colour combines with the destination pixel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Ink {
    #[default]
    Simple,
    LockAlpha,
    Replace,
    Shading,
    Erase,
}

impl Ink {
    pub fn name(self) -> &'static str {
        match self {
            Ink::Simple => "simple",
            Ink::LockAlpha => "lockalpha",
            Ink::Replace => "replace",
            Ink::Shading => "shading",
            Ink::Erase => "erase",
        }
    }

    pub fn parse(name: &str) -> Option<Ink> {
        Some(
            match name.to_ascii_lowercase().replace(['_', '-'], "").as_str() {
                "simple" | "normal" => Ink::Simple,
                "lockalpha" | "alpha" => Ink::LockAlpha,
                "replace" | "replacealpha" => Ink::Replace,
                "shading" | "shade" => Ink::Shading,
                "erase" | "eraser" => Ink::Erase,
                _ => return None,
            },
        )
    }
}

impl Tool {
    pub fn index(self) -> usize {
        match self {
            Tool::Pencil => 0,
            Tool::Eraser => 1,
            Tool::Bucket => 2,
            Tool::Picker => 3,
            Tool::Line => 4,
            Tool::Rect => 5,
            Tool::Ellipse => 6,
            Tool::Gradient => 7,
            Tool::Shade => 8,
            Tool::Spray => 9,
            Tool::Blur => 10,
            Tool::Lasso => 11,
            Tool::Polygon => 12,
            Tool::Text => 13,
            Tool::Tile => 14,
            Tool::Move => 15,
        }
    }
}

const TOOL_COUNT: usize = 16;
/// How many closed documents `:reopen` can walk back through.
const CLOSED_LIMIT: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TileMode {
    #[default]
    Manual,
    Auto,
    Stack,
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

/// A layer-content move in progress: the active cel is detached (its source
/// emptied) and drawn as a floating preview until the drag is committed.
pub struct CelMove {
    pub cel: Cel,
    /// Sprite pixel where the drag started.
    pub from: (u32, u32),
    /// Target top-left of the floating content, in sprite coordinates.
    pub pos: (i32, i32),
    before: Box<Sprite>,
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
    pub lock_movement: bool,
    pub collapsed: bool,
}

/// Approximate cap for the whole undo/redo history; pixel-cell steps are small,
/// structural snapshots are bounded by this even when the count limit is high.
const MAX_UNDO_BYTES: usize = 256 * 1024 * 1024;

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
    /// One cel's z-index.
    CelZ {
        layer: usize,
        frame: usize,
        before: i16,
        after: i16,
    },
    /// Sprite user data (including the background colour).
    SpriteUserData {
        before: Option<crate::sprite::UserData>,
        after: Option<crate::sprite::UserData>,
    },
}

impl UndoStep {
    /// Approximate heap cost, used to bound structural snapshots.
    pub fn bytes(&self) -> usize {
        match self {
            UndoStep::Cells { cells } => {
                cells.len() * std::mem::size_of::<(usize, usize, u32, u32, Rgba, Rgba)>()
            }
            UndoStep::Snapshot { before, after } | UndoStep::Structure { before, after, .. } => {
                snapshot_bytes(before) + snapshot_bytes(after)
            }
            UndoStep::LayerProps { .. } => 64,
            UndoStep::CelZ { .. } => 32,
            UndoStep::SpriteUserData { .. } => 128,
        }
    }

    pub fn label(&self) -> String {
        match self {
            UndoStep::Cells { cells } => {
                let pixels = cells
                    .iter()
                    .map(|c| (c.0, c.1, c.2, c.3))
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                format!("stroke: {pixels} px")
            }
            UndoStep::Snapshot { .. } => "snapshot edit".into(),
            UndoStep::Structure { .. } => "structure: layer/frame/tag".into(),
            UndoStep::LayerProps { layer, .. } => format!("layer {} properties", layer + 1),
            UndoStep::CelZ { layer, frame, .. } => {
                format!("cel z: layer {} frame {}", layer + 1, frame + 1)
            }
            UndoStep::SpriteUserData { .. } => "sprite properties".into(),
        }
    }
}

/// The sprite model plus the interaction state (modes, tools, undo, colors).
/// All view state (zoom/pan) lives in the GUI layer, not here.
pub struct Editor {
    pub sprite: Sprite,
    pub cursor: (u32, u32),
    pub mode: Mode,
    pub tool: Tool,
    /// Ink remembered per tool (indexed by `Tool::index`).
    pub tool_inks: [Ink; TOOL_COUNT],
    /// Restrict every ink to pixels that are already opaque.
    pub alpha_lock: bool,
    /// Pencil/eraser brush width in pixels for the built-in shapes.
    pub brush_size: u32,
    pub brush_kind: BrushKind,
    pub custom_brush: Option<Brush>,
    /// Stamp spacing as a percentage of the brush's largest dimension.
    pub brush_spacing: u32,
    /// Last pressure reported by a touch/pen event.
    pub device_pressure: Option<f32>,
    /// Explicit pressure override (the UI slider or `:pressure`).
    pub manual_pressure: Option<f32>,
    /// Drop L-shaped corner pixels while freehand drawing.
    pub pixel_perfect: bool,
    /// Last points of the current freehand stroke, for the corner filter.
    pp: Vec<(u32, u32)>,
    pub fg: Rgba,
    pub bg: Rgba,
    pub palette: Vec<Rgba>,
    pub palette_names: Vec<String>,
    /// Last colours picked, newest first (max 12); shown by the picker.
    pub recent_colors: Vec<Rgba>,
    pub fg_idx: usize,
    pub bg_idx: usize,
    pub active_layer: usize,
    pub active_frame: usize,
    /// Explicit multi-cel selection; the active cel always participates.
    pub cel_selection: BTreeSet<(usize, usize)>,
    /// Internal guard so per-cel transform loops write only their own cel.
    fanout_suspended: bool,
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
    /// Gradient stops in `0..=1`; empty means the live fg → bg pair.
    pub gradient_stops: Vec<(f32, Rgba)>,
    /// Ordered-dither mode applied while painting a gradient.
    pub gradient_dither: DitherMode,
    /// Pivot for free transforms, in sprite coordinates; `None` = bbox centre.
    pub transform_pivot: Option<(f32, f32)>,
    /// RotSprite-style supersampled mode sampling instead of nearest.
    pub transform_rotsprite: bool,
    pub spray_seed: u64,
    /// Airbrush dots per dab and scatter radius, in pixels.
    pub spray_rate: u32,
    pub spray_radius: u32,
    /// Shade strength as a percentage of the pixel's channel value.
    pub shade_strength: u32,
    /// Blur-tool radius, shared with `box_blur`.
    pub blur_radius: u32,
    /// Contour ring width and whether it grows outside (true) or inside.
    pub contour_width: u32,
    pub contour_inside: bool,
    /// Tilemap editing: brush tile ID/flips and manual-vs-pixel mode.
    pub tile_brush: u32,
    pub tile_flip_x: bool,
    pub tile_flip_y: bool,
    pub tile_flip_diagonal: bool,
    pub tile_pixel_mode: bool,
    pub tile_edit_mode: TileMode,
    pub wand_tolerance: u8,
    pub wand_all_layers: bool,
    /// Wand and bucket restrict themselves to the seed's contiguous region.
    pub contiguous: bool,
    /// Show the sprite tiled and wrap the cursor across its edges.
    pub tile_mode: bool,
    /// Copies on each side of the centre (1 = 3x3 overall), max 8.
    pub tile_x: u32,
    pub tile_y: u32,
    /// Neighbour preview alpha and whether neighbours are drawn at all.
    pub tile_opacity: u8,
    pub tile_preview: bool,
    /// Design grid in sprite pixels (1 = off) and whether the cursor snaps to
    /// it. The pixel grid is a separate view flag.
    pub grid: u32,
    pub snap: bool,
    /// Bitmap-font text stamp: the scale (1 = 8x8 px) and the last string
    /// typed, so the panel keeps it between stamps.
    pub text_scale: u32,
    pub text: String,
    /// Font selector (bitmap only) and 0=left, 1=centre, 2=right.
    pub text_font: String,
    pub text_align: u8,
    /// Snapshot taken before the first live text preview; committed as one undo.
    text_edit_before: Option<Box<Sprite>>,
    /// Playback: wrap at the end of the range, bounce instead of wrapping, and
    /// the current direction for the bounce.
    pub play_loop: bool,
    pub play_pingpong: bool,
    pub play_dir: i32,
    /// Completed passes in the current playback session, for a tag's repeat.
    pub play_passes: u32,
    pub undo: Vec<UndoStep>,
    pub redo: Vec<UndoStep>,
    /// Maximum undo steps kept (oldest are discarded first).
    pub undo_limit: usize,
    pub stroke: Option<UndoStep>,
    pub pending: Option<Pending>,
    pub select_anchor: Option<(u32, u32)>,
    pub selection: Option<Selection>,
    pub clipboard: Option<Clipboard>,
    /// Whole-cel clipboard for copy/paste across layers and frames.
    pub cel_clipboard: Option<Cel>,
    /// Persistent export options (command and dialog share it).
    pub export_config: crate::io::ExportConfig,
    pub paste_pos: Option<(u32, u32)>,
    /// Active layer-content move (floating cel), if any.
    pub cel_move: Option<CelMove>,
    pub cmdline: String,
    pub msg: String,
    pub dirty: bool,
    /// Monotonic counter for cache invalidation (timeline thumbnails). Every
    /// edit bumps it; the view compares it instead of rebuilding each frame.
    pub revision: u64,
    /// Bounds and frame of a pixel-only edit since the canvas last consumed the
    /// dirty state; `dirty_full` means the whole composite must be rebuilt.
    pub dirty_rect: Option<(u32, u32, u32, u32)>,
    pub dirty_frame: Option<usize>,
    pub dirty_full: bool,
    pub should_quit: bool,
    pub g_prefix: bool,
    pub path: Option<PathBuf>,
    /// Recently opened or saved files, newest first (max 10).
    pub recent: Vec<PathBuf>,
    /// Cleanly closed documents, oldest first (max `CLOSED_LIMIT`); `:reopen`.
    pub closed: Vec<PathBuf>,
    /// Path `:reopen` wants the app to open in a new tab.
    pub reopen_request: Option<PathBuf>,
    /// Set when the opened file is a another tool fix bundle: the part the strokes
    /// are for, from the sidecar. Also marks the file as one whose `ref` layer
    /// must survive a save.
    pub bundle_stage: Option<String>,
    /// Refuse to paint a colour that is not in the palette. Off by default: a
    /// plain sprite has no ramp to protect.
    pub palette_lock: bool,
    /// Dithering used when converting to indexed (`:dither`, `:mode indexed`).
    pub dither: DitherMode,
    /// The renderer the user asked for. Applied at startup, so changing it here
    /// only takes effect on the next launch.
    pub renderer: String,
}

fn snapshot_bytes(s: &Sprite) -> usize {
    // Cels are sparse, but snapshots are structural and often dense after a
    // transform; charging a full RGBA frame per layer/frame is conservative.
    let frames = s.frames.len().max(1);
    let layers = s.layers.len().max(1);
    s.width as usize * s.height as usize * 4 * frames * layers + 1024
}

impl Editor {
    pub fn new(width: u32, height: u32) -> Self {
        let palette = crate::color::default_palette();
        let fg_idx = 12; // white
        let fg = palette[fg_idx];
        let bg = palette[0];
        let palette_names = vec![String::new(); palette.len()];
        Self {
            sprite: Sprite::new(width, height),
            cursor: (0, 0),
            mode: Mode::Normal,
            tool: Tool::Pencil,
            tool_inks: {
                let mut inks = [Ink::Simple; TOOL_COUNT];
                inks[Tool::Eraser.index()] = Ink::Erase;
                inks
            },
            alpha_lock: false,
            brush_size: 1,
            brush_kind: BrushKind::Square,
            custom_brush: None,
            brush_spacing: 100,
            device_pressure: None,
            manual_pressure: None,
            pixel_perfect: false,
            pp: Vec::new(),
            fg,
            bg,
            palette,
            palette_names,
            recent_colors: Vec::new(),
            fg_idx,
            bg_idx: 0,
            active_layer: 0,
            active_frame: 0,
            cel_selection: BTreeSet::new(),
            fanout_suspended: false,
            active_tag: None,
            symmetry: Symmetry::None,
            sym_axes: 6,
            sym_center: None,
            lasso_points: Vec::new(),
            selection_op: SelectionOp::Replace,
            filled_shapes: false,
            gradient_radial: false,
            gradient_stops: Vec::new(),
            gradient_dither: DitherMode::None,
            transform_pivot: None,
            transform_rotsprite: false,
            spray_seed: 0x9E37_79B9_7F4A_7C15,
            spray_rate: 6,
            spray_radius: 3,
            shade_strength: 20,
            blur_radius: 1,
            contour_width: 1,
            contour_inside: false,
            tile_brush: 1,
            tile_flip_x: false,
            tile_flip_y: false,
            tile_flip_diagonal: false,
            tile_pixel_mode: false,
            tile_edit_mode: TileMode::Manual,
            wand_tolerance: 0,
            wand_all_layers: false,
            contiguous: true,
            tile_mode: false,
            tile_x: 1,
            tile_y: 1,
            tile_opacity: 120,
            tile_preview: true,
            grid: 8,
            snap: false,
            text_scale: 1,
            text: String::new(),
            text_font: "vixel".into(),
            text_align: 0,
            text_edit_before: None,
            play_loop: true,
            play_pingpong: false,
            play_dir: 1,
            play_passes: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_limit: 100,
            stroke: None,
            pending: None,
            select_anchor: None,
            selection: None,
            clipboard: None,
            cel_clipboard: None,
            export_config: crate::io::ExportConfig::default(),
            paste_pos: None,
            cel_move: None,
            cmdline: String::new(),
            msg: "press ? for help".into(),
            dirty: false,
            revision: 0,
            dirty_rect: None,
            dirty_frame: None,
            dirty_full: true,
            should_quit: false,
            g_prefix: false,
            path: None,
            recent: Vec::new(),
            closed: Vec::new(),
            reopen_request: None,
            bundle_stage: None,
            palette_lock: false,
            dither: DitherMode::None,
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
        self.bump();
        self.dirty_full = true;
        self.dirty_rect = None;
        self.dirty_frame = None;
    }

    fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Push an undo step, trimming the oldest history to the configured count
    /// and byte budget. Structural snapshots are the expensive entries, so the
    /// byte cap is what keeps a long session bounded.
    fn push_undo(&mut self, step: UndoStep) {
        self.undo.push(step);
        self.trim_history();
    }

    fn push_redo(&mut self, step: UndoStep) {
        self.redo.push(step);
        while self.redo.len() > self.undo_limit.max(1) {
            self.redo.remove(0);
        }
    }

    fn trim_history(&mut self) {
        let limit = self.undo_limit.max(1);
        while self.undo.len() > limit {
            self.undo.remove(0);
        }
        let mut bytes: usize = self.undo.iter().map(UndoStep::bytes).sum();
        while bytes > MAX_UNDO_BYTES && self.undo.len() > 1 {
            bytes -= self.undo.remove(0).bytes();
        }
    }

    pub fn history_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(self.redo.iter())
            .map(UndoStep::bytes)
            .sum()
    }

    /// Record one pixel write's bounds for a partial canvas upload. Structural
    /// edits go through `touch`, which forces a full rebuild.
    fn mark_dirty_pixel(&mut self, frame: usize, x: u32, y: u32) {
        if self.dirty_full {
            return;
        }
        if let Some(f) = self.dirty_frame
            && f != frame
        {
            self.dirty_full = true;
            self.dirty_rect = None;
            return;
        }
        self.dirty_frame = Some(frame);
        self.dirty_rect = Some(match self.dirty_rect {
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
            None => (x, y, x + 1, y + 1),
        });
    }

    /// The canvas has consumed the dirty region.
    pub fn clear_dirty(&mut self) {
        self.dirty_rect = None;
        self.dirty_frame = None;
        self.dirty_full = false;
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
            self.push_undo(UndoStep::Cells {
                cells: vec![(layer, frame, x, y, old, new)],
            });
            self.redo.clear();
        }
        self.bump();
        self.mark_dirty_pixel(frame, x, y);
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
                | UndoStep::LayerProps { .. }
                | UndoStep::CelZ { .. }
                | UndoStep::SpriteUserData { .. } => false,
            };
            if !empty {
                self.push_undo(s);
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

    /// Exact, selection-clipped write used by clear/cut/transform/filters so
    /// they do not run through the active ink.
    fn put_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        if x >= self.sprite.width || y >= self.sprite.height || !self.selection_allows(x, y) {
            return;
        }
        self.put_pixel_unclipped(x, y, color);
    }

    pub fn ink(&self) -> Ink {
        self.tool_inks[self.tool.index()]
    }

    pub fn set_ink(&mut self, ink: Ink) {
        self.tool_inks[self.tool.index()] = ink;
        self.msg = format!("ink: {}", ink.name());
    }

    pub fn set_alpha_lock(&mut self, on: bool) {
        self.alpha_lock = on;
        self.msg = if on {
            "alpha lock on".into()
        } else {
            "alpha lock off".into()
        };
    }

    /// Paint through the active ink, respecting the selection.
    fn cel_targets(&self) -> Vec<(usize, usize)> {
        if self.fanout_suspended {
            return vec![(self.active_layer, self.active_frame)];
        }
        let mut targets: Vec<(usize, usize)> = self.cel_selection.iter().copied().collect();
        if !targets.contains(&(self.active_layer, self.active_frame)) {
            targets.push((self.active_layer, self.active_frame));
        }
        targets.sort_unstable();
        targets
    }

    /// Explicitly selected cels plus the active one.
    pub fn selected_cels(&self) -> Vec<(usize, usize)> {
        self.cel_targets()
    }

    fn active_cel_content(&self) -> Cel {
        let (l, f) = (self.active_layer, self.active_frame);
        let mut cel = self.sprite.cels[l][f].clone();
        let mut hops = 0;
        while let Some(target) = cel.link {
            if target >= self.sprite.frames.len() || hops > self.sprite.frames.len() {
                break;
            }
            cel = self.sprite.cels[l][target].clone();
            hops += 1;
        }
        cel.link = None;
        cel
    }

    /// Copy the active cel (resolving links) to the cel clipboard.
    pub fn copy_cel(&mut self) {
        let cel = self.active_cel_content();
        self.msg = format!("cel copied ({}x{})", cel.w, cel.h);
        self.cel_clipboard = Some(cel);
    }

    /// Copy the active cel, then empty it (one undo step).
    pub fn cut_cel(&mut self) {
        let cel = self.active_cel_content();
        self.cel_clipboard = Some(cel);
        let before = self.sprite.clone();
        let (l, f) = (self.active_layer, self.active_frame);
        self.empty_cel_at(l, f);
        self.snapshot(before);
        self.msg = "cel cut".into();
    }

    /// Paste the cel clipboard into the active cel.
    pub fn paste_cel(&mut self) {
        let Some(mut cel) = self.cel_clipboard.clone() else {
            self.msg = "cel clipboard is empty".into();
            return;
        };
        if !self.sprite.layer_editable(self.active_layer) {
            self.msg = "layer is locked or hidden".into();
            return;
        }
        cel.link = None;
        let before = self.sprite.clone();
        let (l, f) = (self.active_layer, self.active_frame);
        self.sprite.cels[l][f] = cel;
        self.snapshot(before);
        self.msg = "cel pasted".into();
    }

    /// Empty the active cel but keep the frame.
    pub fn delete_cel(&mut self) {
        let (l, f) = (self.active_layer, self.active_frame);
        let before = self.sprite.clone();
        self.empty_cel_at(l, f);
        self.snapshot(before);
        self.msg = "cel emptied (frame kept)".into();
    }

    fn empty_cel_at(&mut self, layer: usize, frame: usize) {
        let (z, op) = {
            let cel = &self.sprite.cels[layer][frame];
            (cel.z_index, cel.opacity)
        };
        let mut cel = Cel::blank(0, 0);
        cel.z_index = z;
        cel.opacity = op;
        self.sprite.cels[layer][frame] = cel;
        self.touch();
    }

    pub fn cel_is_empty(&self, layer: usize, frame: usize) -> bool {
        self.sprite
            .cels
            .get(layer)
            .and_then(|f| f.get(frame))
            .is_none_or(|cel| {
                cel.link.is_none()
                    && cel.user_data.is_none()
                    && cel.tilemap.is_none()
                    && cel.pixels.iter().all(|p| p.3 == 0)
            })
    }

    /// Export every selected cel to `dir` as `l<layer>_f<frame>.png`.
    pub fn export_cels(&self, dir: &std::path::Path) -> anyhow::Result<usize> {
        std::fs::create_dir_all(dir)?;
        let mut count = 0;
        for (layer, frame) in self.selected_cels() {
            let path = dir.join(format!("l{layer}_f{frame}.png"));
            crate::io::export_cel(&self.sprite, layer, frame, &path)?;
            count += 1;
        }
        Ok(count)
    }

    pub fn cel_selected(&self, layer: usize, frame: usize) -> bool {
        self.cel_selection.contains(&(layer, frame))
            || (layer == self.active_layer && frame == self.active_frame)
    }

    pub fn toggle_cel_selection(&mut self, layer: usize, frame: usize) {
        if !self.cel_selection.remove(&(layer, frame)) {
            self.cel_selection.insert((layer, frame));
        }
        self.msg = format!("cel selection: {}", self.cel_selection.len());
    }

    pub fn clear_cel_selection(&mut self) {
        self.cel_selection.clear();
    }

    /// Select the inclusive frame range on one layer.
    pub fn select_frame_range(&mut self, layer: usize, from: usize, to: usize) {
        let (a, b) = (from.min(to), from.max(to));
        for frame in a..=b {
            self.cel_selection.insert((layer, frame));
        }
    }

    /// Select a rectangle of cels between two layer/frame corners.
    pub fn select_cel_rect(&mut self, a: (usize, usize), b: (usize, usize)) {
        self.cel_selection.clear();
        let (l0, l1) = (a.0.min(b.0), a.0.max(b.0));
        let (f0, f1) = (a.1.min(b.1), a.1.max(b.1));
        for layer in l0..=l1 {
            for frame in f0..=f1 {
                self.cel_selection.insert((layer, frame));
            }
        }
    }

    /// Select the inclusive layer range on one frame.
    pub fn select_layer_range(&mut self, from: usize, to: usize, frame: usize) {
        let (a, b) = (from.min(to), from.max(to));
        for layer in a..=b {
            self.cel_selection.insert((layer, frame));
        }
    }

    pub(crate) fn paint_pixel_unclipped(&mut self, x: u32, y: u32, color: Rgba) {
        if x >= self.sprite.width || y >= self.sprite.height {
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
        let mut wrote = false;
        for (l, f) in self.cel_targets() {
            if !self.sprite.layer_editable(l) {
                continue;
            }
            let old = self.sprite.get(l, f, x, y);
            if self.alpha_lock && old.3 == 0 {
                continue;
            }
            let new = apply_ink(self.ink(), old, color);
            if old == new {
                continue;
            }
            self.sprite.set(l, f, x, y, new);
            self.record(l, f, x, y, old, new);
            wrote = true;
        }
        if wrote {
            self.dirty = true;
        } else if !self.sprite.layer_editable(self.active_layer) {
            self.msg = "layer is locked or hidden".into();
        }
    }

    /// Exact write for paste, transforms and contour: no ink, but the same
    /// layer/selection-owner checks as a paint.
    pub(crate) fn put_pixel_unclipped(&mut self, x: u32, y: u32, color: Rgba) {
        if x >= self.sprite.width || y >= self.sprite.height {
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
        let mut wrote = false;
        for (l, f) in self.cel_targets() {
            if !self.sprite.layer_editable(l) {
                continue;
            }
            let old = self.sprite.get(l, f, x, y);
            if old == color {
                continue;
            }
            self.sprite.set(l, f, x, y, color);
            self.record(l, f, x, y, old, color);
            wrote = true;
        }
        if wrote {
            self.dirty = true;
        } else if !self.sprite.layer_editable(self.active_layer) {
            self.msg = "layer is locked or hidden".into();
        }
    }

    /// What the active tool does to a pixel when dragged/clicked (primary).
    pub fn paint_with_tool(&mut self, x: u32, y: u32) {
        match self.tool {
            Tool::Eraser => self.paint_freehand(x, y, Rgba::TRANSPARENT),
            Tool::Pencil => self.paint_freehand(x, y, self.fg),
            Tool::Shade => self.shade_at(x, y, true),
            Tool::Spray => self.spray_at(x, y),
            Tool::Blur => self.blur_at(x, y),
            Tool::Text => self.place_text_at(x, y),
            Tool::Tile if self.tile_pixel_mode => self.paint_tile_pixel_at(x, y, self.fg),
            Tool::Tile => self.paint_tile_at(x, y),
            Tool::Move => {}
            _ => self.paint_symmetric(x, y, self.fg),
        }
    }

    /// Secondary action (right mouse button): bg color, or lighten for Shade.
    pub fn paint_secondary(&mut self, x: u32, y: u32) {
        match self.tool {
            Tool::Shade => self.shade_at(x, y, false),
            Tool::Pencil => self.paint_freehand(x, y, self.bg),
            Tool::Tile if self.tile_pixel_mode => self.paint_tile_pixel_at(x, y, self.bg),
            Tool::Tile => self.pick_tile_at(x, y),
            Tool::Move => {}
            _ => self.paint_symmetric(x, y, self.bg),
        }
    }

    /// Darken (or lighten) the existing pixel(s) under the cursor, keeping alpha.
    pub fn shade_at(&mut self, x: u32, y: u32, darken: bool) {
        let brush = self.current_brush();
        let pressure = self.pressure();
        let strength = self.shade_strength.clamp(1, 100);
        let (l, f) = (self.active_layer, self.active_frame);
        let ox = -(brush.w as i32 / 2);
        let oy = -(brush.h as i32 / 2);
        for (sx, sy) in self.symmetry_points(x, y) {
            for by in 0..brush.h {
                for bx in 0..brush.w {
                    let coverage = brush.coverage(bx as i32, by as i32);
                    if coverage == 0 {
                        continue;
                    }
                    let px = sx as i32 + ox + bx as i32;
                    let py = sy as i32 + oy + by as i32;
                    if px < 0 || py < 0 || px as u32 >= self.sprite.width {
                        continue;
                    }
                    if py as u32 >= self.sprite.height {
                        continue;
                    }
                    let old = self.sprite.get(l, f, px as u32, py as u32);
                    if old.is_transparent() {
                        continue;
                    }
                    let shaded = shade_color(old, darken, strength);
                    let t = (f32::from(coverage) * pressure / 255.0).clamp(0.0, 1.0);
                    self.put_pixel(px as u32, py as u32, lerp_rgba(old, shaded, t));
                }
            }
        }
    }

    /// Scatter `spray_rate` brush stamps inside the scatter radius (airbrush).
    pub fn spray_at(&mut self, x: u32, y: u32) {
        let brush = self.current_brush();
        let r = self.spray_radius.clamp(1, 64) as i32;
        let rate = self.spray_rate.clamp(1, 64);
        let points = self.symmetry_points(x, y);
        for (sx, sy) in points {
            for _ in 0..rate {
                let dx = self.next_rand() % (2 * r + 1) - r;
                let dy = self.next_rand() % (2 * r + 1) - r;
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let px = sx as i32 + dx;
                let py = sy as i32 + dy;
                if px >= 0
                    && py >= 0
                    && (px as u32) < self.sprite.width
                    && (py as u32) < self.sprite.height
                {
                    let color = self.fg;
                    self.stamp_brush_symmetric(&brush, px as u32, py as u32, color);
                }
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
        let (l, f) = (self.active_layer, self.active_frame);
        let (w, h) = (self.sprite.width, self.sprite.height);
        let src: Vec<Rgba> = (0..w * h)
            .map(|i| self.sprite.get(l, f, i % w, i / w))
            .collect();
        let soft = box_blur(&src, w, h, self.blur_radius);
        let brush = self.current_brush();
        let pressure = self.pressure();
        let ox = -(brush.w as i32 / 2);
        let oy = -(brush.h as i32 / 2);
        for (sx, sy) in self.symmetry_points(x, y) {
            for by in 0..brush.h {
                for bx in 0..brush.w {
                    let coverage = brush.coverage(bx as i32, by as i32);
                    if coverage == 0 {
                        continue;
                    }
                    let px = sx as i32 + ox + bx as i32;
                    let py = sy as i32 + oy + by as i32;
                    if px < 0 || py < 0 || px as u32 >= w || py as u32 >= h {
                        continue;
                    }
                    let i = (py as u32 * w + px as u32) as usize;
                    let t = (f32::from(coverage) * pressure / 255.0).clamp(0.0, 1.0);
                    self.put_pixel(px as u32, py as u32, lerp_rgba(src[i], soft[i], t));
                }
            }
        }
    }

    /// Draw an outline (in fg) around the selection, or around the contiguous
    /// opaque region under the cursor when there is no selection.
    pub fn contour_at(&mut self, x: u32, y: u32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let region = self.region_mask(x, y);
        let width = self.contour_width.clamp(1, 64);
        let color = self.fg;
        let mut ring = vec![false; region.len()];
        if self.contour_inside {
            let mut kept = region.clone();
            for _ in 0..width {
                let prev = kept.clone();
                for yy in 0..h as i32 {
                    for xx in 0..w as i32 {
                        let i = (yy as u32 * w + xx as u32) as usize;
                        if !prev[i] {
                            continue;
                        }
                        let boundary = [
                            (xx.wrapping_sub(1), yy),
                            (xx + 1, yy),
                            (xx, yy.wrapping_sub(1)),
                            (xx, yy + 1),
                        ]
                        .iter()
                        .any(|&(nx, ny)| {
                            nx < 0
                                || ny < 0
                                || nx >= w as i32
                                || ny >= h as i32
                                || !prev[(ny as u32 * w + nx as u32) as usize]
                        });
                        if boundary {
                            kept[i] = false;
                        }
                    }
                }
            }
            for i in 0..ring.len() {
                ring[i] = region[i] && !kept[i];
            }
        } else {
            let mut grown = region.clone();
            for _ in 0..width {
                let prev = grown.clone();
                for yy in 0..h as i32 {
                    for xx in 0..w as i32 {
                        let i = (yy as u32 * w + xx as u32) as usize;
                        if prev[i] {
                            continue;
                        }
                        let touching = [
                            (xx.wrapping_sub(1), yy),
                            (xx + 1, yy),
                            (xx, yy.wrapping_sub(1)),
                            (xx, yy + 1),
                        ]
                        .iter()
                        .any(|&(nx, ny)| {
                            nx >= 0
                                && ny >= 0
                                && nx < w as i32
                                && ny < h as i32
                                && prev[(ny as u32 * w + nx as u32) as usize]
                        });
                        if touching {
                            grown[i] = true;
                        }
                    }
                }
            }
            for i in 0..ring.len() {
                ring[i] = grown[i] && !region[i];
            }
        }
        self.begin_stroke();
        for y in 0..h {
            for x in 0..w {
                if ring[(y * w + x) as usize] {
                    self.put_pixel_unclipped(x, y, color);
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
                let color = self.gradient_color(t);
                self.paint_pixel(x, y, gradient_dither(color, x, y, self.gradient_dither));
            }
        }
        self.end_stroke();
    }

    /// Colour at `t` along the active gradient (fg → bg when no stops are set).
    pub fn gradient_color(&self, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        if self.gradient_stops.is_empty() {
            return lerp_rgba(self.fg, self.bg, t);
        }
        let stops = &self.gradient_stops;
        let (first_pos, first) = stops[0];
        if t <= first_pos {
            return first;
        }
        for pair in stops.windows(2) {
            let (p0, c0) = pair[0];
            let (p1, c1) = pair[1];
            if t <= p1 {
                let span = (p1 - p0).max(f32::EPSILON);
                return lerp_rgba(c0, c1, ((t - p0) / span).clamp(0.0, 1.0));
            }
        }
        stops.last().map(|(_, c)| *c).unwrap_or(self.bg)
    }

    /// Install a named gradient preset; returns false for an unknown name.
    pub fn gradient_preset(&mut self, name: &str) -> bool {
        let stops: Vec<(f32, Rgba)> = match name.to_ascii_lowercase().as_str() {
            "fg-bg" | "default" => Vec::new(),
            "fg-transparent" => vec![(0.0, self.fg), (1.0, Rgba::TRANSPARENT)],
            "bg-transparent" => vec![(0.0, self.bg), (1.0, Rgba::TRANSPARENT)],
            "black-white" => vec![(0.0, Rgba::rgb(0, 0, 0)), (1.0, Rgba::rgb(255, 255, 255))],
            "white-black" => vec![(0.0, Rgba::rgb(255, 255, 255)), (1.0, Rgba::rgb(0, 0, 0))],
            "rainbow" => vec![
                (0.0, Rgba::rgb(255, 0, 0)),
                (1.0 / 6.0, Rgba::rgb(255, 255, 0)),
                (2.0 / 6.0, Rgba::rgb(0, 255, 0)),
                (3.0 / 6.0, Rgba::rgb(0, 255, 255)),
                (4.0 / 6.0, Rgba::rgb(0, 0, 255)),
                (5.0 / 6.0, Rgba::rgb(255, 0, 255)),
                (1.0, Rgba::rgb(255, 0, 0)),
            ],
            "fire" => vec![
                (0.0, Rgba::rgb(0, 0, 0)),
                (0.35, Rgba::rgb(180, 0, 0)),
                (0.65, Rgba::rgb(255, 120, 0)),
                (0.85, Rgba::rgb(255, 220, 60)),
                (1.0, Rgba::rgb(255, 255, 255)),
            ],
            _ => return false,
        };
        self.gradient_stops = stops;
        self.msg = format!("gradient preset: {name}");
        true
    }

    /// Add or replace a stop at `pos` (0..=1), keeping the list sorted.
    pub fn gradient_set_stop(&mut self, pos: f32, color: Rgba) {
        let pos = pos.clamp(0.0, 1.0);
        self.gradient_stops
            .retain(|(p, _)| (p - pos).abs() > 0.0001);
        self.gradient_stops.push((pos, color));
        self.gradient_stops
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        self.msg = format!("gradient stop: {:.0}%", pos * 100.0);
    }

    /// Neighbour offsets for the tiled preview, including `(0, 0)`.
    pub fn tile_offsets(&self) -> Vec<(i32, i32)> {
        let x = self.tile_x.clamp(1, 8) as i32;
        let y = self.tile_y.clamp(1, 8) as i32;
        let mut out = Vec::with_capacity(((2 * x + 1) * (2 * y + 1)) as usize);
        for dy in -y..=y {
            for dx in -x..=x {
                out.push((dx, dy));
            }
        }
        out
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
            self.put_pixel_unclipped(px, py, color);
        }
    }

    /// Freehand paint: joins the last point to `(x, y)` with a line, and in
    /// pixel-perfect mode drops the middle pixel of an L-shaped corner. Only
    /// the plain 1 px, no-symmetry path gets the filter; brushes and symmetry
    /// fall back to a single stamp.
    pub fn paint_freehand(&mut self, x: u32, y: u32, color: Rgba) {
        let plain = self.brush_kind == BrushKind::Square
            && self.brush_size == 1
            && self.symmetry == Symmetry::None;
        if !self.pixel_perfect || !plain {
            let start = self.pp.last().copied().unwrap_or((x, y));
            let spacing = self.stamp_spacing();
            let brush = self.current_brush();
            for (i, (px, py)) in line_points(start.0 as i32, start.1 as i32, x as i32, y as i32)
                .into_iter()
                .enumerate()
            {
                if i % spacing == 0 {
                    self.stamp_brush_symmetric(&brush, px, py, color);
                }
            }
            self.pp.clear();
            self.pp.push((x, y));
            return;
        }
        let start = self.pp.last().copied().unwrap_or((x, y));
        let brush = self.current_brush();
        for (px, py) in line_points(start.0 as i32, start.1 as i32, x as i32, y as i32) {
            if self.pp.last() == Some(&(px, py)) {
                continue;
            }
            self.stamp_brush_symmetric(&brush, px, py, color);
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

    pub fn current_brush(&self) -> Brush {
        match self.brush_kind {
            BrushKind::Square => brush::square(self.brush_size),
            BrushKind::Circle => brush::circle(self.brush_size),
            BrushKind::Custom => self
                .custom_brush
                .clone()
                .unwrap_or_else(|| brush::square(self.brush_size)),
        }
    }

    pub fn set_brush_kind(&mut self, kind: BrushKind) {
        self.brush_kind = kind;
        self.msg = format!("brush: {}", kind.name());
    }

    pub fn set_brush_spacing(&mut self, percent: u32) {
        self.brush_spacing = percent.clamp(1, 200);
        self.msg = format!("brush spacing: {}%", self.brush_spacing);
    }

    /// Effective pressure: manual override, else device force, else full.
    pub fn pressure(&self) -> f32 {
        self.manual_pressure
            .or(self.device_pressure)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }

    pub fn pressure_source(&self) -> &'static str {
        if self.manual_pressure.is_some() {
            "manual"
        } else if self.device_pressure.is_some() {
            "device"
        } else {
            "default"
        }
    }

    pub fn set_pressure(&mut self, pressure: f32) {
        self.manual_pressure = Some(pressure.clamp(0.0, 1.0));
        self.msg = format!("pressure: {:.0}% (manual)", self.pressure() * 100.0);
    }

    pub fn pressure_auto(&mut self) {
        self.manual_pressure = None;
        self.msg = format!("pressure: auto ({})", self.pressure_source());
    }

    pub fn set_device_pressure(&mut self, pressure: Option<f32>) {
        self.device_pressure = pressure.map(|p| p.clamp(0.0, 1.0));
    }

    pub fn load_brush(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let brush = brush::load(path)?;
        self.custom_brush = Some(brush.clone());
        self.brush_kind = BrushKind::Custom;
        self.msg = format!("brush: {} ({}x{})", brush.name, brush.w, brush.h);
        Ok(())
    }

    pub fn save_brush(&mut self, name: &str) -> anyhow::Result<std::path::PathBuf> {
        let brush = self.current_brush();
        let path = brush::save_named(&brush, name)?;
        self.msg = format!("brush saved: {}", path.display());
        Ok(path)
    }

    pub fn use_brush(&mut self, name: &str) -> anyhow::Result<()> {
        let brush = brush::load_named(name)?;
        self.custom_brush = Some(brush.clone());
        self.brush_kind = BrushKind::Custom;
        self.msg = format!("brush: {} ({}x{})", brush.name, brush.w, brush.h);
        Ok(())
    }

    fn stamp_spacing(&self) -> usize {
        let size = self.current_brush().max_dim() as usize;
        (size * self.brush_spacing.clamp(1, 200) as usize / 100).max(1)
    }

    fn stamp_brush_symmetric(&mut self, brush: &Brush, x: u32, y: u32, color: Rgba) {
        let ox = -(brush.w as i32 / 2);
        let oy = -(brush.h as i32 / 2);
        let erase = self.ink() == Ink::Erase;
        let pressure = self.pressure();
        for by in 0..brush.h {
            for bx in 0..brush.w {
                let coverage = brush.coverage(bx as i32, by as i32);
                if coverage == 0 {
                    continue;
                }
                let coverage = (f32::from(coverage) * pressure).round() as u8;
                if coverage == 0 {
                    continue;
                }
                let a = if color.3 == 0 {
                    if erase { coverage } else { 0 }
                } else {
                    (u32::from(color.3) * u32::from(coverage) / 255) as u8
                };
                if a == 0 {
                    continue;
                }
                let px = x as i32 + ox + bx as i32;
                let py = y as i32 + oy + by as i32;
                if px >= 0
                    && py >= 0
                    && (px as u32) < self.sprite.width
                    && (py as u32) < self.sprite.height
                {
                    self.paint_symmetric(px as u32, py as u32, Rgba(color.0, color.1, color.2, a));
                }
            }
        }
    }

    /// Paint the active brush mask centred on the cursor, mirrored by the
    /// active symmetry and clipped at the canvas edges.
    pub fn paint_brush_symmetric(&mut self, x: u32, y: u32, color: Rgba) {
        let brush = self.current_brush();
        self.stamp_brush_symmetric(&brush, x, y, color);
    }

    /// Create or edit the active cel's text object and re-rasterise it as one
    /// undo step.
    pub fn set_text_object(&mut self, text: &str) {
        if let Some(before) = self.text_edit_before.take() {
            self.sprite = *before;
        }
        self.text = text.to_string();
        self.text_edit_before = Some(Box::new(self.sprite.clone()));
        self.preview_text_object();
        self.commit_text_edit();
        if let Some((object, _)) = self.text_object_for(None) {
            self.msg = format!("text object: {:?} ({} px)", object.text, object.size);
        }
    }

    /// Update the text object from the panel fields without pushing undo yet.
    pub fn preview_text_object(&mut self) {
        let l = self.active_layer;
        if !self.sprite.layer_editable(l) {
            self.msg = "layer is locked or hidden".into();
            return;
        }
        let Some((object, color)) = self.text_object_for(None) else {
            return;
        };
        if self.text_edit_before.is_none() {
            self.text_edit_before = Some(Box::new(self.sprite.clone()));
        }
        self.rebuild_text_cel(&object, color);
        self.msg = format!("text: {} px, align {}", object.size, object.align);
    }

    /// Commit a live preview as exactly one undo step.
    pub fn commit_text_edit(&mut self) {
        if let Some(before) = self.text_edit_before.take() {
            self.snapshot(*before);
        }
    }

    /// Drop a live preview and restore the pre-edit cel.
    pub fn cancel_text_edit(&mut self) {
        if let Some(before) = self.text_edit_before.take() {
            self.sprite = *before;
            self.touch();
            self.msg = "text edit cancelled".into();
        }
    }

    fn tilemap_target(&self) -> Option<(usize, u32, u32, u32)> {
        let layer = self.sprite.layers.get(self.active_layer)?;
        if !layer.tilemap {
            return None;
        }
        let ts = self
            .sprite
            .tilesets
            .iter()
            .find(|t| t.id == layer.tileset)?;
        if ts.image.is_empty() {
            return None;
        }
        Some((self.active_layer, ts.tile_w, ts.tile_h, ts.num_tiles))
    }

    /// Place the tile brush (with flips) at the cursor; one undo step.
    pub fn paint_tile_at(&mut self, x: u32, y: u32) {
        let (l, f) = (self.active_layer, self.active_frame);
        let Some((_, tw, th, _)) = self.tilemap_target() else {
            self.msg = "tile: active layer is not an embedded tilemap".into();
            return;
        };
        let (idx, tiles_w, tiles_h, neighbours_empty) = {
            let cel = &self.sprite.cels[l][f];
            let Some(tm) = &cel.tilemap else {
                self.msg = "tile: this cel has no tile references".into();
                return;
            };
            let tx = (x as i32 - cel.x).div_euclid(tw as i32);
            let ty = (y as i32 - cel.y).div_euclid(th as i32);
            if tx < 0 || ty < 0 || tx as u32 >= tm.tiles_w || ty as u32 >= tm.tiles_h.max(1) {
                self.msg = "tile: outside the tilemap bounds".into();
                return;
            }
            let idx = ty as u32 * tm.tiles_w + tx as u32;
            let mut empty = Vec::new();
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (tx + dx, ty + dy);
                if nx >= 0 && ny >= 0 && (nx as u32) < tm.tiles_w && (ny as u32) < tm.tiles_h {
                    let i = (ny as u32 * tm.tiles_w + nx as u32) as usize;
                    if tm.tiles.get(i).is_some_and(|t| t.id == 0) {
                        empty.push(i);
                    }
                }
            }
            (idx as usize, tm.tiles_w, tm.tiles_h, empty)
        };
        let id = match self.tile_edit_mode {
            TileMode::Stack if self.tile_brush != 0 => self.tile_brush + (idx as u32 % 3),
            _ => self.tile_brush,
        };
        let mut writes: Vec<usize> = vec![idx];
        if self.tile_edit_mode == TileMode::Auto && id != 0 {
            writes.extend(neighbours_empty);
        }
        let _ = (tiles_w, tiles_h);
        let before = self.sprite.clone();
        if let Some(cel) = self
            .sprite
            .cels
            .get_mut(l)
            .and_then(|frames| frames.get_mut(f))
            && let Some(tm) = &mut cel.tilemap
        {
            for i in writes {
                if let Some(slot) = tm.tiles.get_mut(i) {
                    *slot = crate::sprite::TileRef {
                        id,
                        flip_x: self.tile_flip_x,
                        flip_y: self.tile_flip_y,
                        flip_diagonal: self.tile_flip_diagonal,
                    };
                }
            }
        }
        self.sprite.rasterize_tilemap(l, f);
        self.snapshot(before);
        self.msg = if id == 0 {
            "tile: empty".into()
        } else {
            format!("tile: {id} placed")
        };
    }

    /// Edit the tileset atlas pixel shown under the cursor and re-render every
    /// tilemap cel that uses the tileset.
    pub fn paint_tile_pixel_at(&mut self, x: u32, y: u32, color: Rgba) {
        let (l, f) = (self.active_layer, self.active_frame);
        let Some((_, tw, th, _)) = self.tilemap_target() else {
            self.msg = "tile: active layer is not an embedded tilemap".into();
            return;
        };
        let Some(ts_id) = self.sprite.layers.get(l).map(|v| v.tileset) else {
            return;
        };
        let Some(ts) = self.sprite.tilesets.iter().find(|t| t.id == ts_id).cloned() else {
            return;
        };
        let (atlas_idx, base) = {
            let cel = &self.sprite.cels[l][f];
            let Some(tm) = &cel.tilemap else {
                return;
            };
            let tx = (x as i32 - cel.x).div_euclid(tw as i32);
            let ty = (y as i32 - cel.y).div_euclid(th as i32);
            if tx < 0 || ty < 0 || tx as u32 >= tm.tiles_w || ty as u32 >= tm.tiles_h {
                return;
            }
            let i = (ty as u32 * tm.tiles_w + tx as u32) as usize;
            let Some(tile) = tm.tiles.get(i) else {
                return;
            };
            let Some(tile_index) = tile.id.checked_sub(ts.base_index) else {
                return;
            };
            if tile.id == 0 || tile_index >= ts.num_tiles {
                return;
            }
            let dx = (x as i32 - cel.x).rem_euclid(tw as i32) as u32;
            let dy = (y as i32 - cel.y).rem_euclid(th as i32) as u32;
            let Some(off) = crate::sprite::tile_pixel_offset(
                &ts,
                tile_index,
                dx,
                dy,
                tile.flip_x,
                tile.flip_y,
                tile.flip_diagonal,
            ) else {
                return;
            };
            (off, ts.base_index)
        };
        let _ = base;
        if !self.sprite.tilesets.iter().any(|t| t.id == ts_id) {
            return;
        }
        let before = self.sprite.clone();
        if let Some(ts) = self.sprite.tilesets.iter_mut().find(|t| t.id == ts_id)
            && let Some(slot) = ts.image.get_mut(atlas_idx)
        {
            *slot = color;
        }
        for layer in 0..self.sprite.layers.len() {
            if self.sprite.layers[layer].tilemap && self.sprite.layers[layer].tileset == ts_id {
                for frame in 0..self.sprite.frames.len() {
                    self.sprite.rasterize_tilemap(layer, frame);
                }
            }
        }
        self.snapshot(before);
        self.msg = "tile: pixel edited".into();
    }

    /// Pick the tile (and flips) under the cursor into the brush.
    pub fn pick_tile_at(&mut self, x: u32, y: u32) {
        let (l, f) = (self.active_layer, self.active_frame);
        let Some((_, tw, th, _)) = self.tilemap_target() else {
            self.msg = "tile: active layer is not an embedded tilemap".into();
            return;
        };
        let cel = &self.sprite.cels[l][f];
        let Some(tm) = &cel.tilemap else {
            self.msg = "tile: this cel has no tile references".into();
            return;
        };
        let tx = (x as i32 - cel.x).div_euclid(tw as i32);
        let ty = (y as i32 - cel.y).div_euclid(th as i32);
        if tx < 0 || ty < 0 || tx as u32 >= tm.tiles_w || ty as u32 >= tm.tiles_h.max(1) {
            self.msg = "tile: outside the tilemap bounds".into();
            return;
        }
        let idx = (ty as u32 * tm.tiles_w + tx as u32) as usize;
        if let Some(tile) = tm.tiles.get(idx) {
            self.tile_brush = tile.id;
            self.tile_flip_x = tile.flip_x;
            self.tile_flip_y = tile.flip_y;
            self.tile_flip_diagonal = tile.flip_diagonal;
            self.msg = format!("tile: picked {}", tile.id);
        }
    }

    pub fn erase_tile_at(&mut self, x: u32, y: u32) {
        let brush = self.tile_brush;
        self.tile_brush = 0;
        self.paint_tile_at(x, y);
        self.tile_brush = brush;
    }

    /// Clicking with the Text tool places (or moves) the text object.
    pub fn place_text_at(&mut self, x: u32, y: u32) {
        self.cursor = (x, y);
        if self.text.trim().is_empty() {
            self.msg = "text: type something in the panel first".into();
            return;
        }
        self.commit_text_edit();
        let Some((object, color)) = self.text_object_for(Some((x as i32, y as i32))) else {
            return;
        };
        self.text_edit_before = Some(Box::new(self.sprite.clone()));
        self.rebuild_text_cel(&object, color);
        self.commit_text_edit();
        self.msg = format!("text placed at {},{} ({} px)", x, y, object.size);
    }

    fn text_object_for(&self, at: Option<(i32, i32)>) -> Option<(TextObject, Rgba)> {
        let (l, f) = (self.active_layer, self.active_frame);
        let existing = self.sprite.cels[l][f]
            .user_data
            .as_ref()
            .and_then(|d| d.text_object());
        if existing.is_none() && self.text.is_empty() {
            return None;
        }
        let (cx, cy) = at.unwrap_or((self.cursor.0 as i32, self.cursor.1 as i32));
        let color = self.sprite.cels[l][f]
            .user_data
            .as_ref()
            .and_then(|d| d.color)
            .unwrap_or(self.fg);
        let mut object = existing.unwrap_or(TextObject {
            text: String::new(),
            font: "vixel".into(),
            size: self.text_scale.clamp(1, 8) * 8,
            align: self.text_align.min(2),
            antialias: false,
            x: cx,
            y: cy,
        });
        object.text = self.text.clone();
        object.font = self.text_font.clone();
        object.size = self.text_scale.clamp(1, 8) * 8;
        object.align = self.text_align.min(2);
        if let Some((x, y)) = at {
            object.x = x;
            object.y = y;
        }
        Some((object, color))
    }

    fn rebuild_text_cel(&mut self, object: &TextObject, color: Rgba) {
        let (l, f) = (self.active_layer, self.active_frame);
        let (w, h) = (self.sprite.width, self.sprite.height);
        {
            let cel = &mut self.sprite.cels[l][f];
            let (z, op) = (cel.z_index, cel.opacity);
            *cel = Cel::blank(w, h);
            cel.z_index = z;
            cel.opacity = op;
        }
        let ud = self.sprite.cels[l][f]
            .user_data
            .get_or_insert_with(Default::default);
        if !ud.set_text_object(object) {
            self.msg = "text object: cannot update properties".into();
            return;
        }
        ud.color = Some(color);
        self.rasterize_text_object(object, color);
    }

    fn rasterize_text_object(&mut self, object: &TextObject, color: Rgba) {
        let scale = (object.size as usize / 8).clamp(1, 8);
        let (l, f) = (self.active_layer, self.active_frame);
        let lines: Vec<&str> = object.text.split('\n').collect();
        let widest = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0)
            * 9
            * scale;
        for (li, line) in lines.iter().enumerate() {
            let width = line.chars().count() * 9 * scale;
            let offset = match object.align {
                1 => (widest - width) / 2,
                2 => widest - width,
                _ => 0,
            };
            let mut pen = object.x + offset as i32;
            let baseline = object.y + li as i32 * 9 * scale as i32;
            for ch in line.chars() {
                use font8x8::UnicodeFonts;
                if let Some(bits) = font8x8::BASIC_FONTS.get(ch) {
                    for (col, rows) in bits.iter().enumerate() {
                        for row in 0..8 {
                            if rows & (1 << row) == 0 {
                                continue;
                            }
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    let x = pen + col as i32 * scale as i32 + sx as i32;
                                    let y = baseline + row * scale as i32 + sy as i32;
                                    if x >= 0
                                        && y >= 0
                                        && (x as u32) < self.sprite.width
                                        && (y as u32) < self.sprite.height
                                    {
                                        self.sprite.set(l, f, x as u32, y as u32, color);
                                    }
                                }
                            }
                        }
                    }
                }
                pen += 9 * scale as i32;
            }
        }
    }

    pub fn text_object_summary(&self) -> String {
        let (l, f) = (self.active_layer, self.active_frame);
        match self.sprite.cels[l][f]
            .user_data
            .as_ref()
            .and_then(|d| d.text_object())
        {
            Some(o) => format!(
                "text object: {:?}  {} px  at {},{}  align {}  antialias {}",
                o.text, o.size, o.x, o.y, o.align, o.antialias
            ),
            None => "no text object on this cel".into(),
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
        let brush = self.current_brush();
        for (x, y) in pts {
            let (x, y) = if self.tile_mode {
                self.wrap_point(x as i32, y as i32)
            } else {
                (x, y)
            };
            self.stamp_brush_symmetric(&brush, x, y, color);
        }
        self.end_stroke();
    }

    /// Set the sprite-properties background colour (user-data colour) as one
    /// undo step. `None` clears it while preserving other user data.
    pub fn set_background_color(&mut self, color: Option<Rgba>) {
        let before = self.sprite.user_data.clone();
        self.sprite.set_background_color(color);
        let after = self.sprite.user_data.clone();
        if before == after {
            return;
        }
        self.push_undo(UndoStep::SpriteUserData { before, after });
        self.touch();
        self.dirty = true;
    }

    /// Finish a script: if its API calls changed the sprite, record the whole
    /// script as one snapshot undo step.
    pub fn commit_script(&mut self, before: Sprite, before_revision: u64) {
        if self.revision == before_revision {
            return;
        }
        let after = self.sprite.clone();
        self.push_undo(UndoStep::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        });
        self.dirty = true;
    }

    /// Direct pixel write used by scripting (no active-tool ink/selection).
    pub fn script_set_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        if x >= self.sprite.width || y >= self.sprite.height {
            return;
        }
        self.sprite
            .set(self.active_layer, self.active_frame, x, y, color);
        self.touch();
        self.dirty = true;
    }

    pub fn script_add_layer(&mut self) -> usize {
        let idx = self.active_layer;
        self.sprite.add_layer_after(idx);
        self.active_layer = (idx + 1).min(self.sprite.layers.len() - 1);
        self.touch();
        self.dirty = true;
        self.active_layer
    }

    pub fn script_add_frame(&mut self) -> usize {
        let idx = self.active_frame;
        self.sprite.add_frame_after(idx);
        self.active_frame = (idx + 1).min(self.sprite.frames.len() - 1);
        self.touch();
        self.dirty = true;
        self.active_frame
    }

    pub fn pick_color(&mut self, x: u32, y: u32) {
        let comp = self.sprite.composite(self.active_frame);
        let c = comp[(y * self.sprite.width + x) as usize];
        if !c.is_transparent() {
            self.set_fg(c);
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
                UndoStep::CelZ {
                    layer,
                    frame,
                    before,
                    ..
                } => {
                    self.sprite.cels[*layer][*frame].z_index = *before;
                }
                UndoStep::SpriteUserData { before, .. } => {
                    self.sprite.user_data = before.clone();
                }
            }
            self.push_redo(step);
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
                UndoStep::CelZ {
                    layer,
                    frame,
                    after,
                    ..
                } => {
                    self.sprite.cels[*layer][*frame].z_index = *after;
                }
                UndoStep::SpriteUserData { after, .. } => {
                    self.sprite.user_data = after.clone();
                }
            }
            self.push_undo(step);
            self.dirty = true;
            self.touch();
        }
    }

    pub fn history_labels(&self) -> Vec<String> {
        self.undo.iter().map(UndoStep::label).collect()
    }

    pub fn redo_labels(&self) -> Vec<String> {
        self.redo.iter().rev().map(UndoStep::label).collect()
    }

    /// Jump to a linear history position (0 = before the first undo step).
    /// Within one linear stack this is the non-linear navigation the panel
    /// offers; branching would need per-node inverse patches.
    pub fn goto_history(&mut self, index: usize) {
        let index = index.min(self.undo.len() + self.redo.len());
        while self.undo.len() > index {
            self.undo();
        }
        while self.undo.len() < index {
            if self.redo.is_empty() {
                break;
            }
            self.redo();
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
        self.load_palette_named(colors, Vec::new());
    }

    /// Replace the palette and its names. Indexed sprites insert a transparent
    /// slot if needed and remap the pixels when the colours changed.
    pub fn load_palette_named(&mut self, mut colors: Vec<Rgba>, mut names: Vec<String>) {
        if colors.is_empty() {
            return;
        }
        names.resize(colors.len(), String::new());
        names.truncate(colors.len());
        if self.sprite.is_indexed() {
            if !colors.iter().any(|c| c.3 == 0) {
                colors.insert(0, Rgba::TRANSPARENT);
                names.insert(0, String::new());
            }
            colors.truncate(256);
            names.truncate(colors.len());
            let remap = colors != self.sprite.palette;
            self.sprite.palette = colors.clone();
            self.sprite.palette_names = names.clone();
            let ti = self.sprite.transparent_slot();
            self.sprite.transparent_index = ti;
            if remap {
                self.sprite.remap_to_palette();
            }
            self.dirty = true;
            self.touch();
        }
        self.palette = colors;
        self.palette_names = names;
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
            let at = self.palette.len();
            self.insert_palette_at(at, self.fg);
            self.fg_idx = at;
        }
    }

    /// Insert a colour at `index` (clamped), with an empty name. Indexed
    /// sprites keep the new entry in the document palette immediately.
    pub fn insert_palette_at(&mut self, index: usize, color: Rgba) {
        if self.palette.len() >= 256 {
            self.msg = "palette: 256 entries max".into();
            return;
        }
        self.palette_names.resize(self.palette.len(), String::new());
        let i = index.min(self.palette.len());
        self.palette.insert(i, color);
        self.palette_names.insert(i, String::new());
        if i <= self.fg_idx {
            self.fg_idx += 1;
        }
        if i <= self.bg_idx {
            self.bg_idx += 1;
        }
        self.sync_palette_to_sprite(false);
        self.msg = format!("palette: inserted at {i}");
    }

    /// Remove the entry at `index`; indexed pixels that used it snap to the
    /// nearest remaining colour.
    pub fn remove_palette_at(&mut self, index: usize) {
        if self.palette.len() <= 1 || index >= self.palette.len() {
            return;
        }
        self.palette_names.resize(self.palette.len(), String::new());
        self.palette.remove(index);
        self.palette_names.remove(index);
        let (fg, bg) = (self.fg, self.bg);
        self.fg_idx = self
            .palette
            .iter()
            .position(|c| *c == fg)
            .unwrap_or(index.min(self.palette.len() - 1));
        self.bg_idx = self.palette.iter().position(|c| *c == bg).unwrap_or(0);
        self.fg = self.palette[self.fg_idx];
        self.bg = self.palette[self.bg_idx];
        self.sync_palette_to_sprite(true);
        self.msg = format!("palette: removed {index}");
    }

    pub fn remove_palette_color(&mut self) {
        self.remove_palette_at(self.fg_idx);
    }

    /// Sort colours and names together; fg/bg keep their colours. `key` is
    /// `rgb` (default), `luma` or `hue`.
    pub fn sort_palette(&mut self, key: &str) {
        self.palette_names.resize(self.palette.len(), String::new());
        let mut order: Vec<usize> = (0..self.palette.len()).collect();
        let key = key.to_ascii_lowercase();
        match key.as_str() {
            "hue" => order.sort_by(|&a, &b| {
                let (ha, sa, va) = crate::color::to_hsv(self.palette[a]);
                let (hb, sb, vb) = crate::color::to_hsv(self.palette[b]);
                ha.total_cmp(&hb)
                    .then(sa.total_cmp(&sb))
                    .then(va.total_cmp(&vb))
            }),
            "luma" | "luminance" => order.sort_by_key(|&i| {
                let c = self.palette[i];
                299 * u32::from(c.0) + 587 * u32::from(c.1) + 114 * u32::from(c.2)
            }),
            _ => order.sort_by_key(|&i| {
                let c = self.palette[i];
                (c.0, c.1, c.2)
            }),
        }
        let (fg, bg) = (self.fg, self.bg);
        self.palette = order.iter().map(|&i| self.palette[i]).collect();
        let old_names = std::mem::take(&mut self.palette_names);
        self.palette_names = order
            .iter()
            .map(|&i| old_names.get(i).cloned().unwrap_or_default())
            .collect();
        self.fg_idx = self.palette.iter().position(|c| *c == fg).unwrap_or(0);
        self.bg_idx = self.palette.iter().position(|c| *c == bg).unwrap_or(0);
        self.fg = fg;
        self.bg = bg;
        self.sync_palette_to_sprite(false);
        self.msg = format!("palette: sorted by {key}");
    }

    pub fn set_palette_name(&mut self, index: usize, name: &str) {
        if index >= self.palette.len() {
            return;
        }
        self.palette_names.resize(self.palette.len(), String::new());
        self.palette_names[index] = name.to_string();
        if self.sprite.is_indexed() {
            self.sprite.palette_names = self.palette_names.clone();
            self.dirty = true;
            self.touch();
        }
        self.msg = format!("palette: name {index}");
    }

    pub fn palette_name(&self, index: usize) -> &str {
        self.palette_names
            .get(index)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// Load a named preset (`default`, `gray`, `pico8`, `dawnbringer16`).
    pub fn load_palette_preset(&mut self, name: &str) -> bool {
        match crate::palette::preset(name) {
            Some(colors) => {
                let n = colors.len();
                self.load_palette(colors);
                self.msg = format!("palette: {name} ({n})");
                true
            }
            None => {
                self.msg = format!("unknown palette preset: {name}");
                false
            }
        }
    }

    /// Mirror the picker palette into the document when the sprite is indexed.
    fn sync_palette_to_sprite(&mut self, remap: bool) {
        if !self.sprite.is_indexed() {
            return;
        }
        self.palette_names.resize(self.palette.len(), String::new());
        self.palette_names.truncate(self.palette.len());
        self.sprite.palette = self.palette.clone();
        self.sprite.palette_names = self.palette_names.clone();
        let ti = self.sprite.transparent_slot();
        self.sprite.transparent_index = ti;
        if remap {
            self.sprite.remap_to_palette();
        }
        self.dirty = true;
        self.touch();
    }

    /// Move a palette entry, keeping fg/bg pointing at the same colour.
    pub fn move_palette(&mut self, from: usize, to: usize) {
        if from >= self.palette.len() || to >= self.palette.len() || from == to {
            return;
        }
        self.palette_names.resize(self.palette.len(), String::new());
        let c = self.palette.remove(from);
        self.palette.insert(to, c);
        let name = self.palette_names.remove(from);
        self.palette_names.insert(to, name);
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
        self.sync_palette_to_sprite(false);
        self.msg = format!("palette {from} -> {to}");
    }

    /// Set fg/bg from `#rrggbb`, as the picker's hex field and `:fg` do.
    pub fn set_fg(&mut self, colour: Rgba) {
        self.fg = colour;
        if let Some(i) = self.palette.iter().position(|c| *c == colour) {
            self.fg_idx = i;
        }
        self.note_color(colour);
    }

    fn note_color(&mut self, colour: Rgba) {
        if colour.3 == 0 {
            return;
        }
        self.recent_colors.retain(|c| *c != colour);
        self.recent_colors.insert(0, colour);
        self.recent_colors.truncate(12);
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
                    user_data: None,
                });
                self.sprite.tags.len() - 1
            }
        };
        self.active_tag = Some(idx);
        self.msg = format!("tag '{name}': frames {a}-{b}");
        self.record_structure(before, at);
    }

    /// Rename/re-range one tag as a structural undo step.
    pub fn update_tag(&mut self, idx: usize, name: &str, from: usize, to: usize) {
        if idx >= self.sprite.tags.len() || name.is_empty() {
            return;
        }
        let last = self.sprite.frames.len().saturating_sub(1);
        let (a, b) = (from.min(to).min(last), from.max(to).min(last));
        let before = self.sprite.clone();
        let at = self.edit_point();
        let tag = &mut self.sprite.tags[idx];
        tag.name = name.to_string();
        tag.from = a;
        tag.to = b;
        self.msg = format!("tag '{name}': frames {a}-{b}");
        self.record_structure(before, at);
    }

    /// Set the active tag's colour as a structural undo step.
    pub fn set_tag_color(&mut self, color: Rgba) {
        let Some(idx) = self.active_tag else {
            return;
        };
        if idx >= self.sprite.tags.len() {
            return;
        }
        let before = self.sprite.clone();
        let at = self.edit_point();
        self.sprite.tags[idx].color = color;
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
        use crate::sprite::PlayDirection;
        self.play_passes = 0;
        let dir = self
            .active_tag
            .and_then(|i| self.sprite.tags.get(i))
            .map(|t| t.direction);
        self.play_dir = if dir == Some(PlayDirection::PingPongReverse) {
            -1
        } else {
            1
        };
        if dir == Some(PlayDirection::PingPongReverse) {
            self.active_frame = self.play_range().1;
        }
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
            PlayDirection::PingPong | PlayDirection::PingPongReverse => {
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

    /// Set or remove the embedded `.ase` colour-profile chunk.
    pub fn set_ase_profile(&mut self, profile: Option<AseProfile>) {
        self.sprite.ase_profile = profile;
        self.dirty = true;
        self.touch();
        self.msg = self.profile_summary();
    }

    /// One-line description of the embedded `.ase` colour profile.
    pub fn profile_summary(&self) -> String {
        match &self.sprite.ase_profile {
            None => "profile: none (no 0x2007 chunk)".into(),
            Some(p) => {
                let mut s = match p.kind {
                    0 => "profile: none".to_string(),
                    1 => "profile: sRGB".to_string(),
                    2 => format!("profile: ICC ({} bytes)", p.icc.len()),
                    k => format!("profile: type {k}"),
                };
                if p.flags & 1 != 0 || p.gamma > 0.0 {
                    s.push_str(&format!(", gamma {:.4}", p.gamma));
                }
                s
            }
        }
    }

    pub fn set_color_profile(&mut self, to: crate::sprite::ColorProfile) {
        use crate::sprite::ColorProfile;
        if to == self.sprite.color_profile {
            self.msg = format!("profile: {}", to.name());
            return;
        }
        let before = self.sprite.clone();
        self.sprite.convert_profile(to);
        let after = self.sprite.clone();
        self.push_undo(UndoStep::Snapshot {
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
                self.push_undo(UndoStep::Snapshot {
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
                if self.sprite.palette.is_empty() {
                    self.sprite.build_palette();
                }
                self.sprite.dither_to_palette(self.dither);
                let after = self.sprite.clone();
                self.push_undo(UndoStep::Snapshot {
                    before: Box::new(before),
                    after: Box::new(after),
                });
                self.redo.clear();
                self.palette_lock = false;
                self.load_palette_named(
                    self.sprite.palette.clone(),
                    self.sprite.palette_names.clone(),
                );
            }
        }
        self.dirty = true;
        self.touch();
        self.msg = format!("mode: {}", mode.name());
    }

    /// Choose the dithering used for indexed conversion. A non-`None` mode
    /// converts the sprite immediately (one undo step); `None` only sets the
    /// preference for the next `:mode indexed`.
    pub fn set_dither(&mut self, mode: DitherMode) {
        self.dither = mode;
        if mode != DitherMode::None {
            let before = self.sprite.clone();
            if self.sprite.palette.is_empty() {
                self.sprite.build_palette();
            }
            self.sprite.dither_to_palette(mode);
            let after = self.sprite.clone();
            self.push_undo(UndoStep::Snapshot {
                before: Box::new(before),
                after: Box::new(after),
            });
            self.redo.clear();
            self.palette_lock = false;
            self.load_palette_named(
                self.sprite.palette.clone(),
                self.sprite.palette_names.clone(),
            );
            self.dirty = true;
            self.touch();
        }
        self.msg = format!("dither: {}", mode.name());
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

    pub fn set_sprite_grid_origin(&mut self, x: i32, y: i32) {
        self.sprite.grid_origin = (
            x.clamp(i16::MIN as i32, i16::MAX as i32),
            y.clamp(i16::MIN as i32, i16::MAX as i32),
        );
        self.dirty = true;
        self.msg = format!("sprite grid origin: {},{}", x, y);
    }

    pub fn set_sprite_grid_bounds(&mut self, x: i32, y: i32, w: u32, h: u32) {
        self.set_sprite_grid(w, h);
        self.set_sprite_grid_origin(x, y);
        self.msg = format!("sprite grid: {}x{} at {},{}", w, h, x, y);
    }

    // ---- free transform ----

    /// Rotate, scale and skew the selection (or the whole cel) about its own
    /// centre, nearest-neighbour, clipped to the sprite. One undo step. All
    /// three are one affine map, so they compose in the order rotate, scale,
    /// skew; the inverse map is what samples the source.
    pub fn free_transform(&mut self, angle_deg: f32, sx: f32, sy: f32, skew_deg: f32) {
        let targets = self.cel_targets();
        if targets.len() <= 1 {
            self.free_transform_one(angle_deg, sx, sy, skew_deg);
            return;
        }
        let before = self.sprite.clone();
        let (saved_layer, saved_frame) = (self.active_layer, self.active_frame);
        let undo_len = self.undo.len();
        self.fanout_suspended = true;
        for (layer, frame) in targets.iter().copied() {
            self.active_layer = layer;
            self.active_frame = frame;
            self.free_transform_one(angle_deg, sx, sy, skew_deg);
        }
        self.fanout_suspended = false;
        self.active_layer = saved_layer;
        self.active_frame = saved_frame;
        self.undo.truncate(undo_len);
        self.snapshot(before);
        self.msg = format!(
            "transform: {} cels (rotate {angle_deg:.0} deg, scale {sx:.2}x{sy:.2}, skew {skew_deg:.0} deg)",
            targets.len()
        );
    }

    fn free_transform_one(&mut self, angle_deg: f32, sx: f32, sy: f32, skew_deg: f32) {
        if !self.sprite.layer_movable(self.active_layer) {
            self.msg = "layer movement is locked".into();
            return;
        }
        let (w, h) = (self.sprite.width, self.sprite.height);
        let (l, f) = (self.active_layer, self.active_frame);
        let sel = self.current_selection();
        let (rx, ry, rw, rh) = match &sel {
            Some(s) => (s.x, s.y, s.w, s.h),
            None => (0, 0, w, h),
        };
        let (sx, sy) = (sx.abs().max(0.01), sy.abs().max(0.01));
        let centre = (rx as f32 + rw as f32 / 2.0, ry as f32 + rh as f32 / 2.0);
        let (px0, py0) = self.transform_pivot.unwrap_or(centre);
        let (sin, cos) = (-angle_deg).to_radians().sin_cos();
        let tan = (-skew_deg).to_radians().tan();
        let rotsprite = self.transform_rotsprite
            && (angle_deg.abs() > 0.01 || sx != 1.0 || sy != 1.0 || skew_deg.abs() > 0.01);
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
                    self.put_pixel(x, y, Rgba::TRANSPARENT);
                }
            }
        }
        for y in ry..ry + rh {
            for x in rx..rx + rw {
                if !inside(x, y) || x >= w || y >= h {
                    continue;
                }
                if rotsprite {
                    // RotSprite in spirit: sample a 4x4 grid inside the
                    // destination pixel and keep the most frequent source
                    // colour, so clusters survive where nearest would alias.
                    let mut counts: Vec<(Rgba, u32)> = Vec::new();
                    for suby in 0..4 {
                        for subx in 0..4 {
                            let mut dx = x as f32 + (subx as f32 + 0.5) / 4.0 - px0;
                            let mut dy = y as f32 + (suby as f32 + 0.5) / 4.0 - py0;
                            dx -= tan * dy;
                            dx /= sx;
                            dy /= sy;
                            let (sxf, syf) = (dx * cos - dy * sin, dx * sin + dy * cos);
                            let (spx, spy) = ((px0 + sxf).floor(), (py0 + syf).floor());
                            if spx < 0.0 || spy < 0.0 || spx >= w as f32 || spy >= h as f32 {
                                continue;
                            }
                            let (spx, spy) = (spx as u32, spy as u32);
                            if !inside(spx, spy) {
                                continue;
                            }
                            let c = src[(spy * w + spx) as usize];
                            if c.3 == 0 {
                                continue;
                            }
                            if let Some(slot) = counts.iter_mut().find(|(k, _)| *k == c) {
                                slot.1 += 1;
                            } else {
                                counts.push((c, 1));
                            }
                        }
                    }
                    if let Some((c, _)) = counts.iter().max_by_key(|(_, n)| *n) {
                        self.paint_pixel(x, y, *c);
                    }
                    continue;
                }
                let (mut dx, mut dy) = (x as f32 + 0.5 - px0, y as f32 + 0.5 - py0);
                dx -= tan * dy;
                dx /= sx;
                dy /= sy;
                let (sxf, syf) = (dx * cos - dy * sin, dx * sin + dy * cos);
                let (px, py) = ((px0 + sxf).floor(), (py0 + syf).floor());
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
        let dst = apply_filter_parallel(&src, w, h, f, self.fg);
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
            self.put_pixel(x, y, dst[i]);
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

    /// Fill the current selection (or the whole cel when there is none) with
    /// one colour, as a single undo step.
    pub fn fill_selection(&mut self, color: Rgba) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let sel = self.current_selection();
        self.begin_stroke();
        for y in 0..h {
            for x in 0..w {
                if let Some(s) = &sel
                    && !s.contains(x, y)
                {
                    continue;
                }
                self.paint_pixel(x, y, color);
            }
        }
        self.end_stroke();
        self.msg = "selection: fill".into();
    }

    /// Paint `width` pixels along the selection edge, inside or outside it.
    pub fn stroke_selection(&mut self, width: u32, inside: bool, color: Rgba) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        if self.current_selection().is_none() {
            self.msg = "stroke: no selection".into();
            return;
        }
        let n = width.clamp(1, 64);
        let mut ring = self.selection_grid();
        if inside {
            let mut inner = ring.clone();
            for _ in 0..n {
                inner = erode(&inner, w, h);
            }
            for i in 0..ring.len() {
                ring[i] = ring[i] && !inner[i];
            }
        } else {
            let mut outer = ring.clone();
            for _ in 0..n {
                outer = dilate(&outer, w, h);
            }
            for i in 0..ring.len() {
                ring[i] = outer[i] && !ring[i];
            }
        }
        self.begin_stroke();
        for y in 0..h {
            for x in 0..w {
                if ring[(y * w + x) as usize] {
                    self.put_pixel_unclipped(x, y, color);
                }
            }
        }
        self.end_stroke();
        self.msg = format!(
            "selection: stroke {n} {}",
            if inside { "inside" } else { "outside" }
        );
    }

    /// Select every pixel close to `color`, on the active layer or the
    /// flattened frame.
    pub fn selection_from_color(&mut self, color: Rgba, tolerance: u8, all_layers: bool) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let (l, f) = (self.active_layer, self.active_frame);
        let comp = all_layers.then(|| self.sprite.composite(f));
        let mut g = vec![false; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = match &comp {
                    Some(c) => c[(y * w + x) as usize],
                    None => self.sprite.get(l, f, x, y),
                };
                g[(y * w + x) as usize] = if color.3 == 0 {
                    p.3 == 0
                } else {
                    p.3 > 0 && color_close(p, color, tolerance)
                };
            }
        }
        self.grid_to_selection(&g);
        self.msg = format!("selection: colour (tolerance {tolerance})");
    }

    /// Select the opaque pixels of the active layer (or the flattened frame).
    pub fn selection_from_alpha(&mut self, all_layers: bool) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let (l, f) = (self.active_layer, self.active_frame);
        let comp = all_layers.then(|| self.sprite.composite(f));
        let mut g = vec![false; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = match &comp {
                    Some(c) => c[(y * w + x) as usize],
                    None => self.sprite.get(l, f, x, y),
                };
                g[(y * w + x) as usize] = p.3 > 0;
            }
        }
        self.grid_to_selection(&g);
        self.msg = "selection: layer alpha".into();
    }

    /// Rotate/scale/skew the selection mask itself, without touching pixels.
    pub fn transform_selection_mask(&mut self, angle_deg: f32, sx: f32, sy: f32, skew_deg: f32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let src = self.selection_grid();
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if src[(y * w + x) as usize] {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x0 == u32::MAX {
            self.msg = "selection: none".into();
            return;
        }
        let centre = (
            x0 as f32 + (x1 - x0 + 1) as f32 / 2.0,
            y0 as f32 + (y1 - y0 + 1) as f32 / 2.0,
        );
        let (px, py) = self.transform_pivot.unwrap_or(centre);
        let (sx, sy) = (sx.abs().max(0.01), sy.abs().max(0.01));
        let (sin, cos) = (-angle_deg).to_radians().sin_cos();
        let tan = (-skew_deg).to_radians().tan();
        let mut out = vec![false; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut dx = x as f32 + 0.5 - px;
                let mut dy = y as f32 + 0.5 - py;
                dx -= tan * dy;
                dx /= sx;
                dy /= sy;
                let (sxf, syf) = (dx * cos - dy * sin, dx * sin + dy * cos);
                let (spx, spy) = ((px + sxf).floor(), (py + syf).floor());
                if spx < 0.0 || spy < 0.0 || spx >= w as f32 || spy >= h as f32 {
                    continue;
                }
                if src[(spy as u32 * w + spx as u32) as usize] {
                    out[(y * w + x) as usize] = true;
                }
            }
        }
        self.grid_to_selection(&out);
        self.msg =
            format!("selection transform: {angle_deg:.0} deg, {sx:.2}x{sy:.2}, skew {skew_deg:.0}");
    }

    /// Move the selection mask without moving pixels.
    pub fn offset_selection(&mut self, dx: i32, dy: i32) {
        let (w, h) = (self.sprite.width, self.sprite.height);
        let src = self.selection_grid();
        let mut out = vec![false; src.len()];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                if !src[(y as u32 * w + x as u32) as usize] {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                    out[(ny as u32 * w + nx as u32) as usize] = true;
                }
            }
        }
        self.grid_to_selection(&out);
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
            (0..(w * h))
                .map(|i| {
                    self.sprite
                        .get(self.active_layer, self.active_frame, i % w, i / w)
                })
                .collect()
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
        if self.cel_move.is_some() {
            self.commit_cel_move();
        }
        if self.tool == Tool::Text && tool != Tool::Text {
            self.commit_text_edit();
        }
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
        if !self.sprite.layer_movable(self.active_layer) {
            self.msg = "layer movement is locked".into();
            return;
        }
        if let Some(sel) = self.current_selection() {
            self.clipboard = Some(self.selection_to_clipboard(&sel));
            self.clear_pixels(&sel);
        }
        self.clear_selection();
        self.mode = Mode::Normal;
    }

    /// Cut the selection and float it (move), entering paste mode.
    pub fn start_move(&mut self) {
        if !self.sprite.layer_movable(self.active_layer) {
            self.msg = "layer movement is locked".into();
            return;
        }
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
                    self.put_pixel(sel.x + xx, sel.y + yy, Rgba::TRANSPARENT);
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

    // ---- layer-content move ----

    /// Detach the active cel at `(x, y)` for dragging. The source is emptied
    /// and the content floats until `commit_cel_move`; locked, hidden,
    /// reference and empty cels refuse to start.
    pub fn start_cel_move(&mut self, x: u32, y: u32) -> bool {
        if self.cel_move.is_some() {
            return true;
        }
        let l = self.active_layer;
        if !self.sprite.layer_movable(l) {
            self.msg = "layer movement is locked".into();
            return false;
        }
        if !self.sprite.layer_editable(l) {
            self.msg = "layer is locked or hidden".into();
            return false;
        }
        let cel = self.active_cel_content();
        if cel.w == 0 || cel.h == 0 || cel.pixels.iter().all(|p| p.3 == 0) {
            self.msg = "cel is empty".into();
            return false;
        }
        let before = Box::new(self.sprite.clone());
        self.empty_cel_at(l, self.active_frame);
        self.cel_move = Some(CelMove {
            pos: (cel.x, cel.y),
            cel,
            from: (x, y),
            before,
        });
        self.msg = "move: drag the cel content".into();
        true
    }

    /// Move the floating content so the pixel under the cursor follows it.
    pub fn move_cel_to(&mut self, x: u32, y: u32) {
        if let Some(m) = &mut self.cel_move {
            m.pos = (
                m.cel.x + x as i32 - m.from.0 as i32,
                m.cel.y + y as i32 - m.from.1 as i32,
            );
        }
    }

    /// Bake the floating cel into the active cel, clipped to the canvas, as a
    /// single undo step. A zero move restores the source untouched.
    pub fn commit_cel_move(&mut self) {
        let Some(m) = self.cel_move.take() else {
            return;
        };
        let (w, h) = (self.sprite.width as i32, self.sprite.height as i32);
        let (dx, dy) = (m.pos.0 - m.cel.x, m.pos.1 - m.cel.y);
        let (l, f) = (self.active_layer, self.active_frame);
        for yy in 0..m.cel.h as i32 {
            for xx in 0..m.cel.w as i32 {
                let c = m.cel.get(m.cel.x + xx, m.cel.y + yy);
                let (x, y) = (m.cel.x + xx + dx, m.cel.y + yy + dy);
                if c.3 == 0 || x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                self.sprite.set(l, f, x as u32, y as u32, c);
            }
        }
        self.snapshot(*m.before);
        self.msg = if dx == 0 && dy == 0 {
            "move: unchanged".into()
        } else {
            format!("cel moved {dx:+},{dy:+}")
        };
    }

    /// Abandon the drag and put the cel back where it was.
    pub fn cancel_cel_move(&mut self) {
        if let Some(m) = self.cel_move.take() {
            self.sprite = *m.before;
            self.dirty = true;
            self.touch();
            self.msg = "move cancelled".into();
        }
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
            lock_movement: l.lock_movement,
            collapsed: l.collapsed,
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
            l.lock_movement = p.lock_movement;
            l.collapsed = p.collapsed;
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
        self.push_undo(UndoStep::LayerProps {
            layer,
            before,
            after,
        });
        self.redo.clear();
        self.dirty = true;
        self.touch();
    }

    /// Link the active cel to `target` (same layer, frame index).
    pub fn link_cel(&mut self, target: usize) {
        let (l, f) = (self.active_layer, self.active_frame);
        let before = self.sprite.clone();
        if self.sprite.link_cel(l, f, target) {
            self.snapshot(before);
            self.msg = format!("cel linked to frame {target}");
        } else {
            self.msg = "cannot link that frame".into();
        }
    }

    /// Break the active cel's link, copying the shared pixels into it.
    pub fn unlink_cel(&mut self) {
        let (l, f) = (self.active_layer, self.active_frame);
        let before = self.sprite.clone();
        if self.sprite.unlink_cel(l, f) {
            self.snapshot(before);
            self.msg = "cel unlinked".into();
        } else {
            self.msg = "cel is not linked".into();
        }
    }

    pub fn set_layer_lock_movement(&mut self, on: bool) {
        let l = self.active_layer;
        let before = self.layer_props(l);
        self.sprite.layers[l].lock_movement = on;
        self.record_layer_props(l, before);
        self.msg = if on {
            "layer movement locked".into()
        } else {
            "layer movement unlocked".into()
        };
    }

    pub fn set_layer_collapsed(&mut self, on: bool) {
        let l = self.active_layer;
        let before = self.layer_props(l);
        self.sprite.layers[l].collapsed = on;
        self.record_layer_props(l, before);
        self.msg = if on {
            "group collapsed".into()
        } else {
            "group expanded".into()
        };
    }

    pub fn layer_uuid_hex(&self) -> String {
        match self.sprite.layers[self.active_layer].uuid {
            Some(u) => u.iter().map(|b| format!("{b:02x}")).collect(),
            None => "none".into(),
        }
    }

    pub fn new_layer_uuid(&mut self) {
        let l = self.active_layer;
        let before = self.sprite.clone();
        self.sprite.layers[l].uuid = Some(crate::sprite::mint_uuid(l));
        self.snapshot(before);
        self.msg = format!("layer uuid: {}", self.layer_uuid_hex());
    }

    pub fn clear_layer_uuid(&mut self) {
        let l = self.active_layer;
        if self.sprite.layers[l].uuid.is_none() {
            self.msg = "layer has no uuid".into();
            return;
        }
        let before = self.sprite.clone();
        self.sprite.layers[l].uuid = None;
        self.snapshot(before);
        self.msg = "layer uuid cleared".into();
    }

    pub fn set_prefer_linked(&mut self, on: bool) {
        let l = self.active_layer;
        if self.sprite.layers[l].prefer_linked == on {
            self.msg = format!("prefer linked: {on}");
            return;
        }
        let before = self.sprite.clone();
        self.sprite.layers[l].prefer_linked = on;
        self.snapshot(before);
        self.msg = if on {
            "layer prefers linked cels".into()
        } else {
            "layer does not prefer linked cels".into()
        };
    }

    /// Set the active cel's z-index (positive = towards the front).
    pub fn set_cel_z(&mut self, z: i16) {
        let (l, f) = (self.active_layer, self.active_frame);
        let before = self.sprite.cels[l][f].z_index;
        if before == z {
            return;
        }
        self.sprite.cels[l][f].z_index = z;
        self.push_undo(UndoStep::CelZ {
            layer: l,
            frame: f,
            before,
            after: z,
        });
        self.redo.clear();
        self.dirty = true;
        self.touch();
        self.msg = format!("cel z: {z}");
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
        self.push_undo(UndoStep::Structure {
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
        self.push_undo(UndoStep::Snapshot {
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
        self.sprite.materialize_links();
        let (width, height) = (self.sprite.width as i32, self.sprite.height as i32);
        for cel in &mut self.sprite.cels[l] {
            for yy in 0..cel.h {
                for xx in 0..cel.w {
                    let gx = cel.x + xx as i32;
                    let gy = cel.y + yy as i32;
                    let i = (yy as usize) * cel.w as usize + xx as usize;
                    let m = if gx >= 0 && gy >= 0 && gx < width && gy < height {
                        *mask
                            .pixels
                            .get((gy as u32 * width as u32 + gx as u32) as usize)
                            .unwrap_or(&255) as u32
                    } else {
                        255
                    };
                    if let Some(px) = cel.pixels.get_mut(i) {
                        px.3 = (px.3 as u32 * m / 255) as u8;
                    }
                }
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
        self.push_undo(UndoStep::Snapshot {
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
                    self.put_pixel_unclipped(ox + xx, oy + yy, final_grid[i]);
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

    /// Save the sprite to a path chosen by the caller, dispatching on the
    /// extension (`.ase`/`.aseprite`, image formats, otherwise `.vpx` JSON).
    pub fn save_path(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        if crate::io::is_ase(path) {
            crate::ase::save(&self.sprite, path)?;
        } else if crate::io::is_image(path) {
            crate::io::export_png_scaled(&self.sprite, self.active_frame, path, 1)?;
        } else {
            crate::io::save_project(&self.sprite, path)?;
        }
        self.note_recent(path.to_path_buf());
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        self.msg = self.saved_note();
        Ok(())
    }

    /// Remember a file as recently opened, newest first and capped at 10.
    pub fn note_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(10);
    }

    /// Record a cleanly closed document, newest last, deduplicated and bounded
    /// so a long session cannot grow the stack forever.
    pub fn push_closed(&mut self, path: PathBuf) {
        self.closed.retain(|p| p != &path);
        self.closed.push(path);
        while self.closed.len() > CLOSED_LIMIT {
            self.closed.remove(0);
        }
    }

    /// Ask the app to reopen the most recently closed document in a new tab.
    pub fn request_reopen(&mut self) {
        match self.closed.last() {
            Some(path) => {
                self.msg = format!("reopen: {}", path.display());
                self.reopen_request = Some(path.clone());
            }
            None => self.msg = "no closed documents".into(),
        }
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
            self.load_palette_named(
                self.sprite.palette.clone(),
                self.sprite.palette_names.clone(),
            );
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

fn source_over(src: Rgba, dst: Rgba) -> Rgba {
    let sa = src.3 as f32 / 255.0;
    let da = dst.3 as f32 / 255.0;
    let ao = sa + da * (1.0 - sa);
    if ao <= 0.0 {
        return Rgba::TRANSPARENT;
    }
    let ch = |s: u8, d: u8| ((s as f32 * sa + d as f32 * da * (1.0 - sa)) / ao).round() as u8;
    Rgba(
        ch(src.0, dst.0),
        ch(src.1, dst.1),
        ch(src.2, dst.2),
        (ao * 255.0).round() as u8,
    )
}

fn luma(c: Rgba) -> u32 {
    299 * u32::from(c.0) + 587 * u32::from(c.1) + 114 * u32::from(c.2)
}

/// Apply one Aseprite ink to a destination pixel.
pub(crate) fn apply_ink(ink: Ink, old: Rgba, color: Rgba) -> Rgba {
    match ink {
        // Transparent paint has always meant "clear" for the internal callers
        // (cut, transform vacate, eraser fallbacks).
        Ink::Simple => {
            if color.3 == 0 {
                Rgba::TRANSPARENT
            } else {
                source_over(color, old)
            }
        }
        Ink::LockAlpha => {
            if old.3 == 0 {
                old
            } else {
                let mut out = source_over(color, old);
                out.3 = old.3;
                out
            }
        }
        Ink::Replace => color,
        Ink::Shading => {
            if old.3 == 0 {
                old
            } else {
                shade_color(old, luma(color) <= luma(old), 20)
            }
        }
        Ink::Erase => {
            let amount = if color.3 == 0 { 255 } else { color.3 };
            let a = old.3 - (old.3 as u32 * amount as u32 / 255) as u8;
            if a == 0 {
                Rgba::TRANSPARENT
            } else {
                Rgba(old.0, old.1, old.2, a)
            }
        }
    }
}

/// Darken or lighten a colour by `strength` percent, keeping alpha.
fn shade_color(c: Rgba, darken: bool, strength: u32) -> Rgba {
    let s = strength.clamp(1, 100) as f32 / 100.0;
    let f = if darken { 1.0 - s } else { 1.0 + s };
    let ch = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
    Rgba(ch(c.0), ch(c.1), ch(c.2), c.3)
}

/// Recursive Bayer matrix value in `0..n*n` for `(x, y)`.
fn bayer_value(n: u32, x: u32, y: u32) -> u32 {
    if n <= 1 {
        return 0;
    }
    let half = n / 2;
    let quadrant = match (x >= half, y >= half) {
        (false, false) => 0,
        (true, false) => 2,
        (false, true) => 3,
        (true, true) => 1,
    };
    4 * bayer_value(half, x % half, y % half) + quadrant
}

/// Ordered-dither an already-interpolated gradient colour. Alpha is left
/// continuous (dithering it would punch holes in the ramp).
fn gradient_dither(c: Rgba, x: u32, y: u32, mode: DitherMode) -> Rgba {
    let (n, levels) = match mode {
        DitherMode::Bayer2 => (2, 4.0),
        DitherMode::Bayer4 => (4, 8.0),
        DitherMode::Bayer8 => (8, 16.0),
        _ => return c,
    };
    let threshold = (bayer_value(n, x % n, y % n) as f32 + 0.5) / (n * n) as f32;
    let q = |v: u8| {
        let scaled = f32::from(v) / 255.0 * (levels - 1.0);
        let base = scaled.floor();
        let frac = scaled - base;
        let stepped = base + if frac > threshold { 1.0 } else { 0.0 };
        (stepped / (levels - 1.0) * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Rgba(q(c.0), q(c.1), q(c.2), c.3)
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
pub const FILTERS: [&str; 16] = [
    "invert",
    "brightness",
    "contrast",
    "hue",
    "saturation",
    "desaturate",
    "curves",
    "levels",
    "balance",
    "posterize",
    "outline",
    "blur",
    "sharpen",
    "median",
    "convolve",
    "emboss",
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
    Curves {
        shadows: u8,
        midtones: u8,
        highlights: u8,
    },
    Levels {
        in_black: u8,
        in_white: u8,
        gamma: f32,
        out_black: u8,
        out_white: u8,
    },
    Balance {
        r: i16,
        g: i16,
        b: i16,
    },
    Desaturate,
    Median(u32),
    Convolve {
        kernel: [i16; 9],
        divisor: f32,
        bias: i32,
    },
    Emboss,
    Posterize(u32),
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
            Filter::Curves {
                shadows,
                midtones,
                highlights,
            } => format!("curves {shadows} {midtones} {highlights}"),
            Filter::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            } => format!("levels {in_black} {in_white} {gamma:.2} {out_black} {out_white}"),
            Filter::Balance { r, g, b } => format!("balance {r} {g} {b}"),
            Filter::Desaturate => "desaturate".into(),
            Filter::Median(v) => format!("median {v}"),
            Filter::Convolve {
                kernel,
                divisor,
                bias,
            } => format!("convolve {kernel:?} /{divisor:.2} {bias:+}"),
            Filter::Emboss => "emboss".into(),
            Filter::Posterize(v) => format!("posterize {v}"),
        }
    }

    /// Parse `name [amount]`, the same table the `:filter` command uses.
    pub fn parse(name: &str, amount: Option<&str>) -> Option<Filter> {
        let mut args = vec![name];
        if let Some(a) = amount {
            args.push(a);
        }
        Filter::parse_args(&args)
    }

    /// Argument-aware parser for the multi-parameter filters.
    pub fn parse_args(args: &[&str]) -> Option<Filter> {
        let name = *args.first()?;
        let n = |i: usize| args.get(i).and_then(|v| v.parse::<f32>().ok());
        let u = |i: usize, default: u32| {
            args.get(i)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(default)
        };
        Some(match name {
            "invert" | "inv" => Filter::Invert,
            "brightness" | "bri" => Filter::Brightness(n(1).unwrap_or(24.0) as i32),
            "contrast" | "con" => Filter::Contrast(n(1).unwrap_or(1.2)),
            "hue" => Filter::Hue(n(1).unwrap_or(30.0)),
            "saturation" | "sat" => Filter::Saturation(n(1).unwrap_or(1.3)),
            "outline" | "out" => Filter::Outline(n(1).unwrap_or(1.0).max(1.0) as u32),
            "blur" => Filter::Blur(n(1).unwrap_or(1.0).max(1.0) as u32),
            "sharpen" | "sharp" => Filter::Sharpen(n(1).unwrap_or(1.0)),
            "desaturate" | "desat" | "gray" => Filter::Desaturate,
            "curves" | "curve" => Filter::Curves {
                shadows: u(1, 0).min(255) as u8,
                midtones: u(2, 128).min(255) as u8,
                highlights: u(3, 255).min(255) as u8,
            },
            "levels" | "level" => Filter::Levels {
                in_black: u(1, 0).min(255) as u8,
                in_white: u(2, 255).min(255) as u8,
                gamma: n(3).unwrap_or(1.0).clamp(0.1, 10.0),
                out_black: u(4, 0).min(255) as u8,
                out_white: u(5, 255).min(255) as u8,
            },
            "balance" | "colorbalance" | "cb" => Filter::Balance {
                r: n(1).unwrap_or(0.0).clamp(-100.0, 100.0) as i16,
                g: n(2).unwrap_or(0.0).clamp(-100.0, 100.0) as i16,
                b: n(3).unwrap_or(0.0).clamp(-100.0, 100.0) as i16,
            },
            "median" => Filter::Median(u(1, 1).clamp(1, 8)),
            "posterize" | "poster" => Filter::Posterize(u(1, 4).clamp(2, 32)),
            "emboss" => Filter::Emboss,
            "convolve" | "conv" => {
                let mut kernel = [0i16; 9];
                for (i, slot) in kernel.iter_mut().enumerate() {
                    *slot = args
                        .get(i + 1)
                        .and_then(|v| v.parse::<i16>().ok())
                        .unwrap_or(0);
                }
                let divisor = args
                    .get(10)
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(1.0);
                let bias = args
                    .get(11)
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0);
                Filter::Convolve {
                    kernel,
                    divisor: if divisor == 0.0 { 1.0 } else { divisor },
                    bias,
                }
            }
            _ => return None,
        })
    }
}

/// Pointwise filters read only their own pixel, so rows can be split across
/// workers. Neighborhood filters (blur/sharpen/outline/median/convolve/emboss)
/// stay synchronous because banding would change their result.
fn pointwise(f: Filter) -> bool {
    matches!(
        f,
        Filter::Invert
            | Filter::Brightness(_)
            | Filter::Contrast(_)
            | Filter::Hue(_)
            | Filter::Saturation(_)
            | Filter::Curves { .. }
            | Filter::Levels { .. }
            | Filter::Balance { .. }
            | Filter::Desaturate
            | Filter::Posterize(_)
    )
}

/// Apply a pointwise filter on scoped worker threads for large images. The
/// caller still waits for the result (the editor is not thread-shared), but the
/// pixel work no longer runs on one thread.
pub fn apply_filter_parallel(src: &[Rgba], w: u32, h: u32, f: Filter, fg: Rgba) -> Vec<Rgba> {
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8);
    if !pointwise(f) || workers <= 1 || h < 64 || w == 0 {
        return apply_filter(src, w, h, f, fg);
    }
    let rows = (h as usize).div_ceil(workers);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        let mut y = 0usize;
        while y < h as usize {
            let end = (y + rows).min(h as usize);
            let band = &src[y * w as usize..end * w as usize];
            let band_h = (end - y) as u32;
            handles.push(scope.spawn(move || (y, end, apply_filter(band, w, band_h, f, fg))));
            y = end;
        }
        let mut out = vec![Rgba::TRANSPARENT; src.len()];
        for handle in handles {
            let (start, end, band) = handle.join().expect("filter worker");
            out[start * w as usize..end * w as usize].copy_from_slice(&band);
        }
        out
    })
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
        Filter::Curves {
            shadows,
            midtones,
            highlights,
        } => {
            let lut = curve_lut(shadows, midtones, highlights);
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    out[i] = Rgba(lut[p.0 as usize], lut[p.1 as usize], lut[p.2 as usize], p.3);
                }
            }
        }
        Filter::Levels {
            in_black,
            in_white,
            gamma,
            out_black,
            out_white,
        } => {
            let span = (i32::from(in_white) - i32::from(in_black)).max(1) as f32;
            let gamma = gamma.clamp(0.1, 10.0);
            let level = |c: u8| {
                let v = ((f32::from(c) - f32::from(in_black)) / span).clamp(0.0, 1.0);
                let g = v.powf(1.0 / gamma);
                (f32::from(out_black) + g * f32::from(out_white) - g * f32::from(out_black))
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    out[i] = Rgba(level(p.0), level(p.1), level(p.2), p.3);
                }
            }
        }
        Filter::Balance { r, g, b } => {
            let shift =
                |c: u8, d: i16| (i32::from(c) + i32::from(d) * 255 / 100).clamp(0, 255) as u8;
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    out[i] = Rgba(shift(p.0, r), shift(p.1, g), shift(p.2, b), p.3);
                }
            }
        }
        Filter::Desaturate => {
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    let y = ((299 * u32::from(p.0) + 587 * u32::from(p.1) + 114 * u32::from(p.2))
                        / 1000) as u8;
                    out[i] = Rgba(y, y, y, p.3);
                }
            }
        }
        Filter::Median(radius) => {
            let r = radius.clamp(1, 8) as i32;
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let p = src[idx(x as u32, y as u32)];
                    if p.3 == 0 {
                        continue;
                    }
                    let (mut rs, mut gs, mut bs) = (Vec::new(), Vec::new(), Vec::new());
                    for dy in -r..=r {
                        for dx in -r..=r {
                            let (nx, ny) = (x + dx, y + dy);
                            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                                continue;
                            }
                            let q = src[(ny as u32 * w + nx as u32) as usize];
                            if q.3 > 0 {
                                rs.push(q.0);
                                gs.push(q.1);
                                bs.push(q.2);
                            }
                        }
                    }
                    if rs.is_empty() {
                        continue;
                    }
                    let pick = |v: &mut Vec<u8>| {
                        v.sort_unstable();
                        v[v.len() / 2]
                    };
                    out[(y as u32 * w + x as u32) as usize] =
                        Rgba(pick(&mut rs), pick(&mut gs), pick(&mut bs), p.3);
                }
            }
        }
        Filter::Posterize(levels) => {
            let levels = f32::from(levels.clamp(2, 32) as u8 - 1);
            let q = |c: u8| {
                ((f32::from(c) / 255.0 * levels).round() / levels * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            for (i, p) in src.iter().enumerate() {
                if p.3 > 0 {
                    out[i] = Rgba(q(p.0), q(p.1), q(p.2), p.3);
                }
            }
        }
        Filter::Convolve {
            kernel,
            divisor,
            bias,
        } => {
            let divisor = if divisor == 0.0 { 1.0 } else { divisor };
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let p = src[idx(x as u32, y as u32)];
                    if p.3 == 0 {
                        continue;
                    }
                    let (mut sr, mut sg, mut sb) = (0f32, 0f32, 0f32);
                    for (k, slot) in kernel.iter().enumerate() {
                        let (dx, dy) = (k as i32 % 3 - 1, k as i32 / 3 - 1);
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let q = src[(ny as u32 * w + nx as u32) as usize];
                        let k = f32::from(*slot);
                        sr += f32::from(q.0) * k;
                        sg += f32::from(q.1) * k;
                        sb += f32::from(q.2) * k;
                    }
                    let ch = |v: f32| (v / divisor + bias as f32).round().clamp(0.0, 255.0) as u8;
                    out[(y as u32 * w + x as u32) as usize] = Rgba(ch(sr), ch(sg), ch(sb), p.3);
                }
            }
        }
        Filter::Emboss => {
            const K: [i16; 9] = [-2, -1, 0, -1, 0, 1, 0, 1, 2];
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let p = src[idx(x as u32, y as u32)];
                    if p.3 == 0 {
                        continue;
                    }
                    let mut sum = 0i32;
                    for (k, weight) in K.iter().enumerate() {
                        let (dx, dy) = (k as i32 % 3 - 1, k as i32 / 3 - 1);
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let q = src[(ny as u32 * w + nx as u32) as usize];
                        sum += (luma(q) as i32 / 1000) * i32::from(*weight);
                    }
                    let v = (128 + sum).clamp(0, 255) as u8;
                    out[(y as u32 * w + x as u32) as usize] = Rgba(v, v, v, p.3);
                }
            }
        }
    }
    let _ = idx;
    out
}

/// Piecewise-linear 0..255 LUT through (0, shadows), (128, midtones),
/// (255, highlights).
fn curve_lut(shadows: u8, midtones: u8, highlights: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (x, slot) in lut.iter_mut().enumerate() {
        let v = if x <= 128 {
            let t = x as f32 / 128.0;
            f32::from(shadows) + (f32::from(midtones) - f32::from(shadows)) * t
        } else {
            let t = (x - 128) as f32 / 127.0;
            f32::from(midtones) + (f32::from(highlights) - f32::from(midtones)) * t
        };
        *slot = v.round().clamp(0.0, 255.0) as u8;
    }
    lut
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
    fn cel_move_translates_keeps_alpha_clips_and_undoes_once() {
        let mut e = Editor::new(8, 8);
        let (l, f) = (e.active_layer, e.active_frame);
        let solid = Rgba::rgb(10, 20, 30);
        let half = Rgba(40, 50, 60, 128);
        e.paint_pixel(1, 1, solid);
        e.paint_pixel(2, 1, half);
        e.undo.clear();

        assert!(e.start_cel_move(1, 1));
        assert_eq!(e.sprite.get(l, f, 1, 1), Rgba::TRANSPARENT, "source floats");
        e.move_cel_to(6, 6);
        e.commit_cel_move();
        assert_eq!(e.sprite.get(l, f, 6, 6), solid);
        assert_eq!(e.sprite.get(l, f, 7, 6), half, "alpha is preserved");
        assert_eq!(e.sprite.get(l, f, 1, 1), Rgba::TRANSPARENT);

        e.undo();
        assert_eq!(
            e.sprite.get(l, f, 1, 1),
            solid,
            "one undo restores the move"
        );
        assert_eq!(e.sprite.get(l, f, 6, 6), Rgba::TRANSPARENT);

        // Dragging past the right edge drops what no longer fits.
        assert!(e.start_cel_move(1, 1));
        e.move_cel_to(7, 1);
        e.commit_cel_move();
        assert_eq!(e.sprite.get(l, f, 7, 1), solid);
        assert_eq!(
            e.sprite.get(l, f, 7, 6),
            Rgba::TRANSPARENT,
            "the clipped pixel is gone, not wrapped"
        );
    }

    #[test]
    fn cel_move_refuses_locked_layers_and_cancels_cleanly() {
        let mut e = Editor::new(4, 4);
        let l = e.active_layer;
        e.paint_pixel(0, 0, Rgba::rgb(9, 9, 9));
        e.sprite.layers[l].lock_movement = true;
        assert!(!e.start_cel_move(0, 0));
        assert!(e.cel_move.is_none());
        e.sprite.layers[l].lock_movement = false;
        e.sprite.layers[l].editable = false;
        assert!(!e.start_cel_move(0, 0));
        e.sprite.layers[l].editable = true;

        e.undo.clear();
        assert!(e.start_cel_move(0, 0));
        e.move_cel_to(2, 2);
        e.cancel_cel_move();
        assert_eq!(e.sprite.get(l, e.active_frame, 0, 0), Rgba::rgb(9, 9, 9));
        assert!(e.undo.is_empty(), "a cancelled drag records no history");
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
    fn rotsprite_and_custom_pivots_change_the_transform() {
        let mut e = Editor::new(16, 16);
        let (l, f) = (e.active_layer, e.active_frame);
        e.selection = Some(Selection::rect(2, 2, 12, 12));
        for y in 2..14 {
            for x in 2..14 {
                let c = if (x + y) % 2 == 0 {
                    Rgba::rgb(240, 240, 240)
                } else {
                    Rgba::rgb(20, 20, 20)
                };
                e.paint_pixel(x, y, c);
            }
        }
        let before: Vec<Rgba> = (0..16 * 16)
            .map(|i| e.sprite.get(l, f, i % 16, i / 16))
            .collect();
        e.free_transform(45.0, 1.0, 1.0, 0.0);
        let nearest: Vec<Rgba> = (0..16 * 16)
            .map(|i| e.sprite.get(l, f, i % 16, i / 16))
            .collect();
        e.undo();
        assert_eq!(
            (0..16 * 16)
                .map(|i| e.sprite.get(l, f, i % 16, i / 16))
                .collect::<Vec<_>>(),
            before
        );

        e.transform_rotsprite = true;
        e.free_transform(45.0, 1.0, 1.0, 0.0);
        let smooth: Vec<Rgba> = (0..16 * 16)
            .map(|i| e.sprite.get(l, f, i % 16, i / 16))
            .collect();
        assert!(smooth != nearest, "rotsprite differs from nearest");
        e.undo();

        // A 90 degree rotation about (2,2) sends pixel (1,0) to (3,1).
        e.selection = Some(Selection::rect(0, 0, 8, 8));
        e.sprite.set(l, f, 1, 0, Rgba::rgb(200, 0, 0));
        e.transform_pivot = Some((2.0, 2.0));
        e.free_transform(90.0, 1.0, 1.0, 0.0);
        assert_eq!(e.sprite.get(l, f, 1, 0), Rgba::TRANSPARENT);
        assert_eq!(e.sprite.get(l, f, 3, 1), Rgba::rgb(200, 0, 0));
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
    fn palette_edits_keep_names_and_sprite_in_sync() {
        use crate::sprite::ColorMode;
        let mut e = Editor::new(1, 1);
        e.set_color_mode(ColorMode::Indexed);
        e.load_palette(vec![
            Rgba::rgb(10, 20, 30),
            Rgba::rgb(200, 30, 40),
            Rgba::rgb(50, 60, 70),
        ]);
        // Indexed load inserts transparent at 0.
        assert_eq!(e.palette.len(), 4);
        e.set_palette_name(1, "skin");
        e.set_palette_name(2, "blood");
        assert_eq!(e.palette_name(1), "skin");
        assert_eq!(e.sprite.palette_names[1], "skin");

        e.insert_palette_at(1, Rgba::rgb(1, 2, 3));
        assert_eq!(e.palette_name(2), "skin", "name follows the colour");
        assert_eq!(e.sprite.palette_names[2], "skin");
        e.remove_palette_at(1);
        assert_eq!(e.palette_name(1), "skin");

        e.sort_palette("rgb");
        let skin = e
            .palette
            .iter()
            .position(|c| (c.0, c.1, c.2) == (10, 20, 30))
            .unwrap();
        assert_eq!(e.palette_name(skin), "skin");
        assert_eq!(e.sprite.palette_names[skin], "skin");
        assert_eq!(e.sprite.palette, e.palette);
        assert_eq!(e.sprite.palette[e.sprite.transparent_index as usize].3, 0);
    }

    #[test]
    fn palette_presets_and_recent_colours() {
        let mut e = Editor::new(1, 1);
        assert!(e.load_palette_preset("pico8"));
        assert_eq!(e.palette.len(), 16);
        assert_eq!(e.palette[e.fg_idx], e.fg);
        assert!(!e.load_palette_preset("no-such-preset"));

        for i in 0..20u8 {
            e.set_fg(Rgba::rgb(i, 0, 0));
        }
        assert_eq!(e.recent_colors.len(), 12);
        assert_eq!(e.recent_colors[0], Rgba::rgb(19, 0, 0));
        e.set_fg(Rgba::rgb(5, 0, 0));
        assert_eq!(e.recent_colors[0], Rgba::rgb(5, 0, 0));
        assert_eq!(
            e.recent_colors
                .iter()
                .filter(|c| **c == Rgba::rgb(5, 0, 0))
                .count(),
            1,
            "recent colours are deduplicated"
        );
        e.set_fg(Rgba::TRANSPARENT);
        assert_ne!(e.recent_colors[0], Rgba::TRANSPARENT);
    }

    #[test]
    fn dither_converts_to_indexed_and_undoes() {
        use crate::sprite::{ColorMode, DitherMode};
        let mut e = Editor::new(4, 1);
        for (x, v) in [0u8, 85, 170, 255].into_iter().enumerate() {
            e.sprite.set(0, 0, x as u32, 0, Rgba(v, v, v, 255));
        }
        e.sprite.palette = vec![
            Rgba::TRANSPARENT,
            Rgba(0, 0, 0, 255),
            Rgba(255, 255, 255, 255),
        ];
        e.sprite.transparent_index = 0;
        e.set_dither(DitherMode::Bayer2);
        assert!(e.sprite.is_indexed());
        assert_eq!(e.palette, e.sprite.palette);
        assert_eq!(e.msg, "dither: bayer2");

        e.undo();
        assert_eq!(e.sprite.color_mode, ColorMode::Auto);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba(0, 0, 0, 255));
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba(85, 85, 85, 255));
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
    fn curves_levels_balance_desaturate_and_posterize() {
        let fg = Rgba::TRANSPARENT;
        let one = [Rgba(128, 64, 32, 255)];
        let out = apply_filter(
            &one,
            1,
            1,
            Filter::Curves {
                shadows: 0,
                midtones: 200,
                highlights: 255,
            },
            fg,
        );
        assert_eq!(out[0], Rgba(200, 100, 50, 255));

        let out = apply_filter(
            &one,
            1,
            1,
            Filter::Levels {
                in_black: 0,
                in_white: 255,
                gamma: 2.0,
                out_black: 0,
                out_white: 255,
            },
            fg,
        );
        assert!((out[0].0 as i32 - 181).abs() <= 1, "{:?}", out[0]);

        let out = apply_filter(
            &one,
            1,
            1,
            Filter::Balance {
                r: 100,
                g: -100,
                b: 0,
            },
            fg,
        );
        assert_eq!(out[0], Rgba(255, 0, 32, 255));

        let out = apply_filter(&one, 1, 1, Filter::Desaturate, fg);
        assert_eq!(out[0], Rgba(79, 79, 79, 255));

        let dark = [Rgba(100, 64, 32, 255)];
        let out = apply_filter(&dark, 1, 1, Filter::Posterize(2), fg);
        assert_eq!(out[0], Rgba(0, 0, 0, 255));
    }

    #[test]
    fn median_convolve_and_emboss() {
        let fg = Rgba::TRANSPARENT;
        let mut cells = [Rgba(0, 0, 0, 255); 9];
        cells[4] = Rgba(255, 255, 255, 255);
        let out = apply_filter(&cells, 3, 3, Filter::Median(1), fg);
        assert_eq!(out[4], Rgba(0, 0, 0, 255), "median rejects the lone white");

        let one = [Rgba(12, 34, 56, 255)];
        let identity = Filter::Convolve {
            kernel: [0, 0, 0, 0, 1, 0, 0, 0, 0],
            divisor: 1.0,
            bias: 0,
        };
        assert_eq!(apply_filter(&one, 1, 1, identity, fg)[0], one[0]);

        let flat = [Rgba(100, 100, 100, 255); 9];
        let out = apply_filter(&flat, 3, 3, Filter::Emboss, fg);
        assert_eq!(out[4], Rgba(128, 128, 128, 255), "flat surface is neutral");
    }

    #[test]
    fn new_filters_are_selection_aware_and_one_undo_step() {
        let mut e = Editor::new(3, 1);
        let (l, f) = (e.active_layer, e.active_frame);
        e.paint_pixel(0, 0, Rgba::rgb(200, 0, 0));
        e.paint_pixel(1, 0, Rgba::rgb(0, 200, 0));
        e.paint_pixel(2, 0, Rgba::rgb(0, 0, 200));
        e.selection = Some(Selection::rect(1, 0, 1, 1));
        e.undo.clear();
        e.filter(Filter::Desaturate);
        assert_eq!(e.sprite.get(l, f, 0, 0), Rgba::rgb(200, 0, 0));
        assert_eq!(e.sprite.get(l, f, 1, 0), Rgba::rgb(117, 117, 117));
        assert_eq!(e.sprite.get(l, f, 2, 0), Rgba::rgb(0, 0, 200));
        assert_eq!(e.undo.len(), 1, "filter is one undo step");
        e.undo();
        assert_eq!(e.sprite.get(l, f, 1, 0), Rgba::rgb(0, 200, 0));
    }

    #[test]
    fn filter_parser_covers_new_parameters() {
        assert_eq!(
            Filter::parse_args(&["curves", "10", "200", "240"]),
            Some(Filter::Curves {
                shadows: 10,
                midtones: 200,
                highlights: 240
            })
        );
        assert_eq!(
            Filter::parse_args(&["levels", "10", "240", "1.5"]),
            Some(Filter::Levels {
                in_black: 10,
                in_white: 240,
                gamma: 1.5,
                out_black: 0,
                out_white: 255
            })
        );
        assert_eq!(
            Filter::parse_args(&["balance", "20", "-30", "40"]),
            Some(Filter::Balance {
                r: 20,
                g: -30,
                b: 40
            })
        );
        assert_eq!(
            Filter::parse_args(&["convolve", "0", "1", "0", "1", "4", "1", "0", "1", "0"]),
            Some(Filter::Convolve {
                kernel: [0, 1, 0, 1, 4, 1, 0, 1, 0],
                divisor: 1.0,
                bias: 0
            })
        );
        assert_eq!(Filter::parse("median", Some("2")), Some(Filter::Median(2)));
        assert_eq!(Filter::parse_args(&["emboss"]), Some(Filter::Emboss));
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
    fn selection_from_colour_alpha_and_fill_stroke() {
        let mut e = Editor::new(4, 3);
        let (l, f) = (e.active_layer, e.active_frame);
        e.sprite.set(l, f, 0, 0, Rgba::rgb(200, 0, 0));
        e.sprite.set(l, f, 1, 0, Rgba::rgb(0, 0, 200));
        e.selection_from_alpha(false);
        let sel = e.selection.clone().unwrap();
        assert_eq!(sel.mask.iter().filter(|m| **m).count(), 2);

        e.selection_from_color(Rgba::rgb(200, 0, 0), 0, false);
        let sel = e.selection.clone().unwrap();
        assert_eq!(sel.mask.iter().filter(|m| **m).count(), 1);
        assert!(sel.contains(0, 0));

        e.fill_selection(Rgba::rgb(0, 200, 0));
        assert_eq!(e.sprite.get(l, f, 0, 0), Rgba::rgb(0, 200, 0));
        assert_eq!(e.sprite.get(l, f, 1, 0), Rgba::rgb(0, 0, 200));
        e.undo();
        assert_eq!(e.sprite.get(l, f, 0, 0), Rgba::rgb(200, 0, 0));

        let mut e = Editor::new(6, 6);
        e.selection = Some(Selection::rect(1, 1, 4, 4));
        e.stroke_selection(1, true, Rgba::rgb(255, 0, 0));
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::rgb(255, 0, 0));
        assert_eq!(
            e.sprite.get(0, 0, 2, 2),
            Rgba::TRANSPARENT,
            "interior clear"
        );
        assert_eq!(e.sprite.get(0, 0, 0, 1), Rgba::TRANSPARENT);

        let mut e = Editor::new(6, 6);
        e.selection = Some(Selection::rect(1, 1, 2, 2));
        e.stroke_selection(1, false, Rgba::rgb(255, 0, 0));
        assert_eq!(
            e.sprite.get(0, 0, 0, 1),
            Rgba::rgb(255, 0, 0),
            "outside ring"
        );
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::TRANSPARENT);
    }

    #[test]
    fn multi_cel_paint_transform_and_export_fan_out() {
        let mut e = Editor::new(4, 4);
        e.sprite.add_frame_after(0);
        e.sprite.set(0, 0, 0, 0, Rgba::rgb(200, 0, 0));
        e.sprite.set(0, 1, 1, 0, Rgba::rgb(0, 200, 0));
        e.cel_selection.insert((0, 0));
        e.cel_selection.insert((0, 1));
        e.active_frame = 0;

        let blue = Rgba::rgb(0, 0, 200);
        e.begin_stroke();
        e.paint_pixel(2, 2, blue);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 2, 2), blue);
        assert_eq!(e.sprite.get(0, 1, 2, 2), blue, "paint fans out");
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 2, 2), Rgba::TRANSPARENT);
        assert_eq!(e.sprite.get(0, 1, 2, 2), Rgba::TRANSPARENT);

        e.free_transform(180.0, 1.0, 1.0, 0.0);
        assert_eq!(e.sprite.get(0, 0, 3, 3), Rgba::rgb(200, 0, 0));
        assert_eq!(e.sprite.get(0, 1, 2, 3), Rgba::rgb(0, 200, 0));
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(200, 0, 0));
        assert_eq!(e.sprite.get(0, 1, 1, 0), Rgba::rgb(0, 200, 0));

        let dir = std::env::temp_dir().join(format!("vixel-cels-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(e.export_cels(&dir).unwrap(), 2);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cel_rect_and_tag_editor_mutations() {
        let mut e = Editor::new(2, 2);
        e.sprite.add_frame_after(0);
        e.sprite.add_frame_after(1);
        e.select_cel_rect((0, 0), (0, 2));
        assert_eq!(e.selected_cels(), vec![(0, 0), (0, 1), (0, 2)]);

        e.set_tag("walk", Some(0), Some(1));
        let i = e.active_tag.unwrap();
        e.update_tag(i, "run", 0, 2);
        assert_eq!(e.sprite.tags[i].name, "run");
        assert_eq!((e.sprite.tags[i].from, e.sprite.tags[i].to), (0, 2));
        e.set_tag_color(Rgba::rgb(10, 20, 30));
        assert_eq!(e.sprite.tags[i].color, Rgba::rgb(10, 20, 30));
        e.del_tag(Some("run"));
        assert!(e.sprite.tags.is_empty());
    }

    #[test]
    fn cel_copy_cut_paste_and_empty_cel_vs_frame() {
        let mut e = Editor::new(2, 2);
        let red = Rgba::rgb(200, 0, 0);
        e.sprite.set(0, 0, 1, 1, red);
        e.copy_cel();
        e.sprite.add_frame_after(0);
        e.active_frame = 1;
        e.paste_cel();
        assert_eq!(e.sprite.get(0, 1, 1, 1), red);
        e.undo();
        assert_eq!(e.sprite.get(0, 1, 1, 1), Rgba::TRANSPARENT);

        e.active_frame = 0;
        e.cut_cel();
        assert!(e.cel_is_empty(0, 0));
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::TRANSPARENT);
        e.paste_cel();
        assert_eq!(e.sprite.get(0, 0, 1, 1), red);

        let frames = e.sprite.frames.len();
        e.delete_cel();
        assert_eq!(e.sprite.frames.len(), frames, "empty cel keeps the frame");
        assert!(e.cel_is_empty(0, 0));
        e.del_frame();
        assert_eq!(e.sprite.frames.len(), frames - 1, "del_frame removes it");
    }

    #[test]
    fn cel_selection_ranges_include_the_active_cel() {
        let mut e = Editor::new(2, 2);
        e.sprite.add_frame_after(0);
        e.sprite.add_frame_after(1);
        e.active_frame = 1;
        e.select_frame_range(0, 0, 2);
        assert_eq!(e.selected_cels(), vec![(0, 0), (0, 1), (0, 2)]);
        e.clear_cel_selection();
        assert_eq!(
            e.selected_cels(),
            vec![(0, 1)],
            "active always participates"
        );
        e.toggle_cel_selection(0, 0);
        assert!(e.cel_selected(0, 0));
        e.toggle_cel_selection(0, 0);
        assert!(!e.cel_selected(0, 0));
    }

    #[test]
    fn selection_mask_transforms_and_offsets_without_pixels() {
        let mut e = Editor::new(8, 8);
        let (l, f) = (e.active_layer, e.active_frame);
        e.sprite.set(l, f, 1, 1, Rgba::rgb(9, 9, 9));
        e.selection = Some(Selection::rect(1, 1, 2, 2));

        e.offset_selection(2, 3);
        let sel = e.selection.clone().unwrap();
        assert!(sel.contains(3, 4));
        assert!(!sel.contains(1, 1));
        assert_eq!(
            e.sprite.get(l, f, 1, 1),
            Rgba::rgb(9, 9, 9),
            "offset leaves pixels"
        );

        let before = e
            .selection
            .as_ref()
            .unwrap()
            .mask
            .iter()
            .filter(|m| **m)
            .count();
        e.transform_selection_mask(0.0, 2.0, 2.0, 0.0);
        let after = e
            .selection
            .as_ref()
            .unwrap()
            .mask
            .iter()
            .filter(|m| **m)
            .count();
        assert!(after > before, "scale grows the mask: {before} -> {after}");
        assert_eq!(
            e.sprite.get(l, f, 1, 1),
            Rgba::rgb(9, 9, 9),
            "mask transform leaves pixels"
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
                user_data: None,
                ..Default::default()
            },
            Layer {
                name: "base".into(),
                editable: false,
                user_data: None,
                ..Default::default()
            },
            Layer {
                name: "ref".into(),
                editable: false,
                user_data: None,
                ..Default::default()
            },
        ];
        sprite.cels = (0..3).map(|_| vec![Cel::blank(4, 4)]).collect();
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
    fn multi_stop_gradients_interpolate_and_presets_carry_alpha() {
        let mut e = Editor::new(4, 4);
        e.gradient_stops = vec![
            (0.0, Rgba::rgb(255, 0, 0)),
            (0.5, Rgba::rgb(0, 255, 0)),
            (1.0, Rgba::rgb(0, 0, 255)),
        ];
        assert_eq!(e.gradient_color(0.0), Rgba::rgb(255, 0, 0));
        assert_eq!(e.gradient_color(0.5), Rgba::rgb(0, 255, 0));
        assert_eq!(e.gradient_color(1.0), Rgba::rgb(0, 0, 255));
        let quarter = e.gradient_color(0.25);
        assert!((quarter.0 as i32 - 128).abs() <= 1, "{quarter:?}");
        assert!((quarter.1 as i32 - 128).abs() <= 1, "{quarter:?}");

        assert!(e.gradient_preset("fg-transparent"));
        assert_eq!(e.gradient_color(0.0).3, 255);
        assert_eq!(e.gradient_color(1.0).3, 0);
        assert!((e.gradient_color(0.5).3 as i32 - 128).abs() <= 1);
        assert!(!e.gradient_preset("nope"));

        e.gradient_preset("black-white");
        e.gradient_set_stop(0.5, Rgba::rgb(255, 0, 0));
        assert_eq!(e.gradient_stops.len(), 3);
        assert_eq!(e.gradient_stops[1].1, Rgba::rgb(255, 0, 0));
    }

    #[test]
    fn gradient_dither_quantizes_to_bayer_levels() {
        let mut e = Editor::new(16, 1);
        e.gradient_preset("black-white");
        e.gradient_dither = DitherMode::Bayer4;
        e.cursor = (15, 0);
        e.paint_gradient((0, 0));
        let unique: std::collections::BTreeSet<u8> =
            (0..16).map(|x| e.sprite.get(0, 0, x, 0).0).collect();
        assert!(unique.len() > 1 && unique.len() <= 8, "{unique:?}");

        let mut e = Editor::new(16, 1);
        e.gradient_preset("black-white");
        e.cursor = (15, 0);
        e.paint_gradient((0, 0));
        let smooth: std::collections::BTreeSet<u8> =
            (0..16).map(|x| e.sprite.get(0, 0, x, 0).0).collect();
        assert!(
            smooth.len() > unique.len(),
            "dither reduces the level count"
        );
    }

    #[test]
    fn gradient_honours_selection_and_alpha() {
        let mut e = Editor::new(4, 1);
        e.selection = Some(Selection::rect(0, 0, 2, 1));
        e.gradient_preset("fg-transparent");
        e.cursor = (3, 0);
        e.paint_gradient((0, 0));
        assert_eq!(
            e.sprite.get(0, 0, 3, 0),
            Rgba::TRANSPARENT,
            "outside selection"
        );
        let left = e.sprite.get(0, 0, 0, 0).3;
        let right = e.sprite.get(0, 0, 1, 0).3;
        assert!(
            left > right && right > 0,
            "alpha ramps inside: {left}, {right}"
        );
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
        assert!(shade_color(Rgba(100, 100, 100, 255), true, 20).0 < 100);
        assert!(shade_color(Rgba(100, 100, 100, 255), false, 20).0 > 100);
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
    fn save_path_dispatches_project_ase_and_image() {
        let mut e = Editor::new(2, 2);
        e.paint_pixel(0, 0, Rgba::rgb(1, 2, 3));
        let dir = std::env::temp_dir().join(format!("vixel-savepath-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let vpx = dir.join("x.vpx");
        e.save_path(&vpx).unwrap();
        assert_eq!(
            crate::io::load_project(&vpx).unwrap().get(0, 0, 0, 0),
            Rgba::rgb(1, 2, 3)
        );
        let png = dir.join("x.png");
        e.save_path(&png).unwrap();
        assert!(png.exists());
        let ase = dir.join("x.ase");
        e.save_path(&ase).unwrap();
        assert_eq!(
            crate::ase::load(&ase).unwrap().get(0, 0, 0, 0),
            Rgba::rgb(1, 2, 3)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_labels_and_jump_navigation() {
        let mut e = Editor::new(2, 2);
        e.begin_stroke();
        e.paint_pixel(0, 0, Rgba::rgb(255, 0, 0));
        e.end_stroke();
        e.begin_stroke();
        e.paint_pixel(1, 1, Rgba::rgb(0, 0, 255));
        e.end_stroke();
        assert_eq!(e.undo.len(), 2);
        assert!(e.history_labels()[0].starts_with("stroke"));

        e.goto_history(1);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::TRANSPARENT);
        e.goto_history(0);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::TRANSPARENT);
        assert_eq!(e.redo_labels().len(), 2);
        e.goto_history(2);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::rgb(0, 0, 255));

        e.add_frame();
        assert!(
            e.history_labels().last().unwrap().contains("structure"),
            "{:?}",
            e.history_labels()
        );
        e.set_cel_z(3);
        assert!(e.history_labels().last().unwrap().contains("cel z"));
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
    fn ink_modes_blend_replace_erase_and_lock_alpha() {
        let white = Rgba(255, 255, 255, 255);
        let red = Rgba(255, 0, 0, 255);
        assert_eq!(
            apply_ink(Ink::Replace, white, Rgba(1, 2, 3, 4)),
            Rgba(1, 2, 3, 4)
        );

        let mixed = apply_ink(Ink::Simple, white, Rgba(255, 0, 0, 128));
        assert!(mixed.0 == 255 && mixed.1 > 0 && mixed.1 < 255, "{mixed:?}");
        assert_eq!(mixed.3, 255, "opaque destination stays opaque");

        assert_eq!(
            apply_ink(Ink::LockAlpha, Rgba::TRANSPARENT, red),
            Rgba::TRANSPARENT
        );
        let locked = apply_ink(Ink::LockAlpha, Rgba(0, 0, 0, 128), red);
        assert_eq!(locked.3, 128, "alpha is preserved");

        assert_eq!(
            apply_ink(Ink::Erase, white, Rgba::TRANSPARENT),
            Rgba::TRANSPARENT
        );
        assert_eq!(apply_ink(Ink::Erase, white, Rgba(0, 0, 0, 128)).3, 127);

        let shaded = apply_ink(Ink::Shading, Rgba(100, 100, 100, 255), white);
        assert!(shaded.0 > 100, "lighter brush lightens");
    }

    #[test]
    fn inks_are_per_tool_and_alpha_lock_blocks_empty_pixels() {
        let mut e = Editor::new(2, 1);
        e.fg = Rgba(255, 0, 0, 255);
        e.paint_pixel(0, 0, Rgba(255, 255, 255, 255));
        e.set_ink(Ink::Replace);
        assert_eq!(e.ink(), Ink::Replace);
        e.set_tool(Tool::Eraser);
        assert_eq!(e.ink(), Ink::Erase, "eraser remembers its own ink");
        e.paint_with_tool(0, 0);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::TRANSPARENT);
        e.set_tool(Tool::Pencil);
        assert_eq!(e.ink(), Ink::Replace, "pencil keeps its ink");

        e.set_alpha_lock(true);
        e.paint_pixel(1, 0, e.fg);
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba::TRANSPARENT);
        e.set_alpha_lock(false);
        e.paint_pixel(1, 0, e.fg);
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba(255, 0, 0, 255));
    }

    #[test]
    fn text_tool_live_preview_is_one_undo_step() {
        let mut e = Editor::new(32, 16);
        e.set_tool(Tool::Text);
        e.text = "A".into();
        e.cursor = (1, 1);
        e.preview_text_object();
        e.text = "AB".into();
        e.preview_text_object();
        assert!(e.undo.is_empty(), "previews do not push undo yet");
        assert!(e.sprite.cels[0][0].pixels.contains(&e.fg), "glyph painted");
        e.commit_text_edit();
        assert_eq!(e.undo.len(), 1, "commit is exactly one step");
        e.undo();
        assert!(
            e.sprite.cels[0][0]
                .pixels
                .iter()
                .all(|p| p.is_transparent()),
            "undo clears the preview"
        );
    }

    #[test]
    fn text_tool_places_at_cursor_with_size_and_alignment() {
        let mut e = Editor::new(40, 24);
        e.set_tool(Tool::Text);
        e.text = "X".into();
        e.text_scale = 2;
        e.text_align = 2;
        e.place_text_at(5, 3);
        let object = e.sprite.cels[0][0]
            .user_data
            .as_ref()
            .unwrap()
            .text_object()
            .unwrap();
        assert_eq!((object.x, object.y), (5, 3));
        assert_eq!(object.size, 16);
        assert_eq!(object.align, 2);
        assert_eq!(object.font, "vixel");
        assert_eq!(e.undo.len(), 1);
        assert!(!e.sprite.get(0, 0, 5, 3).is_transparent());
    }

    #[test]
    fn text_objects_rasterize_and_update() {
        let mut e = Editor::new(32, 16);
        e.set_text_object("A");
        let object = e.sprite.cels[0][0]
            .user_data
            .as_ref()
            .unwrap()
            .text_object()
            .unwrap();
        assert_eq!(object.text, "A");
        assert!(
            e.sprite.cels[0][0].pixels.contains(&e.fg),
            "glyph pixels are painted"
        );
        e.set_text_object("AB");
        let updated = e.sprite.cels[0][0]
            .user_data
            .as_ref()
            .unwrap()
            .text_object()
            .unwrap();
        assert_eq!(updated.text, "AB");
        e.undo();
        let restored = e.sprite.cels[0][0]
            .user_data
            .as_ref()
            .unwrap()
            .text_object()
            .unwrap();
        assert_eq!(restored.text, "A");
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
    fn movement_lock_blocks_transform_and_cut() {
        let mut e = Editor::new(2, 1);
        e.paint_pixel(0, 0, Rgba(200, 0, 0, 255));
        e.sprite.layers[0].lock_movement = true;
        assert!(!e.sprite.layer_movable(0));
        e.free_transform(90.0, 1.0, 1.0, 0.0);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba(200, 0, 0, 255));
        assert!(e.msg.contains("locked"));
        e.set_layer_lock_movement(false);
        assert!(e.sprite.layer_movable(0));
    }

    #[test]
    fn layer_uuid_minting_and_new_layers() {
        let mut e = Editor::new(1, 1);
        assert_eq!(e.layer_uuid_hex(), "none");
        e.new_layer_uuid();
        let first = e.sprite.layers[0].uuid.expect("uuid");
        assert_eq!(first[6] >> 4, 4, "v4 shape");
        assert_eq!(first[8] & 0xc0, 0x80, "variant bits");
        assert_eq!(e.layer_uuid_hex().len(), 32);
        e.undo();
        assert_eq!(e.sprite.layers[0].uuid, None);

        e.new_layer_uuid();
        e.add_layer();
        let second = e.sprite.layers[1].uuid.expect("new layer uuid");
        assert_ne!(second, e.sprite.layers[0].uuid.unwrap());
        e.active_layer = 1;
        e.clear_layer_uuid();
        assert_eq!(e.sprite.layers[1].uuid, None);
    }

    #[test]
    fn link_and_unlink_are_undoable() {
        let red = Rgba(255, 0, 0, 255);
        let mut e = Editor::new(1, 1);
        e.paint_pixel(0, 0, red);
        e.add_frame();
        e.active_frame = 1;
        e.link_cel(0);
        assert!(e.sprite.is_linked(0, 1));
        assert_eq!(e.sprite.get(0, 1, 0, 0), red);

        e.undo();
        assert!(!e.sprite.is_linked(0, 1));
        e.redo();
        assert!(e.sprite.is_linked(0, 1));

        e.unlink_cel();
        assert!(!e.sprite.is_linked(0, 1));
        assert_eq!(e.sprite.get(0, 1, 0, 0), red);
    }

    #[test]
    fn cel_z_changes_are_undoable() {
        let mut e = Editor::new(1, 1);
        e.set_cel_z(4);
        assert_eq!(e.sprite.cels[0][0].z_index, 4);
        e.undo();
        assert_eq!(e.sprite.cels[0][0].z_index, 0);
        e.redo();
        assert_eq!(e.sprite.cels[0][0].z_index, 4);
    }

    #[test]
    fn paint_and_undo_on_an_offset_cel() {
        use crate::sprite::Cel;
        let mut e = Editor::new(4, 4);
        e.sprite.cels[0][0] = Cel {
            x: 1,
            y: 1,
            w: 2,
            h: 2,
            z_index: 0,
            link: None,
            pixels: vec![Rgba::TRANSPARENT; 4],
            opacity: 255,
            user_data: None,
            tilemap: None,
        };
        e.paint_pixel(1, 1, Rgba(9, 8, 7, 255));
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba(9, 8, 7, 255));
        assert_eq!(e.sprite.cels[0][0].x, 1);
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::TRANSPARENT);
    }

    #[test]
    fn ase_profile_summary_and_vpx_round_trip() {
        let mut e = Editor::new(1, 1);
        assert!(e.profile_summary().contains("none"));
        let profile = AseProfile {
            kind: 2,
            flags: 0,
            gamma: 0.0,
            icc: vec![1, 2, 3, 4],
        };
        e.set_ase_profile(Some(profile.clone()));
        assert!(e.profile_summary().contains("ICC (4 bytes)"));

        let path =
            std::env::temp_dir().join(format!("vixel_ase_profile_{}.vpx", std::process::id()));
        crate::io::save_project(&e.sprite, &path).unwrap();
        let back = crate::io::load_project(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.ase_profile, Some(profile));

        e.set_ase_profile(Some(AseProfile {
            kind: 1,
            flags: 1,
            gamma: 1.0,
            icc: Vec::new(),
        }));
        assert!(e.profile_summary().contains("sRGB"));
        assert!(e.profile_summary().contains("gamma 1.0000"));
        e.set_ase_profile(None);
        assert!(e.profile_summary().contains("none"));
    }

    #[test]
    fn ping_pong_reverse_starts_at_the_end() {
        use crate::sprite::{PlayDirection, Tag};
        let mut e = Editor::new(1, 1);
        e.add_frame();
        e.add_frame(); // 3 frames
        e.sprite.tags.push(Tag {
            name: "back".into(),
            from: 0,
            to: 2,
            color: Rgba(1, 2, 3, 255),
            direction: PlayDirection::PingPongReverse,
            repeat: 0,
            user_data: None,
        });
        e.active_tag = Some(0);
        e.start_playback();
        assert_eq!(e.active_frame, 2, "reverse ping-pong starts at the end");
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 1);
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 0);
        assert!(e.advance_frame());
        assert_eq!(e.active_frame, 1, "bounces off the start");
    }

    fn tilemap_editor() -> Editor {
        use crate::sprite::{TileRef, TilemapCel, Tileset};
        let mut e = Editor::new(4, 4);
        e.sprite.layers[0].tilemap = true;
        e.sprite.layers[0].tileset = 5;
        e.sprite.tilesets.push(Tileset {
            id: 5,
            tile_w: 2,
            tile_h: 2,
            base_index: 1,
            num_tiles: 2,
            image: vec![
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 255, 0),
            ],
            ..Default::default()
        });
        e.sprite.cels[0][0].tilemap = Some(TilemapCel::new(
            2,
            vec![
                TileRef {
                    id: 1,
                    ..Default::default()
                },
                TileRef {
                    id: 1,
                    ..Default::default()
                },
            ],
        ));
        e.sprite.rasterize_tilemap(0, 0);
        e
    }

    #[test]
    fn tile_painting_picks_erases_and_undoes() {
        use crate::sprite::TileRef;
        let mut e = tilemap_editor();
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));

        e.tile_brush = 2;
        e.tile_flip_x = true;
        e.paint_tile_at(2, 0);
        assert_eq!(e.sprite.get(0, 0, 2, 0), Rgba::rgb(0, 255, 0));
        let tm = e.sprite.cels[0][0].tilemap.as_ref().unwrap();
        assert_eq!(tm.tiles[1].id, 2);
        assert!(tm.tiles[1].flip_x);
        assert_eq!(e.undo.len(), 1, "one tile edit = one undo step");
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 2, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(
            e.sprite.cels[0][0].tilemap.as_ref().unwrap().tiles[1],
            TileRef {
                id: 1,
                ..Default::default()
            }
        );

        e.tile_brush = 1;
        e.paint_tile_at(2, 0);
        e.cursor = (2, 0);
        e.tile_flip_x = false;
        e.pick_tile_at(2, 0);
        assert_eq!(e.tile_brush, 1);

        e.erase_tile_at(0, 0);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::TRANSPARENT);
    }

    #[test]
    fn tile_pixel_edits_update_the_atlas_and_tile_modes_apply() {
        let mut e = tilemap_editor();
        e.paint_tile_pixel_at(0, 0, Rgba::rgb(0, 0, 255));
        assert_eq!(e.sprite.tilesets[0].image[0], Rgba::rgb(0, 0, 255));
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(0, 0, 255));
        assert_eq!(e.undo.len(), 1);
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));

        e.sprite.cels[0][0].tilemap.as_mut().unwrap().tiles[1].id = 0;
        e.tile_brush = 2;
        e.tile_edit_mode = TileMode::Auto;
        e.paint_tile_at(0, 0);
        assert_eq!(
            e.sprite.cels[0][0].tilemap.as_ref().unwrap().tiles[1].id,
            2,
            "auto fills the empty neighbour"
        );

        e.tile_edit_mode = TileMode::Stack;
        e.tile_brush = 1;
        e.paint_tile_at(2, 0);
        assert_eq!(
            e.sprite.cels[0][0].tilemap.as_ref().unwrap().tiles[1].id,
            2,
            "stack advances variants"
        );
    }

    #[test]
    fn tile_pixel_mode_paints_pixels_without_changing_refs() {
        let mut e = tilemap_editor();
        let before = e.sprite.cels[0][0].tilemap.clone();
        e.set_tool(Tool::Tile);
        e.tile_pixel_mode = true;
        e.fg = Rgba::rgb(9, 8, 7);
        e.paint_with_tool(0, 0);
        assert_eq!(e.sprite.get(0, 0, 0, 0), Rgba::rgb(9, 8, 7));
        assert_eq!(e.sprite.cels[0][0].tilemap, before);
    }

    #[test]
    fn tile_offsets_follow_repeats_and_clamp() {
        let mut e = Editor::new(4, 4);
        assert_eq!(e.tile_offsets().len(), 9, "default 3x3");
        e.tile_x = 2;
        e.tile_y = 1;
        let offsets = e.tile_offsets();
        assert_eq!(offsets.len(), 15, "5x3");
        assert!(offsets.contains(&(2, 1)) && offsets.contains(&(-2, -1)));
        e.tile_x = 99;
        e.tile_y = 99;
        let offsets = e.tile_offsets();
        assert_eq!(offsets.len(), 17 * 17, "clamped to 8 per side");
        assert!(offsets.contains(&(8, 8)) && !offsets.contains(&(9, 9)));
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
    fn pressure_scales_stamps_and_manual_overrides_device() {
        let mut e = Editor::new(6, 1);
        e.fg = Rgba(255, 0, 0, 255);
        e.set_pressure(0.5);
        assert_eq!(e.pressure_source(), "manual");
        e.begin_stroke();
        e.paint_with_tool(0, 0);
        e.end_stroke();
        let a = e.sprite.get(0, 0, 0, 0).3;
        assert!((120..=136).contains(&a), "half pressure alpha: {a}");

        e.set_pressure(0.0);
        e.begin_stroke();
        e.paint_with_tool(1, 0);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 1, 0), Rgba::TRANSPARENT);

        e.pressure_auto();
        assert_eq!(e.pressure(), 1.0);
        e.begin_stroke();
        e.paint_with_tool(2, 0);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 2, 0).3, 255);

        e.set_device_pressure(Some(0.25));
        assert_eq!(e.pressure_source(), "device");
        e.begin_stroke();
        e.paint_with_tool(3, 0);
        e.end_stroke();
        let a = e.sprite.get(0, 0, 3, 0).3;
        assert!((60..=68).contains(&a), "device pressure alpha: {a}");

        e.set_pressure(1.0);
        assert_eq!(e.pressure_source(), "manual");
        e.begin_stroke();
        e.paint_with_tool(4, 0);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 4, 0).3, 255);
    }

    #[test]
    fn spray_rate_and_radius_shape_the_scatter() {
        let mut e = Editor::new(9, 9);
        e.fg = Rgba(255, 0, 0, 255);
        e.spray_rate = 1;
        e.spray_radius = 4;
        e.spray_at(4, 4);
        let painted = (0..9)
            .flat_map(|y| (0..9).map(move |x| (x, y)))
            .filter(|&(x, y)| !e.sprite.get(0, 0, x, y).is_transparent())
            .count();
        assert_eq!(painted, 1, "one dot per dab at rate 1");

        let mut e = Editor::new(9, 9);
        e.fg = Rgba(255, 0, 0, 255);
        e.spray_rate = 64;
        e.spray_radius = 1;
        e.spray_at(4, 4);
        for y in 0..9 {
            for x in 0..9 {
                if !e.sprite.get(0, 0, x, y).is_transparent() {
                    assert!((3..=5).contains(&x) && (3..=5).contains(&y), "({x},{y})");
                }
            }
        }
    }

    #[test]
    fn shade_strength_scales_and_the_brush_covers() {
        let mut e = Editor::new(5, 5);
        e.brush_size = 1;
        e.shade_strength = 50;
        e.sprite.set(0, 0, 2, 2, Rgba(100, 100, 100, 255));
        e.shade_at(2, 2, true);
        assert_eq!(e.sprite.get(0, 0, 2, 2).0, 50);

        let mut e = Editor::new(5, 5);
        e.brush_size = 3;
        for y in 1..=3 {
            for x in 1..=3 {
                e.sprite.set(0, 0, x, y, Rgba(100, 100, 100, 255));
            }
        }
        e.shade_at(2, 2, false);
        assert_eq!(e.sprite.get(0, 0, 1, 1).0, 120, "brush reaches the corner");
    }

    #[test]
    fn blur_tool_radius_shares_box_blur() {
        let mut e = Editor::new(5, 1);
        e.sprite.set(0, 0, 2, 0, Rgba(255, 255, 255, 255));
        e.blur_radius = 2;
        e.blur_at(2, 0);
        assert_eq!(e.sprite.get(0, 0, 2, 0).3, 51, "5 samples at radius 2");

        let mut e = Editor::new(5, 1);
        e.sprite.set(0, 0, 2, 0, Rgba(255, 255, 255, 255));
        e.blur_radius = 1;
        e.blur_at(2, 0);
        assert_eq!(e.sprite.get(0, 0, 2, 0).3, 85, "3 samples at radius 1");
    }

    #[test]
    fn contour_width_and_side_are_configurable() {
        let mut e = Editor::new(7, 7);
        e.sprite.set(0, 0, 3, 3, Rgba(1, 2, 3, 255));
        e.contour_width = 1;
        e.contour_at(3, 3);
        assert_eq!(e.sprite.get(0, 0, 3, 2), e.fg);
        assert_eq!(e.sprite.get(0, 0, 3, 1), Rgba::TRANSPARENT);

        let mut e = Editor::new(7, 7);
        e.sprite.set(0, 0, 3, 3, Rgba(1, 2, 3, 255));
        e.contour_width = 2;
        e.contour_at(3, 3);
        assert_eq!(e.sprite.get(0, 0, 3, 1), e.fg, "second ring is painted");

        let mut e = Editor::new(5, 5);
        for y in 0..5 {
            for x in 0..5 {
                e.sprite.set(0, 0, x, y, Rgba(1, 2, 3, 255));
            }
        }
        e.contour_width = 1;
        e.contour_inside = true;
        e.contour_at(2, 2);
        assert_eq!(
            e.sprite.get(0, 0, 0, 0),
            e.fg,
            "inside ring hugs the border"
        );
        assert_eq!(e.sprite.get(0, 0, 2, 2), Rgba(1, 2, 3, 255), "middle kept");
    }

    #[test]
    fn custom_brush_mask_limits_which_pixels_paint() {
        let mut e = Editor::new(5, 5);
        e.custom_brush = Some(Brush::new("dot", 3, 3, vec![0, 0, 0, 0, 255, 0, 0, 0, 0]));
        e.brush_kind = BrushKind::Custom;
        e.fg = Rgba(10, 20, 30, 255);
        e.begin_stroke();
        e.paint_with_tool(2, 2);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 2, 2), e.fg);
        for (x, y) in [(1, 1), (3, 3), (1, 2), (3, 2)] {
            assert_eq!(
                e.sprite.get(0, 0, x, y),
                Rgba::TRANSPARENT,
                "mask leaves ({x},{y}) alone"
            );
        }
    }

    #[test]
    fn circle_brush_shapes_and_spacing_use_the_mask() {
        let mut e = Editor::new(6, 6);
        e.brush_size = 3;
        e.brush_kind = BrushKind::Circle;
        e.fg = Rgba(9, 8, 7, 255);
        e.begin_stroke();
        e.paint_with_tool(2, 2);
        e.end_stroke();
        assert_eq!(e.sprite.get(0, 0, 2, 2), e.fg);
        assert_eq!(e.sprite.get(0, 0, 1, 2), e.fg, "edge is inside");
        assert_eq!(e.sprite.get(0, 0, 1, 1), Rgba::TRANSPARENT, "corner is out");

        // Shapes use the brush mask too: a 2 px brush thickens a line.
        e.brush_kind = BrushKind::Square;
        e.brush_size = 2;
        e.pending = Some(Pending {
            shape: Shape::Line,
            start: (0, 1),
        });
        e.cursor = (4, 1);
        e.commit_shape(e.fg);
        assert_eq!(e.sprite.get(0, 0, 0, 0), e.fg, "brush thickens upwards");
        assert_eq!(e.sprite.get(0, 0, 0, 1), e.fg);
        assert_eq!(e.sprite.get(0, 0, 4, 1), e.fg);

        e.brush_size = 4;
        e.brush_spacing = 100;
        assert_eq!(e.stamp_spacing(), 4);
        e.brush_spacing = 50;
        assert_eq!(e.stamp_spacing(), 2);
        e.brush_spacing = 100;
        e.custom_brush = Some(Brush::new("tall", 2, 5, vec![255; 10]));
        e.brush_kind = BrushKind::Custom;
        assert_eq!(e.stamp_spacing(), 5);
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

    #[test]
    fn background_colour_is_user_data_and_undoable() {
        let mut e = Editor::new(4, 4);
        e.sprite.user_data = Some(crate::sprite::UserData {
            text: "keep me".into(),
            ..Default::default()
        });
        assert_eq!(e.sprite.background_color(), None);
        e.set_background_color(Some(Rgba::rgb(10, 20, 30)));
        assert_eq!(e.sprite.background_color(), Some(Rgba::rgb(10, 20, 30)));
        assert_eq!(e.sprite.user_data.as_ref().unwrap().text, "keep me");
        e.undo();
        assert_eq!(e.sprite.background_color(), None);
        assert_eq!(e.sprite.user_data.as_ref().unwrap().text, "keep me");
        e.redo();
        assert_eq!(e.sprite.background_color(), Some(Rgba::rgb(10, 20, 30)));
        e.set_background_color(None);
        assert_eq!(e.sprite.background_color(), None);
        assert_eq!(e.sprite.user_data.as_ref().unwrap().text, "keep me");
    }

    #[test]
    fn undo_history_is_bounded_and_reports_its_cost() {
        let mut e = Editor::new(8, 8);
        e.undo_limit = 3;
        for i in 0..8u32 {
            e.paint_pixel(i, 0, Rgba::rgb(i as u8, 2, 3));
        }
        assert_eq!(e.undo.len(), 3, "oldest steps are discarded");
        assert!(e.history_bytes() > 0);
        e.undo();
        e.redo();
        assert!(e.undo.len() <= 3 && e.redo.len() <= 3);
    }

    #[test]
    fn parallel_pointwise_filters_match_sequential() {
        let (w, h) = (37, 64);
        let src: Vec<Rgba> = (0..w * h)
            .map(|i| {
                let x = (i % w) as u8;
                let y = (i / w) as u8;
                Rgba(
                    20 + x,
                    30 + y,
                    (x ^ y) * 3,
                    if (x + y).is_multiple_of(7) { 0 } else { 220 },
                )
            })
            .collect();
        for f in [
            Filter::Invert,
            Filter::Brightness(12),
            Filter::Contrast(1.3),
            Filter::Hue(20.0),
            Filter::Saturation(0.7),
            Filter::Curves {
                shadows: 20,
                midtones: 130,
                highlights: 230,
            },
            Filter::Levels {
                in_black: 5,
                in_white: 240,
                gamma: 1.1,
                out_black: 0,
                out_white: 255,
            },
            Filter::Balance {
                r: 10,
                g: -5,
                b: 20,
            },
            Filter::Desaturate,
            Filter::Posterize(3),
        ] {
            assert_eq!(
                apply_filter_parallel(&src, w, h, f, Rgba::rgb(1, 2, 3)),
                apply_filter(&src, w, h, f, Rgba::rgb(1, 2, 3)),
                "parallel result differs for {}",
                f.name()
            );
        }
    }
}
