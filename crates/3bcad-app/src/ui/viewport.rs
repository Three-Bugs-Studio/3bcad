//! Embedded wgpu viewport: camera, picking/highlighting, sketch overlay and the orientation triad.

use std::sync::Arc;

use eframe::egui::{self, Color32, PointerButton, Sense};
use eframe::egui_wgpu;
use glam::{Vec2 as GVec2, Vec3};
use threebcad_core::{FeatureId, FeatureKind};
use threebcad_render::{
    pick_clipped, Adjacency, DisplayStyle, HitKind, LineVertex, OrbitCamera, StandardView, ViewportCallback,
};

use crate::app::{App, ModelRef, RefPlane};
use crate::i18n::{tr, trf};
use crate::ui::sketch_mode::{self, View};

const SKETCH_COLOR: Color32 = Color32::from_rgb(70, 90, 140);
const SKETCH_SELECTED: Color32 = Color32::from_rgb(20, 160, 255);
const COM_COLOR: Color32 = Color32::from_rgb(160, 30, 160);

/// Display style, section view, and the viewport's floating tools.
#[derive(Default)]
pub struct ViewState {
    pub style: DisplayStyle,
    pub section: Option<Section>,
    /// Where the S key opened the shortcut bar.
    pub shortcut_bar: Option<egui::Pos2>,
    /// Where a right-drag gesture started.
    gesture: Option<egui::Pos2>,
    /// The view is moving (drag, wheel, animation): large models draw their coarse copy.
    moving: bool,
    adjacency: Option<(u64, Arc<Adjacency>)>,
    silhouettes: Arc<Vec<LineVertex>>,
    silhouettes_for: Option<(u64, [i32; 7])>,
    silhouettes_version: u64,
}

/// A section view: the model is cut by a plane parallel to a standard plane (it is only drawn
/// cut; the part does not change).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub plane: RefPlane,
    /// Where the plane is, along the standard plane's normal.
    pub offset: f32,
    /// Keep the other side.
    pub flip: bool,
}

impl Section {
    /// Through the middle of the model, parallel to the Front plane.
    pub fn through(bounds: Option<(Vec3, Vec3)>) -> Self {
        let plane = RefPlane::Front;
        let n = Self::base(plane);
        Self {
            plane,
            offset: bounds.map_or(0.0, |(lo, hi)| n.dot((lo + hi) * 0.5)),
            flip: false,
        }
    }

    fn base(plane: RefPlane) -> Vec3 {
        let n = plane.plane().normal();
        Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32)
    }

    /// The side the cut takes away, as a unit normal.
    pub fn normal(&self) -> Vec3 {
        Self::base(self.plane) * if self.flip { -1.0 } else { 1.0 }
    }

    /// For the shader and picking: what lies where `n · p > d` is cut away.
    pub fn clip(&self) -> [f32; 4] {
        let n = self.normal();
        [n.x, n.y, n.z, self.offset * if self.flip { -1.0 } else { 1.0 }]
    }
}

/// The view cube's faces: the side they show, and their in-face axes (right, up).
const CUBE: [(&str, Vec3, Vec3, Vec3); 6] = [
    ("Front", Vec3::NEG_Y, Vec3::X, Vec3::Z),
    ("Back", Vec3::Y, Vec3::NEG_X, Vec3::Z),
    ("Right", Vec3::X, Vec3::Y, Vec3::Z),
    ("Left", Vec3::NEG_X, Vec3::NEG_Y, Vec3::Z),
    ("Top", Vec3::Z, Vec3::X, Vec3::Y),
    ("Bottom", Vec3::NEG_Z, Vec3::X, Vec3::NEG_Y),
];

/// The direction a view cube cell looks from: its face, plus the neighbouring faces for edge and
/// corner cells (`i`, `j` in 0..3 across and up the face).
fn cube_cell_direction(face: usize, i: usize, j: usize) -> Vec3 {
    let (_, n, u, v) = CUBE[face];
    let side = |k: usize| match k {
        0 => -1.0,
        2 => 1.0,
        _ => 0.0,
    };
    (n + u * side(i) + v * side(j)).normalize()
}

/// Up on screen for a view from `dir`: Z, except straight down or up.
fn up_for(dir: Vec3) -> Vec3 {
    if dir.z.abs() > 0.99 {
        Vec3::Y * dir.z.signum()
    } else {
        Vec3::Z
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    crate::ui::theme::paint_sky(ui.painter(), rect);
    let ctx = ui.ctx().clone();
    let ctrl = ctx.input(|i| i.modifiers.command);
    let shift = ctx.input(|i| i.modifiers.shift);
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

    // In an assembly, dragging a component moves it instead of the camera.
    if app.asm.is_some() && !consumed {
        consumed = app.drag_component(&resp, rect);
    }
    let gestures = app.files.settings.mouse_gestures && !shift;
    let d = resp.drag_delta();
    if resp.dragged_by(PointerButton::Secondary) && gestures {
        if resp.drag_started() {
            app.view.gesture = resp.interact_pointer_pos();
        }
    } else if resp.dragged_by(PointerButton::Secondary) || (resp.dragged_by(PointerButton::Middle) && ctrl) {
        app.camera.pan(d.x, d.y, rect.height());
    } else if !consumed && (resp.dragged_by(PointerButton::Primary) || resp.dragged_by(PointerButton::Middle)) {
        app.camera.orbit(d.x, d.y);
    }
    let mut scrolled = false;
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            app.camera.zoom(scroll);
            scrolled = true;
        }
    }
    let was_moving = app.view.moving;
    app.view.moving = app.files.settings.lod && (resp.dragged() || scrolled || app.camera.is_animating());
    if was_moving && !app.view.moving {
        // Back to the full model once the view stops.
        ctx.request_repaint();
    }

    if app.sketch.is_none() && !resp.dragged() {
        model_input(app, &resp, rect, size, ctrl);
    }

    let clip = app.view.section.map(|s| s.clip());
    update_silhouettes(app);
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewportCallback {
            view_proj: app.camera.view_proj(rect.aspect_ratio()),
            eye: app.camera.eye(),
            body_color: app.doc.material().color.map(|c| f32::from(c) / 255.0),
            geometry: app.geometry.clone(),
            version: app.geometry_version,
            states: app.states.clone(),
            states_version: app.states_version,
            style: app.view.style,
            clip,
            silhouettes: app.view.silhouettes.clone(),
            silhouettes_version: app.view.silhouettes_version,
            lod: app.view.moving,
            // Without faces behind them, dark edges vanish into a dark background.
            edge_color: (app.view.style == DisplayStyle::Wireframe
                && crate::ui::theme::look() != crate::ui::theme::Look::Light)
                .then_some([0.86, 0.88, 0.92]),
        },
    ));
    let painter = ui.painter_at(rect);
    paint_vertex_markers(app, &painter, rect, size);

    let text_color = crate::ui::theme::viewport_text();
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
                format!("{} — {}", tr(session.tool.label()), tr(session.tool.hint())),
                egui::FontId::proportional(13.0),
                text_color,
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
                let color = if app.selected == Some(id) || app.selection.contains(&ModelRef::Feature(id)) {
                    SKETCH_SELECTED
                } else {
                    SKETCH_COLOR
                };
                sketch_mode::draw_sketch(&painter, &view, sketch, color);
            }
        }
    }

    if app.show_mass && app.print_view().is_none() {
        if let Some(c) = app.mass.as_ref().map(|(_, m)| m.center) {
            if let Some(s) = app
                .camera
                .project(Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32), size)
            {
                let at = rect.min + egui::vec2(s.x, s.y);
                let stroke = egui::Stroke::new(2.0_f32, COM_COLOR);
                painter.circle_stroke(at, 7.0, stroke);
                painter.line_segment([at - egui::vec2(11.0, 0.0), at + egui::vec2(11.0, 0.0)], stroke);
                painter.line_segment([at - egui::vec2(0.0, 11.0), at + egui::vec2(0.0, 11.0)], stroke);
                painter.text(
                    at + egui::vec2(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    tr("Center of mass"),
                    egui::FontId::proportional(12.0),
                    COM_COLOR,
                );
            }
        }
    }
    draw_triad(&painter, rect, &app.camera);
    if app.files.settings.view_cube {
        view_cube(app, ui, rect);
    }
    view_toolbar(app, &ctx, rect);
    section_tools(app, ui, &ctx, rect, size);
    gesture_wheel(app, &ctx, &resp, &painter);
    shortcut_bar(app, &ctx);
    let hint = if app.asm.is_some() {
        tr("LMB select · drag a part to move it, Shift+drag to turn it · drag empty space to orbit · RMB pan · wheel zoom")
    } else if app.files.settings.mouse_gestures {
        tr("LMB orbit / select · RMB gestures, Shift+RMB pan · wheel zoom · F fit · S shortcut bar")
    } else {
        tr("LMB orbit / select · RMB pan · wheel zoom · F fit · S shortcut bar")
    };
    painter.text(
        rect.right_bottom() - egui::vec2(10.0, 10.0),
        egui::Align2::RIGHT_BOTTOM,
        hint,
        egui::FontId::proportional(12.0),
        text_color,
    );
    if app.files.settings.show_fps {
        let dt = ctx.input(|i| i.stable_dt).max(1e-4);
        painter.text(
            rect.left_bottom() + egui::vec2(100.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            trf(
                "{} FPS · {} triangles",
                &[&format!("{:.0}", 1.0 / dt), &(app.geometry.triangles.len() / 3)],
            ),
            egui::FontId::monospace(12.0),
            text_color,
        );
        ctx.request_repaint();
    }
}

/// Outlines of curved faces for the styles that draw them, worked out again when the view or the
/// model changes (but not while large models move: they draw their coarse copy then).
fn update_silhouettes(app: &mut App) {
    if !app.view.style.silhouettes() || app.view.moving && !app.geometry.lod.is_empty() {
        return;
    }
    let version = app.geometry_version;
    if app.view.adjacency.as_ref().is_none_or(|(v, _)| *v != version) {
        app.view.adjacency = Some((version, Arc::new(Adjacency::new(&app.geometry.triangles))));
    }
    let cam = &app.camera;
    let eye = cam.eye();
    let look = cam.rotation * -Vec3::Z;
    // Silhouettes only change when the eye moves noticeably.
    let q = |x: f32, scale: f32| (x * scale).round() as i32;
    let unit = cam.distance.max(1e-3) * 1e-3;
    let key = (
        version,
        [
            q(eye.x, 1.0 / unit),
            q(eye.y, 1.0 / unit),
            q(eye.z, 1.0 / unit),
            q(look.x, 1e3),
            q(look.y, 1e3),
            q(look.z, 1e3),
            i32::from(cam.ortho),
        ],
    );
    if app.view.silhouettes_for == Some(key) {
        return;
    }
    let adjacency = &app.view.adjacency.as_ref().unwrap().1;
    app.view.silhouettes = Arc::new(adjacency.silhouette(eye, cam.ortho.then_some(look)));
    app.view.silhouettes_for = Some(key);
    app.view.silhouettes_version += 1;
}

/// The View toolbar over the viewport: standard views, zoom to fit, display style, section view
/// and perspective.
fn view_toolbar(app: &mut App, ctx: &egui::Context, rect: egui::Rect) {
    use crate::commands::{key_of, text, Command};
    let key = |app: &App, c: Command| key_of(&app.files.settings.keys, c).map_or_else(String::new, |k| text(&k));
    egui::Area::new(egui::Id::new("view_toolbar"))
        .fixed_pos(rect.center_top() + egui::vec2(-170.0, 6.0))
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.menu_button(tr("Views"), |ui| {
                            let views = [
                                (StandardView::Front, Command::Front),
                                (StandardView::Back, Command::Back),
                                (StandardView::Left, Command::Left),
                                (StandardView::Right, Command::Right),
                                (StandardView::Top, Command::Top),
                                (StandardView::Bottom, Command::Bottom),
                                (StandardView::Isometric, Command::Isometric),
                            ];
                            for (v, c) in views {
                                if ui
                                    .add(egui::Button::new(tr(v.name())).shortcut_text(key(app, c)))
                                    .clicked()
                                {
                                    ui.close_menu();
                                    app.run(ui.ctx(), c);
                                }
                            }
                            ui.separator();
                            if ui
                                .add(egui::Button::new(tr("Normal To")).shortcut_text(key(app, Command::NormalTo)))
                                .on_hover_text(tr("Look straight at the selected planar face or plane"))
                                .clicked()
                            {
                                ui.close_menu();
                                app.run(ui.ctx(), Command::NormalTo);
                            }
                        })
                        .response
                        .on_hover_text(tr("Standard views (Ctrl+1 … Ctrl+8)"));
                        if ui
                            .button(tr("Zoom Fit"))
                            .on_hover_text(format!("{} ({})", tr("Zoom to fit"), key(app, Command::ZoomToFit)))
                            .clicked()
                        {
                            app.zoom_to_fit();
                        }
                        // A fixed label, so the bar does not shift when the style changes.
                        ui.menu_button(tr("Display"), |ui| {
                            for s in DisplayStyle::ALL {
                                if ui.selectable_label(app.view.style == s, tr(s.name())).clicked() {
                                    ui.close_menu();
                                    app.view.style = s;
                                }
                            }
                        })
                        .response
                        .on_hover_text(format!(
                            "{} ({})",
                            tr("Display style"),
                            key(app, Command::DisplayStyle)
                        ));
                        if ui
                            .selectable_label(app.view.section.is_some(), tr("Section"))
                            .on_hover_text(tr("Cut the model open to look inside (the part does not change)"))
                            .clicked()
                        {
                            app.toggle_section();
                        }
                        let mut persp = !app.camera.ortho;
                        if ui
                            .toggle_value(&mut persp, tr("Perspective"))
                            .on_hover_text(format!(
                                "{} ({})",
                                tr("Off: orthographic, parallel lines stay parallel"),
                                key(app, Command::Perspective)
                            ))
                            .changed()
                        {
                            app.camera.ortho = !persp;
                        }
                    });
                });
        });
}

/// The clickable view cube in the top-right corner: a face, edge or corner turns the view to look
/// from there.
fn view_cube(app: &mut App, ui: &mut egui::Ui, rect: egui::Rect) {
    let center = rect.right_top() + egui::vec2(-70.0, 110.0);
    let half = 30.0;
    let view = app.camera.view();
    let to_screen = |p: Vec3| {
        let v = view.transform_vector3(p);
        center + egui::vec2(v.x, -v.y) * half
    };
    let hover = ui.ctx().pointer_hover_pos();
    let clicked = ui.ctx().input(|i| i.pointer.primary_clicked());
    let painter = ui.painter_at(rect);
    let grid = [-1.0, -0.55, 0.55, 1.0];
    let mut picked = None;
    let (fill, line, text) = match crate::ui::theme::look() {
        crate::ui::theme::Look::Light => (Color32::from_gray(238), Color32::from_gray(120), Color32::from_gray(50)),
        _ => (Color32::from_gray(70), Color32::from_gray(170), Color32::from_gray(235)),
    };
    for (f, &(name, n, u, v)) in CUBE.iter().enumerate() {
        // Only faces turned to the eye (the camera looks down its -Z).
        let facing = view.transform_vector3(n).z;
        if facing <= 0.02 {
            continue;
        }
        for i in 0..3 {
            for j in 0..3 {
                let quad = [
                    (grid[i], grid[j]),
                    (grid[i + 1], grid[j]),
                    (grid[i + 1], grid[j + 1]),
                    (grid[i], grid[j + 1]),
                ]
                .map(|(a, b)| to_screen(n + u * a + v * b));
                let inside = hover.is_some_and(|p| in_polygon(p, &quad));
                if inside {
                    picked = Some((f, i, j));
                }
                let c = if inside { crate::ui::theme::ACCENT } else { fill };
                painter.add(egui::Shape::convex_polygon(
                    quad.to_vec(),
                    c,
                    egui::Stroke::new(0.5_f32, line),
                ));
            }
        }
        if facing > 0.35 {
            painter.text(
                to_screen(n),
                egui::Align2::CENTER_CENTER,
                tr(name),
                egui::FontId::proportional(10.0),
                text,
            );
        }
    }
    if let (Some((f, i, j)), true) = (picked, clicked) {
        let dir = cube_cell_direction(f, i, j);
        app.camera.look_from(dir, up_for(dir));
        ui.ctx().request_repaint();
    }
}

fn in_polygon(p: egui::Pos2, poly: &[egui::Pos2]) -> bool {
    let mut inside = false;
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// The section view's window (plane, position, flip) and the arrow dragged to move the plane.
fn section_tools(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context, rect: egui::Rect, size: GVec2) {
    let Some(mut s) = app.view.section else {
        return;
    };
    let bounds = app.geometry.bounds().unwrap_or((Vec3::splat(-50.0), Vec3::splat(50.0)));
    let base = Section::base(s.plane);
    let corners = (0..8).map(|k| {
        Vec3::new(
            if k & 1 == 0 { bounds.0.x } else { bounds.1.x },
            if k & 2 == 0 { bounds.0.y } else { bounds.1.y },
            if k & 4 == 0 { bounds.0.z } else { bounds.1.z },
        )
    });
    let (lo, hi) = corners.fold((f32::MAX, f32::MIN), |(a, b), c| {
        (a.min(base.dot(c)), b.max(base.dot(c)))
    });
    let mut open = true;
    egui::Window::new(tr("Section View"))
        .open(&mut open)
        .resizable(false)
        .default_pos(rect.left_top() + egui::vec2(12.0, 50.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for p in RefPlane::ALL {
                    if ui.radio_value(&mut s.plane, p, tr(p.name())).changed() {
                        s.offset = Section::base(p).dot((bounds.0 + bounds.1) * 0.5);
                    }
                }
            });
            ui.add(
                egui::Slider::new(&mut s.offset, lo..=hi)
                    .text(tr("position"))
                    .suffix(" mm"),
            );
            ui.checkbox(&mut s.flip, tr("Flip: keep the other side"));
            ui.weak(tr(
                "Drag the arrow in the view to move the plane. The part does not change.",
            ));
        });
    // The arrow: from the plane, in the middle of the model, along the side cut away.
    let mid = (bounds.0 + bounds.1) * 0.5;
    let on_plane = mid - base * (base.dot(mid) - s.offset);
    let n = s.normal();
    let len = (bounds.1 - bounds.0).length().max(1.0) * 0.15;
    if let (Some(a), Some(b)) = (
        app.camera.project(on_plane, size),
        app.camera.project(on_plane + n * len, size),
    ) {
        let (a, b) = (rect.min + egui::vec2(a.x, a.y), rect.min + egui::vec2(b.x, b.y));
        let knob = egui::Rect::from_center_size(b, egui::vec2(18.0, 18.0));
        let r = ui.interact(knob, egui::Id::new("section_handle"), Sense::drag());
        let color = if r.hovered() || r.dragged() {
            crate::ui::theme::ACCENT
        } else {
            Color32::from_rgb(220, 110, 40)
        };
        let painter = ui.painter_at(rect);
        painter.arrow(a, b - a, egui::Stroke::new(2.5_f32, color));
        painter.circle_filled(b, 6.0, color);
        if r.dragged() {
            let dir = b - a;
            if dir.length_sq() > 1.0 {
                let along = r.drag_delta().dot(dir) / dir.length_sq() * len;
                // Along the arrow is further into the side cut away.
                s.offset = (s.offset + along * if s.flip { -1.0 } else { 1.0 }).clamp(lo, hi);
            }
        }
        r.on_hover_text(tr("Drag to move the section plane"));
    }
    app.view.section = open.then_some(s);
}

/// Right-drag gestures: a wheel of four views while the right button is held.
fn gesture_wheel(app: &mut App, ctx: &egui::Context, resp: &egui::Response, painter: &egui::Painter) {
    use crate::commands::Command;
    let Some(start) = app.view.gesture else {
        return;
    };
    // Up, right, down, left.
    let wheel = [
        (Command::Top, "Top"),
        (Command::ZoomToFit, "Zoom Fit"),
        (Command::Front, "Front"),
        (Command::Isometric, "Isometric"),
    ];
    let now = ctx.pointer_latest_pos().unwrap_or(start);
    let d = now - start;
    let chosen = (d.length() > 25.0).then(|| {
        let a = (-d.y).atan2(d.x); // 0 = right, pi/2 = up
        let sector = ((std::f32::consts::FRAC_PI_2 - a) / std::f32::consts::FRAC_PI_2 + 0.5).floor() as i32;
        sector.rem_euclid(4) as usize
    });
    let text = crate::ui::theme::viewport_text();
    painter.circle_stroke(start, 60.0, egui::Stroke::new(1.5_f32, text));
    for (k, (_, name)) in wheel.iter().enumerate() {
        let a = std::f32::consts::FRAC_PI_2 - k as f32 * std::f32::consts::FRAC_PI_2;
        let at = start + egui::vec2(a.cos(), -a.sin()) * 60.0;
        let color = if chosen == Some(k) {
            crate::ui::theme::ACCENT
        } else {
            text
        };
        painter.text(
            at,
            egui::Align2::CENTER_CENTER,
            tr(name),
            egui::FontId::proportional(14.0),
            color,
        );
    }
    if resp.drag_stopped() || !ctx.input(|i| i.pointer.secondary_down()) {
        app.view.gesture = None;
        if let Some(k) = chosen {
            app.run(ctx, wheel[k].0);
        }
    }
}

/// The S key: the current tab's commands at the mouse, gone after a click or Esc.
fn shortcut_bar(app: &mut App, ctx: &egui::Context) {
    let Some(at) = app.view.shortcut_bar else {
        return;
    };
    let r = egui::Area::new(egui::Id::new("shortcut_bar"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(460.0);
                ui.horizontal_wrapped(|ui| crate::ui::ribbon::tab_commands(app, ui));
            });
        });
    // A click runs a command in the bar (this frame) or misses it: either way the bar closes.
    let _ = r;
    if ctx.input(|i| i.pointer.any_click()) {
        app.view.shortcut_bar = None;
    }
}

/// Sketches above the rollback bar that no feature has used yet (shown in the model).
fn loose_sketches(app: &App) -> Vec<(FeatureId, &threebcad_sketch::Sketch, threebcad_kernel::Plane)> {
    // On the print bed the part is moved; sketches stay in model space, so they are hidden. An
    // assembly shows its parts, not the part kept aside.
    if app.print_view().is_some() || app.asm.is_some() {
        return Vec::new();
    }
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
    let clip = app.view.section.map(|s| s.clip());
    let hit = cursor.and_then(|c| pick_clipped(&app.geometry, &app.camera, size, c, 6.0, clip));
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
        // Ctrl+click adds a sketch to the selection (a sweep's path, a loft's next profile).
        if ctrl && app.selected.is_some_and(|s| s != id) {
            let r = ModelRef::Feature(id);
            if let Some(k) = app.selection.iter().position(|s| *s == r) {
                app.selection.remove(k);
            } else {
                app.selection.push(r);
            }
        } else {
            app.selected = Some(id);
            app.selection.clear();
        }
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

/// B-Rep vertices are only drawn when hovered or selected, to keep the model readable.
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
                        egui::RichText::new(tr("✔ Exit Sketch")).color(Color32::from_rgb(20, 130, 40)),
                    ))
                    .clicked()
                {
                    action = Some(true);
                }
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("✖").color(Color32::from_rgb(200, 40, 40)),
                    ))
                    .on_hover_text(tr("Discard changes"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_cube_cells_look_from_their_face_edge_or_corner() {
        // Middle of the Front face: the Front view.
        assert!(cube_cell_direction(0, 1, 1).distance(Vec3::NEG_Y) < 1e-6);
        // Front face, top-right corner: from front, right and above.
        assert!(cube_cell_direction(0, 2, 2).distance(Vec3::new(1.0, -1.0, 1.0).normalize()) < 1e-6);
        // Top face, bottom edge: between Top and Front.
        assert!(cube_cell_direction(4, 1, 0).distance(Vec3::new(0.0, -1.0, 1.0).normalize()) < 1e-6);
        // Every cell of the cube gives a unit direction, and opposite faces opposite ones.
        for f in 0..6 {
            for i in 0..3 {
                for j in 0..3 {
                    assert!((cube_cell_direction(f, i, j).length() - 1.0).abs() < 1e-6);
                }
            }
        }
        assert!(cube_cell_direction(4, 1, 1).distance(-cube_cell_direction(5, 1, 1)) < 1e-6);
        assert_eq!(up_for(Vec3::Z), Vec3::Y);
        assert_eq!(up_for(Vec3::X), Vec3::Z);
    }

    #[test]
    fn a_section_cuts_away_the_side_its_normal_points_to() {
        let mut s = Section::through(Some((Vec3::splat(-10.0), Vec3::splat(10.0))));
        // Front plane through the middle: the half in front (towards the viewer, -Y) goes.
        let cut = |s: &Section, p: Vec3| {
            let [x, y, z, w] = s.clip();
            Vec3::new(x, y, z).dot(p) > w
        };
        assert!(cut(&s, Vec3::new(0.0, -5.0, 0.0)) && !cut(&s, Vec3::new(0.0, 5.0, 0.0)));
        s.flip = true;
        assert!(!cut(&s, Vec3::new(0.0, -5.0, 0.0)) && cut(&s, Vec3::new(0.0, 5.0, 0.0)));
        s.plane = RefPlane::Top;
        s.flip = false;
        s.offset = 3.0;
        assert!(cut(&s, Vec3::new(0.0, 0.0, 4.0)) && !cut(&s, Vec3::new(0.0, 0.0, 2.0)));
    }

    #[test]
    fn points_are_found_inside_a_quad() {
        let q = [
            egui::pos2(0.0, 0.0),
            egui::pos2(10.0, 0.0),
            egui::pos2(10.0, 10.0),
            egui::pos2(0.0, 10.0),
        ];
        assert!(in_polygon(egui::pos2(5.0, 5.0), &q));
        assert!(!in_polygon(egui::pos2(15.0, 5.0), &q));
    }
}
