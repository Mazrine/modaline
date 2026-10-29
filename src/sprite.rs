use serde::{Deserialize, Serialize};

use crate::color::Rgba;

/// One cel: the image a single layer shows on a single frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cel {
    /// Top-left corner in sprite coordinates (may be negative).
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    /// Cel size in pixels; `pixels` is row-major `w * h`.
    #[serde(default)]
    pub w: u32,
    #[serde(default)]
    pub h: u32,
    /// Aseprite cel z-index: positive moves this cel towards the front.
    #[serde(default)]
    pub z_index: i16,
    /// Linked cel: content comes from this frame in the same layer. Painting
    /// materialises the content first (copy-on-write).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<usize>,
    pub pixels: Vec<Rgba>,
    /// 0..255, cel opacity (Aseprite stores this per cel).
    #[serde(default = "full")]
    pub opacity: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserData>,
    /// Retained tile refs for tilemap cels; pixels stay the render cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilemap: Option<TilemapCel>,
}

/// One tile reference in a tilemap cel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileRef {
    /// Tile ID (0 = empty), mapped through the tileset's base index.
    pub id: u32,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
    #[serde(default)]
    pub flip_diagonal: bool,
}

/// Row-major tile references for one tilemap cel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TilemapCel {
    pub tiles_w: u32,
    pub tiles_h: u32,
    pub tiles: Vec<TileRef>,
}

impl TilemapCel {
    pub fn new(tiles_w: u32, tiles: Vec<TileRef>) -> Self {
        let tiles_w = tiles_w.max(1);
        let tiles_h = (tiles.len() as u32).div_ceil(tiles_w);
        TilemapCel {
            tiles_w,
            tiles_h,
            tiles,
        }
    }
}

fn full() -> u8 {
    255
}

/// Aseprite layer UUID (v4-shaped, not cryptographic).
pub(crate) fn mint_uuid(seed: usize) -> [u8; 16] {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut x = now ^ ((std::process::id() as u128) << 32) ^ seed as u128;
    let mut u = [0u8; 16];
    for b in &mut u {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (x >> 64) as u8;
    }
    u[6] = (u[6] & 0x0f) | 0x40;
    u[8] = (u[8] & 0x3f) | 0x80;
    u
}

impl Cel {
    pub fn blank(w: u32, h: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            w,
            h,
            z_index: 0,
            link: None,
            pixels: vec![Rgba::TRANSPARENT; (w * h) as usize],
            opacity: 255,
            user_data: None,
            tilemap: None,
        }
    }

    pub fn area(&self) -> usize {
        (self.w as usize) * (self.h as usize)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w as i32 && y < self.y + self.h as i32
    }

    pub fn get(&self, x: i32, y: i32) -> Rgba {
        if !self.contains(x, y) {
            return Rgba::TRANSPARENT;
        }
        let i = ((y - self.y) as usize) * self.w as usize + (x - self.x) as usize;
        self.pixels.get(i).copied().unwrap_or(Rgba::TRANSPARENT)
    }

    /// Write a pixel, growing the cel bounds to include it when necessary.
    pub fn set_extend(&mut self, x: i32, y: i32, c: Rgba) {
        if self.contains(x, y) {
            let i = ((y - self.y) as usize) * self.w as usize + (x - self.x) as usize;
            if let Some(slot) = self.pixels.get_mut(i) {
                *slot = c;
            }
            return;
        }
        if c.3 == 0 {
            return; // erasing outside the cel is a no-op
        }
        let nx = self.x.min(x);
        let ny = self.y.min(y);
        let nx2 = (self.x + self.w as i32).max(x + 1);
        let ny2 = (self.y + self.h as i32).max(y + 1);
        let (nw, nh) = ((nx2 - nx) as u32, (ny2 - ny) as u32);
        let mut pixels = vec![Rgba::TRANSPARENT; (nw * nh) as usize];
        let dx = self.x - nx;
        let dy = self.y - ny;
        for yy in 0..self.h as usize {
            for xx in 0..self.w as usize {
                let src = self.pixels.get(yy * self.w as usize + xx).copied();
                if let Some(c) = src {
                    let tx = xx + dx as usize;
                    let ty = yy + dy as usize;
                    pixels[ty * nw as usize + tx] = c;
                }
            }
        }
        self.x = nx;
        self.y = ny;
        self.w = nw;
        self.h = nh;
        self.pixels = pixels;
        self.set_extend(x, y, c);
    }
}

/// Per-layer blend mode (how this layer composites over those below it).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    Add,
    Subtract,
    Divide,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    /// 0..255, layer opacity.
    pub opacity: u8,
    #[serde(default)]
    pub blend: BlendMode,
    /// Nesting depth: 0 = top level, 1 = inside a group, and so on.
    #[serde(default)]
    pub child_level: u16,
    /// A group layer holds the following layers with a deeper `child_level`.
    #[serde(default)]
    pub group: bool,
    /// Painted pixels are ignored (and its children's) when false.
    #[serde(default = "yes")]
    pub editable: bool,
    /// Per-pixel alpha multiplier applied when compositing.
    #[serde(default)]
    pub mask: Option<Mask>,
    /// Aseprite's background flag (the opaque bottom layer).
    #[serde(default)]
    pub background: bool,
    /// Aseprite's reference-layer flag: drawn while tracing, never painted or
    /// exported.
    #[serde(default)]
    pub reference: bool,
    /// New frames on this layer link to the previous frame's cel.
    #[serde(default)]
    pub prefer_linked: bool,
    /// Aseprite's "lock movement": content cannot be cut/moved/transformed.
    #[serde(default)]
    pub lock_movement: bool,
    /// A group layer is displayed collapsed in the layer list.
    #[serde(default)]
    pub collapsed: bool,
    /// Tilemap layer (the cel holds tile references as well as pixels).
    #[serde(default)]
    pub tilemap: bool,
    /// Tileset ID for tilemap layers (0 = none).
    #[serde(default)]
    pub tileset: u32,
    /// Aseprite layer UUID (written when any layer has one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<[u8; 16]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserData>,
}

fn yes() -> bool {
    true
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            name: "Layer".into(),
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
            child_level: 0,
            group: false,
            editable: true,
            mask: None,
            background: false,
            reference: false,
            prefer_linked: false,
            lock_movement: false,
            collapsed: false,
            tilemap: false,
            tileset: 0,
            uuid: None,
            user_data: None,
        }
    }
}

/// Per-layer alpha mask, row-major like a cel: 0 hides, 255 keeps.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mask {
    pub pixels: Vec<u8>,
}

impl Mask {
    /// Rebuild the mask for a new size/orientation; `f` maps old → new coords.
    fn map(&self, ow: u32, oh: u32, nw: u32, nh: u32, f: impl Fn(u32, u32) -> (u32, u32)) -> Mask {
        let mut pixels = vec![255; (nw * nh) as usize];
        for y in 0..oh {
            for x in 0..ow {
                let (nx, ny) = f(x, y);
                if nx < nw && ny < nh {
                    pixels[(ny * nw + nx) as usize] = self.pixels[(y * ow + x) as usize];
                }
            }
        }
        Mask { pixels }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub duration_ms: u16,
}

/// How a tagged range plays back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayDirection {
    #[default]
    Forward,
    Reverse,
    PingPong,
    /// Ping-pong that starts from the tag's last frame (Aseprite's byte 3).
    PingPongReverse,
}

impl PlayDirection {
    pub fn name(self) -> &'static str {
        match self {
            PlayDirection::Forward => "forward",
            PlayDirection::Reverse => "reverse",
            PlayDirection::PingPong => "pingpong",
            PlayDirection::PingPongReverse => "pingpong-reverse",
        }
    }

    /// Aseprite's tag direction byte: 0 forward, 1 reverse, 2 ping-pong,
    /// 3 ping-pong reverse.
    pub fn from_ase(byte: u8) -> Self {
        match byte {
            1 => PlayDirection::Reverse,
            2 => PlayDirection::PingPong,
            3 => PlayDirection::PingPongReverse,
            _ => PlayDirection::Forward,
        }
    }

    pub fn to_ase(self) -> u8 {
        match self {
            PlayDirection::Forward => 0,
            PlayDirection::Reverse => 1,
            PlayDirection::PingPong => 2,
            PlayDirection::PingPongReverse => 3,
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
            "forward" | "fwd" | "f" => Some(PlayDirection::Forward),
            "reverse" | "rev" | "back" | "r" => Some(PlayDirection::Reverse),
            "pingpong" | "pp" | "bounce" | "b" => Some(PlayDirection::PingPong),
            "pingpong-reverse" | "ppr" | "bounceback" => Some(PlayDirection::PingPongReverse),
            _ => None,
        }
    }
}

/// A named, inclusive frame range ("walk", "idle", ...).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    #[serde(default = "tag_color")]
    pub color: Rgba,
    /// Direction the tag plays in; `0` repeats forever in `.ase`.
    #[serde(default)]
    pub direction: PlayDirection,
    #[serde(default)]
    pub repeat: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserData>,
}

fn tag_color() -> Rgba {
    Rgba(120, 170, 255, 255)
}

/// Where an opaque `.ase` chunk sat relative to the chunks vixel understands.
/// User-data chunks attach to the preceding object, so preserving this anchor
/// keeps slices, tag/layer/cel user data and unknown chunks attached across an
/// open→save round trip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkAnchor {
    #[default]
    FrameStart,
    Layer(u16),
    Cel(u16),
    Palette,
    Tags,
    Tileset(u16),
    /// No matching known chunk was written (e.g. the tileset writer is not
    /// implemented yet): emit at the end of the frame.
    End,
}

/// An `.ase` chunk this build does not interpret, kept so saving does not
/// destroy data it has not learned yet (slices, user data, ICC, ...).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawChunk {
    pub frame: usize,
    pub kind: u16,
    #[serde(default)]
    pub body: Vec<u8>,
    #[serde(default)]
    pub after: ChunkAnchor,
}

/// Aseprite user data (0x2020): text, colour and an opaque property payload.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserData {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgba>,
    /// Property payload exactly as stored (DWORD size + maps).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<u8>,
    /// Raw flags, so unknown flag bits survive a round trip.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub flags: u32,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// An editable text object, stored in a cel's user-data properties and
/// rasterised with the bitmap font.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextObject {
    pub text: String,
    pub font: String,
    /// Requested pixel size (the bitmap font uses multiples of 8).
    pub size: u32,
    /// 0 = left, 1 = centre, 2 = right.
    pub align: u8,
    pub antialias: bool,
    pub x: i32,
    pub y: i32,
}

impl UserData {
    pub fn text_object(&self) -> Option<TextObject> {
        let maps = crate::properties::parse(&self.properties)?;
        let map = maps.iter().find(|m| m.key == 0).or_else(|| maps.first())?;
        let get = |name: &str| map.props.iter().find(|(n, _)| n == name).map(|(_, v)| v);
        let text = get("text")?.as_string()?.to_string();
        Some(TextObject {
            text,
            font: get("font")
                .and_then(|v| v.as_string())
                .unwrap_or("vixel")
                .to_string(),
            size: get("size").and_then(|v| v.as_i32()).unwrap_or(8).max(1) as u32,
            align: get("align")
                .and_then(|v| v.as_i32())
                .unwrap_or(0)
                .clamp(0, 2) as u8,
            antialias: get("antialias").and_then(|v| v.as_bool()).unwrap_or(false),
            x: get("x").and_then(|v| v.as_i32()).unwrap_or(0),
            y: get("y").and_then(|v| v.as_i32()).unwrap_or(0),
        })
    }

    /// Store a text object in map key 0, preserving any other properties.
    pub fn set_text_object(&mut self, object: &TextObject) -> bool {
        let mut maps = if self.properties.is_empty() {
            Vec::new()
        } else {
            match crate::properties::parse(&self.properties) {
                Some(m) => m,
                None => return false,
            }
        };
        let index = match maps.iter().position(|m| m.key == 0) {
            Some(i) => i,
            None => {
                maps.push(crate::properties::PropertyMap {
                    key: 0,
                    props: Vec::new(),
                });
                maps.len() - 1
            }
        };
        let props = &mut maps[index].props;
        let mut put = |name: &str, value: crate::properties::PropertyValue| {
            if let Some(slot) = props.iter_mut().find(|(n, _)| n == name) {
                slot.1 = value;
            } else {
                props.push((name.to_string(), value));
            }
        };
        use crate::properties::PropertyValue as V;
        put("text", V::String(object.text.clone()));
        put("font", V::String(object.font.clone()));
        put("size", V::Int32(object.size as i32));
        put("align", V::Int32(i32::from(object.align)));
        put("antialias", V::Bool(object.antialias));
        put("x", V::Int32(object.x));
        put("y", V::Int32(object.y));
        self.properties = crate::properties::write(&maps);
        self.flags |= 4;
        true
    }
}

/// One entry of the `.ase` external-files chunk (0x2008).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExternalFile {
    pub id: u32,
    /// 0 palette, 1 tileset, 2 extension name, 3 tile management.
    pub kind: u8,
    #[serde(default)]
    pub name: String,
}

/// One slice key: valid from `frame` to the end of the animation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SliceKey {
    pub frame: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<(i32, i32, u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<(i32, i32)>,
}

/// An Aseprite slice (0x2022), including 9-patch/pivot data and its user-data
/// colour.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Slice {
    pub name: String,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub nine_patch: bool,
    #[serde(default)]
    pub pivot: bool,
    #[serde(default)]
    pub keys: Vec<SliceKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserData>,
}

/// The `.ase` colour-profile chunk (0x2007), kept verbatim.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AseProfile {
    /// 0 = none, 1 = sRGB, 2 = embedded ICC.
    pub kind: u16,
    /// Bit 1 = the fixed gamma applies.
    pub flags: u16,
    /// 16.16 fixed-point gamma, decoded to a float.
    pub gamma: f32,
    /// ICC profile bytes when `kind == 2`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub icc: Vec<u8>,
}

/// A sprite: a grid of cels indexed `[layer][frame]`.
///
/// `layers[0]` is the topmost layer (drawn last, matching Aseprite's list).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub frames: Vec<Frame>,
    pub cels: Vec<Vec<Cel>>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    /// Declared colour mode. `Rgba` keeps colour; `Grayscale` is applied to the
    /// pixels when chosen, and `Indexed` resolves every pixel to `palette`.
    #[serde(default)]
    pub color_mode: ColorMode,
    /// Document palette. Indexed sprites only paint colours from here; the
    /// entry at `transparent_index` means transparent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette: Vec<Rgba>,
    /// Optional per-entry palette names, parallel to `palette`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette_names: Vec<String>,
    #[serde(default)]
    pub transparent_index: u8,
    /// Pixel aspect ratio (width:height). Aseprite calls it the pixel ratio;
    /// it is metadata, the pixels are not resampled.
    #[serde(default = "unit_pixel_ratio")]
    pub pixel_aspect: (u16, u16),
    /// Sprite grid bounds (0,0 = none), the same thing Aseprite's sprite
    /// properties dialog shows.
    #[serde(default)]
    pub grid: (u32, u32),
    /// Grid origin in sprite pixels (Aseprite writes it as two SHORTs).
    #[serde(default)]
    pub grid_origin: (i32, i32),
    /// Which transfer function the pixel values are in.
    #[serde(default)]
    pub color_profile: ColorProfile,
    /// Embedded `.ase` colour profile (sRGB/ICC/gamma), preserved on save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ase_profile: Option<AseProfile>,
    /// Opaque `.ase` chunks (slices, user data, ICC, ...) retained verbatim
    /// so an open→save round trip does not discard them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_chunks: Vec<RawChunk>,
    /// Sprite-level user data (0x2020 after the palette chunk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserData>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slices: Vec<Slice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_files: Vec<ExternalFile>,
    /// Retained tilesets (embedded image or external reference), keyed by ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tilesets: Vec<Tileset>,
}

/// Atlas offset for destination pixel `(dx, dy)` of `tile`. Aseprite applies
/// the diagonal flip first, then horizontal, then vertical.
pub(crate) fn tile_pixel_offset(
    ts: &Tileset,
    tile: u32,
    dx: u32,
    dy: u32,
    fx: bool,
    fy: bool,
    fd: bool,
) -> Option<usize> {
    let (mut sx, mut sy) = (dx, dy);
    if fd {
        (sx, sy) = (sy, sx);
    }
    if fx {
        sx = ts.tile_w.checked_sub(1)?.checked_sub(sx)?;
    }
    if fy {
        sy = ts.tile_h.checked_sub(1)?.checked_sub(sy)?;
    }
    if sx >= ts.tile_w || sy >= ts.tile_h {
        return None;
    }
    Some(((tile * ts.tile_h + sy) * ts.tile_w + sx) as usize)
}

/// Rasterise tile refs into a cel-sized pixel buffer.
pub fn render_tilemap(ts: &Tileset, tm: &TilemapCel) -> (u32, u32, Vec<Rgba>) {
    let tiles_w = tm.tiles_w.max(1);
    let tiles_h = (tm.tiles.len() as u32).div_ceil(tiles_w).max(1);
    let (w, h) = (tiles_w * ts.tile_w, tiles_h * ts.tile_h);
    let mut pixels = vec![Rgba::TRANSPARENT; (w * h) as usize];
    for (i, tile_ref) in tm.tiles.iter().enumerate() {
        let Some(tile) = tile_ref.id.checked_sub(ts.base_index) else {
            continue;
        };
        if tile_ref.id == 0 || tile >= ts.num_tiles {
            continue;
        }
        let tx = (i as u32 % tiles_w) * ts.tile_w;
        let ty = (i as u32 / tiles_w) * ts.tile_h;
        for dy in 0..ts.tile_h {
            for dx in 0..ts.tile_w {
                let Some(off) = tile_pixel_offset(
                    ts,
                    tile,
                    dx,
                    dy,
                    tile_ref.flip_x,
                    tile_ref.flip_y,
                    tile_ref.flip_diagonal,
                ) else {
                    continue;
                };
                let src = *ts.image.get(off).unwrap_or(&Rgba::TRANSPARENT);
                let (x, y) = (tx + dx, ty + dy);
                pixels[(y * w + x) as usize] = src;
            }
        }
    }
    (w, h, pixels)
}

/// One Aseprite tileset: id/name/tile size/base index/flags and the atlas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tileset {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub tile_w: u32,
    pub tile_h: u32,
    /// Tile ID of the first atlas tile (usually 1; 0 = empty tile).
    #[serde(default)]
    pub base_index: u32,
    #[serde(default)]
    pub flags: u32,
    pub num_tiles: u32,
    /// Row-major tile atlas, `tile_w` wide and `tile_h * num_tiles` tall.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image: Vec<Rgba>,
    /// External file ID + tileset ID when flag 1 is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<(u32, u32)>,
}

/// The two working spaces this editor understands. A sprite authored in linear
/// light round-trips: converting is a real per-pixel edit, and the declared
/// profile goes into the export metadata.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorProfile {
    #[default]
    Srgb,
    Linear,
}

impl ColorProfile {
    pub fn name(self) -> &'static str {
        match self {
            ColorProfile::Srgb => "srgb",
            ColorProfile::Linear => "linear",
        }
    }

    pub fn parse(s: &str) -> Option<ColorProfile> {
        Some(match s.to_lowercase().as_str() {
            "srgb" | "s-rgb" => ColorProfile::Srgb,
            "linear" | "lin" => ColorProfile::Linear,
            _ => return None,
        })
    }
}

/// What an indexed/gray file means for this sprite.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// Smallest lossless depth for what the sprite actually contains. This is
    /// what an export has always done, and the default.
    #[default]
    Auto,
    /// Force 32-bit RGBA.
    Rgba,
    /// Force 16-bit grayscale (and convert the pixels when the mode is chosen).
    Grayscale,
    /// Quantize to a ≤256 document palette and resolve paint to its entries.
    Indexed,
}

/// How RGB pixels are reduced to palette entries when converting to indexed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DitherMode {
    #[default]
    None,
    Bayer2,
    Bayer4,
    Bayer8,
    FloydSteinberg,
}

impl DitherMode {
    pub fn name(self) -> &'static str {
        match self {
            DitherMode::None => "none",
            DitherMode::Bayer2 => "bayer2",
            DitherMode::Bayer4 => "bayer4",
            DitherMode::Bayer8 => "bayer8",
            DitherMode::FloydSteinberg => "floyd",
        }
    }

    pub fn parse(s: &str) -> Option<DitherMode> {
        Some(match s.to_lowercase().replace(['_', '-'], "").as_str() {
            "none" | "off" | "0" => DitherMode::None,
            "bayer2" | "2" => DitherMode::Bayer2,
            "bayer4" | "4" => DitherMode::Bayer4,
            "bayer8" | "8" => DitherMode::Bayer8,
            "floyd" | "fs" | "floydsteinberg" => DitherMode::FloydSteinberg,
            _ => return None,
        })
    }

    fn bayer_size(self) -> Option<usize> {
        match self {
            DitherMode::Bayer2 => Some(2),
            DitherMode::Bayer4 => Some(4),
            DitherMode::Bayer8 => Some(8),
            _ => None,
        }
    }
}

impl ColorMode {
    pub fn name(self) -> &'static str {
        match self {
            ColorMode::Auto => "auto",
            ColorMode::Rgba => "rgba",
            ColorMode::Grayscale => "grayscale",
            ColorMode::Indexed => "indexed",
        }
    }

    pub fn parse(s: &str) -> Option<ColorMode> {
        Some(match s.to_lowercase().as_str() {
            "auto" | "default" => ColorMode::Auto,
            "rgba" | "rgb" | "color" | "colour" | "32" => ColorMode::Rgba,
            "gray" | "grey" | "grayscale" | "greyscale" | "16" => ColorMode::Grayscale,
            "indexed" | "index" | "8" => ColorMode::Indexed,
            _ => return None,
        })
    }
}

fn unit_pixel_ratio() -> (u16, u16) {
    (1, 1)
}

impl Sprite {
    pub fn new(width: u32, height: u32) -> Self {
        let layers = vec![Layer {
            name: "Layer 1".into(),
            user_data: None,
            ..Default::default()
        }];
        let frames = vec![Frame { duration_ms: 100 }];
        let cels = vec![vec![Cel::blank(width, height)]];
        Self {
            width,
            height,
            layers,
            frames,
            cels,
            tags: Vec::new(),
            color_mode: ColorMode::Auto,
            palette: Vec::new(),
            palette_names: Vec::new(),
            transparent_index: 0,
            pixel_aspect: (1, 1),
            grid: (0, 0),
            grid_origin: (0, 0),
            color_profile: ColorProfile::Srgb,
            ase_profile: None,
            raw_chunks: Vec::new(),
            user_data: None,
            slices: Vec::new(),
            external_files: Vec::new(),
            tilesets: Vec::new(),
        }
    }

    pub fn is_indexed(&self) -> bool {
        self.color_mode == ColorMode::Indexed
    }

    /// The palette slot that means transparent. Prefers the stored index when
    /// it really is a transparent entry, then any transparent entry, then the
    /// first free slot (the caller writes the transparent colour there).
    pub fn transparent_slot(&self) -> u8 {
        let ti = self.transparent_index as usize;
        if self.palette.get(ti).is_some_and(|p| p.3 == 0) {
            return self.transparent_index;
        }
        if let Some(i) = self.palette.iter().position(|p| p.3 == 0) {
            return i as u8;
        }
        if self.palette.len() < 256 {
            self.palette.len() as u8
        } else {
            255
        }
    }

    /// Snap a paint colour to the indexed palette. Transparent paint stays
    /// transparent; everything else becomes the nearest opaque entry.
    pub fn indexed_color(&self, c: Rgba) -> Rgba {
        if !self.is_indexed() || self.palette.is_empty() {
            return c;
        }
        if c.3 == 0 {
            return Rgba::TRANSPARENT;
        }
        let ti = self.transparent_slot();
        let idx = nearest_palette_index(&self.palette, ti, c);
        self.palette[idx as usize]
    }

    /// Build a palette from the sprite's pixels without touching them. Up to
    /// 255 opaque colours are kept exactly; more go through median cut. The
    /// transparent slot is index 0.
    pub fn build_palette(&mut self) {
        self.materialize_links();
        let mut seen = std::collections::HashSet::new();
        let mut colors = Vec::new();
        for cel in self.cels.iter().flatten() {
            for p in &cel.pixels {
                if p.3 == 0 {
                    continue;
                }
                if seen.insert((p.0, p.1, p.2)) {
                    colors.push(Rgba(p.0, p.1, p.2, 255));
                }
            }
        }
        colors.sort_by_key(|c| (c.0, c.1, c.2));
        colors.dedup();
        let chosen = if colors.len() <= 255 {
            colors
        } else {
            median_cut(colors, 255)
        };
        self.palette = Vec::with_capacity(chosen.len() + 1);
        self.palette.push(Rgba::TRANSPARENT);
        self.palette.extend(chosen);
        self.palette_names = vec![String::new(); self.palette.len()];
        self.transparent_index = 0;
    }

    /// Build a palette from the sprite's pixels and snap every cel to it.
    pub fn quantize_to_palette(&mut self) {
        self.build_palette();
        self.color_mode = ColorMode::Indexed;
        self.remap_to_palette();
    }

    /// Reduce every cel to the current palette. `None` snaps to the nearest
    /// entry; ordered modes add a Bayer threshold before the lookup; Floyd-
    /// Steinberg diffuses the quantization error.
    pub fn dither_to_palette(&mut self, mode: DitherMode) {
        if self.palette.is_empty() {
            return;
        }
        self.materialize_links();
        self.color_mode = ColorMode::Indexed;
        if mode == DitherMode::None {
            self.remap_to_palette();
            return;
        }
        let palette = self.palette.clone();
        let ti = self.transparent_slot();
        let matrix = mode.bayer_size().map(bayer_matrix);
        for cel in self.cels.iter_mut().flatten() {
            if let Some(matrix) = matrix.as_ref() {
                dither_ordered(
                    &mut cel.pixels,
                    cel.w as usize,
                    &palette,
                    ti,
                    matrix,
                    cel.x,
                    cel.y,
                );
            } else {
                dither_floyd(
                    &mut cel.pixels,
                    cel.w as usize,
                    cel.h as usize,
                    &palette,
                    ti,
                );
            }
        }
    }

    /// Snap every cel pixel to the current palette (transparent pixels become
    /// fully transparent). Idempotent once the pixels are palette entries.
    pub fn remap_to_palette(&mut self) {
        if self.palette.is_empty() {
            return;
        }
        self.materialize_links();
        let palette = self.palette.clone();
        let ti = self.transparent_slot();
        for cel in self.cels.iter_mut().flatten() {
            for p in cel.pixels.iter_mut() {
                *p = if p.3 == 0 {
                    Rgba::TRANSPARENT
                } else {
                    palette[nearest_palette_index(&palette, ti, *p) as usize]
                };
            }
        }
    }

    /// Convert the pixel values between transfer functions. Colours that are
    /// already in the target space are left alone, so this is idempotent.
    pub fn convert_profile(&mut self, to: ColorProfile) {
        if self.color_profile == to {
            return;
        }
        self.materialize_links();
        let f = match to {
            ColorProfile::Linear => crate::color::srgb_to_linear,
            ColorProfile::Srgb => crate::color::linear_to_srgb,
        };
        for cel in self.cels.iter_mut().flatten() {
            for p in cel.pixels.iter_mut() {
                if p.3 == 0 {
                    continue;
                }
                *p = Rgba(f(p.0), f(p.1), f(p.2), p.3);
            }
        }
        self.color_profile = to;
    }

    /// Convert every pixel to its luminance, keeping alpha. Colour mode
    /// `Grayscale` is a real edit, not just a label.
    pub fn to_grayscale(&mut self) {
        self.materialize_links();
        for cel in self.cels.iter_mut().flatten() {
            for p in cel.pixels.iter_mut() {
                if p.3 == 0 {
                    continue;
                }
                let l = ((p.0 as u32 * 299 + p.1 as u32 * 587 + p.2 as u32 * 114) / 1000) as u8;
                *p = Rgba(l, l, l, p.3);
            }
        }
        self.color_mode = ColorMode::Grayscale;
    }

    pub fn area(&self) -> usize {
        (self.width * self.height) as usize
    }

    /// Upgrade old `.vpx` cels (which had no bounds) to canvas-sized cels.
    pub fn normalize_cels(&mut self) {
        let (w, h) = (self.width, self.height);
        let area = (w * h) as usize;
        for cel in self.cels.iter_mut().flatten() {
            if cel.link.is_some() {
                continue;
            }
            if cel.w == 0 || cel.h == 0 || cel.pixels.len() != cel.w as usize * cel.h as usize {
                cel.x = 0;
                cel.y = 0;
                cel.w = w;
                cel.h = h;
                cel.pixels.resize(area, Rgba::TRANSPARENT);
            }
        }
    }

    /// Resolve a cel's link chain to the cel that owns the pixels.
    fn content(&self, layer: usize, frame: usize) -> &Cel {
        let mut cel = &self.cels[layer][frame];
        let mut cur = frame;
        let mut hops = 0;
        while let Some(t) = cel.link {
            if t >= self.frames.len() || t == cur || hops > self.frames.len() {
                break;
            }
            cel = &self.cels[layer][t];
            cur = t;
            hops += 1;
        }
        cel
    }

    /// Re-render a tilemap cel's pixels from its retained tile references.
    pub fn rasterize_tilemap(&mut self, layer: usize, frame: usize) -> bool {
        let Some(l) = self.layers.get(layer) else {
            return false;
        };
        if !l.tilemap {
            return false;
        }
        let ts_id = l.tileset;
        let Some(ts) = self
            .tilesets
            .iter()
            .find(|t| t.id == ts_id)
            .filter(|t| !t.image.is_empty())
            .cloned()
        else {
            return false;
        };
        let Some(tm) = self
            .cels
            .get(layer)
            .and_then(|f| f.get(frame))
            .and_then(|c| c.tilemap.clone())
        else {
            return false;
        };
        let (w, h, pixels) = render_tilemap(&ts, &tm);
        if let Some(cel) = self.cels.get_mut(layer).and_then(|f| f.get_mut(frame)) {
            cel.w = w;
            cel.h = h;
            cel.pixels = pixels;
            cel.link = None;
            true
        } else {
            false
        }
    }

    /// Replace a linked cel with an independent copy of its content.
    fn materialize(&mut self, layer: usize, frame: usize) {
        let Some(target) = self.cels[layer][frame].link else {
            return;
        };
        if target >= self.frames.len() {
            self.cels[layer][frame].link = None;
            return;
        }
        let mut cel = self.content(layer, target).clone();
        let z = self.cels[layer][frame].z_index;
        let op = self.cels[layer][frame].opacity;
        cel.link = None;
        cel.z_index = z;
        cel.opacity = op;
        self.cels[layer][frame] = cel;
    }

    /// Break every link, giving each cel its own copy of the pixels.
    pub fn materialize_links(&mut self) {
        for layer in 0..self.layers.len() {
            for frame in 0..self.frames.len() {
                self.materialize(layer, frame);
            }
        }
    }

    /// Link `frame` to another frame's cel in the same layer.
    pub fn link_cel(&mut self, layer: usize, frame: usize, target: usize) -> bool {
        if layer >= self.cels.len()
            || frame >= self.frames.len()
            || target >= self.frames.len()
            || frame == target
        {
            return false;
        }
        let mut root = target;
        let mut hops = 0;
        while let Some(t) = self.cels[layer][root].link {
            if t >= self.frames.len() || t == root || hops > self.frames.len() {
                break;
            }
            root = t;
            hops += 1;
        }
        if root == frame {
            return false;
        }
        let cel = &mut self.cels[layer][frame];
        let (z, op) = (cel.z_index, cel.opacity);
        cel.link = Some(root);
        cel.x = 0;
        cel.y = 0;
        cel.w = 0;
        cel.h = 0;
        cel.z_index = z;
        cel.opacity = op;
        cel.pixels.clear();
        true
    }

    /// Break one link, copying the shared pixels into this cel.
    pub fn unlink_cel(&mut self, layer: usize, frame: usize) -> bool {
        if self.cels[layer][frame].link.is_none() {
            return false;
        }
        self.materialize(layer, frame);
        true
    }

    pub fn is_linked(&self, layer: usize, frame: usize) -> bool {
        self.cels[layer][frame].link.is_some()
    }

    /// Aseprite's sprite-properties background colour, carried in the sprite's
    /// user-data colour field.
    pub fn background_color(&self) -> Option<Rgba> {
        self.user_data.as_ref().and_then(|u| u.color)
    }

    pub fn set_background_color(&mut self, color: Option<Rgba>) {
        let mut data = self.user_data.clone().unwrap_or_default();
        data.color = color;
        if data.text.is_empty()
            && data.properties.is_empty()
            && data.flags == 0
            && data.color.is_none()
        {
            self.user_data = None;
        } else {
            self.user_data = Some(data);
        }
    }

    pub fn get(&self, layer: usize, frame: usize, x: u32, y: u32) -> Rgba {
        self.content(layer, frame).get(x as i32, y as i32)
    }

    pub fn set(&mut self, layer: usize, frame: usize, x: u32, y: u32, c: Rgba) {
        let c = self.indexed_color(c);
        self.materialize(layer, frame);
        self.cels[layer][frame].set_extend(x as i32, y as i32, c);
    }

    /// Flatten all visible layers of `frame` (top → bottom) into one image.
    /// Groups composite their children first, then apply their own
    /// visibility/opacity/blend/mask to the result.
    pub fn composite(&self, frame: usize) -> Vec<Rgba> {
        self.composite_inner(frame, true)
    }

    /// Composite for export/flatten: reference layers are tracing aids and are
    /// not part of the artwork.
    pub fn export_composite(&self, frame: usize) -> Vec<Rgba> {
        self.composite_inner(frame, false)
    }

    fn composite_inner(&self, frame: usize, include_reference: bool) -> Vec<Rgba> {
        let mut out = vec![Rgba::TRANSPARENT; self.area()];
        self.composite_range(frame, 0, self.layers.len(), &mut out, include_reference);
        out
    }

    /// Composite the sibling run `[start, end)` over `dst`, bottom-most first.
    fn composite_range(
        &self,
        frame: usize,
        start: usize,
        end: usize,
        dst: &mut [Rgba],
        include_reference: bool,
    ) {
        let mut siblings = Vec::new();
        let mut k = start;
        while k < end {
            siblings.push(k);
            k = self.subtree_end(k).min(end);
        }
        // Our list is top-first, so descending index is the base back-to-front
        // order. Aseprite's order is layerIndex + zIndex (larger = in front);
        // with our inverted index that is `-i + z`.
        siblings.sort_by_key(|&i| {
            let z = if self.layers[i].group {
                0
            } else {
                self.cels[i][frame].z_index as i32
            };
            (-(i as i32) + z, z)
        });
        for &i in siblings.iter() {
            let layer = &self.layers[i];
            let stop = self.subtree_end(i).min(end);
            if !layer.visible || layer.opacity == 0 || (!include_reference && layer.reference) {
                continue;
            }
            let op = layer.opacity as u32;
            let cel_op = if layer.group {
                255
            } else {
                self.cels[i][frame].opacity as u32
            };
            if layer.group {
                let mut buf = vec![Rgba::TRANSPARENT; self.area()];
                self.composite_range(frame, i + 1, stop, &mut buf, include_reference);
                for (k, px) in buf.iter().enumerate() {
                    let mut sa = px.3 as u32 * op / 255;
                    if let Some(m) = &layer.mask {
                        sa = sa * *m.pixels.get(k).unwrap_or(&255) as u32 / 255;
                    }
                    if sa > 0 {
                        dst[k] = blend_over(layer.blend, *px, sa, dst[k]);
                    }
                }
                continue;
            }
            let cel = self.content(i, frame);
            let (cw, ch) = (self.width as i32, self.height as i32);
            for yy in 0..cel.h {
                let y = cel.y + yy as i32;
                if y < 0 || y >= ch {
                    continue;
                }
                for xx in 0..cel.w {
                    let x = cel.x + xx as i32;
                    if x < 0 || x >= cw {
                        continue;
                    }
                    let k = (y as u32 * self.width + x as u32) as usize;
                    let px = cel
                        .pixels
                        .get(yy as usize * cel.w as usize + xx as usize)
                        .copied()
                        .unwrap_or(Rgba::TRANSPARENT);
                    let mut sa = px.3 as u32 * op / 255 * cel_op / 255;
                    if let Some(m) = &layer.mask {
                        sa = sa * *m.pixels.get(k).unwrap_or(&255) as u32 / 255;
                    }
                    if sa > 0 {
                        dst[k] = blend_over(layer.blend, px, sa, dst[k]);
                    }
                }
            }
        }
    }

    /// Index one past the last descendant of layer `i`.
    pub fn subtree_end(&self, i: usize) -> usize {
        let level = self.layers[i].child_level;
        let mut k = i + 1;
        while k < self.layers.len() && self.layers[k].child_level > level {
            k += 1;
        }
        k
    }

    /// Can this layer's content be cut, moved or transformed?
    pub fn layer_movable(&self, i: usize) -> bool {
        self.layers
            .get(i)
            .is_some_and(|l| !l.group && !l.lock_movement)
    }

    /// Can `i` (and all its ancestor groups) be painted on right now?
    pub fn layer_editable(&self, i: usize) -> bool {
        let mut ok = self.layers[i].editable
            && self.layers[i].visible
            && !self.layers[i].group
            && !self.layers[i].reference;
        let mut level = self.layers[i].child_level;
        for k in (0..i).rev() {
            if level == 0 {
                break;
            }
            let kl = self.layers[k].child_level;
            if kl < level {
                ok &= self.layers[k].editable && self.layers[k].visible;
                level = kl;
            }
        }
        ok
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.materialize_links();
        let (ow, oh) = (self.width, self.height);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let old = self.cels[layer][frame].clone();
                let mut np = vec![Rgba::TRANSPARENT; (width * height) as usize];
                for y in 0..height.min(self.height) {
                    for x in 0..width.min(self.width) {
                        np[(y * width + x) as usize] = old.get(x as i32, y as i32);
                    }
                }
                self.cels[layer][frame] = Cel {
                    x: 0,
                    y: 0,
                    w: width,
                    h: height,
                    z_index: old.z_index,
                    link: None,
                    pixels: np,
                    opacity: old.opacity,
                    user_data: None,
                    tilemap: None,
                };
            }
        }
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(ow, oh, width, height, |x, y| (x, y)));
            }
        }
        self.width = width;
        self.height = height;
    }

    /// Crop every cel and layer mask to the rect `(cx, cy, w, h)`, clamped to
    /// the current canvas.
    pub fn crop(&mut self, cx: u32, cy: u32, w: u32, h: u32) {
        let (ow, oh) = (self.width, self.height);
        let cx = cx.min(ow.saturating_sub(1));
        let cy = cy.min(oh.saturating_sub(1));
        let w = w.clamp(1, ow - cx);
        let h = h.clamp(1, oh - cy);
        if (cx, cy, w, h) == (0, 0, ow, oh) {
            return;
        }
        self.materialize_links();
        for layer in 0..self.layers.len() {
            for frame in 0..self.frames.len() {
                let old = self.cels[layer][frame].clone();
                let mut np = vec![Rgba::TRANSPARENT; (w * h) as usize];
                for y in 0..h {
                    for x in 0..w {
                        let (sx, sy) = (x + cx, y + cy);
                        if sx < ow && sy < oh {
                            np[(y * w + x) as usize] = old.get(sx as i32, sy as i32);
                        }
                    }
                }
                self.cels[layer][frame] = Cel {
                    x: 0,
                    y: 0,
                    w,
                    h,
                    z_index: old.z_index,
                    link: None,
                    pixels: np,
                    opacity: old.opacity,
                    user_data: None,
                    tilemap: None,
                };
            }
        }
        for layer in &mut self.layers {
            let Some(m) = &layer.mask else { continue };
            let mut pixels = vec![255u8; (w * h) as usize];
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = (x + cx, y + cy);
                    if sx < ow && sy < oh {
                        pixels[(y * w + x) as usize] = m.pixels[(sy * ow + sx) as usize];
                    }
                }
            }
            layer.mask = Some(Mask { pixels });
        }
        self.width = w;
        self.height = h;
    }

    /// Mirror every cel horizontally.
    pub fn flip_h(&mut self) {
        self.materialize_links();
        let (w, h) = (self.width, self.height);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let cel = &mut self.cels[layer][frame];
                let mut np = vec![Rgba::TRANSPARENT; cel.area()];
                for yy in 0..cel.h {
                    for xx in 0..cel.w {
                        let src = (yy * cel.w + xx) as usize;
                        let dst = (yy * cel.w + (cel.w - 1 - xx)) as usize;
                        np[dst] = cel.pixels.get(src).copied().unwrap_or(Rgba::TRANSPARENT);
                    }
                }
                cel.pixels = np;
                cel.x = w as i32 - (cel.x + cel.w as i32);
            }
        }
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(w, h, w, h, |x, y| (w - 1 - x, y)));
            }
        }
    }

    /// Mirror every cel vertically.
    pub fn flip_v(&mut self) {
        self.materialize_links();
        let (w, h) = (self.width, self.height);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let cel = &mut self.cels[layer][frame];
                let mut np = vec![Rgba::TRANSPARENT; cel.area()];
                for yy in 0..cel.h {
                    for xx in 0..cel.w {
                        let src = (yy * cel.w + xx) as usize;
                        let dst = ((cel.h - 1 - yy) * cel.w + xx) as usize;
                        np[dst] = cel.pixels.get(src).copied().unwrap_or(Rgba::TRANSPARENT);
                    }
                }
                cel.pixels = np;
                cel.y = h as i32 - (cel.y + cel.h as i32);
            }
        }
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(w, h, w, h, |x, y| (x, h - 1 - y)));
            }
        }
    }

    /// Rotate every cel 90° clockwise (width/height swap).
    pub fn rotate_cw(&mut self) {
        self.materialize_links();
        let (nw, nh) = (self.height, self.width);
        for frame in 0..self.frames.len() {
            for layer in 0..self.layers.len() {
                let old = self.cels[layer][frame].clone();
                let mut np = vec![Rgba::TRANSPARENT; old.area()];
                for yy in 0..old.h {
                    for xx in 0..old.w {
                        // Rotated global x = height-1-y; local x within the
                        // rotated cel is h-1-yy; local y is xx.
                        let nx = old.h as i32 - 1 - yy as i32;
                        let ny = xx as i32;
                        let src = (yy * old.w + xx) as usize;
                        let dst = (ny * old.h as i32 + nx) as usize;
                        np[dst] = old.pixels.get(src).copied().unwrap_or(Rgba::TRANSPARENT);
                    }
                }
                let cel = &mut self.cels[layer][frame];
                cel.x = self.height as i32 - (old.y + old.h as i32);
                cel.y = old.x;
                cel.w = old.h;
                cel.h = old.w;
                cel.pixels = np;
            }
        }
        let (w, h) = (self.width, self.height);
        for layer in &mut self.layers {
            if let Some(m) = &layer.mask {
                layer.mask = Some(m.map(w, h, nw, nh, |x, y| (h - 1 - y, x)));
            }
        }
        self.width = nw;
        self.height = nh;
    }

    fn shift_links_insert(&mut self, at: usize) {
        for cel in self.cels.iter_mut().flatten() {
            if let Some(t) = cel.link
                && t >= at
            {
                cel.link = Some(t + 1);
            }
        }
    }

    /// Insert a frame just after `idx`; prefer-linked layers link to `idx`.
    pub fn add_frame_after(&mut self, idx: usize) {
        let at = idx + 1;
        self.shift_links_insert(at);
        for l in 0..self.cels.len() {
            let cel = if self.layers[l].prefer_linked {
                let mut c = Cel::blank(0, 0);
                c.link = Some(idx);
                c
            } else {
                Cel::blank(self.width, self.height)
            };
            self.cels[l].insert(at, cel);
        }
        self.frames.insert(at, Frame { duration_ms: 100 });
        self.shift_tags_on_insert(idx);
    }

    /// Duplicate frame `idx` (with its cels and duration) right after it.
    pub fn duplicate_frame(&mut self, idx: usize) {
        let clones: Vec<Cel> = (0..self.layers.len())
            .map(|l| self.cels[l][idx].clone())
            .collect();
        for (l, cel) in clones.into_iter().enumerate() {
            self.cels[l].insert(idx + 1, cel);
        }
        self.frames.insert(idx + 1, self.frames[idx]);
        self.shift_links_insert(idx + 1);
        self.shift_tags_on_insert(idx);
    }

    pub fn remove_frame(&mut self, idx: usize) -> bool {
        if self.frames.len() <= 1 {
            return false;
        }
        // Links into the removed frame would dangle: materialise them first.
        for l in 0..self.layers.len() {
            for f in 0..self.frames.len() {
                if self.cels[l][f].link == Some(idx) {
                    self.materialize(l, f);
                }
            }
        }
        for layer in self.cels.iter_mut() {
            layer.remove(idx);
        }
        self.frames.remove(idx);
        for cel in self.cels.iter_mut().flatten() {
            if let Some(t) = cel.link
                && t > idx
            {
                cel.link = Some(t - 1);
            }
        }
        self.shift_tags_on_remove(idx);
        true
    }

    /// Move frame `from` to index `to` (all layers' cels move with it).
    pub fn move_frame(&mut self, from: usize, to: usize) -> bool {
        if from >= self.frames.len() || to >= self.frames.len() || from == to {
            return false;
        }
        let map = |t: usize| {
            if t == from {
                to
            } else if from < t && t <= to {
                t - 1
            } else if to <= t && t < from {
                t + 1
            } else {
                t
            }
        };
        for cel in self.cels.iter_mut().flatten() {
            if let Some(t) = cel.link {
                cel.link = Some(map(t));
            }
        }
        for layer in &mut self.cels {
            let cel = layer.remove(from);
            layer.insert(to, cel);
        }
        let frame = self.frames.remove(from);
        self.frames.insert(to, frame);
        true
    }

    /// Keep tags within the frame range; drop any that collapse.
    pub fn clamp_tags(&mut self) {
        let last = self.frames.len().saturating_sub(1);
        self.tags.retain_mut(|t| {
            t.to = t.to.min(last);
            t.from = t.from.min(t.to);
            true
        });
    }

    /// A frame was inserted at `idx + 1`: ranges at/after it move, and a range
    /// spanning it grows.
    fn shift_tags_on_insert(&mut self, idx: usize) {
        for t in &mut self.tags {
            if t.from > idx {
                t.from += 1;
                t.to += 1;
            } else if t.to >= idx {
                t.to += 1;
            }
        }
    }

    fn shift_tags_on_remove(&mut self, idx: usize) {
        self.tags.retain_mut(|t| {
            if t.from > idx {
                t.from -= 1;
                t.to -= 1;
            } else if t.to >= idx {
                t.to = t.to.saturating_sub(1);
                if t.to < t.from {
                    return false;
                }
            }
            true
        });
    }

    /// Insert a blank layer just above `idx` (towards the top of the stack).
    pub fn add_layer_after(&mut self, idx: usize) {
        let name = format!("Layer {}", self.layers.len() + 1);
        let child_level = self.layers[idx].child_level;
        let use_uuid = self.layers.iter().any(|l| l.uuid.is_some());
        self.layers.insert(
            idx + 1,
            Layer {
                name,
                child_level,
                uuid: use_uuid.then(|| mint_uuid(idx)),
                user_data: None,
                ..Default::default()
            },
        );
        let cel = Cel::blank(self.width, self.height);
        self.cels.insert(idx + 1, vec![cel; self.frames.len()]);
    }

    /// Insert an empty group just above `idx`, at the same nesting level.
    pub fn add_group_after(&mut self, idx: usize) {
        let name = format!("Group {}", self.layers.len() + 1);
        let child_level = self.layers[idx].child_level;
        let use_uuid = self.layers.iter().any(|l| l.uuid.is_some());
        self.layers.insert(
            idx + 1,
            Layer {
                name,
                group: true,
                child_level,
                uuid: use_uuid.then(|| mint_uuid(idx)),
                user_data: None,
                ..Default::default()
            },
        );
        let cel = Cel::blank(self.width, self.height);
        self.cels.insert(idx + 1, vec![cel; self.frames.len()]);
    }

    /// Duplicate a normal layer (with its cels) directly above itself. Groups
    /// are not duplicated yet.
    pub fn duplicate_layer(&mut self, idx: usize) -> bool {
        if idx >= self.layers.len() || self.layers[idx].group {
            return false;
        }
        let mut layer = self.layers[idx].clone();
        layer.name = format!("{} copy", layer.name);
        if layer.uuid.is_some() {
            layer.uuid = Some(mint_uuid(idx));
        }
        let cels = self.cels[idx].clone();
        self.layers.insert(idx, layer);
        self.cels.insert(idx, cels);
        true
    }

    /// Merge layer `up` into the layer directly below it, baking opacity,
    /// blend mode and mask into the pixels. The editor checks that both are
    /// visible, non-group siblings before calling.
    pub fn merge_down(&mut self, up: usize) {
        let down = up + 1;
        for frame in 0..self.frames.len() {
            let mut buf = vec![Rgba::TRANSPARENT; self.area()];
            self.composite_range(frame, up, down + 1, &mut buf, true);
            self.cels[down][frame] = Cel {
                x: 0,
                y: 0,
                w: self.width,
                h: self.height,
                z_index: 0,
                link: None,
                pixels: buf,
                opacity: 255,
                user_data: None,
                tilemap: None,
            };
        }
        self.layers[down].opacity = 255;
        self.layers[down].blend = BlendMode::Normal;
        self.layers[down].mask = None;
        self.remove_layer(up);
    }

    /// Collapse every visible layer into one flattened layer per frame.
    /// Hidden layers and masks are discarded, as in Aseprite's Flatten.
    pub fn flatten(&mut self) -> bool {
        let flat = self.layers.len() == 1
            && !self.layers[0].group
            && self.layers[0].mask.is_none()
            && self.layers[0].opacity == 255
            && self.layers[0].blend == BlendMode::Normal
            && self.layers[0].child_level == 0
            && self.layers[0].visible
            && !self.layers[0].reference;
        if flat {
            return false;
        }
        let mut cels = Vec::with_capacity(self.frames.len());
        for frame in 0..self.frames.len() {
            cels.push(Cel {
                x: 0,
                y: 0,
                w: self.width,
                h: self.height,
                z_index: 0,
                link: None,
                pixels: self.export_composite(frame),
                opacity: 255,
                user_data: None,
                tilemap: None,
            });
        }
        self.layers = vec![Layer {
            name: "Flattened".into(),
            user_data: None,
            ..Layer::default()
        }];
        self.cels = vec![cels];
        true
    }

    /// Remove layer `idx`; a group takes its whole subtree with it.
    pub fn remove_layer(&mut self, idx: usize) -> bool {
        if idx >= self.layers.len() {
            return false;
        }
        let end = self.subtree_end(idx);
        if end - idx >= self.layers.len() {
            return false;
        }
        self.layers.drain(idx..end);
        self.cels.drain(idx..end);
        self.normalize_levels();
        true
    }

    /// Move layer `from` (with its subtree) so it ends up starting at `to`.
    /// Returns the final start index, or `None` if the move is invalid.
    pub fn move_layer(&mut self, from: usize, to: usize) -> Option<usize> {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return None;
        }
        let end = self.subtree_end(from);
        if to > from && to < end {
            return None; // cannot move a group into itself
        }
        let n = end - from;
        let layers: Vec<Layer> = self.layers.drain(from..end).collect();
        let cels = self.cels.drain(from..end).collect::<Vec<_>>();
        let at = if to > from { to + 1 - n } else { to }.min(self.layers.len());
        self.layers.splice(at..at, layers);
        self.cels.splice(at..at, cels);
        self.normalize_levels();
        Some(at)
    }

    /// Clamp every `child_level` to a structurally valid nesting depth.
    pub fn normalize_levels(&mut self) {
        let mut prev = 0u16;
        for i in 0..self.layers.len() {
            let max = if i == 0 {
                0
            } else if self.layers[i - 1].group {
                prev + 1
            } else {
                prev
            };
            let level = self.layers[i].child_level.min(max);
            self.layers[i].child_level = level;
            prev = level;
        }
    }
}

/// Straight-alpha blend of `src` (effective alpha `sa`) over `dst` with `mode`.
pub(crate) fn blend_over(mode: BlendMode, src: Rgba, sa: u32, dst: Rgba) -> Rgba {
    let as_ = sa as f32 / 255.0;
    let ab = dst.3 as f32 / 255.0;
    let cs = [
        src.0 as f32 / 255.0,
        src.1 as f32 / 255.0,
        src.2 as f32 / 255.0,
    ];
    let cb = [
        dst.0 as f32 / 255.0,
        dst.1 as f32 / 255.0,
        dst.2 as f32 / 255.0,
    ];
    let ao = as_ + ab * (1.0 - as_);
    if ao <= 0.0 {
        return Rgba::TRANSPARENT;
    }
    let b = blend_fn(mode, cs, cb);
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        // Premultiplied source-over with the blend function (W3C compositing).
        let co = as_ * (1.0 - ab) * cs[i] + as_ * ab * b[i] + (1.0 - as_) * ab * cb[i];
        out[i] = (co / ao).clamp(0.0, 1.0);
    }
    Rgba(
        (out[0] * 255.0).round() as u8,
        (out[1] * 255.0).round() as u8,
        (out[2] * 255.0).round() as u8,
        (ao * 255.0).round() as u8,
    )
}

fn blend_fn(mode: BlendMode, cs: [f32; 3], cb: [f32; 3]) -> [f32; 3] {
    use BlendMode::*;
    match mode {
        Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        Color => set_lum(cs, lum(cb)),
        Luminosity => set_lum(cb, lum(cs)),
        _ => [
            separable(mode, cs[0], cb[0]),
            separable(mode, cs[1], cb[1]),
            separable(mode, cs[2], cb[2]),
        ],
    }
}

/// The channel-wise part of the blend function (W3C compositing + Aseprite's
/// additions: Add, Subtract, Divide).
fn separable(mode: BlendMode, cs: f32, cb: f32) -> f32 {
    use BlendMode::*;
    match mode {
        Normal => cs,
        Multiply => cs * cb,
        Screen => cs + cb - cs * cb,
        Overlay => {
            if cb <= 0.5 {
                2.0 * cs * cb
            } else {
                1.0 - 2.0 * (1.0 - cs) * (1.0 - cb)
            }
        }
        Darken => cs.min(cb),
        Lighten => cs.max(cb),
        ColorDodge => {
            if cb <= 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        HardLight => {
            if cs <= 0.5 {
                2.0 * cs * cb
            } else {
                1.0 - 2.0 * (1.0 - cs) * (1.0 - cb)
            }
        }
        SoftLight => {
            let d = if cb <= 0.25 {
                ((16.0 * cb - 12.0) * cb + 4.0) * cb
            } else {
                cb.sqrt()
            };
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        Difference => (cb - cs).abs(),
        Exclusion => cb + cs - 2.0 * cb * cs,
        Add => (cs + cb).min(1.0),
        Subtract => (cb - cs).max(0.0),
        // Aseprite's Divide: backdrop / source, clamped. Backdrop 0 stays 0;
        // a zero source with a non-zero backdrop saturates to 1.
        Divide => {
            if cb <= 0.0 {
                0.0
            } else {
                (cb / cs).min(1.0)
            }
        }
        Hue | Saturation | Color | Luminosity => unreachable!(),
    }
}

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn clip_color(mut c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    if n < 0.0 {
        for v in &mut c {
            *v = l + ((*v - l) * l) / (l - n);
        }
    }
    if x > 1.0 {
        for v in &mut c {
            *v = l + ((*v - l) * (1.0 - l)) / (x - l);
        }
    }
    c
}

fn set_lum(mut c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    for v in &mut c {
        *v += d;
    }
    clip_color(c)
}

fn set_sat(mut c: [f32; 3], s: f32) -> [f32; 3] {
    let minv = c[0].min(c[1]).min(c[2]);
    let maxv = c[0].max(c[1]).max(c[2]);
    let range = maxv - minv;
    if range > 0.0 {
        for v in &mut c {
            *v = ((*v - minv) * s) / range;
        }
    } else {
        c = [0.0; 3];
    }
    c
}

/// Nearest opaque palette entry to `c`, skipping the transparent slot. Shared
/// with the `.ase` writer so both snap identically.
pub(crate) fn nearest_palette_index(palette: &[Rgba], transparent: u8, c: Rgba) -> u8 {
    let mut best = transparent;
    let mut best_d = u32::MAX;
    for (i, p) in palette.iter().enumerate() {
        if i == transparent as usize || p.3 == 0 {
            continue;
        }
        let dr = i32::from(p.0) - i32::from(c.0);
        let dg = i32::from(p.1) - i32::from(c.1);
        let db = i32::from(p.2) - i32::from(c.2);
        let d = (dr * dr + dg * dg + db * db) as u32;
        if d == 0 {
            return i as u8;
        }
        if d < best_d {
            best_d = d;
            best = i as u8;
        }
    }
    if best_d == u32::MAX && !palette.is_empty() {
        (palette.len() - 1) as u8
    } else {
        best
    }
}

/// Recursive Bayer threshold matrix. 1 -> [[0]], then each half-size value is
/// multiplied by four and the quadrant offsets 0,2,3,1 are added.
fn bayer_matrix(n: usize) -> Vec<u8> {
    if n <= 1 {
        return vec![0];
    }
    let half = n / 2;
    let prev = bayer_matrix(half);
    let mut m = vec![0u8; n * n];
    for y in 0..n {
        for x in 0..n {
            let base = prev[(y % half) * half + (x % half)] * 4;
            let add = match (y / half, x / half) {
                (0, 0) => 0,
                (0, 1) => 2,
                (1, 0) => 3,
                _ => 1,
            };
            m[y * n + x] = base + add;
        }
    }
    m
}

fn dither_ordered(
    pixels: &mut [Rgba],
    w: usize,
    palette: &[Rgba],
    transparent: u8,
    matrix: &[u8],
    ox: i32,
    oy: i32,
) {
    let n = (matrix.len() as f64).sqrt() as usize;
    let denom = (n * n) as f32;
    for (i, p) in pixels.iter_mut().enumerate() {
        if p.3 == 0 {
            *p = Rgba::TRANSPARENT;
            continue;
        }
        let x = i % w;
        let y = i / w;
        let mx = (x as i32 + ox).rem_euclid(n as i32) as usize;
        let my = (y as i32 + oy).rem_euclid(n as i32) as usize;
        let t = (matrix[my * n + mx] as f32 + 0.5) / denom - 0.5;
        let off = t * 64.0;
        let probe = Rgba(
            (p.0 as f32 + off).clamp(0.0, 255.0) as u8,
            (p.1 as f32 + off).clamp(0.0, 255.0) as u8,
            (p.2 as f32 + off).clamp(0.0, 255.0) as u8,
            p.3,
        );
        let idx = nearest_palette_index(palette, transparent, probe);
        let q = palette[idx as usize];
        *p = if q.3 == 0 { Rgba::TRANSPARENT } else { q };
    }
}

fn nearest_palette_rgb(palette: &[Rgba], transparent: u8, r: f32, g: f32, b: f32) -> u8 {
    let mut best = transparent;
    let mut best_d = f32::MAX;
    for (i, p) in palette.iter().enumerate() {
        if i == transparent as usize || p.3 == 0 {
            continue;
        }
        let dr = p.0 as f32 - r;
        let dg = p.1 as f32 - g;
        let db = p.2 as f32 - b;
        let d = dr * dr + dg * dg + db * db;
        if d == 0.0 {
            return i as u8;
        }
        if d < best_d {
            best_d = d;
            best = i as u8;
        }
    }
    if best_d == f32::MAX && !palette.is_empty() {
        (palette.len() - 1) as u8
    } else {
        best
    }
}

fn dither_floyd(pixels: &mut [Rgba], w: usize, h: usize, palette: &[Rgba], transparent: u8) {
    let src = pixels.to_vec();
    let mut buf: Vec<[f32; 3]> = src
        .iter()
        .map(|p| [p.0 as f32, p.1 as f32, p.2 as f32])
        .collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if src[i].3 == 0 {
                pixels[i] = Rgba::TRANSPARENT;
                continue;
            }
            let c = buf[i];
            let idx = nearest_palette_rgb(palette, transparent, c[0], c[1], c[2]);
            let q = palette[idx as usize];
            pixels[i] = if q.3 == 0 { Rgba::TRANSPARENT } else { q };
            let err = [c[0] - q.0 as f32, c[1] - q.1 as f32, c[2] - q.2 as f32];
            for (dx, dy, f) in [
                (1isize, 0isize, 7.0 / 16.0),
                (-1, 1, 3.0 / 16.0),
                (0, 1, 5.0 / 16.0),
                (1, 1, 1.0 / 16.0),
            ] {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                    let ni = ny as usize * w + nx as usize;
                    if src[ni].3 > 0 {
                        for ch in 0..3 {
                            buf[ni][ch] += err[ch] * f;
                        }
                    }
                }
            }
        }
    }
}

/// Median-cut quantizer: split the widest box until there are `max` boxes,
/// then average each one. Colours are pre-sorted so the result is stable.
fn median_cut(colors: Vec<Rgba>, max: usize) -> Vec<Rgba> {
    let mut boxes = vec![colors];
    while boxes.len() < max {
        let mut pick = None;
        let mut widest = 0;
        for (i, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            let r = channel_ranges(b);
            let range = r[0].max(r[1]).max(r[2]);
            if range > widest {
                widest = range;
                pick = Some(i);
            }
        }
        let Some(i) = pick else { break };
        let mut b = boxes.swap_remove(i);
        let r = channel_ranges(&b);
        let ch = if r[0] >= r[1] && r[0] >= r[2] {
            0
        } else if r[1] >= r[2] {
            1
        } else {
            2
        };
        b.sort_by_key(|c| match ch {
            0 => c.0,
            1 => c.1,
            _ => c.2,
        });
        let right = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(right);
    }
    let mut out: Vec<Rgba> = boxes.iter().map(|b| average_color(b)).collect();
    out.sort_by_key(|c| (c.0, c.1, c.2));
    out.dedup();
    out
}

fn channel_ranges(boxes: &[Rgba]) -> [u32; 3] {
    let mut min = [255u8; 3];
    let mut max = [0u8; 3];
    for c in boxes {
        let v = [c.0, c.1, c.2];
        for i in 0..3 {
            min[i] = min[i].min(v[i]);
            max[i] = max[i].max(v[i]);
        }
    }
    [
        u32::from(max[0] - min[0]),
        u32::from(max[1] - min[1]),
        u32::from(max[2] - min[2]),
    ]
}

fn average_color(boxes: &[Rgba]) -> Rgba {
    let n = boxes.len() as u32;
    if n == 0 {
        return Rgba::TRANSPARENT;
    }
    let mut sum = [0u32; 3];
    for c in boxes {
        sum[0] += u32::from(c.0);
        sum[1] += u32::from(c.1);
        sum[2] += u32::from(c.2);
    }
    Rgba(
        ((sum[0] + n / 2) / n) as u8,
        ((sum[1] + n / 2) / n) as u8,
        ((sum[2] + n / 2) / n) as u8,
        255,
    )
}

fn set_px(
    s: &mut Sprite,
    l: usize,
    f: usize,
    x: u32,
    y: u32,
    color: Rgba,
    out: &mut Vec<(u32, u32, Rgba, Rgba)>,
) {
    let old = s.get(l, f, x, y);
    if old != color {
        s.set(l, f, x, y, color);
        out.push((x, y, old, color));
    }
}

/// Whether two colours are within `tol` on every channel (transparent equals
/// transparent).
pub fn color_close(a: Rgba, b: Rgba, tol: u8) -> bool {
    if a.3 == 0 && b.3 == 0 {
        return true;
    }
    let t = tol as i16;
    (a.0 as i16 - b.0 as i16).abs() <= t
        && (a.1 as i16 - b.1 as i16).abs() <= t
        && (a.2 as i16 - b.2 as i16).abs() <= t
        && (a.3 as i16 - b.3 as i16).abs() <= t
}

/// Match and reach for the bucket fill: a per-channel tolerance, whether the
/// fill stays inside the seed's contiguous region, and an optional
/// sprite-sized clip mask (the selection).
#[derive(Clone, Copy, Debug)]
pub struct FillOptions<'a> {
    pub tol: u8,
    pub contiguous: bool,
    pub clip: Option<&'a [bool]>,
}

impl Default for FillOptions<'_> {
    fn default() -> Self {
        Self {
            tol: 0,
            contiguous: true,
            clip: None,
        }
    }
}

/// 4-connected flood fill starting at `(x, y)`. Returns changed cells
/// `(x, y, before, after)`.
pub fn flood_fill(
    s: &mut Sprite,
    layer: usize,
    frame: usize,
    x: u32,
    y: u32,
    color: Rgba,
) -> Vec<(u32, u32, Rgba, Rgba)> {
    flood_fill_tol(s, layer, frame, x, y, color, FillOptions::default())
}

/// Fill the seed's colour: the contiguous region around it, or every matching
/// pixel in the cel when `opts.contiguous` is false. `opts.tol` is a
/// per-channel match window.
pub fn flood_fill_tol(
    s: &mut Sprite,
    layer: usize,
    frame: usize,
    x: u32,
    y: u32,
    color: Rgba,
    opts: FillOptions<'_>,
) -> Vec<(u32, u32, Rgba, Rgba)> {
    let w = s.width;
    let h = s.height;
    if x >= w || y >= h {
        return Vec::new();
    }
    let allowed = |x: u32, y: u32| {
        opts.clip
            .is_none_or(|m| m.get((y * w + x) as usize).copied().unwrap_or(false))
    };
    if !allowed(x, y) {
        return Vec::new();
    }
    let target = s.get(layer, frame, x, y);
    if target == color {
        return Vec::new();
    }
    let mut changes = Vec::new();
    if opts.contiguous {
        let mut seen = vec![false; (w * h) as usize];
        seen[(y * w + x) as usize] = true;
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            set_px(s, layer, frame, cx, cy, color, &mut changes);
            for (nx, ny) in [
                (cx.wrapping_sub(1), cy),
                (cx + 1, cy),
                (cx, cy.wrapping_sub(1)),
                (cx, cy + 1),
            ] {
                if nx < w && ny < h {
                    let i = (ny * w + nx) as usize;
                    if !seen[i]
                        && allowed(nx, ny)
                        && color_close(s.get(layer, frame, nx, ny), target, opts.tol)
                    {
                        seen[i] = true;
                        stack.push((nx, ny));
                    }
                }
            }
        }
    } else {
        for yy in 0..h {
            for xx in 0..w {
                if allowed(xx, yy) && color_close(s.get(layer, frame, xx, yy), target, opts.tol) {
                    set_px(s, layer, frame, xx, yy, color, &mut changes);
                }
            }
        }
    }
    changes
}

/// Integer Bresenham line, inclusive of both endpoints.
pub fn line_points(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(u32, u32)> {
    let mut pts = Vec::new();
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut cx, mut cy) = (x0, y0);
    loop {
        pts.push((cx as u32, cy as u32));
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
    pts
}

/// Rectangle outline perimeter, inclusive.
pub fn rect_points(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(u32, u32)> {
    let (xmin, xmax) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (ymin, ymax) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let mut pts = Vec::new();
    for x in xmin..=xmax {
        pts.push((x as u32, ymin as u32));
        pts.push((x as u32, ymax as u32));
    }
    for y in ymin..=ymax {
        pts.push((xmin as u32, y as u32));
        pts.push((xmax as u32, y as u32));
    }
    pts.sort_unstable();
    pts.dedup();
    pts
}

/// Every pixel inside the rectangle, inclusive.
pub fn rect_filled_points(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(u32, u32)> {
    let (xmin, xmax) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (ymin, ymax) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let mut pts = Vec::with_capacity(((xmax - xmin + 1) * (ymax - ymin + 1)) as usize);
    for y in ymin..=ymax {
        for x in xmin..=xmax {
            pts.push((x as u32, y as u32));
        }
    }
    pts
}

/// Axis-aligned ellipse bounded by the corners `(x0,y0)`/`(x1,y1)`.
pub fn ellipse_points(x0: i32, y0: i32, x1: i32, y1: i32, filled: bool) -> Vec<(u32, u32)> {
    let cx = (x0 + x1) as f32 / 2.0;
    let cy = (y0 + y1) as f32 / 2.0;
    let rx = ((x1 - x0).abs() as f32 / 2.0).max(0.5);
    let ry = ((y1 - y0).abs() as f32 / 2.0).max(0.5);
    let (minx, maxx) = (x0.min(x1), x0.max(x1));
    let (miny, maxy) = (y0.min(y1), y0.max(y1));

    let d = |px: i32, py: i32| -> f32 {
        let dx = (px as f32 + 0.5 - cx) / rx;
        let dy = (py as f32 + 0.5 - cy) / ry;
        dx * dx + dy * dy
    };

    let mut pts = Vec::new();
    for y in miny..=maxy {
        for x in minx..=maxx {
            if filled {
                if d(x, y) <= 1.0 {
                    pts.push((x as u32, y as u32));
                }
            } else if d(x, y) <= 1.0
                && (d(x - 1, y) > 1.0
                    || d(x + 1, y) > 1.0
                    || d(x, y - 1) > 1.0
                    || d(x, y + 1) > 1.0)
            {
                pts.push((x as u32, y as u32));
            }
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_fill_respects_boundaries() {
        let mut s = Sprite::new(3, 3);
        s.set(0, 0, 1, 1, Rgba::rgb(255, 0, 0));
        let changes = flood_fill(&mut s, 0, 0, 0, 0, Rgba::rgb(0, 0, 255));
        // Only the 8 pixels not occupied by the red barrier should change.
        assert_eq!(changes.len(), 8);
        assert_eq!(s.get(0, 0, 1, 1), Rgba::rgb(255, 0, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(0, 0, 255));
    }

    #[test]
    fn composite_passes_transparent_and_overrides_opaque() {
        let mut s = Sprite::new(1, 1);
        // top layer (index 0) transparent -> bottom layer shows through
        s.layers.push(Layer {
            name: "Bottom".into(),
            user_data: None,
            ..Default::default()
        });
        let cel = Cel {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            z_index: 0,
            link: None,
            pixels: vec![Rgba::rgb(10, 20, 30)],
            opacity: 255,
            user_data: None,
            tilemap: None,
        };
        s.cels.push(vec![cel; 1]);
        assert_eq!(s.composite(0)[0], Rgba(10, 20, 30, 255));

        // top layer opaque -> hides bottom
        s.cels[0][0].pixels[0] = Rgba::rgb(200, 100, 50);
        assert_eq!(s.composite(0)[0], Rgba(200, 100, 50, 255));
    }

    #[test]
    fn multiply_blend_darkens() {
        let mut s = Sprite::new(1, 1);
        s.layers.push(Layer {
            name: "Bottom".into(),
            user_data: None,
            ..Default::default()
        });
        s.cels.push(vec![Cel {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            z_index: 0,
            link: None,
            pixels: vec![Rgba::rgb(128, 128, 128)],
            opacity: 255,
            user_data: None,
            tilemap: None,
        }]);
        // top layer multiplies gray over gray
        s.layers[0].blend = BlendMode::Multiply;
        s.cels[0][0].pixels[0] = Rgba::rgb(128, 128, 128);
        let c = s.composite(0)[0];
        // 0.5 * 0.5 = 0.25 -> ~64
        assert!((c.0 as i32 - 64).abs() <= 1, "got {}", c.0);
    }

    #[test]
    fn every_blend_mode_matches_the_reference_math() {
        let dst = Rgba(51, 102, 153, 255);
        let src = Rgba(128, 77, 204, 255);
        let opaque = [
            (BlendMode::Normal, [128, 77, 204]),
            (BlendMode::Multiply, [26, 31, 122]),
            (BlendMode::Screen, [153, 148, 235]),
            (BlendMode::Overlay, [51, 62, 214]),
            (BlendMode::Darken, [51, 77, 153]),
            (BlendMode::Lighten, [128, 102, 204]),
            (BlendMode::ColorDodge, [102, 146, 255]),
            (BlendMode::ColorBurn, [0, 0, 128]),
            (BlendMode::HardLight, [52, 62, 214]),
            (BlendMode::SoftLight, [51, 78, 180]),
            (BlendMode::Difference, [77, 25, 51]),
            (BlendMode::Exclusion, [128, 117, 112]),
            (BlendMode::Hue, [110, 69, 171]),
            (BlendMode::Saturation, [41, 104, 168]),
            (BlendMode::Color, [114, 63, 190]),
            (BlendMode::Luminosity, [65, 116, 167]),
            (BlendMode::Add, [179, 179, 255]),
            (BlendMode::Subtract, [0, 25, 0]),
            (BlendMode::Divide, [102, 255, 191]),
        ];
        for (mode, want) in opaque {
            let got = blend_over(mode, src, 255, dst);
            assert_eq!([got.0, got.1, got.2], want, "{mode:?} at alpha 255");
            assert_eq!(got.3, 255, "{mode:?} alpha");
        }

        // Source at 50% over the opaque backdrop: the compositing formula,
        // not just the blend function.
        let half = [
            (BlendMode::Normal, [90, 89, 179]),
            (BlendMode::Multiply, [38, 66, 138]),
            (BlendMode::Screen, [102, 125, 194]),
            (BlendMode::Overlay, [51, 82, 184]),
            (BlendMode::Darken, [51, 89, 153]),
            (BlendMode::Lighten, [90, 102, 179]),
            (BlendMode::ColorDodge, [77, 124, 204]),
            (BlendMode::ColorBurn, [25, 51, 140]),
            (BlendMode::HardLight, [51, 82, 184]),
            (BlendMode::SoftLight, [51, 90, 166]),
            (BlendMode::Difference, [64, 63, 102]),
            (BlendMode::Exclusion, [90, 110, 133]),
            (BlendMode::Hue, [80, 85, 162]),
            (BlendMode::Saturation, [46, 103, 160]),
            (BlendMode::Color, [83, 82, 172]),
            (BlendMode::Luminosity, [58, 109, 160]),
            (BlendMode::Add, [115, 141, 204]),
            (BlendMode::Subtract, [25, 63, 76]),
            (BlendMode::Divide, [76, 179, 172]),
        ];
        for (mode, want) in half {
            let got = blend_over(mode, src, 128, dst);
            assert_eq!([got.0, got.1, got.2], want, "{mode:?} at alpha 128");
            assert_eq!(got.3, 255, "{mode:?} alpha");
        }
    }

    #[test]
    fn linked_cels_share_content_and_copy_on_write() {
        let red = Rgba(255, 0, 0, 255);
        let blue = Rgba(0, 0, 255, 255);
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, red);
        s.add_frame_after(0);
        assert!(s.link_cel(0, 1, 0));
        assert!(s.is_linked(0, 1));
        assert_eq!(s.get(0, 1, 0, 0), red);
        s.set(0, 1, 0, 0, blue);
        assert!(!s.is_linked(0, 1), "painting materialises the link");
        assert_eq!(s.get(0, 1, 0, 0), blue);
        assert_eq!(s.get(0, 0, 0, 0), red, "the source is untouched");
    }

    #[test]
    fn prefer_linked_layers_link_new_frames() {
        let red = Rgba(255, 0, 0, 255);
        let blue = Rgba(0, 0, 255, 255);
        let mut s = Sprite::new(1, 1);
        s.layers[0].prefer_linked = true;
        s.set(0, 0, 0, 0, red);
        s.add_frame_after(0);
        assert_eq!(s.cels[0][1].link, Some(0));
        assert_eq!(s.get(0, 1, 0, 0), red);
        s.set(0, 1, 0, 0, blue);
        assert_eq!(s.get(0, 0, 0, 0), red);
        assert_eq!(s.get(0, 1, 0, 0), blue);
    }

    #[test]
    fn frame_operations_keep_links_valid() {
        let red = Rgba(255, 0, 0, 255);
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, red);
        s.add_frame_after(0);
        s.link_cel(0, 1, 0);
        s.move_frame(0, 1);
        assert_eq!(s.get(0, 0, 0, 0), red, "the link follows the move");
        s.remove_frame(1);
        assert_eq!(s.get(0, 0, 0, 0), red, "removal materialises the link");
        assert!(!s.is_linked(0, 0));
    }

    #[test]
    fn tilemap_cels_rerasterize_from_retained_refs() {
        let mut s = Sprite::new(4, 2);
        s.layers[0].tilemap = true;
        s.layers[0].tileset = 5;
        s.tilesets.push(Tileset {
            id: 5,
            tile_w: 2,
            tile_h: 2,
            base_index: 1,
            num_tiles: 2,
            image: vec![
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 255, 0),
            ],
            ..Default::default()
        });
        let tm = TilemapCel::new(
            2,
            vec![
                TileRef {
                    id: 1,
                    ..Default::default()
                },
                TileRef {
                    id: 2,
                    ..Default::default()
                },
            ],
        );
        s.cels[0][0].tilemap = Some(tm);
        assert!(s.rasterize_tilemap(0, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(s.get(0, 0, 2, 0), Rgba::rgb(0, 255, 0));

        s.cels[0][0].tilemap = Some(TilemapCel::new(
            2,
            vec![
                TileRef {
                    id: 2,
                    flip_x: true,
                    ..Default::default()
                },
                TileRef {
                    id: 1,
                    ..Default::default()
                },
            ],
        ));
        assert!(s.rasterize_tilemap(0, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(0, 255, 0));
        assert_eq!(s.get(0, 0, 2, 0), Rgba::rgb(255, 0, 0));
        assert_eq!((s.cels[0][0].w, s.cels[0][0].h), (4, 2));
    }

    #[test]
    fn cel_z_orders_the_composite() {
        let mut s = Sprite::new(1, 1);
        let red = Rgba(255, 0, 0, 255);
        let blue = Rgba(0, 0, 255, 255);
        s.set(0, 0, 0, 0, red);
        s.layers.push(Layer {
            name: "Bottom".into(),
            user_data: None,
            ..Default::default()
        });
        s.cels.push(vec![Cel {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            z_index: 0,
            link: None,
            pixels: vec![blue],
            opacity: 255,
            user_data: None,
            tilemap: None,
        }]);
        assert_eq!(s.composite(0)[0], red);
        s.cels[1][0].z_index = 1;
        assert_eq!(s.composite(0)[0], blue, "+z moves towards the front");
        s.cels[1][0].z_index = 0;
        s.cels[0][0].z_index = -1;
        assert_eq!(s.composite(0)[0], blue, "-z moves towards the back");
        s.cels[0][0].z_index = 0;
        assert_eq!(s.composite(0)[0], red);
    }

    #[test]
    fn offset_cels_clip_composite_and_expand_on_paint() {
        let mut s = Sprite::new(4, 4);
        let red = Rgba(10, 20, 30, 255);
        s.cels[0][0] = Cel {
            x: 1,
            y: 1,
            w: 2,
            h: 2,
            z_index: 0,
            link: None,
            pixels: vec![red; 4],
            opacity: 255,
            user_data: None,
            tilemap: None,
        };
        let comp = s.composite(0);
        assert_eq!(comp[5], red);
        assert_eq!(comp[0], Rgba::TRANSPARENT);

        s.set(0, 0, 3, 3, Rgba(1, 2, 3, 255));
        assert_eq!(s.get(0, 0, 3, 3), Rgba(1, 2, 3, 255));
        assert!(s.cels[0][0].contains(3, 3), "paint grows the cel bounds");
        assert_eq!(s.composite(0)[3 * 4 + 3], Rgba(1, 2, 3, 255));

        // Negative origins clip at the canvas edge.
        s.cels[0][0] = Cel {
            x: -1,
            y: -1,
            w: 2,
            h: 2,
            z_index: 0,
            link: None,
            pixels: vec![red; 4],
            opacity: 255,
            user_data: None,
            tilemap: None,
        };
        assert_eq!(s.composite(0)[0], red);
        assert_eq!(s.get(0, 0, 3, 3), Rgba::TRANSPARENT);
    }

    #[test]
    fn flips_and_rotate_preserve_offset_bounds() {
        let mut s = Sprite::new(4, 4);
        let red = Rgba(255, 0, 0, 255);
        let blue = Rgba(0, 0, 255, 255);
        s.cels[0][0] = Cel {
            x: 1,
            y: 1,
            w: 2,
            h: 1,
            z_index: 0,
            link: None,
            pixels: vec![red, blue],
            opacity: 255,
            user_data: None,
            tilemap: None,
        };
        s.flip_h();
        assert_eq!(s.cels[0][0].x, 1);
        assert_eq!(s.get(0, 0, 1, 1), blue);
        s.rotate_cw();
        assert_eq!((s.cels[0][0].x, s.cels[0][0].y), (2, 1));
        assert_eq!((s.cels[0][0].w, s.cels[0][0].h), (1, 2));
    }

    #[test]
    fn ase_direction_bytes_round_trip() {
        use PlayDirection::*;
        for (direction, byte) in [
            (Forward, 0),
            (Reverse, 1),
            (PingPong, 2),
            (PingPongReverse, 3),
        ] {
            assert_eq!(direction.to_ase(), byte);
            assert_eq!(PlayDirection::from_ase(byte), direction);
        }
    }

    #[test]
    fn indexed_paint_snaps_to_palette_entries() {
        let mut s = Sprite::new(2, 1);
        s.set(0, 0, 0, 0, Rgba(255, 0, 0, 255));
        s.set(0, 0, 1, 0, Rgba(0, 0, 255, 255));
        s.quantize_to_palette();

        let red = s.palette[1..]
            .iter()
            .copied()
            .find(|c| (c.0, c.1, c.2) == (255, 0, 0))
            .expect("red in palette");
        s.set(0, 0, 0, 0, Rgba(250, 8, 4, 255));
        assert_eq!(s.get(0, 0, 0, 0), red, "near red snaps to the entry");
        s.set(0, 0, 0, 0, Rgba(0, 0, 0, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::TRANSPARENT);
        s.set(0, 0, 0, 0, Rgba(0, 0, 240, 255));
        assert_eq!(s.get(0, 0, 0, 0), Rgba(0, 0, 255, 255));
    }

    #[test]
    fn indexed_quantizer_handles_more_than_256_colours() {
        let mut s = Sprite::new(300, 1);
        for x in 0..300u32 {
            s.set(
                0,
                0,
                x,
                0,
                Rgba(x as u8, (x / 256) as u8, (x * 7 % 256) as u8, 255),
            );
        }
        let original = s.clone();
        s.quantize_to_palette();
        assert_eq!(s.color_mode, ColorMode::Indexed);
        assert_eq!(s.palette.len(), 256, "transparent + 255 quantized");
        assert_eq!(s.palette[0], Rgba::TRANSPARENT);
        for p in &s.cels[0][0].pixels {
            assert!(s.palette.contains(p), "{p:?} is not a palette entry");
        }
        let once = s.cels[0][0].pixels.clone();
        s.quantize_to_palette();
        assert_eq!(s.cels[0][0].pixels, once, "re-quantizing keeps pixels");

        let mut a = original.clone();
        let mut b = original;
        a.quantize_to_palette();
        b.quantize_to_palette();
        assert_eq!(a.palette, b.palette, "independent runs agree");
    }

    #[test]
    fn bayer_dither_follows_the_matrix() {
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
        s.color_mode = ColorMode::Indexed;
        s.dither_to_palette(DitherMode::Bayer2);

        let black = Rgba(0, 0, 0, 255);
        let pattern = [[0, 1], [1, 0]];
        for y in 0..4 {
            for x in 0..4 {
                let got = usize::from(s.get(0, 0, x, y) != black);
                assert_eq!(
                    got,
                    pattern[(y % 2) as usize][(x % 2) as usize],
                    "pixel ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn floyd_steinberg_diffuses_error() {
        let mut s = Sprite::new(4, 1);
        for (x, v) in [0u8, 85, 170, 255].into_iter().enumerate() {
            s.set(0, 0, x as u32, 0, Rgba(v, v, v, 255));
        }
        s.palette = vec![
            Rgba::TRANSPARENT,
            Rgba(0, 0, 0, 255),
            Rgba(255, 255, 255, 255),
        ];
        s.transparent_index = 0;
        s.color_mode = ColorMode::Indexed;
        s.dither_to_palette(DitherMode::FloydSteinberg);

        let black = Rgba(0, 0, 0, 255);
        let got: Vec<u8> = (0..4)
            .map(|x| u8::from(s.get(0, 0, x, 0) != black))
            .collect();
        assert_eq!(got, [0, 0, 1, 1], "error carries rightwards");
    }

    #[test]
    fn dithering_is_deterministic() {
        let mut a = Sprite::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let v = (x * 32 + y * 4) as u8;
                a.set(0, 0, x, y, Rgba(v, v, v, 255));
            }
        }
        a.palette = vec![
            Rgba::TRANSPARENT,
            Rgba(0, 0, 0, 255),
            Rgba(85, 85, 85, 255),
            Rgba(170, 170, 170, 255),
            Rgba(255, 255, 255, 255),
        ];
        a.transparent_index = 0;
        a.color_mode = ColorMode::Indexed;
        for mode in [
            DitherMode::Bayer2,
            DitherMode::Bayer4,
            DitherMode::Bayer8,
            DitherMode::FloydSteinberg,
        ] {
            let mut b = a.clone();
            a.dither_to_palette(mode);
            b.dither_to_palette(mode);
            assert_eq!(a.cels[0][0].pixels, b.cels[0][0].pixels, "{mode:?}");
            // Reset the pixels for the next mode, keeping the palette.
            a = b;
        }
    }

    #[test]
    fn flip_mirrors() {
        let mut s = Sprite::new(3, 1);
        s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
        s.flip_h();
        assert_eq!(s.get(0, 0, 2, 0), Rgba::rgb(255, 0, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::TRANSPARENT);
    }

    #[test]
    fn rotate_swaps_dims() {
        let mut s = Sprite::new(2, 3);
        s.set(0, 0, 0, 0, Rgba::rgb(1, 2, 3));
        s.rotate_cw();
        assert_eq!((s.width, s.height), (3, 2));
        assert_eq!(s.get(0, 0, 2, 0), Rgba::rgb(1, 2, 3));
    }

    #[test]
    fn move_layer_reorders() {
        let mut s = Sprite::new(1, 1);
        s.add_layer_after(0);
        s.layers[1].name = "Second".into();
        assert!(s.move_layer(1, 0).is_some());
        assert_eq!(s.layers[0].name, "Second");
    }

    #[test]
    fn group_visibility_and_opacity_apply_to_children() {
        let mut s = Sprite::new(1, 1);
        // [group, child, bottom]
        s.layers[0] = Layer {
            name: "Group".into(),
            group: true,
            user_data: None,
            ..Default::default()
        };
        s.layers.push(Layer {
            name: "Child".into(),
            child_level: 1,
            user_data: None,
            ..Default::default()
        });
        s.layers.push(Layer {
            name: "Bottom".into(),
            user_data: None,
            ..Default::default()
        });
        for _ in 0..2 {
            s.cels.push(vec![Cel {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
                z_index: 0,
                link: None,
                pixels: vec![Rgba::TRANSPARENT],
                opacity: 255,
                user_data: None,
                tilemap: None,
            }]);
        }
        s.cels[0][0].pixels[0] = Rgba::rgb(255, 0, 0); // group itself is ignored
        s.cels[1][0].pixels[0] = Rgba::rgb(0, 0, 255);
        s.cels[2][0].pixels[0] = Rgba::rgb(0, 255, 0);

        assert_eq!(s.composite(0)[0], Rgba::rgb(0, 0, 255));
        s.layers[0].visible = false;
        assert_eq!(s.composite(0)[0], Rgba::rgb(0, 255, 0));
        s.layers[0].visible = true;
        s.layers[0].opacity = 128;
        let c = s.composite(0)[0];
        assert!(
            c.0 == 0 && c.1 > 100,
            "group opacity dips the child, got {c:?}"
        );
    }

    #[test]
    fn mask_multiplies_alpha_and_transforms() {
        let mut s = Sprite::new(2, 1);
        s.cels[0][0].pixels[0] = Rgba(255, 255, 255, 255);
        s.cels[0][0].pixels[1] = Rgba(255, 255, 255, 255);
        s.layers[0].mask = Some(Mask {
            pixels: vec![0, 255],
        });
        assert_eq!(s.composite(0)[0].3, 0);
        assert_eq!(s.composite(0)[1].3, 255);
        // Masks follow flips and resizes.
        s.flip_h();
        assert_eq!(s.composite(0)[0].3, 255);
        s.resize(3, 1);
        assert_eq!(s.layers[0].mask.as_ref().unwrap().pixels.len(), 3);
    }

    #[test]
    fn locked_or_group_layers_are_not_editable() {
        let mut s = Sprite::new(1, 1);
        assert!(s.layer_editable(0));
        s.layers[0].editable = false;
        assert!(!s.layer_editable(0));
        s.layers[0].editable = true;
        s.layers[0].group = true;
        assert!(!s.layer_editable(0));
        s.add_layer_after(0);
        s.layers[1].child_level = 1;
        assert!(s.layer_editable(1));
        s.layers[0].editable = false;
        assert!(!s.layer_editable(1), "a locked group locks its children");
        s.layers[0].editable = true;
        s.layers[0].visible = false;
        assert!(!s.layer_editable(1), "a hidden group is not editable");
    }

    #[test]
    fn group_remove_takes_children_and_indent_normalizes() {
        let mut s = Sprite::new(1, 1);
        s.add_group_after(0);
        s.add_layer_after(1);
        s.layers[2].child_level = 1;
        s.layers[2].name = "Inner".into();
        assert_eq!(s.subtree_end(1), 3);
        assert!(s.move_layer(1, 0).is_some(), "group block moves up");
        assert_eq!(s.layers[0].name, "Group 2");
        assert_eq!(s.layers[1].name, "Inner");
        assert!(s.remove_layer(0));
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.layers[0].name, "Layer 1");
        assert!(!s.remove_layer(0), "cannot delete the last layer");
    }

    #[test]
    fn move_frame_reorders_cels() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba::rgb(1, 1, 1));
        s.add_frame_after(0);
        s.set(0, 1, 0, 0, Rgba::rgb(2, 2, 2));
        s.add_frame_after(1);
        s.set(0, 2, 0, 0, Rgba::rgb(3, 3, 3));
        assert!(s.move_frame(2, 0));
        assert_eq!(s.get(0, 0, 0, 0), Rgba::rgb(3, 3, 3));
        assert_eq!(s.get(0, 1, 0, 0), Rgba::rgb(1, 1, 1));
        assert_eq!(s.get(0, 2, 0, 0), Rgba::rgb(2, 2, 2));
        assert!(!s.move_frame(0, 0));
    }

    #[test]
    fn tags_follow_frame_insert_and_remove() {
        let mut s = Sprite::new(1, 1);
        s.add_frame_after(0);
        s.add_frame_after(1);
        s.tags.push(Tag {
            name: "walk".into(),
            from: 1,
            to: 2,
            color: Rgba(1, 2, 3, 255),
            direction: PlayDirection::Reverse,
            repeat: 2,
            user_data: None,
        });
        s.add_frame_after(0); // insert before the tag -> shifts to 2..3
        assert_eq!((s.tags[0].from, s.tags[0].to), (2, 3));
        s.remove_frame(2);
        assert_eq!((s.tags[0].from, s.tags[0].to), (2, 2));
        s.remove_frame(2);
        assert!(s.tags.is_empty(), "removing the last tagged frame drops it");
    }

    #[test]
    fn duplicate_frame_copies_cels() {
        let mut s = Sprite::new(1, 1);
        s.set(0, 0, 0, 0, Rgba::rgb(9, 9, 9));
        s.duplicate_frame(0);
        assert_eq!(s.frames.len(), 2);
        assert_eq!(s.get(0, 1, 0, 0), Rgba::rgb(9, 9, 9));
    }

    #[test]
    fn ellipse_and_rect_endpoints() {
        assert_eq!(line_points(0, 0, 2, 0).len(), 3);
        assert_eq!(rect_points(0, 0, 1, 1).len(), 4);
        assert!(!ellipse_points(0, 0, 4, 4, true).is_empty());
        assert!(!ellipse_points(0, 0, 4, 4, false).is_empty());
    }
}
