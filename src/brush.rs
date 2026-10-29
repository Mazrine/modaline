//! Brush masks: square/circle built-ins, custom PNG brushes and the user
//! brush library.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrushKind {
    #[default]
    Square,
    Circle,
    Custom,
}

impl BrushKind {
    pub fn name(self) -> &'static str {
        match self {
            BrushKind::Square => "square",
            BrushKind::Circle => "circle",
            BrushKind::Custom => "custom",
        }
    }

    pub fn parse(name: &str) -> Option<BrushKind> {
        Some(match name.to_ascii_lowercase().as_str() {
            "square" | "sq" => BrushKind::Square,
            "circle" | "round" => BrushKind::Circle,
            "custom" => BrushKind::Custom,
            _ => return None,
        })
    }
}

/// Coverage mask: 0 = untouched, 255 = full paint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Brush {
    pub name: String,
    pub w: u32,
    pub h: u32,
    /// Row-major `w * h` coverage.
    pub mask: Vec<u8>,
}

impl Brush {
    pub fn new(name: impl Into<String>, w: u32, h: u32, mask: Vec<u8>) -> Self {
        Brush {
            name: name.into(),
            w,
            h,
            mask,
        }
    }

    pub fn coverage(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return 0;
        }
        self.mask
            .get(y as usize * self.w as usize + x as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn max_dim(&self) -> u32 {
        self.w.max(self.h)
    }
}

pub fn square(n: u32) -> Brush {
    let n = n.max(1);
    Brush::new("square", n, n, vec![255; (n * n) as usize])
}

pub fn circle(n: u32) -> Brush {
    let n = n.max(1);
    let mut mask = vec![0u8; (n * n) as usize];
    // Shrink a hair so the diagonal corners of small odd circles fall out.
    let r2 = if n <= 2 {
        (n * n) as f32
    } else {
        (n * n) as f32 / 4.0 - 0.5
    };
    let c = (n as f32 - 1.0) / 2.0;
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            if dx * dx + dy * dy <= r2 {
                mask[(y * n + x) as usize] = 255;
            }
        }
    }
    Brush::new("circle", n, n, mask)
}

pub fn load(path: &Path) -> Result<Brush> {
    let image = image::open(path).with_context(|| format!("open {}", path.display()))?;
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mask: Vec<u8> = rgba.pixels().map(|p| p[3]).collect();
    if mask.iter().all(|a| *a == 0) {
        bail!("brush {} is fully transparent", path.display());
    }
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "brush".into());
    Ok(Brush::new(name, w, h, mask))
}

pub fn save(brush: &Brush, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut image = image::RgbaImage::new(brush.w.max(1), brush.h.max(1));
    for y in 0..brush.h {
        for x in 0..brush.w {
            let a = brush.coverage(x as i32, y as i32);
            image.put_pixel(x, y, image::Rgba([255, 255, 255, a]));
        }
    }
    image
        .save(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
}

/// `$XDG_CONFIG_HOME/vixel/brushes`, else `~/.config/vixel/brushes`.
pub fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("vixel").join("brushes"))
}

/// Library names (PNG stems), sorted.
pub fn list() -> Vec<String> {
    let Some(dir) = dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension().and_then(|x| x.to_str()) == Some("png"))
                .then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                .flatten()
        })
        .collect();
    names.sort();
    names
}

pub fn load_named(name: &str) -> Result<Brush> {
    let dir = dir().context("no config directory")?;
    let file = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .context("invalid brush name")?;
    let path = dir.join(file).with_extension("png");
    load(&path)
}

pub fn save_named(brush: &Brush, name: &str) -> Result<PathBuf> {
    let dir = dir().context("no config directory")?;
    let file = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("brush");
    let path = dir.join(file).with_extension("png");
    save(brush, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_the_expected_coverage() {
        let sq = square(3);
        assert_eq!(sq.mask.iter().filter(|a| **a > 0).count(), 9);
        let ci = circle(3);
        assert_eq!(ci.coverage(1, 1), 255);
        assert_eq!(ci.coverage(0, 0), 0, "corner is outside");
        assert!(ci.mask.iter().filter(|a| **a > 0).count() < 9);
        assert_eq!(square(0).w, 1);
    }

    #[test]
    fn png_brushes_round_trip_and_list() {
        let dir = std::env::temp_dir().join(format!("vixel-brush-{}", std::process::id()));
        let brush = Brush::new("test", 3, 2, vec![0, 255, 0, 128, 64, 255]);
        let path = dir.join("test.png");
        save(&brush, &path).unwrap();
        let back = load(&path).unwrap();
        assert_eq!((back.w, back.h), (3, 2));
        assert_eq!(back.mask, brush.mask);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
