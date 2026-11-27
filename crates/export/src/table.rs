//! Guessing which table a SELECT reads.

/// The table after the first `FROM` of the SQL, as it is written there
/// (`orders`, `sales.orders`, `"Order Lines"`). `None` when the SQL reads
/// from something that is not a plain table.
pub fn guess_table(sql: &str) -> Option<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut depth = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or('\0');
        if c == '-' && next == '-' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == '*' {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if c == '\'' || c == '"' || c == '`' {
            i = skip_quoted(&chars, i, c);
        } else if c == '(' {
            depth += 1;
            i += 1;
        } else if c == ')' {
            depth = depth.saturating_sub(1);
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if depth == 0 && word.eq_ignore_ascii_case("from") {
                return name_at(&chars, i);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Index after the closing quote; a doubled quote stays inside.
fn skip_quoted(chars: &[char], start: usize, quote: char) -> usize {
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == quote {
            if chars.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    chars.len()
}

/// A name, with its dots and quotes, after the spaces at `start`.
fn name_at(chars: &[char], start: usize) -> Option<String> {
    let mut i = start;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    let begin = i;
    loop {
        match chars.get(i).copied() {
            Some(quote @ ('"' | '`')) => i = skip_quoted(chars, i, quote),
            Some('[') => {
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                i = (i + 1).min(chars.len());
            }
            Some(c) if c.is_alphanumeric() || c == '_' || c == '$' => {
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
                {
                    i += 1;
                }
            }
            _ => break,
        }
        if chars.get(i) == Some(&'.') {
            i += 1;
        } else {
            break;
        }
    }
    let name: String = chars[begin..i].iter().collect();
    let name = name.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::guess_table;

    #[test]
    fn the_table_after_from_is_found() {
        let guess = |sql| guess_table(sql).unwrap_or_default();
        assert_eq!(guess("select * from orders"), "orders");
        assert_eq!(
            guess("SELECT a, b\nFROM sales.orders o WHERE a = 1"),
            "sales.orders"
        );
        assert_eq!(
            guess("select * from \"Order Lines\" limit 5"),
            "\"Order Lines\""
        );
        assert_eq!(
            guess("select * from [dbo].[Order Lines]"),
            "[dbo].[Order Lines]"
        );
        assert_eq!(guess("select * from `shop`.`items`;"), "`shop`.`items`");
        assert_eq!(
            guess("select (select max(x) from other) m, 'from here' -- from no\nfrom real"),
            "real"
        );
    }

    #[test]
    fn no_table_is_guessed_without_one() {
        assert_eq!(guess_table("select 1"), None);
        assert_eq!(guess_table("select * from (select 1) t"), None);
        assert_eq!(guess_table("select * from"), None);
    }
}
