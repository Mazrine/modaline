//! The settings that outlive a session: window size, zoom, grid/snap, onion
//! skin defaults, playback flags and the theme.
//!
//! One JSON file under the XDG config dir. Missing file, bad JSON or an
//! unwritable directory are all non-fatal: an editor that refuses to start
//! because a preference is corrupt is worse than one that starts on defaults.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::gui::Onion;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub window: (f32, f32),
    pub zoom: u32,
    pub show_grid: bool,
    pub grid: u32,
    pub snap: bool,
    pub onion: OnionConfig,
    pub play_loop: bool,
    pub play_pingpong: bool,
    /// Built-in theme name or a path to a theme `.json`.
    pub theme: String,
    /// UI font file (`.ttf`/`.otf`), or `default`.
    pub ui_font: String,
    pub ui_font_size: f32,
    /// Docked panels and their sizes.
    pub layout: crate::layout::Layout,
    /// Custom action bindings and wheel actions.
    pub keymap: crate::keymap::Keymap,
    /// The renderer asked for at startup: "glow" (default) or "wgpu". The
    /// request is honoured only by a build with the `wgpu` feature; otherwise
    /// vixel says so and falls back to glow.
    pub renderer: String,
    /// Recently opened files, newest first (`:recent`).
    pub recent: Vec<String>,
    /// Favourite directories in the in-app file browser.
    pub favourites: Vec<String>,
    /// Open document paths restored on launch.
    pub session: Vec<String>,
    pub session_active: usize,
    /// Free-form entries a future build can read; round-tripped, never guessed.
    pub extra: BTreeMap<String, String>,
}

/// `Onion` is a GUI type; this is its serialisable image, kept here so the
/// config file does not depend on egui.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OnionConfig {
    pub enabled: bool,
    pub count: u8,
    pub opacity: u8,
    pub prev: [u8; 3],
    pub next: [u8; 3],
}

impl Default for OnionConfig {
    fn default() -> Self {
        let o = Onion::default();
        OnionConfig {
            enabled: o.enabled,
            count: o.count,
            opacity: o.opacity,
            prev: o.prev,
            next: o.next,
        }
    }
}

impl From<Onion> for OnionConfig {
    fn from(o: Onion) -> Self {
        OnionConfig {
            enabled: o.enabled,
            count: o.count,
            opacity: o.opacity,
            prev: o.prev,
            next: o.next,
        }
    }
}

impl From<OnionConfig> for Onion {
    fn from(c: OnionConfig) -> Self {
        Onion {
            enabled: c.enabled,
            count: c.count,
            opacity: c.opacity,
            prev: c.prev,
            next: c.next,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            window: (1280.0, 800.0),
            zoom: 16,
            show_grid: true,
            grid: 8,
            snap: false,
            onion: OnionConfig::default(),
            play_loop: true,
            play_pingpong: false,
            theme: "dark".into(),
            ui_font: "default".into(),
            ui_font_size: 13.0,
            layout: crate::layout::Layout::default(),
            keymap: crate::keymap::Keymap::default(),
            renderer: "glow".into(),
            recent: Vec::new(),
            favourites: Vec::new(),
            session: Vec::new(),
            session_active: 0,
            extra: BTreeMap::new(),
        }
    }
}

/// `$XDG_CONFIG_HOME/vixel`, else `~/.config/vixel`.
pub(crate) fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("vixel"))
}

/// `$XDG_CONFIG_HOME/vixel/config.json`, else `~/.config/vixel/config.json`.
pub fn path() -> Option<PathBuf> {
    dir().map(|d| d.join("config.json"))
}

/// The autosave snapshot a crashed session leaves behind, beside the config.
/// `:recover` loads it; a successful save or a forced quit removes it.
pub fn recovery_path() -> Option<PathBuf> {
    dir().map(|d| d.join("recovery.vpx"))
}

impl Config {
    /// Read the config, falling back to defaults for anything missing.
    pub fn load() -> Config {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_field_and_an_unknown_one_both_load() {
        // Forward and backward compatibility: an old file has fewer fields, a
        // newer one may have keys this build does not know.
        let old = r#"{"zoom": 4, "theme": "light"}"#;
        let c: Config = serde_json::from_str(old).expect("old config loads");
        assert_eq!(c.zoom, 4);
        assert_eq!(c.theme, "light");
        assert_eq!(c.onion, OnionConfig::default());
        assert_eq!(c.grid, 8, "defaults fill the gaps");
        assert_eq!(c.ui_font, "default");
        assert_eq!(c.ui_font_size, 13.0);

        let newer = r#"{"zoom": 2, "future_knob": true}"#;
        let c: Config = serde_json::from_str(newer).expect("unknown keys are ignored");
        assert_eq!(c.zoom, 2);
    }

    #[test]
    fn the_renderer_request_is_a_first_class_field_and_old_files_still_load() {
        let c: Config = serde_json::from_str(r#"{"renderer": "wgpu"}"#).unwrap();
        assert_eq!(c.renderer, "wgpu");
        let old: Config = serde_json::from_str(r#"{"theme": "light"}"#).unwrap();
        assert_eq!(old.renderer, "glow", "a config from before the field");
    }

    #[test]
    fn the_config_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!("vixel-config-{}", std::process::id()));
        let path = dir.join("config.json");
        let mut c = Config {
            zoom: 7,
            grid: 4,
            snap: true,
            ..Config::default()
        };
        c.onion.count = 3;
        c.onion.prev = [10, 20, 30];
        c.recent.push("/tmp/art.vpx".into());
        c.extra.insert("future".into(), "1".into());
        let text = serde_json::to_string_pretty(&c).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, text).unwrap();
        let back: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back, c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
