//! Pages and shared UI components.

pub mod about;
pub mod editor;
pub mod highlight;
pub mod home;
pub mod log;
pub mod query;
pub mod settings;
pub mod statusbar;
pub mod tabs;
pub mod tree;

use eframe::egui::{self, Color32, RichText};

use dbpadu_core::model::DbKind;

use crate::{
    app::{Link, LinkState},
    theme::Theme,
};

/// Card frame: `surface` background with a thin border.
pub fn card(theme: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(14))
}

/// Primary action button, filled with `accent`.
pub fn primary_button(theme: &Theme, text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text.to_owned())
            .color(theme.on_accent)
            .strong(),
    )
    .fill(theme.accent)
}

/// Small dim label above a form field.
pub fn field_label(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    ui.label(RichText::new(label).small().color(theme.text_dim));
}

/// Labelled single-line text field that fills the available width.
pub fn field(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &mut String, hint: &str) {
    field_label(ui, theme, label);
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
}

/// The color a kind of database is known by.
pub fn kind_color(kind: DbKind) -> Color32 {
    let (r, g, b) = match kind {
        DbKind::Postgres => (0x33, 0x67, 0x91),
        DbKind::MySql => (0x00, 0x6E, 0x8A),
        DbKind::MariaDb => (0x9A, 0x4E, 0x2A),
        DbKind::SqlServer => (0xB3, 0x26, 0x1E),
        DbKind::Oracle => (0xC2, 0x41, 0x0C),
        DbKind::Sqlite => (0x0B, 0x6F, 0xB5),
    };
    Color32::from_rgb(r, g, b)
}

/// A small colored square with the kind's two letters.
pub fn kind_badge(ui: &mut egui::Ui, kind: DbKind, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, size * 0.25, kind_color(kind));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        kind.short(),
        egui::FontId::proportional(size * 0.42),
        Color32::WHITE,
    );
    response.on_hover_text(kind.label())
}

/// A color written as `#RRGGBB`.
pub fn hex_color(text: &str) -> Option<Color32> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(Color32::from_rgb(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

/// A page's title with a line of explanation under it.
pub fn page_title(ui: &mut egui::Ui, theme: &Theme, title: &str, about: &str) {
    ui.label(RichText::new(title).size(24.0).strong());
    ui.label(RichText::new(about).color(theme.text_dim));
}

/// The title of a part of a page.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).size(16.0).strong());
}

/// The color of the dot that says how a connection is doing.
pub fn link_color(theme: &Theme, link: Option<&Link>) -> Color32 {
    match link.map(|link| &link.state) {
        Some(LinkState::Connected { .. }) => theme.ok,
        Some(LinkState::Connecting) => theme.warn,
        Some(LinkState::Failed(_)) => theme.danger,
        None => theme.text_dim.gamma_multiply(0.6),
    }
}

/// A filled dot, centered on the text line.
pub fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// SQL on one line: every run of spaces and line breaks becomes one space.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "1 row", "2 rows".
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// "12 ms", "1.4 s".
pub fn duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_is_flattened_to_one_line() {
        assert_eq!(
            one_line("select *\n  from t\nLIMIT 5"),
            "select * from t LIMIT 5"
        );
    }

    #[test]
    fn hex_colors_are_read() {
        assert_eq!(
            hex_color("#2F7FE0"),
            Some(Color32::from_rgb(0x2F, 0x7F, 0xE0))
        );
        assert_eq!(hex_color("2F7FE0"), None);
        assert_eq!(hex_color("#2F7F"), None);
        for (_, color) in dbpadu_core::model::GROUP_COLORS {
            assert!(hex_color(color).is_some(), "{color}");
        }
    }

    #[test]
    fn counts_and_durations_read_naturally() {
        assert_eq!(count(1, "row", "rows"), "1 row");
        assert_eq!(count(0, "row", "rows"), "0 rows");
        assert_eq!(duration(12), "12 ms");
        assert_eq!(duration(1_400), "1.4 s");
    }

    #[test]
    fn white_text_is_readable_on_every_kind_badge() {
        for kind in DbKind::ALL {
            let [r, g, b, _] = kind_color(kind).to_array();
            let lin = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            let luminance = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
            assert!(1.05 / (luminance + 0.05) >= 4.5, "{}", kind.label());
        }
    }
}
