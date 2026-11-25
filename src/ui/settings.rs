//! The Settings page: the look, queries, Java and the JDBC drivers.

use eframe::egui::{self, RichText};

use dbpadu_core::model::{Appearance, DbKind, ROW_LIMITS};

use crate::app::{App, EngineState, Load};

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
            "A driver for every database type is inside dbpadu. To use a newer or another \
             version, download its .jar file and choose it here. It is used for connections \
             opened from then on.",
        )
        .small()
        .color(theme.text_dim),
    );
    for kind in DbKind::ALL {
        super::card(&theme).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                super::kind_badge(ui, kind, 30.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(kind.label()).strong());
                    let inside = match app.bundled_driver(kind) {
                        Some(driver) => format!(
                            "Inside dbpadu: {} · {} · version {}",
                            driver.name, driver.package, driver.version
                        ),
                        None => "Inside dbpadu: shown when the engine is running".to_string(),
                    };
                    ui.add(
                        egui::Label::new(RichText::new(inside).small().color(theme.text_dim))
                            .wrap(),
                    );
                });
            });

            let saved = app.data.settings.drivers.get(&kind);
            let mut own = saved.cloned().unwrap_or_default();
            let before = own.clone();
            let mut check = false;
            super::field_label(ui, &theme, "Your own driver (.jar file)");
            ui.horizontal(|ui| {
                if ui.button("Browse…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("JDBC driver", &["jar"])
                        .pick_file()
                {
                    own.jar = path.display().to_string();
                    check = true;
                }
                if !own.jar.trim().is_empty() {
                    if ui
                        .button("Use the one inside dbpadu")
                        .on_hover_text("Forget your own driver for this database type")
                        .clicked()
                    {
                        own = Default::default();
                    }
                    if ui.button("Check").clicked() {
                        check = true;
                    }
                }
                let jar = ui.add(
                    egui::TextEdit::singleline(&mut own.jar)
                        .hint_text("Empty: the driver inside dbpadu is used")
                        .desired_width(f32::INFINITY),
                );
                check |= jar.lost_focus() && own.jar != before.jar;
            });
            if !own.jar.trim().is_empty() {
                super::field_label(ui, &theme, "Driver class (optional)");
                let class = ui.add(
                    egui::TextEdit::singleline(&mut own.class_name)
                        .hint_text("Empty: the class the .jar file names itself")
                        .desired_width(f32::INFINITY),
                );
                check |= class.lost_focus();
            }
            if own != before {
                if own.jar.trim().is_empty() && own.class_name.trim().is_empty() {
                    app.data.settings.drivers.remove(&kind);
                } else {
                    app.data.settings.drivers.insert(kind, own.clone());
                }
                app.driver_checks.remove(&kind);
                app.save();
            }
            if check {
                app.check_driver(kind);
            }
            if app.own_driver(kind).is_some() {
                let (text, color) = match app.driver_checks.get(&kind) {
                    Some(Load::Ready(info)) => (
                        format!(
                            "In use: {} · version {}",
                            info.class_name,
                            if info.version.is_empty() {
                                "unknown"
                            } else {
                                &info.version
                            }
                        ),
                        theme.ok,
                    ),
                    Some(Load::Failed(error)) => (error.clone(), theme.danger),
                    Some(Load::Loading) => ("Looking into the file…".to_string(), theme.text_dim),
                    _ => (
                        "Click Check to see that the file holds a driver.".to_string(),
                        theme.text_dim,
                    ),
                };
                ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap());
            }
        });
    }
}
