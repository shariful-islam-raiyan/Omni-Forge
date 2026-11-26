//! A deliberately tiny, safe substitution renderer.
//!
//! This is NOT a template language and never will be: no logic, no loops,
//! no conditionals, and no mechanism that takes an arbitrary string and
//! executes it. It only recognizes `{{identifier}}` placeholders, requires
//! every placeholder to be either a declared component input or a
//! composition slot (see `ComponentManifest::composes`), and HTML-escapes
//! every *input* value. A composition slot's value is never a raw string
//! from outside the system — it is always the output of this exact same
//! renderer, called recursively on another component (see
//! `lib.rs::render_component` and
//! `docs/decisions/0010-component-composition-via-slots.md`), so nothing
//! unescaped ever reaches a slot except already-escaped HTML this renderer
//! itself produced. That, plus the four tests below and the adapter-level
//! integration tests in `tests/compile.rs`, is the entire security argument
//! for this adapter's rendering step.

use std::collections::{BTreeMap, BTreeSet};

/// A value substituted for one `{{name}}` placeholder.
#[derive(Debug, Clone)]
pub enum SlotValue {
    /// A plain component input: HTML-escaped before insertion.
    Escaped(String),
    /// A composition slot: already-rendered, already-escaped HTML from a
    /// recursive call to this same renderer. Inserted verbatim — never
    /// derived from unescaped external input.
    Raw(String),
}

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Renders `template`, substituting `{{name}}` with `values[name]` —
/// HTML-escaped for [`SlotValue::Escaped`], inserted verbatim for
/// [`SlotValue::Raw`]. Every placeholder in the template must be present in
/// `declared`, otherwise rendering fails (this catches typos and prevents a
/// template from silently referencing undefined data).
pub fn render(
    template: &str,
    values: &BTreeMap<String, SlotValue>,
    declared: &BTreeSet<String>,
) -> Result<String, Vec<String>> {
    let mut out = String::with_capacity(template.len());
    let mut errors = Vec::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = template[i + 2..].find("}}") {
                let name = template[i + 2..i + 2 + end].trim();
                if !declared.contains(name) {
                    errors.push(format!(
                        "template references undeclared input '{{{{{name}}}}}'"
                    ));
                } else {
                    match values.get(name) {
                        Some(SlotValue::Escaped(v)) => out.push_str(&escape_html(v)),
                        Some(SlotValue::Raw(v)) => out.push_str(v),
                        None => {}
                    }
                }
                i += 2 + end + 2;
                continue;
            } else {
                errors.push("unterminated '{{' placeholder in template".to_string());
                break;
            }
        }
        // push one char (not byte, to stay UTF-8 safe)
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&template[i..i + ch_len]);
        i += ch_len;
    }

    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte & 0x80 == 0 {
        1
    } else if first_byte & 0xE0 == 0xC0 {
        2
    } else if first_byte & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
    fn map(items: &[(&str, &str)]) -> BTreeMap<String, SlotValue> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), SlotValue::Escaped(v.to_string())))
            .collect()
    }

    #[test]
    fn substitutes_declared_placeholder() {
        let out = render(
            "<h1>{{title}}</h1>",
            &map(&[("title", "Hi")]),
            &set(&["title"]),
        )
        .unwrap();
        assert_eq!(out, "<h1>Hi</h1>");
    }

    #[test]
    fn escapes_html_special_characters() {
        let out = render("{{x}}", &map(&[("x", "<script>&\"'")]), &set(&["x"])).unwrap();
        assert_eq!(out, "&lt;script&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn rejects_undeclared_placeholder() {
        let err = render("{{evil}}", &BTreeMap::new(), &set(&["title"])).unwrap_err();
        assert!(err[0].contains("evil"));
    }

    #[test]
    fn raw_slot_value_is_inserted_without_escaping() {
        let mut values = BTreeMap::new();
        values.insert(
            "nav".to_string(),
            SlotValue::Raw("<nav>already escaped inside</nav>".to_string()),
        );
        let out = render("<body>{{nav}}</body>", &values, &set(&["nav"])).unwrap();
        assert_eq!(out, "<body><nav>already escaped inside</nav></body>");
    }

    #[test]
    fn passes_through_plain_text_and_unicode() {
        let out = render("héllo wörld 🎉 {{x}}", &map(&[("x", "y")]), &set(&["x"])).unwrap();
        assert_eq!(out, "héllo wörld 🎉 y");
    }
}
