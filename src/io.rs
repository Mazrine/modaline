use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::color::Rgba;
use crate::sprite::Sprite;

/// Export one retained tileset atlas as PNG (tile pixel dimensions).
pub fn export_tileset(sprite: &Sprite, id: Option<u32>, path: &Path) -> Result<()> {
    let ts = match id {
        Some(id) => sprite
            .tilesets
            .iter()
            .find(|t| t.id == id)
            .with_context(|| format!("no tileset with id {id}"))?,
        None => sprite.tilesets.first().context("sprite has no tilesets")?,
    };
    if ts.image.is_empty() {
        bail!("tileset {} has no embedded pixels", ts.id);
    }
    let w = ts.tile_w.max(1);
    let rows = (ts.image.len() as u32).div_ceil(w);
    let h = rows.max(ts.tile_h);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for i in 0..(w * h) as usize {
        let c = ts.image.get(i).copied().unwrap_or(Rgba::TRANSPARENT);
        rgba.extend_from_slice(&[c.0, c.1, c.2, c.3]);
    }
    image::RgbaImage::from_raw(w, h, rgba)
        .context("tileset atlas buffer")?
        .save(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
}

/// Export one cel's pixels (following links) as a PNG.
pub fn export_cel(sprite: &Sprite, layer: usize, frame: usize, path: &Path) -> Result<()> {
    let mut cel = sprite
        .cels
        .get(layer)
        .and_then(|f| f.get(frame))
        .context("cel index")?;
    let mut hops = 0;
    while let Some(target) = cel.link {
        if target >= sprite.frames.len() || hops > sprite.frames.len() {
            break;
        }
        cel = &sprite.cels[layer][target];
        hops += 1;
    }
    let (w, h) = (cel.w.max(1), cel.h.max(1));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for i in 0..(w * h) as usize {
        let c = cel.pixels.get(i).copied().unwrap_or(Rgba::TRANSPARENT);
        rgba.extend_from_slice(&[c.0, c.1, c.2, c.3]);
    }
    image::RgbaImage::from_raw(w, h, rgba)
        .context("cel buffer")?
        .save(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
}

/// Save the whole sprite (frames + layers + cels) as JSON.
pub fn save_project(sprite: &Sprite, path: &Path) -> Result<()> {
    // Compact: a 256² bundle is three 65k-pixel layers, and pretty-printing
    // blows it up ~8×. Nothing diffs pixel arrays by eye anyway.
    let json = serde_json::to_string(sprite).context("serialize sprite")?;
    std::fs::write(path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn load_project(path: &Path) -> Result<Sprite> {
    let json = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut sprite: Sprite = serde_json::from_str(&json).context("parse project")?;
    sprite.normalize_cels();
    sprite.normalize_levels();
    if sprite.color_mode == crate::sprite::ColorMode::Indexed && sprite.palette.is_empty() {
        // Projects written before indexed mode had a document palette.
        sprite.quantize_to_palette();
    }
    Ok(sprite)
}

pub fn is_ase(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ase") || e.eq_ignore_ascii_case("aseprite"))
}

pub fn is_png(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// Load whatever file vixel can open, dispatched on the extension. The one
/// place that dispatch lives, so `:open` and the command line cannot drift.
pub fn load_any(path: &Path) -> Result<Sprite> {
    if is_ase(path) {
        crate::ase::load(path)
    } else if is_png(path) {
        import_png(path)
    } else {
        load_project(path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportLayers {
    Visible,
    All,
    Indices,
}

/// Options shared by every export path. Frame indices are 0-based inclusive.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportConfig {
    pub frames: Option<(usize, usize)>,
    pub tag: Option<String>,
    pub layers: ExportLayers,
    pub layer_indices: Vec<usize>,
    pub ignore_empty: bool,
    pub merge_duplicates: bool,
    pub scale: u32,
    pub trim: bool,
    pub padding: u32,
    pub sheet: bool,
    pub gif: bool,
    pub meta: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        ExportConfig {
            frames: None,
            tag: None,
            layers: ExportLayers::Visible,
            layer_indices: Vec::new(),
            ignore_empty: false,
            merge_duplicates: false,
            scale: 1,
            trim: false,
            padding: 0,
            sheet: true,
            gif: true,
            meta: true,
        }
    }
}

impl ExportConfig {
    fn selected_frames(&self, sprite: &Sprite) -> Vec<usize> {
        let n = sprite.frames.len();
        if n == 0 {
            return Vec::new();
        }
        let mut frames: Vec<usize> = match &self.tag {
            Some(name) => sprite
                .tags
                .iter()
                .find(|t| &t.name == name)
                .map(|t| (t.from.min(t.to)..=t.from.max(t.to)).collect())
                .unwrap_or_else(|| (0..n).collect()),
            None => (0..n).collect(),
        };
        if let Some((a, b)) = self.frames {
            let (a, b) = (a.min(b).min(n - 1), a.max(b).min(n - 1));
            frames.retain(|f| (a..=b).contains(f));
        }
        frames
    }

    fn selected_layers(&self, sprite: &Sprite) -> Vec<usize> {
        match self.layers {
            ExportLayers::All => (0..sprite.layers.len()).collect(),
            ExportLayers::Visible => (0..sprite.layers.len())
                .filter(|i| sprite.layers[*i].visible)
                .collect(),
            ExportLayers::Indices => self
                .layer_indices
                .iter()
                .copied()
                .filter(|i| *i < sprite.layers.len())
                .collect(),
        }
    }
}

/// Build the export view: selected frames/layers, optional empty/duplicate
/// removal, and optional global trim.
pub fn configured_sprite(sprite: &Sprite, cfg: &ExportConfig) -> Result<Sprite> {
    let frames = cfg.selected_frames(sprite);
    let layers = cfg.selected_layers(sprite);
    if frames.is_empty() || layers.is_empty() {
        bail!("export selection is empty");
    }
    let mut out = sprite.clone();
    out.frames = frames.iter().map(|&f| sprite.frames[f]).collect();
    out.layers = layers.iter().map(|&l| sprite.layers[l].clone()).collect();
    out.cels = layers
        .iter()
        .map(|&l| frames.iter().map(|&f| sprite.cels[l][f].clone()).collect())
        .collect();
    out.tags.clear();
    out.slices.clear();
    out.raw_chunks.clear();

    let mut keep: Vec<usize> = Vec::new();
    let mut seen: Vec<Vec<Rgba>> = Vec::new();
    for frame in 0..out.frames.len() {
        let comp = out.export_composite(frame);
        if cfg.ignore_empty && comp.iter().all(|p| p.3 == 0) {
            continue;
        }
        if cfg.merge_duplicates && seen.iter().any(|prev| prev == &comp) {
            continue;
        }
        seen.push(comp);
        keep.push(frame);
    }
    if keep.is_empty() {
        bail!("export selection is empty after filtering");
    }
    if keep.len() != out.frames.len() {
        out.frames = keep.iter().map(|&f| out.frames[f]).collect();
        for row in &mut out.cels {
            *row = keep.iter().map(|&f| row[f].clone()).collect();
        }
    }

    if cfg.trim {
        let (w, h) = (out.width, out.height);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        let mut any = false;
        for frame in 0..out.frames.len() {
            let comp = out.export_composite(frame);
            for y in 0..h {
                for x in 0..w {
                    if comp[(y * w + x) as usize].3 > 0 {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x + 1);
                        y1 = y1.max(y + 1);
                        any = true;
                    }
                }
            }
        }
        if any {
            out.crop(x0, y0, x1 - x0, y1 - y0);
        }
    }
    Ok(out)
}

/// Export a configured sheet/GIF/meta bundle to `<stem>.*`.
pub fn export_config(sprite: &Sprite, cfg: &ExportConfig, stem: &Path) -> Result<Vec<PathBuf>> {
    let view = configured_sprite(sprite, cfg)?;
    let scale = cfg.scale.clamp(1, 16);
    let padding = cfg.padding.min(64);
    let mut paths = Vec::new();
    if cfg.sheet {
        let sheet_path = with_suffix(stem, ".sheet.png");
        let sheet = export_sheet_scaled_padded(&view, &sheet_path, None, scale, padding)?;
        paths.push(sheet_path.clone());
        if cfg.meta {
            let meta_path = with_suffix(stem, ".json");
            let image = sheet_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            export_meta(&view, sheet, &image, &meta_path)?;
            paths.push(meta_path);
        }
    }
    if cfg.gif {
        let gif_path = with_suffix(stem, ".gif");
        export_gif_scaled(&view, &gif_path, scale)?;
        paths.push(gif_path);
    }
    if paths.is_empty() {
        bail!("export config has no output enabled");
    }
    Ok(paths)
}

/// The composite of one frame as an image, the unit every export shares.
fn frame_image(sprite: &Sprite, frame: usize) -> image::RgbaImage {
    let (w, h) = (sprite.width, sprite.height);
    let comp = sprite.export_composite(frame);
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let c = comp[(y * w + x) as usize];
            img.put_pixel(x, y, image::Rgba([c.0, c.1, c.2, c.3]));
        }
    }
    img
}

/// Nearest-neighbour integer upscale, so exported pixels stay crisp.
fn scaled(img: image::RgbaImage, scale: u32) -> image::RgbaImage {
    if scale <= 1 {
        return img;
    }
    let (w, h) = img.dimensions();
    image::imageops::resize(
        &img,
        w * scale,
        h * scale,
        image::imageops::FilterType::Nearest,
    )
}

/// Export the composite of one frame as a PNG.
pub fn export_png(sprite: &Sprite, frame: usize, path: &Path) -> Result<()> {
    export_png_scaled(sprite, frame, path, 1)
}

/// Export one frame at an integer scale (2x, 4x, ...).
pub fn export_png_scaled(sprite: &Sprite, frame: usize, path: &Path, scale: u32) -> Result<()> {
    scaled(frame_image(sprite, frame), scale)
        .save(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
}

/// One frame's cell in the sheet, as written to `meta.json`.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Sheet {
    pub frame_w: u32,
    pub frame_h: u32,
    pub columns: u32,
    pub rows: u32,
    #[serde(default)]
    pub padding: u32,
}

/// Sheet columns for `frames` cells: roughly a 3:2 grid, so 24 frames pack 6x4
/// rather than 5x5 — a phone-shaped sheet reads better on screen and in a
/// sprite strip.
pub fn columns_for(frames: usize) -> u32 {
    if frames <= 1 {
        return 1;
    }
    ((frames as f32 * 1.5).sqrt().round() as u32).clamp(1, frames as u32)
}

/// Write every frame into one grid PNG. `columns` defaults to `columns_for`.
pub fn export_sheet(sprite: &Sprite, path: &Path, columns: Option<u32>) -> Result<Sheet> {
    export_sheet_scaled(sprite, path, columns, 1)
}

/// `export_sheet` with an integer upscale; `Sheet` reports the scaled frame
/// size, so the meta stays sliceable.
pub fn export_sheet_scaled(
    sprite: &Sprite,
    path: &Path,
    columns: Option<u32>,
    scale: u32,
) -> Result<Sheet> {
    export_sheet_scaled_padded(sprite, path, columns, scale, 0)
}

/// `export_sheet_scaled` with `padding` pixels around every frame, including
/// the outer border.
pub fn export_sheet_scaled_padded(
    sprite: &Sprite,
    path: &Path,
    columns: Option<u32>,
    scale: u32,
    padding: u32,
) -> Result<Sheet> {
    let n = sprite.frames.len().max(1);
    let columns = columns.unwrap_or_else(|| columns_for(n)).clamp(1, n as u32);
    let rows = (n as u32).div_ceil(columns);
    let scale = scale.max(1);
    let pad = padding.min(64);
    let (fw, fh) = (sprite.width * scale, sprite.height * scale);
    let (cw, ch) = (fw + pad * 2, fh + pad * 2);
    let mut sheet = image::RgbaImage::new(cw * columns, ch * rows);
    for frame in 0..n {
        let cell = scaled(frame_image(sprite, frame), scale);
        let (ox, oy) = (
            frame as u32 % columns * cw + pad,
            frame as u32 / columns * ch + pad,
        );
        image::imageops::overlay(&mut sheet, &cell, ox as i64, oy as i64);
    }
    sheet
        .save(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(Sheet {
        frame_w: fw,
        frame_h: fh,
        columns,
        rows,
        padding: pad,
    })
}

/// Write the frames as a looping GIF, honouring each frame's duration.
pub fn export_gif(sprite: &Sprite, path: &Path) -> Result<()> {
    export_gif_scaled(sprite, path, 1)
}

/// `export_gif` with an integer upscale.
pub fn export_gif_scaled(sprite: &Sprite, path: &Path, scale: u32) -> Result<()> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let mut encoder = GifEncoder::new_with_speed(std::io::BufWriter::new(file), 10);
    encoder.set_repeat(Repeat::Infinite).context("gif repeat")?;
    for frame in 0..sprite.frames.len().max(1) {
        let delay = sprite
            .frames
            .get(frame)
            .map_or(100, |f| f.duration_ms)
            .max(10) as u32;
        encoder
            .encode_frame(image::Frame::from_parts(
                scaled(frame_image(sprite, frame), scale),
                0,
                0,
                image::Delay::from_numer_denom_ms(delay, 1),
            ))
            .with_context(|| format!("gif frame {frame}"))?;
    }
    Ok(())
}

/// Write `meta.json`: enough for another tool (or anything else) to slice the sheet
/// and replay the timing without guessing.
pub fn export_meta(sprite: &Sprite, sheet: Sheet, image: &str, path: &Path) -> Result<()> {
    let total: u32 = sprite
        .frames
        .iter()
        .map(|f| f.duration_ms.max(10) as u32)
        .sum::<u32>()
        .max(1);
    let fps = sprite.frames.len() as f32 * 1000.0 / total as f32;
    let meta = serde_json::json!({
        "image": image,
        "frame_w": sheet.frame_w,
        "frame_h": sheet.frame_h,
        "columns": sheet.columns,
        "rows": sheet.rows,
        "padding": sheet.padding,
        "fps": (fps * 100.0).round() / 100.0,
        "profile": sprite.color_profile.name(),
        "color_mode": sprite.color_mode.name(),
        "pixel_aspect": [sprite.pixel_aspect.0, sprite.pixel_aspect.1],
        "frames": sprite.frames,
    });
    let text = serde_json::to_string_pretty(&meta).context("serialize meta")?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// A named region of the sprite: a grid cell, or the whole sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Cut the sprite into a `cols` x `rows` grid of slices. The grid does not
/// have to divide the sprite evenly: the last column and row absorb the
/// remainder, so no pixel is lost. `cols`/`rows` of 0 mean "use the sprite's
/// own grid bounds, else the whole sprite".
pub fn slices(sprite: &Sprite, cols: u32, rows: u32) -> Vec<Slice> {
    let (w, h) = (sprite.width, sprite.height);
    let (cols, rows) = match (cols, rows) {
        (0, _) | (_, 0) => match sprite.grid {
            (gw, gh) if gw > 0 && gh > 0 => (gw.min(w).max(1), gh.min(h).max(1)),
            _ => (1, 1),
        },
        (c, r) => (c.max(1).min(w), r.max(1).min(h)),
    };
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        for c in 0..cols {
            let x0 = w * c / cols;
            let y0 = h * r / rows;
            let x1 = w * (c + 1) / cols;
            let y1 = h * (r + 1) / rows;
            out.push(Slice {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            });
        }
    }
    out
}

/// Write each slice of the active frame as `<dir>/<stem>_r<row>c<col>.png`,
/// plus `<stem>.slices.json` naming the rects. Returns the files written.
pub fn export_slices(
    sprite: &Sprite,
    frame: usize,
    dir: &Path,
    stem: &str,
    cols: u32,
    rows: u32,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let cell = frame_image(sprite, frame);
    let grid = slices(sprite, cols, rows);
    let columns = if cols > 0 { cols } else { sprite.grid.0.max(1) };
    let mut paths = Vec::new();
    let mut named = Vec::new();
    for (i, s) in grid.iter().enumerate() {
        let columns = columns.max(1);
        let (r, c) = (i as u32 / columns, i as u32 % columns);
        let path = dir.join(format!("{stem}_r{r}c{c}.png"));
        image::imageops::crop_imm(&cell, s.x, s.y, s.w, s.h)
            .to_image()
            .save(&path)
            .with_context(|| format!("save {}", path.display()))?;
        named.push(serde_json::json!({
            "name": format!("r{r}c{c}"),
            "x": s.x,
            "y": s.y,
            "w": s.w,
            "h": s.h,
            "file": path.file_name().map(|n| n.to_string_lossy().into_owned()),
        }));
        paths.push(path);
    }
    let meta = dir.join(format!("{stem}.slices.json"));
    let text = serde_json::to_string_pretty(&named).context("serialize slices")?;
    std::fs::write(&meta, text).with_context(|| format!("write {}", meta.display()))?;
    paths.push(meta);
    Ok(paths)
}

fn slice_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Export each named slice's active key on `frame` as a PNG plus metadata.
pub fn export_named_slices(
    sprite: &Sprite,
    frame: usize,
    dir: &Path,
    stem: &str,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let cell = frame_image(sprite, frame);
    let mut paths = Vec::new();
    let mut named = Vec::new();
    let mut used = std::collections::HashSet::new();
    for (i, slice) in sprite.slices.iter().enumerate() {
        let Some(key) = slice
            .keys
            .iter()
            .rfind(|k| k.frame as usize <= frame)
            .or_else(|| slice.keys.first())
        else {
            continue;
        };
        let x0 = key.x.max(0);
        let y0 = key.y.max(0);
        let x1 = key.x.saturating_add(key.w as i32).min(sprite.width as i32);
        let y1 = key.y.saturating_add(key.h as i32).min(sprite.height as i32);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let mut base = slice_filename(&slice.name);
        if base.is_empty() {
            base = format!("slice{i}");
        }
        let mut name = base.clone();
        let mut n = 2u32;
        while !used.insert(name.clone()) {
            name = format!("{base}_{n}");
            n += 1;
        }
        let file = format!("{stem}_{name}_f{frame}.png");
        let path = dir.join(&file);
        image::imageops::crop_imm(
            &cell,
            x0 as u32,
            y0 as u32,
            (x1 - x0) as u32,
            (y1 - y0) as u32,
        )
        .to_image()
        .save(&path)
        .with_context(|| format!("save {}", path.display()))?;
        let color = slice
            .user_data
            .as_ref()
            .and_then(|d| d.color)
            .map(|c| vec![c.0, c.1, c.2, c.3]);
        named.push(serde_json::json!({
            "name": slice.name,
            "file": file,
            "frame": frame,
            "x": x0,
            "y": y0,
            "w": x1 - x0,
            "h": y1 - y0,
            "nine_patch": slice.nine_patch,
            "pivot": slice.pivot,
            "center": key.center.map(|(x, y, w, h)| serde_json::json!([x, y, w, h])),
            "pivot_xy": key.pivot.map(|(x, y)| serde_json::json!([x, y])),
            "color": color,
        }));
        paths.push(path);
    }
    let meta = dir.join(format!("{stem}.slices.json"));
    let text = serde_json::to_string_pretty(&named).context("serialize slices")?;
    std::fs::write(&meta, text).with_context(|| format!("write {}", meta.display()))?;
    paths.push(meta);
    Ok(paths)
}

/// Export an animation: `<stem>.sheet.png`, `<stem>.gif`, `<stem>.json`.
pub fn export_animation(
    sprite: &Sprite,
    stem: &Path,
    columns: Option<u32>,
) -> Result<Vec<PathBuf>> {
    export_animation_scaled(sprite, stem, columns, 1)
}

/// `export_animation` with an integer upscale for the sheet and GIF.
pub fn export_animation_scaled(
    sprite: &Sprite,
    stem: &Path,
    columns: Option<u32>,
    scale: u32,
) -> Result<Vec<PathBuf>> {
    let sheet_path = with_suffix(stem, ".sheet.png");
    let gif_path = with_suffix(stem, ".gif");
    let meta_path = with_suffix(stem, ".json");
    if let Some(dir) = stem.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let sheet = export_sheet_scaled(sprite, &sheet_path, columns, scale)?;
    export_gif_scaled(sprite, &gif_path, scale)?;
    let image_name = sheet_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    export_meta(sprite, sheet, &image_name, &meta_path)?;
    Ok(vec![sheet_path, gif_path, meta_path])
}

fn with_suffix(stem: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", stem.display()))
}

/// Import a PNG as a fresh single-layer, single-frame sprite.
pub fn import_png(path: &Path) -> Result<Sprite> {
    let img = image::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let mut sprite = Sprite::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            sprite.set(0, 0, x, y, Rgba(p[0], p[1], p[2], p[3]));
        }
    }
    Ok(sprite)
}

/// The sidecar `:export` writes beside a sheet: `<stem>.json` for
/// `<stem>.sheet.png`, else the same stem with `.json`.
fn meta_beside(path: &Path) -> PathBuf {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && let Some(stem) = name.strip_suffix(".sheet.png")
    {
        return path.with_file_name(format!("{stem}.json"));
    }
    path.with_extension("json")
}

#[derive(serde::Deserialize)]
struct SheetMeta {
    frame_w: u32,
    frame_h: u32,
    columns: u32,
    rows: u32,
    #[serde(default)]
    frames: Vec<MetaFrame>,
}

#[derive(serde::Deserialize)]
struct MetaFrame {
    #[serde(default = "default_duration")]
    duration_ms: u16,
}

fn default_duration() -> u16 {
    100
}

/// Split a sheet PNG back into frames. `cols`/`rows` of 0 mean "read the
/// sidecar"; explicit values tile the image evenly. The G6 round trip.
pub fn import_sheet(path: &Path, cols: u32, rows: u32) -> Result<Sprite> {
    let img = image::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .to_rgba8();
    let (iw, ih) = img.dimensions();
    let (columns, rows, fw, fh, durations) = if cols > 0 && rows > 0 {
        if iw % cols != 0 || ih % rows != 0 {
            anyhow::bail!("{iw}x{ih} does not tile as {cols}x{rows}");
        }
        (cols, rows, iw / cols, ih / rows, Vec::new())
    } else {
        let meta_path = meta_beside(path);
        if !meta_path.exists() {
            anyhow::bail!(
                "no sheet meta beside {} (pass cols rows instead)",
                path.display()
            );
        }
        let text = std::fs::read_to_string(&meta_path)
            .with_context(|| format!("read {}", meta_path.display()))?;
        let meta: SheetMeta = serde_json::from_str(&text).context("parse sheet meta")?;
        if meta.frame_w == 0 || meta.frame_h == 0 || meta.columns == 0 || meta.rows == 0 {
            anyhow::bail!("sheet meta has an empty frame grid");
        }
        if iw != meta.frame_w * meta.columns || ih != meta.frame_h * meta.rows {
            anyhow::bail!(
                "sheet is {iw}x{ih}, meta says {}x{}",
                meta.frame_w * meta.columns,
                meta.frame_h * meta.rows
            );
        }
        let durations = meta
            .frames
            .iter()
            .map(|f| f.duration_ms.max(10))
            .collect::<Vec<_>>();
        (
            meta.columns,
            meta.rows,
            meta.frame_w,
            meta.frame_h,
            durations,
        )
    };
    let mut sprite = Sprite::new(fw, fh);
    let count = (columns * rows) as usize;
    for i in 0..count {
        if i > 0 {
            sprite.add_frame_after(sprite.frames.len() - 1);
        }
        let (ox, oy) = ((i as u32 % columns) * fw, (i as u32 / columns) * fh);
        for y in 0..fh {
            for x in 0..fw {
                let p = img.get_pixel(ox + x, oy + y);
                let c = Rgba(p[0], p[1], p[2], p[3]);
                if !c.is_transparent() {
                    sprite.set(0, i, x, y, c);
                }
            }
        }
        if let Some(d) = durations.get(i) {
            sprite.frames[i].duration_ms = *d;
        }
    }
    Ok(sprite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    #[test]
    fn export_config_filters_merges_trims_and_layers() {
        let mut s = Sprite::new(4, 4);
        s.add_frame_after(0);
        s.add_frame_after(1);
        let red = Rgba::rgb(200, 0, 0);
        s.set(0, 0, 1, 1, red);
        s.set(0, 1, 1, 1, red);
        s.set(0, 2, 2, 2, Rgba::rgb(0, 0, 200));
        s.add_layer_after(0);
        let hidden = s.layers.len() - 1;
        s.layers[hidden].visible = false;
        s.set(hidden, 0, 0, 0, Rgba::rgb(0, 200, 0));

        let cfg = ExportConfig {
            frames: Some((0, 1)),
            merge_duplicates: true,
            trim: true,
            ..Default::default()
        };
        let out = configured_sprite(&s, &cfg).unwrap();
        assert_eq!(out.frames.len(), 1, "duplicate merged");
        assert_eq!(out.layers.len(), 1, "hidden layer filtered");
        assert_eq!((out.width, out.height), (1, 1), "trimmed to content");
        assert_eq!(out.get(0, 0, 0, 0), red);

        s.set(0, 2, 2, 2, Rgba::TRANSPARENT);
        let cfg = ExportConfig {
            ignore_empty: true,
            ..Default::default()
        };
        let out = configured_sprite(&s, &cfg).unwrap();
        assert_eq!(out.frames.len(), 2, "empty frame skipped");
    }

    #[test]
    fn export_config_writes_sheet_gif_and_meta() {
        let mut s = Sprite::new(2, 2);
        s.add_frame_after(0);
        s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
        s.set(0, 1, 1, 1, Rgba::rgb(0, 255, 0));
        let dir = std::env::temp_dir().join(format!("vixel-exportcfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let stem = dir.join("anim");
        let cfg = ExportConfig {
            scale: 2,
            padding: 1,
            ..Default::default()
        };
        let paths = export_config(&s, &cfg, &stem).unwrap();
        assert_eq!(paths.len(), 3);
        let img = image::open(dir.join("anim.sheet.png")).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (12, 6), "two 4x4 cells + 1px padding");
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("anim.json")).unwrap()).unwrap();
        assert_eq!(meta["padding"], 1);
        assert_eq!(meta["frame_w"], 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tileset_export_writes_the_atlas() {
        let mut s = Sprite::new(2, 2);
        s.tilesets.push(crate::sprite::Tileset {
            id: 9,
            tile_w: 2,
            tile_h: 2,
            num_tiles: 1,
            image: vec![
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 0, 255),
                Rgba::rgb(255, 255, 255),
            ],
            ..Default::default()
        });
        let path = std::env::temp_dir().join(format!("vixel-ts-export-{}.png", std::process::id()));
        export_tileset(&s, Some(9), &path).unwrap();
        let img = image::open(&path).unwrap().to_rgba8();
        let _ = std::fs::remove_file(&path);
        assert_eq!(img.dimensions(), (2, 2));
        assert_eq!(img.get_pixel(1, 0).0, [0, 255, 0, 255]);
    }

    #[test]
    fn tilesets_survive_vpx() {
        let mut s = Sprite::new(2, 2);
        s.layers[0].tileset = 7;
        s.tilesets.push(crate::sprite::Tileset {
            id: 7,
            name: "ground".into(),
            tile_w: 2,
            tile_h: 2,
            base_index: 1,
            flags: 3,
            num_tiles: 2,
            image: vec![
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 0, 255),
                Rgba::TRANSPARENT,
                Rgba::rgb(1, 2, 3),
                Rgba::rgb(4, 5, 6),
                Rgba::rgb(7, 8, 9),
                Rgba::rgb(10, 11, 12),
            ],
            external: None,
        });
        s.cels[0][0].tilemap = Some(crate::sprite::TilemapCel::new(
            1,
            vec![crate::sprite::TileRef {
                id: 1,
                flip_diagonal: true,
                ..Default::default()
            }],
        ));
        let path = std::env::temp_dir().join(format!("vixel-tileset-{}.vpx", std::process::id()));
        save_project(&s, &path).unwrap();
        let back = load_project(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.tilesets, s.tilesets);
        assert_eq!(back.layers[0].tileset, 7);
        assert_eq!(back.cels[0][0].tilemap, s.cels[0][0].tilemap);
    }

    #[test]
    fn slices_cover_the_sprite_even_when_the_grid_does_not_divide_it() {
        let mut s = Sprite::new(10, 7);
        assert_eq!(
            slices(&s, 0, 0),
            vec![Slice {
                x: 0,
                y: 0,
                w: 10,
                h: 7
            }]
        );

        s.grid = (3, 3);
        let g = slices(&s, 0, 0);
        assert_eq!(g.len(), 9);
        // Every pixel is in exactly one slice, and the last row/column absorb
        // the remainder (10/3 = 3,3,4 and 7/3 = 2,2,3).
        let area: u32 = g.iter().map(|s| s.w * s.h).sum();
        assert_eq!(area, 70);
        assert_eq!(g[2].w, 4);
        assert_eq!(g[6].h, 3);

        // Explicit columns/rows win over the sprite grid.
        let by_two = slices(&s, 2, 2);
        assert_eq!(by_two.len(), 4);
        assert_eq!(
            by_two[0],
            Slice {
                x: 0,
                y: 0,
                w: 5,
                h: 3
            }
        );
        assert_eq!(
            by_two[3],
            Slice {
                x: 5,
                y: 3,
                w: 5,
                h: 4
            }
        );
    }

    #[test]
    fn named_slice_export_writes_pngs_and_metadata() {
        use crate::sprite::{Slice, SliceKey};
        let mut s = Sprite::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                s.set(0, 0, x, y, Rgba(200, 30, 40, 255));
            }
        }
        s.slices.push(Slice {
            name: "button".into(),
            flags: 3,
            nine_patch: true,
            pivot: true,
            keys: vec![SliceKey {
                frame: 0,
                x: 1,
                y: 1,
                w: 2,
                h: 2,
                center: Some((1, 1, 1, 1)),
                pivot: Some((0, 1)),
            }],
            user_data: None,
        });
        let dir = std::env::temp_dir().join(format!("vixel-named-{}", std::process::id()));
        let paths = export_named_slices(&s, 0, &dir, "art").expect("named slices");
        assert_eq!(paths.len(), 2, "one png + metadata");
        let png = paths
            .iter()
            .find(|p| p.extension().is_some_and(|e| e == "png"))
            .unwrap();
        assert_eq!(image::image_dimensions(png).unwrap(), (2, 2));
        let meta = std::fs::read_to_string(dir.join("art.slices.json")).unwrap();
        assert!(meta.contains("\"name\": \"button\""));
        assert!(meta.contains("\"nine_patch\": true"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slice_export_writes_one_png_per_cell_and_a_sidecar() {
        let mut s = Sprite::new(4, 2);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        s.set(0, 0, 3, 1, Rgba(0, 0, 255, 255));
        s.grid = (2, 2);
        let dir = std::env::temp_dir().join(format!("vixel-slices-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = export_slices(&s, 0, &dir, "art", 0, 0).expect("slices");
        assert_eq!(paths.len(), 5, "4 cells + the sidecar");
        let pngs = paths
            .iter()
            .filter(|p| p.extension().unwrap() == "png")
            .count();
        assert_eq!(pngs, 4);
        let meta = std::fs::read_to_string(dir.join("art.slices.json")).unwrap();
        assert!(meta.contains("\"w\": 2") && meta.contains("r1c1"));
        let cell = image::open(dir.join("art_r0c0.png")).unwrap().to_rgba8();
        assert_eq!(cell.get_pixel(0, 0), &image::Rgba([255, 0, 0, 255]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn png_round_trip() {
        let mut s = Sprite::new(2, 2);
        s.set(0, 0, 1, 1, Rgba(12, 34, 56, 255));
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 128));
        let path = std::env::temp_dir().join("vixel_png_roundtrip_test.png");
        export_png(&s, 0, &path).unwrap();
        let loaded = import_png(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((loaded.width, loaded.height), (2, 2));
        assert_eq!(loaded.get(0, 0, 1, 1), Rgba(12, 34, 56, 255));
    }

    /// A sprite with `n` frames, each filled with its own flat colour, so a
    /// sliced sheet cell can be told from its neighbours by colour alone.
    fn animation(n: usize) -> Sprite {
        let mut s = Sprite::new(3, 2);
        for frame in 1..n {
            s.add_frame_after(s.frames.len() - 1);
            s.frames[frame].duration_ms = 40 + frame as u16;
        }
        for frame in 0..n {
            let c = Rgba(frame as u8 * 9 + 5, 40, 200, 255);
            for y in 0..2 {
                for x in 0..3 {
                    s.set(0, frame, x, y, c);
                }
            }
        }
        s
    }

    #[test]
    fn a_sheet_round_trips_through_import() {
        let s = animation(6);
        let dir = std::env::temp_dir().join(format!("vixel-importsheet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = export_animation(&s, &dir.join("idle"), None).unwrap();
        assert_eq!(paths.len(), 3);
        let sheet = dir.join("idle.sheet.png");

        let from_meta = import_sheet(&sheet, 0, 0).unwrap();
        assert_eq!(from_meta.frames.len(), 6);
        assert_eq!((from_meta.width, from_meta.height), (3, 2));
        for f in 0..6 {
            assert_eq!(from_meta.composite(f), s.composite(f), "frame {f}");
        }
        assert_eq!(from_meta.frames[1].duration_ms, s.frames[1].duration_ms);

        let explicit = import_sheet(&sheet, 3, 2).unwrap();
        assert_eq!(explicit.frames.len(), 6);
        assert_eq!(explicit.composite(5), s.composite(5));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_scale_upscales_raster_outputs() {
        let s = animation(2);
        let dir = std::env::temp_dir().join(format!("vixel-scale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let png = dir.join("frame.png");
        export_png_scaled(&s, 0, &png, 2).unwrap();
        let img = image::open(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (6, 4));
        let comp = s.composite(0);
        assert_eq!(
            img.get_pixel(1, 1).0,
            [comp[0].0, comp[0].1, comp[0].2, comp[0].3],
            "nearest-neighbour block"
        );

        let sheet_path = dir.join("anim.sheet.png");
        let sheet = export_sheet_scaled(&s, &sheet_path, None, 2).unwrap();
        assert_eq!((sheet.frame_w, sheet.frame_h), (6, 4));
        let sheet_img = image::open(&sheet_path).unwrap().to_rgba8();
        assert_eq!(sheet_img.dimensions(), (12, 4));

        let meta = dir.join("anim.json");
        export_meta(&s, sheet, "anim.sheet.png", &meta).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&meta).unwrap()).unwrap();
        assert_eq!(json["frame_w"], 6, "the meta stays sliceable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_twenty_four_frame_sheet_is_a_six_by_four_grid() {
        let s = animation(24);
        let path = std::env::temp_dir().join("vixel_sheet_test.sheet.png");
        let sheet = export_sheet(&s, &path, None).unwrap();
        assert_eq!((sheet.columns, sheet.rows), (6, 4));
        assert_eq!((sheet.frame_w, sheet.frame_h), (3, 2));

        // Every cell is the frame it claims to be: slicing the sheet must give
        // back the composites pixel for pixel, which is the G6 gate.
        let img = image::open(&path).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (18, 8));
        for frame in 0..24u32 {
            let (ox, oy) = (frame % 6 * 3, frame / 6 * 2);
            let comp = s.composite(frame as usize);
            for y in 0..2 {
                for x in 0..3 {
                    let want = comp[(y * 3 + x) as usize];
                    let got = img.get_pixel(ox + x, oy + y);
                    assert_eq!(
                        (got[0], got[1], got[2], got[3]),
                        (want.0, want.1, want.2, want.3)
                    );
                }
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_meta_describes_the_sheet_well_enough_to_slice_it() {
        let s = animation(7);
        let dir = std::env::temp_dir().join("vixel_meta_test");
        let stem = dir.join("idle");
        let paths = export_animation(&s, &stem, None).unwrap();
        assert_eq!(paths.len(), 3);
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths[2]).unwrap()).unwrap();

        assert_eq!(meta["image"], "idle.sheet.png");
        assert_eq!(meta["frame_w"], 3);
        assert_eq!(meta["frame_h"], 2);
        assert_eq!(meta["columns"], 3); // 7 frames -> 3x3, not 1x7
        assert_eq!(meta["rows"], 3);
        assert_eq!(meta["frames"].as_array().unwrap().len(), 7);
        assert_eq!(meta["frames"][3]["duration_ms"], 43);
        assert!(meta["fps"].as_f64().unwrap() > 0.0);
        for p in &paths {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_gif_holds_every_frame_and_loops() {
        use image::AnimationDecoder;
        let s = animation(5);
        let path = std::env::temp_dir().join("vixel_gif_test.gif");
        export_gif(&s, &path).unwrap();
        let decoder = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(
            std::fs::File::open(&path).unwrap(),
        ))
        .unwrap();
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 5);
        // 40 + frame ms, in hundredths of a second.
        // GIF timing is in centiseconds, so 42 ms lands within one tick of 40.
        let (n, d) = frames[2].delay().numer_denom_ms();
        let ms = n as f64 / d.max(1) as f64;
        assert!((30.0..=50.0).contains(&ms), "{ms} ms");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn indexed_palette_survives_a_project_round_trip() {
        let mut s = Sprite::new(2, 1);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        s.set(0, 0, 1, 0, Rgba(0, 0, 255, 255));
        s.quantize_to_palette();
        s.palette_names = vec![String::new(), "blue".into(), "red".into()];
        let path = std::env::temp_dir().join(format!(
            "vixel_palette_roundtrip_{}.vpx",
            std::process::id()
        ));
        save_project(&s, &path).unwrap();
        let back = load_project(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.color_mode, crate::sprite::ColorMode::Indexed);
        assert_eq!(back.palette, s.palette);
        assert_eq!(back.palette_names, s.palette_names);
        assert_eq!(back.transparent_index, s.transparent_index);
        assert_eq!(back.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
    }

    #[test]
    fn old_vpx_cels_without_bounds_are_normalized() {
        let mut s = Sprite::new(2, 2);
        s.set(0, 0, 1, 1, Rgba(1, 2, 3, 255));
        let path =
            std::env::temp_dir().join(format!("vixel_cel_migrate_{}.vpx", std::process::id()));
        save_project(&s, &path).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        let old = json
            .replace("\"x\":0,\"y\":0,\"w\":2,\"h\":2,", "")
            .replace("\"z_index\":0,", "");
        assert!(!old.contains("\"w\":2"));
        assert!(!old.contains("z_index"));
        std::fs::write(&path, old).unwrap();
        let back = load_project(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((back.cels[0][0].x, back.cels[0][0].y), (0, 0));
        assert_eq!((back.cels[0][0].w, back.cels[0][0].h), (2, 2));
        assert_eq!(back.cels[0][0].z_index, 0);
        assert_eq!(back.get(0, 0, 1, 1), Rgba(1, 2, 3, 255));
    }

    #[test]
    fn grid_origin_survives_vpx_and_old_files_default_to_zero() {
        let mut s = Sprite::new(1, 1);
        s.grid = (8, 6);
        s.grid_origin = (3, -4);
        let path =
            std::env::temp_dir().join(format!("vixel_grid_origin_{}.vpx", std::process::id()));
        save_project(&s, &path).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        let back = load_project(&path).unwrap();
        assert_eq!(back.grid, (8, 6));
        assert_eq!(back.grid_origin, (3, -4));

        let old = json.replace("\"grid_origin\":[3,-4],", "");
        assert!(!old.contains("grid_origin"));
        std::fs::write(&path, old).unwrap();
        let migrated = load_project(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(migrated.grid, (8, 6), "the size survives");
        assert_eq!(migrated.grid_origin, (0, 0), "missing origin defaults to 0");
    }

    #[test]
    fn load_any_dispatches_on_the_extension() {
        // The one dispatch shared by `:open` and the command line: a JSON
        // project has no extension rule of its own, so a `.vpx` must not be
        // mistaken for a PNG or an Aseprite file.
        let mut s = Sprite::new(3, 2);
        s.set(0, 0, 1, 1, Rgba(9, 8, 7, 255));
        let path = std::env::temp_dir().join("vixel_load_any_test.vpx");
        save_project(&s, &path).unwrap();
        let loaded = load_any(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((loaded.width, loaded.height), (3, 2));
        assert_eq!(loaded.get(0, 0, 1, 1), Rgba(9, 8, 7, 255));
        assert!(!is_ase(&path));
        assert!(!is_png(&path));
    }
}
