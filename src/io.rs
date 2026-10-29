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

/// Static image formats vixel can read and write.
pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "bmp" | "tga" | "webp"
        )
    })
}

/// PCX (ZSoft Paintbrush) image, import-only.
pub fn is_pcx(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy())
        .is_some_and(|e| e.eq_ignore_ascii_case("pcx"))
}

/// Autodesk Animator FLI or Animator Pro FLC.
pub fn is_flic(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy())
        .is_some_and(|e| e.eq_ignore_ascii_case("flc") || e.eq_ignore_ascii_case("fli"))
}

/// Formats `load_any` can turn into a sprite, including the legacy codecs that
/// are import-only (PCX, FLC/FLI).
pub fn is_importable(path: &Path) -> bool {
    is_image(path) || is_pcx(path) || is_flic(path)
}

/// Load whatever file vixel can open, dispatched on the extension. The one
/// place that dispatch lives, so `:open` and the command line cannot drift.
pub fn load_any(path: &Path) -> Result<Sprite> {
    if is_ase(path) {
        crate::ase::load(path)
    } else if is_pcx(path) {
        import_pcx(path)
    } else if is_flic(path) {
        import_flic(path)
    } else if is_image(path) {
        import_image(path)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasFormat {
    Vixel,
    JsonHash,
    JsonArray,
}

impl AtlasFormat {
    pub fn name(self) -> &'static str {
        match self {
            AtlasFormat::Vixel => "vixel",
            AtlasFormat::JsonHash => "hash",
            AtlasFormat::JsonArray => "array",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "vixel" | "default" => AtlasFormat::Vixel,
            "hash" | "json-hash" | "jsonhash" => AtlasFormat::JsonHash,
            "array" | "json-array" | "jsonarray" => AtlasFormat::JsonArray,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetLayout {
    Horizontal,
    Vertical,
    Rows,
    Columns,
    Packed,
}

impl SheetLayout {
    pub fn name(self) -> &'static str {
        match self {
            SheetLayout::Horizontal => "horizontal",
            SheetLayout::Vertical => "vertical",
            SheetLayout::Rows => "rows",
            SheetLayout::Columns => "columns",
            SheetLayout::Packed => "packed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "horizontal" | "h" => SheetLayout::Horizontal,
            "vertical" | "v" => SheetLayout::Vertical,
            "rows" | "r" => SheetLayout::Rows,
            "columns" | "cols" | "c" => SheetLayout::Columns,
            "packed" | "pack" | "p" => SheetLayout::Packed,
            _ => return None,
        })
    }
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
    /// One PNG per frame (or per layer) using `filename`.
    pub sequence: bool,
    pub filename: String,
    pub per_layer: bool,
    /// `None` keeps the legacy uniform `padding` sheet.
    pub sheet_layout: Option<SheetLayout>,
    pub columns: Option<u32>,
    pub rows: Option<u32>,
    pub border_padding: u32,
    pub shape_padding: u32,
    pub inner_padding: u32,
    pub extrude: u32,
    pub atlas: AtlasFormat,
    /// GIF repeats: 0 = forever.
    pub gif_loop: u16,
    pub gif_dither: bool,
    pub gif_colors: u16,
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
            sequence: false,
            filename: "{title}_{frame}".into(),
            per_layer: false,
            sheet_layout: None,
            columns: None,
            rows: None,
            border_padding: 0,
            shape_padding: 0,
            inner_padding: 0,
            extrude: 0,
            atlas: AtlasFormat::Vixel,
            gif_loop: 0,
            gif_dither: false,
            gif_colors: 256,
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
    let title = stem
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vixel".into());
    let mut paths = Vec::new();
    if cfg.sheet {
        let sheet_path = with_suffix(stem, ".sheet.png");
        let sheet = if cfg.sheet_layout.is_some() {
            export_sheet_layout(&view, &sheet_path, cfg)?
        } else {
            export_sheet_scaled_padded(&view, &sheet_path, None, scale, padding)?
        };
        paths.push(sheet_path.clone());
        if cfg.meta {
            let meta_path = with_suffix(stem, ".json");
            let image = sheet_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size = image::open(&sheet_path)
                .with_context(|| format!("open {}", sheet_path.display()))?
                .to_rgba8()
                .dimensions();
            export_meta_format(&view, &sheet, &image, size, &meta_path, cfg, &title)?;
            paths.push(meta_path);
        }
    }
    if cfg.gif {
        let gif_path = with_suffix(stem, ".gif");
        export_gif_configured(
            &view,
            &gif_path,
            scale,
            cfg.gif_loop,
            cfg.gif_dither,
            cfg.gif_colors,
        )?;
        paths.push(gif_path);
    }
    if cfg.sequence {
        paths.extend(write_sequence(&view, cfg, stem)?);
    }
    if paths.is_empty() {
        bail!("export config has no output enabled");
    }
    Ok(paths)
}

/// Render a sequence filename. `{frame}` is 1-based and zero-padded to three
/// digits; `{layer}` is `all` when not splitting.
pub fn render_filename(
    pattern: &str,
    title: &str,
    frame: usize,
    layer: Option<&str>,
    tag: &str,
    duration: u32,
) -> String {
    let layer = layer.unwrap_or("all");
    let mut name = pattern
        .replace("{title}", title)
        .replace("{frame}", &format!("{:03}", frame + 1))
        .replace("{layer}", layer)
        .replace("{tag}", tag)
        .replace("{duration}", &duration.to_string());
    name = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if !name.to_ascii_lowercase().ends_with(".png") {
        name.push_str(".png");
    }
    name
}

fn unique_path(dir: &Path, base: &str, used: &mut std::collections::HashSet<String>) -> PathBuf {
    let mut name = base.to_string();
    let mut n = 2;
    while !used.insert(name.clone()) {
        let stem = base.strip_suffix(".png").unwrap_or(base);
        name = format!("{stem}_{n}.png");
        n += 1;
    }
    dir.join(name)
}

fn write_sequence(sprite: &Sprite, cfg: &ExportConfig, stem: &Path) -> Result<Vec<PathBuf>> {
    let dir = stem
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let title = stem
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vixel".into());
    let tag = cfg.tag.as_deref().unwrap_or("all");
    let scale = cfg.scale.clamp(1, 16);
    let per_layer = cfg.per_layer || cfg.filename.contains("{layer}");
    let mut used = std::collections::HashSet::new();
    let mut paths = Vec::new();
    for frame in 0..sprite.frames.len() {
        let duration = u32::from(sprite.frames[frame].duration_ms);
        if per_layer {
            for layer in 0..sprite.layers.len() {
                if sprite.layers[layer].group {
                    continue;
                }
                let mut only = sprite.clone();
                for (i, l) in only.layers.iter_mut().enumerate() {
                    l.visible = i == layer;
                }
                let name = render_filename(
                    &cfg.filename,
                    &title,
                    frame,
                    Some(&sprite.layers[layer].name),
                    tag,
                    duration,
                );
                let path = unique_path(dir, &name, &mut used);
                scaled(frame_image(&only, frame), scale)
                    .save(&path)
                    .with_context(|| format!("save {}", path.display()))?;
                paths.push(path);
            }
        } else {
            let name = render_filename(&cfg.filename, &title, frame, None, tag, duration);
            let path = unique_path(dir, &name, &mut used);
            scaled(frame_image(sprite, frame), scale)
                .save(&path)
                .with_context(|| format!("save {}", path.display()))?;
            paths.push(path);
        }
    }
    Ok(paths)
}

/// Export only a PNG sequence (filters/scale from `cfg`).
pub fn export_sequence(sprite: &Sprite, cfg: &ExportConfig, stem: &Path) -> Result<Vec<PathBuf>> {
    let view = configured_sprite(sprite, cfg)?;
    write_sequence(&view, cfg, stem)
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Sheet {
    pub frame_w: u32,
    pub frame_h: u32,
    pub columns: u32,
    pub rows: u32,
    #[serde(default)]
    pub padding: u32,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub border_padding: u32,
    #[serde(default)]
    pub shape_padding: u32,
    #[serde(default)]
    pub inner_padding: u32,
    #[serde(default)]
    pub extrude: u32,
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
        layout: "uniform".into(),
        border_padding: 0,
        shape_padding: pad,
        inner_padding: pad,
        extrude: 0,
    })
}

/// Copy edge pixels outward by `amount`, framing the source.
fn extrude(img: &image::RgbaImage, amount: u32) -> image::RgbaImage {
    let e = amount.min(64);
    if e == 0 {
        return img.clone();
    }
    let (w, h) = img.dimensions();
    let mut out = image::RgbaImage::new(w + e * 2, h + e * 2);
    for y in 0..out.height() {
        let sy = (y as i32 - e as i32).clamp(0, h.saturating_sub(1) as i32) as u32;
        for x in 0..out.width() {
            let sx = (x as i32 - e as i32).clamp(0, w.saturating_sub(1) as i32) as u32;
            out.put_pixel(x, y, *img.get_pixel(sx, sy));
        }
    }
    out
}

/// Export a sheet using the configured layout, padding and extrusion.
pub fn export_sheet_layout(sprite: &Sprite, path: &Path, cfg: &ExportConfig) -> Result<Sheet> {
    let n = sprite.frames.len().max(1);
    let layout = cfg.sheet_layout.unwrap_or(SheetLayout::Rows);
    let (columns, rows) = match layout {
        SheetLayout::Horizontal => (n as u32, 1),
        SheetLayout::Vertical => (1, n as u32),
        SheetLayout::Rows => {
            let c = cfg
                .columns
                .unwrap_or_else(|| columns_for(n))
                .clamp(1, n as u32);
            (c, (n as u32).div_ceil(c))
        }
        SheetLayout::Columns => {
            let r = cfg
                .rows
                .unwrap_or_else(|| columns_for(n))
                .clamp(1, n as u32);
            ((n as u32).div_ceil(r), r)
        }
        SheetLayout::Packed => {
            let c = cfg
                .columns
                .unwrap_or_else(|| columns_for(n))
                .clamp(1, n as u32);
            (c, (n as u32).div_ceil(c))
        }
    };
    let scale = cfg.scale.clamp(1, 16);
    let border = cfg.border_padding.min(64);
    let shape = cfg.shape_padding.min(64);
    let inner = cfg.inner_padding.min(64);
    let ex = cfg.extrude.min(64);
    let (fw, fh) = (sprite.width * scale, sprite.height * scale);
    let (cw, ch) = (fw + ex * 2 + inner * 2, fh + ex * 2 + inner * 2);
    let stride_x = cw + shape;
    let stride_y = ch + shape;
    let total_w = border * 2 + columns * cw + columns.saturating_sub(1) * shape;
    let total_h = border * 2 + rows * ch + rows.saturating_sub(1) * shape;
    let mut sheet_img = image::RgbaImage::new(total_w, total_h);
    for frame in 0..n {
        let cell = extrude(&scaled(frame_image(sprite, frame), scale), ex);
        let ox = border + (frame as u32 % columns) * stride_x + inner;
        let oy = border + (frame as u32 / columns) * stride_y + inner;
        image::imageops::overlay(&mut sheet_img, &cell, ox as i64, oy as i64);
    }
    sheet_img
        .save(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(Sheet {
        frame_w: fw,
        frame_h: fh,
        columns,
        rows,
        padding: shape,
        layout: layout.name().into(),
        border_padding: border,
        shape_padding: shape,
        inner_padding: inner,
        extrude: ex,
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

fn bayer4(x: u32, y: u32) -> f32 {
    const M: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    (f32::from(M[(y % 4) as usize][(x % 4) as usize]) + 0.5) / 16.0
}

/// Quantize to a fixed RGB cube (levels³ colours); alpha is thresholded.
pub fn gif_quantize(img: &image::RgbaImage, colors: u16, dither: bool) -> image::RgbaImage {
    let levels = ((f32::from(colors.clamp(2, 256))).cbrt().round() as u32).clamp(2, 8);
    let steps = (levels - 1) as f32;
    let mut out = img.clone();
    for y in 0..img.height() {
        for x in 0..img.width() {
            let p = *img.get_pixel(x, y);
            if p.0[3] < 128 {
                out.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
                continue;
            }
            let q = |c: u8, channel: u32| {
                let scaled = f32::from(c) / 255.0 * steps;
                let v = if dither {
                    (scaled + bayer4(x, y + channel * 4)).floor()
                } else {
                    scaled.round()
                };
                (v / steps * 255.0).round().clamp(0.0, 255.0) as u8
            };
            out.put_pixel(
                x,
                y,
                image::Rgba([q(p.0[0], 0), q(p.0[1], 1), q(p.0[2], 2), 255]),
            );
        }
    }
    out
}

/// GIF export with repeat count, palette size and optional ordered dithering.
pub fn export_gif_configured(
    sprite: &Sprite,
    path: &Path,
    scale: u32,
    repeat: u16,
    dither: bool,
    colors: u16,
) -> Result<()> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let mut encoder = GifEncoder::new_with_speed(std::io::BufWriter::new(file), 10);
    encoder
        .set_repeat(if repeat == 0 {
            Repeat::Infinite
        } else {
            Repeat::Finite(repeat)
        })
        .context("gif repeat")?;
    for frame in 0..sprite.frames.len().max(1) {
        let delay = sprite
            .frames
            .get(frame)
            .map_or(100, |f| f.duration_ms)
            .max(10) as u32;
        let img = scaled(frame_image(sprite, frame), scale);
        let img = if dither || colors < 256 {
            gif_quantize(&img, colors, dither)
        } else {
            img
        };
        encoder
            .encode_frame(image::Frame::from_parts(
                img,
                0,
                0,
                image::Delay::from_numer_denom_ms(delay, 1),
            ))
            .with_context(|| format!("gif frame {frame}"))?;
    }
    Ok(())
}

/// Per-frame content rectangles inside the sheet image.
pub fn sheet_frame_rects(sprite: &Sprite, sheet: &Sheet, cfg: &ExportConfig) -> Vec<(u32, u32)> {
    let n = sprite.frames.len();
    let mut out = Vec::with_capacity(n);
    if cfg.sheet_layout.is_some() {
        let ex = cfg.extrude.min(64);
        let inner = cfg.inner_padding.min(64);
        let shape = cfg.shape_padding.min(64);
        let border = cfg.border_padding.min(64);
        let cw = sheet.frame_w + ex * 2 + inner * 2;
        let ch = sheet.frame_h + ex * 2 + inner * 2;
        for frame in 0..n {
            let ox = border + (frame as u32 % sheet.columns) * (cw + shape) + inner + ex;
            let oy = border + (frame as u32 / sheet.columns) * (ch + shape) + inner + ex;
            out.push((ox, oy));
        }
    } else {
        let pad = sheet.padding;
        let cw = sheet.frame_w + pad * 2;
        let ch = sheet.frame_h + pad * 2;
        for frame in 0..n {
            let ox = (frame as u32 % sheet.columns) * cw + pad;
            let oy = (frame as u32 / sheet.columns) * ch + pad;
            out.push((ox, oy));
        }
    }
    out
}

/// Write vixel, JSON-hash or JSON-array atlas metadata.
pub fn export_meta_format(
    sprite: &Sprite,
    sheet: &Sheet,
    image: &str,
    image_size: (u32, u32),
    path: &Path,
    cfg: &ExportConfig,
    title: &str,
) -> Result<()> {
    if cfg.atlas == AtlasFormat::Vixel {
        return export_meta(sprite, sheet.clone(), image, path);
    }
    let rects = sheet_frame_rects(sprite, sheet, cfg);
    let tag = cfg.tag.as_deref().unwrap_or("all");
    let frames: Vec<serde_json::Value> = sprite
        .frames
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (x, y) = rects.get(i).copied().unwrap_or((0, 0));
            let filename =
                render_filename(&cfg.filename, title, i, None, tag, u32::from(f.duration_ms));
            serde_json::json!({
                "filename": filename,
                "frame": {"x": x, "y": y, "w": sheet.frame_w, "h": sheet.frame_h},
                "rotated": false,
                "trimmed": false,
                "spriteSourceSize": {"x": 0, "y": 0, "w": sheet.frame_w, "h": sheet.frame_h},
                "sourceSize": {"w": sheet.frame_w, "h": sheet.frame_h},
                "duration": f.duration_ms,
            })
        })
        .collect();
    let frame_tags: Vec<serde_json::Value> = sprite
        .tags
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "from": t.from,
                "to": t.to,
                "direction": t.direction.name(),
                "repeat": t.repeat,
                "color": [t.color.0, t.color.1, t.color.2],
            })
        })
        .collect();
    let layers: Vec<serde_json::Value> = sprite
        .layers
        .iter()
        .map(|l| {
            serde_json::json!({
                "name": l.name,
                "opacity": l.opacity,
                "blend": format!("{:?}", l.blend).to_lowercase(),
                "visible": l.visible,
                "group": l.group,
            })
        })
        .collect();
    let slices: Vec<serde_json::Value> = sprite
        .slices
        .iter()
        .map(|s| {
            let keys: Vec<serde_json::Value> = s
                .keys
                .iter()
                .map(|k| {
                    serde_json::json!({
                        "frame": k.frame,
                        "bounds": {"x": k.x, "y": k.y, "w": k.w, "h": k.h},
                        "center": k.center,
                        "pivot": k.pivot,
                    })
                })
                .collect();
            serde_json::json!({"name": s.name, "keys": keys})
        })
        .collect();
    let meta = serde_json::json!({
        "image": image,
        "size": {"w": image_size.0, "h": image_size.1},
        "scale": cfg.scale,
        "format": cfg.atlas.name(),
        "frameTags": frame_tags,
        "layers": layers,
        "slices": slices,
    });
    let value = if cfg.atlas == AtlasFormat::JsonHash {
        let map: serde_json::Map<String, serde_json::Value> = frames
            .into_iter()
            .filter_map(|f| {
                let name = f.get("filename")?.as_str()?.to_string();
                Some((name, f))
            })
            .collect();
        serde_json::json!({"frames": map, "meta": meta})
    } else {
        serde_json::json!({"frames": frames, "meta": meta})
    };
    let text = serde_json::to_string_pretty(&value).context("serialize atlas")?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
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
        "layout": sheet.layout,
        "border_padding": sheet.border_padding,
        "shape_padding": sheet.shape_padding,
        "inner_padding": sheet.inner_padding,
        "extrude": sheet.extrude,
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

/// Export one frame to PNG/JPG/BMP/TGA/WebP, dispatching on the extension.
pub fn export_image_scaled(sprite: &Sprite, frame: usize, path: &Path, scale: u32) -> Result<()> {
    let img = scaled(frame_image(sprite, frame), scale);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => {
            let file = std::fs::File::create(path)
                .with_context(|| format!("create {}", path.display()))?;
            let mut rgb = image::RgbImage::new(img.width(), img.height());
            for (x, y, p) in img.enumerate_pixels() {
                let a = u32::from(p.0[3]);
                let ch = |v: u8| ((u32::from(v) * a + 255 * (255 - a)) / 255) as u8;
                rgb.put_pixel(x, y, image::Rgb([ch(p.0[0]), ch(p.0[1]), ch(p.0[2])]));
            }
            image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 92)
                .encode_image(&rgb)
                .with_context(|| format!("save {}", path.display()))?;
        }
        "png" | "bmp" | "tga" | "webp" => {
            img.save(path)
                .with_context(|| format!("save {}", path.display()))?;
        }
        _ => bail!("unsupported image extension: {ext}"),
    }
    Ok(())
}

/// Import any supported image as a fresh single-layer, single-frame sprite.
pub fn import_image(path: &Path) -> Result<Sprite> {
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

/// Import a PCX (ZSoft Paintbrush) image. PCX has no alpha, so imported
/// pixels are opaque; paletted files are expanded through their palette.
pub fn import_pcx(path: &Path) -> Result<Sprite> {
    let mut reader =
        pcx::Reader::from_file(path).with_context(|| format!("open {}", path.display()))?;
    let (w, h) = (reader.width() as u32, reader.height() as u32);
    if w == 0 || h == 0 {
        bail!("{}: empty PCX", path.display());
    }
    let mut rgb = vec![0u8; (w as usize) * (h as usize) * 3];
    reader
        .read_rgb_pixels(&mut rgb)
        .with_context(|| format!("decode {}", path.display()))?;
    let mut sprite = Sprite::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;
            sprite.set(0, 0, x, y, Rgba(rgb[i], rgb[i + 1], rgb[i + 2], 255));
        }
    }
    Ok(sprite)
}

/// Import an Autodesk Animator FLI / Animator Pro FLC animation. Every frame
/// becomes a sprite frame and all frames use the file's playback speed.
pub fn import_flic(path: &Path) -> Result<Sprite> {
    let mut flic =
        flic::FlicFile::open(path).with_context(|| format!("open {}", path.display()))?;
    let w = flic.width() as u32;
    let h = flic.height() as u32;
    let count = flic.frame_count() as usize;
    if w == 0 || h == 0 || count == 0 {
        bail!("{}: empty FLIC", path.display());
    }
    let delay = flic.speed_msec().clamp(1, u16::MAX as u32) as u16;
    let mut sprite = Sprite::new(w, h);
    let mut pixels = vec![0u8; (w as usize) * (h as usize)];
    let mut palette = vec![0u8; 3 * 256];
    for frame in 0..count {
        if frame > 0 {
            sprite.add_frame_after(sprite.frames.len() - 1);
        }
        let index = sprite.frames.len() - 1;
        sprite.frames[index].duration_ms = delay;
        let mut raster = flic::RasterMut::new(w as usize, h as usize, &mut pixels, &mut palette);
        flic.read_next_frame(&mut raster)
            .with_context(|| format!("decode {} frame {frame}", path.display()))?;
        for y in 0..h {
            for x in 0..w {
                let p = pixels[(y * w + x) as usize] as usize * 3;
                sprite.set(
                    0,
                    index,
                    x,
                    y,
                    Rgba(palette[p], palette[p + 1], palette[p + 2], 255),
                );
            }
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

/// Import an Aseprite/TexturePacker atlas (image + JSON hash/array or XML).
pub fn import_atlas(image_path: &Path, meta_path: &Path) -> Result<Sprite> {
    let img = image::open(image_path)
        .with_context(|| format!("open {}", image_path.display()))?
        .to_rgba8();
    let text = std::fs::read_to_string(meta_path)
        .with_context(|| format!("read {}", meta_path.display()))?;
    if meta_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
        || text.trim_start().starts_with('<')
    {
        return import_atlas_xml(&img, &text);
    }
    import_atlas_json(&img, &text)
}

struct AtlasFrame {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    duration: u16,
}

fn import_atlas_json(img: &image::RgbaImage, text: &str) -> Result<Sprite> {
    use crate::sprite::{PlayDirection, Slice, SliceKey, Tag};
    let value: serde_json::Value = serde_json::from_str(text).context("parse atlas json")?;
    let entries: Vec<&serde_json::Value> = match value.get("frames") {
        Some(serde_json::Value::Array(a)) => a.iter().collect(),
        Some(serde_json::Value::Object(m)) => m.values().collect(),
        _ => bail!("atlas json has no frames"),
    };
    let mut frames = Vec::new();
    for f in entries {
        let rect = f.get("frame").unwrap_or(f);
        let num = |k: &str| rect.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
        let duration = f
            .get("duration")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(100)
            .clamp(1, u16::MAX as u64) as u16;
        frames.push(AtlasFrame {
            x: num("x"),
            y: num("y"),
            w: num("w").max(1),
            h: num("h").max(1),
            duration,
        });
    }
    if frames.is_empty() {
        bail!("atlas has no frames");
    }
    let (w, h) = (
        frames.iter().map(|f| f.x + f.w).max().unwrap_or(1),
        frames.iter().map(|f| f.y + f.h).max().unwrap_or(1),
    );
    let mut sprite = Sprite::new(w, h);
    for (i, f) in frames.iter().enumerate() {
        if i > 0 {
            sprite.add_frame_after(sprite.frames.len() - 1);
        }
        sprite.frames[i].duration_ms = f.duration;
        for y in 0..f.h.min(h) {
            for x in 0..f.w.min(w) {
                let px = img.get_pixel(f.x + x, f.y + y);
                let c = Rgba(px[0], px[1], px[2], px[3]);
                if !c.is_transparent() {
                    sprite.set(0, i, x, y, c);
                }
            }
        }
    }
    if let Some(meta) = value.get("meta") {
        if let Some(tags) = meta.get("frameTags").and_then(|v| v.as_array()) {
            for t in tags {
                let direction = match t
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("forward")
                {
                    "reverse" => PlayDirection::Reverse,
                    "pingpong" => PlayDirection::PingPong,
                    "pingpong-reverse" => PlayDirection::PingPongReverse,
                    _ => PlayDirection::Forward,
                };
                let color = t
                    .get("color")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        let c = |i: usize| a.get(i).and_then(|v| v.as_u64()).unwrap_or(255) as u8;
                        Rgba(c(0), c(1), c(2), 255)
                    })
                    .unwrap_or(Rgba(120, 170, 255, 255));
                sprite.tags.push(Tag {
                    name: t
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Tag")
                        .to_string(),
                    from: t.get("from").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                    to: t.get("to").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                    color,
                    direction,
                    repeat: t.get("repeat").and_then(|v| v.as_u64()).unwrap_or(0) as u16,
                    user_data: None,
                });
            }
        }
        if let Some(slices) = meta.get("slices").and_then(|v| v.as_array()) {
            for s in slices {
                let mut keys = Vec::new();
                for k in s
                    .get("keys")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    let bounds = k.get("bounds");
                    let n = |name: &str| {
                        bounds
                            .and_then(|b| b.get(name))
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(0)
                    };
                    keys.push(SliceKey {
                        frame: k.get("frame").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        x: n("x") as i32,
                        y: n("y") as i32,
                        w: n("w").max(0) as u32,
                        h: n("h").max(0) as u32,
                        center: k.get("center").and_then(|c| {
                            let a = c.as_array()?;
                            Some((
                                a.first()?.as_i64()? as i32,
                                a.get(1)?.as_i64()? as i32,
                                a.get(2)?.as_u64()? as u32,
                                a.get(3)?.as_u64()? as u32,
                            ))
                        }),
                        pivot: k.get("pivot").and_then(|c| {
                            let a = c.as_array()?;
                            Some((a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32))
                        }),
                    });
                }
                sprite.slices.push(Slice {
                    name: s
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("slice")
                        .to_string(),
                    keys,
                    ..Default::default()
                });
            }
        }
    }
    sprite.clamp_tags();
    Ok(sprite)
}

fn import_atlas_xml(img: &image::RgbaImage, text: &str) -> Result<Sprite> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    let mut reader = Reader::from_str(text);
    let mut buf = Vec::new();
    let mut frames: Vec<AtlasFrame> = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"SubTexture" {
                    let mut map = std::collections::HashMap::new();
                    for attr in e.attributes().flatten() {
                        if let Ok(v) = attr.unescape_value() {
                            map.insert(
                                String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                                v.into_owned(),
                            );
                        }
                    }
                    let n = |k: &str| map.get(k).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
                    frames.push(AtlasFrame {
                        x: n("x"),
                        y: n("y"),
                        w: n("width").max(1),
                        h: n("height").max(1),
                        duration: map
                            .get("duration")
                            .and_then(|v| v.parse::<u16>().ok())
                            .unwrap_or(100),
                    });
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("xml: {e}")),
            _ => {}
        }
        buf.clear();
    }
    if frames.is_empty() {
        bail!("xml atlas has no SubTexture frames");
    }
    let (w, h) = (
        frames.iter().map(|f| f.x + f.w).max().unwrap_or(1),
        frames.iter().map(|f| f.y + f.h).max().unwrap_or(1),
    );
    let mut sprite = Sprite::new(w, h);
    for (i, f) in frames.iter().enumerate() {
        if i > 0 {
            sprite.add_frame_after(sprite.frames.len() - 1);
        }
        sprite.frames[i].duration_ms = f.duration;
        for y in 0..f.h.min(h) {
            for x in 0..f.w.min(w) {
                let p = img.get_pixel(f.x + x, f.y + y);
                let c = Rgba(p[0], p[1], p[2], p[3]);
                if !c.is_transparent() {
                    sprite.set(0, i, x, y, c);
                }
            }
        }
    }
    Ok(sprite)
}

/// Import a directory of images as frames, sorted by natural filename order.
pub fn import_sequence_dir(dir: &Path) -> Result<Sprite> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_importable(p))
        .collect();
    if files.is_empty() {
        bail!("no image files in {}", dir.display());
    }
    files.sort_by_key(|p| natural_key(&p.file_name().unwrap_or_default().to_string_lossy()));
    import_sequence(&files)
}

fn natural_key(s: &str) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let mut digits = String::new();
    let mut text = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !text.is_empty() {
                out.push((u64::MAX, std::mem::take(&mut text)));
            }
            digits.push(c);
        } else {
            if !digits.is_empty() {
                out.push((digits.parse().unwrap_or(0), std::mem::take(&mut digits)));
            }
            text.push(c);
        }
    }
    if !digits.is_empty() {
        out.push((digits.parse().unwrap_or(0), digits));
    }
    if !text.is_empty() {
        out.push((u64::MAX, text));
    }
    out
}

/// Import explicit image files as animation frames. PCX contributes one frame;
/// an FLC/FLI contributes all of its frames (and playback speed).
pub fn import_sequence(files: &[PathBuf]) -> Result<Sprite> {
    if files.is_empty() {
        bail!("no frames to import");
    }
    let mut parts = Vec::with_capacity(files.len());
    for path in files {
        parts.push(if is_pcx(path) {
            import_pcx(path)?
        } else if is_flic(path) {
            import_flic(path)?
        } else {
            import_image(path)?
        });
    }
    let w = parts.iter().map(|s| s.width).max().unwrap_or(1);
    let h = parts.iter().map(|s| s.height).max().unwrap_or(1);
    let mut sprite = Sprite::new(w.max(1), h.max(1));
    let mut first = true;
    for part in &parts {
        for frame in 0..part.frames.len() {
            if !first {
                sprite.add_frame_after(sprite.frames.len() - 1);
            }
            first = false;
            let index = sprite.frames.len() - 1;
            sprite.frames[index].duration_ms = part.frames[frame].duration_ms;
            for y in 0..part.height.min(h) {
                for x in 0..part.width.min(w) {
                    let c = part.get(0, frame, x, y);
                    if !c.is_transparent() {
                        sprite.set(0, index, x, y, c);
                    }
                }
            }
        }
    }
    Ok(sprite)
}

/// Split a sheet PNG back into frames. `cols`/`rows` of 0 mean "read the
/// sidecar"; explicit values tile the image evenly. Hash/array atlas metadata
/// is recognized and delegated to `import_atlas`.
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
        if serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .is_some_and(|v| v.get("meta").is_some() && v.get("frames").is_some())
        {
            return import_atlas(path, &meta_path);
        }
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

    /// Regenerate the legacy-format fixtures (run with `--ignored`).
    #[test]
    #[ignore = "writes tests/fixtures/*"]
    fn generate_legacy_codec_fixtures() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        std::fs::create_dir_all(&dir).unwrap();
        let pcx_path = dir.join("sample.pcx");
        let mut w = pcx::WriterRgb::create_file(&pcx_path, (4, 3), (72, 72)).unwrap();
        for row in [
            [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
            [0, 0, 0, 128, 128, 128, 255, 255, 0, 0, 255, 255],
            [255, 0, 255, 12, 34, 56, 90, 80, 70, 255, 128, 0],
        ] {
            w.write_row(&row).unwrap();
        }
        drop(w);

        let flc_path = dir.join("sample.flc");
        let mut w = flic::FlicFileWriter::create(&flc_path, 8, 8, 120).unwrap();
        let mut pal = vec![0u8; 3 * 256];
        pal[3..6].copy_from_slice(&[255, 0, 0]);
        pal[6..9].copy_from_slice(&[0, 0, 255]);
        let first = vec![1u8; 8 * 8];
        let mut second = first.clone();
        second[0..32].fill(2);
        {
            let raster = flic::Raster::new(8, 8, &first, &pal);
            w.write_next_frame(None, &raster).unwrap();
        }
        {
            let raster = flic::Raster::new(8, 8, &second, &pal);
            w.write_next_frame(Some(&flic::Raster::new(8, 8, &first, &pal)), &raster)
                .unwrap();
        }
        {
            // Ring frame: the player loops back to the first frame.
            let raster = flic::Raster::new(8, 8, &first, &pal);
            w.write_next_frame(Some(&flic::Raster::new(8, 8, &second, &pal)), &raster)
                .unwrap();
        }
        w.close().unwrap();
    }

    #[test]
    fn pcx_fixtures_load_as_opaque_pixels() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pcx");
        let sprite = load_any(&path).unwrap();
        assert_eq!((sprite.width, sprite.height), (4, 3));
        assert_eq!(sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(sprite.get(0, 0, 2, 0), Rgba::rgb(0, 0, 255));
        assert_eq!(sprite.get(0, 0, 1, 1), Rgba::rgb(128, 128, 128));
        assert_eq!(sprite.get(0, 0, 3, 2).3, 255, "PCX import is opaque");
    }

    #[test]
    fn flic_fixtures_load_all_frames_with_speed() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.flc");
        let sprite = load_any(&path).unwrap();
        assert_eq!((sprite.width, sprite.height), (8, 8));
        assert_eq!(sprite.frames.len(), 2);
        assert_eq!(sprite.frames[0].duration_ms, 120);
        assert_eq!(sprite.frames[1].duration_ms, 120);
        assert_eq!(sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(sprite.get(0, 1, 0, 0), Rgba::rgb(0, 0, 255));
        assert_eq!(sprite.get(0, 1, 7, 7), Rgba::rgb(255, 0, 0));
    }

    #[test]
    fn a_sequence_can_mix_pcx_and_images() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let sprite = import_sequence(&[dir.join("sample.pcx"), dir.join("sample.pcx")]).unwrap();
        assert_eq!(sprite.frames.len(), 2);
        assert_eq!(sprite.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(sprite.get(0, 1, 3, 2), Rgba::rgb(255, 128, 0));
    }

    #[test]
    fn atlas_metadata_hash_and_array_include_tags_layers_slices() {
        use crate::sprite::{Slice, SliceKey, Tag};
        let mut s = Sprite::new(2, 2);
        s.add_frame_after(0);
        s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
        s.set(0, 1, 1, 1, Rgba::rgb(0, 0, 255));
        s.tags.push(Tag {
            name: "walk".into(),
            from: 0,
            to: 1,
            color: Rgba::rgb(10, 20, 30),
            direction: crate::sprite::PlayDirection::Forward,
            repeat: 0,
            user_data: None,
        });
        s.slices.push(Slice {
            name: "head".into(),
            keys: vec![SliceKey {
                frame: 0,
                x: 0,
                y: 0,
                w: 1,
                h: 1,
                center: None,
                pivot: None,
            }],
            ..Default::default()
        });
        let cfg = ExportConfig {
            gif: false,
            atlas: AtlasFormat::JsonHash,
            filename: "{title}_{frame}".into(),
            ..Default::default()
        };
        let dir = std::env::temp_dir().join(format!("vixel-atlas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        export_config(&s, &cfg, &dir.join("walk")).unwrap();
        let text = std::fs::read_to_string(dir.join("walk.json")).unwrap();
        let meta: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(meta["frames"].as_object().unwrap().len(), 2);
        assert!(meta["frames"]["walk_001.png"]["frame"]["w"] == 2);
        assert_eq!(meta["meta"]["frameTags"][0]["name"], "walk");
        assert_eq!(meta["meta"]["layers"].as_array().unwrap().len(), 1);
        assert_eq!(meta["meta"]["slices"][0]["name"], "head");

        let mut array_cfg = cfg.clone();
        array_cfg.atlas = AtlasFormat::JsonArray;
        export_config(&s, &array_cfg, &dir.join("walk_array")).unwrap();
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("walk_array.json")).unwrap())
                .unwrap();
        assert_eq!(meta["frames"].as_array().unwrap().len(), 2);
        assert_eq!(meta["frames"][0]["filename"], "walk_array_001.png");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gif_quantize_limits_colours_and_keeps_alpha_threshold() {
        let mut img = image::RgbaImage::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                img.put_pixel(
                    x,
                    y,
                    image::Rgba([
                        (x * 31) as u8,
                        (y * 31) as u8,
                        128,
                        if x == 0 { 0 } else { 255 },
                    ]),
                );
            }
        }
        let out = gif_quantize(&img, 8, true);
        let unique: std::collections::BTreeSet<[u8; 3]> = out
            .pixels()
            .filter(|p| p.0[3] > 0)
            .map(|p| [p.0[0], p.0[1], p.0[2]])
            .collect();
        assert!(unique.len() <= 8, "{unique:?}");
        assert_eq!(out.get_pixel(0, 0).0[3], 0, "transparent stays out");
        let dir = std::env::temp_dir().join(format!("vixel-gifopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("anim.gif");
        let mut s = Sprite::new(2, 2);
        s.add_frame_after(0);
        export_gif_configured(&s, &path, 1, 2, true, 8).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sheet_layout_applies_padding_and_extrusion() {
        let mut s = Sprite::new(2, 2);
        s.add_frame_after(0);
        s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
        s.set(0, 1, 0, 0, Rgba::rgb(0, 0, 255));
        let cfg = ExportConfig {
            sheet_layout: Some(SheetLayout::Horizontal),
            border_padding: 2,
            shape_padding: 3,
            inner_padding: 1,
            extrude: 1,
            ..Default::default()
        };
        let dir = std::env::temp_dir().join(format!("vixel-sheet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sheet.png");
        let sheet = export_sheet_layout(&s, &path, &cfg).unwrap();
        assert_eq!((sheet.columns, sheet.rows), (2, 1));
        assert_eq!(sheet.layout, "horizontal");
        assert_eq!(sheet.extrude, 1);
        let img = image::open(&path).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (19, 10));
        assert_eq!(img.get_pixel(4, 4).0, [255, 0, 0, 255], "extruded start");
        assert_eq!(img.get_pixel(13, 4).0, [0, 0, 255, 255], "second cell");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sheet_layout_vertical_and_columns_choose_grids() {
        let mut s = Sprite::new(2, 2);
        s.add_frame_after(0);
        s.add_frame_after(1);
        let dir = std::env::temp_dir().join(format!("vixel-sheet-grid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut cfg = ExportConfig {
            sheet_layout: Some(SheetLayout::Vertical),
            ..Default::default()
        };
        let path = dir.join("vertical.png");
        let sheet = export_sheet_layout(&s, &path, &cfg).unwrap();
        assert_eq!((sheet.columns, sheet.rows), (1, 3));
        assert_eq!(image::open(&path).unwrap().to_rgba8().dimensions(), (2, 6));

        cfg.sheet_layout = Some(SheetLayout::Columns);
        cfg.rows = Some(2);
        let path = dir.join("columns.png");
        let sheet = export_sheet_layout(&s, &path, &cfg).unwrap();
        assert_eq!((sheet.columns, sheet.rows), (2, 2));
        assert_eq!(image::open(&path).unwrap().to_rgba8().dimensions(), (4, 4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_atlas_round_trips_through_import_sheet() {
        use crate::sprite::Tag;
        let mut s = Sprite::new(2, 2);
        s.add_frame_after(0);
        s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
        s.set(0, 1, 1, 1, Rgba::rgb(0, 0, 255));
        s.tags.push(Tag {
            name: "walk".into(),
            from: 0,
            to: 1,
            color: Rgba::rgb(1, 2, 3),
            direction: crate::sprite::PlayDirection::PingPong,
            repeat: 2,
            user_data: None,
        });
        let dir = std::env::temp_dir().join(format!("vixel-atlas-in-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = ExportConfig {
            gif: false,
            atlas: AtlasFormat::JsonHash,
            ..Default::default()
        };
        export_config(&s, &cfg, &dir.join("walk")).unwrap();
        let back = import_sheet(&dir.join("walk.sheet.png"), 0, 0).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(back.frames.len(), 2);
        assert_eq!(back.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(back.get(0, 1, 1, 1), Rgba::rgb(0, 0, 255));
        assert_eq!(back.tags[0].name, "walk");
        assert_eq!(
            back.tags[0].direction,
            crate::sprite::PlayDirection::PingPong
        );
        assert_eq!(back.tags[0].repeat, 2);
    }

    #[test]
    fn xml_atlas_imports_subtextures() {
        let dir = std::env::temp_dir().join(format!("vixel-xml-atlas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut img = image::RgbaImage::new(4, 2);
        for y in 0..2 {
            for x in 0..2 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                img.put_pixel(x + 2, y, image::Rgba([0, 0, 255, 255]));
            }
        }
        img.save(dir.join("atlas.png")).unwrap();
        let xml = r#"<TextureAtlas imagePath="atlas.png">
            <SubTexture name="a" x="0" y="0" width="2" height="2"/>
            <SubTexture name="b" x="2" y="0" width="2" height="2"/>
        </TextureAtlas>"#;
        std::fs::write(dir.join("atlas.xml"), xml).unwrap();
        let s = import_atlas(&dir.join("atlas.png"), &dir.join("atlas.xml")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(s.frames.len(), 2);
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(s.get(0, 1, 0, 0), Rgba::rgb(0, 0, 255));
    }

    #[test]
    fn image_sequence_imports_in_natural_order() {
        let dir = std::env::temp_dir().join(format!("vixel-seq-in-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, color) in [
            ("frame1.png", [255, 0, 0, 255]),
            ("frame2.png", [0, 255, 0, 255]),
            ("frame10.png", [0, 0, 255, 255]),
        ] {
            let mut img = image::RgbaImage::new(2, 2);
            for y in 0..2 {
                for x in 0..2 {
                    img.put_pixel(x, y, image::Rgba(color));
                }
            }
            img.save(dir.join(name)).unwrap();
        }
        let s = import_sequence_dir(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(s.frames.len(), 3);
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(s.get(0, 1, 0, 0), Rgba::rgb(0, 255, 0));
        assert_eq!(s.get(0, 2, 0, 0), Rgba::rgb(0, 0, 255));
    }

    #[test]
    fn extra_image_formats_export_and_reload() {
        let mut s = Sprite::new(2, 2);
        s.set(0, 0, 0, 0, Rgba::rgb(200, 10, 20));
        s.set(0, 0, 1, 1, Rgba(0, 80, 255, 255));
        let dir = std::env::temp_dir().join(format!("vixel-formats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for ext in ["bmp", "tga", "webp", "jpg"] {
            let path = dir.join(format!("frame.{ext}"));
            export_image_scaled(&s, 0, &path, 2).unwrap();
            assert!(path.exists(), "{ext}");
            let img = image::open(&path).unwrap();
            assert_eq!(img.width(), 4, "{ext} scale");
            assert!(is_image(&path));
            assert!(!is_png(&path));
        }
        let bmp = dir.join("frame.bmp");
        let back = load_any(&bmp).unwrap();
        assert_eq!(back.get(0, 0, 0, 0), Rgba::rgb(200, 10, 20));
        assert_eq!(back.get(0, 0, 2, 2), Rgba::rgb(0, 80, 255));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filename_tokens_render_and_sanitize() {
        assert_eq!(
            render_filename(
                "{title}_{frame}_{layer}_{tag}_{duration}",
                "walk",
                0,
                Some("Layer 1"),
                "run",
                100
            ),
            "walk_001_Layer_1_run_100.png"
        );
        assert_eq!(
            render_filename("{title}", "walk", 0, None, "all", 100),
            "walk.png"
        );
    }

    #[test]
    fn sequence_export_writes_frames_and_layers() {
        let mut s = Sprite::new(2, 2);
        s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
        s.add_layer_after(0);
        let top = s.layers.len() - 1;
        s.set(top, 0, 1, 1, Rgba::rgb(0, 0, 255));
        s.add_frame_after(0);
        let dir = std::env::temp_dir().join(format!("vixel-seq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = ExportConfig {
            sheet: false,
            gif: false,
            meta: false,
            sequence: true,
            per_layer: true,
            filename: "{title}_f{frame}_l{layer}".into(),
            scale: 2,
            ..Default::default()
        };
        let paths = export_sequence(&s, &cfg, &dir.join("walk")).unwrap();
        assert_eq!(paths.len(), 4, "2 frames x 2 layers");
        assert!(
            paths
                .iter()
                .any(|p| p.to_string_lossy().contains("walk_f001_lLayer_1"))
        );
        assert!(
            paths
                .iter()
                .any(|p| p.to_string_lossy().contains("walk_f002_lLayer_2"))
        );
        let img = image::open(&paths[0]).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (4, 4), "nearest upscale");
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        let loaded = import_image(&path).unwrap();
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

    #[test]
    fn project_round_trip_keeps_sprite_background_colour() {
        let mut s = Sprite::new(2, 2);
        s.set_background_color(Some(Rgba::rgb(7, 8, 9)));
        let path = std::env::temp_dir().join("vixel_bg_roundtrip.vpx");
        save_project(&s, &path).unwrap();
        let loaded = load_project(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded.background_color(), Some(Rgba::rgb(7, 8, 9)));
    }
}
