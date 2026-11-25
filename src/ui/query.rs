//! A query tab: toolbar, SQL editor and the result under it.

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use dbpadu_core::{model::ROW_LIMITS, sql};
use dbpadu_engine::protocol::QueryResult;

use crate::{
    app::{App, Link, LinkState, Outcome, QueryTab},
    theme::Theme,
};

const ROW_HEIGHT: f32 = 24.0;
/// Characters of a cell shown when the pointer rests on it.
const HOVER_CHARS: usize = 2_000;

#[derive(Default)]
struct Actions {
    run: bool,
    stop: bool,
    format: bool,
    connect: bool,
    auto_commit: Option<bool>,
    commit: bool,
    rollback: bool,
}

pub fn show(app: &mut App, ui: &mut egui::Ui, tab_id: u64) {
    let theme = app.theme;
    let Some(index) = app.tabs.iter().position(|tab| tab.id == tab_id) else {
        return;
    };
    let connection = app.tabs[index].connection;
    let mut actions = Actions::default();

    // Shortcuts are taken before the editor sees the keys.
    let command = egui::Modifiers::COMMAND;
    ui.input_mut(|input| {
        actions.run = input.consume_key(command, egui::Key::Enter)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::F5);
        actions.format = input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::F);
    });

    egui::Panel::top("query_toolbar")
        .resizable(false)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(theme.bg)
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| {
            let tab = &mut app.tabs[index];
            toolbar(ui, &theme, tab, app.links.get(&connection), &mut actions);
        });

    egui::Panel::top("query_editor")
        .resizable(true)
        .default_size(200.0)
        .size_range(80.0..=600.0)
        .frame(
            egui::Frame::new()
                .fill(theme.bg)
                .inner_margin(egui::Margin {
                    left: 12,
                    right: 12,
                    top: 0,
                    bottom: 8,
                }),
        )
        .show(ui, |ui| editor(ui, &theme, &mut app.tabs[index]));

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme.bg)
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| result(ui, &theme, &app.tabs[index]));

    if actions.format {
        let tab = &mut app.tabs[index];
        if !tab.sql.trim().is_empty() {
            tab.sql = sql::format(&tab.sql);
            tab.selection = None;
            tab.cursor = tab.sql.chars().count();
        }
    }
    if actions.connect {
        app.connect(connection);
    }
    if actions.run {
        app.run(tab_id);
    }
    if actions.stop {
        app.cancel(connection);
    }
    if let Some(value) = actions.auto_commit {
        app.set_auto_commit(connection, value);
    }
    if actions.commit {
        app.commit(connection);
    }
    if actions.rollback {
        app.rollback(connection);
    }
}

fn toolbar(
    ui: &mut egui::Ui,
    theme: &Theme,
    tab: &mut QueryTab,
    link: Option<&Link>,
    actions: &mut Actions,
) {
    let connected = link.is_some_and(Link::connected);
    ui.horizontal(|ui| {
        if let Some(running) = &tab.running {
            if ui
                .button(RichText::new("Stop").color(theme.danger))
                .on_hover_text("Ask the database to stop this query")
                .clicked()
            {
                actions.stop = true;
            }
            ui.spinner();
            ui.label(
                RichText::new(format!("{:.1} s", running.since.elapsed().as_secs_f32()))
                    .color(theme.text_dim),
            );
        } else if connected {
            if ui
                .add(super::primary_button(theme, "▶  Run"))
                .on_hover_text(
                    "Runs the statement the cursor is in, or the selected text.\nCtrl+Enter or F5",
                )
                .clicked()
            {
                actions.run = true;
            }
        } else {
            let connecting = matches!(link.map(|link| &link.state), Some(LinkState::Connecting));
            if ui
                .add_enabled(
                    !connecting,
                    super::primary_button(
                        theme,
                        if connecting {
                            "Connecting…"
                        } else {
                            "Connect"
                        },
                    ),
                )
                .clicked()
            {
                actions.connect = true;
            }
        }
        if ui
            .button("Format")
            .on_hover_text("Tidy the SQL: one clause per line, keywords in capitals.\nCtrl+Shift+F")
            .clicked()
        {
            actions.format = true;
        }

        ui.separator();
        ui.label(RichText::new("Max rows").color(theme.text_dim))
            .on_hover_text(
                "A SELECT never returns more rows than this.\n\
                 If your SQL has no limit, dbpadu adds one and shows it in the bar at the bottom.",
            );
        egui::ComboBox::from_id_salt(("limit", tab.id))
            .width(80.0)
            .selected_text(tab.limit.to_string())
            .show_ui(ui, |ui| {
                for limit in ROW_LIMITS {
                    ui.selectable_value(&mut tab.limit, limit, limit.to_string());
                }
            });

        if let Some(link) = link.filter(|link| link.connected()) {
            ui.separator();
            let mut auto = link.tx.auto_commit;
            if ui
                .checkbox(&mut auto, "Auto-commit")
                .on_hover_text(
                    "On: every change is saved as soon as it runs.\n\
                     Off: changes wait until you click Commit, and Rollback undoes them.",
                )
                .changed()
            {
                actions.auto_commit = Some(auto);
            }
            if !link.tx.auto_commit {
                let waiting = link.tx.pending > 0;
                if ui
                    .add_enabled(waiting, egui::Button::new("Commit"))
                    .on_hover_text("Save the changes to the database")
                    .clicked()
                {
                    actions.commit = true;
                }
                if ui
                    .add_enabled(waiting, egui::Button::new("Rollback"))
                    .on_hover_text("Undo the changes that are not committed")
                    .clicked()
                {
                    actions.rollback = true;
                }
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (text, hover) = match link.map(|link| &link.state) {
                Some(LinkState::Connected { product, version }) => {
                    (product.clone(), Some(format!("{product} {version}")))
                }
                Some(LinkState::Connecting) => ("Connecting…".to_string(), None),
                Some(LinkState::Failed(error)) => {
                    ("Could not connect".to_string(), Some(error.clone()))
                }
                None => ("Not connected".to_string(), None),
            };
            let label = ui.label(RichText::new(text).small().color(theme.text_dim));
            if let Some(hover) = hover {
                label.on_hover_text(hover);
            }
            super::dot(ui, super::link_color(theme, link));
        });
    });
}

fn editor(ui: &mut egui::Ui, theme: &Theme, tab: &mut QueryTab) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut job = super::highlight::job(theme, text.as_str(), font.clone());
        job.wrap.max_width = wrap_width;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    egui::ScrollArea::vertical()
        .id_salt(("editor", tab.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The field fills the panel, however tall it is dragged.
            let line = ui.fonts_mut(|fonts| fonts.row_height(&font));
            let rows = ((ui.available_height() - 12.0) / line).floor().max(3.0) as usize;
            let output = egui::TextEdit::multiline(&mut tab.sql)
                .id_salt(("sql", tab.id))
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(rows)
                .hint_text("Type SQL here, then press Ctrl+Enter")
                .layouter(&mut layouter)
                .show(ui);
            if let Some(range) = output.cursor_range {
                let (a, b): (usize, usize) =
                    (range.primary.index.into(), range.secondary.index.into());
                tab.cursor = a;
                tab.selection = (a != b).then(|| a.min(b)..a.max(b));
            }
        });
}

fn result(ui: &mut egui::Ui, theme: &Theme, tab: &QueryTab) {
    match &tab.outcome {
        None => {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                let text = if tab.running.is_some() {
                    "Running…"
                } else {
                    "Results show here."
                };
                ui.label(RichText::new(text).color(theme.text_dim));
            });
        }
        Some(Outcome::Error(error)) => {
            super::card(theme)
                .stroke(egui::Stroke::new(1.0, theme.danger))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("The database said no")
                            .strong()
                            .color(theme.danger),
                    );
                    ui.add(egui::Label::new(RichText::new(error).monospace()).wrap());
                });
        }
        Some(Outcome::Result(result)) => {
            headline(ui, theme, result);
            if result.has_rows() {
                grid(ui, theme, tab, result);
            }
        }
    }
}

/// The line above the grid: how much came back, and what limited it.
fn headline(ui: &mut egui::Ui, theme: &Theme, result: &QueryResult) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(super::statusbar::summary(result)).strong());
        if result.has_rows() {
            if result.limit_reached {
                ui.label(
                    RichText::new(format!(
                        "Stopped at the limit of {} rows. The table may have more.",
                        result.limit
                    ))
                    .color(theme.warn),
                );
            }
            if let Some(added) = &result.added {
                ui.label(
                    RichText::new(format!("dbpadu added {added} to your SQL."))
                        .color(theme.text_dim),
                );
            }
        } else if !result.tx.auto_commit {
            ui.label(RichText::new("Not committed yet.").color(theme.warn));
        }
    });
}

fn grid(ui: &mut egui::Ui, theme: &Theme, tab: &QueryTab, result: &QueryResult) {
    if result.columns.is_empty() {
        return;
    }
    egui::ScrollArea::horizontal()
        .id_salt(("grid_scroll", tab.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            TableBuilder::new(ui)
                // Each run starts with fresh column widths.
                .id_salt(("grid", tab.id, tab.runs))
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto().at_least(36.0).resizable(false))
                .columns(
                    Column::initial(150.0).at_least(40.0).clip(true),
                    result.columns.len(),
                )
                .header(ROW_HEIGHT + 4.0, |mut header| {
                    header.col(|ui| {
                        ui.label(RichText::new("#").small().color(theme.text_dim));
                    });
                    for column in &result.columns {
                        header.col(|ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&column.name).strong()).truncate(),
                            )
                            .on_hover_text(format!(
                                "{} · {}",
                                column.name,
                                column.type_name.to_lowercase()
                            ));
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_HEIGHT, result.rows.len(), |mut row| {
                        let index = row.index();
                        row.col(|ui| {
                            ui.label(
                                RichText::new((index + 1).to_string())
                                    .small()
                                    .color(theme.text_dim),
                            );
                        });
                        for value in &result.rows[index] {
                            row.col(|ui| cell(ui, theme, value.as_deref()));
                        }
                    });
                });
        });
}

fn cell(ui: &mut egui::Ui, theme: &Theme, value: Option<&str>) {
    let Some(value) = value else {
        ui.label(
            RichText::new("NULL")
                .italics()
                .color(theme.text_dim.gamma_multiply(0.8)),
        );
        return;
    };
    // One line in the grid; the rest shows when the pointer rests on it.
    let first_line = value.lines().next().unwrap_or("");
    let cut = first_line.len() < value.len();
    let shown = if cut {
        format!("{first_line} …")
    } else {
        first_line.to_string()
    };
    let label = ui.add(egui::Label::new(shown).truncate());
    let label = if cut || value.len() > 24 {
        let hover: String = value.chars().take(HOVER_CHARS).collect();
        label.on_hover_text(hover)
    } else {
        label
    };
    label.context_menu(|ui| {
        if ui.button("Copy value").clicked() {
            ui.ctx().copy_text(value.to_string());
            ui.close();
        }
    });
}
