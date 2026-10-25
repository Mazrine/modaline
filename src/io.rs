use std::path::Path;

use anyhow::{Context, Result};

use crate::color::Rgba;
use crate::sprite::Sprite;

/// Save the whole sprite (frames + layers + cels) as JSON.
pub fn save_project(sprite: &Sprite, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(sprite).context("serialize sprite")?;
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

/// Export the composite of one frame as a PNG.
pub fn export_png(sprite: &Sprite, frame: usize, path: &Path) -> Result<()> {
    let (w, h) = (sprite.width, sprite.height);
    let comp = sprite.composite(frame);
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let c = comp[(y * w + x) as usize];
            img.put_pixel(x, y, image::Rgba([c.0, c.1, c.2, c.3]));
        }
    }
    img.save(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
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
