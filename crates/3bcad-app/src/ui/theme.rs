//! Light, dark and high-contrast looks (Settings > General > Theme), each with its gradient behind
//! the model. Colours other panels need follow the look in use through the functions below.

use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const ERROR: Color32 = Color32::from_rgb(190, 30, 30);
pub const WARNING: Color32 = Color32::from_rgb(170, 105, 0);
/// Hover and focus outlines.
pub const ACCENT: Color32 = Color32::from_rgb(0, 120, 215);

/// The theme the user picked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Follow the operating system's light or dark setting.
    #[default]
    System,
    Light,
    Dark,
    /// White on black with yellow focus, thicker outlines.
    HighContrast,
}

impl Theme {
    pub const ALL: [Self; 4] = [Self::System, Self::Light, Self::Dark, Self::HighContrast];

    pub fn name(self) -> &'static str {
        match self {
            Self::System => "Same as the system",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::HighContrast => "High contrast",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::HighContrast => "high_contrast",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id.trim())
    }
}

/// The look being drawn this frame (System resolved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Light,
    Dark,
    HighContrast,
}

static LOOK: AtomicU8 = AtomicU8::new(0);

pub fn look() -> Look {
    match LOOK.load(Ordering::Relaxed) {
        1 => Look::Dark,
        2 => Look::HighContrast,
        _ => Look::Light,
    }
}

/// Panels and the ribbon's title row.
pub fn panel() -> Color32 {
    match look() {
        Look::Light => Color32::from_rgb(240, 240, 240),
        Look::Dark => Color32::from_rgb(38, 40, 44),
        Look::HighContrast => Color32::BLACK,
    }
}

/// The ribbon's command area and other raised boxes.
pub fn raised() -> (Color32, Stroke) {
    match look() {
        Look::Light => (Color32::from_rgb(250, 250, 250), Stroke::new(1.0_f32, Color32::from_rgb(210, 210, 210))),
        Look::Dark => (Color32::from_rgb(48, 51, 56), Stroke::new(1.0_f32, Color32::from_rgb(70, 74, 80))),
        Look::HighContrast => (Color32::BLACK, Stroke::new(1.5_f32, Color32::WHITE)),
    }
}

/// Secondary text.
pub fn muted() -> Color32 {
    match look() {
        Look::Light => Color32::from_rgb(140, 140, 140),
        Look::Dark => Color32::from_rgb(150, 154, 160),
        Look::HighContrast => Color32::from_rgb(220, 220, 220),
    }
}

/// Text drawn over the viewport (tool hints, the mouse hint).
pub fn viewport_text() -> Color32 {
    match look() {
        Look::Light => Color32::from_gray(70),
        Look::Dark => Color32::from_gray(200),
        Look::HighContrast => Color32::WHITE,
    }
}

pub fn hover_fill() -> Color32 {
    match look() {
        Look::Light => Color32::from_rgb(229, 241, 251),
        Look::Dark => Color32::from_rgb(52, 70, 92),
        Look::HighContrast => Color32::from_rgb(60, 60, 0),
    }
}

pub fn selected_fill() -> Color32 {
    match look() {
        Look::Light => Color32::from_rgb(204, 228, 247),
        Look::Dark => Color32::from_rgb(40, 84, 130),
        Look::HighContrast => Color32::from_rgb(0, 0, 160),
    }
}

/// Viewport gradient, top and bottom.
pub fn sky() -> (Color32, Color32) {
    match look() {
        Look::Light => (Color32::from_rgb(252, 252, 253), Color32::from_rgb(196, 205, 218)),
        Look::Dark => (Color32::from_rgb(58, 62, 70), Color32::from_rgb(24, 26, 30)),
        Look::HighContrast => (Color32::BLACK, Color32::BLACK),
    }
}

fn light() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = Color32::from_rgb(240, 240, 240);
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(247, 247, 247);
    v.selection.bg_fill = Color32::from_rgb(204, 228, 247);
    v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 84, 153));
    v.hyperlink_color = Color32::from_rgb(0, 102, 204);
    v.error_fg_color = ERROR;
    v.warn_fg_color = WARNING;
    v.window_corner_radius = CornerRadius::same(4);
    let (hover, sel, accent) = (
        Color32::from_rgb(229, 241, 251),
        Color32::from_rgb(204, 228, 247),
        Color32::from_rgb(0, 120, 215),
    );
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(205, 205, 205));
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(30, 30, 30));
    w.inactive.weak_bg_fill = Color32::from_rgb(250, 250, 250);
    w.inactive.bg_fill = Color32::from_rgb(250, 250, 250);
    w.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(200, 200, 200));
    w.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(25, 25, 25));
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_fill = hover;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, accent);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::BLACK);
    w.active.weak_bg_fill = sel;
    w.active.bg_fill = sel;
    w.active.bg_stroke = Stroke::new(1.0_f32, accent);
    w.active.fg_stroke = Stroke::new(1.0_f32, Color32::BLACK);
    w.open.weak_bg_fill = sel;
    v
}

fn dark() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(38, 40, 44);
    v.window_fill = Color32::from_rgb(32, 34, 38);
    v.extreme_bg_color = Color32::from_rgb(24, 25, 28);
    v.faint_bg_color = Color32::from_rgb(44, 46, 50);
    v.selection.bg_fill = Color32::from_rgb(40, 84, 130);
    v.hyperlink_color = Color32::from_rgb(110, 170, 255);
    v.error_fg_color = Color32::from_rgb(255, 110, 100);
    v.warn_fg_color = Color32::from_rgb(240, 180, 70);
    v.window_corner_radius = CornerRadius::same(4);
    v
}

fn high_contrast() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    let (fg, focus) = (Color32::WHITE, Color32::from_rgb(255, 230, 0));
    v.panel_fill = Color32::BLACK;
    v.window_fill = Color32::BLACK;
    v.extreme_bg_color = Color32::BLACK;
    v.faint_bg_color = Color32::from_gray(20);
    v.window_stroke = Stroke::new(2.0_f32, fg);
    v.selection.bg_fill = Color32::from_rgb(0, 0, 160);
    v.selection.stroke = Stroke::new(2.0_f32, focus);
    v.hyperlink_color = Color32::from_rgb(120, 200, 255);
    v.error_fg_color = Color32::from_rgb(255, 90, 90);
    v.warn_fg_color = focus;
    v.override_text_color = Some(fg);
    let w = &mut v.widgets;
    for s in [&mut w.noninteractive, &mut w.inactive] {
        s.bg_fill = Color32::BLACK;
        s.weak_bg_fill = Color32::BLACK;
        s.bg_stroke = Stroke::new(1.5_f32, fg);
        s.fg_stroke = Stroke::new(1.5_f32, fg);
    }
    for s in [&mut w.hovered, &mut w.active, &mut w.open] {
        s.bg_fill = Color32::from_rgb(60, 60, 0);
        s.weak_bg_fill = Color32::from_rgb(60, 60, 0);
        s.bg_stroke = Stroke::new(2.0_f32, focus);
        s.fg_stroke = Stroke::new(2.0_f32, focus);
    }
    v
}

/// Sets egui's looks for `theme`. Call when the setting changes, and `track` every frame.
pub fn apply(ctx: &egui::Context, theme: Theme) {
    ctx.set_visuals_of(egui::Theme::Light, light());
    let dark_slot = if theme == Theme::HighContrast { high_contrast() } else { dark() };
    ctx.set_visuals_of(egui::Theme::Dark, dark_slot);
    ctx.set_theme(match theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark | Theme::HighContrast => egui::ThemePreference::Dark,
    });
    track(ctx, theme);
}

/// Keeps `look()` in step with the theme egui draws (the system's may change while running).
pub fn track(ctx: &egui::Context, theme: Theme) {
    let look = match (theme, ctx.theme()) {
        (Theme::HighContrast, _) => 2,
        (_, egui::Theme::Dark) => 1,
        _ => 0,
    };
    LOOK.store(look, Ordering::Relaxed);
}

/// Vertical gradient filling `rect`.
pub fn paint_sky(painter: &egui::Painter, rect: egui::Rect) {
    let (top, bottom) = sky();
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_theme_draws_its_own_look_and_round_trips() {
        let ctx = egui::Context::default();
        for (theme, want) in [
            (Theme::Light, Look::Light),
            (Theme::Dark, Look::Dark),
            (Theme::HighContrast, Look::HighContrast),
        ] {
            apply(&ctx, theme);
            // A frame resolves the preference into the theme in use.
            let _ = ctx.run(Default::default(), |_| {});
            track(&ctx, theme);
            assert_eq!(look(), want, "{theme:?}");
            assert_eq!(Theme::from_id(theme.id()), Some(theme));
        }
        // Dark and light skies differ, and text over the viewport stands out from both ends.
        apply(&ctx, Theme::Dark);
        let _ = ctx.run(Default::default(), |_| {});
        track(&ctx, Theme::Dark);
        let (top, bottom) = sky();
        let text = viewport_text();
        assert!(top.r() < 100 && bottom.r() < 100);
        assert!(text.r() > 150);
    }
}
