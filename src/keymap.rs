//! Customisable action keymap layered over the fixed modal editing keys.
//!
//! Actions have default chords that match the built-in keymap. Rebinding an
//! action suppresses its old default so the hardcoded handler cannot fire;
//! bindings and wheel actions persist in `config.json`.

use std::collections::BTreeMap;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::editor::{Editor, Mode, Pending, Shape, Tool};
use crate::gui::View;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Undo,
    Redo,
    Save,
    Quit,
    Help,
    Command,
    ToolPencil,
    ToolEraser,
    ToolBucket,
    ToolPicker,
    ToolLine,
    ToolRect,
    ToolEllipse,
    ToolLasso,
    ToolGradient,
    ToolSpray,
    ToolShade,
    ToolBlur,
    ToolText,
    ToolTile,
    PrevFrame,
    NextFrame,
    PrevLayer,
    NextLayer,
    SwapColors,
    ZoomIn,
    ZoomOut,
    ToggleGrid,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::Undo,
        Action::Redo,
        Action::Save,
        Action::Quit,
        Action::Help,
        Action::Command,
        Action::ToolPencil,
        Action::ToolEraser,
        Action::ToolBucket,
        Action::ToolPicker,
        Action::ToolLine,
        Action::ToolRect,
        Action::ToolEllipse,
        Action::ToolLasso,
        Action::ToolGradient,
        Action::ToolSpray,
        Action::ToolShade,
        Action::ToolBlur,
        Action::ToolText,
        Action::ToolTile,
        Action::PrevFrame,
        Action::NextFrame,
        Action::PrevLayer,
        Action::NextLayer,
        Action::SwapColors,
        Action::ZoomIn,
        Action::ZoomOut,
        Action::ToggleGrid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::Save => "save",
            Action::Quit => "quit",
            Action::Help => "help",
            Action::Command => "command",
            Action::ToolPencil => "tool-pencil",
            Action::ToolEraser => "tool-eraser",
            Action::ToolBucket => "tool-bucket",
            Action::ToolPicker => "tool-picker",
            Action::ToolLine => "tool-line",
            Action::ToolRect => "tool-rect",
            Action::ToolEllipse => "tool-ellipse",
            Action::ToolLasso => "tool-lasso",
            Action::ToolGradient => "tool-gradient",
            Action::ToolSpray => "tool-spray",
            Action::ToolShade => "tool-shade",
            Action::ToolBlur => "tool-blur",
            Action::ToolText => "tool-text",
            Action::ToolTile => "tool-tile",
            Action::PrevFrame => "prev-frame",
            Action::NextFrame => "next-frame",
            Action::PrevLayer => "prev-layer",
            Action::NextLayer => "next-layer",
            Action::SwapColors => "swap-colors",
            Action::ZoomIn => "zoom-in",
            Action::ZoomOut => "zoom-out",
            Action::ToggleGrid => "toggle-grid",
        }
    }

    pub fn parse(name: &str) -> Option<Action> {
        let name = name.to_ascii_lowercase();
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }

    fn default_chord(self) -> Chord {
        let key = match self {
            Action::Undo => "u",
            Action::Redo => "r",
            Action::Save => "s",
            Action::Quit => "q",
            Action::Help => "/",
            Action::Command => ";",
            Action::ToolPencil => "b",
            Action::ToolEraser => "e",
            Action::ToolBucket => "f",
            Action::ToolPicker => "c",
            Action::ToolLine => "d",
            Action::ToolRect => "r",
            Action::ToolEllipse => "o",
            Action::ToolLasso => "l",
            Action::ToolGradient => "z",
            Action::ToolSpray => "a",
            Action::ToolShade => "q",
            Action::ToolBlur => "n",
            Action::ToolText => "t",
            Action::ToolTile => "k",
            Action::PrevFrame => "[",
            Action::NextFrame => "]",
            Action::PrevLayer => ",",
            Action::NextLayer => ".",
            Action::SwapColors => "x",
            Action::ZoomIn => "+",
            Action::ZoomOut => "-",
            Action::ToggleGrid => "'",
        };
        let mut chord =
            Chord::parse(key).unwrap_or_else(|| panic!("valid default key {key:?} for {self:?}"));
        chord.shift = matches!(
            self,
            Action::Save
                | Action::Quit
                | Action::Help
                | Action::Command
                | Action::ToolRect
                | Action::ToolLasso
                | Action::ToolText
                | Action::ToolTile
                | Action::PrevLayer
                | Action::NextLayer
                | Action::SwapColors
                | Action::ZoomIn
        );
        chord.ctrl = matches!(self, Action::Redo | Action::Save | Action::Quit);
        chord
    }

    pub fn apply(self, editor: &mut Editor, view: &mut View, ctx: &egui::Context) {
        match self {
            Action::Undo => editor.undo(),
            Action::Redo => editor.redo(),
            Action::Save => crate::input::save_active(editor),
            Action::Quit => editor.should_quit = true,
            Action::Help => editor.mode = Mode::Help,
            Action::Command => {
                editor.mode = Mode::Command;
                editor.cmdline.clear();
            }
            Action::ToolPencil => editor.set_tool(Tool::Pencil),
            Action::ToolEraser => editor.set_tool(Tool::Eraser),
            Action::ToolBucket => {
                editor.set_tool(Tool::Bucket);
                let (x, y) = editor.cursor;
                editor.apply_bucket(x, y);
            }
            Action::ToolPicker => {
                editor.set_tool(Tool::Picker);
                let (x, y) = editor.cursor;
                editor.pick_color(x, y);
            }
            Action::ToolLine => {
                editor.set_tool(Tool::Line);
                let start = editor.cursor;
                editor.pending = Some(Pending {
                    shape: Shape::Line,
                    start,
                });
            }
            Action::ToolRect => {
                editor.set_tool(Tool::Rect);
                let start = editor.cursor;
                editor.pending = Some(Pending {
                    shape: Shape::Rect,
                    start,
                });
            }
            Action::ToolEllipse => {
                editor.set_tool(Tool::Ellipse);
                let start = editor.cursor;
                editor.pending = Some(Pending {
                    shape: Shape::Ellipse,
                    start,
                });
            }
            Action::ToolLasso => editor.set_tool(Tool::Lasso),
            Action::ToolGradient => {
                editor.set_tool(Tool::Gradient);
                let start = editor.cursor;
                editor.pending = Some(Pending {
                    shape: Shape::Gradient,
                    start,
                });
            }
            Action::ToolSpray => editor.set_tool(Tool::Spray),
            Action::ToolShade => editor.set_tool(Tool::Shade),
            Action::ToolBlur => editor.set_tool(Tool::Blur),
            Action::ToolText => editor.set_tool(Tool::Text),
            Action::ToolTile => editor.set_tool(Tool::Tile),
            Action::PrevFrame => editor.cycle_frame(-1),
            Action::NextFrame => editor.cycle_frame(1),
            Action::PrevLayer => editor.cycle_layer(-1),
            Action::NextLayer => editor.cycle_layer(1),
            Action::SwapColors => editor.swap_colors(),
            Action::ZoomIn => view.zoom_in(),
            Action::ZoomOut => view.zoom_out(),
            Action::ToggleGrid => view.show_grid = !view.show_grid,
        }
        let _ = ctx;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Chord {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
}

impl Chord {
    pub fn parse(spec: &str) -> Option<Chord> {
        let spec = spec.trim();
        if spec.is_empty() {
            return None;
        }
        if spec == "-" {
            return Some(Chord {
                key: "-".into(),
                ctrl: false,
                shift: false,
                alt: false,
            });
        }
        let mut chord = Chord {
            key: String::new(),
            ctrl: false,
            shift: false,
            alt: false,
        };
        let parts: Vec<&str> = spec.split('-').collect();
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            if last {
                chord.key = normalize_key(part)?;
            } else {
                match part.to_ascii_lowercase().as_str() {
                    "ctrl" | "control" | "cmd" | "command" => chord.ctrl = true,
                    "shift" => chord.shift = true,
                    "alt" | "option" => chord.alt = true,
                    _ => return None,
                }
            }
        }
        Some(chord)
    }

    pub fn from_event(key: egui::Key, mods: egui::Modifiers) -> Chord {
        Chord {
            key: key.name().to_string(),
            ctrl: mods.ctrl || mods.command && !cfg!(target_os = "macos"),
            shift: mods.shift,
            alt: mods.alt,
        }
    }

    pub fn matches(&self, key: egui::Key, mods: egui::Modifiers) -> bool {
        let event = Chord::from_event(key, mods);
        self.key.eq_ignore_ascii_case(&event.key)
            && self.ctrl == event.ctrl
            && self.shift == event.shift
            && self.alt == event.alt
    }

    pub fn label(&self) -> String {
        let mut out = String::new();
        if self.ctrl {
            out.push_str("ctrl-");
        }
        if self.alt {
            out.push_str("alt-");
        }
        if self.shift {
            out.push_str("shift-");
        }
        let friendly = match self.key.as_str() {
            "Slash" => "/",
            "Semicolon" => ";",
            "Comma" => ",",
            "Period" => ".",
            "Minus" => "-",
            "Plus" => "+",
            "Equals" => "=",
            "Quote" => "'",
            "OpenBracket" => "[",
            "CloseBracket" => "]",
            "Backslash" => "\\",
            other if other.len() == 1 => {
                let lower = other.to_ascii_lowercase();
                out.push_str(&lower);
                return out;
            }
            other => other,
        };
        out.push_str(friendly);
        out
    }
}

fn normalize_key(spec: &str) -> Option<String> {
    let lower = spec.to_ascii_lowercase();
    let named = match lower.as_str() {
        "space" => " ",
        "enter" | "return" => "Enter",
        "escape" | "esc" => "Escape",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" | "del" => "Delete",
        "insert" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "up" | "arrowup" => "ArrowUp",
        "down" | "arrowdown" => "ArrowDown",
        "left" | "arrowleft" => "ArrowLeft",
        "right" | "arrowright" => "ArrowRight",
        "," => ",",
        "." => ".",
        "<" => ",",
        ">" => ".",
        "/" => "/",
        "?" => "/",
        ";" => ";",
        ":" => ";",
        "[" => "[",
        "]" => "]",
        "'" => "'",
        "=" => "=",
        "+" => "+",
        "-" => "-",
        _ => spec,
    };
    let key = egui::Key::from_name(named)
        .or_else(|| egui::Key::from_name(&named.to_ascii_uppercase()))?;
    Some(key.name().to_string())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keymap {
    pub bindings: BTreeMap<Action, Chord>,
    pub wheel_up: Option<Action>,
    pub wheel_down: Option<Action>,
}

impl Default for Keymap {
    fn default() -> Self {
        let bindings = Action::ALL
            .iter()
            .map(|a| (*a, a.default_chord()))
            .collect();
        Keymap {
            bindings,
            wheel_up: None,
            wheel_down: None,
        }
    }
}

impl Keymap {
    pub fn action_for(&self, key: egui::Key, mods: egui::Modifiers) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(_, chord)| chord.matches(key, mods))
            .map(|(action, _)| *action)
    }

    /// True when this chord used to be an action's default but that action is
    /// now bound elsewhere (or unbound): the hardcoded handler must not run.
    pub fn blocks_default(&self, key: egui::Key, mods: egui::Modifiers) -> bool {
        Action::ALL.iter().any(|action| {
            let default = action.default_chord();
            default.matches(key, mods)
                && self
                    .bindings
                    .get(action)
                    .is_none_or(|chord| *chord != default)
        })
    }

    /// Bind an action, stealing the chord from any other action. Returns the
    /// previous owner when a conflict was resolved.
    pub fn set(&mut self, action: Action, chord: Chord) -> Option<Action> {
        let stolen = self
            .bindings
            .iter()
            .find(|(a, c)| **a != action && **c == chord)
            .map(|(a, _)| *a);
        if let Some(old) = stolen {
            self.bindings.remove(&old);
        }
        self.bindings.insert(action, chord);
        stolen
    }

    pub fn unbind(&mut self, action: Action) {
        self.bindings.remove(&action);
    }

    pub fn wheel_action(&self, up: bool) -> Option<Action> {
        if up { self.wheel_up } else { self.wheel_down }
    }

    pub fn set_wheel(&mut self, action: Action, up: bool) {
        if up {
            self.wheel_up = Some(action);
        } else {
            self.wheel_down = Some(action);
        }
    }

    /// Chords assigned to more than one action (possible in a hand-edited
    /// config). The editor steals on `:bind`, so this is a diagnostic.
    pub fn conflicts(&self) -> Vec<(Chord, Vec<Action>)> {
        let mut out: BTreeMap<Chord, Vec<Action>> = BTreeMap::new();
        for (action, chord) in &self.bindings {
            out.entry(chord.clone()).or_default().push(*action);
        }
        out.into_iter().filter(|(_, a)| a.len() > 1).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_parse_modifiers_and_keys() {
        let c = Chord::parse("ctrl-shift-z").unwrap();
        assert!(c.ctrl && c.shift && !c.alt);
        assert_eq!(c.key, "Z");
        assert_eq!(Chord::parse("shift-?").unwrap().label(), "shift-/");
        assert_eq!(Chord::parse("space").unwrap().key, "Space");
        assert!(Chord::parse("ctrl-").is_none());
        assert!(Chord::parse("hyper-x").is_none());
    }

    #[test]
    fn defaults_are_complete_and_unique() {
        let map = Keymap::default();
        assert_eq!(map.bindings.len(), Action::ALL.len());
        assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
        assert_eq!(
            map.action_for(egui::Key::U, egui::Modifiers::NONE),
            Some(Action::Undo)
        );
        let redo = Chord::parse("ctrl-r").unwrap();
        assert_eq!(
            map.action_for(
                egui::Key::R,
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                }
            ),
            Some(Action::Redo)
        );
        assert!(redo.ctrl);
    }

    #[test]
    fn rebinding_steals_and_blocks_the_old_default() {
        let mut map = Keymap::default();
        let stolen = map.set(Action::Undo, Chord::parse("ctrl-z").unwrap());
        assert_eq!(stolen, None);
        assert!(
            map.blocks_default(egui::Key::U, egui::Modifiers::NONE),
            "old u default must be swallowed"
        );
        let conflict = map.set(Action::Redo, Chord::parse("ctrl-z").unwrap());
        assert_eq!(conflict, Some(Action::Undo));
        assert_eq!(map.bindings[&Action::Redo].label(), "ctrl-z");
        assert!(!map.bindings.contains_key(&Action::Undo));
        map.unbind(Action::Redo);
        assert!(
            map.action_for(
                egui::Key::Z,
                egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn wheel_bindings_and_serde_round_trip() {
        let mut map = Keymap::default();
        map.set_wheel(Action::NextFrame, false);
        map.set(Action::Help, Chord::parse("f1").unwrap());
        let text = serde_json::to_string(&map).unwrap();
        let back: Keymap = serde_json::from_str(&text).unwrap();
        assert_eq!(back.wheel_down, Some(Action::NextFrame));
        assert_eq!(back.bindings[&Action::Help].label(), "F1");
        assert_eq!(
            back.action_for(egui::Key::F1, egui::Modifiers::NONE),
            Some(Action::Help)
        );
    }

    #[test]
    fn action_names_are_unique_and_round_trip() {
        for action in Action::ALL {
            assert_eq!(Action::parse(action.name()), Some(*action));
        }
    }
}
