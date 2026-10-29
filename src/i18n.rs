//! Localization: string catalogs are JSON so translators can work without a
//! build. English is the source catalog and the fallback for missing keys.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub lang: String,
    pub name: String,
    pub strings: BTreeMap<String, String>,
    /// English status template → translated template.
    pub messages: BTreeMap<String, String>,
    /// Whole-value translations used inside status templates (enum names).
    pub values: BTreeMap<String, String>,
}

impl Catalog {
    pub fn tr(&self, key: &str) -> String {
        self.strings
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// Translate with the source catalog as fallback; unknown keys stay as the
    /// key so a missing translation is visible, not blank.
    pub fn tr_with(&self, fallback: &Catalog, key: &str) -> String {
        self.strings
            .get(key)
            .or_else(|| fallback.strings.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// Keys present in the source but absent here.
    pub fn missing_against(&self, source: &Catalog) -> Vec<String> {
        source
            .strings
            .keys()
            .filter(|key| !self.strings.contains_key(*key))
            .cloned()
            .collect()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }
}

/// A language plus the English source used for fallback/missing-key checks.
#[derive(Clone, Debug, PartialEq)]
pub struct Translator {
    pub catalog: Catalog,
    pub source: Catalog,
}

impl Translator {
    pub fn new(catalog: Catalog) -> Self {
        Translator {
            catalog,
            source: english(),
        }
    }

    pub fn tr(&self, key: &str) -> String {
        self.catalog.tr_with(&self.source, key)
    }

    /// Translation for a status template (the English `format!` source text).
    pub fn message(&self, template: &str) -> String {
        self.catalog
            .messages
            .get(template)
            .or_else(|| self.source.messages.get(template))
            .cloned()
            .unwrap_or_else(|| template.to_string())
    }

    /// Translate a whole status value (an enum name such as `simple`).
    pub fn value(&self, english: &str) -> String {
        self.catalog
            .values
            .get(english)
            .or_else(|| self.source.values.get(english))
            .cloned()
            .unwrap_or_else(|| english.to_string())
    }

    pub fn missing(&self) -> Vec<String> {
        self.catalog.missing_against(&self.source)
    }
}

impl Default for Translator {
    fn default() -> Self {
        Translator::new(english())
    }
}

pub const BUILTINS: &[&str] = &["en", "pt-BR"];

pub fn builtin(code: &str) -> Option<Catalog> {
    let code = code.to_ascii_lowercase();
    let text = match code.as_str() {
        "en" | "en-us" | "en-gb" => include_str!("../lang/en.json"),
        "pt-br" | "pt" => include_str!("../lang/pt-BR.json"),
        _ => return None,
    };
    serde_json::from_str(text).ok()
}

pub fn english() -> Catalog {
    builtin("en").expect("embedded English catalog")
}

/// Resolve a built-in code or a language `.json` path.
pub fn resolve(spec: &str) -> Result<Catalog> {
    if let Some(catalog) = builtin(spec) {
        return Ok(catalog);
    }
    let path = Path::new(spec);
    if path.exists() {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        return serde_json::from_str(&text)
            .with_context(|| format!("parse language {}", path.display()));
    }
    anyhow::bail!(
        "unknown language '{spec}' (built-ins: {}; or a .json path)",
        BUILTINS.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_english_source_and_a_translation() {
        let en = english();
        assert_eq!(en.tr("brush"), "brush");
        let pt = builtin("PT-br").unwrap();
        assert_eq!(pt.tr("brush"), "pincel");
        assert_eq!(pt.tr("untitled"), "sem título");
        assert!(resolve("nope").unwrap_err().to_string().contains("unknown"));
    }

    #[test]
    fn missing_keys_fall_back_to_english_not_the_key() {
        let en = english();
        let partial = Catalog {
            lang: "xx".into(),
            name: "Partial".into(),
            strings: BTreeMap::from([("brush".into(), "pen".into())]),
            ..Catalog::default()
        };
        assert_eq!(partial.tr_with(&en, "brush"), "pen");
        assert_eq!(partial.tr_with(&en, "text"), "text");
        assert_eq!(partial.tr_with(&en, "absent.key"), "absent.key");
    }

    #[test]
    fn catalogs_round_trip_and_partial_files_default() {
        let dir = std::env::temp_dir().join(format!("vixel-lang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("xx.json");
        let custom = Catalog {
            lang: "xx".into(),
            name: "Test".into(),
            strings: BTreeMap::from([("brush".into(), "brush!".into())]),
            ..Catalog::default()
        };
        custom.save(&path).unwrap();
        assert_eq!(resolve(path.to_str().unwrap()).unwrap(), custom);
        std::fs::write(&path, r#"{"strings":{"brush":"b"}}"#).unwrap();
        let partial = resolve(path.to_str().unwrap()).unwrap();
        assert_eq!(partial.tr("brush"), "b");
        assert!(partial.lang.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unescape(lit: &str) -> String {
        let mut out = String::new();
        let mut chars = lit.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                // Rust's `\` + newline strips the newline and the next line's
                // leading whitespace.
                Some('\n') => {
                    while matches!(chars.clone().next(), Some(' ')) {
                        chars.next();
                    }
                }
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => {}
            }
        }
        out
    }

    /// String literals in `src` immediately followed by `.into()`.
    fn into_literals(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut chars = src.char_indices().peekable();
        while let Some((_, c)) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut raw = String::new();
            let mut end = 0;
            let mut escaped = false;
            while let Some(&(j, ch)) = chars.peek() {
                chars.next();
                end = j + ch.len_utf8();
                if escaped {
                    raw.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    raw.push(ch);
                    escaped = true;
                } else if ch == '"' {
                    break;
                } else {
                    raw.push(ch);
                }
            }
            if end > 0 && src[end..].starts_with(".into()") {
                out.push(unescape(&raw));
            }
        }
        out
    }

    /// The literal at the start of `s`, after optional whitespace.
    fn leading_literal(s: &str) -> Option<String> {
        let t = s.trim_start();
        let t = t.strip_prefix('"')?;
        let mut raw = String::new();
        let mut escaped = false;
        for c in t.chars() {
            if escaped {
                raw.push('\\');
                raw.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                return Some(unescape(&raw));
            } else {
                raw.push(c);
            }
        }
        None
    }

    #[test]
    fn every_status_message_has_a_translation() {
        let pt = builtin("pt-BR").unwrap();
        let sources = [
            include_str!("editor.rs"),
            include_str!("input.rs"),
            include_str!("gui.rs"),
            include_str!("app.rs"),
            include_str!("script.rs"),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for src in sources {
            for part in src.split("status!(").skip(1) {
                if let Some(tpl) = leading_literal(part) {
                    seen.insert(tpl);
                }
            }
            let mut rest = src;
            while let Some(pos) = rest.find(".msg = ") {
                let window = &rest[pos..(pos + 900).min(rest.len())];
                if let Some(tpl) = leading_literal(&window[".msg = ".len()..]) {
                    seen.insert(tpl);
                }
                for lit in into_literals(window) {
                    if !lit.is_empty() && !lit.contains('{') {
                        seen.insert(lit);
                    }
                }
                rest = &rest[pos + 1..];
            }
        }
        assert!(seen.len() > 300, "the scan found {} messages", seen.len());
        let missing: Vec<_> = seen
            .iter()
            .filter(|m| !pt.messages.contains_key(*m))
            .collect();
        assert!(missing.is_empty(), "pt-BR missing messages: {missing:?}");
    }

    #[test]
    fn status_value_dictionary_is_complete() {
        let en = english();
        let pt = builtin("pt-BR").unwrap();
        let missing: Vec<_> = en
            .values
            .keys()
            .filter(|k| !pt.values.contains_key(*k))
            .collect();
        assert!(missing.is_empty(), "pt-BR missing values: {missing:?}");
        assert!(!en.values.is_empty());
    }

    #[test]
    fn every_literal_ui_key_exists_in_both_catalogs() {
        let en = english();
        let pt = builtin("pt-BR").unwrap();
        let mut keys = std::collections::BTreeSet::new();
        for source in [include_str!("gui.rs"), include_str!("app.rs")] {
            let mut rest = source;
            while let Some(pos) = rest.find(".tr(\"") {
                let tail = &rest[pos + 5..];
                if let Some(end) = tail.find('"') {
                    keys.insert(tail[..end].to_string());
                }
                rest = tail;
            }
        }
        assert!(keys.contains("help.title"), "the scan found no keys");
        for key in &keys {
            assert!(en.strings.contains_key(key), "en.json missing {key}");
            assert!(pt.strings.contains_key(key), "pt-BR.json missing {key}");
        }
        assert!(
            pt.missing_against(&en).is_empty(),
            "pt-BR is missing source keys"
        );
    }

    #[test]
    fn missing_against_reports_untranslated_source_keys() {
        let en = english();
        let pt = builtin("pt-BR").unwrap();
        assert!(pt.missing_against(&en).is_empty());
        let partial = Catalog::default();
        assert_eq!(partial.missing_against(&en).len(), en.strings.len());
    }
}
