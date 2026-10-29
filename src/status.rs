//! A status-line message: English text plus the template it came from.
//!
//! The English text is what tests and logs see. The template lets the GUI
//! re-render the message in the active language without the model knowing
//! anything about translations: values are substituted back into the translated
//! literal segments, in order.

use std::fmt;
use std::ops::Deref;

use crate::i18n::Translator;

/// Build a `Status` from a literal format template. `modaline::status!` is the
/// version the shell modules use (they live in the binary crate).
#[macro_export]
macro_rules! status {
    ($tpl:literal $(, $arg:expr)* $(,)?) => {
        $crate::status::Status::new($tpl, format!($tpl $(, $arg)*))
    };
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// Rendered English message.
    text: String,
    /// English `format!` template (equal to `text` for plain literals).
    template: String,
}

impl Status {
    pub fn new(template: &str, text: String) -> Self {
        Status {
            text,
            template: template.to_string(),
        }
    }

    /// Re-render in `tr`'s language. Falls back to the English text when the
    /// template is not translated or the placeholder shapes do not match.
    pub fn render(&self, tr: &Translator) -> String {
        let translated = tr.message(&self.template);
        if translated == self.template {
            return self.text.clone();
        }
        substitute(&self.template, &translated, &self.text, tr).unwrap_or_else(|| self.text.clone())
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl From<String> for Status {
    fn from(text: String) -> Self {
        Status {
            template: text.clone(),
            text,
        }
    }
}

impl From<&str> for Status {
    fn from(text: &str) -> Self {
        Status::from(text.to_string())
    }
}

impl Deref for Status {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl PartialEq<&str> for Status {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl PartialEq<str> for Status {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

/// Split a format template into its literal segments, treating `{...}` as one
/// placeholder and `{{`/`}}` as literal braces.
fn literal_parts(template: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                cur.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                cur.push('}');
            }
            '{' => {
                parts.push(std::mem::take(&mut cur));
                // Skip to the closing brace of this placeholder.
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
            }
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    parts
}

/// Rebuild `text` using the translated literal segments. `None` means the
/// shapes disagree, so the caller keeps the English text.
fn substitute(template: &str, translated: &str, text: &str, tr: &Translator) -> Option<String> {
    let src = literal_parts(template);
    let dst = literal_parts(translated);
    if src.len() != dst.len() {
        return None;
    }
    let mut out = String::new();
    let mut cursor = 0;
    for (i, (from, to)) in src.iter().zip(dst.iter()).enumerate() {
        let start = if from.is_empty() {
            cursor
        } else {
            cursor + text[cursor..].find(from.as_str())?
        };
        if i > 0 {
            out.push_str(&tr.value(&text[cursor..start]));
        }
        out.push_str(to);
        cursor = start + from.len();
    }
    out.push_str(&tr.value(&text[cursor..]));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalog, Translator};
    use std::collections::BTreeMap;

    fn translator(messages: &[(&str, &str)]) -> Translator {
        let catalog = Catalog {
            messages: messages
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect::<BTreeMap<_, _>>(),
            ..Catalog::default()
        };
        Translator::new(catalog)
    }

    #[test]
    fn translated_templates_keep_the_rendered_values() {
        let s = Status::new("layer {} -> {}", "layer 1 -> 2".into());
        let tr = translator(&[("layer {} -> {}", "camada {} -> {}")]);
        assert_eq!(s.render(&tr), "camada 1 -> 2");
        assert_eq!(s.text(), "layer 1 -> 2", "English stays available");
    }

    #[test]
    fn enum_values_inside_messages_are_translated() {
        let s = Status::new("ink: {}", "ink: simple".into());
        let mut catalog = Catalog::default();
        catalog
            .messages
            .insert("ink: {}".into(), "tinta: {}".into());
        catalog.values.insert("simple".into(), "simples".into());
        let tr = Translator::new(catalog);
        assert_eq!(s.render(&tr), "tinta: simples");
    }

    #[test]
    fn plain_literals_translate_without_placeholders() {
        let s = Status::from("no filter running");
        let tr = translator(&[("no filter running", "nenhum filtro em execução")]);
        assert_eq!(s.render(&tr), "nenhum filtro em execução");
    }

    #[test]
    fn mismatched_or_missing_templates_fall_back_to_english() {
        let s = Status::new("a {} b", "a 1 b".into());
        let tr = translator(&[("a {} b", "x {} {}")]);
        assert_eq!(s.render(&tr), "a 1 b");
        assert_eq!(
            Status::from("plain").render(&Translator::default()),
            "plain"
        );
    }

    #[test]
    fn escaped_braces_are_literal() {
        let s = Status::new("set {{x}} {} ", "set {x} 1 ".into());
        let tr = translator(&[("set {{x}} {} ", "def {{x}} {} ")]);
        assert_eq!(s.render(&tr), "def {x} 1 ");
    }
}
