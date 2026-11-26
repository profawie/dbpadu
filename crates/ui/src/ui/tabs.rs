//! The tab strip: the pinned Connections tab, then one tab per query. At
//! the far right: the version, Drivers, Settings and About.

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
        // The right end is laid out first, so the tabs scroll in what is left.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let about = app.view == View::About;
            if tab_button(ui, app, "About", None, about).clicked() {
                app.view = View::About;
            }
            ui.separator();
            let settings = app.view == View::Settings;
            if tab_button(ui, app, "Settings", None, settings).clicked() {
                app.view = View::Settings;
            }
            ui.separator();
            let drivers = app.view == View::Drivers;
            if tab_button(ui, app, "Drivers", None, drivers).clicked() {
                app.view = View::Drivers;
            }
            ui.separator();
            ui.label(
                RichText::new(format!("v{}", crate::APP_VERSION))
                    .small()
                    .color(theme.text_dim),
            );
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                egui::ScrollArea::horizontal()
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            query_tabs(app, ui, &mut close, &mut new_tab);
                        });
                    });
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

fn query_tabs(
    app: &mut App,
    ui: &mut egui::Ui,
    close: &mut Option<u64>,
    new_tab: &mut Option<uuid::Uuid>,
) {
    let theme = app.theme;
    for index in 0..app.tabs.len() {
        let (id, connection) = (app.tabs[index].id, app.tabs[index].connection);
        let selected = app.view == View::Tab(id);
        let color = super::link_color(&theme, app.links.get(&connection));
        let title = app.tabs[index].title.clone();
        let response = tab_button(ui, app, &title, Some(color), selected);
        // A line in the color of the connection's group, so production and
        // test are told apart at a glance.
        let group = app
            .data
            .connection(connection)
            .and_then(|connection| app.data.group_color(&connection.group))
            .and_then(super::hex_color);
        if let Some(group) = group {
            let rect = response.rect;
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() + 3.0,
                egui::Stroke::new(2.0, group),
            );
        }
        if response.clicked() {
            app.view = View::Tab(id);
        }
        if response.middle_clicked() {
            *close = Some(id);
        }
        let x = ui
            .add(egui::Button::new(RichText::new("×").color(theme.text_dim)).frame(false))
            .on_hover_text("Close tab");
        if x.clicked() {
            *close = Some(id);
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
            *new_tab = Some(connection);
        }
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
