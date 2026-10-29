//! Palette import/export: GIMP `.gpl`, plain `.hex`, and JASC `.pal`.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::color::Rgba;

/// Palette presets offered by `:pal preset`.
pub const PRESETS: &[&str] = &["default", "gray", "pico8", "dawnbringer16"];

pub fn preset(name: &str) -> Option<Vec<Rgba>> {
    Some(match name.to_ascii_lowercase().as_str() {
        "default" | "vixel" => crate::color::default_palette(),
        "gray" | "grey" | "grayscale" | "greyscale" => (0..16)
            .map(|i| {
                let v = (i * 17) as u8;
                Rgba::rgb(v, v, v)
            })
            .collect(),
        "pico8" | "pico-8" => vec![
            Rgba::rgb(0x00, 0x00, 0x00),
            Rgba::rgb(0x1d, 0x2b, 0x53),
            Rgba::rgb(0x7e, 0x25, 0x53),
            Rgba::rgb(0x00, 0x87, 0x51),
            Rgba::rgb(0xab, 0x52, 0x36),
            Rgba::rgb(0x5f, 0x57, 0x4f),
            Rgba::rgb(0xc2, 0xc3, 0xc7),
            Rgba::rgb(0xff, 0xf1, 0xe8),
            Rgba::rgb(0xff, 0x00, 0x4d),
            Rgba::rgb(0xff, 0xa3, 0x00),
            Rgba::rgb(0xff, 0xec, 0x27),
            Rgba::rgb(0x00, 0xe4, 0x36),
            Rgba::rgb(0x29, 0xad, 0xff),
            Rgba::rgb(0x83, 0x76, 0x9c),
            Rgba::rgb(0xff, 0x77, 0xa8),
            Rgba::rgb(0xff, 0xcc, 0xaa),
        ],
        "dawnbringer16" | "db16" => vec![
            Rgba::rgb(0x14, 0x0c, 0x1c),
            Rgba::rgb(0x44, 0x24, 0x34),
            Rgba::rgb(0x30, 0x34, 0x6d),
            Rgba::rgb(0x4e, 0x4a, 0x4e),
            Rgba::rgb(0x85, 0x4c, 0x30),
            Rgba::rgb(0x34, 0x65, 0x24),
            Rgba::rgb(0xd0, 0x46, 0x48),
            Rgba::rgb(0x75, 0x71, 0x61),
            Rgba::rgb(0x59, 0x7d, 0xce),
            Rgba::rgb(0xd2, 0x7d, 0x2c),
            Rgba::rgb(0x85, 0x95, 0xa1),
            Rgba::rgb(0x6d, 0xaa, 0x2c),
            Rgba::rgb(0xd2, 0xaa, 0x99),
            Rgba::rgb(0x6d, 0xc2, 0xca),
            Rgba::rgb(0xda, 0xd4, 0x5e),
            Rgba::rgb(0xde, 0xee, 0xd6),
        ],
        _ => return None,
    })
}

pub fn load(path: &Path) -> Result<Vec<Rgba>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let colors = match ext.as_str() {
        "gpl" => parse_gpl(&text),
        "hex" => parse_hex(&text),
        "pal" => parse_jasc(&text),
        _ => {
            if text.starts_with("GIMP") {
                parse_gpl(&text)
            } else if text.starts_with("JASC-PAL") {
                parse_jasc(&text)
            } else {
                parse_hex(&text)
            }
        }
    };
    if colors.is_empty() {
        bail!("no colors parsed from {}", path.display());
    }
    Ok(colors)
}

pub fn save(palette: &[Rgba], path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = match ext.as_str() {
        "hex" => to_hex(palette),
        "pal" => to_jasc(palette),
        _ => to_gpl(palette),
    };
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn parse_gpl(text: &str) -> Vec<Rgba> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("GIMP")
            || line.starts_with("Name:")
            || line.starts_with("Columns:")
        {
            continue;
        }
        let v: Vec<&str> = line.split_whitespace().collect();
        if v.len() >= 3
            && let (Ok(r), Ok(g), Ok(b)) =
                (v[0].parse::<u8>(), v[1].parse::<u8>(), v[2].parse::<u8>())
        {
            out.push(Rgba(r, g, b, 255));
        }
    }
    out
}

fn parse_hex(text: &str) -> Vec<Rgba> {
    let mut out = Vec::new();
    for line in text.lines() {
        let s = line.trim().trim_start_matches('#').trim();
        if s.len() < 6 {
            continue;
        }
        let parse = |a: usize, b: usize| u8::from_str_radix(&s[a..b], 16).ok();
        if let (Some(r), Some(g), Some(b)) = (parse(0, 2), parse(2, 4), parse(4, 6)) {
            let a = if s.len() >= 8 {
                parse(6, 8).unwrap_or(255)
            } else {
                255
            };
            out.push(Rgba(r, g, b, a));
        }
    }
    out
}

fn parse_jasc(text: &str) -> Vec<Rgba> {
    let mut out = Vec::new();
    for line in text.lines() {
        let v: Vec<&str> = line.split_whitespace().collect();
        if v.len() >= 3
            && let (Ok(r), Ok(g), Ok(b)) =
                (v[0].parse::<u8>(), v[1].parse::<u8>(), v[2].parse::<u8>())
        {
            out.push(Rgba(r, g, b, 255));
        }
    }
    out
}

fn to_gpl(palette: &[Rgba]) -> String {
    let mut s = String::from("GIMP Palette\nName: vixel\nColumns: 8\n#\n");
    for c in palette {
        s.push_str(&format!(
            "{:3} {:3} {:3}\t#{:02x}{:02x}{:02x}\n",
            c.0, c.1, c.2, c.0, c.1, c.2
        ));
    }
    s
}

fn to_hex(palette: &[Rgba]) -> String {
    palette
        .iter()
        .map(|c| format!("#{:02x}{:02x}{:02x}\n", c.0, c.1, c.2))
        .collect()
}

fn to_jasc(palette: &[Rgba]) -> String {
    let mut s = format!("JASC-PAL\n0100\n{}\n", palette.len());
    for c in palette {
        s.push_str(&format!("{} {} {}\n", c.0, c.1, c.2));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_known_and_complete() {
        assert_eq!(preset("default").unwrap().len(), 16);
        assert_eq!(preset("gray").unwrap().len(), 16);
        assert_eq!(preset("pico8").unwrap().len(), 16);
        assert_eq!(preset("dawnbringer16").unwrap().len(), 16);
        assert_eq!(preset("PICO-8").unwrap()[8], Rgba::rgb(255, 0, 77));
        assert!(preset("nope").is_none());
        assert_eq!(PRESETS.len(), 4);
    }

    #[test]
    fn parses_all_formats() {
        let gpl = "GIMP Palette\nName: x\nColumns: 4\n#\n  1   2   3\tA\n255 128 0\tB\n";
        assert_eq!(
            parse_gpl(gpl),
            vec![Rgba(1, 2, 3, 255), Rgba(255, 128, 0, 255)]
        );

        let hex = "#112233\n#aabbcc80\n";
        assert_eq!(
            parse_hex(hex),
            vec![Rgba(17, 34, 51, 255), Rgba(170, 187, 204, 128)]
        );

        let jasc = "JASC-PAL\n0100\n2\n0 0 0\n255 255 255\n";
        assert_eq!(
            parse_jasc(jasc),
            vec![Rgba(0, 0, 0, 255), Rgba(255, 255, 255, 255)]
        );
    }
}
