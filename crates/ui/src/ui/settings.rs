//! The Settings page: the look, queries and Java.

use eframe::egui::{self, RichText};

use dbpadu_core::model::{Appearance, DbKind, ROW_LIMITS};

use crate::app::{App, EngineState, View};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    super::page_title(
        ui,
        &theme,
        "Settings",
        "Changes are saved as you make them.",
    );
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .id_salt("settings_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(760.0);

            super::section(ui, "Look");
            ui.horizontal(|ui| {
                let mut appearance = app.data.settings.appearance;
                for choice in Appearance::ALL {
                    ui.selectable_value(&mut appearance, choice, choice.label());
                }
                if appearance != app.data.settings.appearance {
                    app.set_appearance(appearance);
                }
            });

            super::section(ui, "Queries");
            ui.horizontal(|ui| {
                ui.label("Max rows of a new query tab");
                let mut limit = app.data.settings.row_limit;
                egui::ComboBox::from_id_salt("default_row_limit")
                    .selected_text(limit.to_string())
                    .show_ui(ui, |ui| {
                        for choice in ROW_LIMITS {
                            ui.selectable_value(&mut limit, choice, choice.to_string());
                        }
                    });
                if limit != app.data.settings.row_limit {
                    app.data.settings.row_limit = limit;
                    app.save();
                }
            });
            let mut log_open = app.data.settings.log_open;
            if ui
                .checkbox(&mut log_open, "Show the log at the bottom")
                .changed()
            {
                app.set_log_open(log_open);
            }

            java(app, ui);
            drivers(app, ui);
            ui.add_space(12.0);
        });
}

fn java(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    super::section(ui, "Java");
    ui.label(
        RichText::new(
            "The query engine runs on Java 17 or newer. Leave this empty to use JAVA_HOME, \
             then the java on PATH.",
        )
        .small()
        .color(theme.text_dim),
    );
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.button("Browse…").clicked()
            && let Some(path) = rfd::FileDialog::new().pick_file()
        {
            app.data.settings.java_path = path.display().to_string();
            changed = true;
        }
        if ui
            .button("Restart engine")
            .on_hover_text("Open connections are closed. Needed after changing Java.")
            .clicked()
        {
            app.start_engine();
        }
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut app.data.settings.java_path)
                    .hint_text("Path of the java program")
                    .desired_width(f32::INFINITY),
            )
            .changed();
    });
    if changed {
        app.save();
    }
    let (text, color) = match &app.engine_state {
        EngineState::Starting => ("The engine is starting…".to_string(), theme.text_dim),
        EngineState::Ready(hello) => (
            format!(
                "Engine {} is running on Java {}.",
                hello.version, hello.java
            ),
            theme.ok,
        ),
        EngineState::Failed(reason) => (reason.clone(), theme.danger),
    };
    ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap());
}

fn drivers(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    super::section(ui, "JDBC drivers");
    ui.label(
        RichText::new(
            "A driver for every database type is inside dbpadu. Other versions and other \
             databases are added on the Drivers page.",
        )
        .small()
        .color(theme.text_dim),
    );
    ui.horizontal(|ui| {
        if ui.button("Open Drivers").clicked() {
            app.view = View::Drivers;
        }
        ui.label(
            RichText::new(format!(
                "{} inside dbpadu · {} of your own",
                DbKind::ALL.len(),
                app.data.drivers.len()
            ))
            .small()
            .color(theme.text_dim),
        );
    });
}
