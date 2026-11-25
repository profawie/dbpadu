//! Colors for SQL in the editor: keywords, strings, numbers and comments.

use eframe::egui::{
    FontId,
    text::{LayoutJob, TextFormat},
};

use crate::theme::Theme;

const KEYWORDS: &[&str] = &[
    "ADD",
    "ALL",
    "ALTER",
    "AND",
    "AS",
    "ASC",
    "BEGIN",
    "BETWEEN",
    "BY",
    "CASE",
    "CHECK",
    "COLUMN",
    "COMMIT",
    "CONSTRAINT",
    "CREATE",
    "CROSS",
    "DATABASE",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "DROP",
    "ELSE",
    "END",
    "EXCEPT",
    "EXISTS",
    "FALSE",
    "FETCH",
    "FIRST",
    "FOREIGN",
    "FROM",
    "FULL",
    "GROUP",
    "HAVING",
    "IF",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTO",
    "IS",
    "JOIN",
    "KEY",
    "LEFT",
    "LIKE",
    "LIMIT",
    "NEXT",
    "NOT",
    "NULL",
    "OFFSET",
    "ON",
    "ONLY",
    "OR",
    "ORDER",
    "OUTER",
    "PRIMARY",
    "REFERENCES",
    "RETURNING",
    "RIGHT",
    "ROLLBACK",
    "ROW",
    "ROWS",
    "SELECT",
    "SET",
    "TABLE",
    "THEN",
    "TOP",
    "TRUE",
    "TRUNCATE",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USING",
    "VALUES",
    "VIEW",
    "WHEN",
    "WHERE",
    "WITH",
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Plain,
    Keyword,
    Text,
    Number,
    Comment,
}

/// The pieces of `sql`, each with what it is. Together they are all of
/// `sql`, in order.
fn pieces(sql: &str) -> Vec<(&str, Kind)> {
    let bytes = sql.as_bytes();
    let mut pieces = Vec::new();
    let mut plain_from = 0;
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let kind = match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                Kind::Comment
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = sql[i + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |at| i + 2 + at + 2);
                Kind::Comment
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                Kind::Text
            }
            b'0'..=b'9' => {
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                Kind::Number
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let word = sql[start..i].to_ascii_uppercase();
                if KEYWORDS.binary_search(&word.as_str()).is_ok() {
                    Kind::Keyword
                } else {
                    Kind::Plain
                }
            }
            _ => {
                i += 1;
                // Stay on a character boundary.
                while i < bytes.len() && !sql.is_char_boundary(i) {
                    i += 1;
                }
                Kind::Plain
            }
        };
        if kind != Kind::Plain {
            if plain_from < start {
                pieces.push((&sql[plain_from..start], Kind::Plain));
            }
            pieces.push((&sql[start..i], kind));
            plain_from = i;
        }
    }
    if plain_from < bytes.len() {
        pieces.push((&sql[plain_from..], Kind::Plain));
    }
    pieces
}

/// `sql` laid out with its colors, ready for a text field.
pub fn job(theme: &Theme, sql: &str, font: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (text, kind) in pieces(sql) {
        let color = match kind {
            Kind::Plain => theme.text,
            Kind::Keyword => theme.accent_text,
            Kind::Text => theme.ok,
            Kind::Number => theme.warn,
            Kind::Comment => theme.text_dim,
        };
        job.append(text, 0.0, TextFormat::simple(font.clone(), color));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_are_sorted_for_the_search() {
        assert!(KEYWORDS.is_sorted());
    }

    #[test]
    fn pieces_cover_the_whole_text() {
        for sql in [
            "select 'it''s' from t -- note\nwhere a = 1.5 /* x */",
            "sélect 'é",
            "/* open",
            "",
        ] {
            let joined: String = pieces(sql).iter().map(|(text, _)| *text).collect();
            assert_eq!(joined, sql);
        }
    }

    #[test]
    fn words_are_told_apart() {
        let found = pieces("select name from t where id = 42 and x = 'from' -- from");
        let kind_of = |text: &str| found.iter().find(|(piece, _)| *piece == text).unwrap().1;
        assert_eq!(kind_of("select"), Kind::Keyword);
        assert_eq!(kind_of("42"), Kind::Number);
        assert_eq!(kind_of("'from'"), Kind::Text);
        assert_eq!(kind_of("-- from"), Kind::Comment);
        assert!(
            !found
                .iter()
                .any(|(piece, kind)| *piece == "name" && *kind != Kind::Plain)
        );
    }
}
