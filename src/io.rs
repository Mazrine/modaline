use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::color::Rgba;
use crate::sprite::Sprite;

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
    sprite.normalize_levels();
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

/// Export the composite of one frame as a PNG.
pub fn export_png(sprite: &Sprite, frame: usize, path: &Path) -> Result<()> {
    frame_image(sprite, frame)
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
    let n = sprite.frames.len().max(1);
    let columns = columns.unwrap_or_else(|| columns_for(n)).clamp(1, n as u32);
    let rows = (n as u32).div_ceil(columns);
    let (fw, fh) = (sprite.width, sprite.height);
    let mut sheet = image::RgbaImage::new(fw * columns, fh * rows);
    for frame in 0..n {
        let cell = frame_image(sprite, frame);
        let (ox, oy) = (frame as u32 % columns * fw, frame as u32 / columns * fh);
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
    })
}

/// Write the frames as a looping GIF, honouring each frame's duration.
pub fn export_gif(sprite: &Sprite, path: &Path) -> Result<()> {
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
                frame_image(sprite, frame),
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

/// Export an animation: `<stem>.sheet.png`, `<stem>.gif`, `<stem>.json`.
pub fn export_animation(
    sprite: &Sprite,
    stem: &Path,
    columns: Option<u32>,
) -> Result<Vec<PathBuf>> {
    let sheet_path = with_suffix(stem, ".sheet.png");
    let gif_path = with_suffix(stem, ".gif");
    let meta_path = with_suffix(stem, ".json");
    if let Some(dir) = stem.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let sheet = export_sheet(sprite, &sheet_path, columns)?;
    export_gif(sprite, &gif_path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

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
