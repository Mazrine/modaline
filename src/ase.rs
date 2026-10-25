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
use crate::sprite::{BlendMode, Cel, Frame, Layer, Sprite, Tag};

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
    /// `palette[0]` is transparent; pixels map to entries by exact RGBA match.
    Indexed(Vec<Rgba>),
}

/// Pick the smallest depth that represents every pixel exactly. Grayscale wins
/// when it can (r == g == b), then indexed when the sprite uses <= 255 distinct
/// colors, else full RGBA.
fn out_depth(sprite: &Sprite) -> OutDepth {
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
        OutDepth::Indexed(colors)
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
        OutDepth::Indexed(_) => 8,
    };
    h[12..14].copy_from_slice(&bits.to_le_bytes());
    let mut flags: u32 = 1; // layer opacity is valid
    if s.layers.iter().any(|l| l.group) {
        flags |= 2; // group blend/opacity valid
    }
    h[14..18].copy_from_slice(&flags.to_le_bytes());
    h[18..20].copy_from_slice(&0u16.to_le_bytes()); // speed (deprecated)
    if let OutDepth::Indexed(pal) = depth {
        h[28] = 0; // transparent color index
        h[32..34].copy_from_slice(&(pal.len() as u16).to_le_bytes());
    }
    h[34] = 1; // pixel width
    h[35] = 1; // pixel height
    out.extend_from_slice(&h);
}

fn write_frame(out: &mut Vec<u8>, s: &Sprite, frame: usize, depth: &OutDepth) {
    let mut body: Vec<u8> = Vec::new();
    if frame == 0 {
        if let OutDepth::Indexed(pal) = depth {
            write_palette_chunk(&mut body, pal);
        }
        for layer in &s.layers {
            write_layer_chunk(&mut body, layer);
        }
        if !s.tags.is_empty() {
            write_tags_chunk(&mut body, &s.tags);
        }
    }
    for li in 0..s.layers.len() {
        if s.layers[li].group {
            continue; // groups have no cels
        }
        write_cel_chunk(&mut body, s, li, frame, depth);
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

fn write_palette_chunk(out: &mut Vec<u8>, palette: &[Rgba]) {
    let mut b = Vec::new();
    b.extend_from_slice(&(palette.len() as u32).to_le_bytes()); // new palette size
    b.extend_from_slice(&0u32.to_le_bytes()); // first color index
    b.extend_from_slice(&(palette.len().saturating_sub(1) as u32).to_le_bytes());
    b.extend_from_slice(&[0u8; 8]); // reserved
    for c in palette {
        b.extend_from_slice(&0u16.to_le_bytes()); // no color name
        b.extend_from_slice(&[c.0, c.1, c.2, c.3]);
    }
    write_chunk(out, CHUNK_PALETTE, &b);
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

fn write_tags_chunk(out: &mut Vec<u8>, tags: &[Tag]) {
    let mut b = Vec::new();
    b.extend_from_slice(&(tags.len() as u16).to_le_bytes());
    b.extend_from_slice(&[0u8; 8]); // reserved
    for t in tags {
        b.extend_from_slice(&(t.from as u16).to_le_bytes());
        b.extend_from_slice(&(t.to as u16).to_le_bytes());
        b.push(0); // loop direction: forward
        b.extend_from_slice(&0u16.to_le_bytes()); // repeat
        b.extend_from_slice(&[0u8; 6]); // reserved
        b.extend_from_slice(&[t.color.0, t.color.1, t.color.2]);
        b.push(0); // extra byte
        let name = t.name.as_bytes();
        b.extend_from_slice(&(name.len() as u16).to_le_bytes());
        b.extend_from_slice(name);
    }
    write_chunk(out, CHUNK_TAGS, &b);
}

fn write_cel_chunk(out: &mut Vec<u8>, s: &Sprite, li: usize, frame: usize, depth: &OutDepth) {
    let cel = &s.cels[li][frame];
    if cel.pixels.iter().all(|p| p.3 == 0) {
        return; // empty cels are implicit in .ase
    }
    let mut b = Vec::new();
    b.extend_from_slice(&(li as u16).to_le_bytes());
    b.extend_from_slice(&0i16.to_le_bytes()); // x
    b.extend_from_slice(&0i16.to_le_bytes()); // y
    b.push(cel.opacity);
    b.extend_from_slice(&0u16.to_le_bytes()); // cel type: raw
    b.extend_from_slice(&[0u8; 7]); // z-index + reserved
    b.extend_from_slice(&(s.width as u16).to_le_bytes());
    b.extend_from_slice(&(s.height as u16).to_le_bytes());
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
        OutDepth::Indexed(pal) => {
            for p in &cel.pixels {
                let idx = if p.3 == 0 {
                    0
                } else {
                    pal.iter().position(|c| c == p).unwrap_or(0)
                };
                b.push(idx as u8);
            }
        }
    }
    write_chunk(out, CHUNK_CEL, &b);
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
        BlendMode::Add => 16,
        BlendMode::Subtract => 17,
    }
}

// ---- import ----

const CHUNK_TILESET: u16 = 0x2023;

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
}

struct Import {
    depth: u16,
    width: u32,
    height: u32,
    frames: usize,
    palette: Vec<Rgba>,
    /// `[layer][frame]`; `None` means an empty cel.
    cels: Vec<Vec<Option<Cel>>>,
    layer_tilesets: Vec<u32>,
    tilesets: Vec<Tileset>,
    tilemaps: Vec<TilemapCel>,
    tags: Vec<Tag>,
}

impl Import {
    fn decode_cel(&mut self, frame: usize, body: &[u8]) -> Result<()> {
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
        match u16_at(body, 7) {
            0 => {
                let cw = u16_at(body, 16) as u32;
                let ch = u16_at(body, 18) as u32;
                let px = decode_pixels(&body[20..], self.depth, cw, ch, &self.palette).2;
                self.place((li, frame), (ox, oy), (cw, ch), &px, opacity);
            }
            1 => {
                // Linked cel: same pixels as the cel in the referenced frame.
                let link = u16_at(body, 16) as usize;
                if let Some(Some(mut cel)) = self.cels.get(li).and_then(|l| l.get(link)).cloned() {
                    cel.opacity = opacity;
                    self.cels[li][frame] = Some(cel);
                }
            }
            2 => {
                let cw = u16_at(body, 16) as u32;
                let ch = u16_at(body, 18) as u32;
                let mut raw = Vec::new();
                flate2::read::ZlibDecoder::new(&body[20..]).read_to_end(&mut raw)?;
                let px = decode_pixels(&raw, self.depth, cw, ch, &self.palette).2;
                self.place((li, frame), (ox, oy), (cw, ch), &px, opacity);
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
                });
            }
            _ => {}
        }
        Ok(())
    }

    /// Copy a `cw x ch` block at `(ox, oy)` into a full-canvas cel buffer.
    fn place(
        &mut self,
        (li, frame): (usize, usize),
        (ox, oy): (i32, i32),
        (cw, ch): (u32, u32),
        px: &[Rgba],
        opacity: u8,
    ) {
        let mut full = vec![Rgba::TRANSPARENT; (self.width * self.height) as usize];
        for yy in 0..ch {
            for xx in 0..cw {
                let (x, y) = (ox + xx as i32, oy + yy as i32);
                if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
                    let src = *px
                        .get((yy * cw + xx) as usize)
                        .unwrap_or(&Rgba::TRANSPARENT);
                    full[(y as u32 * self.width + x as u32) as usize] = src;
                }
            }
        }
        self.cels[li][frame] = Some(Cel {
            pixels: full,
            opacity,
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
            let mut full = vec![Rgba::TRANSPARENT; (self.width * self.height) as usize];
            for (i, &(id, fx, fy, fd)) in tc.tiles.iter().enumerate() {
                if id == 0 || id >= ts.num_tiles {
                    continue;
                }
                let tx = (i as u32 % tc.tiles_w) * ts.tile_w;
                let ty = (i as u32 / tc.tiles_w) * ts.tile_h;
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
                        let (x, y) = (tc.ox + (tx + dx) as i32, tc.oy + (ty + dy) as i32);
                        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
                            full[(y as u32 * self.width + x as u32) as usize] = src;
                        }
                    }
                }
            }
            if tc.frame < self.frames && tc.layer < self.cels.len() {
                self.cels[tc.layer][tc.frame] = Some(Cel {
                    pixels: full,
                    opacity: tc.opacity,
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

    let mut im = Import {
        depth,
        width,
        height,
        frames: frames_count.max(1),
        palette: vec![Rgba::TRANSPARENT; 256],
        cels: Vec::new(),
        layer_tilesets: Vec::new(),
        tilesets: Vec::new(),
        tilemaps: Vec::new(),
        tags: Vec::new(),
    };
    let has_uuid = u32_at(&data, 14) & 4 != 0;
    let mut layers: Vec<Layer> = Vec::new();
    let mut durations: Vec<u16> = Vec::new();
    // Cel bodies are decoded after the whole file is read so the palette and
    // tilesets are complete regardless of chunk order.
    let mut cel_bodies: Vec<(usize, Vec<u8>)> = Vec::new();

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
                    let (layer, tileset) = parse_layer(body, has_uuid)?;
                    layers.push(layer);
                    im.cels.push(vec![None; im.frames]);
                    im.layer_tilesets.push(tileset);
                }
                CHUNK_CEL => cel_bodies.push((frame, body.to_vec())),
                CHUNK_TAGS => im.tags.extend(parse_tags(body)),
                CHUNK_TILESET => im.tilesets.push(parse_tileset(body, depth, &im.palette)?),
                CHUNK_PALETTE => parse_palette_new(body, &mut im.palette)?,
                CHUNK_PALETTE_RGB => parse_palette_rgb(body, &mut im.palette, false),
                CHUNK_PALETTE_RGBA => parse_palette_rgb(body, &mut im.palette, true),
                _ => {}
            }
            pos += chunk_size;
        }
        pos = frame_end;
    }

    for (frame, body) in cel_bodies {
        im.decode_cel(frame, &body)?;
    }
    im.resolve_tilemaps();

    if layers.is_empty() {
        layers.push(Layer {
            name: "Layer 1".into(),
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
                        .unwrap_or_else(|| Cel::blank((width * height) as usize))
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
        tags: im.tags,
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
        });
    }
    tags
}

fn parse_tileset(body: &[u8], depth: u16, palette: &[Rgba]) -> Result<Tileset> {
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
            image = decode_pixels(&raw, depth, tile_w, tile_h * num_tiles, palette).2;
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
    let tileset = if kind == 2 {
        u32_at(body, 18 + name_len + if has_uuid { 16 } else { 0 })
    } else {
        0
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
        16 => BlendMode::Add,
        17 => BlendMode::Subtract,
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
) -> (u32, u32, Vec<Rgba>) {
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
            // skip a UTF-16 name
            let name_len = u16_at(body, p) as usize;
            p += 2 + name_len * 2;
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
        });
        s.tags.push(Tag {
            name: "idle".into(),
            from: 2,
            to: 2,
            color: Rgba(10, 20, 30, 255),
        });
        let path = tmp("vixel_ase_tags.ase");
        save(&s, &path).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.tags.len(), 2);
        assert_eq!(loaded.tags[0].name, "walk");
        assert_eq!((loaded.tags[0].from, loaded.tags[0].to), (0, 1));
        assert_eq!(loaded.tags[0].color, Rgba(200, 100, 50, 255));
        assert_eq!(loaded.tags[1].name, "idle");
        assert_eq!((loaded.tags[1].from, loaded.tags[1].to), (2, 2));
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
