//! The tab strip: the pinned Connections tab, then one tab per query.

use eframe::egui::{self, RichText};

use crate::app::{App, View};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    let mut close = None;
    let mut new_tab = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let home = app.view == View::Home;
        if tab_button(ui, app, "Connections", None, home).clicked() {
            app.view = View::Home;
        }
        egui::ScrollArea::horizontal()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    for index in 0..app.tabs.len() {
                        let (id, connection) = (app.tabs[index].id, app.tabs[index].connection);
                        let selected = app.view == View::Tab(id);
                        let color = super::link_color(&theme, app.links.get(&connection));
                        let title = app.tabs[index].title.clone();
                        let response = tab_button(ui, app, &title, Some(color), selected);
                        if response.clicked() {
                            app.view = View::Tab(id);
                        }
                        if response.middle_clicked() {
                            close = Some(id);
                        }
                        let x = ui
                            .add(
                                egui::Button::new(RichText::new("×").color(theme.text_dim))
                                    .frame(false),
                            )
                            .on_hover_text("Close tab");
                        if x.clicked() {
                            close = Some(id);
                        }
                        ui.add_space(10.0);
                    }
                    if let Some(tab) = app.current_tab() {
                        let connection = tab.connection;
                        if ui
                            .add(egui::Button::new("+").frame_when_inactive(false))
                            .on_hover_text("New query tab on the same connection")
                            .clicked()
                        {
                            new_tab = Some(connection);
                        }
                    }
                });
            });
    });
    if let Some(id) = close {
        app.close_tab(id);
    }
    if let Some(connection) = new_tab {
        app.open_tab(connection, String::new(), None);
    }
}

fn tab_button(
    ui: &mut egui::Ui,
    app: &App,
    title: &str,
    dot: Option<egui::Color32>,
    selected: bool,
) -> egui::Response {
    let theme = app.theme;
    let text = if selected {
        RichText::new(title).color(theme.on_accent).strong()
    } else {
        RichText::new(title)
    };
    let mut button = egui::Button::new(text).frame_when_inactive(false);
    if selected {
        button = button.fill(theme.accent).frame_when_inactive(true);
    }
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if let Some(color) = dot {
            super::dot(ui, color);
        }
        ui.add(button)
    })
    .inner
}
