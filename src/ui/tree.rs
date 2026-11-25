//! The tree on the left: connections, their schemas, tables and columns.

use eframe::egui::{self, Color32, RichText};
use uuid::Uuid;

use dbpadu_core::model::Appearance;

use super::editor::ConnectionEditor;
use crate::{
    app::{App, Link, LinkState, Load, connection_key, schema_key, table_key},
    theme::Theme,
};

/// Width one level of the tree is moved to the right.
const INDENT: f32 = 14.0;
const ROW_HEIGHT: f32 = 24.0;

/// What a click in the tree asks for. Collected while drawing, done after.
enum Action {
    Connect(Uuid),
    Toggle(String),
    OpenSchema(Uuid, String),
    OpenTable(Uuid, String, String),
    /// A new tab with `SELECT * FROM` the table, run straight away.
    SelectRows(Uuid, String, String),
    NewQuery(Uuid),
    Disconnect(Uuid),
    Refresh(Uuid),
    Edit(Uuid),
    Copy(String),
}

/// One line of the tree.
struct Row<'a> {
    depth: usize,
    /// `Some(open)` draws the arrow of a node that can be opened.
    arrow: Option<bool>,
    dot: Option<Color32>,
    text: &'a str,
    /// Dim text after the name, such as a column's type.
    detail: &'a str,
    strong: bool,
    selected: bool,
}

fn row(ui: &mut egui::Ui, theme: &Theme, row: Row) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::click(),
    );
    let painter = ui.painter();
    if row.selected {
        painter.rect_filled(rect, 6.0, theme.accent.gamma_multiply(0.35));
    } else if response.hovered() {
        painter.rect_filled(rect, 6.0, theme.border.gamma_multiply(0.6));
    }
    let mut x = rect.left() + 6.0 + row.depth as f32 * INDENT;
    let y = rect.center().y;
    if let Some(open) = row.arrow {
        let c = egui::pos2(x + 5.0, y);
        let points = if open {
            vec![
                c + egui::vec2(-4.0, -2.5),
                c + egui::vec2(4.0, -2.5),
                c + egui::vec2(0.0, 3.5),
            ]
        } else {
            vec![
                c + egui::vec2(-2.5, -4.0),
                c + egui::vec2(3.5, 0.0),
                c + egui::vec2(-2.5, 4.0),
            ]
        };
        painter.add(egui::Shape::convex_polygon(
            points,
            theme.text_dim,
            egui::Stroke::NONE,
        ));
    }
    x += 16.0;
    if let Some(color) = row.dot {
        painter.circle_filled(egui::pos2(x + 4.0, y), 4.0, color);
        x += 14.0;
    }
    // The name and its detail, cut short with "…" when they do not fit.
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    job.append(
        row.text,
        0.0,
        egui::text::TextFormat::simple(
            font.clone(),
            if row.strong {
                theme.text
            } else {
                theme.text.gamma_multiply(0.92)
            },
        ),
    );
    if !row.detail.is_empty() {
        job.append(
            row.detail,
            8.0,
            egui::text::TextFormat::simple(egui::FontId::proportional(12.0), theme.text_dim),
        );
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - 4.0 - x).max(0.0));
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let at = egui::pos2(x, y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, theme.text);
    response
}

/// A dim line under a node: "Loading…", an error, or "nothing here".
fn note(ui: &mut egui::Ui, depth: usize, text: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.add_space(22.0 + depth as f32 * INDENT);
        ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap());
    });
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::APP_NAME).size(18.0).strong());
        ui.label(
            RichText::new(format!("v{}", crate::APP_VERSION))
                .small()
                .color(theme.text_dim),
        );
    });
    ui.add(
        egui::TextEdit::singleline(&mut app.tree_filter)
            .hint_text("Filter tables")
            .desired_width(f32::INFINITY),
    );

    let mut actions = Vec::new();
    // The footer is laid out first, from the bottom, so the tree gets the
    // rest of the height.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Look").small().color(theme.text_dim));
            let mut appearance = app.data.settings.appearance;
            for choice in Appearance::ALL {
                ui.selectable_value(&mut appearance, choice, choice.label());
            }
            if appearance != app.data.settings.appearance {
                app.set_appearance(appearance);
            }
        });
        if ui
            .add(
                egui::Button::new("New connection")
                    .min_size(egui::vec2(ui.available_width(), 28.0)),
            )
            .clicked()
        {
            app.editor = Some(ConnectionEditor::new());
        }
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    tree(app, ui, &mut actions);
                });
        });
    });

    for action in actions {
        match action {
            Action::Connect(id) => {
                app.expanded.insert(connection_key(id));
                app.connect(id);
            }
            Action::Toggle(key) => {
                if !app.expanded.remove(&key) {
                    app.expanded.insert(key);
                }
            }
            Action::OpenSchema(id, schema) => {
                let key = schema_key(id, &schema);
                if !app.expanded.remove(&key) {
                    app.expanded.insert(key);
                    app.load_tables(id, &schema);
                }
            }
            Action::OpenTable(id, schema, table) => {
                let key = table_key(id, &schema, &table);
                if !app.expanded.remove(&key) {
                    app.expanded.insert(key);
                    app.load_columns(id, &schema, &table);
                }
            }
            Action::SelectRows(id, table, sql_name) => {
                let tab = app.open_tab(id, format!("SELECT * FROM {sql_name}"), Some(table));
                app.run(tab);
            }
            Action::NewQuery(id) => {
                app.open_tab(id, String::new(), None);
            }
            Action::Disconnect(id) => app.disconnect(id),
            Action::Refresh(id) => app.refresh_tree(id),
            Action::Edit(id) => {
                if let Some(connection) = app.data.connection(id) {
                    app.editor = Some(ConnectionEditor::edit(connection, &app.secrets));
                }
            }
            Action::Copy(text) => ui.ctx().copy_text(text),
        }
    }
}

fn tree(app: &App, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    let theme = app.theme;
    if app.data.connections.is_empty() {
        note(
            ui,
            0,
            "Your connections will be listed here.",
            theme.text_dim,
        );
        return;
    }
    let current = app.current_tab().map(|tab| tab.connection);
    let mut connections: Vec<_> = app.data.connections.iter().collect();
    connections.sort_by_key(|connection| connection.display_name().to_lowercase());
    for connection in connections {
        let id = connection.id;
        let link = app.links.get(&id);
        let open = link.is_some() && app.expanded.contains(&connection_key(id));
        let name = connection.display_name();
        let response = row(
            ui,
            &theme,
            Row {
                depth: 0,
                arrow: Some(open),
                dot: Some(super::link_color(&theme, link)),
                text: &name,
                detail: connection.kind.label(),
                strong: true,
                selected: current == Some(id),
            },
        );
        if response.clicked() {
            actions.push(match link {
                None => Action::Connect(id),
                Some(Link {
                    state: LinkState::Failed(_),
                    ..
                }) => Action::Connect(id),
                Some(_) => Action::Toggle(connection_key(id)),
            });
        }
        let connected = link.is_some_and(Link::connected);
        response.context_menu(|ui| {
            if connected {
                if ui.button("New query").clicked() {
                    actions.push(Action::NewQuery(id));
                    ui.close();
                }
                if ui.button("Refresh").clicked() {
                    actions.push(Action::Refresh(id));
                    ui.close();
                }
            } else if link.is_none() && ui.button("Connect").clicked() {
                actions.push(Action::Connect(id));
                ui.close();
            }
            if link.is_some() && ui.button("Disconnect").clicked() {
                actions.push(Action::Disconnect(id));
                ui.close();
            }
            if ui.button("Edit…").clicked() {
                actions.push(Action::Edit(id));
                ui.close();
            }
        });
        if !open {
            continue;
        }
        let Some(link) = link else {
            continue;
        };
        match &link.state {
            LinkState::Connecting => note(ui, 1, "Connecting…", theme.text_dim),
            LinkState::Failed(error) => note(ui, 1, error, theme.danger),
            LinkState::Connected { .. } => schemas(app, ui, id, link, actions),
        }
    }
}

fn schemas(app: &App, ui: &mut egui::Ui, id: Uuid, link: &Link, actions: &mut Vec<Action>) {
    let theme = app.theme;
    match &link.schemas {
        Load::Idle | Load::Loading => note(ui, 1, "Loading…", theme.text_dim),
        Load::Failed(error) => note(ui, 1, error, theme.danger),
        Load::Ready(nodes) => {
            // A database without schemas lists its tables right away.
            let flat = link.mode == "none";
            for node in nodes {
                let open = flat || app.expanded.contains(&schema_key(id, &node.name));
                if !flat {
                    let response = row(
                        ui,
                        &theme,
                        Row {
                            depth: 1,
                            arrow: Some(open),
                            dot: None,
                            text: &node.name,
                            detail: "",
                            strong: false,
                            selected: false,
                        },
                    );
                    if response.clicked() {
                        actions.push(Action::OpenSchema(id, node.name.clone()));
                    }
                }
                if open {
                    tables(app, ui, id, node, if flat { 1 } else { 2 }, actions);
                }
            }
        }
    }
}

fn tables(
    app: &App,
    ui: &mut egui::Ui,
    id: Uuid,
    schema: &crate::app::SchemaNode,
    depth: usize,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    let needle = app.tree_filter.trim().to_lowercase();
    match &schema.tables {
        Load::Idle | Load::Loading => note(ui, depth, "Loading…", theme.text_dim),
        Load::Failed(error) => note(ui, depth, error, theme.danger),
        Load::Ready(nodes) => {
            let mut shown = 0;
            for node in nodes {
                if !needle.is_empty() && !node.info.name.to_lowercase().contains(&needle) {
                    continue;
                }
                shown += 1;
                let open = app
                    .expanded
                    .contains(&table_key(id, &schema.name, &node.info.name));
                let response = row(
                    ui,
                    &theme,
                    Row {
                        depth,
                        arrow: Some(open),
                        dot: None,
                        text: &node.info.name,
                        detail: if node.info.view { "view" } else { "" },
                        strong: false,
                        selected: false,
                    },
                )
                .on_hover_text("Double-click to see its rows");
                let select =
                    || Action::SelectRows(id, node.info.name.clone(), node.info.sql_name.clone());
                if response.double_clicked() {
                    actions.push(select());
                } else if response.clicked() {
                    actions.push(Action::OpenTable(
                        id,
                        schema.name.clone(),
                        node.info.name.clone(),
                    ));
                }
                response.context_menu(|ui| {
                    if ui.button("See rows").clicked() {
                        actions.push(select());
                        ui.close();
                    }
                    if ui.button("Copy name").clicked() {
                        actions.push(Action::Copy(node.info.name.clone()));
                        ui.close();
                    }
                });
                if !open {
                    continue;
                }
                match &node.columns {
                    Load::Idle | Load::Loading => note(ui, depth + 1, "Loading…", theme.text_dim),
                    Load::Failed(error) => note(ui, depth + 1, error, theme.danger),
                    Load::Ready(columns) => {
                        for column in columns {
                            let mut detail = column.type_name.to_lowercase();
                            if column.key {
                                detail.push_str(" · key");
                            }
                            if !column.nullable {
                                detail.push_str(" · not null");
                            }
                            let response = row(
                                ui,
                                &theme,
                                Row {
                                    depth: depth + 1,
                                    arrow: None,
                                    dot: None,
                                    text: &column.name,
                                    detail: &detail,
                                    strong: false,
                                    selected: false,
                                },
                            );
                            response.context_menu(|ui| {
                                if ui.button("Copy name").clicked() {
                                    actions.push(Action::Copy(column.name.clone()));
                                    ui.close();
                                }
                            });
                        }
                    }
                }
            }
            if shown == 0 {
                let text = if nodes.is_empty() {
                    "No tables here."
                } else {
                    "No table matches the filter."
                };
                note(ui, depth, text, theme.text_dim);
            }
        }
    }
}
