//! Evaluate windows: What's Wrong, Measure and Mass Properties.

use eframe::egui::{self, Color32, RichText};
use threebcad_kernel::topology::{cross, dot, norm, point_triangle, sub, unit, EdgeCurve, P3};

use crate::app::{App, ModelRef, MASS_TOL};

/// A hint for a rebuild message.
pub fn suggestion(msg: &str) -> &'static str {
    let has = |s: &str| msg.contains(s);
    if has("Missing reference") {
        "Edit the feature and pick the face or edge again; the one it used is gone."
    } else if has("boolean operation failed") {
        "Change a dimension slightly or turn off Merge result; truck could not intersect these faces."
    } else if has("too large") || has("larger than the profile") || has("removes the whole body") {
        "Use a smaller value."
    } else if has("Open contour") || has("Branching contour") || has("no closed profile") {
        "Edit the sketch and close the profile (or use a thin feature)."
    } else if has("over-defined") || has("conflicting") {
        "Edit the sketch and run Diagnose to find the conflicting relations."
    } else if has("has an error") {
        "Fix the parent feature first."
    } else if has("no depth") || has("nothing in that direction") || has("does not reach") {
        "Reverse the direction or pick another end condition."
    } else if has("instance") {
        "Check the skipped instances: they may fall outside the part."
    } else if has("centerline") {
        "Draw a centerline in the sketch, or pick an axis in the PropertyManager."
    } else {
        "Double-click the feature to edit it and check its inputs."
    }
}

pub fn windows(app: &mut App, ctx: &egui::Context) {
    whats_wrong(app, ctx);
    measure(app, ctx);
    mass(app, ctx);
}

fn whats_wrong(app: &mut App, ctx: &egui::Context) {
    if !app.show_whats_wrong {
        return;
    }
    let h = app.doc.history();
    let list = |map: &std::collections::HashMap<threebcad_core::FeatureId, String>| {
        let mut v: Vec<_> = map
            .iter()
            .filter_map(|(id, m)| Some((h.position(*id)?, *id, h.get(*id)?.name.clone(), m.clone())))
            .collect();
        v.sort_by_key(|e| e.0);
        v
    };
    let (errors, warnings) = (list(app.doc.errors()), list(app.doc.warnings()));
    let mut open = true;
    let mut pick = None;
    egui::Window::new("What's Wrong")
        .open(&mut open)
        .default_width(420.0)
        .show(ctx, |ui| {
            if errors.is_empty() && warnings.is_empty() {
                ui.label("No rebuild errors or warnings.");
            }
            for (title, items, color) in [
                ("Errors", &errors, Color32::from_rgb(200, 40, 40)),
                ("Warnings", &warnings, Color32::from_rgb(190, 130, 20)),
            ] {
                if items.is_empty() {
                    continue;
                }
                ui.label(
                    RichText::new(format!("{title} ({})", items.len()))
                        .strong()
                        .color(color),
                );
                for (_, id, name, msg) in items.iter() {
                    ui.horizontal_wrapped(|ui| {
                        if ui.link(name).on_hover_text("Select the feature").clicked() {
                            pick = Some(*id);
                        }
                        ui.label(msg);
                    });
                    ui.weak(format!("    → {}", suggestion(msg)));
                }
                ui.separator();
            }
        });
    if let Some(id) = pick {
        app.selected = Some(id);
        app.selection.clear();
    }
    app.show_whats_wrong = open;
}

/// Points standing for a selection item (for distances) and its center.
fn geometry(app: &App, r: ModelRef) -> Option<(Vec<P3>, Vec<[P3; 3]>, P3)> {
    let b = |body: usize| app.doc.bodies().get(body);
    match r {
        ModelRef::Vertex { body, vertex } => {
            let p = *b(body)?.topo.vertices.get(vertex)?;
            Some((vec![p], vec![], p))
        }
        ModelRef::Edge { body, edge } => {
            let e = b(body)?.topo.edges.get(edge)?;
            let c = match e.curve {
                EdgeCurve::Circle { center, .. } => center,
                _ => e.mid,
            };
            Some((e.points.clone(), vec![], c))
        }
        ModelRef::Face { body, face } => {
            let f = b(body)?.topo.faces.get(face)?;
            let pts = f.tris.iter().flatten().copied().collect();
            Some((pts, f.tris.clone(), f.center()))
        }
        ModelRef::Feature(id) => {
            let p = app.doc.point_of(id).or_else(|| app.doc.axis_of(id).map(|a| a.0))?;
            Some((vec![p], vec![], p))
        }
        ModelRef::Plane(_) => None,
    }
}

/// Direction of a straight edge or normal of a planar face.
fn direction(app: &App, r: ModelRef) -> Option<(P3, bool)> {
    match r {
        ModelRef::Edge { body, edge } => match app.doc.bodies().get(body)?.topo.edges.get(edge)?.curve {
            EdgeCurve::Line { a, b } => Some((unit(sub(b, a)), false)),
            _ => None,
        },
        ModelRef::Face { body, face } => {
            Some((app.doc.bodies().get(body)?.topo.faces.get(face)?.plane?.normal(), true))
        }
        ModelRef::Plane(p) => Some((p.plane().normal(), true)),
        ModelRef::Feature(id) => app
            .doc
            .plane_of(id)
            .map(|p| (p.normal(), true))
            .or_else(|| app.doc.axis_of(id).map(|a| (a.1, false))),
        _ => None,
    }
}

fn fmt3(p: P3) -> String {
    format!("({:.3}, {:.3}, {:.3})", p[0], p[1], p[2])
}

fn min_distance(a: &(Vec<P3>, Vec<[P3; 3]>, P3), b: &(Vec<P3>, Vec<[P3; 3]>, P3)) -> f64 {
    let mut best = f64::INFINITY;
    for p in &a.0 {
        for q in &b.0 {
            best = best.min(norm(sub(*p, *q)));
        }
        for t in &b.1 {
            best = best.min(point_triangle(*p, *t));
        }
    }
    for q in &b.0 {
        for t in &a.1 {
            best = best.min(point_triangle(*q, *t));
        }
    }
    best
}

/// Measurement lines for the current selection.
pub fn measurements(app: &App) -> Vec<String> {
    let sel = &app.selection;
    let mut out = Vec::new();
    match sel.as_slice() {
        [] => out.push("Select one or two vertices, edges or faces.".into()),
        [r] => {
            out.push(app.describe(*r));
            match *r {
                ModelRef::Vertex { .. } => {
                    if let Some((_, _, p)) = geometry(app, *r) {
                        out.push(format!("Point {}", fmt3(p)));
                    }
                }
                ModelRef::Edge { body, edge } => {
                    if let Some(e) = app.doc.bodies().get(body).and_then(|b| b.topo.edges.get(edge)) {
                        match e.curve {
                            EdgeCurve::Line { a, b } => {
                                out.push(format!("Length {:.3} mm", norm(sub(b, a))));
                            }
                            EdgeCurve::Circle { center, radius, .. } => {
                                out.push(format!("Arc length {:.3} mm", e.length));
                                out.push(format!("Radius {radius:.3} mm, diameter {:.3} mm", 2.0 * radius));
                                out.push(format!("Center {}", fmt3(center)));
                            }
                            EdgeCurve::Other => out.push(format!("Length {:.3} mm", e.length)),
                        }
                    }
                }
                ModelRef::Face { body, face } => {
                    if let Some(f) = app.doc.bodies().get(body).and_then(|b| b.topo.faces.get(face)) {
                        out.push(format!("Area {:.3} mm²", f.area));
                        if let Some((_, axis, r)) = f.cylinder {
                            out.push(format!(
                                "Cylinder radius {r:.3} mm, diameter {:.3} mm, axis {}",
                                2.0 * r,
                                fmt3(axis)
                            ));
                        }
                        if let Some(p) = f.plane {
                            out.push(format!("Normal {}", fmt3(p.normal())));
                        }
                    }
                }
                _ => {}
            }
        }
        [a, b, ..] => {
            out.push(format!("{} → {}", app.describe(*a), app.describe(*b)));
            if let (Some(ga), Some(gb)) = (geometry(app, *a), geometry(app, *b)) {
                let center = norm(sub(gb.2, ga.2));
                out.push(format!("Center distance {center:.3} mm"));
                let d = sub(gb.2, ga.2);
                out.push(format!("dX {:.3}  dY {:.3}  dZ {:.3}", d[0], d[1], d[2]));
                // Two parallel flat faces: exact gap between the planes.
                let planes = match (*a, *b) {
                    (ModelRef::Face { body: b1, face: f1 }, ModelRef::Face { body: b2, face: f2 }) => {
                        let p = |b: usize, f: usize| {
                            app.doc
                                .bodies()
                                .get(b)
                                .and_then(|x| x.topo.faces.get(f))
                                .and_then(|f| f.plane)
                        };
                        p(b1, f1).zip(p(b2, f2))
                    }
                    _ => None,
                };
                match planes {
                    Some((p, q)) if norm(cross(p.normal(), q.normal())) < 1e-9 => {
                        out.push(format!(
                            "Normal distance {:.3} mm",
                            dot(sub(q.origin, p.origin), p.normal()).abs()
                        ));
                    }
                    _ => out.push(format!("Minimum distance ≈ {:.3} mm", min_distance(&ga, &gb))),
                }
            }
            if let (Some((u, pu)), Some((v, pv))) = (direction(app, *a), direction(app, *b)) {
                let c = dot(u, v).clamp(-1.0, 1.0);
                // A face normal against an edge: angle between the edge and the face.
                let deg = if pu != pv {
                    90.0 - c.abs().acos().to_degrees()
                } else {
                    c.acos().to_degrees()
                };
                out.push(format!("Angle {deg:.3}°"));
            }
        }
    }
    out
}

fn measure(app: &mut App, ctx: &egui::Context) {
    if !app.show_measure {
        return;
    }
    let lines = measurements(app);
    let mut open = true;
    egui::Window::new("Measure")
        .open(&mut open)
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.weak("Ctrl+click to measure between two items.");
            for l in &lines {
                ui.label(l);
            }
            if ui.button("Copy").clicked() {
                ctx.copy_text(lines.join("\n"));
            }
        });
    app.show_measure = open;
}

const MATERIALS: [(&str, f64); 6] = [
    ("PLA", 1.24),
    ("PETG", 1.27),
    ("ABS", 1.04),
    ("Nylon (PA12)", 1.01),
    ("Aluminium 6061", 2.70),
    ("Steel", 7.85),
];

fn mass(app: &mut App, ctx: &egui::Context) {
    if !app.show_mass {
        return;
    }
    let mut open = true;
    let mut density = app.density;
    let props = app.mass_properties().cloned();
    egui::Window::new("Mass Properties")
        .open(&mut open)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Density");
                ui.add(
                    egui::DragValue::new(&mut density)
                        .speed(0.01)
                        .range(0.001..=30.0)
                        .suffix(" g/cm³"),
                );
                egui::ComboBox::from_id_salt("material")
                    .selected_text("Material")
                    .show_ui(ui, |ui| {
                        for (name, d) in MATERIALS {
                            if ui.button(format!("{name} ({d})")).clicked() {
                                density = d;
                            }
                        }
                    });
            });
            let Some(p) = props else {
                ui.label("The part has no bodies.");
                return;
            };
            let g = density / 1000.0; // g/mm³
            let lines = [
                format!("Mass {:.3} g", p.volume * g),
                format!("Volume {:.3} mm³", p.volume),
                format!("Surface area {:.3} mm²", p.area),
                format!("Center of mass {}", fmt3(p.center)),
                format!(
                    "Principal moments (g·mm²) {:.1}, {:.1}, {:.1}",
                    p.principal[0] * g,
                    p.principal[1] * g,
                    p.principal[2] * g
                ),
                format!("Units: mm, g. From a {MASS_TOL} mm tessellation."),
            ];
            for l in &lines {
                ui.label(l);
            }
            if ui.button("Copy").clicked() {
                ctx.copy_text(lines.join("\n"));
            }
            ui.weak("The center of mass is marked in the graphics area.");
        });
    app.density = density;
    app.show_mass = open;
}
