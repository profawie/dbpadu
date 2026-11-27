//! The Export box: a result, or part of it, written to a file or copied
//! in one of the formats of the export library.

use std::{fs::File, io::BufWriter, path::Path};

use eframe::egui::{self, RichText};

use dbpadu_engine::protocol::QueryResult;
use dbpadu_export::{Format, Group, Options, Writer};

use super::{
    editor::dialog_frame,
    grid::{self, Block},
};
use crate::app::{App, Link, Outcome, QueryTab};

/// Rows of the result shown as an example.
const PREVIEW_ROWS: usize = 5;

/// Which rows are exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The selected cells.
    Selection,
    /// The rows in the grid.
    Shown,
    /// Every row the SQL gives: it runs again, without the row limit.
    All,
}

pub struct ExportDialog {
    pub tab: u64,
    pub format: Format,
    pub scope: Scope,
    /// The table the SQL formats name.
    pub table: String,
    /// The columns an UPDATE finds its row by.
    pub keys: Vec<String>,
    /// Whether the separated formats start with the column names.
    pub header: bool,
}

/// The format saved in the settings under a name, or the first one.
pub fn saved_format(id: &str) -> Format {
    Format::from_id(id).unwrap_or(Format::ALL[0])
}

/// The selection of a tab's grid, when it is more than one cell.
fn selected(tab: &QueryTab, result: &QueryResult) -> Option<Block> {
    tab.grid
        .selection
        .and_then(|selection| selection.block(result.rows.len(), result.columns.len()))
        .filter(|block| !block.is_one_cell())
}

fn everything(result: &QueryResult) -> Option<Block> {
    (!result.rows.is_empty() && !result.columns.is_empty()).then(|| Block {
        rows: (0, result.rows.len() - 1),
        columns: (0, result.columns.len() - 1),
    })
}

/// Open the Export box on a tab's result.
pub fn open(app: &mut App, tab_id: u64) {
    let Some(tab) = app.tabs.iter().find(|tab| tab.id == tab_id) else {
        return;
    };
    let Some(Outcome::Result(result)) = &tab.outcome else {
        return;
    };
    let mut format = saved_format(&app.data.settings.export_format);
    if app.data.settings.export_format.is_empty() {
        format = Format::Csv;
    }
    app.export = Some(ExportDialog {
        tab: tab_id,
        format,
        scope: if selected(tab, result).is_some() {
            Scope::Selection
        } else {
            Scope::Shown
        },
        table: tab.extract.table.clone(),
        keys: tab.extract.keys.clone(),
        header: true,
    });
}

/// A menu of the formats, in their groups.
pub fn format_combo(ui: &mut egui::Ui, id: impl egui::AsIdSalt, format: &mut Format, width: f32) {
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .height(420.0)
        .selected_text(format.label())
        .show_ui(ui, |ui| {
            for group in Group::ALL {
                ui.label(RichText::new(group.label()).small().weak());
                for choice in Format::ALL
                    .into_iter()
                    .filter(|choice| choice.group() == group)
                {
                    ui.selectable_value(format, choice, choice.label())
                        .on_hover_text(choice.about());
                }
            }
        });
}

/// Write a block of a result to a file. Returns the rows written.
fn write(
    path: &Path,
    result: &QueryResult,
    block: Block,
    format: Format,
    options: &Options,
) -> std::io::Result<u64> {
    let positions = grid::column_positions(result, block, format, options);
    let out = BufWriter::new(File::create(path)?);
    let mut writer = Writer::new(out, format, &grid::columns(result, &positions), options)?;
    for row in grid::rows(result, block, &positions) {
        writer.row(&row)?;
    }
    writer.finish()
}

enum Done {
    Keep,
    Close,
    Save,
    Copy,
}

pub fn dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut dialog) = app.export.take() else {
        return;
    };
    let theme = app.theme;
    let Some(tab) = app.tabs.iter().find(|tab| tab.id == dialog.tab) else {
        return;
    };
    let Some(Outcome::Result(result)) = &tab.outcome else {
        return;
    };
    let Some(shown) = everything(result) else {
        return;
    };
    let selection = selected(tab, result);
    if dialog.scope == Scope::Selection && selection.is_none() {
        dialog.scope = Scope::Shown;
    }
    let connected = app.links.get(&tab.connection).is_some_and(Link::connected);
    let busy = tab.running.is_some();
    let options = |dialog: &ExportDialog| Options {
        table: dialog.table.clone(),
        keys: dialog.keys.clone(),
        header: dialog.header,
        quote: tab.extract.quote,
    };
    let block = |dialog: &ExportDialog| match (dialog.scope, selection) {
        (Scope::Selection, Some(block)) => block,
        _ => shown,
    };

    let mut done = Done::Keep;
    let modal = egui::Modal::new(egui::Id::unique("export_dialog"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new("Export result").size(18.0).strong());
            ui.add_space(4.0);

            super::field_label(ui, &theme, "Format");
            ui.horizontal(|ui| {
                format_combo(ui, "export_format", &mut dialog.format, 220.0);
                ui.label(RichText::new(dialog.format.about()).color(theme.text_dim));
            });

            ui.add_space(6.0);
            super::field_label(ui, &theme, "Rows");
            if let Some(block) = selection {
                ui.radio_value(
                    &mut dialog.scope,
                    Scope::Selection,
                    format!("The selection: {}", block.describe()),
                );
            }
            ui.radio_value(
                &mut dialog.scope,
                Scope::Shown,
                format!(
                    "The {} in the grid",
                    super::count(result.rows.len(), "row", "rows")
                ),
            );
            if result.limit_reached {
                ui.add_enabled_ui(connected && !busy, |ui| {
                    ui.radio_value(
                        &mut dialog.scope,
                        Scope::All,
                        "All rows: run the query again without the row limit",
                    )
                    .on_disabled_hover_text(if busy {
                        "Wait for what is running to finish."
                    } else {
                        "Connect first."
                    });
                });
                let about = if dialog.scope == Scope::All {
                    "The rows go straight from the database to the file, so they never fill \
                     the grid or the memory. A large table takes a while; Stop ends it."
                } else {
                    "The grid stopped at the row limit, so the database may have more rows \
                     than it shows."
                };
                ui.label(RichText::new(about).small().color(theme.text_dim));
            }

            if dialog.format.uses_table() {
                ui.add_space(6.0);
                super::field(
                    ui,
                    &theme,
                    "Table name in the SQL",
                    &mut dialog.table,
                    "my_table",
                );
            }
            if dialog.format.uses_keys() {
                ui.add_space(6.0);
                super::field_label(ui, &theme, "Key columns: how each UPDATE finds its row");
                egui::ScrollArea::vertical()
                    .id_salt("export_keys")
                    .max_height(84.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for column in &result.columns {
                                let mut key = dialog.keys.contains(&column.name);
                                if ui.checkbox(&mut key, &column.name).changed() {
                                    if key {
                                        dialog.keys.push(column.name.clone());
                                    } else {
                                        dialog.keys.retain(|name| name != &column.name);
                                    }
                                }
                            }
                        });
                    });
                if dialog.keys.is_empty() {
                    ui.label(
                        RichText::new(
                            "None ticked: the column called id is used, or every column \
                             when there is none. Key columns are written even when they \
                             are not selected.",
                        )
                        .small()
                        .color(theme.text_dim),
                    );
                }
            }
            if dialog.format.optional_header() {
                ui.add_space(6.0);
                ui.checkbox(&mut dialog.header, "First line holds the column names");
            }

            ui.add_space(6.0);
            let block = block(&dialog);
            let preview = Block {
                rows: (
                    block.rows.0,
                    block.rows.1.min(block.rows.0 + PREVIEW_ROWS - 1),
                ),
                ..block
            };
            super::field_label(
                ui,
                &theme,
                &format!(
                    "Preview: the first {}",
                    super::count(preview.row_count(), "row", "rows")
                ),
            );
            let mut text = grid::text(result, preview, dialog.format, &options(&dialog));
            egui::ScrollArea::both()
                .id_salt("export_preview")
                .max_height(150.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .code_editor()
                            .interactive(false)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(super::primary_button(&theme, "Save to file…"))
                        .clicked()
                    {
                        done = Done::Save;
                    }
                    if ui
                        .add_enabled(dialog.scope != Scope::All, egui::Button::new("Copy"))
                        .on_hover_text("Put it on the clipboard instead of in a file")
                        .on_disabled_hover_text("All rows can only go to a file.")
                        .clicked()
                    {
                        done = Done::Copy;
                    }
                    if ui.button("Cancel").clicked() {
                        done = Done::Close;
                    }
                });
            });
        });
    if modal.should_close() && matches!(done, Done::Keep) {
        done = Done::Close;
    }

    let options = options(&dialog);
    let block = block(&dialog);
    match done {
        Done::Keep => {}
        Done::Close => return,
        Done::Copy => {
            ctx.copy_text(grid::text(result, block, dialog.format, &options));
        }
        Done::Save => {
            let name = match dialog.table.rsplit('.').next().map(|name| {
                name.chars()
                    .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
                    .collect::<String>()
            }) {
                Some(name) if !name.trim().is_empty() => name.trim().to_string(),
                _ => "result".to_string(),
            };
            let extension = dialog.format.extension();
            let Some(path) = rfd::FileDialog::new()
                .set_title("Export result")
                .set_file_name(format!("{name}.{extension}"))
                .add_filter(dialog.format.label(), &[extension])
                .save_file()
            else {
                // Back to the box, as it was.
                app.export = Some(dialog);
                return;
            };
            if dialog.scope == Scope::All {
                let (tab, format) = (dialog.tab, dialog.format);
                remember(app, &dialog);
                app.export_all(tab, path, format, options);
                return;
            }
            let written =
                write(&path, result, block, dialog.format, &options).map_err(|err| err.to_string());
            remember(app, &dialog);
            app.export_done(written, &path);
            return;
        }
    }
    if matches!(done, Done::Copy) {
        remember(app, &dialog);
        return;
    }
    app.export = Some(dialog);
}

/// Keep what was chosen: the format for next time, the table and the keys
/// for copying from this result.
fn remember(app: &mut App, dialog: &ExportDialog) {
    if let Some(tab) = app.tabs.iter_mut().find(|tab| tab.id == dialog.tab) {
        tab.extract.table = dialog.table.clone();
        tab.extract.keys = dialog.keys.clone();
    }
    if app.data.settings.export_format != dialog.format.id() {
        app.data.settings.export_format = dialog.format.id().to_string();
        app.save();
    }
}
