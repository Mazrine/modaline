use serde::{Deserialize, Serialize};

use crate::color::Rgba;

/// One cel: the image a single layer shows on a single frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cel {
    /// Row-major, `width * height` pixels.
    pub pixels: Vec<Rgba>,
    /// 0..255, cel opacity (Aseprite stores this per cel).
    #[serde(default = "full")]
    pub opacity: u8,
}

fn full() -> u8 {
    255
}

impl Cel {
    pub fn blank(area: usize) -> Self {
        Self {
            pixels: vec![Rgba::TRANSPARENT; area],
            opacity: 255,
        }
    }
}

/// Per-layer blend mode (how this layer composites over those below it).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Add,
    Subtract,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    /// 0..255, layer opacity.
    pub opacity: u8,
    #[serde(default)]
    pub blend: BlendMode,
    /// Nesting depth: 0 = top level, 1 = inside a group, and so on.
    #[serde(default)]
    pub child_level: u16,
    /// A group layer holds the following layers with a deeper `child_level`.
    #[serde(default)]
    pub group: bool,
    /// Painted pixels are ignored (and its children's) when false.
    #[serde(default = "yes")]
    pub editable: bool,
    /// Per-pixel alpha multiplier applied when compositing.
    #[serde(default)]
    pub mask: Option<Mask>,
}

fn yes() -> bool {
    true
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            name: "Layer".into(),
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
            child_level: 0,
            group: false,
            editable: true,
            mask: None,
        }
    }
}

/// Per-layer alpha mask, row-major like a cel: 0 hides, 255 keeps.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mask {
    pub pixels: Vec<u8>,
}

impl Mask {
    /// Rebuild the mask for a new size/orientation; `f` maps old → new coords.
    fn map(&self, ow: u32, oh: u32, nw: u32, nh: u32, f: impl Fn(u32, u32) -> (u32, u32)) -> Mask {
        let mut pixels = vec![255; (nw * nh) as usize];
        for y in 0..oh {
            for x in 0..ow {
                let (nx, ny) = f(x, y);
                if nx < nw && ny < nh {
                    pixels[(ny * nw + nx) as usize] = self.pixels[(y * ow + x) as usize];
                }
            }
        }
        Mask { pixels }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub duration_ms: u16,
}

/// A named, inclusive frame range ("walk", "idle", ...).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    #[serde(default = "tag_color")]
    pub color: Rgba,
}

fn tag_color() -> Rgba {
    Rgba(120, 170, 255, 255)
}

/// A sprite: a grid of cels indexed `[layer][frame]`.
///
/// `layers[0]` is the topmost layer (drawn last, matching Aseprite's list).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub frames: Vec<Frame>,
    pub cels: Vec<Vec<Cel>>,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

impl Sprite {
    pub fn new(width: u32, height: u32) -> Self {
        let area = (width * height) as usize;
        let layers = vec![Layer {
            name: "Layer 1".into(),
            ..Default::default()
        }];
        let frames = vec![Frame { duration_ms: 100 }];
        let cels = vec![vec![Cel::blank(area)]];
        Self {
            width,
            height,
            layers,
            frames,
            cels,
            tags: Vec::new(),
        }
    }

    pub fn area(&self) -> usize {
        (self.width * self.height) as usize
    }

    pub fn get(&self, layer: usize, frame: usize, x: u32, y: u32) -> Rgba {
        self.cels[layer][frame].pixels[(y * self.width + x) as usize]
    }

    pub fn set(&mut self, layer: usize, frame: usize, x: u32, y: u32, c: Rgba) {
        let w = self.width;
        self.cels[layer][frame].pixels[(y * w + x) as usize] = c;
    }

    /// Flatten all visible layers of `frame` (top → bottom) into one image.
    /// Groups composite their children first, then apply their own
    /// visibility/opacity/blend/mask to the result.
    pub fn composite(&self, frame: usize) -> Vec<Rgba> {
        let mut out = vec![Rgba::TRANSPARENT; self.area()];
        self.composite_range(frame, 0, self.layers.len(), &mut out);
        out
    }

    /// Composite the sibling run `[start, end)` over `dst`, bottom-most first.
    fn composite_range(&self, frame: usize, start: usize, end: usize, dst: &mut [Rgba]) {
        let mut siblings = Vec::new();
        let mut k = start;
        while k < end {
            siblings.push(k);
            k = self.subtree_end(k).min(end);
        }
        for &i in siblings.iter().rev() {
            let layer = &self.layers[i];
            let stop = self.subtree_end(i).min(end);
            if !layer.visible || layer.opacity == 0 {
                continue;
            }
            let op = layer.opacity as u32;
            let cel_op = if layer.group {
                255
            } else {
                self.cels[i][frame].opacity as u32
            };
            let grouped;
            let src: &[Rgba] = if layer.group {
                let mut buf = vec![Rgba::TRANSPARENT; self.area()];
                self.composite_range(frame, i + 1, stop, &mut buf);
                grouped = buf;
                &grouped
            } else {
                &self.cels[i][frame].pixels
            };
            for (k, px) in src.iter().enumerate() {
                let mut sa = px.3 as u32 * op / 255 * cel_op / 255;
                if let Some(m) = &layer.mask {
                    sa = sa * *m.pixels.get(k).unwrap_or(&255) as u32 / 255;
                }
                if sa > 0 {
                    dst[k] = blend_over(layer.blend, *px, sa, dst[k]);
                }
            }
        }
    }

    /// Index one past the last descendant of layer `i`.
    pub fn subtree_end(&self, i: usize) -> usize {
        let level = self.layers[i].child_level;
        let mut k = i + 1;
        while k < self.layers.len() && self.layers[k].child_level > level {
            k += 1;
        }
        k
    }

    /// Can `i` (and all its ancestor groups) be painted on right now?
    pub fn layer_editable(&self, i: usize) -> bool {
        let mut ok = self.layers[i].editable && self.layers[i].visible && !self.layers[i].group;
        let mut level = self.layers[i].child_level;
        for k in (0..i).rev() {
            if level == 0 {
                break;
            }
            let kl = self.layers[k].child_level;
            if kl < level {
                ok &= self.layers[k].editable && self.layers[k].visible;
                level = kl;
            }
        }
        ok
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (ow, oh) = (self.width, self.height);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let mut np = vec![Rgba::TRANSPARENT; (width * height) as usize];
                for y in 0..height.min(self.height) {
                    for x in 0..width.min(self.width) {
                        np[(y * width + x) as usize] =
                            self.cels[layer][frame].pixels[(y * self.width + x) as usize];
                    }
                }
                self.cels[layer][frame].pixels = np;
            }
        }
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(ow, oh, width, height, |x, y| (x, y)));
            }
        }
        self.width = width;
        self.height = height;
    }

    /// Mirror every cel horizontally.
    pub fn flip_h(&mut self) {
        let (w, h) = (self.width, self.height);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                for y in 0..self.height {
                    for x in 0..w / 2 {
                        let a = (y * w + x) as usize;
                        let b = (y * w + (w - 1 - x)) as usize;
                        self.cels[layer][frame].pixels.swap(a, b);
                    }
                }
            }
        }
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(w, h, w, h, |x, y| (w - 1 - x, y)));
            }
        }
    }

    /// Mirror every cel vertically.
    pub fn flip_v(&mut self) {
        let (w, h) = (self.width, self.height);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                for y in 0..h / 2 {
                    for x in 0..w {
                        let a = (y * w + x) as usize;
                        let b = ((h - 1 - y) * w + x) as usize;
                        self.cels[layer][frame].pixels.swap(a, b);
                    }
                }
            }
        }
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(w, h, w, h, |x, y| (x, h - 1 - y)));
            }
        }
    }

    /// Rotate every cel 90° clockwise (width/height swap).
    pub fn rotate_cw(&mut self) {
        let (nw, nh) = (self.height, self.width);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let old = self.cels[layer][frame].pixels.clone();
                let mut np = vec![Rgba::TRANSPARENT; (nw * nh) as usize];
                for y in 0..self.height {
                    for x in 0..self.width {
                        let nx = self.height - 1 - y;
                        let ny = x;
                        np[(ny * nw + nx) as usize] = old[(y * self.width + x) as usize];
                    }
                }
                self.cels[layer][frame].pixels = np;
            }
        }
        let (w, h) = (self.width, self.height);
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(w, h, nw, nh, |x, y| (h - 1 - y, x)));
            }
        }
        self.width = nw;
        self.height = nh;
    }

    /// Insert a blank frame just after `idx`.
    pub fn add_frame_after(&mut self, idx: usize) {
        let cel = Cel::blank(self.area());
        for layer in self.cels.iter_mut() {
            layer.insert(idx + 1, cel.clone());
        }
        self.frames.insert(idx + 1, Frame { duration_ms: 100 });
        self.shift_tags_on_insert(idx);
    }

    /// Duplicate frame `idx` (with its cels and duration) right after it.
    pub fn duplicate_frame(&mut self, idx: usize) {
        let clones: Vec<Cel> = (0..self.layers.len())
            .map(|l| self.cels[l][idx].clone())
            .collect();
        for (l, cel) in clones.into_iter().enumerate() {
            self.cels[l].insert(idx + 1, cel);
        }
        self.frames.insert(idx + 1, self.frames[idx]);
        self.shift_tags_on_insert(idx);
    }

    pub fn remove_frame(&mut self, idx: usize) -> bool {
        if self.frames.len() <= 1 {
            return false;
        }
        for layer in self.cels.iter_mut() {
            layer.remove(idx);
        }
        self.frames.remove(idx);
        self.shift_tags_on_remove(idx);
        true
    }

    /// Move frame `from` to index `to` (all layers' cels move with it).
    pub fn move_frame(&mut self, from: usize, to: usize) -> bool {
        if from >= self.frames.len() || to >= self.frames.len() || from == to {
            return false;
        }
        for layer in &mut self.cels {
            let cel = layer.remove(from);
            layer.insert(to, cel);
        }
        let frame = self.frames.remove(from);
        self.frames.insert(to, frame);
        true
    }

    /// Keep tags within the frame range; drop any that collapse.
    pub fn clamp_tags(&mut self) {
        let last = self.frames.len().saturating_sub(1);
        self.tags.retain_mut(|t| {
            t.to = t.to.min(last);
            t.from = t.from.min(t.to);
            true
        });
    }

    /// A frame was inserted at `idx + 1`: ranges at/after it move, and a range
    /// spanning it grows.
    fn shift_tags_on_insert(&mut self, idx: usize) {
        for t in &mut self.tags {
            if t.from > idx {
                t.from += 1;
                t.to += 1;
            } else if t.to >= idx {
                t.to += 1;
            }
        }
    }

    fn shift_tags_on_remove(&mut self, idx: usize) {
        self.tags.retain_mut(|t| {
            if t.from > idx {
                t.from -= 1;
                t.to -= 1;
            } else if t.to >= idx {
                t.to = t.to.saturating_sub(1);
                if t.to < t.from {
                    return false;
                }
            }
            true
        });
    }

    /// Insert a blank layer just above `idx` (towards the top of the stack).
    pub fn add_layer_after(&mut self, idx: usize) {
        let name = format!("Layer {}", self.layers.len() + 1);
        let child_level = self.layers[idx].child_level;
        self.layers.insert(
            idx + 1,
            Layer {
                name,
                child_level,
                ..Default::default()
            },
        );
        let cel = Cel::blank(self.area());
        self.cels.insert(idx + 1, vec![cel; self.frames.len()]);
    }

    /// Insert an empty group just above `idx`, at the same nesting level.
    pub fn add_group_after(&mut self, idx: usize) {
        let name = format!("Group {}", self.layers.len() + 1);
        let child_level = self.layers[idx].child_level;
        self.layers.insert(
            idx + 1,
            Layer {
                name,
                group: true,
                child_level,
                ..Default::default()
            },
        );
        let cel = Cel::blank(self.area());
        self.cels.insert(idx + 1, vec![cel; self.frames.len()]);
    }

    /// Remove layer `idx`; a group takes its whole subtree with it.
    pub fn remove_layer(&mut self, idx: usize) -> bool {
        if idx >= self.layers.len() {
            return false;
        }
        let end = self.subtree_end(idx);
        if end - idx >= self.layers.len() {
            return false;
        }
        self.layers.drain(idx..end);
        self.cels.drain(idx..end);
        self.normalize_levels();
        true
    }

    /// Move layer `from` (with its subtree) so it ends up starting at `to`.
    /// Returns the final start index, or `None` if the move is invalid.
    pub fn move_layer(&mut self, from: usize, to: usize) -> Option<usize> {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return None;
        }
        let end = self.subtree_end(from);
        if to > from && to < end {
            return None; // cannot move a group into itself
        }
        let n = end - from;
        let layers: Vec<Layer> = self.layers.drain(from..end).collect();
        let cels = self.cels.drain(from..end).collect::<Vec<_>>();
        let at = if to > from { to + 1 - n } else { to }.min(self.layers.len());
        self.layers.splice(at..at, layers);
        self.cels.splice(at..at, cels);
        self.normalize_levels();
        Some(at)
    }

    /// Clamp every `child_level` to a structurally valid nesting depth.
    pub fn normalize_levels(&mut self) {
        let mut prev = 0u16;
        for i in 0..self.layers.len() {
            let max = if i == 0 {
                0
            } else if self.layers[i - 1].group {
                prev + 1
            } else {
                prev
            };
            let level = self.layers[i].child_level.min(max);
            self.layers[i].child_level = level;
            prev = level;
        }
    }
}

/// Straight-alpha blend of `src` (effective alpha `sa`) over `dst` with `mode`.
pub(crate) fn blend_over(mode: BlendMode, src: Rgba, sa: u32, dst: Rgba) -> Rgba {
    let as_ = sa as f32 / 255.0;
    let ab = dst.3 as f32 / 255.0;
    let cs = [
        src.0 as f32 / 255.0,
        src.1 as f32 / 255.0,
        src.2 as f32 / 255.0,
    ];
    let cb = [
        dst.0 as f32 / 255.0,
        dst.1 as f32 / 255.0,
        dst.2 as f32 / 255.0,
    ];
    let ao = as_ + ab * (1.0 - as_);
    if ao <= 0.0 {
        return Rgba::TRANSPARENT;
    }
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        let b = blend_fn(mode, cs[i], cb[i]);
        // Premultiplied source-over with separable blend (W3C compositing spec).
        let co = as_ * (1.0 - ab) * cs[i] + as_ * ab * b + (1.0 - as_) * ab * cb[i];
        out[i] = (co / ao).clamp(0.0, 1.0);
    }
    Rgba(
        (out[0] * 255.0).round() as u8,
        (out[1] * 255.0).round() as u8,
        (out[2] * 255.0).round() as u8,
        (ao * 255.0).round() as u8,
    )
}

fn blend_fn(mode: BlendMode, cs: f32, cb: f32) -> f32 {
    match mode {
        BlendMode::Normal => cs,
        BlendMode::Multiply => cs * cb,
        BlendMode::Screen => cs + cb - cs * cb,
        BlendMode::Overlay => {
            if cb <= 0.5 {
                2.0 * cs * cb
            } else {
                1.0 - 2.0 * (1.0 - cs) * (1.0 - cb)
            }
        }
        BlendMode::Darken => cs.min(cb),
        BlendMode::Lighten => cs.max(cb),
        BlendMode::Add => (cs + cb).min(1.0),
        BlendMode::Subtract => (cb - cs).max(0.0),
    }
}

fn set_px(
    s: &mut Sprite,
    l: usize,
    f: usize,
    x: u32,
    y: u32,
    color: Rgba,
    out: &mut Vec<(u32, u32, Rgba, Rgba)>,
) {
    let old = s.get(l, f, x, y);
    if old != color {
        s.set(l, f, x, y, color);
        out.push((x, y, old, color));
    }
}

/// 4-connected flood fill starting at `(x, y)`. Returns changed cells `(x, y, before, after)`.
pub fn flood_fill(
    s: &mut Sprite,
    layer: usize,
    frame: usize,
    x: u32,
    y: u32,
    color: Rgba,
) -> Vec<(u32, u32, Rgba, Rgba)> {
    let w = s.width;
    let h = s.height;
    let target = s.get(layer, frame, x, y);
    if target == color {
        return Vec::new();
    }
    let mut seen = vec![false; (w * h) as usize];
    seen[(y * w + x) as usize] = true;
    let mut stack = vec![(x, y)];
    let mut changes = Vec::new();

    while let Some((cx, cy)) = stack.pop() {
        set_px(s, layer, frame, cx, cy, color, &mut changes);
        for (nx, ny) in [
            (cx.wrapping_sub(1), cy),
            (cx + 1, cy),
            (cx, cy.wrapping_sub(1)),
            (cx, cy + 1),
        ] {
            if nx < w && ny < h {
                let i = (ny * w + nx) as usize;
                if !seen[i] && s.get(layer, frame, nx, ny) == target {
                    seen[i] = true;
                    stack.push((nx, ny));
                }
            }
        }
    }
    changes
}

/// Integer Bresenham line, inclusive of both endpoints.
pub fn line_points(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(u32, u32)> {
    let mut pts = Vec::new();
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut cx, mut cy) = (x0, y0);
    loop {
        pts.push((cx as u32, cy as u32));
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
    pts
}

/// Rectangle outline perimeter, inclusive.
pub fn rect_points(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(u32, u32)> {
    let (xmin, xmax) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (ymin, ymax) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let mut pts = Vec::new();
    for x in xmin..=xmax {
        pts.push((x as u32, ymin as u32));
        pts.push((x as u32, ymax as u32));
    }
    for y in ymin..=ymax {
        pts.push((xmin as u32, y as u32));
        pts.push((xmax as u32, y as u32));
    }
    pts.sort_unstable();
    pts.dedup();
    pts
}

/// Every pixel inside the rectangle, inclusive.
pub fn rect_filled_points(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(u32, u32)> {
    let (xmin, xmax) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (ymin, ymax) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let mut pts = Vec::with_capacity(((xmax - xmin + 1) * (ymax - ymin + 1)) as usize);
    for y in ymin..=ymax {
        for x in xmin..=xmax {
            pts.push((x as u32, y as u32));
        }
    }
    pts
}

/// Axis-aligned ellipse bounded by the corners `(x0,y0)`/`(x1,y1)`.
pub fn ellipse_points(x0: i32, y0: i32, x1: i32, y1: i32, filled: bool) -> Vec<(u32, u32)> {
    let cx = (x0 + x1) as f32 / 2.0;
    let cy = (y0 + y1) as f32 / 2.0;
    let rx = ((x1 - x0).abs() as f32 / 2.0).max(0.5);
    let ry = ((y1 - y0).abs() as f32 / 2.0).max(0.5);
    let (minx, maxx) = (x0.min(x1), x0.max(x1));
    let (miny, maxy) = (y0.min(y1), y0.max(y1));

    let d = |px: i32, py: i32| -> f32 {
        let dx = (px as f32 + 0.5 - cx) / rx;
        let dy = (py as f32 + 0.5 - cy) / ry;
        dx * dx + dy * dy
    };

    let mut pts = Vec::new();
    for y in miny..=maxy {
        for x in minx..=maxx {
            if filled {
                if d(x, y) <= 1.0 {
                    pts.push((x as u32, y as u32));
                }
            } else if d(x, y) <= 1.0
                && (d(x - 1, y) > 1.0
                    || d(x + 1, y) > 1.0
                    || d(x, y - 1) > 1.0
                    || d(x, y + 1) > 1.0)
            {
                pts.push((x as u32, y as u32));
            }
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_fill_respects_boundaries() {
        let mut s = Sprite::new(3, 3);
        s.set(0, 0, 1, 1, Rgba::rgb(255, 0, 0));
        let changes = flood_fill(&mut s, 0, 0, 0, 0, Rgba::rgb(0, 0, 255));
        // Only the 8 pixels not occupied by the red barrier should change.
        assert_eq!(changes.len(), 8);
        assert_eq!(s.get(0, 0, 1, 1), Rgba::rgb(255, 0, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(0, 0, 255));
    }

    #[test]
    fn composite_passes_transparent_and_overrides_opaque() {
        let mut s = Sprite::new(1, 1);
        // top layer (index 0) transparent -> bottom layer shows through
        s.layers.push(Layer {
            name: "Bottom".into(),
            ..Default::default()
        });
        let cel = Cel {
            pixels: vec![Rgba::rgb(10, 20, 30)],
            opacity: 255,
        };
        s.cels.push(vec![cel; 1]);
        assert_eq!(s.composite(0)[0], Rgba(10, 20, 30, 255));

        // top layer opaque -> hides bottom
        s.cels[0][0].pixels[0] = Rgba::rgb(200, 100, 50);
        assert_eq!(s.composite(0)[0], Rgba(200, 100, 50, 255));
    }

    #[test]
    fn multiply_blend_darkens() {
        let mut s = Sprite::new(1, 1);
        s.layers.push(Layer {
            name: "Bottom".into(),
            ..Default::default()
        });
        s.cels.push(vec![Cel {
            pixels: vec![Rgba::rgb(128, 128, 128)],
            opacity: 255,
        }]);
        // top layer multiplies gray over gray
        s.layers[0].blend = BlendMode::Multiply;
        s.cels[0][0].pixels[0] = Rgba::rgb(128, 128, 128);
        let c = s.composite(0)[0];
        // 0.5 * 0.5 = 0.25 -> ~64
        assert!((c.0 as i32 - 64).abs() <= 1, "got {}", c.0);
    }

    #[test]
    fn flip_mirrors() {
        let mut s = Sprite::new(3, 1);
        s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
        s.flip_h();
        assert_eq!(s.get(0, 0, 2, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::TRANSPARENT);
    }

    #[test]
    fn rotate_swaps_dims() {
        let mut s = Sprite::new(2, 3);
        s.set(0, 0, 0, 0, Rgba::rgb(1, 2, 3));
        s.rotate_cw();
        assert_eq!((s.width, s.height), (3, 2));
        assert_eq!(s.get(0, 0, 2, 0), Rgba::rgb(1, 2, 3));
    }

    #[test]
    fn move_layer_reorders() {
        let mut s = Sprite::new(1, 1);
        s.add_layer_after(0);
        s.layers[1].name = "Second".into();
        assert!(s.move_layer(1, 0).is_some());
        assert_eq!(s.layers[0].name, "Second");
    }

    #[test]
    fn group_visibility_and_opacity_apply_to_children() {
        let mut s = Sprite::new(1, 1);
        // [group, child, bottom]
        s.layers[0] = Layer {
            name: "Group".into(),
            group: true,
            ..Default::default()
        };
        s.layers.push(Layer {
            name: "Child".into(),
            child_level: 1,
            ..Default::default()
        });
        s.layers.push(Layer {
            name: "Bottom".into(),
            ..Default::default()
        });
        for _ in 0..2 {
            s.cels.push(vec![Cel {
                pixels: vec![Rgba::TRANSPARENT],
                opacity: 255,
            }]);
        }
        s.cels[0][0].pixels[0] = Rgba::rgb(255, 0, 0); // group itself is ignored
        s.cels[1][0].pixels[0] = Rgba::rgb(0, 0, 255);
        s.cels[2][0].pixels[0] = Rgba::rgb(0, 255, 0);

        assert_eq!(s.composite(0)[0], Rgba::rgb(0, 0, 255));
        s.layers[0].visible = false;
        assert_eq!(s.composite(0)[0], Rgba::rgb(0, 255, 0));
        s.layers[0].visible = true;
        s.layers[0].opacity = 128;
        let c = s.composite(0)[0];
        assert!(
            c.0 == 0 && c.1 > 100,
            "group opacity dips the child, got {c:?}"
        );
    }

    #[test]
    fn mask_multiplies_alpha_and_transforms() {
        let mut s = Sprite::new(2, 1);
        s.cels[0][0].pixels[0] = Rgba(255, 255, 255, 255);
        s.cels[0][0].pixels[1] = Rgba(255, 255, 255, 255);
        s.layers[0].mask = Some(Mask {
            pixels: vec![0, 255],
        });
        assert_eq!(s.composite(0)[0].3, 0);
        assert_eq!(s.composite(0)[1].3, 255);
        // Masks follow flips and resizes.
        s.flip_h();
        assert_eq!(s.composite(0)[0].3, 255);
        s.resize(3, 1);
        assert_eq!(s.layers[0].mask.as_ref().unwrap().pixels.len(), 3);
    }

    #[test]
    fn locked_or_group_layers_are_not_editable() {
        let mut s = Sprite::new(1, 1);
        assert!(s.layer_editable(0));
        s.layers[0].editable = false;
        assert!(!s.layer_editable(0));
        s.layers[0].editable = true;
        s.layers[0].group = true;
        assert!(!s.layer_editable(0));
        s.add_layer_after(0);
        s.layers[1].child_level = 1;
        assert!(s.layer_editable(1));
        s.layers[0].editable = false;
        assert!(!s.layer_editable(1), "a locked group locks its children");
        s.layers[0].editable = true;
        s.layers[0].visible = false;
        assert!(!s.layer_editable(1), "a hidden group is not editable");
    }

    #[test]
    fn group_remove_takes_children_and_indent_normalizes() {
        let mut s = Sprite::new(1, 1);
        s.add_group_after(0);
        s.add_layer_after(1);
        s.layers[2].child_level = 1;
        s.layers[2].name = "Inner".into();
        assert_eq!(s.subtree_end(1), 3);
        assert!(s.move_layer(1, 0).is_some(), "group block moves up");
        assert_eq!(s.layers[0].name, "Group 2");
        assert_eq!(s.layers[1].name, "Inner");
        assert!(s.remove_layer(0));
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.layers[0].name, "Layer 1");
        assert!(!s.remove_layer(0), "cannot delete the last layer");
    }

    #[test]
    fn move_frame_reorders_cels() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba::rgb(1, 1, 1));
        s.add_frame_after(0);
        s.set(0, 1, 0, 0, Rgba::rgb(2, 2, 2));
        s.add_frame_after(1);
        s.set(0, 2, 0, 0, Rgba::rgb(3, 3, 3));
        assert!(s.move_frame(2, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(3, 3, 3));
        assert_eq!(s.get(0, 1, 0, 0), Rgba::rgb(1, 1, 1));
        assert_eq!(s.get(0, 2, 0, 0), Rgba::rgb(2, 2, 2));
        assert!(!s.move_frame(0, 0));
    }

    #[test]
    fn tags_follow_frame_insert_and_remove() {
        let mut s = Sprite::new(1, 1);
        s.add_frame_after(0);
        s.add_frame_after(1);
        s.tags.push(Tag {
            name: "walk".into(),
            from: 1,
            to: 2,
            color: Rgba(1, 2, 3, 255),
        });
        s.add_frame_after(0); // insert before the tag -> shifts to 2..3
        assert_eq!((s.tags[0].from, s.tags[0].to), (2, 3));
        s.remove_frame(2);
        assert_eq!((s.tags[0].from, s.tags[0].to), (2, 2));
        s.remove_frame(2);
        assert!(s.tags.is_empty(), "removing the last tagged frame drops it");
    }

    #[test]
    fn duplicate_frame_copies_cels() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba::rgb(9, 9, 9));
        s.duplicate_frame(0);
        assert_eq!(s.frames.len(), 2);
        assert_eq!(s.get(0, 1, 0, 0), Rgba::rgb(9, 9, 9));
    }

    #[test]
    fn ellipse_and_rect_endpoints() {
        assert_eq!(line_points(0, 0, 2, 0).len(), 3);
        assert_eq!(rect_points(0, 0, 1, 1).len(), 4);
        assert!(!ellipse_points(0, 0, 4, 4, true).is_empty());
        assert!(!ellipse_points(0, 0, 4, 4, false).is_empty());
    }
}
