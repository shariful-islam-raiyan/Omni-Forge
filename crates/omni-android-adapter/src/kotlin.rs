//! A deliberately tiny, safe substitution renderer for Android component
//! templates — the exact same design as `omni-web-adapter`'s `template.rs`
//! (see ADR 0003 and ADR 0010), just targeting a Kotlin string literal
//! instead of HTML. Only `{{identifier}}` placeholders are recognized,
//! every placeholder must be a declared component input, and every
//! substituted value is escaped for safe inclusion inside a Kotlin
//! double-quoted string literal before being written into generated
//! source. There is no logic, no loops, no conditionals, and nothing that
//! takes an arbitrary string and executes/compiles it beyond the normal
//! Kotlin compiler compiling source `omni` itself generated.

use std::collections::{BTreeMap, BTreeSet};

/// Escapes `input` so it can be embedded inside a Kotlin double-quoted
/// string literal (`"..."`) without breaking out of the literal, without
/// triggering Kotlin's `$`-prefixed string-template interpolation, and
/// without containing a raw newline (illegal in a non-triple-quoted Kotlin
/// string).
pub fn escape_kotlin_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Renders `template`, substituting `{{name}}` with the Kotlin-string-
/// escaped value of `values[name]`. Every placeholder in the template must
/// be present in `declared`, otherwise rendering fails.
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
                    out.push_str(&escape_kotlin_string(&value));
                }
                i += 2 + end + 2;
                continue;
            } else {
                errors.push("unterminated '{{' placeholder in template".to_string());
                break;
            }
        }
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
            "Hello, {{name}}!",
            &map(&[("name", "Android")]),
            &set(&["name"]),
        )
        .unwrap();
        assert_eq!(out, "Hello, Android!");
    }

    #[test]
    fn escapes_quotes_backslashes_and_dollar_signs() {
        let out = escape_kotlin_string(r#"say "hi" \ then $interpolate"#);
        assert_eq!(out, r#"say \"hi\" \\ then \$interpolate"#);
    }

    #[test]
    fn escapes_newlines_and_control_characters() {
        let out = escape_kotlin_string("line1\nline2\ttabbed");
        assert_eq!(out, "line1\\nline2\\ttabbed");
    }

    #[test]
    fn rejects_undeclared_placeholder() {
        let err = render("{{evil}}", &BTreeMap::new(), &set(&["name"])).unwrap_err();
        assert!(err[0].contains("evil"));
    }

    #[test]
    fn a_dollar_sign_cannot_trigger_kotlin_string_interpolation() {
        // If this were inserted unescaped, "${System.exit(1)}" would be
        // live Kotlin string-template interpolation, not text. Escaping the
        // '$' prevents that regardless of what the input looks like.
        let out = render("{{x}}", &map(&[("x", "${System.exit(1)}")]), &set(&["x"])).unwrap();
        // The '$' is escaped to '\$', so Kotlin sees a literal dollar sign
        // followed by '{', never the '${' token that starts interpolation.
        assert_eq!(out, r"\${System.exit(1)}");
    }
}
