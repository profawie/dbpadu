//! The file the engine writes for an export of every row, and turning it
//! into the format that was asked for.
//!
//! The file has one JSON value per line: first `{"columns": [...]}`, then
//! one list of values per row, where `null` is a NULL.

use std::{
    fs::File,
    io::{self, BufRead, BufReader, BufWriter},
    path::Path,
};

use serde::Deserialize;

use crate::{
    format::Format,
    writer::{Column, Options, Writer},
};

#[derive(Deserialize)]
struct Head {
    columns: Vec<Column>,
}

/// Reads the engine's rows file, one row at a time.
pub struct RowsFile {
    columns: Vec<Column>,
    lines: io::Lines<BufReader<File>>,
}

fn unreadable(err: serde_json::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("the rows the engine wrote could not be read: {err}"),
    )
}

impl RowsFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut lines = BufReader::new(File::open(path)?).lines();
        let first = lines.next().transpose()?.unwrap_or_default();
        let head: Head = serde_json::from_str(&first).map_err(unreadable)?;
        Ok(Self {
            columns: head.columns,
            lines,
        })
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// The next row, or `None` after the last.
    pub fn next_row(&mut self) -> io::Result<Option<Vec<Option<String>>>> {
        for line in self.lines.by_ref() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            return serde_json::from_str(&line).map(Some).map_err(unreadable);
        }
        Ok(None)
    }
}

/// Write the rows file `rows` to `target` in a format. Returns how many
/// rows were written.
pub fn convert(rows: &Path, target: &Path, format: Format, options: &Options) -> io::Result<u64> {
    let mut source = RowsFile::open(rows)?;
    let out = BufWriter::new(File::create(target)?);
    let mut writer = Writer::new(out, format, source.columns(), options)?;
    while let Some(row) = source.next_row()? {
        let cells: Vec<Option<&str>> = row.iter().map(Option::as_deref).collect();
        writer.row(&cells)?;
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rows_file_becomes_the_format_asked_for() {
        let dir = std::env::temp_dir();
        let rows = dir.join(format!("dbpadu-export-test-{}.jsonl", std::process::id()));
        let target = dir.join(format!("dbpadu-export-test-{}.sql", std::process::id()));
        std::fs::write(
            &rows,
            "{\"columns\":[{\"name\":\"id\",\"type\":\"INTEGER\"},{\"name\":\"note\",\"type\":\"TEXT\"}]}\n\
             [\"1\",\"two\\nlines\"]\n\
             [\"2\",null]\n",
        )
        .unwrap();
        let options = Options {
            table: "notes".to_string(),
            ..Options::default()
        };
        let written = convert(&rows, &target, Format::SqlInsert, &options).unwrap();
        assert_eq!(written, 2);
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "INSERT INTO notes (id, note) VALUES (1, 'two\nlines');\n\
             INSERT INTO notes (id, note) VALUES (2, NULL);\n"
        );
        let _ = std::fs::remove_file(&rows);
        let _ = std::fs::remove_file(&target);
    }

    #[test]
    fn a_broken_rows_file_is_reported() {
        let rows =
            std::env::temp_dir().join(format!("dbpadu-export-broken-{}.jsonl", std::process::id()));
        std::fs::write(&rows, "not json\n").unwrap();
        assert!(RowsFile::open(&rows).is_err());
        let _ = std::fs::remove_file(&rows);
    }
}
