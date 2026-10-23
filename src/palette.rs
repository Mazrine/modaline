//! Palette import/export: GIMP `.gpl`, plain `.hex`, and JASC `.pal`.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::color::Rgba;

pub fn load(path: &Path) -> Result<Vec<Rgba>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
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
            && let (Ok(r), Ok(g), Ok(b)) = (
                v[0].parse::<u8>(),
                v[1].parse::<u8>(),
                v[2].parse::<u8>(),
            )
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
            && let (Ok(r), Ok(g), Ok(b)) = (
                v[0].parse::<u8>(),
                v[1].parse::<u8>(),
                v[2].parse::<u8>(),
            )
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
    fn parses_all_formats() {
        let gpl = "GIMP Palette\nName: x\nColumns: 4\n#\n  1   2   3\tA\n255 128 0\tB\n";
        assert_eq!(parse_gpl(gpl), vec![Rgba(1, 2, 3, 255), Rgba(255, 128, 0, 255)]);

        let hex = "#112233\n#aabbcc80\n";
        assert_eq!(parse_hex(hex), vec![Rgba(17, 34, 51, 255), Rgba(170, 187, 204, 128)]);

        let jasc = "JASC-PAL\n0100\n2\n0 0 0\n255 255 255\n";
        assert_eq!(parse_jasc(jasc), vec![Rgba(0, 0, 0, 255), Rgba(255, 255, 255, 255)]);
    }
}
