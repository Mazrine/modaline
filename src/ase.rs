//! `.ase` / `.aseprite` import/export.
//!
//! Aseprite file format, 32-bit RGBA cels. Reads both raw (type 0) and
//! zlib-compressed (type 2) cels, and both RGBA (depth 32) and indexed
//! (depth 8/16, via the palette chunk) sprites. Writes depth-32 RGBA with raw
//! cels, which Aseprite opens directly.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::color::Rgba;
use crate::sprite::{BlendMode, Cel, Frame, Layer, Sprite};

const MAGIC: u16 = 0xA5E0;
const FRAME_MAGIC: u16 = 0xF1FA;

const CHUNK_LAYER: u16 = 0x2004;
const CHUNK_CEL: u16 = 0x2005;
const CHUNK_PALETTE: u16 = 0x2019;
const CHUNK_PALETTE_RGB: u16 = 0x0004;
const CHUNK_PALETTE_RGBA: u16 = 0x0011;

// ---- export ----

pub fn save(sprite: &Sprite, path: &Path) -> Result<()> {
    let mut out: Vec<u8> = Vec::with_capacity(128 + sprite.area() * 4 + 256);
    write_header(&mut out, sprite);
    for frame in 0..sprite.frames.len() {
        write_frame(&mut out, sprite, frame);
    }
    let size = out.len() as u32;
    out[0..4].copy_from_slice(&size.to_le_bytes());
    std::fs::write(path, &out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_header(out: &mut Vec<u8>, s: &Sprite) {
    let mut h = vec![0u8; 128];
    h[0..4].copy_from_slice(&0u32.to_le_bytes()); // size (backfilled)
    h[4..6].copy_from_slice(&MAGIC.to_le_bytes());
    h[6..8].copy_from_slice(&(s.frames.len() as u16).to_le_bytes());
    h[8..10].copy_from_slice(&(s.width as u16).to_le_bytes());
    h[10..12].copy_from_slice(&(s.height as u16).to_le_bytes());
    h[12..14].copy_from_slice(&32u16.to_le_bytes()); // color depth: RGBA
    h[14..18].copy_from_slice(&1u32.to_le_bytes()); // flags: layer opacity valid
    h[18..20].copy_from_slice(&0u16.to_le_bytes()); // speed (deprecated)
    h[34] = 1; // pixel width
    h[35] = 1; // pixel height
    out.extend_from_slice(&h);
}

fn write_frame(out: &mut Vec<u8>, s: &Sprite, frame: usize) {
    let mut body: Vec<u8> = Vec::new();
    if frame == 0 {
        for layer in &s.layers {
            write_layer_chunk(&mut body, layer);
        }
    }
    for li in 0..s.layers.len() {
        if s.layers[li].group {
            continue; // groups have no cels
        }
        write_cel_chunk(&mut body, s, li, frame);
    }

    let mut f = Vec::new();
    f.extend_from_slice(&0u32.to_le_bytes()); // size (backfilled)
    f.extend_from_slice(&FRAME_MAGIC.to_le_bytes());
    f.extend_from_slice(&0u16.to_le_bytes()); // old chunk count (unused)
    f.extend_from_slice(&(s.frames[frame].duration_ms).to_le_bytes());
    f.extend_from_slice(&[0u8, 0]);
    f.extend_from_slice(&0u32.to_le_bytes()); // new chunk count (unused)
    f.extend_from_slice(&body);
    let size = f.len() as u32;
    f[0..4].copy_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&f);
}

fn write_layer_chunk(out: &mut Vec<u8>, layer: &Layer) {
    let name = layer.name.as_bytes();
    let mut b = Vec::new();
    let mut flags: u16 = if layer.visible { 1 } else { 0 };
    if layer.editable {
        flags |= 2; // editable
    }
    b.extend_from_slice(&flags.to_le_bytes());
    b.extend_from_slice(&(layer.group as u16).to_le_bytes()); // 0 image, 1 group
    b.extend_from_slice(&layer.child_level.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes()); // default width
    b.extend_from_slice(&0u16.to_le_bytes()); // default height
    b.extend_from_slice(&blend_word(layer.blend).to_le_bytes());
    b.push(layer.opacity);
    b.extend_from_slice(&[0u8; 3]);
    b.extend_from_slice(&(name.len() as u16).to_le_bytes());
    b.extend_from_slice(name);
    write_chunk(out, CHUNK_LAYER, &b);
}

fn write_cel_chunk(out: &mut Vec<u8>, s: &Sprite, li: usize, frame: usize) {
    let cel = &s.cels[li][frame];
    if cel.pixels.iter().all(|p| p.3 == 0) {
        return; // empty cels are implicit in .ase
    }
    let mut b = Vec::new();
    b.extend_from_slice(&(li as u16).to_le_bytes());
    b.extend_from_slice(&0i16.to_le_bytes()); // x
    b.extend_from_slice(&0i16.to_le_bytes()); // y
    b.push(255); // cel opacity
    b.extend_from_slice(&0u16.to_le_bytes()); // cel type: raw
    b.extend_from_slice(&[0u8; 7]); // reserved
    b.extend_from_slice(&(s.width as u16).to_le_bytes());
    b.extend_from_slice(&(s.height as u16).to_le_bytes());
    for p in &cel.pixels {
        b.extend_from_slice(&[p.0, p.1, p.2, p.3]);
    }
    write_chunk(out, CHUNK_CEL, &b);
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: u16, body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&chunk_type.to_le_bytes());
    out.extend_from_slice(body);
}

fn blend_word(b: BlendMode) -> u16 {
    match b {
        BlendMode::Normal => 0,
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Overlay => 3,
        BlendMode::Darken => 4,
        BlendMode::Lighten => 5,
        BlendMode::Add => 16,
        BlendMode::Subtract => 17,
    }
}

// ---- import ----

pub fn load(path: &Path) -> Result<Sprite> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if data.len() < 128 {
        bail!("not an .ase file (too small)");
    }
    if u16_at(&data, 4) != MAGIC {
        bail!("not an .ase/.aseprite file (bad magic)");
    }
    let frames_count = u16_at(&data, 6) as usize;
    let width = u16_at(&data, 8) as u32;
    let height = u16_at(&data, 10) as u32;
    let depth = u16_at(&data, 12);
    if depth != 8 && depth != 16 && depth != 32 {
        bail!("unsupported color depth {depth}");
    }

    let mut layers: Vec<Layer> = Vec::new();
    let mut durations: Vec<u16> = Vec::new();
    let mut cel_pixels: Vec<Vec<Option<Vec<Rgba>>>> = Vec::new(); // [layer][frame]
    let mut palette = vec![Rgba::TRANSPARENT; 256];

    let mut pos = 128usize;
    for frame in 0..frames_count {
        let frame_size = u32_at(&data, pos) as usize;
        if pos + frame_size > data.len() {
            bail!("frame {frame} extends past end of file");
        }
        let frame_end = pos + frame_size;
        durations.push(u16_at(&data, pos + 6));
        pos += 16;

        while pos + 6 <= frame_end {
            let chunk_size = u32_at(&data, pos) as usize;
            let chunk_type = u16_at(&data, pos + 4);
            let body = &data[pos + 6..pos + 6 + chunk_size];
            match chunk_type {
                CHUNK_LAYER => {
                    layers.push(parse_layer(body)?);
                    cel_pixels.push(Vec::new());
                }
                CHUNK_CEL => parse_cel(body, depth, width, height, &palette, &mut cel_pixels)?,
                CHUNK_PALETTE => parse_palette_new(body, &mut palette)?,
                CHUNK_PALETTE_RGB => parse_palette_rgb(body, &mut palette, false),
                CHUNK_PALETTE_RGBA => parse_palette_rgb(body, &mut palette, true),
                _ => {}
            }
            pos += 6 + chunk_size;
        }
        pos = frame_end;
    }

    if layers.is_empty() {
        layers.push(Layer {
            name: "Layer 1".into(),
            ..Default::default()
        });
        cel_pixels.push(Vec::new());
    }

    let frames: Vec<Frame> = durations
        .into_iter()
        .map(|d| Frame { duration_ms: d.max(1) })
        .collect();

    let cels: Vec<Vec<Cel>> = layers
        .iter()
        .enumerate()
        .map(|(li, _)| {
            (0..frames_count)
                .map(|fi| Cel {
                    pixels: cel_pixels
                        .get(li)
                        .and_then(|f| f.get(fi))
                        .and_then(|c| c.clone())
                        .unwrap_or_else(|| vec![Rgba::TRANSPARENT; (width * height) as usize]),
                })
                .collect()
        })
        .collect();

    let mut sprite = Sprite {
        width,
        height,
        layers,
        frames,
        cels,
    };
    sprite.normalize_levels();
    Ok(sprite)
}

fn parse_layer(body: &[u8]) -> Result<Layer> {
    let flags = u16_at(body, 0);
    let group = u16_at(body, 2) == 1;
    let child_level = u16_at(body, 4);
    let blend = blend_from_word(u16_at(body, 10));
    let opacity = *body.get(12).unwrap_or(&255);
    let name_len = u16_at(body, 16) as usize;
    let name_bytes = &body.get(18..18 + name_len).unwrap_or(&[]);
    let name = String::from_utf8_lossy(name_bytes).into_owned();
    Ok(Layer {
        name: if name.is_empty() { "Layer".into() } else { name },
        visible: flags & 1 != 0,
        opacity,
        blend,
        child_level,
        group,
        editable: flags & 2 != 0,
        mask: None,
    })
}

fn blend_from_word(w: u16) -> BlendMode {
    match w {
        1 => BlendMode::Multiply,
        2 => BlendMode::Screen,
        3 => BlendMode::Overlay,
        4 => BlendMode::Darken,
        5 => BlendMode::Lighten,
        16 => BlendMode::Add,
        17 => BlendMode::Subtract,
        _ => BlendMode::Normal,
    }
}

fn parse_cel(
    body: &[u8],
    depth: u16,
    canvas_w: u32,
    canvas_h: u32,
    palette: &[Rgba],
    cel_pixels: &mut [Vec<Option<Vec<Rgba>>>],
) -> Result<()> {
    let layer_index = u16_at(body, 0) as usize;
    let ox = i16_at(body, 2);
    let oy = i16_at(body, 4);
    let cel_type = u16_at(body, 7);

    let pixels = match cel_type {
        0 => {
            // raw
            let cw = u16_at(body, 16) as u32;
            let ch = u16_at(body, 18) as u32;
            let raw = &body[20..];
            decode_pixels(raw, depth, cw, ch, palette)
        }
        2 => {
            // zlib-compressed
            let cw = u16_at(body, 16) as u32;
            let ch = u16_at(body, 18) as u32;
            let compressed = &body[20..];
            let mut raw = Vec::new();
            flate2::read::ZlibDecoder::new(compressed).read_to_end(&mut raw)?;
            decode_pixels(&raw, depth, cw, ch, palette)
        }
        1 | 3 => {
            // linked cel / compressed tileset: not supported, treat as empty
            return Ok(());
        }
        _ => return Ok(()),
    };

    if cel_pixels.get(layer_index).is_none() {
        return Ok(());
    }
    // Place the (possibly smaller, offset) cel into a full-canvas buffer.
    let mut full = vec![Rgba::TRANSPARENT; (canvas_w * canvas_h) as usize];
    let cw = pixels.0;
    let ch = pixels.1;
    for yy in 0..ch {
        for xx in 0..cw {
            let px = ox as i32 + xx as i32;
            let py = oy as i32 + yy as i32;
            if px >= 0 && py >= 0 && (px as u32) < canvas_w && (py as u32) < canvas_h {
                full[(py as u32 * canvas_w + px as u32) as usize] = pixels.2[(yy * cw + xx) as usize];
            }
        }
    }
    cel_pixels[layer_index].push(Some(full));
    Ok(())
}

/// Decode a `cw x ch` block of pixel bytes into `(cw, ch, pixels)`.
fn decode_pixels(raw: &[u8], depth: u16, cw: u32, ch: u32, palette: &[Rgba]) -> (u32, u32, Vec<Rgba>) {
    let mut out = Vec::with_capacity((cw * ch) as usize);
    let mut i = 0usize;
    for _ in 0..cw * ch {
        let px = match depth {
            8 => {
                let idx = *raw.get(i).unwrap_or(&0) as usize;
                i += 1;
                palette[idx.min(255)]
            }
            16 => {
                let v = *raw.get(i).unwrap_or(&0);
                let a = *raw.get(i + 1).unwrap_or(&255);
                i += 2;
                Rgba(v, v, v, a)
            }
            _ => {
                // 32-bit RGBA
                let r = *raw.get(i).unwrap_or(&0);
                let g = *raw.get(i + 1).unwrap_or(&0);
                let b = *raw.get(i + 2).unwrap_or(&0);
                let a = *raw.get(i + 3).unwrap_or(&255);
                i += 4;
                Rgba(r, g, b, a)
            }
        };
        out.push(px);
    }
    (cw, ch, out)
}

fn parse_palette_new(body: &[u8], palette: &mut [Rgba]) -> Result<()> {
    let first = u32_at(body, 0) as usize;
    let last = u32_at(body, 4) as usize;
    let mut p = 8usize;
    for (idx, slot) in palette.iter_mut().enumerate() {
        if idx < first || idx > last {
            continue;
        }
        if p + 6 > body.len() {
            break;
        }
        let flags = u16_at(body, p);
        *slot = Rgba(body[p + 2], body[p + 3], body[p + 4], body[p + 5]);
        p += 6;
        if flags & 1 != 0 {
            // skip a UTF-16 name
            let name_len = u16_at(body, p) as usize;
            p += 2 + name_len * 2;
        }
    }
    Ok(())
}

fn parse_palette_rgb(body: &[u8], palette: &mut [Rgba], has_alpha: bool) {
    let count = u16_at(body, 0) as usize;
    let mut p = 2usize;
    for i in 0..count {
        if p + 3 > body.len() {
            break;
        }
        let r = body[p];
        let g = body[p + 1];
        let b = body[p + 2];
        let a = if has_alpha { *body.get(p + 3).unwrap_or(&255) } else { 255 };
        palette[i.min(255)] = Rgba(r, g, b, a);
        p += if has_alpha { 4 } else { 3 };
    }
}

// ---- little-endian readers (bounds-tolerant) ----

fn u16_at(d: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([*d.get(i).unwrap_or(&0), *d.get(i + 1).unwrap_or(&0)])
}

fn i16_at(d: &[u8], i: usize) -> i16 {
    i16::from_le_bytes([*d.get(i).unwrap_or(&0), *d.get(i + 1).unwrap_or(&0)])
}

fn u32_at(d: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([
        *d.get(i).unwrap_or(&0),
        *d.get(i + 1).unwrap_or(&0),
        *d.get(i + 2).unwrap_or(&0),
        *d.get(i + 3).unwrap_or(&0),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_rgba_sprite() {
        let mut s = Sprite::new(3, 2);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        s.set(0, 0, 2, 1, Rgba(0, 128, 255, 255));
        s.layers[0].name = "Art".into();
        s.layers[0].blend = BlendMode::Multiply;
        s.add_frame_after(0);
        s.set(0, 1, 1, 0, Rgba(0, 255, 0, 255));

        let path = std::env::temp_dir().join("vixel_ase_roundtrip.ase");
        save(&s, &path).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!((loaded.width, loaded.height), (3, 2));
        assert_eq!(loaded.layers.len(), 1);
        assert_eq!(loaded.frames.len(), 2);
        assert_eq!(loaded.layers[0].name, "Art");
        assert_eq!(loaded.layers[0].blend, BlendMode::Multiply);
        assert_eq!(loaded.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
        assert_eq!(loaded.get(0, 0, 2, 1), Rgba(0, 128, 255, 255));
        assert_eq!(loaded.get(0, 1, 1, 0), Rgba(0, 255, 0, 255));
    }

    #[test]
    fn round_trips_layer_groups_and_flags() {
        let mut s = Sprite::new(2, 1);
        s.set(0, 0, 1, 0, Rgba(10, 20, 30, 255));
        s.layers.insert(
            0,
            Layer {
                name: "Grp".into(),
                group: true,
                ..Default::default()
            },
        );
        s.cels.insert(0, vec![s.cels[0][0].clone()]);
        s.layers[1].child_level = 1;
        s.layers[1].editable = false;

        let path = std::env::temp_dir().join("vixel_ase_groups.ase");
        save(&s, &path).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.layers.len(), 2);
        assert!(loaded.layers[0].group);
        assert_eq!(loaded.layers[1].child_level, 1);
        assert!(!loaded.layers[1].editable);
        assert_eq!(loaded.get(1, 0, 1, 0), Rgba(10, 20, 30, 255));
    }
}
