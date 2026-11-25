//! The first page: every saved connection as a card, filed in groups.

use eframe::egui::{self, RichText};
use uuid::Uuid;

use dbpadu_core::model::{
    Connection, GROUP_COLORS, Group, group_label, group_name, group_parent, in_group,
};

use super::editor::{ConnectionEditor, dialog_frame};
use crate::app::{App, Link, LinkState};

const CARD_INNER_WIDTH: f32 = 250.0;
/// How far what is inside a group is moved to the right.
const INDENT: f32 = 22.0;

/// What a click on the page asks for. Collected while drawing, done after.
enum Action {
    Connect(Uuid),
    NewQuery(Uuid),
    Disconnect(Uuid),
    Edit(Uuid),
    NewConnection(String),
    NewGroup(Option<String>),
    ToggleGroup(String),
    RenameGroup(String),
    DeleteGroup(String),
    /// `None` takes the color of the group it is inside.
    ColorGroup(String, Option<String>),
}

/// The box that names a new group or renames one.
pub struct GroupDialog {
    /// The group to rename; `None` makes a new one.
    rename: Option<String>,
    /// The group a new one goes inside.
    parent: Option<String>,
    name: String,
    error: Option<&'static str>,
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            super::page_title(
                ui,
                &theme,
                "Connections",
                "One card for each database. Click Connect to start working.",
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(super::primary_button(&theme, "New connection"))
                .clicked()
            {
                actions.push(Action::NewConnection(String::new()));
            }
            if ui
                .button("New group")
                .on_hover_text("Groups keep connections together, such as Company1 › Production")
                .clicked()
            {
                actions.push(Action::NewGroup(None));
            }
            if app.data.connections.len() > 3 {
                ui.add(
                    egui::TextEdit::singleline(&mut app.search)
                        .hint_text("Search")
                        .desired_width(180.0),
                );
            }
        });
    });
    ui.add_space(12.0);

    if app.data.connections.is_empty() && app.data.groups.is_empty() {
        empty(app, ui);
    } else {
        let needle = app.search.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("home_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let found = app
                    .data
                    .connections
                    .iter()
                    .any(|connection| matches(connection, &needle));
                if !needle.is_empty() && !found {
                    ui.label(
                        RichText::new("No connection matches the search.").color(theme.text_dim),
                    );
                    return;
                }
                groups(app, ui, None, 0, &needle, &mut actions);
                let loose = cards_of(app, "", &needle);
                if !loose.is_empty() {
                    if !app.data.groups.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Without a group").color(theme.text_dim));
                    }
                    cards(app, ui, &loose, 0, &mut actions);
                }
            });
    }

    for action in actions {
        match action {
            Action::Connect(id) => app.connect(id),
            Action::NewQuery(id) => {
                app.open_tab(id, String::new(), None);
            }
            Action::Disconnect(id) => app.disconnect(id),
            Action::Edit(id) => app.edit_connection(id),
            Action::NewConnection(group) => app.editor = Some(ConnectionEditor::new(&group)),
            Action::NewGroup(parent) => {
                app.group_dialog = Some(GroupDialog {
                    rename: None,
                    parent,
                    name: String::new(),
                    error: None,
                });
            }
            Action::RenameGroup(path) => {
                app.group_dialog = Some(GroupDialog {
                    name: group_name(&path).to_string(),
                    rename: Some(path),
                    parent: None,
                    error: None,
                });
            }
            Action::ToggleGroup(path) => {
                if let Some(group) = app.data.group_mut(&path) {
                    group.collapsed = !group.collapsed;
                    app.save();
                }
            }
            Action::DeleteGroup(path) => {
                app.data.delete_group(&path);
                app.save();
            }
            Action::ColorGroup(path, color) => {
                if let Some(group) = app.data.group_mut(&path) {
                    group.color = color;
                    app.save();
                }
            }
        }
    }
}

fn matches(connection: &Connection, needle: &str) -> bool {
    needle.is_empty()
        || connection.display_name().to_lowercase().contains(needle)
        || connection.target().to_lowercase().contains(needle)
        || group_label(&connection.group)
            .to_lowercase()
            .contains(needle)
}

/// The connections filed right under `group` that the search lets through.
fn cards_of<'a>(app: &'a App, group: &str, needle: &str) -> Vec<&'a Connection> {
    let mut shown: Vec<&Connection> = app
        .data
        .connections
        .iter()
        .filter(|connection| connection.group == group && matches(connection, needle))
        .collect();
    shown.sort_by_key(|connection| connection.display_name().to_lowercase());
    shown
}

fn cards(
    app: &App,
    ui: &mut egui::Ui,
    shown: &[&Connection],
    depth: usize,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * INDENT);
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                for connection in shown {
                    card(ui, app, connection, actions);
                }
            });
        });
    });
    ui.add_space(6.0);
}

/// The groups inside `parent` (the top ones for `None`), each with its
/// cards and the groups inside it.
fn groups(
    app: &App,
    ui: &mut egui::Ui,
    parent: Option<&str>,
    depth: usize,
    needle: &str,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    for group in &app.data.groups {
        if group_parent(&group.path) != parent {
            continue;
        }
        let inside = app
            .data
            .connections
            .iter()
            .filter(|connection| in_group(&connection.group, &group.path))
            .filter(|connection| matches(connection, needle))
            .count();
        if !needle.is_empty() && inside == 0 {
            continue;
        }
        // While searching, what was found is shown even in a closed group.
        let open = !group.collapsed || !needle.is_empty();
        group_header(app, ui, group, depth, open, inside, actions);
        if !open {
            continue;
        }
        let shown = cards_of(app, &group.path, needle);
        if !shown.is_empty() {
            cards(app, ui, &shown, depth + 1, actions);
        } else if inside == 0 {
            ui.horizontal(|ui| {
                ui.add_space((depth + 1) as f32 * INDENT);
                ui.label(
                    RichText::new("No connections in this group yet.")
                        .small()
                        .color(theme.text_dim),
                );
            });
        }
        groups(app, ui, Some(&group.path), depth + 1, needle, actions);
    }
}

/// The line that starts a group: arrow, color, name, how many, and what
/// can be done with it.
fn group_header(
    app: &App,
    ui: &mut egui::Ui,
    group: &Group,
    depth: usize,
    open: bool,
    inside: usize,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    let color = app.data.group_color(&group.path).and_then(super::hex_color);
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * INDENT);
        let arrow = if open { "⏷" } else { "⏵" };
        if ui
            .add(egui::Button::new(RichText::new(arrow).color(theme.text_dim)).frame(false))
            .on_hover_text(if open {
                "Close the group"
            } else {
                "Open the group"
            })
            .clicked()
        {
            actions.push(Action::ToggleGroup(group.path.clone()));
        }

        // The color: a square that opens the choice of colors.
        let (rect, swatch) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
        match color {
            Some(color) => {
                ui.painter().rect_filled(rect, 4.0, color);
            }
            None => {
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.5, theme.text_dim),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let swatch = swatch
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Choose the color of this group");
        egui::Popup::menu(&swatch).show(|ui| {
            for (name, hex) in GROUP_COLORS {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    if let Some(color) = super::hex_color(hex) {
                        ui.painter().rect_filled(rect, 3.0, color);
                    }
                    let chosen = group.color.as_deref() == Some(hex);
                    if ui.selectable_label(chosen, name).clicked() {
                        actions.push(Action::ColorGroup(
                            group.path.clone(),
                            Some(hex.to_string()),
                        ));
                        ui.close();
                    }
                });
            }
            let inherit = if group_parent(&group.path).is_some() {
                "Same as the group above"
            } else {
                "No color"
            };
            if ui
                .selectable_label(group.color.is_none(), inherit)
                .clicked()
            {
                actions.push(Action::ColorGroup(group.path.clone(), None));
                ui.close();
            }
        });

        let name = ui
            .add(
                egui::Label::new(RichText::new(group_name(&group.path)).size(16.0).strong())
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(group_label(&group.path));
        if name.clicked() {
            actions.push(Action::ToggleGroup(group.path.clone()));
        }
        ui.label(
            RichText::new(super::count(inside, "connection", "connections"))
                .small()
                .color(theme.text_dim),
        );
        if ui
            .small_button("+ Connection")
            .on_hover_text("New connection in this group")
            .clicked()
        {
            actions.push(Action::NewConnection(group.path.clone()));
        }
        ui.menu_button("⋯", |ui| {
            if ui.button("New group inside").clicked() {
                actions.push(Action::NewGroup(Some(group.path.clone())));
                ui.close();
            }
            if ui.button("Rename…").clicked() {
                actions.push(Action::RenameGroup(group.path.clone()));
                ui.close();
            }
            if ui
                .button("Delete group")
                .on_hover_text(
                    "Its connections are kept: they move to the group above, or out of any group.",
                )
                .clicked()
            {
                actions.push(Action::DeleteGroup(group.path.clone()));
                ui.close();
            }
        });
    });
}

/// Names a new group, or renames one.
pub fn group_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut dialog) = app.group_dialog.take() else {
        return;
    };
    let theme = app.theme;
    let (mut done, mut cancel) = (false, false);
    let modal = egui::Modal::new(egui::Id::unique("group_dialog"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            let title = match (&dialog.rename, &dialog.parent) {
                (Some(_), _) => "Rename group".to_string(),
                (None, Some(parent)) => format!("New group in {}", group_label(parent)),
                (None, None) => "New group".to_string(),
            };
            ui.label(RichText::new(title).size(18.0).strong());
            let hint = if dialog.rename.is_some() || dialog.parent.is_some() {
                "For example: Production"
            } else {
                "For example: Company1, or Company1 / Production"
            };
            let field = ui.add(
                egui::TextEdit::singleline(&mut dialog.name)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            );
            if field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                done = true;
            } else {
                field.request_focus();
            }
            if let Some(error) = dialog.error {
                ui.label(RichText::new(error).small().color(theme.danger));
            }
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if dialog.rename.is_some() {
                        "Rename"
                    } else {
                        "Add group"
                    };
                    if ui.add(super::primary_button(&theme, label)).clicked() {
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if done {
        let result = match &dialog.rename {
            Some(path) => app.data.rename_group(path, &dialog.name).map(|_| ()),
            None => {
                let path = match &dialog.parent {
                    Some(parent) => format!("{parent}/{}", dialog.name),
                    None => dialog.name.clone(),
                };
                match app.data.add_group(&path) {
                    Some(_) => Ok(()),
                    None => Err("Enter a name."),
                }
            }
        };
        match result {
            Ok(()) => app.save(),
            Err(error) => {
                dialog.error = Some(error);
                app.group_dialog = Some(dialog);
            }
        }
    } else if !(cancel || modal.should_close()) {
        app.group_dialog = Some(dialog);
    }
}

/// Shown instead of cards while nothing is saved yet.
fn empty(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("No connections yet").size(20.0).strong());
        ui.label(
            RichText::new(
                "Add your first database. You only need its address, your user name and password.",
            )
            .color(theme.text_dim),
        );
        ui.add_space(8.0);
        if ui
            .add(super::primary_button(&theme, "Add a connection"))
            .clicked()
        {
            app.editor = Some(ConnectionEditor::new(""));
        }
    });
}

fn card(ui: &mut egui::Ui, app: &App, connection: &Connection, actions: &mut Vec<Action>) {
    let theme = app.theme;
    let id = connection.id;
    let link = app.links.get(&id);
    let mut frame = super::card(&theme);
    if link.is_some_and(Link::connected) {
        frame = frame.stroke(egui::Stroke::new(1.5, theme.ok));
    }
    let shown = frame.show(ui, |ui| {
        ui.set_width(CARD_INNER_WIDTH);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                super::kind_badge(ui, connection.kind, 34.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add(
                        egui::Label::new(
                            RichText::new(connection.display_name()).size(15.0).strong(),
                        )
                        .truncate(),
                    );
                    let mut kind = connection.kind.label().to_string();
                    if connection.uses_ssh() {
                        kind.push_str(" · through SSH");
                    }
                    ui.label(RichText::new(kind).small().color(theme.text_dim));
                });
            });
            ui.add(
                egui::Label::new(
                    RichText::new(connection.target())
                        .monospace()
                        .small()
                        .color(theme.text_dim),
                )
                .truncate(),
            );
            ui.horizontal(|ui| {
                super::dot(ui, super::link_color(&theme, link));
                let (text, hover) = match link.map(|link| &link.state) {
                    None => ("Not connected".to_string(), None),
                    Some(LinkState::Connecting) => ("Connecting…".to_string(), None),
                    Some(LinkState::Connected { product, version }) => (
                        "Connected".to_string(),
                        Some(format!("{product} {version}")),
                    ),
                    Some(LinkState::Failed(error)) => {
                        ("Could not connect".to_string(), Some(error.clone()))
                    }
                };
                let label = ui.add(
                    egui::Label::new(RichText::new(text).small().color(theme.text_dim)).truncate(),
                );
                if let Some(hover) = hover {
                    label.on_hover_text(hover);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Edit").clicked() {
                        actions.push(Action::Edit(id));
                    }
                    if link.is_some() && ui.small_button("Disconnect").clicked() {
                        actions.push(Action::Disconnect(id));
                    }
                });
            });
            if let Some(LinkState::Failed(error)) = link.map(|link| &link.state) {
                ui.add(egui::Label::new(RichText::new(error).small().color(theme.danger)).wrap());
            }
            ui.add_space(2.0);
            let wide = egui::vec2(ui.available_width(), 28.0);
            match link.map(|link| &link.state) {
                Some(LinkState::Connected { .. }) => {
                    if ui
                        .add(super::primary_button(&theme, "New query").min_size(wide))
                        .clicked()
                    {
                        actions.push(Action::NewQuery(id));
                    }
                }
                Some(LinkState::Connecting) => {
                    ui.add_enabled(false, egui::Button::new("Connecting…").min_size(wide));
                }
                Some(LinkState::Failed(_)) => {
                    if ui
                        .add(super::primary_button(&theme, "Try again").min_size(wide))
                        .clicked()
                    {
                        actions.push(Action::Connect(id));
                    }
                }
                None => {
                    if ui
                        .add(super::primary_button(&theme, "Connect").min_size(wide))
                        .clicked()
                    {
                        actions.push(Action::Connect(id));
                    }
                }
            }
        });
    });
    // The color of the group, as a bar down the left edge of the card.
    if let Some(color) = app
        .data
        .group_color(&connection.group)
        .and_then(super::hex_color)
    {
        let rect = shown.response.rect;
        let bar = egui::Rect::from_min_max(
            rect.left_top() + egui::vec2(1.0, 8.0),
            egui::pos2(rect.left() + 5.0, rect.bottom() - 8.0),
        );
        ui.painter().rect_filled(bar, 2.0, color);
    }
}
