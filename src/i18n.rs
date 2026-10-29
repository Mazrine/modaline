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
        };
        custom.save(&path).unwrap();
        assert_eq!(resolve(path.to_str().unwrap()).unwrap(), custom);
        std::fs::write(&path, r#"{"strings":{"brush":"b"}}"#).unwrap();
        let partial = resolve(path.to_str().unwrap()).unwrap();
        assert_eq!(partial.tr("brush"), "b");
        assert!(partial.lang.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
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
