use serde::{Deserialize, Serialize};

/// A single RGBA color. Tuple struct so it serializes to a compact `[r,g,b,a]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba(0, 0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba(r, g, b, 255)
    }

    pub fn is_transparent(self) -> bool {
        self.3 == 0
    }
}

/// Default palette: the "Sweetie 16" set, a small hand-curated 16-color ramp.
pub fn default_palette() -> Vec<Rgba> {
    vec![
        Rgba::rgb(26, 28, 44),
        Rgba::rgb(93, 39, 93),
        Rgba::rgb(177, 62, 83),
        Rgba::rgb(239, 125, 87),
        Rgba::rgb(255, 205, 117),
        Rgba::rgb(167, 240, 112),
        Rgba::rgb(56, 183, 100),
        Rgba::rgb(37, 113, 121),
        Rgba::rgb(41, 54, 111),
        Rgba::rgb(59, 93, 201),
        Rgba::rgb(65, 166, 246),
        Rgba::rgb(115, 239, 247),
        Rgba::rgb(244, 244, 244),
        Rgba::rgb(148, 176, 194),
        Rgba::rgb(86, 108, 134),
        Rgba::rgb(51, 60, 87),
    ]
}
