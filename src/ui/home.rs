//! The first page: every saved connection as a card.

use eframe::egui::{self, RichText};
use uuid::Uuid;

use dbpadu_core::model::Connection;

use super::editor::ConnectionEditor;
use crate::app::{App, Link, LinkState};

const CARD_INNER_WIDTH: f32 = 250.0;

enum CardAction {
    Connect,
    NewQuery,
    Disconnect,
    Edit,
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new("Connections").size(24.0).strong());
            ui.label(
                RichText::new("One card for each database. Click Connect to start working.")
                    .color(theme.text_dim),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(super::primary_button(&theme, "New connection"))
                .clicked()
            {
                app.editor = Some(ConnectionEditor::new());
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

    if app.data.connections.is_empty() {
        empty(app, ui);
        return;
    }

    let needle = app.search.trim().to_lowercase();
    let mut shown: Vec<&Connection> = app
        .data
        .connections
        .iter()
        .filter(|connection| {
            needle.is_empty()
                || connection.display_name().to_lowercase().contains(&needle)
                || connection.target().to_lowercase().contains(&needle)
        })
        .collect();
    shown.sort_by_key(|connection| connection.display_name().to_lowercase());

    let mut action: Option<(Uuid, CardAction)> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if shown.is_empty() {
                ui.label(RichText::new("No connection matches the search.").color(theme.text_dim));
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                for connection in shown {
                    if let Some(clicked) = card(ui, app, connection) {
                        action = Some((connection.id, clicked));
                    }
                }
            });
        });

    match action {
        Some((id, CardAction::Connect)) => app.connect(id),
        Some((id, CardAction::NewQuery)) => {
            app.open_tab(id, String::new(), None);
        }
        Some((id, CardAction::Disconnect)) => app.disconnect(id),
        Some((id, CardAction::Edit)) => {
            if let Some(connection) = app.data.connection(id) {
                app.editor = Some(ConnectionEditor::edit(connection, &app.secrets));
            }
        }
        None => {}
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
            app.editor = Some(ConnectionEditor::new());
        }
    });
}

fn card(ui: &mut egui::Ui, app: &App, connection: &Connection) -> Option<CardAction> {
    let theme = app.theme;
    let link = app.links.get(&connection.id);
    let mut frame = super::card(&theme);
    if link.is_some_and(Link::connected) {
        frame = frame.stroke(egui::Stroke::new(1.5, theme.ok));
    }
    let mut action = None;
    frame.show(ui, |ui| {
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
                    ui.label(
                        RichText::new(connection.kind.label())
                            .small()
                            .color(theme.text_dim),
                    );
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
                        action = Some(CardAction::Edit);
                    }
                    if link.is_some() && ui.small_button("Disconnect").clicked() {
                        action = Some(CardAction::Disconnect);
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
                        action = Some(CardAction::NewQuery);
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
                        action = Some(CardAction::Connect);
                    }
                }
                None => {
                    if ui
                        .add(super::primary_button(&theme, "Connect").min_size(wide))
                        .clicked()
                    {
                        action = Some(CardAction::Connect);
                    }
                }
            }
        });
    });
    action
}
