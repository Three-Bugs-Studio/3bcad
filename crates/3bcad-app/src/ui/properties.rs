//! PropertyManager: parameters of the selected feature, or the active sketch tool and selection.

use eframe::egui::{self, Color32, RichText};
use threebcad_core::holes;
use threebcad_core::*;
use threebcad_sketch::tools::Item;
use threebcad_sketch::Status;

use crate::app::App;
use crate::ui::sketch_mode::{fmt_value, SketchSession, Tool};

/// Undo-merge key: one undo step per feature while a value is being dragged.
fn key(id: FeatureId, param: usize) -> u64 {
    ((id.index() as u64) << 16) | param as u64
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if let Some(session) = &mut app.sketch {
        egui::SidePanel::right("property_manager")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| sketch_panel(session, ui))
            });
        return;
    }
    if app.selected.is_none() {
        if let Some((_, name, _)) = app.sketch_target() {
            let mut start = false;
            egui::SidePanel::right("property_manager")
                .default_width(260.0)
                .show(ctx, |ui| {
                    ui.heading("Selection");
                    ui.separator();
                    ui.label(name);
                    start = ui.button("✏ Sketch on it").clicked();
                });
            if start {
                app.start_sketch();
            }
        }
        return;
    }
    feature_panel(app, ctx);
}

/// What the current model selection offers to a feature's "Use selection" buttons.
struct Picks {
    faces: Vec<FaceRef>,
    edges: Vec<EdgeRef>,
    vertex: Option<VertexRef>,
    axis: Option<(AxisRef, String)>,
    plane: Option<(PlaneRef, String)>,
    point: Option<(PointRef, String)>,
    feature: Option<FeatureId>,
}

fn picks(app: &App) -> Picks {
    Picks {
        faces: app.selected_faces(),
        edges: app.selected_edges(false),
        vertex: app.selected_vertices().into_iter().next(),
        axis: app
            .selection
            .iter()
            .find_map(|&r| app.axis_ref_of(r).map(|a| (a, app.describe(r)))),
        plane: app
            .selection
            .iter()
            .find_map(|&r| app.plane_ref_of(r).map(|(_, p)| (p, app.describe(r)))),
        point: app
            .selection
            .iter()
            .find_map(|&r| app.point_ref_of(r).map(|p| (p, app.describe(r)))),
        feature: None,
    }
}

/// Edits made by the panel: the new feature kind, and whether a value is being dragged.
struct Ed {
    changed: bool,
    dragging: bool,
}

impl Ed {
    fn track(&mut self, r: egui::Response) -> egui::Response {
        if r.changed() {
            self.changed = true;
        }
        if r.dragged() {
            self.dragging = true;
        }
        r
    }

    fn length(&mut self, ui: &mut egui::Ui, label: &str, v: &mut f64, min: f64) {
        ui.horizontal(|ui| {
            ui.label(label);
            self.track(ui.add(egui::DragValue::new(v).speed(0.1).range(min..=10_000.0).suffix(" mm")));
        });
    }

    fn angle(&mut self, ui: &mut egui::Ui, label: &str, rad: &mut f64, range: std::ops::RangeInclusive<f64>) {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut deg = rad.to_degrees();
            if self
                .track(ui.add(egui::DragValue::new(&mut deg).speed(0.5).range(range).suffix("°")))
                .changed()
            {
                *rad = deg.to_radians();
            }
        });
    }

    fn check(&mut self, ui: &mut egui::Ui, v: &mut bool, label: &str) {
        self.track(ui.checkbox(v, label));
    }

    fn count(&mut self, ui: &mut egui::Ui, label: &str, v: &mut usize) {
        ui.horizontal(|ui| {
            ui.label(label);
            self.track(ui.add(egui::DragValue::new(v).range(1..=500)));
        });
    }

    fn button(&mut self, ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
        let clicked = ui.add_enabled(enabled, egui::Button::new(label)).clicked();
        self.changed |= clicked;
        clicked
    }
}

fn describe_plane(h: &History, p: &PlaneRef) -> String {
    match p {
        PlaneRef::Std(s) => s.name().into(),
        PlaneRef::Face(f) => format!("a face of {}", h.get(f.tag.feature).map_or("?", |x| x.name.as_str())),
        PlaneRef::Feature(id) => h.get(*id).map_or("?".into(), |f| f.name.clone()),
    }
}

fn describe_axis(h: &History, a: &AxisRef) -> String {
    match a {
        AxisRef::Std(i) => ["X axis", "Y axis", "Z axis"][(*i as usize).min(2)].into(),
        AxisRef::Edge(e) => format!(
            "an edge of {}",
            h.get(e.faces[0].feature).map_or("?", |x| x.name.as_str())
        ),
        AxisRef::Face(f) => format!("a face of {}", h.get(f.tag.feature).map_or("?", |x| x.name.as_str())),
        AxisRef::Feature(id) => h.get(*id).map_or("?".into(), |f| f.name.clone()),
    }
}

fn axis_editor(ed: &mut Ed, ui: &mut egui::Ui, h: &History, axis: &mut AxisRef, p: &Picks, label: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{label}: {}", describe_axis(h, axis)));
        for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
            if ed.button(ui, n, true) {
                *axis = AxisRef::Std(i as u8);
            }
        }
        if ed.button(ui, "Use selection", p.axis.is_some()) {
            *axis = p.axis.clone().unwrap().0;
        }
    });
}

fn plane_editor(ed: &mut Ed, ui: &mut egui::Ui, h: &History, plane: &mut PlaneRef, p: &Picks, label: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{label}: {}", describe_plane(h, plane)));
        if ed.button(ui, "Use selection", p.plane.is_some()) {
            *plane = p.plane.clone().unwrap().0;
        }
    });
}

fn end_editor(ed: &mut Ed, ui: &mut egui::Ui, salt: &str, end: &mut End, p: &Picks) {
    let options: Vec<End> = {
        let depth = match end {
            End::Blind(d) | End::MidPlane(d) => *d,
            End::OffsetFromSurface(_, d) => *d,
            _ => 10.0,
        };
        let mut v = vec![End::Blind(depth), End::ThroughAll, End::UpToNext, End::MidPlane(depth)];
        if let Some(vx) = &p.vertex {
            v.push(End::UpToVertex(vx.clone()));
        }
        if let Some(f) = p.faces.first() {
            v.push(End::UpToSurface(f.clone()));
            v.push(End::OffsetFromSurface(f.clone(), 1.0));
        }
        v
    };
    egui::ComboBox::from_id_salt(salt)
        .selected_text(end.name())
        .show_ui(ui, |ui| {
            for o in options {
                let name = o.name();
                if ui.selectable_label(end.name() == name, name).clicked() && end.name() != name {
                    *end = o;
                    ed.changed = true;
                }
            }
        });
    match end {
        End::Blind(d) | End::MidPlane(d) => ed.length(ui, "Depth", d, 0.001),
        End::OffsetFromSurface(_, d) => {
            ui.horizontal(|ui| {
                ui.label("Offset");
                ed.track(ui.add(egui::DragValue::new(d).speed(0.1).suffix(" mm")));
            });
        }
        End::UpToVertex(_) | End::UpToSurface(_) => {
            ui.weak("Select another vertex/face and pick the condition again to change it.");
        }
        _ => {}
    }
}

fn refs_line(ed: &mut Ed, ui: &mut egui::Ui, what: &str, n: usize, avail: bool) -> bool {
    let mut take = false;
    ui.horizontal(|ui| {
        ui.label(format!("{n} {what}"));
        take = ed.button(ui, "Use selection", avail);
    });
    take
}

fn parse_list(s: &str) -> Vec<Vec<usize>> {
    s.split(';')
        .filter_map(|part| {
            let v: Vec<usize> = part.split(',').filter_map(|x| x.trim().parse::<usize>().ok()).collect();
            (!v.is_empty()).then_some(v)
        })
        .collect()
}

fn feature_panel(app: &mut App, ctx: &egui::Context) {
    let Some(id) = app.selected else { return };
    let Some(feature) = app.doc.history().get(id) else {
        return;
    };
    let name = feature.name.clone();
    let mut kind = feature.kind.clone();
    let error = app.doc.errors().get(&id).cloned();
    let warning = app.doc.warnings().get(&id).cloned();
    let status = app.doc.sketch_status(id);
    let thread = app.doc.built().threads.get(&id).cloned();
    let mut p = picks(app);
    p.feature = app.selected;
    let h = app.doc.history().clone();

    let mut close = false;
    let mut edit_sketch = false;
    let mut sketch_dim: Option<(usize, f64)> = None;
    let mut ed = Ed {
        changed: false,
        dragging: false,
    };
    egui::SidePanel::right("property_manager")
        .resizable(true)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&name);
                close = ui.small_button("✖").on_hover_text("Close").clicked();
            });
            ui.separator();
            if let Some(e) = &error {
                ui.colored_label(Color32::LIGHT_RED, e);
                ui.weak(crate::ui::evaluate::suggestion(e));
                ui.separator();
            }
            if let Some(w) = &warning {
                ui.colored_label(Color32::from_rgb(190, 130, 20), w);
                ui.separator();
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                let from = |s: &FeatureId| format!("From: {}", h.get(*s).map_or("?", |f| f.name.as_str()));
                match &mut kind {
                    FeatureKind::Sketch { sketch, on, .. } => {
                        if let Some((s, dof)) = status {
                            ui.label(match s {
                                Status::UnderDefined => format!("Under defined ({dof} DOF)"),
                                Status::FullyDefined => "Fully defined".to_string(),
                                Status::OverDefined => "Over defined".to_string(),
                            });
                        }
                        if let Some(o) = on {
                            ui.label(format!("Follows {}", describe_plane(&h, o)));
                        }
                        edit_sketch = ui.button("✏ Edit Sketch").clicked();
                        let dims: Vec<_> = sketch
                            .constraints()
                            .iter()
                            .enumerate()
                            .filter_map(|(i, c)| Some((i, c.name(), c.dimension()?, c.is_angular())))
                            .collect();
                        if !dims.is_empty() {
                            ui.label("Dimensions");
                        }
                        for (i, label, value, is_angle) in dims {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                let (mut shown, suffix) = if is_angle {
                                    (value.to_degrees(), "°")
                                } else {
                                    (value, " mm")
                                };
                                let range = if is_angle { -360.0..=360.0 } else { 0.01..=10_000.0 };
                                let r = ui.add(egui::DragValue::new(&mut shown).speed(0.2).range(range).suffix(suffix));
                                if r.changed() {
                                    sketch_dim = Some((i, if is_angle { shown.to_radians() } else { shown }));
                                }
                            });
                        }
                        let groups = sketch.pattern_groups();
                        for g in groups {
                            ui.horizontal(|ui| {
                                ui.label(format!("Pattern {} instances", g + 1));
                                let mut n = sketch.pattern_count(g);
                                if ed.track(ui.add(egui::DragValue::new(&mut n).range(2..=200))).changed() {
                                    let _ = sketch.set_pattern_count(g, n);
                                }
                            });
                        }
                    }
                    FeatureKind::Extrude(e) => {
                        ui.label(from(&e.sketch));
                        ui.label(RichText::new("Direction 1").strong());
                        end_editor(&mut ed, ui, "end1", &mut e.end, &p);
                        ed.check(ui, &mut e.reverse, "Reverse direction");
                        let mut two = e.end2.is_some();
                        if !matches!(e.end, End::MidPlane(_)) {
                            ed.check(ui, &mut two, "Direction 2");
                            if two != e.end2.is_some() {
                                e.end2 = two.then_some(End::Blind(10.0));
                            }
                            if let Some(e2) = &mut e.end2 {
                                end_editor(&mut ed, ui, "end2", e2, &p);
                            }
                        }
                        ui.separator();
                        let mut drafted = e.draft != 0.0;
                        ed.check(ui, &mut drafted, "Draft");
                        if drafted && e.draft == 0.0 {
                            e.draft = 3f64.to_radians();
                        } else if !drafted {
                            e.draft = 0.0;
                        }
                        if drafted {
                            let mut outward = e.draft < 0.0;
                            let mut a = e.draft.abs();
                            ed.angle(ui, "Angle", &mut a, 0.1..=60.0);
                            ed.check(ui, &mut outward, "Draft outward");
                            e.draft = if outward { -a } else { a };
                        }
                        let mut thin = e.thin.is_some();
                        ed.check(ui, &mut thin, "Thin feature");
                        if thin != e.thin.is_some() {
                            e.thin = thin.then_some(Thin {
                                kind: ThinKind::OneDirection,
                                t1: 1.0,
                                t2: 1.0,
                                reverse: false,
                            });
                        }
                        if let Some(t) = &mut e.thin {
                            ui.horizontal(|ui| {
                                for (k, n) in [
                                    (ThinKind::OneDirection, "One"),
                                    (ThinKind::MidPlane, "Mid"),
                                    (ThinKind::TwoDirection, "Two"),
                                ] {
                                    ed.track(ui.radio_value(&mut t.kind, k, n));
                                }
                            });
                            ed.length(ui, "Thickness", &mut t.t1, 0.001);
                            if t.kind == ThinKind::TwoDirection {
                                ed.length(ui, "Thickness 2", &mut t.t2, 0.001);
                            }
                            if t.kind == ThinKind::OneDirection {
                                ed.check(ui, &mut t.reverse, "Other side");
                            }
                        }
                        ui.separator();
                        if e.cut {
                            ed.check(ui, &mut e.flip_side, "Flip side to cut");
                        } else {
                            ed.check(ui, &mut e.merge, "Merge result");
                        }
                    }
                    FeatureKind::Revolve(r) => {
                        ui.label(from(&r.sketch));
                        let axis_name = match &r.axis {
                            None => "first centerline of the sketch".to_string(),
                            Some(RevolveAxis::SketchLine(e)) => format!("sketch line {}", e.0 + 1),
                            Some(RevolveAxis::Axis(a)) => describe_axis(&h, a),
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("Axis: {axis_name}"));
                            if ed.button(ui, "Centerline", r.axis.is_some()) {
                                r.axis = None;
                            }
                            if ed.button(ui, "Use selection", p.axis.is_some()) {
                                r.axis = Some(RevolveAxis::Axis(p.axis.clone().unwrap().0));
                            }
                        });
                        ed.angle(ui, "Angle", &mut r.angle, 0.1..=360.0);
                        ed.angle(ui, "Direction 2 angle", &mut r.angle2, 0.0..=360.0);
                        ed.check(ui, &mut r.reverse, "Reverse direction");
                        if !r.cut {
                            ed.check(ui, &mut r.merge, "Merge result");
                        }
                    }
                    FeatureKind::Fillet {
                        edges,
                        radius,
                        propagate,
                    } => {
                        ed.length(ui, "Radius", radius, 0.001);
                        ed.check(ui, propagate, "Tangent propagation");
                        if refs_line(&mut ed, ui, "edges", edges.len(), !p.edges.is_empty()) {
                            *edges = p.edges.clone();
                        }
                    }
                    FeatureKind::Chamfer {
                        edges,
                        kind: ck,
                        d1,
                        d2,
                        angle,
                        flip,
                    } => {
                        for (k, n) in [
                            (ChamferKind::EqualDistance, "Equal distance"),
                            (ChamferKind::DistanceDistance, "Distance-distance"),
                            (ChamferKind::AngleDistance, "Angle-distance"),
                        ] {
                            ed.track(ui.radio_value(ck, k, n));
                        }
                        ed.length(ui, "Distance", d1, 0.001);
                        match ck {
                            ChamferKind::DistanceDistance => ed.length(ui, "Distance 2", d2, 0.001),
                            ChamferKind::AngleDistance => ed.angle(ui, "Angle", angle, 1.0..=89.0),
                            ChamferKind::EqualDistance => {}
                        }
                        ed.check(ui, flip, "Flip direction");
                        if refs_line(&mut ed, ui, "edges", edges.len(), !p.edges.is_empty()) {
                            *edges = p.edges.clone();
                        }
                    }
                    FeatureKind::Shell {
                        faces,
                        thickness,
                        outward,
                    } => {
                        ed.length(ui, "Thickness", thickness, 0.001);
                        ed.check(ui, outward, "Shell outward");
                        if refs_line(&mut ed, ui, "faces removed", faces.len(), !p.faces.is_empty()) {
                            *faces = p.faces.clone();
                        }
                    }
                    FeatureKind::Hole(hole) => {
                        ui.label(from(&hole.sketch));
                        egui::ComboBox::from_id_salt("hole_kind")
                            .selected_text(hole.kind.name())
                            .show_ui(ui, |ui| {
                                for k in HoleKind::ALL {
                                    ed.track(ui.selectable_value(&mut hole.kind, k, k.name()));
                                }
                            });
                        ui.horizontal(|ui| {
                            let before = hole.standard;
                            ed.track(ui.radio_value(&mut hole.standard, HoleStandard::Iso, "ISO"));
                            ed.track(ui.radio_value(&mut hole.standard, HoleStandard::Ansi, "ANSI Inch"));
                            if before != hole.standard {
                                hole.size = holes::table(hole.standard)[4].name.into();
                            }
                        });
                        egui::ComboBox::from_id_salt("hole_size")
                            .selected_text(hole.size.clone())
                            .show_ui(ui, |ui| {
                                for s in holes::table(hole.standard) {
                                    if ui.selectable_label(hole.size == s.name, s.name).clicked() {
                                        hole.size = s.name.into();
                                        ed.changed = true;
                                    }
                                }
                            });
                        let mut through = matches!(hole.end, End::ThroughAll);
                        ed.check(ui, &mut through, "Through All");
                        if through != matches!(hole.end, End::ThroughAll) {
                            hole.end = if through { End::ThroughAll } else { End::Blind(10.0) };
                        }
                        if let End::Blind(d) = &mut hole.end {
                            ed.length(ui, "Depth", d, 0.01);
                        }
                        if let Some(s) = holes::find(hole.standard, &hole.size) {
                            ui.weak(format!("Drill Ø{:.2} mm", s.drill(hole.kind)));
                            match hole.kind {
                                HoleKind::Counterbore => ui.weak(format!(
                                    "Counterbore Ø{:.2} × {:.2} mm",
                                    s.cbore_diameter, s.cbore_depth
                                )),
                                HoleKind::Countersink => {
                                    ui.weak(format!("Countersink Ø{:.2} mm × {}°", s.csk_diameter, s.csk_angle))
                                }
                                _ => ui.label(""),
                            };
                        }
                        if let Some(t) = &thread {
                            ui.label(format!("Thread: {} (cosmetic)", t.designation));
                        }
                        ui.weak("Holes go at the sketch's points; edit the sketch to move or add them.");
                    }
                    FeatureKind::LinearPattern(lp) => {
                        seeds_editor(&mut ed, ui, &h, id, &mut lp.seeds);
                        ui.label(RichText::new("Direction 1").strong());
                        axis_editor(&mut ed, ui, &h, &mut lp.dir1.axis, &p, "Along");
                        ed.length(ui, "Spacing", &mut lp.dir1.spacing, 0.001);
                        ed.count(ui, "Instances", &mut lp.dir1.count);
                        ed.check(ui, &mut lp.dir1.reverse, "Reverse");
                        let mut two = lp.dir2.is_some();
                        ed.check(ui, &mut two, "Direction 2");
                        if two != lp.dir2.is_some() {
                            lp.dir2 = two.then_some(PatternDir {
                                axis: AxisRef::Std(1),
                                spacing: 20.0,
                                count: 2,
                                reverse: false,
                            });
                        }
                        if let Some(d) = &mut lp.dir2 {
                            axis_editor(&mut ed, ui, &h, &mut d.axis, &p, "Along");
                            ed.length(ui, "Spacing", &mut d.spacing, 0.001);
                            ed.count(ui, "Instances", &mut d.count);
                            ed.check(ui, &mut d.reverse, "Reverse");
                        }
                        let mut text = lp
                            .skip
                            .iter()
                            .map(|(i, j)| format!("{},{}", i + 1, j + 1))
                            .collect::<Vec<_>>()
                            .join("; ");
                        ui.label("Instances to skip (row,column; …)");
                        if ui.text_edit_singleline(&mut text).lost_focus() {
                            lp.skip = parse_list(&text)
                                .into_iter()
                                .filter(|v| v[0] >= 1)
                                .map(|v| (v[0] - 1, v.get(1).copied().unwrap_or(1).saturating_sub(1)))
                                .collect();
                            ed.changed = true;
                        }
                    }
                    FeatureKind::CircularPattern(cp) => {
                        seeds_editor(&mut ed, ui, &h, id, &mut cp.seeds);
                        axis_editor(&mut ed, ui, &h, &mut cp.axis, &p, "Axis");
                        ed.count(ui, "Instances", &mut cp.count);
                        ed.check(ui, &mut cp.equal, "Equal spacing");
                        ed.angle(
                            ui,
                            if cp.equal { "Total angle" } else { "Angle between" },
                            &mut cp.angle,
                            0.1..=360.0,
                        );
                        ed.check(ui, &mut cp.reverse, "Reverse");
                        let mut text = cp
                            .skip
                            .iter()
                            .map(|k| (k + 1).to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        ui.label("Instances to skip (1 is the seed)");
                        if ui.text_edit_singleline(&mut text).lost_focus() {
                            cp.skip = parse_list(&text.replace(',', ";"))
                                .into_iter()
                                .filter(|v| v[0] >= 2)
                                .map(|v| v[0] - 1)
                                .collect();
                            ed.changed = true;
                        }
                    }
                    FeatureKind::Mirror(m) => {
                        seeds_editor(&mut ed, ui, &h, id, &mut m.seeds);
                        plane_editor(&mut ed, ui, &h, &mut m.plane, &p, "Mirror face/plane");
                        if m.seeds == Seeds::Bodies {
                            ed.check(ui, &mut m.merge, "Merge solids");
                        }
                    }
                    FeatureKind::RefPlane(def) => match def {
                        RefPlaneDef::Offset { base, distance, flip } => {
                            plane_editor(&mut ed, ui, &h, base, &p, "Offset from");
                            ui.horizontal(|ui| {
                                ui.label("Distance");
                                ed.track(ui.add(egui::DragValue::new(distance).speed(0.2).suffix(" mm")));
                            });
                            ed.check(ui, flip, "Flip normal");
                        }
                        RefPlaneDef::Angle { base, axis, angle } => {
                            plane_editor(&mut ed, ui, &h, base, &p, "Base");
                            axis_editor(&mut ed, ui, &h, axis, &p, "Through");
                            ed.angle(ui, "Angle", angle, -360.0..=360.0);
                        }
                        RefPlaneDef::ThreePoint(_) => {
                            ui.label("Through three points");
                        }
                        RefPlaneDef::Mid(a, b) => {
                            plane_editor(&mut ed, ui, &h, a, &p, "First");
                            plane_editor(&mut ed, ui, &h, b, &p, "Second");
                        }
                    },
                    FeatureKind::RefAxis(def) => {
                        ui.label(match def {
                            RefAxisDef::TwoPoints(..) => "Through two points",
                            RefAxisDef::Edge(_) => "Along an edge",
                            RefAxisDef::Cylinder(_) => "Axis of a cylindrical face",
                            RefAxisDef::TwoPlanes(..) => "Where two planes meet",
                        });
                    }
                    FeatureKind::RefPoint(def) => match def {
                        RefPointDef::Coords(c) => {
                            for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(*n);
                                    ed.track(ui.add(egui::DragValue::new(&mut c[i]).speed(0.2).suffix(" mm")));
                                });
                            }
                        }
                        RefPointDef::Vertex(_) => {
                            ui.label("At a vertex");
                        }
                        RefPointDef::FaceCenter(_) => {
                            ui.label("At the center of a face");
                        }
                        RefPointDef::ArcCenter(_) => {
                            ui.label("At the center of an arc");
                        }
                    },
                    FeatureKind::CoordSys(c) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(match &c.origin {
                                PointRef::Origin => "Origin: part origin".to_string(),
                                PointRef::Vertex(_) => "Origin: a vertex".to_string(),
                                PointRef::Feature(id) => {
                                    format!("Origin: {}", h.get(*id).map_or("?", |f| f.name.as_str()))
                                }
                            });
                            if ed.button(ui, "Use selection", p.point.is_some()) {
                                c.origin = p.point.clone().unwrap().0;
                            }
                        });
                        let mut x = c.x.clone().unwrap_or(AxisRef::Std(0));
                        let mut z = c.z.clone().unwrap_or(AxisRef::Std(2));
                        axis_editor(&mut ed, ui, &h, &mut x, &p, "X");
                        axis_editor(&mut ed, ui, &h, &mut z, &p, "Z");
                        c.x = Some(x);
                        c.z = Some(z);
                    }
                    FeatureKind::Draft { neutral, faces, angle } => {
                        ed.angle(ui, "Draft angle", angle, -60.0..=60.0);
                        plane_editor(&mut ed, ui, &h, neutral, &p, "Neutral plane");
                        if refs_line(&mut ed, ui, "faces to draft", faces.len(), !p.faces.is_empty()) {
                            *faces = p.faces.clone();
                        }
                    }
                    FeatureKind::Rib {
                        sketch,
                        thickness,
                        both_sides,
                        flip,
                    } => {
                        ui.label(from(sketch));
                        ed.length(ui, "Thickness", thickness, 0.001);
                        ed.check(ui, both_sides, "Both sides of the sketch plane");
                        ed.check(ui, flip, "Flip material side");
                        ui.weak(
                            "The rib grows from the line to the nearest wall and stops at the part's bounding box.",
                        );
                    }
                }
            });
        });

    if close {
        app.selected = None;
    }
    if edit_sketch {
        app.edit_sketch(id);
    }
    if let Some((param, value)) = sketch_dim {
        app.doc.edit_merging(Some(key(id, param)), |h| {
            if let Some(FeatureKind::Sketch { sketch, .. }) = h.get_mut(id).map(|f| &mut f.kind) {
                sketch.set_dimension(param, value);
            }
        });
    }
    if ed.changed {
        let merge = ed.dragging.then(|| key(id, 0xFFFF));
        app.doc.edit_merging(merge, |h| {
            if let Some(f) = h.get_mut(id) {
                f.kind = kind;
            }
        });
    }
}

fn seeds_editor(ed: &mut Ed, ui: &mut egui::Ui, h: &History, me: FeatureId, seeds: &mut Seeds) {
    let mut bodies = *seeds == Seeds::Bodies;
    ed.check(ui, &mut bodies, "Bodies (every body so far)");
    if bodies != (*seeds == Seeds::Bodies) {
        *seeds = if bodies {
            Seeds::Bodies
        } else {
            Seeds::Features(Vec::new())
        };
    }
    if let Seeds::Features(list) = seeds {
        ui.label("Features to repeat:");
        // Solid features above this one.
        for (id, f) in h.iter().take_while(|(id, _)| *id != me) {
            let solid = matches!(
                f.kind,
                FeatureKind::Extrude(_)
                    | FeatureKind::Revolve(_)
                    | FeatureKind::Hole(_)
                    | FeatureKind::Fillet { .. }
                    | FeatureKind::Chamfer { .. }
                    | FeatureKind::Rib { .. }
            );
            if !solid {
                continue;
            }
            let mut on = list.contains(&id);
            if ed.track(ui.checkbox(&mut on, &f.name)).changed() {
                if on {
                    list.push(id);
                } else {
                    list.retain(|x| *x != id);
                }
            }
        }
    }
}
fn sketch_panel(s: &mut SketchSession, ui: &mut egui::Ui) {
    ui.heading(format!("Sketch on {}", s.plane_name));
    let (text, color) = s.status_text();
    ui.colored_label(color, RichText::new(text).strong());
    ui.separator();

    ui.label(RichText::new(s.tool.label()).strong());
    tool_options(s, ui);
    if let Some(msg) = &s.message {
        ui.colored_label(Color32::from_rgb(200, 40, 40), msg);
    }
    ui.separator();

    if s.analysis.status == Status::OverDefined {
        ui.colored_label(
            Color32::from_rgb(200, 40, 40),
            "Some relations conflict or repeat each other.",
        );
        if ui.button("Diagnose (SketchXpert)").clicked() {
            s.run_diagnosis();
        }
        if let Some(fixes) = s.diagnosis.clone() {
            if fixes.is_empty() {
                ui.label("No single relation fixes it; delete relations until the sketch turns black or blue.");
            } else {
                ui.label("Deleting any one of these fixes it:");
                for i in fixes {
                    ui.horizontal(|ui| {
                        if ui.small_button("Delete").clicked() {
                            s.remove_relation(i);
                        }
                        ui.label(s.describe_relation(i));
                    });
                }
            }
        }
        ui.separator();
    }

    if s.selection.is_empty() {
        ui.weak("Select sketch geometry to see and add relations.");
        return;
    }
    ui.label(RichText::new("Selected").strong());
    for item in s.selection.clone() {
        ui.label(format!("• {}", s.describe(item)));
    }
    let entities: Vec<_> = s
        .selection
        .iter()
        .filter_map(|i| match *i {
            Item::Entity(e) => Some(e),
            _ => None,
        })
        .collect();
    if !entities.is_empty() {
        let mut construction = entities.iter().all(|&e| s.sketch.entity(e).construction);
        if ui.checkbox(&mut construction, "For construction").changed() {
            s.set_construction(construction);
        }
    }
    if let [Item::Constraint(i)] = s.selection[..] {
        if let Some(v) = s.sketch.constraints()[i].dimension() {
            ui.horizontal(|ui| {
                ui.label(fmt_value(&s.sketch.constraints()[i], v));
                if ui.button("Edit value").clicked() {
                    s.start_editing(i);
                }
            });
        }
    }

    let existing = s.relations_of_selection();
    if !existing.is_empty() {
        ui.separator();
        ui.label(RichText::new("Existing relations").strong());
        for i in existing {
            ui.horizontal(|ui| {
                if ui.small_button("✖").on_hover_text("Delete relation").clicked() {
                    s.remove_relation(i);
                }
                let over = s.analysis.over_constraints.get(i).copied().unwrap_or(false);
                let text = RichText::new(s.describe_relation(i));
                ui.label(if over {
                    text.color(Color32::from_rgb(200, 40, 40))
                } else {
                    text
                });
            });
        }
    }
    let candidates = s.sketch.relation_candidates(&s.selection);
    if !candidates.is_empty() {
        ui.separator();
        ui.label(RichText::new("Add relations").strong());
        ui.horizontal_wrapped(|ui| {
            for c in candidates {
                if ui.button(c.name()).clicked() {
                    s.add_relation(c);
                }
            }
        });
    }
    ui.separator();
    if ui.button("Delete selected").clicked() {
        s.delete_selection();
    }
}

fn tool_options(s: &mut SketchSession, ui: &mut egui::Ui) {
    let o = &mut s.opts;
    match s.tool {
        Tool::Circle => {
            let mut typed = o.radius.is_some();
            ui.horizontal(|ui| {
                ui.checkbox(&mut typed, "Radius");
                let mut r = o.radius.unwrap_or(10.0);
                if ui
                    .add_enabled(
                        typed,
                        egui::DragValue::new(&mut r)
                            .speed(0.1)
                            .range(0.001..=10_000.0)
                            .suffix(" mm"),
                    )
                    .changed()
                    || typed
                {
                    o.radius = typed.then_some(r);
                }
                if !typed {
                    o.radius = None;
                }
            });
        }
        Tool::Polygon => {
            ui.horizontal(|ui| {
                ui.label("Sides");
                ui.add(egui::DragValue::new(&mut o.polygon_sides).range(3..=40));
            });
            ui.radio_value(&mut o.inscribed, true, "Inscribed circle (click a vertex)");
            ui.radio_value(&mut o.inscribed, false, "Circumscribed circle (click an edge midpoint)");
        }
        Tool::Fillet => {
            ui.horizontal(|ui| {
                ui.label("Radius");
                ui.add(
                    egui::DragValue::new(&mut o.fillet_radius)
                        .speed(0.1)
                        .range(0.001..=10_000.0)
                        .suffix(" mm"),
                );
            });
        }
        Tool::Chamfer => {
            ui.radio_value(&mut o.chamfer_angle_mode, false, "Distance-distance");
            ui.radio_value(&mut o.chamfer_angle_mode, true, "Angle-distance");
            ui.horizontal(|ui| {
                ui.label("Distance");
                ui.add(
                    egui::DragValue::new(&mut o.chamfer_d1)
                        .speed(0.1)
                        .range(0.001..=10_000.0)
                        .suffix(" mm"),
                );
            });
            ui.horizontal(|ui| {
                if o.chamfer_angle_mode {
                    ui.label("Angle");
                    ui.add(
                        egui::DragValue::new(&mut o.chamfer_angle)
                            .speed(0.5)
                            .range(1.0..=179.0)
                            .suffix("°"),
                    );
                } else {
                    ui.label("Distance 2");
                    ui.add(
                        egui::DragValue::new(&mut o.chamfer_d2)
                            .speed(0.1)
                            .range(0.001..=10_000.0)
                            .suffix(" mm"),
                    );
                }
            });
        }
        Tool::Offset => {
            ui.horizontal(|ui| {
                ui.label("Distance");
                ui.add(
                    egui::DragValue::new(&mut o.offset)
                        .speed(0.1)
                        .range(0.001..=10_000.0)
                        .suffix(" mm"),
                );
            });
            ui.checkbox(&mut o.offset_reverse, "Reverse");
        }
        Tool::LinearPattern => {
            ui.horizontal(|ui| {
                ui.label("Instances");
                ui.add(egui::DragValue::new(&mut o.pattern_count).range(2..=200));
            });
            ui.horizontal(|ui| {
                ui.label("Spacing");
                ui.add(egui::DragValue::new(&mut o.pattern_spacing).speed(0.1).suffix(" mm"));
            });
            ui.horizontal(|ui| {
                ui.label("Direction");
                ui.add(egui::DragValue::new(&mut o.pattern_angle).speed(1.0).suffix("°"));
            });
        }
        Tool::CircularPattern => {
            ui.horizontal(|ui| {
                ui.label("Instances");
                ui.add(egui::DragValue::new(&mut o.circular_count).range(2..=200));
            });
            ui.horizontal(|ui| {
                ui.label("Total angle");
                ui.add(
                    egui::DragValue::new(&mut o.circular_total)
                        .speed(1.0)
                        .range(1.0..=360.0)
                        .suffix("°"),
                );
            });
        }
        _ => {}
    }
    ui.weak(s.tool.hint());
    if s.tool.uses_selection() {
        let ready = !s.selection.is_empty();
        if ui.add_enabled(ready, egui::Button::new("Apply")).clicked() {
            s.apply_selection_tool();
        }
    }
}
