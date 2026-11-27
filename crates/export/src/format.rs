//! The formats a result can be written in.

/// How the formats are grouped in menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Values with a separator between them.
    Delimited,
    Sql,
    /// JSON, XML, HTML.
    Data,
    /// Made to be read by people.
    Text,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Delimited, Group::Sql, Group::Data, Group::Text];

    pub fn label(self) -> &'static str {
        match self {
            Group::Delimited => "Separated values",
            Group::Sql => "SQL",
            Group::Data => "Data",
            Group::Text => "Text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Tabs between the values: what spreadsheets paste best.
    Tsv,
    Csv,
    Semicolon,
    Pipe,
    /// One `INSERT` per row.
    SqlInsert,
    /// One `INSERT` for many rows.
    SqlInsertMultiRow,
    SqlUpdate,
    /// A `WHERE` that finds the rows again.
    Where,
    /// The values on one line, written as SQL: for `IN (...)`.
    OneRow,
    Json,
    Xml,
    Html,
    Markdown,
    /// A table drawn with `+`, `-` and `|`.
    Pretty,
}

impl Format {
    pub const ALL: [Format; 14] = [
        Format::Tsv,
        Format::Csv,
        Format::Semicolon,
        Format::Pipe,
        Format::SqlInsert,
        Format::SqlInsertMultiRow,
        Format::SqlUpdate,
        Format::Where,
        Format::OneRow,
        Format::Json,
        Format::Xml,
        Format::Html,
        Format::Markdown,
        Format::Pretty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Format::Tsv => "TSV (tabs)",
            Format::Csv => "CSV (commas)",
            Format::Semicolon => "Semicolon-separated",
            Format::Pipe => "Pipe-separated",
            Format::SqlInsert => "SQL INSERT",
            Format::SqlInsertMultiRow => "SQL INSERT, many rows",
            Format::SqlUpdate => "SQL UPDATE",
            Format::Where => "WHERE clause",
            Format::OneRow => "One line, for IN (…)",
            Format::Json => "JSON",
            Format::Xml => "XML",
            Format::Html => "HTML table",
            Format::Markdown => "Markdown table",
            Format::Pretty => "Text table",
        }
    }

    /// What the format is for, in one line.
    pub fn about(self) -> &'static str {
        match self {
            Format::Tsv => "Pastes straight into Excel and other spreadsheets.",
            Format::Csv => "The usual file for spreadsheets and imports.",
            Format::Semicolon => "CSV for Excel where the comma is the decimal sign.",
            Format::Pipe => "Values with | between them.",
            Format::SqlInsert => "One INSERT statement per row.",
            Format::SqlInsertMultiRow => "INSERT statements that each carry many rows.",
            Format::SqlUpdate => "One UPDATE per row, found by its key columns.",
            Format::Where => "A WHERE that finds these rows again.",
            Format::OneRow => "All values on one line, written as SQL values.",
            Format::Json => "A list of objects, one per row.",
            Format::Xml => "One <row> per row.",
            Format::Html => "A page with one table.",
            Format::Markdown => "A table for notes, issues and chat.",
            Format::Pretty => "A table drawn with lines, for plain text.",
        }
    }

    /// The name the format is saved under in the settings.
    pub fn id(self) -> &'static str {
        match self {
            Format::Tsv => "tsv",
            Format::Csv => "csv",
            Format::Semicolon => "semicolon",
            Format::Pipe => "pipe",
            Format::SqlInsert => "sql-insert",
            Format::SqlInsertMultiRow => "sql-insert-multi",
            Format::SqlUpdate => "sql-update",
            Format::Where => "where",
            Format::OneRow => "one-row",
            Format::Json => "json",
            Format::Xml => "xml",
            Format::Html => "html",
            Format::Markdown => "markdown",
            Format::Pretty => "pretty",
        }
    }

    pub fn from_id(id: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|format| format.id() == id)
    }

    /// The ending of a file in this format, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Tsv => "tsv",
            Format::Csv | Format::Semicolon => "csv",
            Format::Pipe | Format::OneRow | Format::Pretty => "txt",
            Format::SqlInsert | Format::SqlInsertMultiRow | Format::SqlUpdate | Format::Where => {
                "sql"
            }
            Format::Json => "json",
            Format::Xml => "xml",
            Format::Html => "html",
            Format::Markdown => "md",
        }
    }

    pub fn group(self) -> Group {
        match self {
            Format::Tsv | Format::Csv | Format::Semicolon | Format::Pipe => Group::Delimited,
            Format::SqlInsert
            | Format::SqlInsertMultiRow
            | Format::SqlUpdate
            | Format::Where
            | Format::OneRow => Group::Sql,
            Format::Json | Format::Xml | Format::Html => Group::Data,
            Format::Markdown | Format::Pretty => Group::Text,
        }
    }

    /// The character between the values of a separated format.
    pub fn delimiter(self) -> Option<char> {
        match self {
            Format::Tsv => Some('\t'),
            Format::Csv => Some(','),
            Format::Semicolon => Some(';'),
            Format::Pipe => Some('|'),
            _ => None,
        }
    }

    /// Whether the format names a table.
    pub fn uses_table(self) -> bool {
        matches!(
            self,
            Format::SqlInsert | Format::SqlInsertMultiRow | Format::SqlUpdate
        )
    }

    /// Whether the format finds rows by key columns.
    pub fn uses_keys(self) -> bool {
        self == Format::SqlUpdate
    }

    /// Whether a first line with the column names can be left out.
    pub fn optional_header(self) -> bool {
        self.group() == Group::Delimited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_is_listed_once_and_found_by_its_id() {
        for (index, format) in Format::ALL.into_iter().enumerate() {
            assert_eq!(Format::from_id(format.id()), Some(format));
            assert_eq!(
                Format::ALL.iter().position(|other| *other == format),
                Some(index)
            );
            assert!(!format.extension().is_empty());
        }
        assert_eq!(Format::from_id("nope"), None);
    }
}
