//! `.ase` / `.aseprite` import/export.
//!
//! Reads raw (0) and zlib-compressed (2) cels, linked cels (1), and compressed
//! tilemap cels (3) with embedded tileset images; RGBA (depth 32), grayscale
//! (16) and indexed (8) sprites; layers, groups, cel opacity and frame
//! durations. Writes raw cels at an automatically chosen lossless depth:
//! grayscale when every pixel is gray, indexed when the sprite uses <= 255
//! colors, otherwise 32-bit RGBA.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::color::Rgba;
use crate::sprite::{
    AseProfile, BlendMode, Cel, ChunkAnchor, Frame, Layer, PlayDirection, RawChunk, Slice,
    SliceKey, Sprite, Tag, UserData,
};

const MAGIC: u16 = 0xA5E0;
const FRAME_MAGIC: u16 = 0xF1FA;

const CHUNK_LAYER: u16 = 0x2004;
const CHUNK_CEL: u16 = 0x2005;
const CHUNK_PALETTE: u16 = 0x2019;
const CHUNK_TAGS: u16 = 0x2018;
const CHUNK_PALETTE_RGB: u16 = 0x0004;
const CHUNK_PALETTE_RGBA: u16 = 0x0011;

// ---- export ----

/// Lossless output depth, chosen from what the sprite actually contains.
enum OutDepth {
    Rgba,
    Gray,
    /// 8-bit indices into `palette`; `transparent` is the header's transparent
    /// colour index and pixels are matched by RGB.
    Indexed {
        palette: Vec<Rgba>,
        transparent: u8,
    },
}

/// Pick the smallest depth that represents every pixel exactly. Grayscale wins
/// when it can (r == g == b), then indexed when the sprite uses <= 255 distinct
/// colors, else full RGBA.
fn out_depth(sprite: &Sprite) -> OutDepth {
    // A declared mode wins when it can be honoured; `Rgba` means "pick the
    // smallest lossless depth", which is the historical behaviour.
    match sprite.color_mode {
        crate::sprite::ColorMode::Auto => {}
        crate::sprite::ColorMode::Rgba => return OutDepth::Rgba,
        crate::sprite::ColorMode::Grayscale => return OutDepth::Gray,
        crate::sprite::ColorMode::Indexed => {
            let (mut palette, mut transparent) = if sprite.palette.is_empty() {
                let mut colors: Vec<Rgba> = vec![Rgba::TRANSPARENT];
                for p in sprite.cels.iter().flatten().flat_map(|c| &c.pixels) {
                    if p.3 == 0 {
                        continue;
                    }
                    let c = Rgba(p.0, p.1, p.2, 255);
                    if !colors.contains(&c) {
                        if colors.len() == 256 {
                            break;
                        }
                        colors.push(c);
                    }
                }
                (colors, 0)
            } else {
                (
                    sprite.palette.iter().copied().take(256).collect(),
                    sprite.transparent_slot(),
                )
            };
            if palette.get(transparent as usize).is_none_or(|p| p.3 != 0) {
                if let Some(i) = palette.iter().position(|p| p.3 == 0) {
                    transparent = i as u8;
                } else if palette.len() < 256 {
                    palette.push(Rgba::TRANSPARENT);
                    transparent = (palette.len() - 1) as u8;
                } else {
                    palette[255] = Rgba::TRANSPARENT;
                    transparent = 255;
                }
            }
            return OutDepth::Indexed {
                palette,
                transparent,
            };
        }
    }
    auto_depth(sprite)
}

fn auto_depth(sprite: &Sprite) -> OutDepth {
    let mut all_gray = true;
    let mut colors: Vec<Rgba> = vec![Rgba::TRANSPARENT];
    for p in sprite.cels.iter().flatten().flat_map(|c| &c.pixels) {
        if p.0 != p.1 || p.1 != p.2 {
            all_gray = false;
        }
        if p.3 == 0 {
            continue;
        }
        if !colors.contains(p) {
            if colors.len() == 256 {
                return OutDepth::Rgba;
            }
            colors.push(*p);
        }
    }
    if all_gray {
        OutDepth::Gray
    } else {
        OutDepth::Indexed {
            palette: colors,
            transparent: 0,
        }
    }
}

pub fn save(sprite: &Sprite, path: &Path) -> Result<()> {
    let depth = out_depth(sprite);
    let mut out: Vec<u8> = Vec::with_capacity(128 + sprite.area() * 4 + 256);
    write_header(&mut out, sprite, &depth);
    for frame in 0..sprite.frames.len() {
        write_frame(&mut out, sprite, frame, &depth);
    }
    let size = out.len() as u32;
    out[0..4].copy_from_slice(&size.to_le_bytes());
    std::fs::write(path, &out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_header(out: &mut Vec<u8>, s: &Sprite, depth: &OutDepth) {
    let mut h = vec![0u8; 128];
    h[0..4].copy_from_slice(&0u32.to_le_bytes()); // size (backfilled)
    h[4..6].copy_from_slice(&MAGIC.to_le_bytes());
    h[6..8].copy_from_slice(&(s.frames.len() as u16).to_le_bytes());
    h[8..10].copy_from_slice(&(s.width as u16).to_le_bytes());
    h[10..12].copy_from_slice(&(s.height as u16).to_le_bytes());
    let bits: u16 = match depth {
        OutDepth::Rgba => 32,
        OutDepth::Gray => 16,
        OutDepth::Indexed { .. } => 8,
    };
    h[12..14].copy_from_slice(&bits.to_le_bytes());
    let mut flags: u32 = 1; // layer opacity is valid
    if s.layers.iter().any(|l| l.group) {
        flags |= 2; // group blend/opacity valid
    }
    if s.layers.iter().any(|l| l.uuid.is_some()) {
        flags |= 4; // layers carry UUIDs
    }
    h[14..18].copy_from_slice(&flags.to_le_bytes());
    h[18..20].copy_from_slice(&0u16.to_le_bytes()); // speed (deprecated)
    if let OutDepth::Indexed {
        palette,
        transparent,
    } = depth
    {
        h[28] = *transparent;
        h[32..34].copy_from_slice(&(palette.len() as u16).to_le_bytes());
    }
    h[34] = s.pixel_aspect.0.clamp(1, 255) as u8; // pixel width
    h[35] = s.pixel_aspect.1.clamp(1, 255) as u8; // pixel height
    let gx = s.grid_origin.0.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    let gy = s.grid_origin.1.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    h[36..38].copy_from_slice(&gx.to_le_bytes());
    h[38..40].copy_from_slice(&gy.to_le_bytes());
    h[40..42].copy_from_slice(&(s.grid.0.min(u16::MAX as u32) as u16).to_le_bytes());
    h[42..44].copy_from_slice(&(s.grid.1.min(u16::MAX as u32) as u16).to_le_bytes());
    out.extend_from_slice(&h);
}

fn write_frame(out: &mut Vec<u8>, s: &Sprite, frame: usize, depth: &OutDepth) {
    let mut body: Vec<u8> = Vec::new();
    let mut count: u32 = 0;
    let mut used = vec![false; s.raw_chunks.len()];
    let mut emit_raw = |body: &mut Vec<u8>, count: &mut u32, anchor: ChunkAnchor| {
        for (i, rc) in s.raw_chunks.iter().enumerate() {
            if !used[i] && rc.frame == frame && rc.after == anchor {
                write_chunk(body, rc.kind, &rc.body);
                used[i] = true;
                *count += 1;
            }
        }
    };

    emit_raw(&mut body, &mut count, ChunkAnchor::FrameStart);
    if frame == 0 {
        // Aseprite always writes a profile chunk. An imported one wins; else
        // describe the working space (linear sRGB is fixed gamma 1.0).
        let fallback;
        let profile = match &s.ase_profile {
            Some(p) => p,
            None => {
                fallback = match s.color_profile {
                    crate::sprite::ColorProfile::Srgb => AseProfile {
                        kind: 1,
                        flags: 0,
                        gamma: 0.0,
                        icc: Vec::new(),
                    },
                    crate::sprite::ColorProfile::Linear => AseProfile {
                        kind: 1,
                        flags: 1,
                        gamma: 1.0,
                        icc: Vec::new(),
                    },
                };
                &fallback
            }
        };
        write_color_profile(&mut body, profile);
        count += 1;
    }
    if frame == 0 {
        if let OutDepth::Indexed { palette, .. } = depth {
            // Names describe the document palette; an auto-generated palette
            // has its own order and no names.
            let names: &[String] = if s.is_indexed() {
                &s.palette_names
            } else {
                &[]
            };
            write_palette_chunk(&mut body, palette, names);
            count += 1;
            emit_raw(&mut body, &mut count, ChunkAnchor::Palette);
        }
        if let Some(ud) = &s.user_data {
            write_user_data(&mut body, ud, true);
            count += 1;
        }
        let has_uuid = s.layers.iter().any(|l| l.uuid.is_some());
        for (i, layer) in s.layers.iter().enumerate() {
            write_layer_chunk(&mut body, layer, has_uuid);
            count += 1;
            if let Some(ud) = &layer.user_data {
                write_user_data(&mut body, ud, true);
                count += 1;
            }
            emit_raw(&mut body, &mut count, ChunkAnchor::Layer(i as u16));
        }
        if !s.tags.is_empty() {
            write_tags_chunk(&mut body, &s.tags);
            count += 1;
            let empty = UserData::default();
            for tag in &s.tags {
                write_user_data(&mut body, tag.user_data.as_ref().unwrap_or(&empty), true);
                count += 1;
            }
            emit_raw(&mut body, &mut count, ChunkAnchor::Tags);
        }
    }
    for li in 0..s.layers.len() {
        if s.layers[li].group {
            continue; // groups have no cels
        }
        if write_cel_chunk(&mut body, s, li, frame, depth) {
            count += 1;
            if let Some(ud) = &s.cels[li][frame].user_data {
                write_user_data(&mut body, ud, true);
                count += 1;
            }
            emit_raw(&mut body, &mut count, ChunkAnchor::Cel(li as u16));
        }
    }
    if frame == 0 {
        for slice in &s.slices {
            write_slice_chunk(&mut body, slice);
            count += 1;
            if let Some(ud) = &slice.user_data {
                write_user_data(&mut body, ud, true);
                count += 1;
            }
        }
    }
    // Anchors with no matching known chunk (an unwritten tileset, a deleted
    // layer, or user data on an empty cel) go to the end of the frame instead
    // of vanishing.
    for (i, rc) in s.raw_chunks.iter().enumerate() {
        if !used[i] && rc.frame == frame {
            write_chunk(&mut body, rc.kind, &rc.body);
            count += 1;
            used[i] = true;
        }
    }

    let mut f = Vec::new();
    f.extend_from_slice(&0u32.to_le_bytes()); // size (backfilled)
    f.extend_from_slice(&FRAME_MAGIC.to_le_bytes());
    let old_count = if count > u32::from(u16::MAX) {
        u16::MAX
    } else {
        count as u16
    };
    f.extend_from_slice(&old_count.to_le_bytes());
    f.extend_from_slice(&(s.frames[frame].duration_ms).to_le_bytes());
    f.extend_from_slice(&[0u8, 0]);
    f.extend_from_slice(&count.to_le_bytes());
    f.extend_from_slice(&body);
    let size = f.len() as u32;
    f[0..4].copy_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&f);
}

fn write_slice_chunk(out: &mut Vec<u8>, slice: &Slice) {
    let mut b = Vec::new();
    let flags = (slice.flags & !3) | u32::from(slice.nine_patch) | (u32::from(slice.pivot) << 1);
    b.extend_from_slice(&(slice.keys.len() as u32).to_le_bytes());
    b.extend_from_slice(&flags.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&(slice.name.len() as u16).to_le_bytes());
    b.extend_from_slice(slice.name.as_bytes());
    for key in &slice.keys {
        b.extend_from_slice(&key.frame.to_le_bytes());
        b.extend_from_slice(&key.x.to_le_bytes());
        b.extend_from_slice(&key.y.to_le_bytes());
        b.extend_from_slice(&key.w.to_le_bytes());
        b.extend_from_slice(&key.h.to_le_bytes());
        if flags & 1 != 0 {
            let (cx, cy, cw, ch) = key.center.unwrap_or((0, 0, 0, 0));
            b.extend_from_slice(&cx.to_le_bytes());
            b.extend_from_slice(&cy.to_le_bytes());
            b.extend_from_slice(&cw.to_le_bytes());
            b.extend_from_slice(&ch.to_le_bytes());
        }
        if flags & 2 != 0 {
            let (px, py) = key.pivot.unwrap_or((0, 0));
            b.extend_from_slice(&px.to_le_bytes());
            b.extend_from_slice(&py.to_le_bytes());
        }
    }
    write_chunk(out, CHUNK_SLICE, &b);
}

fn write_user_data(out: &mut Vec<u8>, data: &UserData, force: bool) {
    let mut flags = data.flags & !7;
    if !data.text.is_empty() {
        flags |= 1;
    }
    if data.color.is_some() {
        flags |= 2;
    }
    if !data.properties.is_empty() {
        flags |= 4;
    }
    if flags == 0 && !force {
        return;
    }
    let mut b = Vec::new();
    b.extend_from_slice(&flags.to_le_bytes());
    if flags & 1 != 0 {
        b.extend_from_slice(&(data.text.len() as u16).to_le_bytes());
        b.extend_from_slice(data.text.as_bytes());
    }
    if flags & 2 != 0 {
        let c = data.color.unwrap_or(Rgba::TRANSPARENT);
        b.extend_from_slice(&[c.0, c.1, c.2, c.3]);
    }
    if flags & 4 != 0 {
        b.extend_from_slice(&data.properties);
    }
    write_chunk(out, CHUNK_USER_DATA, &b);
}

fn write_color_profile(out: &mut Vec<u8>, profile: &AseProfile) {
    let mut b = Vec::new();
    b.extend_from_slice(&profile.kind.to_le_bytes());
    b.extend_from_slice(&profile.flags.to_le_bytes());
    let gamma = (profile.gamma.max(0.0) * 65536.0).round() as u32;
    b.extend_from_slice(&gamma.to_le_bytes());
    b.extend_from_slice(&[0u8; 8]);
    if profile.kind == 2 {
        b.extend_from_slice(&(profile.icc.len() as u32).to_le_bytes());
        b.extend_from_slice(&profile.icc);
    }
    write_chunk(out, CHUNK_PROFILE, &b);
}

fn write_palette_chunk(out: &mut Vec<u8>, palette: &[Rgba], names: &[String]) {
    let mut b = Vec::new();
    b.extend_from_slice(&(palette.len() as u32).to_le_bytes()); // new palette size
    b.extend_from_slice(&0u32.to_le_bytes()); // first color index
    b.extend_from_slice(&(palette.len().saturating_sub(1) as u32).to_le_bytes());
    b.extend_from_slice(&[0u8; 8]); // reserved
    for (i, c) in palette.iter().enumerate() {
        let name = names.get(i).map(String::as_str).unwrap_or("");
        if name.is_empty() {
            b.extend_from_slice(&0u16.to_le_bytes());
        } else {
            b.extend_from_slice(&1u16.to_le_bytes()); // has a name
        }
        b.extend_from_slice(&[c.0, c.1, c.2, c.3]);
        if !name.is_empty() {
            let units: Vec<u16> = name.encode_utf16().collect();
            b.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for u in units {
                b.extend_from_slice(&u.to_le_bytes());
            }
        }
    }
    write_chunk(out, CHUNK_PALETTE, &b);
}

fn write_layer_chunk(out: &mut Vec<u8>, layer: &Layer, has_uuid: bool) {
    let name = layer.name.as_bytes();
    let mut b = Vec::new();
    let mut flags: u16 = if layer.visible { 1 } else { 0 };
    if layer.editable {
        flags |= 2; // editable
    }
    if layer.lock_movement {
        flags |= 4;
    }
    if layer.background {
        flags |= 8; // background
    }
    if layer.reference {
        flags |= 64; // reference layer
    }
    if layer.prefer_linked {
        flags |= 16; // new frames prefer linked cels
    }
    if layer.collapsed {
        flags |= 32; // group displayed collapsed
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
    if has_uuid {
        b.extend_from_slice(&layer.uuid.unwrap_or([0u8; 16]));
    }
    write_chunk(out, CHUNK_LAYER, &b);
}

fn write_tags_chunk(out: &mut Vec<u8>, tags: &[Tag]) {
    let mut b = Vec::new();
    b.extend_from_slice(&(tags.len() as u16).to_le_bytes());
    b.extend_from_slice(&[0u8; 8]); // reserved
    for t in tags {
        b.extend_from_slice(&(t.from as u16).to_le_bytes());
        b.extend_from_slice(&(t.to as u16).to_le_bytes());
        b.push(t.direction.to_ase());
        b.extend_from_slice(&t.repeat.to_le_bytes());
        b.extend_from_slice(&[0u8; 6]); // reserved
        b.extend_from_slice(&[t.color.0, t.color.1, t.color.2]);
        b.push(0); // extra byte
        let name = t.name.as_bytes();
        b.extend_from_slice(&(name.len() as u16).to_le_bytes());
        b.extend_from_slice(name);
    }
    write_chunk(out, CHUNK_TAGS, &b);
}

fn write_cel_chunk(
    out: &mut Vec<u8>,
    s: &Sprite,
    li: usize,
    frame: usize,
    depth: &OutDepth,
) -> bool {
    let cel = &s.cels[li][frame];
    let link = cel.link.filter(|&t| t < s.frames.len() && t != frame);
    if link.is_none() && cel.pixels.iter().all(|p| p.3 == 0) {
        return false; // empty cels are implicit in .ase
    }
    let mut b = Vec::new();
    b.extend_from_slice(&(li as u16).to_le_bytes());
    b.extend_from_slice(&(cel.x.clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes());
    b.extend_from_slice(&(cel.y.clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes());
    b.push(cel.opacity);
    if let Some(target) = link {
        b.extend_from_slice(&1u16.to_le_bytes()); // cel type: linked
        b.extend_from_slice(&cel.z_index.to_le_bytes());
        b.extend_from_slice(&[0u8; 5]);
        b.extend_from_slice(&(target as u16).to_le_bytes());
    } else {
        b.extend_from_slice(&0u16.to_le_bytes()); // cel type: raw
        b.extend_from_slice(&cel.z_index.to_le_bytes());
        b.extend_from_slice(&[0u8; 5]);
        b.extend_from_slice(&(cel.w.min(u16::MAX as u32) as u16).to_le_bytes());
        b.extend_from_slice(&(cel.h.min(u16::MAX as u32) as u16).to_le_bytes());
        match depth {
            OutDepth::Rgba => {
                for p in &cel.pixels {
                    b.extend_from_slice(&[p.0, p.1, p.2, p.3]);
                }
            }
            OutDepth::Gray => {
                for p in &cel.pixels {
                    b.extend_from_slice(&[p.0, p.3]);
                }
            }
            OutDepth::Indexed {
                palette,
                transparent,
            } => {
                for p in &cel.pixels {
                    let idx = if p.3 == 0 {
                        *transparent
                    } else {
                        palette
                            .iter()
                            .position(|c| c == p)
                            .map(|i| i as u8)
                            .unwrap_or_else(|| {
                                crate::sprite::nearest_palette_index(palette, *transparent, *p)
                            })
                    };
                    b.push(idx);
                }
            }
        }
    }
    write_chunk(out, CHUNK_CEL, &b);
    true
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: u16, body: &[u8]) {
    // The chunk size includes this 6-byte header (size + type).
    out.extend_from_slice(&((body.len() + 6) as u32).to_le_bytes());
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
        BlendMode::ColorDodge => 6,
        BlendMode::ColorBurn => 7,
        BlendMode::HardLight => 8,
        BlendMode::SoftLight => 9,
        BlendMode::Difference => 10,
        BlendMode::Exclusion => 11,
        BlendMode::Hue => 12,
        BlendMode::Saturation => 13,
        BlendMode::Color => 14,
        BlendMode::Luminosity => 15,
        BlendMode::Add => 16,
        BlendMode::Subtract => 17,
        BlendMode::Divide => 18,
    }
}

// ---- import ----

const CHUNK_PROFILE: u16 = 0x2007;
const CHUNK_USER_DATA: u16 = 0x2020;
const CHUNK_SLICE: u16 = 0x2022;
const CHUNK_TILESET: u16 = 0x2023;

fn parse_slice(body: &[u8]) -> Option<Slice> {
    if body.len() < 14 {
        return None;
    }
    let count = u32_at(body, 0) as usize;
    let flags = u32_at(body, 4);
    let name_len = u16_at(body, 12) as usize;
    let name = String::from_utf8_lossy(body.get(14..14 + name_len)?).into_owned();
    let mut p = 14 + name_len;
    let mut keys = Vec::with_capacity(count);
    for _ in 0..count {
        let key = body.get(p..p + 20)?;
        let frame = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
        let x = i32::from_le_bytes([key[4], key[5], key[6], key[7]]);
        let y = i32::from_le_bytes([key[8], key[9], key[10], key[11]]);
        let w = u32::from_le_bytes([key[12], key[13], key[14], key[15]]);
        let h = u32::from_le_bytes([key[16], key[17], key[18], key[19]]);
        p += 20;
        let center = if flags & 1 != 0 {
            let c = body.get(p..p + 16)?;
            p += 16;
            Some((
                i32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                i32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                u32::from_le_bytes([c[8], c[9], c[10], c[11]]),
                u32::from_le_bytes([c[12], c[13], c[14], c[15]]),
            ))
        } else {
            None
        };
        let pivot = if flags & 2 != 0 {
            let c = body.get(p..p + 8)?;
            p += 8;
            Some((
                i32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                i32::from_le_bytes([c[4], c[5], c[6], c[7]]),
            ))
        } else {
            None
        };
        keys.push(SliceKey {
            frame,
            x,
            y,
            w,
            h,
            center,
            pivot,
        });
    }
    Some(Slice {
        name,
        flags,
        nine_patch: flags & 1 != 0,
        pivot: flags & 2 != 0,
        keys,
        user_data: None,
    })
}

fn parse_user_data(body: &[u8]) -> Option<UserData> {
    if body.len() < 4 {
        return None;
    }
    let flags = u32_at(body, 0);
    let mut p = 4usize;
    let text = if flags & 1 != 0 {
        let len = u16_at(body, p) as usize;
        p += 2;
        let bytes = body.get(p..p + len)?;
        p += len;
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        String::new()
    };
    let color = if flags & 2 != 0 {
        let c = body.get(p..p + 4)?;
        p += 4;
        Some(Rgba(c[0], c[1], c[2], c[3]))
    } else {
        None
    };
    let properties = if flags & 4 != 0 {
        body.get(p..)?.to_vec()
    } else {
        Vec::new()
    };
    Some(UserData {
        text,
        color,
        properties,
        flags,
    })
}

fn parse_color_profile(body: &[u8]) -> Option<AseProfile> {
    if body.len() < 16 {
        return None;
    }
    let kind = u16_at(body, 0);
    if kind > 2 {
        return None;
    }
    let flags = u16_at(body, 2);
    let gamma = u32_at(body, 4) as f32 / 65536.0;
    let mut icc = Vec::new();
    if kind == 2 {
        let len = u32_at(body, 16) as usize;
        icc = body.get(20..20 + len).unwrap_or(&[]).to_vec();
    }
    Some(AseProfile {
        kind,
        flags,
        gamma,
        icc,
    })
}

struct Tileset {
    num_tiles: u32,
    tile_w: u32,
    tile_h: u32,
    /// Row-major, `tile_w` wide and `tile_h * num_tiles` tall.
    image: Vec<Rgba>,
}

/// A tilemap cel, kept until every tileset chunk has been read.
struct TilemapCel {
    layer: usize,
    frame: usize,
    ox: i32,
    oy: i32,
    tiles_w: u32,
    /// Tile id + (flip x, flip y, flip diagonal).
    tiles: Vec<(u32, bool, bool, bool)>,
    opacity: u8,
    z_index: i16,
    user_data: Option<UserData>,
}

struct CelInfo {
    opacity: u8,
    z_index: i16,
    user_data: Option<UserData>,
}

#[derive(Clone, Copy)]
enum Last {
    None,
    Layer(usize),
    Cel(usize),
    Tags(usize, usize),
    Slice(usize),
    Palette,
    Other,
}

struct Import {
    depth: u16,
    frames: usize,
    palette: Vec<Rgba>,
    palette_names: Vec<String>,
    ase_profile: Option<AseProfile>,
    sprite_user_data: Option<UserData>,
    slices: Vec<Slice>,
    transparent_index: u8,
    /// `[layer][frame]`; `None` means an empty cel.
    cels: Vec<Vec<Option<Cel>>>,
    layer_tilesets: Vec<u32>,
    tilesets: Vec<Tileset>,
    tilemaps: Vec<TilemapCel>,
    tags: Vec<Tag>,
    raw_chunks: Vec<RawChunk>,
}

impl Import {
    fn decode_cel(&mut self, frame: usize, body: &[u8], user_data: Option<UserData>) -> Result<()> {
        if body.len() < 16 || frame >= self.frames {
            return Ok(());
        }
        let li = u16_at(body, 0) as usize;
        if li >= self.cels.len() {
            return Ok(());
        }
        let ox = i16_at(body, 2) as i32;
        let oy = i16_at(body, 4) as i32;
        let opacity = *body.get(6).unwrap_or(&255);
        let z_index = i16_at(body, 9);
        match u16_at(body, 7) {
            0 => {
                let cw = u16_at(body, 16) as u32;
                let ch = u16_at(body, 18) as u32;
                let px = decode_pixels(
                    &body[20..],
                    self.depth,
                    cw,
                    ch,
                    &self.palette,
                    self.transparent_index,
                )
                .2;
                self.place(
                    (li, frame),
                    (ox, oy),
                    (cw, ch),
                    &px,
                    CelInfo {
                        opacity,
                        z_index,
                        user_data,
                    },
                );
            }
            1 => {
                // Linked cel: keep the reference; content resolves on read and
                // painting copies it into this frame (copy-on-write).
                let link = u16_at(body, 16) as usize;
                let mut cel = Cel::blank(0, 0);
                cel.link = Some(link);
                cel.opacity = opacity;
                cel.z_index = z_index;
                cel.user_data = user_data;
                self.cels[li][frame] = Some(cel);
            }
            2 => {
                let cw = u16_at(body, 16) as u32;
                let ch = u16_at(body, 18) as u32;
                let mut raw = Vec::new();
                flate2::read::ZlibDecoder::new(&body[20..]).read_to_end(&mut raw)?;
                let px = decode_pixels(
                    &raw,
                    self.depth,
                    cw,
                    ch,
                    &self.palette,
                    self.transparent_index,
                )
                .2;
                self.place(
                    (li, frame),
                    (ox, oy),
                    (cw, ch),
                    &px,
                    CelInfo {
                        opacity,
                        z_index,
                        user_data,
                    },
                );
            }
            3 => {
                let w = u16_at(body, 16) as u32;
                let h = u16_at(body, 18) as u32;
                let bits = u16_at(body, 20) as usize;
                let id_mask = u32_at(body, 22);
                let x_mask = u32_at(body, 26);
                let y_mask = u32_at(body, 30);
                let d_mask = u32_at(body, 34);
                let mut raw = Vec::new();
                flate2::read::ZlibDecoder::new(body.get(48..).unwrap_or(&[]))
                    .read_to_end(&mut raw)?;
                let per = (bits / 8).max(1);
                let mut tiles = Vec::with_capacity((w * h) as usize);
                for i in 0..(w * h) as usize {
                    let off = i * per;
                    let v = match per {
                        1 => *raw.get(off).unwrap_or(&0) as u32,
                        2 => u16_at(&raw, off) as u32,
                        _ => u32_at(&raw, off),
                    };
                    tiles.push((
                        v & id_mask,
                        v & x_mask != 0,
                        v & y_mask != 0,
                        v & d_mask != 0,
                    ));
                }
                self.tilemaps.push(TilemapCel {
                    layer: li,
                    frame,
                    ox,
                    oy,
                    tiles_w: w,
                    tiles,
                    opacity,
                    z_index,
                    user_data,
                });
            }
            _ => {}
        }
        Ok(())
    }

    /// Keep the cel's real bounds instead of baking it into a canvas buffer.
    fn place(
        &mut self,
        (li, frame): (usize, usize),
        (ox, oy): (i32, i32),
        (cw, ch): (u32, u32),
        px: &[Rgba],
        info: CelInfo,
    ) {
        let mut pixels = px.to_vec();
        pixels.resize((cw * ch) as usize, Rgba::TRANSPARENT);
        self.cels[li][frame] = Some(Cel {
            x: ox,
            y: oy,
            w: cw,
            h: ch,
            z_index: info.z_index,
            link: None,
            pixels,
            opacity: info.opacity,
            user_data: info.user_data,
        });
    }

    fn resolve_tilemaps(&mut self) {
        for tc in std::mem::take(&mut self.tilemaps) {
            let ts_index = self.layer_tilesets.get(tc.layer).copied().unwrap_or(0) as usize;
            let Some(ts) = self.tilesets.get(ts_index) else {
                continue;
            };
            if ts.image.is_empty() || ts.tile_w == 0 || ts.tile_h == 0 {
                continue;
            }
            let tiles_w = tc.tiles_w.max(1);
            let tiles_h = (tc.tiles.len() as u32).div_ceil(tiles_w);
            let (tw, th) = (tiles_w * ts.tile_w, tiles_h * ts.tile_h);
            let mut pixels = vec![Rgba::TRANSPARENT; (tw * th) as usize];
            for (i, &(id, fx, fy, fd)) in tc.tiles.iter().enumerate() {
                if id == 0 || id >= ts.num_tiles {
                    continue;
                }
                let tx = (i as u32 % tiles_w) * ts.tile_w;
                let ty = (i as u32 / tiles_w) * ts.tile_h;
                for dy in 0..ts.tile_h {
                    for dx in 0..ts.tile_w {
                        let (mut sx, mut sy) = (dx, dy);
                        if fx {
                            sx = ts.tile_w - 1 - sx;
                        }
                        if fy {
                            sy = ts.tile_h - 1 - sy;
                        }
                        if fd {
                            (sx, sy) = (sy, sx);
                        }
                        let idx = ((id * ts.tile_h + sy) * ts.tile_w + sx) as usize;
                        let src = *ts.image.get(idx).unwrap_or(&Rgba::TRANSPARENT);
                        let (x, y) = (tx + dx, ty + dy);
                        pixels[(y * tw + x) as usize] = src;
                    }
                }
            }
            if tc.frame < self.frames && tc.layer < self.cels.len() {
                self.cels[tc.layer][tc.frame] = Some(Cel {
                    x: tc.ox,
                    y: tc.oy,
                    w: tw,
                    h: th,
                    z_index: tc.z_index,
                    link: None,
                    pixels,
                    opacity: tc.opacity,
                    user_data: tc.user_data,
                });
            }
        }
    }
}

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
    // Pixel ratio: two bytes, 0 meaning "not set".
    let pixel_aspect = match (data[34], data[35]) {
        (0, _) | (_, 0) => None,
        (x, y) => Some((x as u16, y as u16)),
    };
    let grid_origin = (i16_at(&data, 36) as i32, i16_at(&data, 38) as i32);
    let grid = (u16_at(&data, 40) as u32, u16_at(&data, 42) as u32);

    let mut im = Import {
        depth,
        frames: frames_count.max(1),
        // Undeclared indexed slots are opaque black, matching Aseprite's
        // padded palette; the transparent index overrides its own slot.
        palette: vec![Rgba(0, 0, 0, 255); 256],
        palette_names: vec![String::new(); 256],
        ase_profile: None,
        sprite_user_data: None,
        slices: Vec::new(),
        transparent_index: data[28],
        cels: Vec::new(),
        layer_tilesets: Vec::new(),
        tilesets: Vec::new(),
        tilemaps: Vec::new(),
        tags: Vec::new(),
        raw_chunks: Vec::new(),
    };
    let has_uuid = u32_at(&data, 14) & 4 != 0;
    let mut layers: Vec<Layer> = Vec::new();
    let mut durations: Vec<u16> = Vec::new();
    // Cel bodies are decoded after the whole file is read so the palette and
    // tilesets are complete regardless of chunk order.
    let mut cel_bodies: Vec<(usize, Vec<u8>, Option<UserData>)> = Vec::new();

    let mut pos = 128usize;
    for frame in 0..frames_count {
        let frame_size = u32_at(&data, pos) as usize;
        if pos + frame_size > data.len() {
            bail!("frame {frame} extends past end of file");
        }
        let frame_end = pos + frame_size;
        durations.push(u16_at(&data, pos + 6));
        pos += 16;
        let mut anchor = ChunkAnchor::FrameStart;
        let mut last = Last::None;

        while pos + 6 <= frame_end {
            let chunk_size = u32_at(&data, pos) as usize; // includes the header
            if chunk_size < 6 {
                break;
            }
            let chunk_type = u16_at(&data, pos + 4);
            let Some(body) = data.get(pos + 6..pos + chunk_size) else {
                break;
            };
            match chunk_type {
                CHUNK_LAYER => {
                    let li = layers.len() as u16;
                    let (layer, tileset) = parse_layer(body, has_uuid)?;
                    layers.push(layer);
                    im.cels.push(vec![None; im.frames]);
                    im.layer_tilesets.push(tileset);
                    anchor = ChunkAnchor::Layer(li);
                    last = Last::Layer(layers.len() - 1);
                }
                CHUNK_CEL => {
                    let li = u16_at(body, 0);
                    cel_bodies.push((frame, body.to_vec(), None));
                    anchor = ChunkAnchor::Cel(li);
                    last = Last::Cel(cel_bodies.len() - 1);
                }
                CHUNK_TAGS => {
                    let base = im.tags.len();
                    im.tags.extend(parse_tags(body));
                    anchor = ChunkAnchor::Tags;
                    last = Last::Tags(base, 0);
                }
                CHUNK_TILESET => {
                    let ti = im.tilesets.len() as u16;
                    im.tilesets.push(parse_tileset(
                        body,
                        depth,
                        &im.palette,
                        im.transparent_index,
                    )?);
                    anchor = ChunkAnchor::Tileset(ti);
                    last = Last::Other;
                }
                CHUNK_SLICE => {
                    if let Some(slice) = parse_slice(body) {
                        im.slices.push(slice);
                        last = Last::Slice(im.slices.len() - 1);
                    } else {
                        im.raw_chunks.push(RawChunk {
                            frame,
                            kind: chunk_type,
                            body: body.to_vec(),
                            after: anchor,
                        });
                    }
                }
                CHUNK_PROFILE => {
                    match parse_color_profile(body) {
                        Some(profile) => im.ase_profile = Some(profile),
                        None => im.raw_chunks.push(RawChunk {
                            frame,
                            kind: chunk_type,
                            body: body.to_vec(),
                            after: anchor,
                        }),
                    }
                    last = Last::Other;
                }
                CHUNK_USER_DATA => {
                    let mut handled = false;
                    if let Some(ud) = parse_user_data(body) {
                        match last {
                            Last::Layer(i) => {
                                if let Some(layer) = layers.get_mut(i) {
                                    layer.user_data = Some(ud);
                                    handled = true;
                                }
                            }
                            Last::Cel(i) => {
                                if let Some(slot) = cel_bodies.get_mut(i) {
                                    slot.2 = Some(ud);
                                    handled = true;
                                }
                            }
                            Last::Tags(base, k) => {
                                if let Some(tag) = im.tags.get_mut(base + k) {
                                    if let Some(c) = ud.color {
                                        tag.color = c;
                                    }
                                    tag.user_data = Some(ud);
                                    last = Last::Tags(base, k + 1);
                                    handled = true;
                                }
                            }
                            Last::Slice(i) => {
                                if let Some(slice) = im.slices.get_mut(i) {
                                    slice.user_data = Some(ud);
                                    handled = true;
                                }
                            }
                            Last::Palette | Last::Other
                                if frame == 0
                                    && im.sprite_user_data.is_none()
                                    && layers.is_empty()
                                    && cel_bodies.is_empty()
                                    && im.tags.is_empty() =>
                            {
                                im.sprite_user_data = Some(ud);
                                handled = true;
                            }
                            _ => {}
                        }
                    }
                    if handled {
                        if !matches!(last, Last::Tags(..)) {
                            last = Last::Other;
                        }
                    } else {
                        im.raw_chunks.push(RawChunk {
                            frame,
                            kind: chunk_type,
                            body: body.to_vec(),
                            after: anchor,
                        });
                        last = Last::Other;
                    }
                }
                CHUNK_PALETTE => {
                    parse_palette_new(body, &mut im.palette, &mut im.palette_names)?;
                    anchor = ChunkAnchor::Palette;
                    last = Last::Palette;
                }
                CHUNK_PALETTE_RGB => {
                    parse_palette_rgb(body, &mut im.palette, false);
                    anchor = ChunkAnchor::Palette;
                    last = Last::Palette;
                }
                CHUNK_PALETTE_RGBA => {
                    parse_palette_rgb(body, &mut im.palette, true);
                    anchor = ChunkAnchor::Palette;
                    last = Last::Palette;
                }
                _ => {
                    im.raw_chunks.push(RawChunk {
                        frame,
                        kind: chunk_type,
                        body: body.to_vec(),
                        after: anchor,
                    });
                    last = Last::Other;
                }
            }
            pos += chunk_size;
        }
        pos = frame_end;
    }

    for (frame, body, user_data) in cel_bodies {
        im.decode_cel(frame, &body, user_data)?;
    }
    im.resolve_tilemaps();

    if layers.is_empty() {
        layers.push(Layer {
            name: "Layer 1".into(),
            user_data: None,
            ..Default::default()
        });
        im.cels.push(vec![None; im.frames]);
    }

    let frames: Vec<Frame> = durations
        .into_iter()
        .map(|d| Frame {
            duration_ms: d.max(1),
        })
        .collect();

    let cels: Vec<Vec<Cel>> = (0..layers.len())
        .map(|li| {
            (0..im.frames)
                .map(|fi| {
                    im.cels
                        .get(li)
                        .and_then(|l| l.get(fi))
                        .and_then(|c| c.clone())
                        .unwrap_or_else(|| Cel::blank(width, height))
                })
                .collect()
        })
        .collect();

    // Header byte 32 is the palette size (0 means 256 for indexed files). The
    // transparent index entry is transparent regardless of its stored RGB.
    let palette_size = u16_at(&data, 32) as usize;
    let palette_len = if depth == 8 {
        256
    } else {
        palette_size.min(256)
    };
    let mut palette: Vec<Rgba> = im.palette[..palette_len.min(im.palette.len())].to_vec();
    if depth == 8
        && let Some(slot) = palette.get_mut(im.transparent_index as usize)
    {
        *slot = Rgba::TRANSPARENT;
    }

    // Linear sRGB is sRGB with fixed gamma 1.0; an embedded ICC cannot be
    // interpreted here, so those pixels stay in the sRGB working space as-is.
    let color_profile = match &im.ase_profile {
        Some(p) if p.kind == 1 && p.flags & 1 != 0 && (p.gamma - 1.0).abs() < 0.001 => {
            crate::sprite::ColorProfile::Linear
        }
        _ => crate::sprite::ColorProfile::Srgb,
    };
    let mut sprite = Sprite {
        width,
        height,
        layers,
        frames,
        cels,
        tags: im.tags,
        palette,
        palette_names: im.palette_names[..palette_len.min(im.palette_names.len())].to_vec(),
        transparent_index: im.transparent_index,
        color_mode: match depth {
            32 => crate::sprite::ColorMode::Rgba,
            8 => crate::sprite::ColorMode::Indexed,
            _ => crate::sprite::ColorMode::Grayscale,
        },
        pixel_aspect: pixel_aspect.unwrap_or((1, 1)),
        grid,
        grid_origin,
        color_profile,
        ase_profile: im.ase_profile,
        raw_chunks: im.raw_chunks,
        user_data: im.sprite_user_data,
        slices: im.slices,
    };
    sprite.normalize_levels();
    sprite.clamp_tags();
    Ok(sprite)
}

fn parse_tags(body: &[u8]) -> Vec<Tag> {
    let count = u16_at(body, 0) as usize;
    let mut p = 10usize; // count (2) + reserved (8)
    let mut tags = Vec::new();
    for _ in 0..count {
        if p + 17 > body.len() {
            break;
        }
        let from = u16_at(body, p) as usize;
        let to = u16_at(body, p + 2) as usize;
        // p+4 direction, p+5..7 repeat, p+7..13 reserved, p+13..16 RGB,
        // p+16 extra byte, then a STRING name.
        let direction = PlayDirection::from_ase(body[p + 4]);
        let repeat = u16_at(body, p + 5);
        let color = Rgba(body[p + 13], body[p + 14], body[p + 15], 255);
        let name_len = u16_at(body, p + 17) as usize;
        let name = body
            .get(p + 19..p + 19 + name_len)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();
        p += 19 + name_len;
        tags.push(Tag {
            name: if name.is_empty() { "Tag".into() } else { name },
            from,
            to,
            color,
            direction,
            repeat,
            user_data: None,
        });
    }
    tags
}

fn parse_tileset(body: &[u8], depth: u16, palette: &[Rgba], transparent: u8) -> Result<Tileset> {
    let flags = u32_at(body, 4);
    let num_tiles = u32_at(body, 8);
    let tile_w = u16_at(body, 12) as u32;
    let tile_h = u16_at(body, 14) as u32;
    let name_len = u16_at(body, 32) as usize;
    let mut p = 34 + name_len;
    if flags & 1 != 0 {
        p += 8; // external tileset file reference: pixels live elsewhere
    }
    let mut image = Vec::new();
    if flags & 2 != 0 {
        let len = u32_at(body, p) as usize;
        p += 4;
        if let Some(compressed) = body.get(p..p + len) {
            let mut raw = Vec::new();
            flate2::read::ZlibDecoder::new(compressed).read_to_end(&mut raw)?;
            image = decode_pixels(
                &raw,
                depth,
                tile_w,
                tile_h * num_tiles,
                palette,
                transparent,
            )
            .2;
        }
    }
    Ok(Tileset {
        num_tiles,
        tile_w,
        tile_h,
        image,
    })
}

fn parse_layer(body: &[u8], has_uuid: bool) -> Result<(Layer, u32)> {
    let flags = u16_at(body, 0);
    let kind = u16_at(body, 2);
    let group = kind == 1;
    let child_level = u16_at(body, 4);
    let blend = blend_from_word(u16_at(body, 10));
    let opacity = *body.get(12).unwrap_or(&255);
    let name_len = u16_at(body, 16) as usize;
    let name_bytes = &body.get(18..18 + name_len).unwrap_or(&[]);
    let name = String::from_utf8_lossy(name_bytes).into_owned();
    let mut end = 18 + name_len;
    let tileset = if kind == 2 {
        let t = u32_at(body, end);
        end += 4;
        t
    } else {
        0
    };
    let uuid = if has_uuid {
        body.get(end..end + 16).map(|b| {
            let mut u = [0u8; 16];
            u.copy_from_slice(b);
            u
        })
    } else {
        None
    };
    Ok((
        Layer {
            name: if name.is_empty() {
                "Layer".into()
            } else {
                name
            },
            visible: flags & 1 != 0,
            opacity,
            blend,
            child_level,
            group,
            editable: flags & 2 != 0,
            mask: None,
            background: flags & 8 != 0,
            reference: flags & 64 != 0,
            prefer_linked: flags & 16 != 0,
            lock_movement: flags & 4 != 0,
            collapsed: flags & 32 != 0,
            uuid,
            user_data: None,
        },
        tileset,
    ))
}

fn blend_from_word(w: u16) -> BlendMode {
    match w {
        1 => BlendMode::Multiply,
        2 => BlendMode::Screen,
        3 => BlendMode::Overlay,
        4 => BlendMode::Darken,
        5 => BlendMode::Lighten,
        6 => BlendMode::ColorDodge,
        7 => BlendMode::ColorBurn,
        8 => BlendMode::HardLight,
        9 => BlendMode::SoftLight,
        10 => BlendMode::Difference,
        11 => BlendMode::Exclusion,
        12 => BlendMode::Hue,
        13 => BlendMode::Saturation,
        14 => BlendMode::Color,
        15 => BlendMode::Luminosity,
        16 => BlendMode::Add,
        17 => BlendMode::Subtract,
        18 => BlendMode::Divide,
        _ => BlendMode::Normal,
    }
}

/// Decode a `cw x ch` block of pixel bytes into `(cw, ch, pixels)`.
fn decode_pixels(
    raw: &[u8],
    depth: u16,
    cw: u32,
    ch: u32,
    palette: &[Rgba],
    transparent: u8,
) -> (u32, u32, Vec<Rgba>) {
    let mut out = Vec::with_capacity((cw * ch) as usize);
    let mut i = 0usize;
    for _ in 0..cw * ch {
        let px = match depth {
            8 => {
                let idx = *raw.get(i).unwrap_or(&0) as usize;
                i += 1;
                if idx == transparent as usize {
                    Rgba::TRANSPARENT
                } else {
                    palette[idx.min(255)]
                }
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

fn parse_palette_new(body: &[u8], palette: &mut [Rgba], names: &mut [String]) -> Result<()> {
    // DWORD new palette size, DWORD first index, DWORD last index, 8 reserved.
    let first = u32_at(body, 4) as usize;
    let last = u32_at(body, 8) as usize;
    let mut p = 20usize;
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
            let name_len = u16_at(body, p) as usize;
            p += 2;
            let end = p + name_len * 2;
            if let Some(raw) = body.get(p..end) {
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                if let Some(name) = names.get_mut(idx) {
                    *name = String::from_utf16_lossy(&units);
                }
            }
            p = end;
        }
    }
    Ok(())
}

fn parse_palette_rgb(body: &[u8], palette: &mut [Rgba], six_bit: bool) {
    // Legacy palette chunk: WORD packet count, then (skip, count, colors).
    let packets = u16_at(body, 0) as usize;
    let mut p = 2usize;
    let mut idx = 0usize;
    for _ in 0..packets {
        let skip = *body.get(p).unwrap_or(&0) as usize;
        let count = match *body.get(p + 1).unwrap_or(&0) as usize {
            0 => 256,
            n => n,
        };
        p += 2;
        idx += skip;
        for _ in 0..count {
            if p + 3 > body.len() {
                return;
            }
            let scale = |v: u8| if six_bit { v * 255 / 63 } else { v };
            if idx < palette.len() {
                palette[idx] = Rgba(scale(body[p]), scale(body[p + 1]), scale(body[p + 2]), 255);
            }
            idx += 1;
            p += 3;
        }
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
    fn color_mode_and_pixel_ratio_round_trip_through_ase() {
        use crate::sprite::ColorMode;
        let dir = std::env::temp_dir().join(format!("vixel-ase-mode-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        for mode in [ColorMode::Rgba, ColorMode::Grayscale, ColorMode::Indexed] {
            let mut s = Sprite::new(4, 4);
            for y in 0..4 {
                for x in 0..4 {
                    s.set(0, 0, x, y, Rgba::rgb(10 * x as u8, 20, 200));
                }
            }
            s.color_mode = mode;
            s.pixel_aspect = (2, 1);
            let path = dir.join(format!("{}.ase", mode.name()));
            save(&s, &path).expect("save");
            let back = self::super::load(&path).expect("load");
            assert_eq!(back.pixel_aspect, (2, 1), "{}", mode.name());
            assert_eq!(
                back.color_mode,
                mode,
                "{} came back as {}",
                mode.name(),
                back.color_mode.name()
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn linked_cels_write_type_one_and_round_trip() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        s.add_frame_after(0);
        assert!(s.link_cel(0, 1, 0));
        s.layers[0].prefer_linked = true;
        s.color_mode = crate::sprite::ColorMode::Rgba;

        let path = tmp("vixel_ase_linked_write.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(f, k, _)| *f == 1 && *k == CHUNK_CEL)
            .expect("linked cel")
            .2;
        assert_eq!(u16_at(&body, 7), 1, "cel type linked");
        assert_eq!(u16_at(&body, 16), 0, "link target frame");

        let mut back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(back.layers[0].prefer_linked);
        assert!(back.is_linked(0, 1));
        assert_eq!(back.get(0, 1, 0, 0), Rgba(255, 0, 0, 255));
        back.set(0, 1, 0, 0, Rgba(0, 0, 255, 255));
        assert!(!back.is_linked(0, 1), "painting copies-on-write");
        assert_eq!(back.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
        assert_eq!(back.get(0, 1, 0, 0), Rgba(0, 0, 255, 255));
    }

    #[test]
    fn cel_z_index_round_trips_through_ase() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        s.cels[0][0].z_index = -3;
        let path = tmp("vixel_ase_celz.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_CEL)
            .expect("cel chunk")
            .2;
        assert_eq!(i16_at(&body, 9), -3, "z-index field");
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.cels[0][0].z_index, -3);
    }

    #[test]
    fn offset_cels_keep_bounds_through_ase() {
        let mut s = Sprite::new(4, 4);
        let red = Rgba(200, 10, 10, 255);
        s.cels[0][0] = Cel {
            x: 1,
            y: 2,
            w: 2,
            h: 2,
            z_index: 0,
            link: None,
            pixels: vec![
                red,
                Rgba(0, 255, 0, 255),
                Rgba(0, 0, 255, 255),
                Rgba(255, 255, 0, 255),
            ],
            opacity: 255,
            user_data: None,
        };
        let composite = s.composite(0);
        let path = tmp("vixel_ase_bounds.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_CEL)
            .expect("cel chunk")
            .2;
        assert_eq!((i16_at(&body, 2), i16_at(&body, 4)), (1, 2));
        assert_eq!((u16_at(&body, 16), u16_at(&body, 18)), (2, 2));
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((back.cels[0][0].x, back.cels[0][0].y), (1, 2));
        assert_eq!((back.cels[0][0].w, back.cels[0][0].h), (2, 2));
        assert_eq!(back.composite(0), composite);
    }

    #[test]
    fn color_profile_chunk_round_trips() {
        let mut s = Sprite::new(1, 1);
        s.ase_profile = Some(AseProfile {
            kind: 2,
            flags: 0,
            gamma: 1.0,
            icc: vec![0, 1, 2, 3, 4, 5],
        });
        let path = tmp("vixel_ase_icc.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_PROFILE)
            .expect("profile chunk")
            .2;
        assert_eq!(u16_at(&body, 0), 2, "ICC type");
        assert_eq!(u16_at(&body, 2), 0, "flags");
        assert_eq!(u32_at(&body, 4), 65536, "gamma 1.0");
        assert_eq!(u32_at(&body, 16), 6, "ICC length");
        assert_eq!(&body[20..26], &[0, 1, 2, 3, 4, 5]);
        let back = load(&path).unwrap();
        assert_eq!(back.ase_profile, s.ase_profile);

        s.ase_profile = Some(AseProfile {
            kind: 1,
            flags: 1,
            gamma: 1.0,
            icc: Vec::new(),
        });
        save(&s, &path).unwrap();
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.ase_profile, s.ase_profile);
    }

    #[test]
    fn linear_profile_is_written_as_fixed_gamma() {
        let mut s = Sprite::new(1, 1);
        s.color_profile = crate::sprite::ColorProfile::Linear;
        let path = tmp("vixel_ase_linear.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_PROFILE)
            .expect("profile chunk")
            .2;
        assert_eq!(u16_at(&body, 0), 1);
        assert_eq!(u16_at(&body, 2) & 1, 1, "fixed gamma flag");
        assert_eq!(u32_at(&body, 4), 65536, "gamma 1.0");
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.color_profile, crate::sprite::ColorProfile::Linear);
        assert_eq!(back.ase_profile.map(|p| p.kind), Some(1));
    }

    #[test]
    fn icc_fixture_survives_open_save_open() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ase/prop.ase");
        let a = load(&src).unwrap();
        let profile = a.ase_profile.clone().expect("prop.ase has an ICC profile");
        assert_eq!(profile.kind, 2);
        assert_eq!(profile.icc.len(), 536);
        let out = tmp("vixel_ase_icc_fixture.ase");
        save(&a, &out).unwrap();
        let b = load(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(b.ase_profile, a.ase_profile);
    }

    #[test]
    fn header_grid_origin_and_size_round_trip() {
        let mut s = Sprite::new(2, 2);
        s.grid = (8, 6);
        s.grid_origin = (-4, 7);
        let path = tmp("vixel_ase_grid.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        assert_eq!(i16_at(&data, 36), -4);
        assert_eq!(i16_at(&data, 38), 7);
        assert_eq!(u16_at(&data, 40), 8);
        assert_eq!(u16_at(&data, 42), 6);
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.grid, (8, 6));
        assert_eq!(back.grid_origin, (-4, 7));
    }

    #[test]
    fn tag_direction_three_round_trips() {
        let mut s = Sprite::new(1, 1);
        s.tags.push(Tag {
            name: "back".into(),
            from: 0,
            to: 0,
            color: Rgba(1, 2, 3, 255),
            direction: PlayDirection::PingPongReverse,
            repeat: 0,
            user_data: None,
        });
        let path = tmp("vixel_ase_tag3.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_TAGS)
            .expect("tags chunk")
            .2;
        assert_eq!(body[14], 3, "raw direction byte");
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.tags[0].direction, PlayDirection::PingPongReverse);
    }

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
                user_data: None,
                ..Default::default()
            },
        );
        s.cels.insert(0, vec![s.cels[0][0].clone()]);
        s.layers[1].child_level = 1;
        s.layers[1].editable = false;
        s.layers[1].background = true;
        s.layers[1].reference = true;

        let path = std::env::temp_dir().join("vixel_ase_groups.ase");
        save(&s, &path).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.layers.len(), 2);
        assert!(loaded.layers[0].group);
        assert_eq!(loaded.layers[1].child_level, 1);
        assert!(!loaded.layers[1].editable);
        assert!(loaded.layers[1].background);
        assert!(loaded.layers[1].reference);
        assert_eq!(loaded.get(1, 0, 1, 0), Rgba(10, 20, 30, 255));
    }

    // ---- depth selection + round trips ----

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(name)
    }

    fn saved_depth(sprite: &Sprite, name: &str) -> (Sprite, u16) {
        let path = tmp(name);
        save(sprite, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        (loaded, u16_at(&data, 12))
    }

    #[test]
    fn picks_grayscale_depth_losslessly() {
        let mut s = Sprite::new(2, 1);
        s.set(0, 0, 0, 0, Rgba(10, 10, 10, 255));
        s.set(0, 0, 1, 0, Rgba(200, 200, 200, 128));
        let (loaded, depth) = saved_depth(&s, "vixel_ase_gray.ase");
        assert_eq!(depth, 16);
        assert_eq!(loaded.get(0, 0, 0, 0), Rgba(10, 10, 10, 255));
        assert_eq!(loaded.get(0, 0, 1, 0), Rgba(200, 200, 200, 128));
    }

    #[test]
    fn picks_indexed_depth_and_keeps_alpha() {
        let mut s = Sprite::new(3, 1);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        s.set(0, 0, 1, 0, Rgba(0, 0, 255, 128));
        s.set(0, 0, 2, 0, Rgba(0, 0, 0, 0));
        let (loaded, depth) = saved_depth(&s, "vixel_ase_indexed.ase");
        assert_eq!(depth, 8);
        assert_eq!(loaded.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
        assert_eq!(loaded.get(0, 0, 1, 0), Rgba(0, 0, 255, 128));
        assert_eq!(loaded.get(0, 0, 2, 0), Rgba::TRANSPARENT);
    }

    #[test]
    fn falls_back_to_rgba_above_255_colors() {
        let mut s = Sprite::new(300, 1);
        for x in 0..300u32 {
            s.set(0, 0, x, 0, Rgba((x % 256) as u8, (x / 256) as u8, 7, 255));
        }
        let (loaded, depth) = saved_depth(&s, "vixel_ase_rgba.ase");
        assert_eq!(depth, 32);
        assert_eq!(loaded.get(0, 0, 299, 0), Rgba(43, 1, 7, 255));
    }

    #[test]
    fn round_trips_cel_opacity() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba(255, 255, 255, 255));
        s.cels[0][0].opacity = 96;
        let (loaded, _) = saved_depth(&s, "vixel_ase_celop.ase");
        assert_eq!(loaded.cels[0][0].opacity, 96);
        assert!(loaded.composite(0)[0].3 < 100);
    }

    // ---- synthesized fixtures for cel types the writer never emits ----

    fn ud_chunk(text: &str, color: Option<Rgba>) -> Vec<u8> {
        let mut flags = 0u32;
        if !text.is_empty() {
            flags |= 1;
        }
        if color.is_some() {
            flags |= 2;
        }
        let mut b = flags.to_le_bytes().to_vec();
        if flags & 1 != 0 {
            b.extend_from_slice(&(text.len() as u16).to_le_bytes());
            b.extend_from_slice(text.as_bytes());
        }
        if let Some(c) = color {
            b.extend_from_slice(&[c.0, c.1, c.2, c.3]);
        }
        b
    }

    fn one_tag(name: &str) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 8]);
        b.extend_from_slice(&0u16.to_le_bytes()); // from
        b.extend_from_slice(&0u16.to_le_bytes()); // to
        b.push(0); // direction
        b.extend_from_slice(&0u16.to_le_bytes()); // repeat
        b.extend_from_slice(&[0u8; 6]);
        b.extend_from_slice(&[1, 2, 3]);
        b.push(0);
        b.extend_from_slice(&(name.len() as u16).to_le_bytes());
        b.extend_from_slice(name.as_bytes());
        b
    }

    fn ase_header(w: u16, h: u16, depth: u16, frames: u16) -> Vec<u8> {
        let mut hdr = vec![0u8; 128];
        hdr[4..6].copy_from_slice(&MAGIC.to_le_bytes());
        hdr[6..8].copy_from_slice(&frames.to_le_bytes());
        hdr[8..10].copy_from_slice(&w.to_le_bytes());
        hdr[10..12].copy_from_slice(&h.to_le_bytes());
        hdr[12..14].copy_from_slice(&depth.to_le_bytes());
        hdr[14..18].copy_from_slice(&1u32.to_le_bytes());
        hdr
    }

    fn ase_frame(chunks: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (ty, b) in chunks {
            write_chunk(&mut body, *ty, b);
        }
        let mut f = Vec::new();
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&FRAME_MAGIC.to_le_bytes());
        f.extend_from_slice(&0u16.to_le_bytes());
        f.extend_from_slice(&100u16.to_le_bytes());
        f.extend_from_slice(&[0u8, 0]);
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&body);
        let size = f.len() as u32;
        f[0..4].copy_from_slice(&size.to_le_bytes());
        f
    }

    fn load_bytes(name: &str, mut data: Vec<u8>) -> Sprite {
        let size = data.len() as u32;
        data[0..4].copy_from_slice(&size.to_le_bytes());
        let path = tmp(name);
        std::fs::write(&path, &data).unwrap();
        let sprite = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        sprite
    }

    fn cel_chunk(layer: u16, ty: u16, data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&layer.to_le_bytes());
        b.extend_from_slice(&0i16.to_le_bytes());
        b.extend_from_slice(&0i16.to_le_bytes());
        b.push(255);
        b.extend_from_slice(&ty.to_le_bytes());
        b.extend_from_slice(&[0u8; 7]);
        b.extend_from_slice(data);
        b
    }

    fn layer_chunk(chunk_type: u16, tileset: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&3u16.to_le_bytes()); // visible | editable
        b.extend_from_slice(&chunk_type.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // child level
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // blend: normal
        b.push(255);
        b.extend_from_slice(&[0u8; 3]);
        let name = b"L";
        b.extend_from_slice(&(name.len() as u16).to_le_bytes());
        b.extend_from_slice(name);
        if chunk_type == 2 {
            b.extend_from_slice(&tileset.to_le_bytes());
        }
        b
    }

    #[test]
    fn reads_linked_cels() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&1u16.to_le_bytes()); // width
        raw.extend_from_slice(&1u16.to_le_bytes()); // height
        raw.extend_from_slice(&[255, 0, 0, 255]);
        let mut link = Vec::new();
        link.extend_from_slice(&0u16.to_le_bytes()); // link to frame 0

        let mut data = ase_header(1, 1, 32, 2);
        data.extend(ase_frame(&[
            (CHUNK_LAYER, layer_chunk(0, 0)),
            (CHUNK_CEL, cel_chunk(0, 0, &raw)),
        ]));
        data.extend(ase_frame(&[(CHUNK_CEL, cel_chunk(0, 1, &link))]));

        let s = load_bytes("vixel_ase_linked.ase", data);
        assert_eq!(s.frames.len(), 2);
        assert_eq!(s.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
        assert_eq!(s.get(0, 1, 0, 0), Rgba(255, 0, 0, 255));
        assert!(s.is_linked(0, 1));
        assert_eq!(s.cels[0][1].link, Some(0));
    }

    #[test]
    fn reads_compressed_tilemap_cels() {
        // Tileset: 2x2 tiles. Tile 0 transparent, tile 1 = red/green/blue/white.
        let mut tile_pixels = vec![0u8; 2 * 4 * 4];
        tile_pixels[16..].copy_from_slice(&[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ]);
        let mut ts = Vec::new();
        ts.extend_from_slice(&1u32.to_le_bytes()); // tileset id
        ts.extend_from_slice(&2u32.to_le_bytes()); // flags: embedded image
        ts.extend_from_slice(&2u32.to_le_bytes()); // number of tiles
        ts.extend_from_slice(&2u16.to_le_bytes()); // tile width
        ts.extend_from_slice(&2u16.to_le_bytes()); // tile height
        ts.extend_from_slice(&1i16.to_le_bytes()); // base index
        ts.extend_from_slice(&[0u8; 14]);
        ts.extend_from_slice(&0u16.to_le_bytes()); // name
        let mut packed = Vec::new();
        {
            use std::io::Write;
            let mut enc =
                flate2::write::ZlibEncoder::new(&mut packed, flate2::Compression::default());
            enc.write_all(&tile_pixels).unwrap();
            enc.finish().unwrap();
        }
        ts.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        ts.extend_from_slice(&packed);

        // Tilemap cel: 2x1 tiles, both tile 1, the second flipped horizontally.
        let mut tiles_raw = Vec::new();
        tiles_raw.extend_from_slice(&1u32.to_le_bytes());
        tiles_raw.extend_from_slice(&(1u32 | (1 << 29)).to_le_bytes());
        let mut packed = Vec::new();
        {
            use std::io::Write;
            let mut enc =
                flate2::write::ZlibEncoder::new(&mut packed, flate2::Compression::default());
            enc.write_all(&tiles_raw).unwrap();
            enc.finish().unwrap();
        }
        let mut cel = Vec::new();
        cel.extend_from_slice(&2u16.to_le_bytes()); // tiles wide
        cel.extend_from_slice(&1u16.to_le_bytes()); // tiles tall
        cel.extend_from_slice(&32u16.to_le_bytes()); // bits per tile
        cel.extend_from_slice(&0x1fff_ffffu32.to_le_bytes()); // id mask
        cel.extend_from_slice(&(1u32 << 29).to_le_bytes()); // flip x
        cel.extend_from_slice(&(1u32 << 30).to_le_bytes()); // flip y
        cel.extend_from_slice(&(1u32 << 31).to_le_bytes()); // flip diagonal
        cel.extend_from_slice(&[0u8; 10]);
        cel.extend_from_slice(&packed);

        let mut data = ase_header(4, 2, 32, 1);
        data.extend(ase_frame(&[
            (CHUNK_TILESET, ts),
            (CHUNK_LAYER, layer_chunk(2, 0)),
            (CHUNK_CEL, cel_chunk(0, 3, &cel)),
        ]));

        let s = load_bytes("vixel_ase_tilemap.ase", data);
        assert_eq!(s.get(0, 0, 0, 0), Rgba(255, 0, 0, 255));
        assert_eq!(s.get(0, 0, 1, 0), Rgba(0, 255, 0, 255));
        assert_eq!(s.get(0, 0, 2, 0), Rgba(0, 255, 0, 255), "flipped tile");
        assert_eq!(s.get(0, 0, 3, 0), Rgba(255, 0, 0, 255));
        assert_eq!(s.get(0, 0, 2, 1), Rgba(255, 255, 255, 255));
        assert_eq!(s.get(0, 0, 3, 1), Rgba(0, 0, 255, 255));
    }

    #[test]
    fn round_trips_frame_tags() {
        let mut s = Sprite::new(1, 1);
        s.add_frame_after(0);
        s.add_frame_after(1);
        s.set(0, 0, 0, 0, Rgba(1, 2, 3, 255));
        s.tags.push(Tag {
            name: "walk".into(),
            from: 0,
            to: 1,
            color: Rgba(200, 100, 50, 255),
            direction: PlayDirection::PingPong,
            repeat: 3,
            user_data: None,
        });
        s.tags.push(Tag {
            name: "idle".into(),
            from: 2,
            to: 2,
            color: Rgba(10, 20, 30, 255),
            direction: PlayDirection::Forward,
            repeat: 0,
            user_data: None,
        });
        let path = tmp("vixel_ase_tags.ase");
        save(&s, &path).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.tags.len(), 2);
        assert_eq!(loaded.tags[0].name, "walk");
        assert_eq!((loaded.tags[0].from, loaded.tags[0].to), (0, 1));
        assert_eq!(loaded.tags[0].color, Rgba(200, 100, 50, 255));
        assert_eq!(loaded.tags[0].direction, PlayDirection::PingPong);
        assert_eq!(loaded.tags[0].repeat, 3);
        assert_eq!(loaded.tags[1].name, "idle");
        assert_eq!((loaded.tags[1].from, loaded.tags[1].to), (2, 2));
    }

    fn walk_chunks(data: &[u8]) -> Vec<(usize, u16, Vec<u8>)> {
        let frames = u16_at(data, 6) as usize;
        let mut out = Vec::new();
        let mut pos = 128usize;
        for frame in 0..frames {
            let frame_size = u32_at(data, pos) as usize;
            let end = pos + frame_size;
            let mut p = pos + 16;
            while p + 6 <= end {
                let size = u32_at(data, p) as usize;
                if size < 6 {
                    break;
                }
                let kind = u16_at(data, p + 4);
                out.push((
                    frame,
                    kind,
                    data.get(p + 6..p + size).unwrap_or(&[]).to_vec(),
                ));
                p += size;
            }
            pos = end;
        }
        out
    }

    fn frame_counts(data: &[u8], frame: usize) -> (u16, u32) {
        let mut pos = 128usize;
        for _ in 0..frame {
            pos += u32_at(data, pos) as usize;
        }
        (u16_at(data, pos + 6), u32_at(data, pos + 12))
    }

    #[test]
    fn every_blend_mode_round_trips_through_ase() {
        use crate::sprite::BlendMode;
        let modes = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::HardLight,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Exclusion,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Divide,
        ];
        for mode in modes {
            let mut s = Sprite::new(1, 1);
            s.set(0, 0, 0, 0, Rgba(1, 2, 3, 255));
            s.layers[0].blend = mode;
            let path = tmp(&format!("vixel_ase_blend_{mode:?}.ase"));
            save(&s, &path).unwrap();
            let back = load(&path).unwrap();
            let _ = std::fs::remove_file(&path);
            assert_eq!(back.layers[0].blend, mode, "{mode:?}");
        }
    }

    #[test]
    fn frame_headers_count_every_chunk() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba(1, 2, 3, 255));
        s.tags.push(Tag {
            name: "t".into(),
            from: 0,
            to: 0,
            color: Rgba(1, 2, 3, 255),
            direction: PlayDirection::Forward,
            repeat: 0,
            user_data: None,
        });
        let path = tmp("vixel_ase_counts.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let actual = walk_chunks(&data)
            .iter()
            .filter(|(f, _, _)| *f == 0)
            .count() as u32;
        let (old, new) = frame_counts(&data, 0);
        assert_eq!(new, actual, "the new count field is exact");
        assert_eq!(old as u32, actual.min(u16::MAX as u32));
        assert!(actual >= 4, "palette, layer, tags and cel");
    }

    #[test]
    fn unknown_chunks_keep_their_frame_and_anchor() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&[255, 0, 0, 255]);
        let mut data = ase_header(1, 1, 32, 1);
        data.extend(ase_frame(&[
            (0x3333, vec![3, 3, 3]), // frame start
            (CHUNK_LAYER, layer_chunk(0, 0)),
            (0x1234, vec![1, 2, 3]), // after the layer
            (CHUNK_CEL, cel_chunk(0, 0, &raw)),
            (0x2020, vec![9, 9]), // after the cel (user-data shape)
            (0x2222, vec![7]),    // still after the cel
        ]));
        let s = load_bytes("vixel_ase_unknown.ase", data);
        assert_eq!(s.raw_chunks.len(), 4);

        let path = tmp("vixel_ase_unknown_rt.ase");
        save(&s, &path).unwrap();
        let out = std::fs::read(&path).unwrap();
        let chunks = walk_chunks(&out);
        let kinds: Vec<u16> = chunks.iter().map(|(_, k, _)| *k).collect();
        let at = |k: u16| {
            kinds
                .iter()
                .position(|x| *x == k)
                .unwrap_or_else(|| panic!("missing {k:#x} in {kinds:?}"))
        };
        assert!(
            at(0x3333) < at(CHUNK_LAYER),
            "frame-start chunk stays first"
        );
        assert!(at(CHUNK_LAYER) < at(0x1234) && at(0x1234) < at(CHUNK_CEL));
        assert!(at(CHUNK_CEL) < at(0x2020) && at(0x2020) < at(0x2222));
        assert_eq!(chunks[at(0x1234)].2, vec![1, 2, 3]);

        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.raw_chunks.len(), 4, "a reopen still sees them");
    }

    #[test]
    fn real_fixtures_survive_open_save_open() {
        for name in [
            "prop.ase",
            "test_grayscale.ase",
            "test_nrgba.ase",
            "test_paletted.ase",
        ] {
            let src = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/ase")
                .join(name);
            let a = load(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
            let out = tmp(&format!("vixel_ase_rt_{name}"));
            save(&a, &out).unwrap();
            let b = load(&out).unwrap();
            let _ = std::fs::remove_file(&out);

            assert_eq!(b.frames.len(), a.frames.len(), "{name} frames");
            assert_eq!(b.layers.len(), a.layers.len(), "{name} layers");
            for f in 0..a.frames.len() {
                assert_eq!(b.composite(f), a.composite(f), "{name} frame {f}");
            }
            // Groups are re-emitted after their anchor object, so compare the
            // set of (frame, kind, body) rather than the regrouped order.
            let mut raw_a: Vec<_> = a
                .raw_chunks
                .iter()
                .map(|c| (c.frame, c.kind, c.body.clone()))
                .collect();
            let mut raw_b: Vec<_> = b
                .raw_chunks
                .iter()
                .map(|c| (c.frame, c.kind, c.body.clone()))
                .collect();
            raw_a.sort();
            raw_b.sort();
            assert_eq!(raw_b, raw_a, "{name} unknown chunks");
        }
    }

    #[test]
    fn indexed_fixture_edits_keep_indices_and_transparency() {
        use crate::sprite::ColorMode;
        let src =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ase/test_paletted.ase");
        let mut s = load(&src).unwrap();
        assert_eq!(s.color_mode, ColorMode::Indexed);
        assert_eq!(s.palette.len(), 256);
        let ti = s.transparent_index;
        let want = (1..s.palette.len())
            .find(|&i| i != ti as usize && s.palette[i].3 > 0)
            .expect("an opaque palette slot");
        let color = s.palette[want];
        // Layer 2's cel starts at (10, 8); layer 0 is a group and layer 1 is
        // the tilemap, so edit the image layer.
        s.set(2, 0, 10, 8, color);
        s.set(2, 0, 11, 8, Rgba::TRANSPARENT);

        let out = tmp("vixel_ase_indexed_edit.ase");
        save(&s, &out).unwrap();
        let data = std::fs::read(&out).unwrap();
        assert_eq!(data[28], ti, "header transparent index");
        assert_eq!(u16_at(&data, 32) as usize, s.palette.len());
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(f, k, body)| *f == 0 && *k == CHUNK_CEL && u16_at(body, 0) == 2)
            .expect("layer 2 cel")
            .2;
        assert_eq!(u16_at(&body, 7), 0, "raw cel");
        assert_eq!((i16_at(&body, 2), i16_at(&body, 4)), (10, 8), "cel origin");
        assert_eq!((u16_at(&body, 16), u16_at(&body, 18)), (44, 50));
        assert_eq!((body[20], body[21]), (want as u8, ti));

        let back = load(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(back.palette, s.palette);
        assert_eq!(back.transparent_index, ti);
        assert_eq!(back.get(2, 0, 10, 8), color);
        assert_eq!(back.get(2, 0, 11, 8), Rgba::TRANSPARENT);
    }

    #[test]
    fn slice_chunks_round_trip() {
        let mut s = Sprite::new(4, 4);
        s.set(0, 0, 1, 1, Rgba(9, 8, 7, 255));
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
                center: Some((1, 1, 2, 2)),
                pivot: Some((0, 1)),
            }],
            user_data: Some(UserData {
                text: String::new(),
                color: Some(Rgba(1, 2, 3, 4)),
                properties: Vec::new(),
                flags: 2,
            }),
        });
        let path = tmp("vixel_ase_slice.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_SLICE)
            .expect("slice chunk")
            .2;
        assert_eq!(u32_at(&body, 0), 1, "one key");
        assert_eq!(u32_at(&body, 4), 3, "9-patch + pivot flags");
        assert_eq!(u16_at(&body, 12), 6, "name length");
        assert_eq!(&body[14..20], b"button");
        assert_eq!(u32_at(&body, 20), 0, "frame");
        assert_eq!(u32_at(&body, 24) as i32, 1, "x");
        assert_eq!(u32_at(&body, 40) as i32, 1, "center x");
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.slices, s.slices);
    }

    #[test]
    fn fixture_slices_round_trip() {
        let src =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ase/test_grayscale.ase");
        let a = load(&src).unwrap();
        assert_eq!(a.slices.len(), 1);
        assert_eq!(a.slices[0].name, "slice1");
        assert!(a.slices[0].nine_patch && a.slices[0].pivot);
        assert_eq!(a.slices[0].keys.len(), 1);
        let out = tmp("vixel_ase_slice_fixture.ase");
        save(&a, &out).unwrap();
        let b = load(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(b.slices, a.slices);
    }

    #[test]
    fn layer_flags_and_uuid_round_trip() {
        let mut s = Sprite::new(2, 2);
        s.layers[0].lock_movement = true;
        s.layers[0].collapsed = true;
        let uuid = [1, 2, 3, 4, 5, 6, 0x47, 8, 0x89, 10, 11, 12, 13, 14, 15, 16];
        s.layers[0].uuid = Some(uuid);

        let path = tmp("vixel_ase_layer_uuid.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        assert_eq!(u32_at(&data, 14) & 4, 4, "header UUID flag");
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_LAYER)
            .expect("layer chunk")
            .2;
        assert_eq!(u16_at(&body, 0) & 4, 4, "lock movement flag");
        assert_eq!(u16_at(&body, 0) & 32, 32, "collapsed flag");
        let name_len = u16_at(&body, 16) as usize;
        assert_eq!(&body[18 + name_len..18 + name_len + 16], &uuid);

        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(back.layers[0].lock_movement);
        assert!(back.layers[0].collapsed);
        assert_eq!(back.layers[0].uuid, Some(uuid));

        let mut plain = Sprite::new(1, 1);
        plain.layers[0].uuid = None;
        let path = tmp("vixel_ase_no_uuid.ase");
        save(&plain, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(u32_at(&data, 14) & 4, 0, "no UUIDs, no header flag");
    }

    #[test]
    fn fixture_layer_uuids_survive_save() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ase/test_nrgba.ase");
        let a = load(&src).unwrap();
        assert!(a.layers.iter().all(|l| l.uuid.is_some()));
        let out = tmp("vixel_ase_fixture_uuid.ase");
        save(&a, &out).unwrap();
        let b = load(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        let before: Vec<_> = a.layers.iter().map(|l| l.uuid).collect();
        let after: Vec<_> = b.layers.iter().map(|l| l.uuid).collect();
        assert_eq!(after, before);
    }

    #[test]
    fn user_data_attaches_to_layer_cel_and_tag() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&1u16.to_le_bytes());
        raw.extend_from_slice(&1u16.to_le_bytes());
        raw.extend_from_slice(&[255, 0, 0, 255]);
        let mut data = ase_header(1, 1, 32, 1);
        data.extend(ase_frame(&[
            (CHUNK_LAYER, layer_chunk(0, 0)),
            (CHUNK_USER_DATA, ud_chunk("layer", Some(Rgba(1, 2, 3, 4)))),
            (CHUNK_CEL, cel_chunk(0, 0, &raw)),
            (CHUNK_USER_DATA, ud_chunk("cel", None)),
            (CHUNK_TAGS, one_tag("walk")),
            (CHUNK_USER_DATA, ud_chunk("tag", Some(Rgba(9, 8, 7, 255)))),
        ]));
        let s = load_bytes("vixel_ase_userdata.ase", data);
        assert_eq!(s.layers[0].user_data.as_ref().unwrap().text, "layer");
        assert_eq!(
            s.layers[0].user_data.as_ref().unwrap().color,
            Some(Rgba(1, 2, 3, 4))
        );
        assert_eq!(s.cels[0][0].user_data.as_ref().unwrap().text, "cel");
        assert_eq!(s.tags[0].user_data.as_ref().unwrap().text, "tag");
        assert_eq!(s.tags[0].color, Rgba(9, 8, 7, 255));

        let path = tmp("vixel_ase_userdata_rt.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let uds = walk_chunks(&data)
            .into_iter()
            .filter(|(_, k, _)| *k == CHUNK_USER_DATA)
            .count();
        assert_eq!(uds, 3);
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.layers[0].user_data, s.layers[0].user_data);
        assert_eq!(back.cels[0][0].user_data, s.cels[0][0].user_data);
        assert_eq!(back.tags[0].user_data, s.tags[0].user_data);
    }

    #[test]
    fn user_data_properties_and_sprite_data_round_trip() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        let properties = vec![8, 0, 0, 0, 0, 0, 0, 0];
        s.user_data = Some(UserData {
            text: "sprite".into(),
            color: Some(Rgba(4, 5, 6, 7)),
            properties: properties.clone(),
            flags: 7,
        });
        s.layers[0].user_data = Some(UserData {
            text: "layer".into(),
            color: None,
            properties: Vec::new(),
            flags: 1,
        });
        let path = tmp("vixel_ase_userdata_props.ase");
        save(&s, &path).unwrap();
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.user_data, s.user_data);
        assert_eq!(back.user_data.unwrap().properties, properties);
        assert_eq!(back.layers[0].user_data, s.layers[0].user_data);
    }

    #[test]
    fn fixture_user_data_survives_open_save_open() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ase/prop.ase");
        let a = load(&src).unwrap();
        assert!(a.layers.iter().any(|l| l.user_data.is_some()));
        let layers: Vec<_> = a.layers.iter().map(|l| l.user_data.clone()).collect();
        let cels: Vec<_> = a
            .cels
            .iter()
            .map(|f| f.iter().map(|c| c.user_data.clone()).collect::<Vec<_>>())
            .collect();
        let out = tmp("vixel_ase_userdata_fixture.ase");
        save(&a, &out).unwrap();
        let b = load(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(
            b.layers
                .iter()
                .map(|l| l.user_data.clone())
                .collect::<Vec<_>>(),
            layers
        );
        assert_eq!(
            b.cels
                .iter()
                .map(|f| f.iter().map(|c| c.user_data.clone()).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            cels
        );
        assert_eq!(b.user_data, a.user_data);
    }

    #[test]
    fn palette_names_round_trip_through_ase() {
        let mut s = Sprite::new(2, 1);
        s.set(0, 0, 0, 0, Rgba(200, 30, 40, 255));
        s.palette = vec![
            Rgba::TRANSPARENT,
            Rgba(200, 30, 40, 255),
            Rgba(10, 20, 30, 255),
        ];
        s.palette_names = vec![String::new(), "blood".into(), "sky".into()];
        s.transparent_index = 0;
        s.color_mode = crate::sprite::ColorMode::Indexed;

        let path = tmp("vixel_ase_palette_names.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let body = walk_chunks(&data)
            .into_iter()
            .find(|(_, k, _)| *k == CHUNK_PALETTE)
            .expect("palette chunk")
            .2;
        // 20-byte header, entry 0 has no name (6 bytes), entry 1 starts at 26.
        assert_eq!(u16_at(&body, 26) & 1, 1, "name flag");
        assert_eq!(u16_at(&body, 32), 5, "UTF-16 name length");

        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.palette_names[..3], s.palette_names[..3]);
    }

    #[test]
    fn dithered_pattern_survives_ase() {
        use crate::sprite::DitherMode;
        let mut s = Sprite::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                s.set(0, 0, x, y, Rgba(128, 128, 128, 255));
            }
        }
        s.palette = vec![
            Rgba::TRANSPARENT,
            Rgba(0, 0, 0, 255),
            Rgba(255, 255, 255, 255),
        ];
        s.transparent_index = 0;
        s.color_mode = crate::sprite::ColorMode::Indexed;
        s.dither_to_palette(DitherMode::Bayer2);
        let composite = s.composite(0);

        let path = tmp("vixel_ase_dither.ase");
        save(&s, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        assert_eq!(u16_at(&data, 32) as usize, s.palette.len());
        let back = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.palette[..s.palette.len()], s.palette);
        assert_eq!(back.composite(0), composite, "indices keep the pattern");
    }

    #[test]
    fn loads_real_aseprite_files() {
        for name in [
            "prop.ase",
            "test_grayscale.ase",
            "test_nrgba.ase",
            "test_paletted.ase",
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/ase")
                .join(name);
            let s = load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(s.width > 0 && s.height > 0, "{name}");
            assert!(
                s.composite(0).iter().any(|p| p.3 > 0),
                "{name} came out empty"
            );
        }
    }
}
