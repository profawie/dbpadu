//! Colors. The app has a light and a dark look, the same ones as termker.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// Main background.
    pub bg: Color32,
    /// Sidebar, tab strip and status bar background.
    pub panel: Color32,
    /// Card and input field background.
    pub surface: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    /// Fill of primary buttons and of the selected item.
    pub accent: Color32,
    /// Accent for text, links, outlines and status dots, readable straight
    /// on the backgrounds.
    pub accent_text: Color32,
    /// Text color on top of `accent`.
    pub on_accent: Color32,
    pub danger: Color32,
    /// "All is well": connected, engine ready, committed.
    pub ok: Color32,
    /// "Look here": changes not committed yet, SQL added by the engine.
    pub warn: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Dark look: soft greys rather than black, easy on the eyes.
pub const UI_DARK: Theme = Theme {
    dark: true,
    bg: rgb(0x24272B),
    panel: rgb(0x1C1F22),
    surface: rgb(0x2E3237),
    border: rgb(0x40454D),
    text: rgb(0xE4E6E9),
    text_dim: rgb(0xA4ABB3),
    accent: rgb(0x0B6BA8),
    accent_text: rgb(0x58A6FF),
    on_accent: rgb(0xFFFFFF),
    danger: rgb(0xF87171),
    ok: rgb(0x4ADE80),
    warn: rgb(0xFBBF24),
};

/// Light look.
pub const UI_LIGHT: Theme = Theme {
    dark: false,
    bg: rgb(0xF6F7F8),
    panel: rgb(0xECEEF1),
    surface: rgb(0xFFFFFF),
    border: rgb(0xD2D7DD),
    text: rgb(0x1F2328),
    text_dim: rgb(0x57606A),
    accent: rgb(0x0969DA),
    accent_text: rgb(0x0550AE),
    on_accent: rgb(0xFFFFFF),
    danger: rgb(0xB91C1C),
    ok: rgb(0x116329),
    warn: rgb(0x7D4E00),
};

impl Theme {
    /// The app's dark or light look.
    pub fn ui(dark: bool) -> Theme {
        if dark { UI_DARK } else { UI_LIGHT }
    }

    /// Use this theme for the app's windows, lists and dialogs.
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_theme(if self.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });

        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let radius = CornerRadius::same(6);

        visuals.override_text_color = Some(self.text);
        visuals.panel_fill = self.bg;
        visuals.window_fill = self.panel;
        visuals.window_stroke = Stroke::new(1.0, self.border);
        visuals.extreme_bg_color = self.surface;
        visuals.faint_bg_color = self.surface;
        visuals.hyperlink_color = self.accent_text;
        visuals.selection.bg_fill = self.accent.gamma_multiply(0.35);
        visuals.selection.stroke = Stroke::new(1.0, self.accent_text);
        visuals.warn_fg_color = self.warn;
        visuals.error_fg_color = self.danger;

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = self.panel;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        widgets.noninteractive.corner_radius = radius;

        widgets.inactive.bg_fill = self.surface;
        widgets.inactive.weak_bg_fill = self.surface;
        widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        widgets.inactive.corner_radius = radius;

        widgets.hovered.bg_fill = self.accent.gamma_multiply(0.18);
        widgets.hovered.weak_bg_fill = self.accent.gamma_multiply(0.18);
        widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent_text);
        widgets.hovered.fg_stroke = Stroke::new(1.5, self.text);
        widgets.hovered.corner_radius = radius;

        widgets.active.bg_fill = self.accent.gamma_multiply(0.3);
        widgets.active.weak_bg_fill = self.accent.gamma_multiply(0.3);
        widgets.active.bg_stroke = Stroke::new(1.0, self.accent_text);
        widgets.active.fg_stroke = Stroke::new(2.0, self.text);
        widgets.active.corner_radius = radius;

        widgets.open = widgets.hovered;

        ctx.set_visuals(visuals);
        ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);
            style.spacing.interact_size.y = 28.0;
            for (text_style, font) in style.text_styles.iter_mut() {
                font.size = match text_style {
                    egui::TextStyle::Small => 12.0,
                    egui::TextStyle::Heading => 22.0,
                    _ => 14.0,
                };
            }
        });
    }
}

/// Relative luminance (WCAG), 0 for black to 1 for white.
#[cfg(test)]
fn luminance(color: Color32) -> f32 {
    let [r, g, b, _] = color.to_array();
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG contrast ratio between two colors, from 1 to 21.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// WCAG contrast ratio between text and background must be at least 4.5.
    #[test]
    fn text_is_readable_on_every_background() {
        for theme in [UI_DARK, UI_LIGHT] {
            for bg in [theme.bg, theme.panel, theme.surface] {
                for (name, color) in [
                    ("text", theme.text),
                    ("dim", theme.text_dim),
                    ("accent", theme.accent_text),
                    ("danger", theme.danger),
                    ("ok", theme.ok),
                    ("warn", theme.warn),
                ] {
                    assert!(
                        contrast(color, bg) >= 4.5,
                        "{name} on {bg:?}, dark: {}",
                        theme.dark
                    );
                }
            }
            assert!(contrast(theme.on_accent, theme.accent) >= 4.5);
        }
    }
}
