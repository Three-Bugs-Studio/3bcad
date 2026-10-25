//! Embedded wgpu viewport plus the orientation triad.

use eframe::egui::{self, Color32, PointerButton, Sense};
use eframe::egui_wgpu;
use glam::Vec3;
use threebcad_render::{OrbitCamera, ViewportCallback};

use crate::app::App;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let d = resp.drag_delta();
    let ctrl = ui.input(|i| i.modifiers.ctrl);
    if resp.dragged_by(PointerButton::Secondary) || (resp.dragged_by(PointerButton::Middle) && ctrl) {
        app.camera.pan(d.x, d.y, rect.height());
    } else if resp.dragged_by(PointerButton::Primary) || resp.dragged_by(PointerButton::Middle) {
        app.camera.orbit(d.x, d.y);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            app.camera.zoom(scroll);
        }
    }
    if resp.double_clicked() {
        app.zoom_to_fit();
    }

    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewportCallback {
            view_proj: app.camera.view_proj(rect.aspect_ratio()),
            eye: app.camera.eye(),
            geometry: app.geometry.clone(),
            version: app.geometry_version,
        },
    ));
    draw_triad(ui.painter(), rect, &app.camera);
    ui.painter().text(
        rect.right_bottom() - egui::vec2(10.0, 10.0),
        egui::Align2::RIGHT_BOTTOM,
        "LMB orbit · RMB pan · wheel zoom · F / double-click fit",
        egui::FontId::proportional(12.0),
        Color32::from_gray(90),
    );
}

/// Bottom-left XYZ triad that follows the camera.
fn draw_triad(painter: &egui::Painter, rect: egui::Rect, camera: &OrbitCamera) {
    let origin = rect.left_bottom() + egui::vec2(50.0, -50.0);
    let view = camera.view();
    for (axis, color, label) in [
        (Vec3::X, Color32::from_rgb(210, 50, 50), "X"),
        (Vec3::Y, Color32::from_rgb(40, 160, 60), "Y"),
        (Vec3::Z, Color32::from_rgb(50, 90, 220), "Z"),
    ] {
        let v = view.transform_vector3(axis);
        let dir = egui::vec2(v.x, -v.y);
        painter.line_segment([origin, origin + dir * 34.0], egui::Stroke::new(2.5, color));
        painter.text(
            origin + dir * 44.0,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.0),
            color,
        );
    }
}
