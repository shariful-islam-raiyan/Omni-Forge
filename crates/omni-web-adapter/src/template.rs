//! A deliberately tiny, safe substitution renderer.
//!
//! This is NOT a template language and never will be: no logic, no loops,
//! no includes, no embedded code of any kind. It only recognizes
//! `{{identifier}}` placeholders, requires every placeholder to be a
//! declared component input, and HTML-escapes every substituted value. That
//! is the entire security and capability boundary of the web-static
//! adapter's rendering step — see docs for why this scope was chosen over
//! embedding a JS engine.

use std::collections::{BTreeMap, BTreeSet};

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

/// Renders `template`, substituting `{{name}}` with the HTML-escaped value
/// of `values[name]`. Every placeholder in the template must be present in
/// `declared`, otherwise rendering fails (this catches typos and prevents a
/// template from silently referencing undefined data).
pub fn render(
    template: &str,
    values: &BTreeMap<String, String>,
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
                    let value = values.get(name).cloned().unwrap_or_default();
                    out.push_str(&escape_html(&value));
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
    fn map(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
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
    fn passes_through_plain_text_and_unicode() {
        let out = render("héllo wörld 🎉 {{x}}", &map(&[("x", "y")]), &set(&["x"])).unwrap();
        assert_eq!(out, "héllo wörld 🎉 y");
    }
}
