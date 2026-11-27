//! How one value is written in each language.

use std::fmt::Write as _;

/// What a column holds, as far as writing its values goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Boolean,
    Json,
    Text,
}

/// Read the kind from the type name the database gave, such as `int4`,
/// `NUMBER`, `decimal(10,2)` or `varchar`.
pub fn kind(type_name: &str) -> Kind {
    let name = type_name.to_ascii_lowercase();
    if name.contains("json") {
        Kind::Json
    } else if name.contains("bool") || name == "bit" {
        Kind::Boolean
    } else if name.contains("interval") || name.contains("point") {
        // Not numbers, although `int` is in the name.
        Kind::Text
    } else if [
        "int", "serial", "numeric", "decimal", "number", "float", "double", "real",
    ]
    .iter()
    .any(|part| name.contains(part))
    {
        Kind::Number
    } else {
        Kind::Text
    }
}

/// `12`, `-0.5`, `1e9`: a number both SQL and JSON read the same way.
pub fn is_number(text: &str) -> bool {
    let rest = text.strip_prefix('-').unwrap_or(text);
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let (mantissa, exponent) = match rest.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (rest, None),
    };
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if !digits(exponent) {
            return false;
        }
    }
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    if !digits(whole) || fraction.is_some_and(|fraction| !digits(fraction)) {
        return false;
    }
    // JSON does not allow 007.
    whole.len() == 1 || !whole.starts_with('0')
}

fn boolean(text: &str) -> Option<bool> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "t" | "1" => Some(true),
        "false" | "f" | "0" => Some(false),
        _ => None,
    }
}

/// A value as SQL: `NULL`, `12`, `TRUE`, `'it''s'`.
pub fn sql(out: &mut String, value: Option<&str>, kind: Kind) {
    let Some(text) = value else {
        out.push_str("NULL");
        return;
    };
    match kind {
        Kind::Number if is_number(text) => out.push_str(text),
        // 1 and 0 stay as they are: SQL Server and Oracle have no TRUE.
        Kind::Boolean if text == "1" || text == "0" => out.push_str(text),
        Kind::Boolean if boolean(text).is_some() => {
            out.push_str(if boolean(text) == Some(true) {
                "TRUE"
            } else {
                "FALSE"
            });
        }
        _ => {
            out.push('\'');
            for c in text.chars() {
                if c == '\'' {
                    out.push('\'');
                }
                out.push(c);
            }
            out.push('\'');
        }
    }
}

/// A value as JSON: `null`, `12`, `true`, `"text"`, or the JSON a JSON
/// column holds.
pub fn json(out: &mut String, value: Option<&str>, kind: Kind) {
    let Some(text) = value else {
        out.push_str("null");
        return;
    };
    match kind {
        Kind::Number if is_number(text) => out.push_str(text),
        Kind::Boolean if boolean(text).is_some() => {
            out.push_str(if boolean(text) == Some(true) {
                "true"
            } else {
                "false"
            });
        }
        Kind::Json if serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok() => {
            out.push_str(text.trim());
        }
        _ => json_string(out, text),
    }
}

pub fn json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A value between the separators of a CSV-like line. It is put in double
/// quotes when it would otherwise be read as more than one value.
pub fn delimited(out: &mut String, text: &str, delimiter: char) {
    let quote =
        text.contains([delimiter, '"', '\n', '\r']) || text.starts_with(' ') || text.ends_with(' ');
    if !quote {
        out.push_str(text);
        return;
    }
    out.push('"');
    for c in text.chars() {
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
    out.push('"');
}

/// Text inside XML or HTML. Characters XML cannot hold are left out.
pub fn markup(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' | '\r' | '\t' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
}

/// Text in a cell of a Markdown table.
pub fn markdown(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '|' => out.push_str("\\|"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("<br>"),
            '\r' => {}
            c => out.push(c),
        }
    }
}

/// Text on one line of a text table: line breaks and tabs are spelled out.
pub fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(write: impl Fn(&mut String)) -> String {
        let mut out = String::new();
        write(&mut out);
        out
    }

    #[test]
    fn kinds_are_read_from_type_names() {
        for name in [
            "int4",
            "INTEGER",
            "bigserial",
            "NUMBER",
            "decimal(10,2)",
            "float8",
        ] {
            assert_eq!(kind(name), Kind::Number, "{name}");
        }
        for name in [
            "varchar",
            "text",
            "timestamp",
            "interval",
            "point",
            "uuid",
            "",
        ] {
            assert_eq!(kind(name), Kind::Text, "{name}");
        }
        assert_eq!(kind("bool"), Kind::Boolean);
        assert_eq!(kind("BIT"), Kind::Boolean);
        assert_eq!(kind("jsonb"), Kind::Json);
    }

    #[test]
    fn numbers_are_told_from_text() {
        for text in ["0", "12", "-3", "1.50", "0.5", "1e9", "2.5E-3"] {
            assert!(is_number(text), "{text}");
        }
        for text in [
            "", "-", "007", "1.", ".5", "1e", "NaN", "Infinity", "1 000", "$5", "0x1F",
        ] {
            assert!(!is_number(text), "{text}");
        }
    }

    #[test]
    fn sql_values() {
        let s = |value, kind| with(|out| sql(out, value, kind));
        assert_eq!(s(None, Kind::Text), "NULL");
        assert_eq!(s(Some("12"), Kind::Number), "12");
        assert_eq!(s(Some("NaN"), Kind::Number), "'NaN'");
        assert_eq!(s(Some("12"), Kind::Text), "'12'");
        assert_eq!(s(Some("it's"), Kind::Text), "'it''s'");
        assert_eq!(s(Some("t"), Kind::Boolean), "TRUE");
        assert_eq!(s(Some("false"), Kind::Boolean), "FALSE");
        assert_eq!(s(Some("1"), Kind::Boolean), "1");
        assert_eq!(s(Some("{\"a\":1}"), Kind::Json), "'{\"a\":1}'");
    }

    #[test]
    fn json_values() {
        let j = |value, kind| with(|out| json(out, value, kind));
        assert_eq!(j(None, Kind::Number), "null");
        assert_eq!(j(Some("1.5"), Kind::Number), "1.5");
        assert_eq!(j(Some("1.5"), Kind::Text), "\"1.5\"");
        assert_eq!(j(Some("f"), Kind::Boolean), "false");
        assert_eq!(j(Some("{\"a\": [1]}"), Kind::Json), "{\"a\": [1]}");
        assert_eq!(j(Some("{broken"), Kind::Json), "\"{broken\"");
        assert_eq!(
            j(Some("a\"b\\c\n\u{1}"), Kind::Text),
            "\"a\\\"b\\\\c\\n\\u0001\""
        );
    }

    #[test]
    fn separated_values_are_quoted_only_when_needed() {
        let d = |text, delimiter| with(|out| delimited(out, text, delimiter));
        assert_eq!(d("plain", ','), "plain");
        assert_eq!(d("a,b", ','), "\"a,b\"");
        assert_eq!(d("a,b", '\t'), "a,b");
        assert_eq!(d("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
        assert_eq!(d("two\nlines", '\t'), "\"two\nlines\"");
        assert_eq!(d(" padded", ','), "\" padded\"");
    }

    #[test]
    fn markup_and_markdown_are_escaped() {
        assert_eq!(
            with(|out| markup(out, "a<b & \"c\"\u{0}")),
            "a&lt;b &amp; &quot;c&quot;"
        );
        assert_eq!(with(|out| markdown(out, "a|b\nc")), "a\\|b<br>c");
        assert_eq!(one_line("a\tb\nc"), "a\\tb\\nc");
    }
}
