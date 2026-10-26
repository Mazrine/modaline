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

/// sRGB transfer function, encoded -> linear light, as an 8-bit result.
pub fn srgb_to_linear(c: u8) -> u8 {
    let v = c as f32 / 255.0;
    let lin = if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    };
    (lin * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Linear light -> sRGB, the inverse of [`srgb_to_linear`].
pub fn linear_to_srgb(c: u8) -> u8 {
    let v = c as f32 / 255.0;
    let enc = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (enc * 255.0).round().clamp(0.0, 255.0) as u8
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

/// `#rrggbb`, `rrggbb` or `#rgb`. Alpha is always opaque: the fg/bg are paint
/// colours, and transparency is a property of the pixel, not of the swatch.
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let h = s.trim().trim_start_matches('#');
    let digit = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok();
    match h.len() {
        6 => Some(Rgba::rgb(
            u8::from_str_radix(&h[0..2], 16).ok()?,
            u8::from_str_radix(&h[2..4], 16).ok()?,
            u8::from_str_radix(&h[4..6], 16).ok()?,
        )),
        3 => Some(Rgba::rgb(digit(0)? * 17, digit(1)? * 17, digit(2)? * 17)),
        _ => None,
    }
}

/// RGB -> (hue 0..360, saturation 0..1, value 0..1).
pub fn to_hsv(c: Rgba) -> (f32, f32, f32) {
    let (r, g, b) = (c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

/// (hue 0..360, saturation 0..1, value 0..1) -> RGB, opaque.
pub fn from_hsv(h: f32, s: f32, v: f32) -> Rgba {
    let h = h.rem_euclid(360.0) / 60.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Rgba::rgb(
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_and_linear_are_inverses() {
        // shortcut: 8-bit linear is coarse in the shadows - the worst round-trip
        // error is 6/255 (at sRGB 6), and it is exact within 1 from 64 up. That
        // is the format, not the maths: store linear in u16 if a linear-sprite
        // pipeline ever needs the darks.
        let mut worst = 0;
        for c in 0..=255u8 {
            let back = linear_to_srgb(srgb_to_linear(c));
            let err = back.abs_diff(c);
            worst = worst.max(err);
            if c >= 64 {
                assert!(err <= 1, "{c} -> {back}");
            }
        }
        assert_eq!(worst, 6);
        assert_eq!(srgb_to_linear(0), 0);
        assert_eq!(srgb_to_linear(255), 255);
        // Mid grey is much darker in linear light: 128 -> ~55.
        let mid = srgb_to_linear(128);
        assert!((54..=57).contains(&mid), "128 -> {mid}");
    }

    #[test]
    fn hex_and_hsv_round_trip() {
        assert_eq!(parse_hex("#ff8000"), Some(Rgba::rgb(255, 128, 0)));
        assert_eq!(parse_hex("0f0"), Some(Rgba::rgb(0, 255, 0)));
        assert_eq!(parse_hex("#12345"), None);
        for c in default_palette() {
            let (h, s, v) = to_hsv(c);
            let back = from_hsv(h, s, v);
            let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
            assert!(
                d(c.0, back.0) <= 1 && d(c.1, back.1) <= 1 && d(c.2, back.2) <= 1,
                "{c:?} -> hsv({h:.1},{s:.1},{v:.1}) -> {back:?}"
            );
        }
    }
}
