//! Embedded wgpu viewport: camera, picking/highlighting, sketch overlay and the orientation triad.

use eframe::egui::{self, Color32, PointerButton, Sense};
use eframe::egui_wgpu;
use glam::{Vec2 as GVec2, Vec3};
use threebcad_core::{FeatureId, FeatureKind};
use threebcad_render::{pick, HitKind, OrbitCamera, ViewportCallback};

use crate::app::{App, ModelRef};
use crate::ui::sketch_mode::{self, View};

const SKETCH_COLOR: Color32 = Color32::from_rgb(70, 90, 140);
const SKETCH_SELECTED: Color32 = Color32::from_rgb(20, 160, 255);
const COM_COLOR: Color32 = Color32::from_rgb(160, 30, 160);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let ctx = ui.ctx().clone();
    let ctrl = ctx.input(|i| i.modifiers.command);
    let size = GVec2::new(rect.width(), rect.height());

    // Sketch mode gets the mouse first: it may be dragging geometry.
    let mut consumed = false;
    if let Some(session) = &mut app.sketch {
        let view = View {
            cam: &app.camera,
            rect,
            plane: session.plane,
        };
        consumed = session.interact(&view, &resp, &ctx);
    }

    let d = resp.drag_delta();
    if resp.dragged_by(PointerButton::Secondary) || (resp.dragged_by(PointerButton::Middle) && ctrl) {
        app.camera.pan(d.x, d.y, rect.height());
    } else if !consumed && (resp.dragged_by(PointerButton::Primary) || resp.dragged_by(PointerButton::Middle)) {
        app.camera.orbit(d.x, d.y);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            app.camera.zoom(scroll);
        }
    }

    if app.sketch.is_none() {
        model_input(app, &resp, rect, size, ctrl);
    }

    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewportCallback {
            view_proj: app.camera.view_proj(rect.aspect_ratio()),
            eye: app.camera.eye(),
            geometry: app.geometry.clone(),
            version: app.geometry_version,
            states: app.states.clone(),
            states_version: app.states_version,
        },
    ));
    let painter = ui.painter_at(rect);
    paint_vertex_markers(app, &painter, rect, size);

    match &mut app.sketch {
        Some(session) => {
            let view = View {
                cam: &app.camera,
                rect,
                plane: session.plane,
            };
            session.paint(&painter, &view, resp.hover_pos(), ctrl);
            session.editor(&ctx, &view);
            painter.text(
                rect.left_top() + egui::vec2(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                format!("{} — {}", session.tool.label(), session.tool.hint()),
                egui::FontId::proportional(13.0),
                Color32::from_gray(60),
            );
            if let Some(msg) = &session.message {
                painter.text(
                    rect.left_top() + egui::vec2(10.0, 30.0),
                    egui::Align2::LEFT_TOP,
                    msg,
                    egui::FontId::proportional(13.0),
                    Color32::from_rgb(200, 40, 40),
                );
            }
            confirmation_corner(app, &ctx, rect);
        }
        None => {
            for (id, sketch, plane) in loose_sketches(app) {
                let view = View {
                    cam: &app.camera,
                    rect,
                    plane,
                };
                let color = if app.selected == Some(id) {
                    SKETCH_SELECTED
                } else {
                    SKETCH_COLOR
                };
                sketch_mode::draw_sketch(&painter, &view, sketch, color);
            }
        }
    }

    if app.show_mass {
        if let Some(c) = app.mass.as_ref().map(|(_, m)| m.center) {
            if let Some(s) = app
                .camera
                .project(Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32), size)
            {
                let at = rect.min + egui::vec2(s.x, s.y);
                let stroke = egui::Stroke::new(2.0, COM_COLOR);
                painter.circle_stroke(at, 7.0, stroke);
                painter.line_segment([at - egui::vec2(11.0, 0.0), at + egui::vec2(11.0, 0.0)], stroke);
                painter.line_segment([at - egui::vec2(0.0, 11.0), at + egui::vec2(0.0, 11.0)], stroke);
                painter.text(
                    at + egui::vec2(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    "Center of mass",
                    egui::FontId::proportional(12.0),
                    COM_COLOR,
                );
            }
        }
    }
    draw_triad(&painter, rect, &app.camera);
    painter.text(
        rect.right_bottom() - egui::vec2(10.0, 10.0),
        egui::Align2::RIGHT_BOTTOM,
        "LMB orbit / select · RMB pan · wheel zoom · F fit",
        egui::FontId::proportional(12.0),
        Color32::from_gray(90),
    );
}

/// Sketches above the rollback bar that no feature has used yet (shown in the model).
fn loose_sketches(app: &App) -> Vec<(FeatureId, &threebcad_sketch::Sketch, threebcad_kernel::Plane)> {
    let h = app.doc.history();
    // A sketch is used once a feature is built from it (an extrude, a hole, a rib...).
    let used: Vec<FeatureId> = h
        .iter()
        .filter(|(_, f)| !f.kind.is_sketch())
        .flat_map(|(_, f)| f.kind.references())
        .collect();
    h.iter()
        .filter(|(id, _)| h.is_active(*id) && !used.contains(id))
        .filter_map(|(id, f)| match &f.kind {
            FeatureKind::Sketch { sketch, plane, .. } => Some((id, sketch, *plane)),
            _ => None,
        })
        .collect()
}

/// Hover highlight and click selection of faces, edges, vertices, planes and loose sketches.
fn model_input(app: &mut App, resp: &egui::Response, rect: egui::Rect, size: GVec2, ctrl: bool) {
    let cursor = resp.hover_pos().map(|p| GVec2::new(p.x - rect.min.x, p.y - rect.min.y));
    let hit = cursor.and_then(|c| pick(&app.geometry, &app.camera, size, c, 6.0));
    app.hover = hit.and_then(|h| app.refs.get(h.id as usize).copied());

    // A loose sketch under the cursor beats faces behind it (so it can be picked for Extrude).
    let sketch_hit = resp.hover_pos().and_then(|pos| {
        loose_sketches(app).into_iter().find_map(|(id, sketch, plane)| {
            let view = View {
                cam: &app.camera,
                rect,
                plane,
            };
            (sketch_mode::distance_to_sketch(&view, sketch, pos)? <= 6.0).then_some(id)
        })
    });
    if sketch_hit.is_some() && hit.is_none_or(|h| h.kind != HitKind::Vertex) {
        app.hover = None;
    }

    if resp.double_clicked() {
        if let Some(id) = sketch_hit {
            app.edit_sketch(id);
        } else {
            app.zoom_to_fit();
        }
        return;
    }
    if !resp.clicked() {
        return;
    }
    if let Some(id) = sketch_hit {
        app.selected = Some(id);
        app.selection.clear();
        return;
    }
    match app.hover {
        Some(r) if ctrl => {
            if let Some(k) = app.selection.iter().position(|s| *s == r) {
                app.selection.remove(k);
            } else {
                app.selection.push(r);
            }
        }
        Some(r) => app.selection = vec![r],
        None if !ctrl => app.selection.clear(),
        None => {}
    }
    if let Some(r) = app.selection.last() {
        app.status = format!("Selected {}", app.describe(*r));
    }
}

/// B-Rep vertices are only drawn when hovered or selected, like SolidWorks.
fn paint_vertex_markers(app: &App, painter: &egui::Painter, rect: egui::Rect, size: GVec2) {
    for &(p, id) in &app.geometry.points {
        let r = app.refs.get(id as usize).copied();
        let color = match r {
            Some(r) if app.selection.contains(&r) => Color32::from_rgb(20, 130, 255),
            Some(r) if app.hover == Some(r) => Color32::from_rgb(255, 140, 20),
            // Reference points are always shown.
            Some(ModelRef::Feature(_)) => Color32::from_rgb(200, 110, 20),
            _ => continue,
        };
        if let Some(s) = app.camera.project(Vec3::from(p), size) {
            painter.circle_filled(rect.min + egui::vec2(s.x, s.y), 4.5, color);
        }
    }
}

/// OK / Cancel in the top-right corner while sketching.
fn confirmation_corner(app: &mut App, ctx: &egui::Context, rect: egui::Rect) {
    let mut action = None;
    egui::Area::new(egui::Id::new("confirmation_corner"))
        .fixed_pos(rect.right_top() + egui::vec2(-170.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("✔ Exit Sketch").color(Color32::from_rgb(20, 130, 40)),
                    ))
                    .clicked()
                {
                    action = Some(true);
                }
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("✖").color(Color32::from_rgb(200, 40, 40)),
                    ))
                    .on_hover_text("Discard changes")
                    .clicked()
                {
                    action = Some(false);
                }
            });
        });
    if let Some(keep) = action {
        app.exit_sketch(keep);
    }
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
        painter.line_segment([origin, origin + dir * 34.0], egui::Stroke::new(2.5_f32, color));
        painter.text(
            origin + dir * 44.0,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.0),
            color,
        );
    }
}
