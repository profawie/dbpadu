//! Helpers for SQL text: tidy formatting, and finding the statement the
//! cursor is in.

use std::ops::Range;

/// Lay the SQL out with one clause per line and keywords in capitals.
pub fn format(sql: &str) -> String {
    let options = sqlformat::FormatOptions {
        uppercase: Some(true),
        ..Default::default()
    };
    sqlformat::format(sql, &sqlformat::QueryParams::None, &options)
}

/// Where each statement is in `text`, in bytes, without the `;` that ends
/// it. A `;` inside a string or a comment does not end a statement.
pub fn statements(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |at| i + 2 + at + 2);
            }
            quote @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == quote {
                        // A doubled quote stays inside the string.
                        if bytes.get(i + 1) == Some(&quote) {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            b';' => {
                ranges.push(start..i);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    ranges.push(start..bytes.len().max(start));
    ranges
}

/// The statement the cursor is in, trimmed. `cursor` counts characters, the
/// way a text field does. With the cursor after the last `;`, it is the
/// statement before it. `None` when there is nothing to run.
pub fn statement_at(text: &str, cursor: usize) -> Option<&str> {
    let at = text
        .char_indices()
        .nth(cursor)
        .map_or(text.len(), |(index, _)| index);
    let ranges = statements(text);
    // The `;` itself counts as part of the statement it ends.
    let current = ranges
        .iter()
        .position(|range| at <= range.end)
        .unwrap_or(ranges.len() - 1);
    ranges[..=current]
        .iter()
        .rev()
        .map(|range| text[range.clone()].trim())
        .find(|statement| has_code(statement))
        .or_else(|| {
            ranges[current..]
                .iter()
                .map(|range| text[range.clone()].trim())
                .find(|statement| has_code(statement))
        })
}

/// False for text that is empty or only comments.
fn has_code(statement: &str) -> bool {
    let mut rest = statement.trim_start();
    loop {
        if let Some(comment) = rest.strip_prefix("--") {
            rest = comment
                .split_once('\n')
                .map_or("", |(_, after)| after)
                .trim_start();
        } else if let Some(comment) = rest.strip_prefix("/*") {
            rest = comment
                .split_once("*/")
                .map_or("", |(_, after)| after)
                .trim_start();
        } else {
            return !rest.is_empty();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_capital_keywords_and_one_clause_per_line() {
        let formatted = format("select id, name from users where id = 1 order by name");
        assert!(formatted.starts_with("SELECT"), "{formatted}");
        assert!(formatted.contains("\nFROM"), "{formatted}");
        assert!(formatted.contains("\nWHERE"), "{formatted}");
        assert!(formatted.contains("\nORDER BY"), "{formatted}");
    }

    #[test]
    fn formatting_keeps_strings_as_they_are() {
        let formatted = format("select 'from  where' from t");
        assert!(formatted.contains("'from  where'"), "{formatted}");
    }

    #[test]
    fn picks_the_statement_under_the_cursor() {
        let text = "select 1;\nselect 2;\nselect 3";
        assert_eq!(statement_at(text, 0), Some("select 1"));
        assert_eq!(statement_at(text, 8), Some("select 1"));
        assert_eq!(statement_at(text, 12), Some("select 2"));
        assert_eq!(statement_at(text, 999), Some("select 3"));
    }

    #[test]
    fn after_the_last_semicolon_it_is_the_statement_before() {
        assert_eq!(statement_at("select 1;\n", 10), Some("select 1"));
        assert_eq!(statement_at("select 1;  -- done", 18), Some("select 1"));
    }

    #[test]
    fn semicolons_in_strings_and_comments_do_not_split() {
        let text = "select 'a;b' from t; -- x; y\nselect \"c;d\" /* ; */ from u";
        let found: Vec<&str> = statements(text)
            .into_iter()
            .map(|r| text[r].trim())
            .collect();
        assert_eq!(
            found,
            [
                "select 'a;b' from t",
                "-- x; y\nselect \"c;d\" /* ; */ from u"
            ]
        );
    }

    #[test]
    fn counts_characters_not_bytes() {
        let text = "select 'é';select 2";
        assert_eq!(statement_at(text, 10), Some("select 'é'"));
        assert_eq!(statement_at(text, 11), Some("select 2"));
    }

    #[test]
    fn nothing_to_run() {
        assert_eq!(statement_at("", 0), None);
        assert_eq!(statement_at("  -- just a note\n", 3), None);
    }
}
