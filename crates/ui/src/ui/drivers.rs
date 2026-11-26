//! The Drivers page: every JDBC driver as a row, the ones inside dbpadu and
//! the user's own. And the driver form, where a driver of the user's own is
//! added or changed.

use eframe::egui::{self, RichText};
use uuid::Uuid;

use dbpadu_core::model::{DbKind, Driver, Property};
use dbpadu_engine::protocol::OwnDriverInfo;

use super::editor::dialog_frame;
use crate::{
    app::{App, EngineState, Load},
    theme::Theme,
};

const FORM_WIDTH: f32 = 640.0;
/// Height of the list of libraries or properties in the form.
const LIST_HEIGHT: f32 = 132.0;

/// The pages at the bottom of the driver form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Libraries,
    Properties,
}

/// The driver form while it is open.
pub struct DriverEditor {
    pub draft: Driver,
    port: String,
    is_new: bool,
    page: Page,
    /// What the engine found in the libraries.
    probe: Load<OwnDriverInfo>,
    /// The number of the look whose answer is awaited.
    probe_id: u64,
    /// Look into the libraries again on the next frame.
    probe_wanted: bool,
    confirm_delete: bool,
}

impl DriverEditor {
    /// A new driver for `kind`, set up like the one inside dbpadu.
    pub fn new(kind: DbKind) -> Self {
        Self::with(Driver::new(kind), true)
    }

    pub fn edit(driver: &Driver) -> Self {
        Self::with(driver.clone(), false)
    }

    /// A new driver that starts as a copy of another.
    pub fn copy_of(driver: Driver) -> Self {
        Self::with(driver, true)
    }

    fn with(draft: Driver, is_new: bool) -> Self {
        Self {
            port: match draft.default_port {
                0 => String::new(),
                port => port.to_string(),
            },
            probe_wanted: !draft.library_paths().is_empty(),
            draft,
            is_new,
            page: Page::Libraries,
            probe: Load::Idle,
            probe_id: 0,
            confirm_delete: false,
        }
    }

    /// The engine's answer to look number `probe`.
    pub fn probed(&mut self, probe: u64, found: Load<OwnDriverInfo>) {
        if probe != self.probe_id {
            return;
        }
        // The class is filled in when the files hold exactly one driver.
        if let Load::Ready(info) = &found
            && self.draft.class_name.trim().is_empty()
            && info.classes.len() == 1
        {
            self.draft.class_name = info.classes[0].clone();
        }
        self.probe = found;
    }
}

/// What a click on the page asks for. Collected while drawing, done after.
enum Action {
    New,
    Edit(Uuid),
    Duplicate(Uuid),
    /// A driver of the user's own that starts like the one inside dbpadu.
    CopyBuiltIn(DbKind),
}

/// A small outlined label, for a version.
fn pill(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme.border))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().color(theme.text_dim));
        });
}

/// "No connections", "1 connection", "3 connections".
fn uses(n: usize) -> String {
    match n {
        0 => "Not used yet".to_string(),
        n => format!("Used by {}", super::count(n, "connection", "connections")),
    }
}

/// The file name of a path, for a short line.
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// One driver on the page.
struct Row<'a> {
    name: &'a str,
    version: &'a str,
    /// What it is, in one dim line.
    detail: &'a str,
    /// A dot before the name, with what it means.
    status: Option<(egui::Color32, String)>,
    /// How many connections use it.
    used: usize,
}

impl Row<'_> {
    /// Name and version, what it is, and its buttons at the right.
    fn show(self, ui: &mut egui::Ui, theme: &Theme, buttons: impl FnOnce(&mut egui::Ui)) {
        super::card(theme)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        buttons(ui);
                        ui.label(RichText::new(uses(self.used)).small().color(theme.text_dim));
                        ui.add_space(8.0);
                        // What is left, at the left.
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.spacing_mut().item_spacing.y = 3.0;
                            ui.horizontal(|ui| {
                                if let Some((color, hover)) = self.status {
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(10.0, 14.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(rect.center(), 4.0, color);
                                    response.on_hover_text(hover);
                                }
                                ui.label(RichText::new(self.name).size(15.0).strong());
                                if !self.version.is_empty() {
                                    pill(ui, theme, self.version);
                                }
                            });
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.detail).small().color(theme.text_dim),
                                )
                                .truncate(),
                            );
                        });
                    });
                });
            });
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            super::page_title(
                ui,
                &theme,
                "Drivers",
                "The JDBC drivers dbpadu connects with. Add your own for another version or \
                 another database.",
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(super::primary_button(&theme, "New driver"))
                .clicked()
            {
                actions.push(Action::New);
            }
            ui.add(
                egui::TextEdit::singleline(&mut app.driver_search)
                    .hint_text("Search")
                    .desired_width(180.0),
            );
        });
    });
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut app.driver_kind, None, "All");
        for kind in DbKind::WITH_OTHER {
            // "Other" is offered once there is a driver for it.
            if kind == DbKind::Other && !app.data.drivers.iter().any(|d| d.kind == kind) {
                continue;
            }
            ui.selectable_value(&mut app.driver_kind, Some(kind), kind.label());
        }
    });
    ui.add_space(8.0);

    let needle = app.driver_search.trim().to_lowercase();
    let matches = |texts: &[&str]| {
        needle.is_empty()
            || texts
                .iter()
                .any(|text| text.to_lowercase().contains(&needle))
    };
    egui::ScrollArea::vertical()
        .id_salt("drivers_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(900.0);
            let mut shown = 0;
            for kind in DbKind::WITH_OTHER {
                if app.driver_kind.is_some_and(|only| only != kind) {
                    continue;
                }
                let built_in = app
                    .bundled_driver(kind)
                    .filter(|driver| matches(&[kind.label(), &driver.name, &driver.class_name]));
                // Until the engine has said what is inside it.
                let waiting = kind != DbKind::Other
                    && !matches!(app.engine_state, EngineState::Ready(_))
                    && matches(&[kind.label()]);
                let own: Vec<&Driver> = app
                    .data
                    .drivers
                    .iter()
                    .filter(|driver| driver.kind == kind)
                    .filter(|driver| {
                        matches(&[
                            kind.label(),
                            &driver.name,
                            &driver.class_name,
                            &driver.description,
                        ])
                    })
                    .collect();
                if built_in.is_none() && !waiting && own.is_empty() {
                    continue;
                }
                shown += 1;
                ui.add_space(6.0);
                ui.label(RichText::new(kind.label()).color(theme.text_dim).strong());
                let used = app
                    .data
                    .connections
                    .iter()
                    .filter(|c| c.kind == kind && c.driver.is_none())
                    .count();
                if let Some(driver) = built_in {
                    let detail = format!(
                        "Inside dbpadu  ·  {}  ·  {}",
                        driver.class_name, driver.package
                    );
                    Row {
                        name: &driver.name,
                        version: &driver.version,
                        detail: &detail,
                        status: None,
                        used,
                    }
                    .show(ui, &theme, |ui| {
                        if ui
                            .button("Copy as new")
                            .on_hover_text(
                                "Start a driver of your own from this one, for another \
                                     version of its .jar file",
                            )
                            .clicked()
                        {
                            actions.push(Action::CopyBuiltIn(kind));
                        }
                    });
                } else if waiting {
                    Row {
                        name: kind.label(),
                        version: "",
                        detail: "Inside dbpadu  ·  shown when the engine is running",
                        status: None,
                        used,
                    }
                    .show(ui, &theme, |_| {});
                }
                for driver in own {
                    let libraries = driver.library_paths();
                    let files = match libraries.as_slice() {
                        [one] => file_name(one),
                        many => format!("{} files", many.len()),
                    };
                    let (class, status) = match app.driver_checks.get(&driver.id) {
                        Some(Load::Ready(info)) => (
                            info.class_name.clone(),
                            (theme.ok, "The driver was found in its files.".to_string()),
                        ),
                        Some(Load::Failed(error)) => (
                            driver.class_name.trim().to_string(),
                            (theme.danger, error.clone()),
                        ),
                        Some(Load::Loading) => (
                            driver.class_name.trim().to_string(),
                            (theme.warn, "Looking into the files…".to_string()),
                        ),
                        _ => (
                            driver.class_name.trim().to_string(),
                            (
                                theme.text_dim.gamma_multiply(0.6),
                                "Checked when the engine is running.".to_string(),
                            ),
                        ),
                    };
                    let mut detail = format!("Your driver  ·  {class}  ·  {files}");
                    if !driver.description.trim().is_empty() {
                        detail.push_str(&format!("  ·  {}", driver.description.trim()));
                    }
                    let id = driver.id;
                    Row {
                        name: &driver.name,
                        version: &app.driver_version(driver),
                        detail: &detail,
                        status: Some(status),
                        used: app.data.driver_uses(id),
                    }
                    .show(ui, &theme, |ui| {
                        if ui
                            .button("Duplicate")
                            .on_hover_text("A copy to change into another version")
                            .clicked()
                        {
                            actions.push(Action::Duplicate(id));
                        }
                        if ui.button("Edit").clicked() {
                            actions.push(Action::Edit(id));
                        }
                    });
                }
            }
            if shown == 0 {
                ui.label(RichText::new("No driver matches the search.").color(theme.text_dim));
            }
            ui.add_space(12.0);
        });

    for action in actions {
        app.driver_editor = match action {
            Action::New => Some(DriverEditor::new(app.driver_kind.unwrap_or_default())),
            Action::Edit(id) => app.data.driver(id).map(DriverEditor::edit),
            Action::Duplicate(id) => app.data.duplicate_driver(id).map(DriverEditor::copy_of),
            Action::CopyBuiltIn(kind) => {
                let mut driver = Driver::new(kind);
                if let Some(built_in) = app.bundled_driver(kind) {
                    driver.class_name = built_in.class_name.clone();
                }
                Some(DriverEditor::copy_of(driver))
            }
        };
    }
}

enum Outcome {
    Keep,
    Close,
    Save,
    Delete,
}

/// The list of `.jar` files and folders. Returns whether it changed.
fn libraries_page(ui: &mut egui::Ui, theme: &Theme, draft: &mut Driver) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.button("Add .jar file…").clicked()
            && let Some(paths) = rfd::FileDialog::new()
                .add_filter("JDBC driver", &["jar"])
                .pick_files()
        {
            for path in paths {
                let path = path.display().to_string();
                if !draft.libraries.contains(&path) {
                    draft.libraries.push(path);
                    changed = true;
                }
            }
        }
        if ui
            .button("Add folder…")
            .on_hover_text("Every .jar file in the folder belongs to the driver")
            .clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            draft.libraries.push(path.display().to_string());
            changed = true;
        }
        if ui
            .button("Add a line")
            .on_hover_text("Type or paste the path of a .jar file")
            .clicked()
        {
            draft.libraries.push(String::new());
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("driver_libraries")
        .max_height(LIST_HEIGHT)
        .min_scrolled_height(LIST_HEIGHT)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if draft.libraries.is_empty() {
                ui.label(
                    RichText::new(
                        "No files yet. Add the .jar file of the driver, as downloaded from \
                         the database's maker or from Maven Central.",
                    )
                    .small()
                    .color(theme.text_dim),
                );
            }
            let mut remove = None;
            for (index, path) in draft.libraries.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                    let field = ui.add(
                        egui::TextEdit::singleline(path)
                            .hint_text("Path of a .jar file")
                            .desired_width(f32::INFINITY),
                    );
                    changed |= field.lost_focus();
                });
            }
            if let Some(index) = remove {
                draft.libraries.remove(index);
                changed = true;
            }
        });
    changed
}

fn properties_page(ui: &mut egui::Ui, theme: &Theme, draft: &mut Driver) {
    ui.horizontal(|ui| {
        if ui.button("Add property").clicked() {
            draft.properties.push(Property::default());
        }
        ui.label(
            RichText::new("Handed to the driver with every connection, such as useSSL = false.")
                .small()
                .color(theme.text_dim),
        );
    });
    egui::ScrollArea::vertical()
        .id_salt("driver_properties")
        .max_height(LIST_HEIGHT)
        .min_scrolled_height(LIST_HEIGHT)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if draft.properties.is_empty() {
                ui.label(
                    RichText::new("No properties. Most drivers need none.")
                        .small()
                        .color(theme.text_dim),
                );
            }
            let mut remove = None;
            for (index, property) in draft.properties.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut property.name)
                            .hint_text("Name")
                            .desired_width(220.0),
                    );
                    ui.label("=");
                    ui.add(
                        egui::TextEdit::singleline(&mut property.value)
                            .hint_text("Value")
                            .desired_width(f32::INFINITY),
                    );
                });
            }
            if let Some(index) = remove {
                draft.properties.remove(index);
            }
        });
}

/// What the engine found in the libraries, under the list of them.
fn probe_line(ui: &mut egui::Ui, theme: &Theme, editor: &mut DriverEditor, engine_ready: bool) {
    let mut pick = None;
    match &editor.probe {
        Load::Idle if editor.draft.library_paths().is_empty() => {}
        Load::Idle if !engine_ready => {
            ui.label(
                RichText::new("The files are checked when the engine is running.")
                    .small()
                    .color(theme.text_dim),
            );
        }
        Load::Idle => {}
        Load::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Looking into the files…")
                        .small()
                        .color(theme.text_dim),
                );
            });
        }
        Load::Failed(error) => {
            ui.add(egui::Label::new(RichText::new(error).small().color(theme.danger)).wrap());
        }
        Load::Ready(info) => {
            if let Some(error) = &info.error {
                ui.add(egui::Label::new(RichText::new(error).small().color(theme.danger)).wrap());
            } else {
                let version = if info.version.is_empty() {
                    "unknown"
                } else {
                    &info.version
                };
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Found: {} · version {version}", info.class_name))
                            .small()
                            .color(theme.ok),
                    )
                    .wrap(),
                );
            }
            // More than one driver in the files: any of them can be chosen.
            if info.classes.len() > 1 || info.error.is_some() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Driver classes in the files:")
                            .small()
                            .color(theme.text_dim),
                    );
                    for class in &info.classes {
                        let selected = editor.draft.class_name.trim() == class;
                        if ui
                            .selectable_label(selected, RichText::new(class).small())
                            .clicked()
                            && !selected
                        {
                            pick = Some(class.clone());
                        }
                    }
                });
            }
        }
    }
    if let Some(class) = pick {
        editor.draft.class_name = class;
        editor.probe_wanted = true;
    }
}

/// The driver form.
pub fn editor(app: &mut App, ctx: &egui::Context) {
    let Some(mut editor) = app.driver_editor.take() else {
        return;
    };
    let theme = app.theme;
    let engine_ready = matches!(app.engine_state, EngineState::Ready(_));
    if editor.probe_wanted && engine_ready {
        editor.probe_wanted = false;
        if editor.draft.library_paths().is_empty() {
            editor.probe = Load::Idle;
            editor.probe_id = 0;
        } else if let Some(id) = app.probe_driver(&editor.draft) {
            editor.probe_id = id;
            editor.probe = Load::Loading;
        }
    }
    let used = app.data.driver_uses(editor.draft.id);
    let taken = app.data.drivers.iter().any(|driver| {
        driver.id != editor.draft.id
            && driver
                .name
                .trim()
                .eq_ignore_ascii_case(editor.draft.name.trim())
    });
    let mut outcome = Outcome::Keep;

    let modal = egui::Modal::new(egui::Id::unique("driver_editor"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(FORM_WIDTH);
            ui.label(
                RichText::new(if editor.is_new {
                    "New driver"
                } else {
                    "Edit driver"
                })
                .size(20.0)
                .strong(),
            );
            ui.add_space(4.0);
            let draft = &mut editor.draft;

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(430.0);
                    super::field(ui, &theme, "Name", &mut draft.name, "For example: MySQL 5");
                });
                ui.vertical(|ui| {
                    super::field(
                        ui,
                        &theme,
                        "Version (optional)",
                        &mut draft.version,
                        "Read from the file",
                    );
                });
            });

            super::field_label(ui, &theme, "Database type");
            ui.horizontal_wrapped(|ui| {
                let mut kind = draft.kind;
                for choice in DbKind::WITH_OTHER {
                    ui.selectable_value(&mut kind, choice, choice.label());
                }
                if kind != draft.kind {
                    let port = draft.default_port;
                    draft.set_kind(kind);
                    if draft.default_port != port {
                        editor.port = match draft.default_port {
                            0 => String::new(),
                            port => port.to_string(),
                        };
                    }
                }
            });
            let about_kind = match draft.kind {
                DbKind::Other => {
                    "Any database with a JDBC driver. The row limit is then kept by the driver \
                     alone; no LIMIT is added to the SQL."
                }
                _ => "Decides how the row limit is written and how the tree is read.",
            };
            ui.label(RichText::new(about_kind).small().color(theme.text_dim));

            super::field_label(ui, &theme, "Class name");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!draft.library_paths().is_empty(), egui::Button::new("Find"))
                    .on_hover_text("Look for the driver class in the libraries")
                    .on_disabled_hover_text("Add the .jar file first")
                    .clicked()
                {
                    draft.class_name.clear();
                    editor.probe_wanted = true;
                }
                let class = ui.add(
                    egui::TextEdit::singleline(&mut draft.class_name)
                        .hint_text("Empty: the class the .jar file names itself")
                        .desired_width(f32::INFINITY),
                );
                editor.probe_wanted |= class.lost_focus() && class.changed();
            });

            super::field(
                ui,
                &theme,
                "URL template",
                &mut draft.url_template,
                "jdbc:mysql://{host}[:{port}]/[{database}]",
            );
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{{host}}, {{port}}, {{database}} and {{user}} are filled in from the \
                         connection; a part in [ ] is left out when it is empty. For example: {}",
                        draft.example_url()
                    ))
                    .small()
                    .color(theme.text_dim),
                )
                .wrap(),
            );

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(110.0);
                    super::field_label(ui, &theme, "Default port");
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut editor.port)
                            .hint_text("None")
                            .desired_width(f32::INFINITY),
                    );
                    if field.changed() {
                        editor.port.retain(|c| c.is_ascii_digit());
                        draft.default_port = editor.port.parse().unwrap_or(0);
                    }
                });
                ui.vertical(|ui| {
                    ui.set_width(250.0);
                    super::field(
                        ui,
                        &theme,
                        "Default database",
                        &mut draft.default_database,
                        "",
                    );
                });
                ui.vertical(|ui| {
                    super::field(ui, &theme, "Default user", &mut draft.default_user, "");
                });
            });
            super::field(
                ui,
                &theme,
                "Description (optional)",
                &mut draft.description,
                "For example: for the old MySQL 5.5 servers",
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let libraries = format!("Libraries ({})", draft.library_paths().len());
                let properties =
                    format!("Connection properties ({})", draft.set_properties().len());
                ui.selectable_value(&mut editor.page, Page::Libraries, libraries);
                ui.selectable_value(&mut editor.page, Page::Properties, properties);
            });
            match editor.page {
                Page::Libraries => {
                    editor.probe_wanted |= libraries_page(ui, &theme, draft);
                }
                Page::Properties => properties_page(ui, &theme, draft),
            }
            probe_line(ui, &theme, &mut editor, engine_ready);

            let problem = editor.draft.problem().or(taken
                .then_some("Another driver has this name. Add the version to tell them apart."));
            if let Some(problem) = problem {
                ui.label(RichText::new(problem).small().color(theme.text_dim));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if !editor.is_new {
                    if editor.confirm_delete {
                        if ui
                            .button(RichText::new("Yes, delete it").color(theme.danger))
                            .clicked()
                        {
                            outcome = Outcome::Delete;
                        }
                    } else if ui
                        .add_enabled(used == 0, egui::Button::new("Delete"))
                        .on_disabled_hover_text(format!(
                            "{}. Give them another driver first.",
                            uses(used)
                        ))
                        .clicked()
                    {
                        editor.confirm_delete = true;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(problem.is_none(), super::primary_button(&theme, "Save"))
                        .clicked()
                    {
                        outcome = Outcome::Save;
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Outcome::Close;
                    }
                });
            });
        });
    if modal.should_close() && matches!(outcome, Outcome::Keep) {
        outcome = Outcome::Close;
    }

    match outcome {
        Outcome::Keep => app.driver_editor = Some(editor),
        Outcome::Close => {}
        Outcome::Delete => app.delete_driver(editor.draft.id),
        Outcome::Save => app.save_driver(editor.draft),
    }
}
