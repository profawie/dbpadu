//! The result grid: rows and columns, a selection made with the mouse or
//! the keyboard, and copying it.
//!
//! Click a cell to select it; drag or Shift+click for a block of cells.
//! A row number selects the row, a column name the column, `#` everything.
//! Ctrl+C copies the selection in the format chosen above the grid.

use std::time::Instant;

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use dbpadu_engine::protocol::QueryResult;
use dbpadu_export::{Format, Group, Options};

use crate::{app::QueryTab, theme::Theme};

const ROW_HEIGHT: f32 = 24.0;
/// Characters of a cell shown when the pointer rests on it.
const HOVER_CHARS: usize = 2_000;
/// How long "Copied …" stays above the grid.
const COPIED_SECONDS: f32 = 2.5;

/// A block of cells, from the cell it was started at to the cell it was
/// last stretched to. Each is (row, column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

/// The rows and columns of a selection, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub rows: (usize, usize),
    pub columns: (usize, usize),
}

impl Block {
    pub fn row_count(self) -> usize {
        self.rows.1 - self.rows.0 + 1
    }

    pub fn column_count(self) -> usize {
        self.columns.1 - self.columns.0 + 1
    }

    pub fn is_one_cell(self) -> bool {
        self.rows.0 == self.rows.1 && self.columns.0 == self.columns.1
    }

    fn contains(self, row: usize, column: usize) -> bool {
        (self.rows.0..=self.rows.1).contains(&row)
            && (self.columns.0..=self.columns.1).contains(&column)
    }

    /// "3 rows × 2 columns", "1 cell".
    pub fn describe(self) -> String {
        if self.is_one_cell() {
            "1 cell".to_string()
        } else {
            format!(
                "{} × {}",
                super::count(self.row_count(), "row", "rows"),
                super::count(self.column_count(), "column", "columns")
            )
        }
    }
}

impl Selection {
    #[cfg(test)]
    fn cell(row: usize, column: usize) -> Self {
        Self {
            anchor: (row, column),
            head: (row, column),
        }
    }

    /// The block, kept inside a result of `rows` × `columns`. `None` when
    /// the result is empty.
    pub fn block(self, rows: usize, columns: usize) -> Option<Block> {
        if rows == 0 || columns == 0 {
            return None;
        }
        let (a, b) = (self.anchor, self.head);
        Some(Block {
            rows: (a.0.min(b.0).min(rows - 1), a.0.max(b.0).min(rows - 1)),
            columns: (a.1.min(b.1).min(columns - 1), a.1.max(b.1).min(columns - 1)),
        })
    }
}

/// What the grid of a tab remembers between frames.
#[derive(Default)]
pub struct GridState {
    pub selection: Option<Selection>,
    /// The mouse button is held since it went down on a cell.
    dragging: bool,
    /// A row the keyboard moved to, to be scrolled into view.
    scroll_to: Option<usize>,
    /// What was copied last, and when.
    copied: Option<(String, Instant)>,
}

impl GridState {
    /// "Copied 3 rows × 2 columns", for a moment after copying.
    pub fn copied(&self) -> Option<&str> {
        self.copied
            .as_ref()
            .filter(|(_, at)| at.elapsed().as_secs_f32() < COPIED_SECONDS)
            .map(|(text, _)| text.as_str())
    }
}

/// What the user asked for in the grid that the grid cannot do itself.
pub enum Asked {
    /// Open the Export box.
    Export,
}

/// What to copy.
#[derive(Clone, Copy)]
enum Copy {
    /// The selection, in the format given.
    Selection { format: Format, header: bool },
    /// The whole rows the selection touches.
    Rows,
    /// The value of the one cell, as it is.
    Value,
    /// The name of the column.
    ColumnName(usize),
}

/// The columns of a result that a block is written with, by position: the
/// columns of the block and, for a format that finds rows by their key, the
/// key columns too, so an UPDATE of a few selected cells still says which
/// rows it is for.
pub fn column_positions(
    result: &QueryResult,
    block: Block,
    format: Format,
    options: &Options,
) -> Vec<usize> {
    let selected: Vec<usize> = (block.columns.0..=block.columns.1).collect();
    if !format.uses_keys() {
        return selected;
    }
    let position = |name: &str| result.columns.iter().position(|column| column.name == name);
    let mut positions: Vec<usize> = options
        .keys
        .iter()
        .filter_map(|key| position(key))
        .collect();
    if positions.is_empty() {
        positions.extend(
            result
                .columns
                .iter()
                .position(|column| column.name.eq_ignore_ascii_case("id")),
        );
    }
    positions.retain(|position| !selected.contains(position));
    positions.extend(selected);
    positions
}

/// Columns of a result, as the export library takes them.
pub fn columns(result: &QueryResult, positions: &[usize]) -> Vec<dbpadu_export::Column> {
    positions
        .iter()
        .map(|&position| {
            let column = &result.columns[position];
            dbpadu_export::Column::new(&column.name, &column.type_name)
        })
        .collect()
}

/// The rows of a block of a result with the columns at `positions`, as the
/// export library takes them.
pub fn rows<'a>(
    result: &'a QueryResult,
    block: Block,
    positions: &'a [usize],
) -> impl Iterator<Item = Vec<Option<&'a str>>> {
    result.rows[block.rows.0..=block.rows.1]
        .iter()
        .map(move |row| {
            positions
                .iter()
                .map(|&position| row.get(position).and_then(Option::as_deref))
                .collect()
        })
}

/// A block of a result as text.
pub fn text(result: &QueryResult, block: Block, format: Format, options: &Options) -> String {
    // One value pastes best as it is, without quotes around it.
    if block.is_one_cell() && format.delimiter().is_some() && !options.header {
        return result.rows[block.rows.0]
            .get(block.columns.0)
            .and_then(Option::as_deref)
            .unwrap_or("")
            .to_string();
    }
    let positions = column_positions(result, block, format, options);
    dbpadu_export::to_string(
        format,
        &columns(result, &positions),
        rows(result, block, &positions),
        options,
    )
}

/// The grid of a tab's result. `format` is what Ctrl+C copies in.
pub fn show(
    ui: &mut egui::Ui,
    theme: &Theme,
    tab: &mut QueryTab,
    result: &QueryResult,
    format: Format,
) -> Option<Asked> {
    let (row_count, column_count) = (result.rows.len(), result.columns.len());
    if column_count == 0 {
        return None;
    }
    let last = (row_count.saturating_sub(1), column_count - 1);
    let tab_id = tab.id;
    let runs = tab.runs;
    let grid = &mut tab.grid;
    let mut selection = grid.selection;
    let mut copy: Option<Copy> = None;
    let mut asked = None;

    let (pressed, down, shift) = ui.input(|input| {
        (
            input.pointer.primary_pressed(),
            input.pointer.primary_down(),
            input.modifiers.shift,
        )
    });
    if !down {
        grid.dragging = false;
    }
    // While the button is held the cell it went down on keeps the pointer,
    // so the cell under it is found by where it is.
    let pointer = ui.input(|input| input.pointer.latest_pos());

    // The keyboard works on the grid while nothing else has it: the SQL
    // editor keeps its own Ctrl+C and Ctrl+A.
    let free =
        ui.memory(|memory| memory.focused().is_none()) && !egui::Popup::is_any_open(ui.ctx());
    let mut scroll_to = grid.scroll_to.take();
    if free && row_count > 0 {
        let command = egui::Modifiers::COMMAND;
        ui.input_mut(|input| {
            if input.consume_key(command, egui::Key::A) {
                selection = Some(Selection {
                    anchor: (0, 0),
                    head: last,
                });
            }
            let Some(current) = selection else {
                return;
            };
            if input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::C) {
                copy = Some(Copy::Selection {
                    format,
                    header: true,
                });
            } else if input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Copy))
            {
                copy = Some(Copy::Selection {
                    format,
                    header: false,
                });
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                selection = None;
                return;
            }
            let moves = [
                (egui::Key::ArrowUp, -1, 0),
                (egui::Key::ArrowDown, 1, 0),
                (egui::Key::ArrowLeft, 0, -1),
                (egui::Key::ArrowRight, 0, 1),
                (egui::Key::PageUp, -20, 0),
                (egui::Key::PageDown, 20, 0),
            ];
            for (key, rows, columns) in moves {
                let stretch = input.consume_key(egui::Modifiers::SHIFT, key);
                if !stretch && !input.consume_key(egui::Modifiers::NONE, key) {
                    continue;
                }
                let head = (
                    current.head.0.saturating_add_signed(rows).min(last.0),
                    current.head.1.saturating_add_signed(columns).min(last.1),
                );
                selection = Some(Selection {
                    anchor: if stretch { current.anchor } else { head },
                    head,
                });
                scroll_to = Some(head.0);
            }
        });
    }
    let block = selection.and_then(|selection| selection.block(row_count, column_count));
    let fill = theme.accent.gamma_multiply(0.30);

    // A press on the grid takes the keyboard away from the SQL editor.
    let mut touched = false;
    // What a press or a drag does to the selection, for a cell, a row
    // number (`column` is None) or a column name (`row` is None).
    let mut point = |selection: &mut Option<Selection>,
                     dragging: &mut bool,
                     response: &egui::Response,
                     inside: bool,
                     row: Option<usize>,
                     column: Option<usize>| {
        let from = (row.unwrap_or(0), column.unwrap_or(0));
        let to = (row.unwrap_or(last.0), column.unwrap_or(last.1));
        if pressed && inside && response.contains_pointer() {
            touched = true;
            *dragging = true;
            *selection = Some(match selection.filter(|_| shift) {
                Some(current) => Selection {
                    // The far corner of a row or a column, seen from where
                    // the selection was started.
                    anchor: (
                        if row.is_none() { 0 } else { current.anchor.0 },
                        if column.is_none() {
                            0
                        } else {
                            current.anchor.1
                        },
                    ),
                    head: to,
                },
                None => Selection {
                    anchor: from,
                    head: to,
                },
            });
        } else if *dragging
            && down
            && inside
            && let Some(current) = selection
        {
            current.head = to;
        }
        if response.secondary_clicked() {
            touched = true;
            let inside_block = selection
                .and_then(|selection| selection.block(row_count, column_count))
                .is_some_and(|block| {
                    block.contains(
                        row.unwrap_or(block.rows.0),
                        column.unwrap_or(block.columns.0),
                    ) && (row.is_some() || block.row_count() == row_count)
                        && (column.is_some() || block.column_count() == column_count)
                });
            // A right-click outside the selection moves it there first.
            if !inside_block {
                *selection = Some(Selection {
                    anchor: from,
                    head: to,
                });
            }
        }
    };

    egui::ScrollArea::horizontal()
        .id_salt(("grid_scroll", tab_id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Text in the cells is not picked letter by letter: a click
            // selects the cell.
            ui.style_mut().interaction.selectable_labels = false;
            let mut table = TableBuilder::new(ui)
                // Each run starts with fresh column widths.
                .id_salt(("grid", tab_id, runs))
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click_and_drag())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto().at_least(36.0).resizable(false))
                .columns(
                    Column::initial(150.0).at_least(40.0).clip(true),
                    column_count,
                );
            if let Some(row) = scroll_to {
                table = table.scroll_to_row(row, None);
            }
            table
                .header(ROW_HEIGHT + 4.0, |mut header| {
                    let (_, response) = header.col(|ui| {
                        ui.label(RichText::new("#").small().color(theme.text_dim));
                    });
                    let inside = pointer.is_some_and(|pointer| response.rect.contains(pointer));
                    point(
                        &mut selection,
                        &mut grid.dragging,
                        &response,
                        inside,
                        None,
                        None,
                    );
                    response.on_hover_text("Select everything (Ctrl+A)");
                    for (index, column) in result.columns.iter().enumerate() {
                        let selected = block.is_some_and(|block| {
                            block.contains(block.rows.0, index) && block.row_count() == row_count
                        });
                        let (_, response) = header.col(|ui| {
                            let name = RichText::new(&column.name).strong();
                            ui.add(
                                egui::Label::new(if selected {
                                    name.color(theme.accent_text)
                                } else {
                                    name
                                })
                                .truncate(),
                            );
                        });
                        let inside = pointer.is_some_and(|pointer| response.rect.contains(pointer));
                        point(
                            &mut selection,
                            &mut grid.dragging,
                            &response,
                            inside,
                            None,
                            Some(index),
                        );
                        response.context_menu(|ui| {
                            menu(
                                ui,
                                result,
                                selection,
                                format,
                                Some(index),
                                &mut copy,
                                &mut asked,
                            );
                        });
                        response.on_hover_text(format!(
                            "{} · {}\nClick to select the column",
                            column.name,
                            column.type_name.to_lowercase()
                        ));
                    }
                })
                .body(|body| {
                    let viewport = body.max_rect();
                    body.rows(ROW_HEIGHT, row_count, |mut row| {
                        let index = row.index();
                        let in_rows = block
                            .is_some_and(|block| (block.rows.0..=block.rows.1).contains(&index));
                        let (_, response) = row.col(|ui| {
                            ui.label(RichText::new((index + 1).to_string()).small().color(
                                if in_rows {
                                    theme.accent_text
                                } else {
                                    theme.text_dim
                                },
                            ));
                        });
                        let inside = pointer.is_some_and(|pointer| response.rect.contains(pointer));
                        point(
                            &mut selection,
                            &mut grid.dragging,
                            &response,
                            inside,
                            Some(index),
                            None,
                        );
                        response.context_menu(|ui| {
                            menu(ui, result, selection, format, None, &mut copy, &mut asked);
                        });
                        for (column, value) in result.rows[index].iter().enumerate() {
                            let selected = block.is_some_and(|block| block.contains(index, column));
                            let value = value.as_deref();
                            let mut cut = false;
                            let (_, response) = row.col(|ui| {
                                if selected {
                                    // Reaches back over the gap to the
                                    // selected cell above and to the left,
                                    // so a block is one piece. Never over
                                    // the column names.
                                    let gap = ui.spacing().item_spacing;
                                    let mut rect = ui.max_rect();
                                    if block.is_some_and(|block| index > block.rows.0) {
                                        rect.min.y -= gap.y;
                                    }
                                    if block.is_some_and(|block| column > block.columns.0) {
                                        rect.min.x -= gap.x;
                                    }
                                    let clip = egui::Rect::from_x_y_ranges(
                                        rect.x_range(),
                                        rect.y_range().intersection(viewport.y_range()),
                                    );
                                    ui.ctx()
                                        .layer_painter(ui.layer_id())
                                        .with_clip_rect(clip)
                                        .rect_filled(rect, 0.0, fill);
                                }
                                cut = cell(ui, theme, value);
                            });
                            let inside =
                                pointer.is_some_and(|pointer| response.rect.contains(pointer));
                            point(
                                &mut selection,
                                &mut grid.dragging,
                                &response,
                                inside,
                                Some(index),
                                Some(column),
                            );
                            if response.double_clicked() {
                                copy = Some(Copy::Value);
                            }
                            response.context_menu(|ui| {
                                menu(
                                    ui,
                                    result,
                                    selection,
                                    format,
                                    Some(column),
                                    &mut copy,
                                    &mut asked,
                                );
                            });
                            if let Some(value) = value.filter(|value| cut || value.len() > 24)
                                && !grid.dragging
                            {
                                response.on_hover_ui(|ui| {
                                    let shown: String = value.chars().take(HOVER_CHARS).collect();
                                    ui.add(egui::Label::new(shown).wrap());
                                });
                            }
                        }
                    });
                });
        });

    if touched {
        ui.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
    }
    grid.selection = selection;
    grid.scroll_to = None;

    let block = selection.and_then(|selection| selection.block(row_count, column_count));
    if let (Some(copy), Some(block)) = (copy, block) {
        let (text, what) = match copy {
            Copy::Selection { format, header } => (
                text(
                    result,
                    block,
                    format,
                    &Options {
                        header,
                        ..tab.extract.clone()
                    },
                ),
                block.describe(),
            ),
            Copy::Rows => {
                let block = Block {
                    columns: (0, last.1),
                    ..block
                };
                (
                    text(
                        result,
                        block,
                        format,
                        &Options {
                            header: false,
                            ..tab.extract.clone()
                        },
                    ),
                    super::count(block.row_count(), "row", "rows"),
                )
            }
            Copy::Value => (
                result.rows[block.rows.0]
                    .get(block.columns.0)
                    .and_then(Option::as_deref)
                    .unwrap_or("")
                    .to_string(),
                "1 cell".to_string(),
            ),
            Copy::ColumnName(column) => (
                result.columns[column].name.clone(),
                "the column name".to_string(),
            ),
        };
        ui.ctx().copy_text(text);
        tab.grid.copied = Some((format!("Copied {what}"), Instant::now()));
        // So the note above the grid goes away by itself.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f32(COPIED_SECONDS + 0.1));
    }
    asked
}

/// One cell: the first line of the value. Returns whether lines were left out.
fn cell(ui: &mut egui::Ui, theme: &Theme, value: Option<&str>) -> bool {
    let Some(value) = value else {
        ui.label(
            RichText::new("NULL")
                .italics()
                .color(theme.text_dim.gamma_multiply(0.8)),
        );
        return false;
    };
    // One line in the grid; the rest shows when the pointer rests on it.
    let first_line = value.lines().next().unwrap_or("");
    let cut = first_line.len() < value.len();
    if cut {
        ui.add(egui::Label::new(format!("{first_line} …")).truncate());
    } else {
        ui.add(egui::Label::new(first_line).truncate());
    }
    cut
}

/// The menu of a right-click. `column` is the column that was clicked, if
/// the click was not on a row number.
fn menu(
    ui: &mut egui::Ui,
    result: &QueryResult,
    selection: Option<Selection>,
    format: Format,
    column: Option<usize>,
    copy: &mut Option<Copy>,
    asked: &mut Option<Asked>,
) {
    let Some(block) =
        selection.and_then(|selection| selection.block(result.rows.len(), result.columns.len()))
    else {
        ui.close();
        return;
    };
    ui.set_min_width(220.0);
    let shortcut =
        |text: &str, keys: &str| egui::Button::new(text.to_owned()).shortcut_text(keys.to_owned());
    let what = if block.is_one_cell() {
        "Copy cell".to_string()
    } else {
        format!("Copy {}", block.describe())
    };
    if ui.add(shortcut(&what, "Ctrl+C")).clicked() {
        *copy = Some(Copy::Selection {
            format,
            header: false,
        });
        ui.close();
    }
    if ui
        .add(shortcut("Copy with column names", "Ctrl+Shift+C"))
        .clicked()
    {
        *copy = Some(Copy::Selection {
            format,
            header: true,
        });
        ui.close();
    }
    let whole_rows = block.column_count() == result.columns.len();
    if !whole_rows {
        let rows = if block.row_count() == 1 {
            "Copy the whole row".to_string()
        } else {
            format!("Copy the {} whole rows", block.row_count())
        };
        if ui.button(rows).clicked() {
            *copy = Some(Copy::Rows);
            ui.close();
        }
    }
    ui.menu_button("Copy as", |ui| {
        ui.set_min_width(200.0);
        for group in Group::ALL {
            if group != Group::Delimited {
                ui.separator();
            }
            for format in Format::ALL
                .into_iter()
                .filter(|format| format.group() == group)
            {
                if ui
                    .button(format.label())
                    .on_hover_text(format.about())
                    .clicked()
                {
                    *copy = Some(Copy::Selection {
                        format,
                        header: true,
                    });
                    ui.close();
                }
            }
        }
    });
    if let Some(column) = column
        && ui.button("Copy column name").clicked()
    {
        *copy = Some(Copy::ColumnName(column));
        ui.close();
    }
    ui.separator();
    if ui.button("Export…").clicked() {
        *asked = Some(Asked::Export);
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbpadu_engine::protocol::ResultColumn;

    fn result() -> QueryResult {
        let column = |name: &str, type_name: &str| ResultColumn {
            name: name.to_string(),
            type_name: type_name.to_string(),
        };
        let row = |cells: [Option<&str>; 3]| cells.map(|cell| cell.map(str::to_string)).to_vec();
        QueryResult {
            kind: "rows".to_string(),
            columns: vec![
                column("id", "int4"),
                column("name", "text"),
                column("note", "text"),
            ],
            rows: vec![
                row([Some("1"), Some("Ali"), Some("a, b")]),
                row([Some("2"), Some("Siti"), None]),
                row([Some("3"), Some("Tan"), Some("two\nlines")]),
            ],
            ..QueryResult::default()
        }
    }

    fn block(selection: Selection) -> Block {
        selection.block(3, 3).unwrap()
    }

    #[test]
    fn a_selection_is_a_block_whichever_way_it_was_dragged() {
        let dragged_up = Selection {
            anchor: (2, 2),
            head: (1, 0),
        };
        assert_eq!(
            block(dragged_up),
            Block {
                rows: (1, 2),
                columns: (0, 2)
            }
        );
        assert_eq!(block(dragged_up).describe(), "2 rows × 3 columns");
        assert_eq!(block(Selection::cell(0, 1)).describe(), "1 cell");
        // A selection left from a larger result stays inside this one.
        assert_eq!(
            block(Selection::cell(9, 9)),
            Block {
                rows: (2, 2),
                columns: (2, 2)
            }
        );
        assert_eq!(Selection::cell(0, 0).block(0, 3), None);
    }

    #[test]
    fn one_cell_is_copied_as_it_is() {
        let plain = Options {
            header: false,
            ..Options::default()
        };
        let result = result();
        assert_eq!(
            text(&result, block(Selection::cell(0, 2)), Format::Csv, &plain),
            "a, b"
        );
        assert_eq!(
            text(&result, block(Selection::cell(2, 2)), Format::Tsv, &plain),
            "two\nlines"
        );
        assert_eq!(
            text(&result, block(Selection::cell(1, 2)), Format::Tsv, &plain),
            ""
        );
        // In another format the cell is written in that format.
        assert_eq!(
            text(&result, block(Selection::cell(0, 1)), Format::Where, &plain),
            "WHERE name IN ('Ali')"
        );
    }

    #[test]
    fn a_block_is_copied_for_a_spreadsheet() {
        let result = result();
        let selection = Selection {
            anchor: (0, 0),
            head: (1, 1),
        };
        assert_eq!(
            text(
                &result,
                block(selection),
                Format::Tsv,
                &Options {
                    header: false,
                    ..Options::default()
                }
            ),
            "1\tAli\n2\tSiti"
        );
        assert_eq!(
            text(&result, block(selection), Format::Tsv, &Options::default()),
            "id\tname\n1\tAli\n2\tSiti"
        );
        let rows = Selection {
            anchor: (1, 0),
            head: (1, 2),
        };
        assert_eq!(
            text(
                &result,
                block(rows),
                Format::SqlInsert,
                &Options {
                    table: "people".to_string(),
                    ..Options::default()
                }
            ),
            "INSERT INTO people (id, name, note) VALUES (2, 'Siti', NULL);"
        );
    }

    #[test]
    fn an_update_of_selected_cells_still_names_its_rows() {
        let result = result();
        let names = Selection {
            anchor: (0, 1),
            head: (1, 1),
        };
        let people = Options {
            table: "people".to_string(),
            ..Options::default()
        };
        assert_eq!(
            text(&result, block(names), Format::SqlUpdate, &people),
            "UPDATE people SET name = 'Ali' WHERE id = 1;\n\
             UPDATE people SET name = 'Siti' WHERE id = 2;"
        );
        let by_name = Options {
            keys: vec!["name".to_string()],
            ..people
        };
        assert_eq!(
            text(
                &result,
                block(Selection::cell(2, 0)),
                Format::SqlUpdate,
                &by_name
            ),
            "UPDATE people SET id = 3 WHERE name = 'Tan';"
        );
        // Other formats write the selected columns only.
        assert_eq!(
            text(&result, block(names), Format::OneRow, &by_name),
            "'Ali', 'Siti'"
        );
    }
}
