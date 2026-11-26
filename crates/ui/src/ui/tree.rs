//! The tree on the left: groups, connections, their schemas, and under each
//! schema its tables, views, routines and sequences.

use eframe::egui::{self, Color32, RichText};
use uuid::Uuid;

use dbpadu_core::model::{Connection, group_name, group_parent};

use super::editor::ConnectionEditor;
use crate::{
    app::{
        App, Category, Link, LinkState, Load, SchemaNode, TableNode, category_key, connection_key,
        schema_key, table_key,
    },
    theme::Theme,
};

/// Width one level of the tree is moved to the right.
const INDENT: f32 = 14.0;
const ROW_HEIGHT: f32 = 24.0;

/// What a click in the tree asks for. Collected while drawing, done after.
enum Action {
    Connect(Uuid),
    Toggle(String),
    ToggleGroup(String),
    OpenSchema(Uuid, String),
    OpenCategory(Uuid, String, Category),
    OpenTable(Uuid, String, String),
    /// A new tab with `SELECT * FROM` the table, run straight away.
    SelectRows(Uuid, String, String),
    NewQuery(Uuid),
    NewConnection(String),
    Disconnect(Uuid),
    Refresh(Uuid),
    Edit(Uuid),
    Copy(String),
}

/// The mark in front of a row's name.
enum Mark {
    None,
    /// How a connection is doing.
    Dot(Color32),
    /// The color of a group.
    Square(Color32),
}

/// One line of the tree.
struct Row<'a> {
    depth: usize,
    /// `Some(open)` draws the arrow of a node that can be opened.
    arrow: Option<bool>,
    mark: Mark,
    text: &'a str,
    /// Dim text after the name, such as a column's type.
    detail: &'a str,
    strong: bool,
    selected: bool,
}

impl<'a> Row<'a> {
    fn plain(depth: usize, text: &'a str) -> Self {
        Self {
            depth,
            arrow: None,
            mark: Mark::None,
            text,
            detail: "",
            strong: false,
            selected: false,
        }
    }
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
    match row.mark {
        Mark::None => {}
        Mark::Dot(color) => {
            painter.circle_filled(egui::pos2(x + 4.0, y), 4.0, color);
            x += 14.0;
        }
        Mark::Square(color) => {
            let square =
                egui::Rect::from_center_size(egui::pos2(x + 5.0, y), egui::vec2(10.0, 10.0));
            painter.rect_filled(square, 3.0, color);
            x += 16.0;
        }
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
    ui.add(
        egui::TextEdit::singleline(&mut app.tree_filter)
            .hint_text("Filter tables, views, routines")
            .desired_width(f32::INFINITY),
    );

    let mut actions = Vec::new();
    // The button is laid out first, from the bottom, so the tree gets the
    // rest of the height.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        if ui
            .add(
                egui::Button::new("New connection")
                    .min_size(egui::vec2(ui.available_width(), 28.0)),
            )
            .clicked()
        {
            actions.push(Action::NewConnection(String::new()));
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
            Action::ToggleGroup(path) => {
                if let Some(group) = app.data.group_mut(&path) {
                    group.collapsed = !group.collapsed;
                    app.save();
                }
            }
            Action::OpenSchema(id, schema) => {
                let key = schema_key(id, &schema);
                if !app.expanded.remove(&key) {
                    // The first time, its tables are shown right away.
                    if app.expanded.insert(key) {
                        app.expanded
                            .insert(category_key(id, &schema, Category::Tables));
                    }
                    app.load_tables(id, &schema);
                }
            }
            Action::OpenCategory(id, schema, category) => {
                let key = category_key(id, &schema, category);
                if !app.expanded.remove(&key) {
                    app.expanded.insert(key);
                    app.load_category(id, &schema, category);
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
            Action::NewConnection(group) => app.editor = Some(ConnectionEditor::new(&group)),
            Action::Disconnect(id) => app.disconnect(id),
            Action::Refresh(id) => app.refresh_tree(id),
            Action::Edit(id) => app.edit_connection(id),
            Action::Copy(text) => ui.ctx().copy_text(text),
        }
    }
}

fn tree(app: &App, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    let theme = app.theme;
    if app.data.connections.is_empty() && app.data.groups.is_empty() {
        note(
            ui,
            0,
            "Your connections will be listed here.",
            theme.text_dim,
        );
        return;
    }
    groups(app, ui, None, 0, actions);
    connections(app, ui, "", 0, actions);
}

/// The groups inside `parent` (the top ones for `None`), each with what is
/// inside it.
fn groups(
    app: &App,
    ui: &mut egui::Ui,
    parent: Option<&str>,
    depth: usize,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    for group in &app.data.groups {
        if group_parent(&group.path) != parent {
            continue;
        }
        let color = app
            .data
            .group_color(&group.path)
            .and_then(super::hex_color)
            .unwrap_or(theme.text_dim.gamma_multiply(0.6));
        let response = row(
            ui,
            &theme,
            Row {
                arrow: Some(!group.collapsed),
                mark: Mark::Square(color),
                strong: true,
                ..Row::plain(depth, group_name(&group.path))
            },
        );
        if response.clicked() {
            actions.push(Action::ToggleGroup(group.path.clone()));
        }
        response.context_menu(|ui| {
            if ui.button("New connection here").clicked() {
                actions.push(Action::NewConnection(group.path.clone()));
                ui.close();
            }
        });
        if !group.collapsed {
            groups(app, ui, Some(&group.path), depth + 1, actions);
            connections(app, ui, &group.path, depth + 1, actions);
        }
    }
}

/// The connections filed right under `group`.
fn connections(app: &App, ui: &mut egui::Ui, group: &str, depth: usize, actions: &mut Vec<Action>) {
    let mut inside: Vec<&Connection> = app
        .data
        .connections
        .iter()
        .filter(|connection| connection.group == group)
        .collect();
    inside.sort_by_key(|connection| connection.display_name().to_lowercase());
    for connection in inside {
        connection_node(app, ui, connection, depth, actions);
    }
}

fn connection_node(
    app: &App,
    ui: &mut egui::Ui,
    connection: &Connection,
    depth: usize,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    let id = connection.id;
    let link = app.links.get(&id);
    let open = link.is_some() && app.expanded.contains(&connection_key(id));
    let name = connection.display_name();
    let response = row(
        ui,
        &theme,
        Row {
            arrow: Some(open),
            mark: Mark::Dot(super::link_color(&theme, link)),
            detail: connection.kind.label(),
            strong: true,
            selected: app.current_tab().map(|tab| tab.connection) == Some(id),
            ..Row::plain(depth, &name)
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
        return;
    }
    let Some(link) = link else {
        return;
    };
    match &link.state {
        LinkState::Connecting => note(ui, depth + 1, "Connecting…", theme.text_dim),
        LinkState::Failed(error) => note(ui, depth + 1, error, theme.danger),
        LinkState::Connected { .. } => schemas(app, ui, id, link, depth + 1, actions),
    }
}

fn schemas(
    app: &App,
    ui: &mut egui::Ui,
    id: Uuid,
    link: &Link,
    depth: usize,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    match &link.schemas {
        Load::Idle | Load::Loading => note(ui, depth, "Loading…", theme.text_dim),
        Load::Failed(error) => note(ui, depth, error, theme.danger),
        Load::Ready(nodes) => {
            // A database without schemas lists its folders right away.
            let flat = link.mode == "none";
            for node in nodes {
                let open = flat || app.expanded.contains(&schema_key(id, &node.name));
                if !flat {
                    let response = row(
                        ui,
                        &theme,
                        Row {
                            arrow: Some(open),
                            ..Row::plain(depth, &node.name)
                        },
                    );
                    if response.clicked() {
                        actions.push(Action::OpenSchema(id, node.name.clone()));
                    }
                }
                if open {
                    let depth = if flat { depth } else { depth + 1 };
                    for category in Category::ALL {
                        folder(app, ui, id, node, category, depth, actions);
                    }
                }
            }
        }
    }
}

/// How many of a list are shown, for the dim text after a folder's name.
fn shown_of(shown: usize, all: usize) -> String {
    if shown == all {
        all.to_string()
    } else {
        format!("{shown} of {all}")
    }
}

/// One folder under a schema: Tables, Views, Routines or Sequences.
fn folder(
    app: &App,
    ui: &mut egui::Ui,
    id: Uuid,
    schema: &SchemaNode,
    category: Category,
    depth: usize,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    let needle = app.tree_filter.trim().to_lowercase();
    let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);

    // What the folder lists, once it is known.
    let tables: Vec<&TableNode> = match (&schema.tables, category) {
        (Load::Ready(nodes), Category::Tables | Category::Views) => nodes
            .iter()
            .filter(|node| node.info.view == (category == Category::Views))
            .collect(),
        _ => Vec::new(),
    };
    let (state, all, shown): (Option<Result<(), &str>>, usize, usize) = match category {
        Category::Tables | Category::Views => match &schema.tables {
            Load::Ready(_) => (
                Some(Ok(())),
                tables.len(),
                tables
                    .iter()
                    .filter(|node| matches(&node.info.name))
                    .count(),
            ),
            Load::Failed(error) => (Some(Err(error)), 0, 0),
            _ => (None, 0, 0),
        },
        Category::Routines => match &schema.routines {
            Load::Ready(items) => (
                Some(Ok(())),
                items.len(),
                items.iter().filter(|item| matches(&item.name)).count(),
            ),
            Load::Failed(error) => (Some(Err(error)), 0, 0),
            _ => (None, 0, 0),
        },
        Category::Sequences => match &schema.sequences {
            Load::Ready(items) => (
                Some(Ok(())),
                items.len(),
                items.iter().filter(|item| matches(item)).count(),
            ),
            Load::Failed(error) => (Some(Err(error)), 0, 0),
            _ => (None, 0, 0),
        },
    };
    let loaded = matches!(state, Some(Ok(())));
    // While filtering, a folder that has a match opens by itself.
    let open = app
        .expanded
        .contains(&category_key(id, &schema.name, category))
        || (!needle.is_empty() && loaded && shown > 0);
    let detail = if loaded {
        shown_of(shown, all)
    } else {
        String::new()
    };
    let response = row(
        ui,
        &theme,
        Row {
            arrow: Some(open),
            detail: &detail,
            ..Row::plain(depth, category.label())
        },
    );
    if response.clicked() {
        actions.push(Action::OpenCategory(id, schema.name.clone(), category));
    }
    if !open {
        return;
    }
    let depth = depth + 1;
    match state {
        None => note(ui, depth, "Loading…", theme.text_dim),
        Some(Err(error)) => note(ui, depth, error, theme.danger),
        Some(Ok(())) => {
            match category {
                Category::Tables | Category::Views => {
                    for node in tables.iter().filter(|node| matches(&node.info.name)) {
                        table(app, ui, id, &schema.name, node, depth, actions);
                    }
                }
                Category::Routines => {
                    if let Load::Ready(items) = &schema.routines {
                        for item in items.iter().filter(|item| matches(&item.name)) {
                            leaf(ui, &theme, depth, &item.name, &item.kind, actions);
                        }
                    }
                }
                Category::Sequences => {
                    if let Load::Ready(items) = &schema.sequences {
                        for item in items.iter().filter(|item| matches(item)) {
                            leaf(ui, &theme, depth, item, "", actions);
                        }
                    }
                }
            }
            if shown == 0 {
                let text = if all == 0 {
                    "Nothing here."
                } else {
                    "Nothing matches the filter."
                };
                note(ui, depth, text, theme.text_dim);
            }
        }
    }
}

/// A row with nothing under it: a routine, a sequence or a column.
fn leaf(
    ui: &mut egui::Ui,
    theme: &Theme,
    depth: usize,
    name: &str,
    detail: &str,
    actions: &mut Vec<Action>,
) {
    let response = row(
        ui,
        theme,
        Row {
            detail,
            ..Row::plain(depth, name)
        },
    );
    response.context_menu(|ui| {
        if ui.button("Copy name").clicked() {
            actions.push(Action::Copy(name.to_string()));
            ui.close();
        }
    });
}

/// A table or a view, with its columns under it when open.
fn table(
    app: &App,
    ui: &mut egui::Ui,
    id: Uuid,
    schema: &str,
    node: &TableNode,
    depth: usize,
    actions: &mut Vec<Action>,
) {
    let theme = app.theme;
    let open = app
        .expanded
        .contains(&table_key(id, schema, &node.info.name));
    let response = row(
        ui,
        &theme,
        Row {
            arrow: Some(open),
            ..Row::plain(depth, &node.info.name)
        },
    )
    .on_hover_text("Double-click to see its rows");
    let select = || Action::SelectRows(id, node.info.name.clone(), node.info.sql_name.clone());
    if response.double_clicked() {
        actions.push(select());
    } else if response.clicked() {
        actions.push(Action::OpenTable(
            id,
            schema.to_string(),
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
        return;
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
                leaf(ui, &theme, depth + 1, &column.name, &detail, actions);
            }
        }
    }
}
