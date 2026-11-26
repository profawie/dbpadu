//! The About page: versions, drivers and where the app keeps its files.

use eframe::egui::{self, RichText};

use dbpadu_core::model::DbKind;
use dbpadu_engine::locate;

use crate::app::{App, EngineState, Load};

const SOURCE: &str = "https://github.com/profawie/dbpadu";

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    super::page_title(
        ui,
        &theme,
        crate::APP_NAME,
        "A friendly desktop database client.",
    );
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .id_salt("about_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(860.0);
            let dim = |text: &str| RichText::new(text).color(theme.text_dim);

            egui::Grid::new("about_versions")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    ui.label(dim("Version"));
                    ui.label(crate::APP_VERSION);
                    ui.end_row();
                    ui.label(dim("Query engine"));
                    match &app.engine_state {
                        EngineState::Starting => ui.label("Starting…"),
                        EngineState::Ready(hello) => ui.label(format!(
                            "{} on Java {} · at most {} rows for one query",
                            hello.version, hello.java, hello.max_rows
                        )),
                        EngineState::Failed(reason) => {
                            ui.label(RichText::new(reason).color(theme.danger))
                        }
                    };
                    ui.end_row();
                    ui.label(dim("Source code"));
                    ui.hyperlink(SOURCE);
                    ui.end_row();
                });

            super::section(ui, "JDBC drivers");
            egui::Grid::new("about_drivers")
                .num_columns(4)
                .striped(true)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for title in ["Database", "Driver", "Package", "Version"] {
                        ui.label(dim(title));
                    }
                    ui.end_row();
                    for kind in DbKind::ALL {
                        ui.label(kind.label());
                        if let Some(driver) = app.bundled_driver(kind) {
                            ui.label(&driver.name);
                            ui.label(&driver.package);
                            ui.label(&driver.version);
                        } else {
                            ui.label(dim("…"));
                            ui.label("");
                            ui.label("");
                        }
                        ui.end_row();
                    }
                    for driver in &app.data.drivers {
                        ui.label(driver.kind.label());
                        ui.label(&driver.name);
                        match app.driver_checks.get(&driver.id) {
                            Some(Load::Ready(info)) => ui.label(&info.class_name),
                            Some(Load::Failed(error)) => ui
                                .label(RichText::new("Not usable").color(theme.danger))
                                .on_hover_text(error),
                            _ => ui.label(dim("…")),
                        };
                        ui.label(app.driver_version(driver));
                        ui.end_row();
                    }
                });
            ui.label(
                RichText::new("The first six are inside dbpadu. Add your own on the Drivers page.")
                    .small()
                    .color(theme.text_dim),
            );

            super::section(ui, "Files");
            let path = |path: Option<std::path::PathBuf>| match path {
                Some(path) => path.display().to_string(),
                None => "Not available on this system".to_string(),
            };
            egui::Grid::new("about_files")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    ui.label(dim("Connections and settings"));
                    ui.label(RichText::new(path(app.data_path().cloned())).monospace());
                    ui.end_row();
                    ui.label(dim("Known SSH servers"));
                    ui.label(RichText::new(path(app.known_hosts())).monospace());
                    ui.end_row();
                    ui.label(dim("Query engine"));
                    ui.label(RichText::new(path(locate::jar())).monospace());
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "Passwords are never in these files. They stay in memory, or in the \
                     system's credential store when you ask for them to be remembered.",
                )
                .small()
                .color(theme.text_dim),
            );
        });
}
