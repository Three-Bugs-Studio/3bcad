//! Light, SolidWorks-like look: light grey panels, white lists, blue selection, and a white-to-grey
//! gradient behind the model.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const PANEL: Color32 = Color32::from_rgb(240, 240, 240);
pub const HOVER_FILL: Color32 = Color32::from_rgb(229, 241, 251);
pub const HOVER_STROKE: Color32 = Color32::from_rgb(0, 120, 215);
pub const SELECTED_FILL: Color32 = Color32::from_rgb(204, 228, 247);
pub const ERROR: Color32 = Color32::from_rgb(190, 30, 30);
pub const WARNING: Color32 = Color32::from_rgb(170, 105, 0);
pub const MUTED: Color32 = Color32::from_rgb(140, 140, 140);
/// Viewport gradient, top and bottom.
pub const SKY_TOP: Color32 = Color32::from_rgb(252, 252, 253);
pub const SKY_BOTTOM: Color32 = Color32::from_rgb(196, 205, 218);

/// Uses the light theme whatever the system theme is.
pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = PANEL;
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(247, 247, 247);
    v.selection.bg_fill = SELECTED_FILL;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 84, 153));
    v.hyperlink_color = Color32::from_rgb(0, 102, 204);
    v.error_fg_color = ERROR;
    v.warn_fg_color = WARNING;
    v.window_corner_radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(205, 205, 205));
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(30, 30, 30));
    w.inactive.weak_bg_fill = Color32::from_rgb(250, 250, 250);
    w.inactive.bg_fill = Color32::from_rgb(250, 250, 250);
    w.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(200, 200, 200));
    w.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(25, 25, 25));
    w.hovered.weak_bg_fill = HOVER_FILL;
    w.hovered.bg_fill = HOVER_FILL;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, HOVER_STROKE);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::BLACK);
    w.active.weak_bg_fill = SELECTED_FILL;
    w.active.bg_fill = SELECTED_FILL;
    w.active.bg_stroke = Stroke::new(1.0_f32, HOVER_STROKE);
    w.active.fg_stroke = Stroke::new(1.0_f32, Color32::BLACK);
    w.open.weak_bg_fill = SELECTED_FILL;
    ctx.set_visuals_of(egui::Theme::Light, v);
    ctx.set_theme(egui::ThemePreference::Light);
}

/// Vertical gradient filling `rect`.
pub fn paint_sky(painter: &egui::Painter, rect: egui::Rect) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), SKY_TOP);
    mesh.colored_vertex(rect.right_top(), SKY_TOP);
    mesh.colored_vertex(rect.right_bottom(), SKY_BOTTOM);
    mesh.colored_vertex(rect.left_bottom(), SKY_BOTTOM);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}
