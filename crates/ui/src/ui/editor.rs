//! The connection form, and the box that asks for a password.

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use uuid::Uuid;

use dbpadu_core::{
    model::{Connection, DbKind, Driver, Jump, SshAuth, group_label},
    secrets::{Secret, Secrets},
};

use crate::{
    app::{App, Load, PasswordPrompt},
    theme::Theme,
};

/// Width of the list of drivers at the left of the form.
const KINDS_WIDTH: f32 = 190.0;
const FORM_WIDTH: f32 = 540.0;
/// The least height of the form, so the dialog does not jump between pages.
const FORM_HEIGHT: f32 = 440.0;

/// The pages of the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Main,
    Ssh,
    Advanced,
}

/// The connection form while it is open.
pub struct ConnectionEditor {
    pub draft: Connection,
    pub password: String,
    /// The SSH password, or the passphrase of the SSH key.
    pub ssh_secret: String,
    /// The same for each jump server, by its id.
    pub jump_secrets: HashMap<Uuid, String>,
    port: String,
    ssh_port: String,
    /// The port of each jump server as typed, by its id.
    jump_ports: HashMap<Uuid, String>,
    is_new: bool,
    page: Page,
    /// The result of "Test connection": what answered, or why it failed.
    pub test: Option<Result<String, String>>,
    pub testing: bool,
    confirm_delete: bool,
}

impl ConnectionEditor {
    /// A new connection, filed under `group` (empty for none).
    pub fn new(group: &str) -> Self {
        let draft = Connection {
            group: group.to_string(),
            ..Connection::new(DbKind::default())
        };
        Self::with(draft, String::new(), String::new(), HashMap::new(), true)
    }

    /// `known` is what was typed for it earlier in this run of the app:
    /// the database password and the SSH password; `jump_secrets` is the
    /// same for its jump servers.
    pub fn edit(
        connection: &Connection,
        secrets: &Secrets,
        known: (String, String),
        mut jump_secrets: HashMap<Uuid, String>,
    ) -> Self {
        let (mut password, mut ssh_secret) = known;
        if connection.save_password {
            if password.is_empty() {
                password = secrets
                    .get(connection.id, Secret::Password)
                    .unwrap_or_default();
            }
            if ssh_secret.is_empty() {
                ssh_secret = secrets.get(connection.id, Secret::Ssh).unwrap_or_default();
            }
            for jump in &connection.ssh.jumps {
                if !jump_secrets.contains_key(&jump.id)
                    && let Some(secret) = secrets.get(connection.id, Secret::Jump(jump.id))
                {
                    jump_secrets.insert(jump.id, secret);
                }
            }
        }
        Self::with(
            connection.clone(),
            password,
            ssh_secret,
            jump_secrets,
            false,
        )
    }

    fn with(
        draft: Connection,
        password: String,
        ssh_secret: String,
        jump_secrets: HashMap<Uuid, String>,
        is_new: bool,
    ) -> Self {
        Self {
            port: draft.port.to_string(),
            ssh_port: draft.ssh.port.to_string(),
            jump_ports: HashMap::new(),
            draft,
            password,
            ssh_secret,
            jump_secrets,
            is_new,
            page: Page::Main,
            test: None,
            testing: false,
            confirm_delete: false,
        }
    }
}

/// The frame of a dialog: a window with room around what is in it.
pub fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(18))
}

enum Outcome {
    Keep,
    Close,
    Save,
    Test,
    Delete,
    /// Open the driver form on a new driver, on top of this form.
    NewDriver,
}

/// One driver in the list at the left of the form: its name, and under it
/// its version.
fn driver_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    version: &str,
    selected: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect);
    if selected {
        painter.rect_filled(rect, 6.0, theme.accent);
    } else if response.hovered() {
        painter.rect_filled(rect, 6.0, theme.border.gamma_multiply(0.6));
    }
    let (text, dim) = if selected {
        (theme.on_accent, theme.on_accent.gamma_multiply(0.8))
    } else {
        (theme.text, theme.text_dim)
    };
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.top() + 12.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(14.0),
        text,
    );
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.top() + 29.0),
        egui::Align2::LEFT_CENTER,
        version,
        egui::FontId::proportional(11.0),
        dim,
    );
    response.on_hover_text(format!("{name} · {version}"))
}

/// The list of drivers at the left of the form: the ones inside dbpadu,
/// then the user's own. Returns the one that was clicked.
fn driver_list(
    app: &App,
    ui: &mut egui::Ui,
    draft: &Connection,
    drivers: &[Driver],
) -> Option<(DbKind, Option<Driver>)> {
    let theme = app.theme;
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .id_salt("connection_drivers")
        .max_height(FORM_HEIGHT - 30.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for kind in DbKind::ALL {
                let version = match app.bundled_driver(kind) {
                    Some(driver) => format!("built in · {}", driver.version),
                    None => "built in".to_string(),
                };
                let selected = draft.kind == kind && draft.driver.is_none();
                if driver_row(ui, &theme, kind.label(), &version, selected).clicked() && !selected {
                    chosen = Some((kind, None));
                }
            }
            if !drivers.is_empty() {
                ui.add_space(6.0);
                super::field_label(ui, &theme, "Your drivers");
            }
            for driver in drivers {
                let version = match app.driver_version(driver) {
                    version if version.is_empty() => driver.kind.label().to_string(),
                    version => format!("{} · {version}", driver.kind.label()),
                };
                let selected = draft.driver == Some(driver.id);
                if driver_row(ui, &theme, &driver.name, &version, selected).clicked() && !selected {
                    chosen = Some((driver.kind, Some(driver.clone())));
                }
            }
        });
    chosen
}

/// Name, class and version of the driver the connection will use.
fn driver_box(app: &App, ui: &mut egui::Ui, kind: DbKind, own: Option<&Driver>) {
    let theme = app.theme;
    let (name, package, version, note) = if let Some(own) = own {
        let class = match app.driver_checks.get(&own.id) {
            Some(Load::Ready(info)) => info.class_name.clone(),
            Some(Load::Failed(_)) => "could not be read".to_string(),
            _ => own.class_name.trim().to_string(),
        };
        (
            own.name.clone(),
            class,
            app.driver_version(own),
            "Your own driver. Change it on the Drivers page.",
        )
    } else if let Some(driver) = app.bundled_driver(kind) {
        (
            driver.name.clone(),
            driver.package.clone(),
            driver.version.clone(),
            "Inside dbpadu. For another version, add a driver of your own.",
        )
    } else {
        (
            "…".to_string(),
            String::new(),
            String::new(),
            "Shown when the engine is running",
        )
    };
    let second = if own.is_some() { "Class" } else { "Package" };
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(22.0, 4.0);
                for (label, value) in [
                    ("JDBC driver", name),
                    (second, package),
                    ("Version", version),
                ] {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new(label).small().color(theme.text_dim));
                        let value = if value.is_empty() { "…" } else { &value };
                        ui.label(RichText::new(value).size(13.0));
                    });
                }
            });
        })
        .response
        .on_hover_text(note);
}

/// A port field: digits only, kept as text while it is typed.
fn port_field(ui: &mut egui::Ui, text: &mut String, port: &mut u16) {
    let field = ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
    if field.changed() {
        text.retain(|c| c.is_ascii_digit());
        *port = text.parse().unwrap_or(0);
    }
}

fn remember(ui: &mut egui::Ui, draft: &mut Connection, can_save: bool) {
    ui.add_enabled_ui(can_save, |ui| {
        ui.checkbox(
            &mut draft.save_password,
            "Remember the passwords on this computer",
        )
        .on_hover_text(
            "The database password and the SSH passwords are kept in the system's credential \
             store (Windows Credential Manager or the macOS Keychain), never in a file of dbpadu.",
        )
        .on_disabled_hover_text("This system has no credential store.");
    });
}

fn main_page(
    ui: &mut egui::Ui,
    theme: &Theme,
    editor: &mut ConnectionEditor,
    groups: &[String],
    can_save: bool,
    with_host: bool,
) {
    let draft = &mut editor.draft;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(290.0);
            super::field(
                ui,
                theme,
                "Name (optional)",
                &mut draft.name,
                "For example: prod1",
            );
        });
        ui.vertical(|ui| {
            super::field_label(ui, theme, "Group (optional)");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!groups.is_empty(), |ui| {
                    ui.menu_button("Choose", |ui| {
                        if ui.button("No group").clicked() {
                            draft.group.clear();
                            ui.close();
                        }
                        for group in groups {
                            if ui.button(group_label(group)).clicked() {
                                draft.group = group.clone();
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_disabled_hover_text("There are no groups yet. Type a name to make one.");
                });
                ui.add(
                    egui::TextEdit::singleline(&mut draft.group)
                        .hint_text("Company1 / Production")
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text("A group inside a group is written with / between the names.");
            });
        });
    });

    if draft.kind.is_file() {
        super::field_label(ui, theme, draft.kind.database_label());
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
        return;
    }
    // A driver whose address has no host, such as one for a database in a
    // file, is not asked for one.
    if with_host {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(410.0);
                super::field(ui, theme, "Host", &mut draft.host, "localhost");
            });
            ui.vertical(|ui| {
                super::field_label(ui, theme, "Port");
                port_field(ui, &mut editor.port, &mut draft.port);
            });
        });
    }
    super::field(
        ui,
        theme,
        draft.kind.database_label(),
        &mut draft.database,
        "",
    );
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(260.0);
            super::field(ui, theme, "User name", &mut draft.username, "");
        });
        ui.vertical(|ui| {
            super::field_label(ui, theme, "Password");
            ui.add(
                egui::TextEdit::singleline(&mut editor.password)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
        });
    });
    remember(ui, draft, can_save);
}

/// The fields to sign in at an SSH server: `secret` is the password, or
/// the passphrase of the key.
fn sign_in_fields(
    ui: &mut egui::Ui,
    theme: &Theme,
    auth: &mut SshAuth,
    key_file: &mut String,
    secret: &mut String,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Sign in with").small().color(theme.text_dim));
        ui.radio_value(auth, SshAuth::Password, "Password");
        ui.radio_value(auth, SshAuth::Key, "Private key file");
    });
    match auth {
        SshAuth::Password => {
            super::field_label(ui, theme, "SSH password");
        }
        SshAuth::Key => {
            super::field_label(ui, theme, "Private key file");
            ui.horizontal(|ui| {
                let browse = ui.button("Browse…");
                ui.add(
                    egui::TextEdit::singleline(key_file)
                        .hint_text("For example: ~/.ssh/id_ed25519")
                        .desired_width(f32::INFINITY),
                );
                if browse.clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_file()
                {
                    *key_file = path.display().to_string();
                }
            });
            super::field_label(ui, theme, "Passphrase of the key (empty if it has none)");
        }
    }
    ui.add(
        egui::TextEdit::singleline(secret)
            .password(true)
            .desired_width(f32::INFINITY),
    );
}

/// The name of an SSH server in the route, or `fallback` while it has none.
fn hop_name(host: &str, port: u16, fallback: &str) -> String {
    match host.trim() {
        "" => fallback.to_string(),
        host if port == 22 => host.to_string(),
        host => format!("{host}:{port}"),
    }
}

/// What to do with a jump server in the list.
enum JumpAction {
    Remove(usize),
    /// Swap with the one before it.
    Earlier(usize),
}

fn ssh_page(ui: &mut egui::Ui, theme: &Theme, editor: &mut ConnectionEditor, can_save: bool) {
    let draft = &mut editor.draft;
    ui.checkbox(&mut draft.ssh.enabled, "Connect through an SSH server")
        .on_hover_text(
            "For a database that is only reachable from another computer. dbpadu signs in to \
             that computer with SSH and reaches the database from there.",
        );
    if !draft.ssh.enabled {
        ui.label(
            RichText::new(
                "Switch this on when the database cannot be reached from this computer directly.",
            )
            .small()
            .color(theme.text_dim),
        );
        return;
    }
    // With jump servers the page is longer than the form.
    egui::ScrollArea::vertical()
        .max_height(FORM_HEIGHT - 110.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // Room for the scroll bar.
            ui.set_width(ui.available_width() - 12.0);
            ssh_fields(ui, theme, editor, can_save);
        });
}

fn ssh_fields(ui: &mut egui::Ui, theme: &Theme, editor: &mut ConnectionEditor, can_save: bool) {
    let draft = &mut editor.draft;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(390.0);
            super::field(
                ui,
                theme,
                "SSH host",
                &mut draft.ssh.host,
                "For example: db-gateway.example.com",
            );
        });
        ui.vertical(|ui| {
            super::field_label(ui, theme, "SSH port");
            port_field(ui, &mut editor.ssh_port, &mut draft.ssh.port);
        });
    });
    super::field(ui, theme, "SSH user name", &mut draft.ssh.username, "");
    sign_in_fields(
        ui,
        theme,
        &mut draft.ssh.auth,
        &mut draft.ssh.key_file,
        &mut editor.ssh_secret,
    );
    remember(ui, draft, can_save);

    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Jump servers").strong());
    ui.add(
        egui::Label::new(
            RichText::new(
                "For an SSH server that this computer cannot reach directly. dbpadu signs in to \
                 each jump server in turn and reaches the next one from there, like ProxyJump \
                 in OpenSSH.",
            )
            .small()
            .color(theme.text_dim),
        )
        .wrap(),
    );
    let count = draft.ssh.jumps.len();
    let mut action = None;
    for (index, jump) in draft.ssh.jumps.iter_mut().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Jump server {}", index + 1)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Remove").clicked() {
                        action = Some(JumpAction::Remove(index));
                    }
                    if count > 1 {
                        if ui
                            .add_enabled(index + 1 < count, egui::Button::new("↓").small())
                            .on_hover_text("Pass through this one later")
                            .clicked()
                        {
                            action = Some(JumpAction::Earlier(index + 1));
                        }
                        if ui
                            .add_enabled(index > 0, egui::Button::new("↑").small())
                            .on_hover_text("Pass through this one earlier")
                            .clicked()
                        {
                            action = Some(JumpAction::Earlier(index));
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(370.0);
                    super::field(
                        ui,
                        theme,
                        "Host",
                        &mut jump.host,
                        "For example: bastion.example.com",
                    );
                });
                ui.vertical(|ui| {
                    super::field_label(ui, theme, "Port");
                    let text = editor
                        .jump_ports
                        .entry(jump.id)
                        .or_insert_with(|| jump.port.to_string());
                    port_field(ui, text, &mut jump.port);
                });
            });
            super::field(ui, theme, "User name", &mut jump.username, "");
            ui.push_id(jump.id, |ui| {
                sign_in_fields(
                    ui,
                    theme,
                    &mut jump.auth,
                    &mut jump.key_file,
                    editor.jump_secrets.entry(jump.id).or_default(),
                );
            });
        });
    }
    match action {
        Some(JumpAction::Remove(index)) => {
            let jump = draft.ssh.jumps.remove(index);
            editor.jump_secrets.remove(&jump.id);
            editor.jump_ports.remove(&jump.id);
        }
        Some(JumpAction::Earlier(index)) => draft.ssh.jumps.swap(index - 1, index),
        None => {}
    }
    if ui
        .button("Add a jump server")
        .on_hover_text(
            "The first jump server is the one this computer reaches; the last one reaches the \
             SSH server above.",
        )
        .clicked()
    {
        draft.ssh.jumps.push(Jump::default());
    }

    // The way to the database, one server after the other.
    let mut route = vec!["this computer".to_string()];
    for (index, jump) in draft.ssh.jumps.iter().enumerate() {
        let fallback = format!("jump server {}", index + 1);
        route.push(hop_name(&jump.host, jump.port, &fallback));
    }
    route.push(hop_name(&draft.ssh.host, draft.ssh.port, "SSH server"));
    route.push(format!("{}:{}", draft.host.trim(), draft.port));
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(
            RichText::new(format!(
                "Route: {}. The database's host and port on the Main page are as the last SSH \
                 server sees them. Each server's key is remembered the first time; if it ever \
                 changes, dbpadu refuses to connect.",
                route.join(" → ")
            ))
            .small()
            .color(theme.text_dim),
        )
        .wrap(),
    );
}

fn advanced_page(ui: &mut egui::Ui, theme: &Theme, draft: &mut Connection, own: Option<&Driver>) {
    super::field(
        ui,
        theme,
        "Extra driver settings",
        &mut draft.params,
        "For example: sslmode=require",
    );
    super::field_label(ui, theme, "Address the engine connects to");
    let url = if draft.uses_ssh() {
        draft
            .jdbc_url_through_tunnel(own)
            .replace(dbpadu_core::model::TUNNEL_PORT, "<tunnel port>")
    } else {
        draft.jdbc_url(own)
    };
    ui.add(
        egui::Label::new(
            RichText::new(url)
                .monospace()
                .size(12.0)
                .color(theme.text_dim),
        )
        .wrap(),
    );
    if let Some(own) = own {
        let properties = own.set_properties();
        if !properties.is_empty() {
            super::field_label(ui, theme, "Connection properties of the driver");
            let text = properties
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("  ·  ");
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .monospace()
                        .size(12.0)
                        .color(theme.text_dim),
                )
                .wrap(),
            );
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut editor) = app.editor.take() else {
        return;
    };
    let theme = app.theme;
    let can_save_passwords = app.secrets.available();
    let groups: Vec<String> = app
        .data
        .groups
        .iter()
        .map(|group| group.path.clone())
        .collect();
    let drivers = app.data.drivers.clone();
    let mut outcome = Outcome::Keep;

    let modal = egui::Modal::new(egui::Id::unique("connection_editor"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
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

            ui.horizontal_top(|ui| {
                // The user's own driver of the connection, if it has one.
                let own = editor
                    .draft
                    .driver
                    .and_then(|id| drivers.iter().find(|driver| driver.id == id))
                    .cloned();
                // At the left: the drivers, one under the other.
                ui.vertical(|ui| {
                    ui.set_width(KINDS_WIDTH);
                    super::field_label(ui, &theme, "Database driver");
                    if let Some((kind, driver)) = driver_list(app, ui, &editor.draft, &drivers) {
                        editor.draft.set_driver(own.as_ref(), kind, driver.as_ref());
                        editor.port = editor.draft.port.to_string();
                        editor.test = None;
                        if kind.is_file() && editor.page == Page::Ssh {
                            editor.page = Page::Main;
                        }
                    }
                    ui.add_space(4.0);
                    if ui
                        .button("New driver…")
                        .on_hover_text(
                            "Add a JDBC driver of your own: another version, such as MySQL 5, \
                             or another database.",
                        )
                        .clicked()
                    {
                        outcome = Outcome::NewDriver;
                    }
                });
                // The click above may have changed it.
                let own = editor
                    .draft
                    .driver
                    .and_then(|id| drivers.iter().find(|driver| driver.id == id))
                    .cloned();
                let own = own.as_ref();
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.set_width(FORM_WIDTH);
                    ui.set_min_height(FORM_HEIGHT);
                    driver_box(app, ui, editor.draft.kind, own);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut editor.page, Page::Main, "Main");
                        if !editor.draft.kind.is_file() {
                            let ssh = if editor.draft.ssh.enabled {
                                "SSH · on"
                            } else {
                                "SSH"
                            };
                            ui.selectable_value(&mut editor.page, Page::Ssh, ssh);
                        }
                        ui.selectable_value(&mut editor.page, Page::Advanced, "Advanced");
                    });
                    let with_host = own.is_none_or(|driver| {
                        ["{host}", "{server}", "{port}"]
                            .iter()
                            .any(|name| driver.url_template.contains(name))
                    });
                    match editor.page {
                        Page::Main => main_page(
                            ui,
                            &theme,
                            &mut editor,
                            &groups,
                            can_save_passwords,
                            with_host,
                        ),
                        Page::Ssh => ssh_page(ui, &theme, &mut editor, can_save_passwords),
                        Page::Advanced => advanced_page(ui, &theme, &mut editor.draft, own),
                    }

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
                    if let Some(problem) = editor.draft.problem(own) {
                        ui.label(RichText::new(problem).small().color(theme.text_dim));
                    }
                });
            });

            let problem = editor.draft.problem(app.data.driver_of(&editor.draft));
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
        Outcome::NewDriver => {
            app.driver_editor = Some(super::drivers::DriverEditor::new(editor.draft.kind));
            app.editor = Some(editor);
        }
        Outcome::Test => {
            let draft = editor.draft.clone();
            let (password, ssh_secret) = (editor.password.clone(), editor.ssh_secret.clone());
            let jump_secrets = editor.jump_secrets.clone();
            app.editor = Some(editor);
            app.test_connection(&draft, &password, &ssh_secret, &jump_secrets);
        }
        Outcome::Delete => app.delete_connection(editor.draft.id),
        Outcome::Save => {
            let ConnectionEditor {
                mut draft,
                password,
                mut ssh_secret,
                mut jump_secrets,
                ..
            } = editor;
            let id = draft.id;
            if !draft.uses_ssh() {
                ssh_secret.clear();
                jump_secrets.clear();
            }
            // Jump servers that were taken out are forgotten.
            let before: Vec<Uuid> = app
                .data
                .connection(id)
                .map(|old| old.ssh.jumps.iter().map(|jump| jump.id).collect())
                .unwrap_or_default();
            for jump in before {
                if !draft.ssh.jumps.iter().any(|kept| kept.id == jump) {
                    app.secrets.delete(id, Secret::Jump(jump));
                    app.keep_jump_secrets([(jump, String::new())]);
                }
            }
            let jumps: Vec<(Uuid, String)> = draft
                .ssh
                .jumps
                .iter()
                .map(|jump| {
                    let secret = jump_secrets.remove(&jump.id).unwrap_or_default();
                    (jump.id, secret)
                })
                .collect();
            let mut secrets = vec![(Secret::Password, &password), (Secret::Ssh, &ssh_secret)];
            secrets.extend(
                jumps
                    .iter()
                    .map(|(jump, secret)| (Secret::Jump(*jump), secret)),
            );
            if draft.save_password && secrets.iter().any(|(_, value)| !value.is_empty()) {
                for (secret, value) in secrets {
                    if value.is_empty() {
                        app.secrets.delete(id, secret);
                    } else if let Err(err) = app.secrets.set(id, secret, value) {
                        draft.save_password = false;
                        app.notify(format!("The passwords could not be remembered: {err}"));
                        break;
                    }
                }
            } else {
                draft.save_password = false;
                for (secret, _) in secrets {
                    app.secrets.delete(id, secret);
                }
            }
            app.keep_jump_secrets(jumps);
            app.keep_secrets(id, password, ssh_secret);
            app.data.upsert(draft);
            app.save();
        }
    }
}

/// Asks for the passwords of a connection that are not known yet.
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
    let asked = usize::from(prompt.password.is_some())
        + usize::from(prompt.ssh.is_some())
        + prompt.jumps.len();
    let both = asked > 1;
    let modal = egui::Modal::new(egui::Id::unique("password_prompt"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(
                RichText::new(if both { "Passwords" } else { "Password" })
                    .size(18.0)
                    .strong(),
            );
            ui.label(RichText::new(connection.display_name()).color(theme.text_dim));
            let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
            let mut first = true;
            let mut ask = |ui: &mut egui::Ui, label: String, value: &mut String| {
                super::field_label(ui, &theme, &label);
                let field = ui.add(
                    egui::TextEdit::singleline(value)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
                if field.lost_focus() && enter {
                    connect = true;
                }
                // The first field is ready to type in when the box opens.
                if first && ui.memory(|memory| memory.focused().is_none()) {
                    field.request_focus();
                }
                first = false;
            };
            for (id, secret) in &mut prompt.jumps {
                if let Some(jump) = connection.ssh.jumps.iter().find(|jump| jump.id == *id) {
                    let label = format!(
                        "SSH password of {} on jump server {}",
                        jump.username.trim(),
                        jump.host.trim()
                    );
                    ask(ui, label, secret);
                }
            }
            if let Some(ssh) = &mut prompt.ssh {
                let label = format!(
                    "SSH password of {} on {}",
                    connection.ssh.username.trim(),
                    connection.ssh.host.trim()
                );
                ask(ui, label, ssh);
            }
            if let Some(password) = &mut prompt.password {
                let label = format!("Database password of {}", connection.username.trim());
                ask(ui, label, password);
            }
            ui.add_enabled_ui(can_save, |ui| {
                ui.checkbox(&mut prompt.remember, "Remember on this computer");
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
            ssh,
            jumps,
            remember,
        } = prompt;
        if remember {
            let typed = [(Secret::Password, &password), (Secret::Ssh, &ssh)];
            let saved = typed
                .into_iter()
                .try_for_each(|(secret, value)| match value {
                    Some(value) => app.secrets.set(id, secret, value),
                    None => Ok(()),
                })
                .and_then(|()| {
                    jumps.iter().try_for_each(|(jump, value)| {
                        app.secrets.set(id, Secret::Jump(*jump), value)
                    })
                });
            match saved {
                Ok(()) => {
                    let mut saved = connection;
                    saved.save_password = true;
                    app.data.upsert(saved);
                    app.save();
                }
                Err(err) => app.notify(format!("The password could not be remembered: {err}")),
            }
        }
        // What was not asked for is known from before.
        app.keep_jump_secrets(jumps);
        let known = app.known_secrets(id);
        app.connect_with(id, password.unwrap_or(known.0), ssh.unwrap_or(known.1));
    } else if !(cancel || modal.should_close()) {
        app.prompt = Some(prompt);
    }
}
