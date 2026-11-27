//! Pages and shared UI components.

pub mod about;
pub mod drivers;
pub mod editor;
pub mod export;
pub mod grid;
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
        DbKind::Other => (0x5B, 0x61, 0x6B),
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

/// A line of text in bold, cut off with … where there is no room for it.
/// The app's font has no bold face, so the text is drawn twice, half a
/// point apart.
pub fn bold_label(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    const SHIFT: f32 = 0.5;
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        ui.available_width() - SHIFT,
        egui::TextStyle::Body,
    );
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + egui::vec2(SHIFT, 0.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.galley_with_override_text_color(rect.min, galley.clone(), color);
        painter.galley_with_override_text_color(rect.min + egui::vec2(SHIFT, 0.0), galley, color);
    }
    response
}

/// How a card was picked up, kept while it is dragged so its copy under
/// the pointer stays where the hand took hold of it.
#[derive(Debug, Clone, Copy)]
pub struct Grab {
    /// From the top left corner of the card to the pointer.
    offset: egui::Vec2,
    /// Size of the card where it lies.
    size: egui::Vec2,
    /// When it was picked up, in egui's clock.
    at: f64,
}

/// Makes an area draggable without taking clicks away from the buttons
/// drawn in it. Call this before drawing the area, then
/// [`drag_area_drawn`] with the space it took. Returns how it was picked up
/// in the frame a drag starts.
///
/// The drag has to be registered before the content so the content ends up
/// on top of it; where the content was is remembered from the last frame.
pub fn drag_area(ui: &mut egui::Ui, id: egui::Id) -> Option<Grab> {
    let rect = ui.data(|data| data.get_temp::<egui::Rect>(id))?;
    if !ui.interact(rect, id, egui::Sense::drag()).drag_started() {
        return None;
    }
    let (pressed, at) = ui.input(|input| (input.pointer.press_origin(), input.time));
    Some(Grab {
        offset: pressed.unwrap_or(rect.center()) - rect.min,
        size: rect.size(),
        at,
    })
}

pub fn drag_area_drawn(ui: &egui::Ui, id: egui::Id, rect: egui::Rect) {
    ui.data_mut(|data| data.insert_temp(id, rect));
}

/// Seconds a card takes to rise off the page once it is picked up.
const LIFT_TIME: f64 = 0.12;

/// How much of a card stays visible where it lies while its copy is being
/// dragged.
pub const DRAGGED_OPACITY: f32 = 0.3;

/// The dragged card itself, lifted off the page: it follows the pointer
/// from the spot it was taken by, a little larger and with a shadow under
/// it. `content` draws the card's inside, `padding` away from its edge, at
/// the width it has where it lies.
/// A line under the card says what dropping here does; `ok` is false when
/// dropping here does nothing.
pub fn drag_float(
    ctx: &egui::Context,
    theme: &Theme,
    grab: Grab,
    padding: f32,
    hint: &str,
    ok: bool,
    content: impl FnOnce(&mut egui::Ui),
) {
    let Some(pointer) = ctx.pointer_latest_pos() else {
        return;
    };
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    // 0 on the page, 1 fully lifted.
    let lift = (((ctx.input(|input| input.time) - grab.at) / LIFT_TIME) as f32).clamp(0.0, 1.0);
    if lift < 1.0 {
        ctx.request_repaint();
    }
    let shadow = egui::Shadow {
        offset: [0, (10.0 * lift) as i8],
        blur: (28.0 * lift) as u8,
        spread: 0,
        color: Color32::from_black_alpha(if theme.dark { 150 } else { 70 }),
    };
    let area = egui::Area::new(egui::Id::unique("drag_float"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer - grab.offset - egui::vec2(0.0, 3.0 * lift))
        .interactable(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            card(theme)
                .stroke(egui::Stroke::new(1.5, theme.accent_text))
                .inner_margin(egui::Margin::same(padding as i8))
                .shadow(shadow)
                .show(ui, |ui| {
                    ui.set_width(grab.size.x - 2.0 * padding);
                    ui.set_min_height(grab.size.y - 2.0 * padding);
                    content(ui);
                });
            let (fill, text) = if ok {
                (theme.accent, theme.on_accent)
            } else {
                (theme.panel, theme.text_dim)
            };
            egui::Frame::new()
                .fill(fill)
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(10, 3))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(hint).small().color(text)).extend());
                });
        });
    // Grown around the pointer, so the spot it is held by does not move.
    let scale = 1.0 + 0.03 * lift;
    ctx.set_transform_layer(
        area.response.layer_id,
        egui::emath::TSTransform::from_translation(pointer.to_vec2())
            * egui::emath::TSTransform::from_scaling(scale)
            * egui::emath::TSTransform::from_translation(-pointer.to_vec2()),
    );
}

/// Above the content, but cut off where the list it belongs to is.
fn drop_painter(ui: &egui::Ui) -> egui::Painter {
    ui.ctx()
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("drop_marks"),
        ))
        .with_clip_rect(ui.clip_rect())
}

/// Outline of a place the dragged thing can be dropped on, brighter while
/// the pointer is over it.
pub fn drop_outline(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, over: bool) {
    let painter = drop_painter(ui);
    if over {
        painter.rect_filled(rect, 8.0, theme.accent_text.gamma_multiply(0.10));
    }
    let (width, color) = if over {
        (2.0, theme.accent_text)
    } else {
        (1.0, theme.accent_text.gamma_multiply(0.5))
    };
    painter.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(width, color),
        egui::StrokeKind::Inside,
    );
}

/// A bar between two cards: the dragged card would land here.
pub fn drop_bar(ui: &egui::Ui, theme: &Theme, x: f32, y: egui::Rangef) {
    let rect = egui::Rect::from_x_y_ranges(x - 2.0..=x + 2.0, y);
    drop_painter(ui).rect_filled(rect, 2.0, theme.accent_text);
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
        for kind in DbKind::WITH_OTHER {
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
