//! Writes a result in a [`Format`], row by row.

use std::io::{self, Write};

use serde::Deserialize;

use crate::{
    format::Format,
    value::{self, Kind},
};

/// A column of a result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Column {
    pub name: String,
    /// The type as the database names it, such as `int4` or `VARCHAR`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Column {
    pub fn new(name: impl Into<String>, type_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_name: type_name.into(),
        }
    }
}

/// How a database wants names with spaces or capitals written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quote {
    /// `"name"`: the SQL standard.
    #[default]
    Double,
    /// `` `name` ``: MySQL and MariaDB.
    Backtick,
    /// `[name]`: SQL Server.
    Bracket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The table the SQL formats name, as it should be written.
    pub table: String,
    /// The columns an `UPDATE` finds its row by. When none of them is in
    /// the result, a column called `id` is used, and without one, every
    /// column.
    pub keys: Vec<String>,
    /// Whether the separated formats start with the column names.
    pub header: bool,
    pub quote: Quote,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            table: String::new(),
            keys: Vec::new(),
            header: true,
            quote: Quote::Double,
        }
    }
}

/// The name used when no table is given.
const TABLE: &str = "my_table";
/// Rows in one `INSERT` of [`Format::SqlInsertMultiRow`]. SQL Server takes
/// no more than 1000.
const ROWS_PER_INSERT: u64 = 500;

/// Words that cannot be a column name without quotes.
const RESERVED: &[&str] = &[
    "all",
    "and",
    "as",
    "asc",
    "between",
    "by",
    "case",
    "check",
    "column",
    "create",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "from",
    "group",
    "having",
    "in",
    "index",
    "insert",
    "into",
    "is",
    "join",
    "key",
    "like",
    "limit",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "primary",
    "references",
    "select",
    "set",
    "table",
    "then",
    "to",
    "union",
    "unique",
    "update",
    "user",
    "values",
    "when",
    "where",
];

/// A column name as SQL. Plain names stay as they are; others are quoted.
fn identifier(name: &str, quote: Quote) -> String {
    let simple = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        // Mixed case only survives in quotes.
        && (name.chars().all(|c| !c.is_ascii_uppercase())
            || name.chars().all(|c| !c.is_ascii_lowercase()))
        && !RESERVED.contains(&name.to_ascii_lowercase().as_str());
    if simple {
        return name.to_string();
    }
    match quote {
        Quote::Double => format!("\"{}\"", name.replace('"', "\"\"")),
        Quote::Backtick => format!("`{}`", name.replace('`', "``")),
        Quote::Bracket => format!("[{}]", name.replace(']', "]]")),
    }
}

/// Writes a result in one format. Give it every row with [`Writer::row`],
/// then call [`Writer::finish`].
pub struct Writer<W: Write> {
    out: W,
    format: Format,
    columns: Vec<Column>,
    kinds: Vec<Kind>,
    /// The column names as SQL.
    names: Vec<String>,
    table: String,
    /// Positions of the key columns.
    keys: Vec<usize>,
    rows: u64,
    /// The line being put together.
    line: String,
    /// The rows of a text table, kept until the widths are known.
    held: Vec<Vec<Option<String>>>,
    /// `WHERE` over one column: an `IN (` was opened, a NULL was seen.
    list_open: bool,
    saw_null: bool,
}

impl<W: Write> Writer<W> {
    pub fn new(
        mut out: W,
        format: Format,
        columns: &[Column],
        options: &Options,
    ) -> io::Result<Self> {
        let names: Vec<String> = columns
            .iter()
            .map(|column| identifier(&column.name, options.quote))
            .collect();
        let position = |name: &str| columns.iter().position(|column| column.name == name);
        let mut keys: Vec<usize> = options
            .keys
            .iter()
            .filter_map(|key| position(key))
            .collect();
        if keys.is_empty() {
            keys = columns
                .iter()
                .position(|column| column.name.eq_ignore_ascii_case("id"))
                .map_or_else(|| (0..columns.len()).collect(), |id| vec![id]);
        }
        let table = match options.table.trim() {
            "" => TABLE.to_string(),
            table => table.to_string(),
        };

        let mut line = String::new();
        match format {
            Format::Tsv | Format::Csv | Format::Semicolon | Format::Pipe if options.header => {
                let delimiter = format.delimiter().expect("a separated format");
                for (index, column) in columns.iter().enumerate() {
                    if index > 0 {
                        line.push(delimiter);
                    }
                    value::delimited(&mut line, &column.name, delimiter);
                }
                line.push('\n');
            }
            Format::Json => line.push('['),
            Format::Xml => line.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<data>\n"),
            Format::Html => {
                line.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>");
                value::markup(&mut line, &table);
                line.push_str(
                    "</title>\n<style>\n\
                     table { border-collapse: collapse; font-family: sans-serif; font-size: 14px; }\n\
                     th, td { border: 1px solid #c8ccd0; padding: 4px 8px; text-align: left; }\n\
                     th { background: #eef0f2; }\n\
                     td.null { color: #8a9099; font-style: italic; }\n\
                     </style>\n</head>\n<body>\n<table>\n<thead>\n<tr>",
                );
                for column in columns {
                    line.push_str("<th>");
                    value::markup(&mut line, &column.name);
                    line.push_str("</th>");
                }
                line.push_str("</tr>\n</thead>\n<tbody>\n");
            }
            Format::Markdown => {
                line.push('|');
                for column in columns {
                    line.push(' ');
                    value::markdown(&mut line, &column.name);
                    line.push_str(" |");
                }
                line.push_str("\n|");
                for _ in columns {
                    line.push_str(" --- |");
                }
                line.push('\n');
            }
            _ => {}
        }
        out.write_all(line.as_bytes())?;
        Ok(Self {
            out,
            format,
            kinds: columns
                .iter()
                .map(|column| value::kind(&column.type_name))
                .collect(),
            columns: columns.to_vec(),
            names,
            table,
            keys,
            rows: 0,
            line,
            held: Vec::new(),
            list_open: false,
            saw_null: false,
        })
    }

    /// Write one row. Values beyond the columns are left out; missing ones
    /// are NULL.
    pub fn row(&mut self, row: &[Option<&str>]) -> io::Result<()> {
        let count = self.columns.len();
        let cell = |index: usize| row.get(index).copied().flatten();
        let mut line = std::mem::take(&mut self.line);
        line.clear();
        match self.format {
            Format::Tsv | Format::Csv | Format::Semicolon | Format::Pipe => {
                let delimiter = self.format.delimiter().expect("a separated format");
                for index in 0..count {
                    if index > 0 {
                        line.push(delimiter);
                    }
                    value::delimited(&mut line, cell(index).unwrap_or(""), delimiter);
                }
                line.push('\n');
            }
            Format::SqlInsert => {
                self.insert_head(&mut line);
                line.push(' ');
                self.values(&mut line, row);
                line.push_str(";\n");
            }
            Format::SqlInsertMultiRow => {
                if self.rows.is_multiple_of(ROWS_PER_INSERT) {
                    if self.rows > 0 {
                        line.push_str(";\n");
                    }
                    self.insert_head(&mut line);
                } else {
                    line.push(',');
                }
                line.push_str("\n  ");
                self.values(&mut line, row);
            }
            Format::SqlUpdate => {
                // Without a column that is not a key, every column is set.
                let all = self.keys.len() == count;
                line.push_str("UPDATE ");
                line.push_str(&self.table);
                line.push_str(" SET ");
                let mut first = true;
                for index in (0..count).filter(|index| all || !self.keys.contains(index)) {
                    if !first {
                        line.push_str(", ");
                    }
                    first = false;
                    line.push_str(&self.names[index]);
                    line.push_str(" = ");
                    value::sql(&mut line, cell(index), self.kinds[index]);
                }
                line.push_str(" WHERE ");
                let keys = self.keys.clone();
                self.conditions(&mut line, row, &keys);
                line.push_str(";\n");
            }
            Format::Where if count == 1 => match cell(0) {
                None => self.saw_null = true,
                Some(text) => {
                    if self.list_open {
                        line.push_str(", ");
                    } else {
                        line.push_str("WHERE ");
                        line.push_str(&self.names[0]);
                        line.push_str(" IN (");
                        self.list_open = true;
                    }
                    value::sql(&mut line, Some(text), self.kinds[0]);
                }
            },
            Format::Where => {
                line.push_str(if self.rows == 0 {
                    "WHERE ("
                } else {
                    "\n   OR ("
                });
                let every: Vec<usize> = (0..count).collect();
                self.conditions(&mut line, row, &every);
                line.push(')');
            }
            Format::OneRow => {
                for index in 0..count {
                    if self.rows > 0 || index > 0 {
                        line.push_str(", ");
                    }
                    value::sql(&mut line, cell(index), self.kinds[index]);
                }
            }
            Format::Json => {
                line.push_str(if self.rows == 0 { "\n  {" } else { ",\n  {" });
                for index in 0..count {
                    if index > 0 {
                        line.push_str(", ");
                    }
                    value::json_string(&mut line, &self.columns[index].name);
                    line.push_str(": ");
                    value::json(&mut line, cell(index), self.kinds[index]);
                }
                line.push('}');
            }
            Format::Xml => {
                line.push_str("  <row>\n");
                for index in 0..count {
                    line.push_str("    <column name=\"");
                    value::markup(&mut line, &self.columns[index].name);
                    match cell(index) {
                        None => line.push_str("\" null=\"true\"/>\n"),
                        Some(text) => {
                            line.push_str("\">");
                            value::markup(&mut line, text);
                            line.push_str("</column>\n");
                        }
                    }
                }
                line.push_str("  </row>\n");
            }
            Format::Html => {
                line.push_str("<tr>");
                for index in 0..count {
                    match cell(index) {
                        None => line.push_str("<td class=\"null\">NULL</td>"),
                        Some(text) => {
                            line.push_str("<td>");
                            value::markup(&mut line, text);
                            line.push_str("</td>");
                        }
                    }
                }
                line.push_str("</tr>\n");
            }
            Format::Markdown => {
                line.push('|');
                for index in 0..count {
                    line.push(' ');
                    value::markdown(&mut line, cell(index).unwrap_or("NULL"));
                    line.push_str(" |");
                }
                line.push('\n');
            }
            Format::Pretty => self.held.push(
                (0..count)
                    .map(|index| cell(index).map(value::one_line))
                    .collect(),
            ),
        }
        self.rows += 1;
        self.out.write_all(line.as_bytes())?;
        self.line = line;
        Ok(())
    }

    /// Write what closes the format. Returns how many rows were written.
    pub fn finish(mut self) -> io::Result<u64> {
        let mut line = String::new();
        match self.format {
            Format::SqlInsertMultiRow if self.rows > 0 => line.push_str(";\n"),
            Format::Where if self.columns.len() == 1 => {
                if self.list_open {
                    line.push(')');
                    if self.saw_null {
                        line.push_str(" OR ");
                    }
                } else if self.saw_null {
                    line.push_str("WHERE ");
                }
                if self.saw_null {
                    line.push_str(&self.names[0]);
                    line.push_str(" IS NULL");
                }
                if self.rows > 0 {
                    line.push('\n');
                }
            }
            Format::Where | Format::OneRow if self.rows > 0 => line.push('\n'),
            Format::Json => line.push_str(if self.rows == 0 { "]\n" } else { "\n]\n" }),
            Format::Xml => line.push_str("</data>\n"),
            Format::Html => line.push_str("</tbody>\n</table>\n</body>\n</html>\n"),
            Format::Pretty => self.pretty(&mut line),
            _ => {}
        }
        self.out.write_all(line.as_bytes())?;
        self.out.flush()?;
        Ok(self.rows)
    }

    /// `INSERT INTO t (a, b) VALUES`
    fn insert_head(&self, line: &mut String) {
        line.push_str("INSERT INTO ");
        line.push_str(&self.table);
        line.push_str(" (");
        line.push_str(&self.names.join(", "));
        line.push_str(") VALUES");
    }

    /// `(1, 'x', NULL)`
    fn values(&self, line: &mut String, row: &[Option<&str>]) {
        line.push('(');
        for index in 0..self.columns.len() {
            if index > 0 {
                line.push_str(", ");
            }
            value::sql(line, row.get(index).copied().flatten(), self.kinds[index]);
        }
        line.push(')');
    }

    /// `a = 1 AND b IS NULL`
    fn conditions(&self, line: &mut String, row: &[Option<&str>], columns: &[usize]) {
        for (position, &index) in columns.iter().enumerate() {
            if position > 0 {
                line.push_str(" AND ");
            }
            line.push_str(&self.names[index]);
            match row.get(index).copied().flatten() {
                None => line.push_str(" IS NULL"),
                Some(text) => {
                    line.push_str(" = ");
                    value::sql(line, Some(text), self.kinds[index]);
                }
            }
        }
    }

    /// The text table, now that every row is known.
    fn pretty(&self, out: &mut String) {
        let width = |text: &str| text.chars().count();
        let mut widths: Vec<usize> = self
            .columns
            .iter()
            .map(|column| width(&column.name))
            .collect();
        for row in &self.held {
            for (index, cell) in row.iter().enumerate() {
                widths[index] = widths[index].max(cell.as_deref().map_or(4, width));
            }
        }
        let rule = |out: &mut String| {
            out.push('+');
            for width in &widths {
                out.extend(std::iter::repeat_n('-', width + 2));
                out.push('+');
            }
            out.push('\n');
        };
        let cell = |out: &mut String, text: &str, index: usize, right: bool| {
            let pad = widths[index] - width(text);
            out.push(' ');
            if right {
                out.extend(std::iter::repeat_n(' ', pad));
                out.push_str(text);
            } else {
                out.push_str(text);
                out.extend(std::iter::repeat_n(' ', pad));
            }
            out.push_str(" |");
        };
        rule(out);
        out.push('|');
        for (index, column) in self.columns.iter().enumerate() {
            cell(out, &column.name, index, false);
        }
        out.push('\n');
        rule(out);
        for row in &self.held {
            out.push('|');
            for (index, value) in row.iter().enumerate() {
                let text = value.as_deref().unwrap_or("NULL");
                // Numbers line up on the right.
                let right =
                    value.is_some() && self.kinds[index] == Kind::Number && value::is_number(text);
                cell(out, text, index, right);
            }
            out.push('\n');
        }
        rule(out);
    }
}

/// A result as text, for the clipboard: without the line break at the end.
pub fn to_string<'a, R>(
    format: Format,
    columns: &[Column],
    rows: impl IntoIterator<Item = R>,
    options: &Options,
) -> String
where
    R: AsRef<[Option<&'a str>]>,
{
    let mut out = Vec::new();
    // Writing to memory does not fail.
    let mut writer = Writer::new(&mut out, format, columns, options).expect("memory");
    for row in rows {
        writer.row(row.as_ref()).expect("memory");
    }
    writer.finish().expect("memory");
    let mut text = String::from_utf8(out).expect("made of strings");
    while text.ends_with('\n') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns() -> Vec<Column> {
        vec![
            Column::new("id", "int4"),
            Column::new("name", "varchar"),
            Column::new("Paid At", "timestamp"),
        ]
    }

    fn rows() -> Vec<Vec<Option<&'static str>>> {
        vec![
            vec![Some("1"), Some("Ali"), Some("2026-01-02 03:04:05")],
            vec![Some("2"), Some("O'Neil, \"Sam\""), None],
        ]
    }

    fn text(format: Format, options: &Options) -> String {
        to_string(format, &columns(), rows(), options)
    }

    fn orders() -> Options {
        Options {
            table: "orders".to_string(),
            ..Options::default()
        }
    }

    #[test]
    fn separated_formats() {
        assert_eq!(
            text(Format::Csv, &orders()),
            "id,name,Paid At\n1,Ali,2026-01-02 03:04:05\n2,\"O'Neil, \"\"Sam\"\"\","
        );
        assert_eq!(
            text(Format::Tsv, &orders()),
            "id\tname\tPaid At\n1\tAli\t2026-01-02 03:04:05\n2\t\"O'Neil, \"\"Sam\"\"\"\t"
        );
        let bare = Options {
            header: false,
            ..orders()
        };
        assert_eq!(
            text(Format::Semicolon, &bare),
            "1;Ali;2026-01-02 03:04:05\n2;\"O'Neil, \"\"Sam\"\"\";"
        );
        assert!(text(Format::Pipe, &bare).starts_with("1|Ali|"));
    }

    #[test]
    fn sql_inserts() {
        assert_eq!(
            text(Format::SqlInsert, &orders()),
            "INSERT INTO orders (id, name, \"Paid At\") VALUES (1, 'Ali', '2026-01-02 03:04:05');\n\
             INSERT INTO orders (id, name, \"Paid At\") VALUES (2, 'O''Neil, \"Sam\"', NULL);"
        );
        assert_eq!(
            text(Format::SqlInsertMultiRow, &orders()),
            "INSERT INTO orders (id, name, \"Paid At\") VALUES\n  \
             (1, 'Ali', '2026-01-02 03:04:05'),\n  \
             (2, 'O''Neil, \"Sam\"', NULL);"
        );
    }

    #[test]
    fn many_row_inserts_start_again_every_500_rows() {
        let columns = [Column::new("n", "integer")];
        let numbers: Vec<String> = (0..1001).map(|n| n.to_string()).collect();
        let rows = numbers.iter().map(|n| [Some(n.as_str())]);
        let text = to_string(
            Format::SqlInsertMultiRow,
            &columns,
            rows,
            &Options::default(),
        );
        assert_eq!(text.matches("INSERT INTO my_table (n) VALUES").count(), 3);
        assert_eq!(text.matches(';').count(), 3);
        assert!(text.contains("  (499);\nINSERT INTO my_table (n) VALUES\n  (500),"));
        assert!(text.ends_with("  (1000);"));
    }

    #[test]
    fn sql_updates_find_the_row_by_its_keys() {
        // No keys given: the column called id.
        assert_eq!(
            text(Format::SqlUpdate, &orders()).lines().next().unwrap(),
            "UPDATE orders SET name = 'Ali', \"Paid At\" = '2026-01-02 03:04:05' WHERE id = 1;"
        );
        let by_name = Options {
            keys: vec!["name".to_string(), "Paid At".to_string()],
            quote: Quote::Bracket,
            ..orders()
        };
        assert_eq!(
            text(Format::SqlUpdate, &by_name).lines().nth(1).unwrap(),
            "UPDATE orders SET id = 2 WHERE name = 'O''Neil, \"Sam\"' AND [Paid At] IS NULL;"
        );
        // No id and no keys: every column finds the row, and is set.
        let columns = [Column::new("a", "int"), Column::new("b", "text")];
        assert_eq!(
            to_string(
                Format::SqlUpdate,
                &columns,
                [[Some("1"), Some("x")]],
                &Options::default()
            ),
            "UPDATE my_table SET a = 1, b = 'x' WHERE a = 1 AND b = 'x';"
        );
    }

    #[test]
    fn where_clauses() {
        assert_eq!(
            text(Format::Where, &orders()),
            "WHERE (id = 1 AND name = 'Ali' AND \"Paid At\" = '2026-01-02 03:04:05')\n   \
             OR (id = 2 AND name = 'O''Neil, \"Sam\"' AND \"Paid At\" IS NULL)"
        );
        let one = |rows: &[[Option<&'static str>; 1]]| {
            to_string(
                Format::Where,
                &[Column::new("order", "int4")],
                rows,
                &Options {
                    quote: Quote::Backtick,
                    ..Options::default()
                },
            )
        };
        assert_eq!(one(&[[Some("1")], [Some("2")]]), "WHERE `order` IN (1, 2)");
        assert_eq!(
            one(&[[Some("1")], [None]]),
            "WHERE `order` IN (1) OR `order` IS NULL"
        );
        assert_eq!(one(&[[None]]), "WHERE `order` IS NULL");
        assert_eq!(one(&[]), "");
    }

    #[test]
    fn one_line_of_values() {
        assert_eq!(
            text(Format::OneRow, &orders()),
            "1, 'Ali', '2026-01-02 03:04:05', 2, 'O''Neil, \"Sam\"', NULL"
        );
    }

    #[test]
    fn json_is_valid_and_keeps_numbers_as_numbers() {
        let json = text(Format::Json, &orders());
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(
            value,
            serde_json::json!([
                {"id": 1, "name": "Ali", "Paid At": "2026-01-02 03:04:05"},
                {"id": 2, "name": "O'Neil, \"Sam\"", "Paid At": null},
            ])
        );
        let empty = to_string(
            Format::Json,
            &columns(),
            Vec::<Vec<Option<&str>>>::new(),
            &orders(),
        );
        assert_eq!(empty, "[]");
    }

    #[test]
    fn xml_and_html() {
        let xml = text(Format::Xml, &orders());
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<data>\n  <row>\n"));
        assert!(xml.contains("    <column name=\"name\">O'Neil, &quot;Sam&quot;</column>\n"));
        assert!(xml.contains("    <column name=\"Paid At\" null=\"true\"/>\n"));
        assert!(xml.ends_with("</data>"));

        let html = text(Format::Html, &orders());
        assert!(html.contains("<tr><th>id</th><th>name</th><th>Paid At</th></tr>"));
        assert!(html.contains(
            "<tr><td>2</td><td>O'Neil, &quot;Sam&quot;</td><td class=\"null\">NULL</td></tr>"
        ));
        assert!(html.ends_with("</html>"));
    }

    #[test]
    fn markdown_and_text_tables() {
        assert_eq!(
            text(Format::Markdown, &orders()),
            "| id | name | Paid At |\n\
             | --- | --- | --- |\n\
             | 1 | Ali | 2026-01-02 03:04:05 |\n\
             | 2 | O'Neil, \"Sam\" | NULL |"
        );
        let columns = [Column::new("n", "int"), Column::new("word", "text")];
        let rows = [[Some("5"), Some("a\nb")], [Some("120"), None]];
        assert_eq!(
            to_string(Format::Pretty, &columns, rows, &Options::default()),
            "+-----+------+\n\
             | n   | word |\n\
             +-----+------+\n\
             |   5 | a\\nb |\n\
             | 120 | NULL |\n\
             +-----+------+"
        );
    }

    #[test]
    fn names_are_quoted_only_when_needed() {
        assert_eq!(identifier("total_2", Quote::Double), "total_2");
        assert_eq!(identifier("TOTAL", Quote::Double), "TOTAL");
        assert_eq!(identifier("Total", Quote::Double), "\"Total\"");
        assert_eq!(identifier("order", Quote::Double), "\"order\"");
        assert_eq!(identifier("2nd", Quote::Backtick), "`2nd`");
        assert_eq!(identifier("a]b", Quote::Bracket), "[a]]b]");
        assert_eq!(identifier("say \"x\"", Quote::Double), "\"say \"\"x\"\"\"");
    }

    #[test]
    fn short_and_long_rows_are_tolerated() {
        let columns = [Column::new("a", "int"), Column::new("b", "int")];
        let rows: [&[Option<&str>]; 2] = [&[Some("1")], &[Some("2"), Some("3"), Some("4")]];
        assert_eq!(
            to_string(Format::Csv, &columns, rows, &Options::default()),
            "a,b\n1,\n2,3"
        );
    }
}
