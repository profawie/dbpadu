//! The connection form, and the box that asks for a password.

use eframe::egui::{self, RichText};

use dbpadu_core::{
    model::{Connection, DbKind},
    secrets::Secrets,
};

use crate::app::{App, PasswordPrompt};

/// The connection form while it is open.
pub struct ConnectionEditor {
    pub draft: Connection,
    pub password: String,
    port: String,
    is_new: bool,
    /// The result of "Test connection": what answered, or why it failed.
    pub test: Option<Result<String, String>>,
    pub testing: bool,
    confirm_delete: bool,
}

impl ConnectionEditor {
    pub fn new() -> Self {
        Self::with(Connection::new(DbKind::default()), String::new(), true)
    }

    pub fn edit(connection: &Connection, secrets: &Secrets) -> Self {
        let password = if connection.save_password {
            secrets.get(connection.id).unwrap_or_default()
        } else {
            String::new()
        };
        Self::with(connection.clone(), password, false)
    }

    fn with(draft: Connection, password: String, is_new: bool) -> Self {
        Self {
            port: draft.port.to_string(),
            draft,
            password,
            is_new,
            test: None,
            testing: false,
            confirm_delete: false,
        }
    }
}

/// The frame of a dialog: a window with room around what is in it.
fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(18))
}

enum Outcome {
    Keep,
    Close,
    Save,
    Test,
    Delete,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut editor) = app.editor.take() else {
        return;
    };
    let theme = app.theme;
    let can_save_passwords = app.secrets.available();
    let mut outcome = Outcome::Keep;

    let modal = egui::Modal::new(egui::Id::unique("connection_editor"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(
                RichText::new(if editor.is_new {
                    "New connection"
                } else {
                    "Edit connection"
                })
                .size(20.0)
                .strong(),
            );
            ui.add_space(4.0);

            super::field_label(ui, &theme, "Database type");
            ui.horizontal_wrapped(|ui| {
                for kind in DbKind::ALL {
                    let selected = editor.draft.kind == kind;
                    let text = if selected {
                        RichText::new(kind.label()).color(theme.on_accent).strong()
                    } else {
                        RichText::new(kind.label())
                    };
                    let mut button = egui::Button::new(text);
                    if selected {
                        button = button.fill(theme.accent);
                    }
                    if ui.add(button).clicked() && !selected {
                        editor.draft.set_kind(kind);
                        editor.port = editor.draft.port.to_string();
                        editor.test = None;
                    }
                }
            });
            ui.add_space(4.0);

            let draft = &mut editor.draft;
            super::field(
                ui,
                &theme,
                "Name (optional)",
                &mut draft.name,
                "For example: Shop, production",
            );

            if draft.kind.is_file() {
                super::field_label(ui, &theme, draft.kind.database_label());
                ui.horizontal(|ui| {
                    let browse = ui.button("Browse…");
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.database)
                            .hint_text("Path of the .db file")
                            .desired_width(f32::INFINITY),
                    );
                    if browse.clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        draft.database = path.display().to_string();
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(390.0);
                        super::field(ui, &theme, "Host", &mut draft.host, "localhost");
                    });
                    ui.vertical(|ui| {
                        super::field_label(ui, &theme, "Port");
                        let port = ui.add(
                            egui::TextEdit::singleline(&mut editor.port)
                                .desired_width(f32::INFINITY),
                        );
                        if port.changed() {
                            editor.port.retain(|c| c.is_ascii_digit());
                            draft.port = editor.port.parse().unwrap_or(0);
                        }
                    });
                });
                super::field(
                    ui,
                    &theme,
                    draft.kind.database_label(),
                    &mut draft.database,
                    "",
                );
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(250.0);
                        super::field(ui, &theme, "User name", &mut draft.username, "");
                    });
                    ui.vertical(|ui| {
                        super::field_label(ui, &theme, "Password");
                        ui.add(
                            egui::TextEdit::singleline(&mut editor.password)
                                .password(true)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
                ui.add_enabled_ui(can_save_passwords, |ui| {
                    ui.checkbox(
                        &mut draft.save_password,
                        "Remember the password on this computer",
                    )
                    .on_hover_text(
                        "Kept in the system's credential store (Windows Credential Manager or \
                     the macOS Keychain), never in a file of dbpadu.",
                    )
                    .on_disabled_hover_text("This system has no credential store.");
                });
            }

            ui.collapsing("More settings", |ui| {
                super::field(
                    ui,
                    &theme,
                    "Extra driver settings",
                    &mut draft.params,
                    "For example: sslmode=require",
                );
                super::field_label(ui, &theme, "Address the engine connects to");
                ui.add(
                    egui::Label::new(
                        RichText::new(draft.jdbc_url())
                            .monospace()
                            .small()
                            .color(theme.text_dim),
                    )
                    .wrap(),
                );
            });

            if editor.testing {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Trying to connect…").color(theme.text_dim));
                });
            } else if let Some(test) = &editor.test {
                let (text, color) = match test {
                    Ok(text) => (text.as_str(), theme.ok),
                    Err(text) => (text.as_str(), theme.danger),
                };
                ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
            }
            let problem = draft.problem();
            if let Some(problem) = problem {
                ui.label(RichText::new(problem).small().color(theme.text_dim));
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        problem.is_none() && !editor.testing,
                        egui::Button::new("Test connection"),
                    )
                    .clicked()
                {
                    outcome = Outcome::Test;
                }
                if !editor.is_new {
                    if editor.confirm_delete {
                        if ui
                            .button(RichText::new("Yes, delete it").color(theme.danger))
                            .clicked()
                        {
                            outcome = Outcome::Delete;
                        }
                    } else if ui.button("Delete").clicked() {
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
        Outcome::Keep => app.editor = Some(editor),
        Outcome::Close => {}
        Outcome::Test => {
            let (draft, password) = (editor.draft.clone(), editor.password.clone());
            app.editor = Some(editor);
            app.test_connection(&draft, &password);
        }
        Outcome::Delete => app.delete_connection(editor.draft.id),
        Outcome::Save => {
            let ConnectionEditor {
                mut draft,
                password,
                ..
            } = editor;
            let id = draft.id;
            if draft.save_password && !password.is_empty() {
                if let Err(err) = app.secrets.set(id, &password) {
                    draft.save_password = false;
                    app.notify(format!("The password could not be remembered: {err}"));
                }
            } else {
                draft.save_password = false;
                app.secrets.delete(id);
            }
            if !password.is_empty() {
                app.keep_password(id, password);
            }
            app.data.upsert(draft);
            app.save();
        }
    }
}

/// Asks for the password of a connection that has none remembered.
pub fn password_prompt(app: &mut App, ctx: &egui::Context) {
    let Some(mut prompt) = app.prompt.take() else {
        return;
    };
    let Some(connection) = app.data.connection(prompt.connection).cloned() else {
        return;
    };
    let theme = app.theme;
    let can_save = app.secrets.available();
    let (mut connect, mut cancel) = (false, false);
    let modal = egui::Modal::new(egui::Id::unique("password_prompt"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new("Password").size(18.0).strong());
            ui.label(
                RichText::new(format!(
                    "for {} on {}",
                    connection.username.trim(),
                    connection.display_name()
                ))
                .color(theme.text_dim),
            );
            let field = ui.add(
                egui::TextEdit::singleline(&mut prompt.password)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
            if field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                connect = true;
            } else {
                field.request_focus();
            }
            ui.add_enabled_ui(can_save, |ui| {
                ui.checkbox(&mut prompt.remember, "Remember it on this computer");
            });
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(super::primary_button(&theme, "Connect")).clicked() {
                        connect = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if connect {
        let PasswordPrompt {
            connection: id,
            password,
            remember,
        } = prompt;
        if remember {
            match app.secrets.set(id, &password) {
                Ok(()) => {
                    let mut saved = connection;
                    saved.save_password = true;
                    app.data.upsert(saved);
                    app.save();
                }
                Err(err) => app.notify(format!("The password could not be remembered: {err}")),
            }
        }
        app.connect_with(id, password);
    } else if !(cancel || modal.should_close()) {
        app.prompt = Some(prompt);
    }
}
