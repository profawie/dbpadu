//! The log at the bottom: the SQL that was sent to the databases, and what
//! the engine said. Both can only be read, copied and cleared.

use eframe::egui::{self, RichText};

use crate::app::{App, LogTab};

const ROW_HEIGHT: f32 = 20.0;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("Log").strong());
        ui.add_space(6.0);
        let sql = format!("SQL ({})", app.sql_log.len());
        ui.selectable_value(&mut app.log_tab, LogTab::Sql, sql)
            .on_hover_text("Every statement that was sent to a database, as it was sent");
        let engine = format!("Engine ({})", app.engine_log.len());
        ui.selectable_value(&mut app.log_tab, LogTab::Engine, engine)
            .on_hover_text("What the query engine and its drivers said");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Hide")
                .on_hover_text("Show it again with Log in the bar at the bottom")
                .clicked()
            {
                app.set_log_open(false);
            }
            let empty = match app.log_tab {
                LogTab::Sql => app.sql_log.is_empty(),
                LogTab::Engine => app.engine_log.is_empty(),
            };
            if ui
                .add_enabled(!empty, egui::Button::new("Clear").small())
                .clicked()
            {
                match app.log_tab {
                    LogTab::Sql => app.sql_log.clear(),
                    LogTab::Engine => app.engine_log.clear(),
                }
            }
            if ui
                .add_enabled(!empty, egui::Button::new("Copy all").small())
                .clicked()
            {
                ui.ctx().copy_text(all_text(app));
            }
        });
    });

    let rows = match app.log_tab {
        LogTab::Sql => app.sql_log.len(),
        LogTab::Engine => app.engine_log.len(),
    };
    if rows == 0 {
        let text = match app.log_tab {
            LogTab::Sql => "No SQL has been run yet.",
            LogTab::Engine => "The engine has not said anything yet.",
        };
        ui.label(RichText::new(text).small().color(theme.text_dim));
        // Keeps the panel as tall as it is with lines in it.
        ui.allocate_space(ui.available_size());
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show_rows(ui, ROW_HEIGHT, rows, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for index in range {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| match app.log_tab {
                        LogTab::Sql => sql_row(app, ui, index),
                        LogTab::Engine => engine_row(app, ui, index),
                    },
                );
            }
        });
}

/// The log that is shown, as text.
fn all_text(app: &App) -> String {
    match app.log_tab {
        LogTab::Sql => app
            .sql_log
            .iter()
            .map(|line| {
                let outcome = match &line.outcome {
                    Ok(text) => text.clone(),
                    Err(text) => format!("Failed: {text}"),
                };
                format!(
                    "-- {} · {} · {}\n{}\n",
                    line.time,
                    line.connection,
                    super::one_line(&outcome),
                    line.sql
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        LogTab::Engine => app
            .engine_log
            .iter()
            .map(|line| format!("{} {}", line.time, line.text))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn time(ui: &mut egui::Ui, app: &App, text: &str) {
    ui.label(
        RichText::new(text)
            .monospace()
            .size(12.0)
            .color(app.theme.text_dim),
    );
}

fn sql_row(app: &App, ui: &mut egui::Ui, index: usize) {
    let theme = app.theme;
    let line = &app.sql_log[index];
    time(ui, app, &line.time);
    ui.label(
        RichText::new(&line.connection)
            .small()
            .color(theme.accent_text),
    );
    // The outcome keeps its place at the right; the SQL is cut short.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        match &line.outcome {
            Ok(text) => {
                ui.label(RichText::new(text).small().color(theme.text_dim));
            }
            Err(text) => {
                ui.label(RichText::new("Failed").small().color(theme.danger))
                    .on_hover_text(text);
            }
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let label = ui
                .add(
                    egui::Label::new(
                        RichText::new(super::one_line(&line.sql))
                            .monospace()
                            .size(12.0),
                    )
                    .truncate(),
                )
                .on_hover_text(RichText::new(&line.sql).monospace());
            label.context_menu(|ui| {
                if ui.button("Copy SQL").clicked() {
                    ui.ctx().copy_text(line.sql.clone());
                    ui.close();
                }
            });
        });
    });
}

fn engine_row(app: &App, ui: &mut egui::Ui, index: usize) {
    let theme = app.theme;
    let line = &app.engine_log[index];
    time(ui, app, &line.time);
    // What the app notes itself is dimmer than what the engine wrote.
    let color = if line.text.starts_with("[app]") {
        theme.text_dim
    } else {
        theme.text
    };
    let label = ui.add(
        egui::Label::new(
            RichText::new(&line.text)
                .monospace()
                .size(12.0)
                .color(color),
        )
        .truncate(),
    );
    label.context_menu(|ui| {
        if ui.button("Copy line").clicked() {
            ui.ctx().copy_text(line.text.clone());
            ui.close();
        }
    });
}
