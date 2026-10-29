//! Theme files and UI fonts. Themes are JSON so they can be shared and edited
//! without a build; `dark`/`light`/`midnight`/`paper` are built in.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    pub bg: [u8; 3],
    pub panel: [u8; 3],
    pub text: [u8; 3],
    pub accent: [u8; 3],
    pub grid: [u8; 3],
    pub checker: [u8; 3],
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            name: "dark".into(),
            dark: true,
            bg: [18, 18, 20],
            panel: [28, 28, 32],
            text: [232, 232, 236],
            accent: [120, 190, 255],
            grid: [70, 70, 80],
            checker: [56, 56, 56],
        }
    }
}

fn color(rgb: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn builtin(name: &str) -> Option<Theme> {
    let mut theme = Theme {
        name: name.to_ascii_lowercase(),
        ..Theme::default()
    };
    match theme.name.as_str() {
        "dark" => {}
        "light" => {
            theme.dark = false;
            theme.bg = [245, 245, 245];
            theme.panel = [230, 230, 230];
            theme.text = [28, 28, 30];
            theme.accent = [0, 96, 190];
            theme.grid = [175, 175, 180];
            theme.checker = [205, 205, 205];
        }
        "midnight" => {
            theme.bg = [8, 10, 24];
            theme.panel = [18, 22, 44];
            theme.text = [214, 222, 255];
            theme.accent = [92, 144, 255];
            theme.grid = [48, 58, 104];
            theme.checker = [28, 34, 62];
        }
        "paper" => {
            theme.dark = false;
            theme.bg = [238, 232, 213];
            theme.panel = [250, 246, 230];
            theme.text = [38, 38, 32];
            theme.accent = [158, 72, 28];
            theme.grid = [188, 178, 148];
            theme.checker = [220, 210, 185];
        }
        _ => return None,
    }
    Some(theme)
}

pub fn names() -> &'static [&'static str] {
    &["dark", "light", "midnight", "paper"]
}

/// Resolve a built-in name or a path to a `.json` theme file.
pub fn resolve(spec: &str) -> Result<Theme> {
    if let Some(theme) = builtin(spec) {
        return Ok(theme);
    }
    let path = Path::new(spec);
    if path.exists() {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        return serde_json::from_str(&text)
            .with_context(|| format!("parse theme {}", path.display()));
    }
    anyhow::bail!(
        "unknown theme '{spec}' (built-ins: {}; or a .json path)",
        names().join(", ")
    )
}

pub fn save(theme: &Theme, path: &Path) -> Result<()> {
    let text = serde_json::to_string_pretty(theme)?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Apply a theme to the whole egui context (panels, widgets, text, selection).
pub fn apply(ctx: &egui::Context, theme: &Theme) {
    let mut v = if theme.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.window_fill = color(theme.bg);
    v.panel_fill = color(theme.bg);
    v.extreme_bg_color = color(theme.bg);
    v.faint_bg_color = color(theme.panel);
    v.window_stroke.color = color(theme.grid);
    v.override_text_color = Some(color(theme.text));
    v.hyperlink_color = color(theme.accent);
    v.selection.bg_fill = color(theme.accent);
    v.selection.stroke.color = color(theme.text);
    v.widgets.noninteractive.bg_fill = color(theme.panel);
    v.widgets.noninteractive.bg_stroke.color = color(theme.grid);
    v.widgets.inactive.bg_fill = color(theme.panel);
    v.widgets.inactive.fg_stroke.color = color(theme.text);
    ctx.set_visuals(v);
}

/// Load a `.ttf`/`.otf` as the UI font, or restore egui's default for
/// `default`/empty. `size` clamps to 8–32 pt.
pub fn install_font(ctx: &egui::Context, spec: &str, size: f32) -> Result<()> {
    let mut fonts = egui::FontDefinitions::default();
    let spec = spec.trim();
    if !spec.is_empty() && !spec.eq_ignore_ascii_case("default") {
        let bytes = std::fs::read(spec).with_context(|| format!("read font {spec}"))?;
        fonts.font_data.insert(
            "modaline-ui".into(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "modaline-ui".into());
        }
    }
    ctx.set_fonts(fonts);
    let size = size.clamp(8.0, 32.0);
    ctx.all_styles_mut(|style| {
        for font in style.text_styles.values_mut() {
            font.size = size;
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_resolve_and_are_distinct() {
        let dark = builtin("dark").unwrap();
        let paper = builtin("paper").unwrap();
        assert!(dark.dark);
        assert!(!paper.dark);
        assert_ne!(dark.bg, paper.bg);
        assert_eq!(resolve("MIDNIGHT").unwrap().name, "midnight");
        assert!(resolve("nope").unwrap_err().to_string().contains("unknown"));
    }

    #[test]
    fn a_theme_file_round_trips_and_applies_missing_fields() {
        let dir = std::env::temp_dir().join(format!("modaline-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mine.json");
        let mut custom = builtin("midnight").unwrap();
        custom.name = "mine".into();
        custom.accent = [255, 0, 128];
        save(&custom, &path).unwrap();
        assert_eq!(resolve(path.to_str().unwrap()).unwrap(), custom);
        std::fs::write(&path, r#"{"name":"partial","accent":[1,2,3]}"#).unwrap();
        let partial = resolve(path.to_str().unwrap()).unwrap();
        assert_eq!(partial.accent, [1, 2, 3]);
        assert_eq!(partial.bg, Theme::default().bg);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
