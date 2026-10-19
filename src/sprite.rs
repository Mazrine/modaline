use serde::{Deserialize, Serialize};

use crate::color::Rgba;

/// One cel: the image a single layer shows on a single frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cel {
    /// Row-major, `width * height` pixels.
    pub pixels: Vec<Rgba>,
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
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub duration_ms: u16,
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
}

impl Sprite {
    pub fn new(width: u32, height: u32) -> Self {
        let area = (width * height) as usize;
        let layers = vec![Layer {
            name: "Layer 1".into(),
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
        }];
        let frames = vec![Frame { duration_ms: 100 }];
        let cels = vec![vec![Cel {
            pixels: vec![Rgba::TRANSPARENT; area],
        }]];
        Self {
            width,
            height,
            layers,
            frames,
            cels,
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
    pub fn composite(&self, frame: usize) -> Vec<Rgba> {
        let mut out = vec![Rgba::TRANSPARENT; self.area()];
        for li in (0..self.layers.len()).rev() {
            let layer = &self.layers[li];
            if !layer.visible {
                continue;
            }
            let op = layer.opacity as u32;
            let blend = layer.blend;
            let cel = &self.cels[li][frame];
            for (i, dst) in out.iter_mut().enumerate() {
                let src = cel.pixels[i];
                if src.3 == 0 {
                    continue;
                }
                let sa = (src.3 as u32 * op) / 255;
                if sa == 0 {
                    continue;
                }
                *dst = blend_over(blend, src, sa, *dst);
            }
        }
        out
    }

    pub fn resize(&mut self, width: u32, height: u32) {
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
        self.width = width;
        self.height = height;
    }

    /// Mirror every cel horizontally.
    pub fn flip_h(&mut self) {
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let w = self.width;
                for y in 0..self.height {
                    for x in 0..w / 2 {
                        let a = (y * w + x) as usize;
                        let b = (y * w + (w - 1 - x)) as usize;
                        self.cels[layer][frame].pixels.swap(a, b);
                    }
                }
            }
        }
    }

    /// Mirror every cel vertically.
    pub fn flip_v(&mut self) {
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let (w, h) = (self.width, self.height);
                for y in 0..h / 2 {
                    for x in 0..w {
                        let a = (y * w + x) as usize;
                        let b = ((h - 1 - y) * w + x) as usize;
                        self.cels[layer][frame].pixels.swap(a, b);
                    }
                }
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
        self.width = nw;
        self.height = nh;
    }

    /// Insert a blank frame just after `idx`.
    pub fn add_frame_after(&mut self, idx: usize) {
        let cel = Cel {
            pixels: vec![Rgba::TRANSPARENT; self.area()],
        };
        for layer in self.cels.iter_mut() {
            layer.insert(idx + 1, cel.clone());
        }
        self.frames.insert(idx + 1, Frame { duration_ms: 100 });
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
    }

    pub fn remove_frame(&mut self, idx: usize) -> bool {
        if self.frames.len() <= 1 {
            return false;
        }
        for layer in self.cels.iter_mut() {
            layer.remove(idx);
        }
        self.frames.remove(idx);
        true
    }

    /// Insert a blank layer just above `idx` (towards the top of the stack).
    pub fn add_layer_after(&mut self, idx: usize) {
        let name = format!("Layer {}", self.layers.len() + 1);
        self.layers.insert(
            idx + 1,
            Layer {
                name,
                visible: true,
                opacity: 255,
                blend: BlendMode::Normal,
            },
        );
        let cel = Cel {
            pixels: vec![Rgba::TRANSPARENT; self.area()],
        };
        self.cels
            .insert(idx + 1, vec![cel; self.frames.len()]);
    }

    pub fn remove_layer(&mut self, idx: usize) -> bool {
        if self.layers.len() <= 1 {
            return false;
        }
        self.layers.remove(idx);
        self.cels.remove(idx);
        true
    }

    /// Move the layer at `from` to `to`, keeping `layers[0]` as topmost.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return false;
        }
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        let cels = self.cels.remove(from);
        self.cels.insert(to, cels);
        true
    }
}

/// Straight-alpha blend of `src` (effective alpha `sa`) over `dst` with `mode`.
fn blend_over(mode: BlendMode, src: Rgba, sa: u32, dst: Rgba) -> Rgba {
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
        for (nx, ny) in [(cx.wrapping_sub(1), cy), (cx + 1, cy), (cx, cy.wrapping_sub(1)), (cx, cy + 1)]
        {
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
                && (d(x - 1, y) > 1.0 || d(x + 1, y) > 1.0 || d(x, y - 1) > 1.0 || d(x, y + 1) > 1.0)
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
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
        });
        let cel = Cel {
            pixels: vec![Rgba::rgb(10, 20, 30)],
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
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
        });
        s.cels.push(vec![Cel {
            pixels: vec![Rgba::rgb(128, 128, 128)],
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
        assert!(s.move_layer(1, 0));
        assert_eq!(s.layers[0].name, "Second");
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
