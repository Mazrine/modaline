//! Workspace layouts: which panels are docked, and their sizes. Layouts are
//! plain JSON so they can be shared; the built-ins cover the common setups.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub name: String,
    pub show_toolbar: bool,
    pub show_timeline: bool,
    pub show_side: bool,
    pub side_width: f32,
    pub timeline_height: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            name: "default".into(),
            show_toolbar: true,
            show_timeline: true,
            show_side: true,
            side_width: 260.0,
            timeline_height: 120.0,
        }
    }
}

pub fn builtin(name: &str) -> Option<Layout> {
    let mut layout = Layout {
        name: name.to_ascii_lowercase(),
        ..Layout::default()
    };
    match layout.name.as_str() {
        "default" => {}
        "drawing" => {
            layout.show_timeline = false;
            layout.side_width = 300.0;
        }
        "animation" => {
            layout.show_side = false;
            layout.timeline_height = 210.0;
        }
        "compact" => {
            layout.show_timeline = false;
            layout.show_side = false;
        }
        _ => return None,
    }
    Some(layout)
}

pub fn names() -> &'static [&'static str] {
    &["default", "drawing", "animation", "compact"]
}

pub fn resolve(spec: &str) -> Result<Layout> {
    if let Some(layout) = builtin(spec) {
        return Ok(layout);
    }
    let path = Path::new(spec);
    if path.exists() {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        return serde_json::from_str(&text)
            .with_context(|| format!("parse layout {}", path.display()));
    }
    anyhow::bail!(
        "unknown layout '{spec}' (built-ins: {}; or a .json path)",
        names().join(", ")
    )
}

pub fn save(layout: &Layout, path: &Path) -> Result<()> {
    let text = serde_json::to_string_pretty(layout)?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Keep dragged panel sizes sane when loading a hand-edited file.
pub fn clamp_sizes(layout: &mut Layout) {
    layout.side_width = layout.side_width.clamp(120.0, 900.0);
    layout.timeline_height = layout.timeline_height.clamp(60.0, 700.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_change_the_right_panels() {
        assert!(!builtin("drawing").unwrap().show_timeline);
        assert!(!builtin("animation").unwrap().show_side);
        assert!(builtin("animation").unwrap().show_timeline);
        assert!(!builtin("compact").unwrap().show_side);
        assert!(builtin("nope").is_none());
        assert_eq!(resolve("ANIMATION").unwrap().name, "animation");
    }

    #[test]
    fn layout_files_round_trip_and_missing_fields_default() {
        let dir = std::env::temp_dir().join(format!("vixel-layout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wide.json");
        let mut custom = builtin("animation").unwrap();
        custom.name = "wide".into();
        custom.side_width = 444.0;
        save(&custom, &path).unwrap();
        assert_eq!(resolve(path.to_str().unwrap()).unwrap(), custom);
        std::fs::write(&path, r#"{"name":"partial","show_side":false}"#).unwrap();
        let partial = resolve(path.to_str().unwrap()).unwrap();
        assert!(!partial.show_side);
        assert_eq!(partial.side_width, Layout::default().side_width);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dragged_sizes_are_clamped() {
        let mut layout = Layout {
            side_width: 5000.0,
            timeline_height: 1.0,
            ..Layout::default()
        };
        clamp_sizes(&mut layout);
        assert_eq!(layout.side_width, 900.0);
        assert_eq!(layout.timeline_height, 60.0);
    }
}
