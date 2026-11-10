//! The v0.4 features in the UI: their ribbon commands and their Properties panel editors.
//! Sweeps, lofts, helices, threads, variable and face fillets, multibody tools, sketch- and
//! curve-driven patterns, domes and wraps.

use eframe::egui;
use crate::i18n::tr;
use threebcad_core::holes;
use threebcad_core::*;

use crate::app::{App, ModelRef};
use crate::ui::icons;
use crate::ui::properties::{axis_editor, length, number, plane_editor, refs_line, seeds_editor, Ed, Picks};

fn name_of(h: &History, id: FeatureId) -> String {
    h.get(id).map_or("?".into(), |f| f.name.clone())
}

/// Features above `me` that pass `keep`.
fn above(h: &History, me: FeatureId, keep: impl Fn(&FeatureKind) -> bool) -> Vec<(FeatureId, String)> {
    h.iter()
        .take_while(|(id, _)| *id != me)
        .filter(|(_, f)| keep(&f.kind))
        .map(|(id, f)| (id, f.name.clone()))
        .collect()
}

fn sketch_combo(ed: &mut Ed, ui: &mut egui::Ui, salt: &str, h: &History, me: FeatureId, cur: &mut FeatureId) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(name_of(h, *cur))
        .show_ui(ui, |ui| {
            for (id, name) in above(h, me, FeatureKind::is_sketch) {
                if ui.selectable_label(*cur == id, name).clicked() && *cur != id {
                    *cur = id;
                    ed.changed = true;
                }
            }
        });
}

fn curve_name(h: &History, c: &CurveRef) -> String {
    match *c {
        CurveRef::Sketch(id) | CurveRef::Helix(id) => name_of(h, id),
    }
}

fn curve_combo(ed: &mut Ed, ui: &mut egui::Ui, salt: &str, h: &History, me: FeatureId, cur: &mut CurveRef) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(curve_name(h, cur))
        .show_ui(ui, |ui| {
            for (id, name) in above(h, me, |k| k.is_sketch() || matches!(k, FeatureKind::Helix(_))) {
                let c = if h.get(id).is_some_and(|f| f.kind.is_sketch()) {
                    CurveRef::Sketch(id)
                } else {
                    CurveRef::Helix(id)
                };
                if ui.selectable_label(*cur == c, name).clicked() && *cur != c {
                    *cur = c;
                    ed.changed = true;
                }
            }
        });
}

/// An ordered list of sketches with reorder/remove buttons and an "Add" menu.
fn sketch_list(ed: &mut Ed, ui: &mut egui::Ui, salt: &str, h: &History, me: FeatureId, list: &mut Vec<FeatureId>) {
    let mut action = None;
    for (i, id) in list.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}. {}", i + 1, name_of(h, *id)));
            if ui.small_button("⬆").on_hover_text(tr("Move up")).clicked() && i > 0 {
                action = Some((i, 0));
            }
            if ui.small_button("⬇").on_hover_text(tr("Move down")).clicked() && i + 1 < list.len() {
                action = Some((i, 1));
            }
            if ui.small_button("✖").on_hover_text(tr("Remove")).clicked() {
                action = Some((i, 2));
            }
        });
    }
    match action {
        Some((i, 0)) => list.swap(i, i - 1),
        Some((i, 1)) => list.swap(i, i + 1),
        Some((i, _)) => {
            list.remove(i);
        }
        None => {}
    }
    if action.is_some() {
        ed.changed = true;
    }
    egui::ComboBox::from_id_salt(salt)
        .selected_text(tr("Add…"))
        .show_ui(ui, |ui| {
            for (id, name) in above(h, me, FeatureKind::is_sketch) {
                if !list.contains(&id) && ui.selectable_label(false, name).clicked() {
                    list.push(id);
                    ed.changed = true;
                }
            }
        });
}

fn faces_line(ed: &mut Ed, ui: &mut egui::Ui, what: &str, list: &mut Vec<FaceRef>, p: &Picks) {
    if refs_line(ed, ui, what, list.len(), !p.faces.is_empty()) {
        *list = p.faces.clone();
    }
}

fn face_line(ed: &mut Ed, ui: &mut egui::Ui, what: &str, face: &mut FaceRef, p: &Picks) {
    ui.horizontal(|ui| {
        ui.label(what);
        if ed.button(ui, "Use selection", !p.faces.is_empty()) {
            *face = p.faces[0].clone();
        }
    });
}

fn bodies_line(ed: &mut Ed, ui: &mut egui::Ui, list: &mut Vec<FaceRef>, p: &Picks) {
    ui.horizontal(|ui| {
        if list.is_empty() {
            ui.label(tr("All bodies"));
        } else {
            ui.label(format!("{} bodies (picked by a face)", list.len()));
        }
        if ed.button(ui, "Use selection", !p.faces.is_empty()) {
            *list = p.faces.clone();
        }
        if !list.is_empty() && ed.button(ui, "All", true) {
            list.clear();
        }
    });
}

fn signed(ed: &mut Ed, ui: &mut egui::Ui, label: &str, v: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ed.track(ui.add(length(v).speed(0.1)));
    });
}

fn size_editor(ed: &mut Ed, ui: &mut egui::Ui, salt: &str, standard: &mut HoleStandard, size: &mut String) {
    ui.horizontal(|ui| {
        let before = *standard;
        ed.track(ui.radio_value(standard, HoleStandard::Iso, tr("ISO metric")));
        ed.track(ui.radio_value(standard, HoleStandard::Ansi, tr("ANSI Inch")));
        if before != *standard {
            *size = holes::table(*standard)[4].name.into();
        }
    });
    egui::ComboBox::from_id_salt(salt)
        .selected_text(size.clone())
        .show_ui(ui, |ui| {
            for s in holes::table(*standard) {
                if ui.selectable_label(*size == s.name, s.name).clicked() {
                    *size = s.name.into();
                    ed.changed = true;
                }
            }
        });
}

/// The Properties panel for a v0.4 feature.
pub(crate) fn editor(ed: &mut Ed, ui: &mut egui::Ui, h: &History, me: FeatureId, kind: &mut FeatureKind, p: &Picks) {
    match kind {
        FeatureKind::Sweep(w) => {
            ui.label(tr("Profile"));
            let mut circle = matches!(w.profile, SweepProfile::Circle(_));
            ui.horizontal(|ui| {
                ed.track(ui.radio_value(&mut circle, false, tr("Sketch profile")));
                ed.track(ui.radio_value(&mut circle, true, tr("Circular profile")));
            });
            if circle != matches!(w.profile, SweepProfile::Circle(_)) {
                w.profile = if circle {
                    SweepProfile::Circle(5.0)
                } else {
                    match above(h, me, FeatureKind::is_sketch).first() {
                        Some(&(id, _)) => SweepProfile::Sketch(id),
                        None => SweepProfile::Circle(5.0),
                    }
                };
            }
            match &mut w.profile {
                SweepProfile::Circle(d) => ed.length(ui, "Diameter", d, 0.001),
                SweepProfile::Sketch(id) => sketch_combo(ed, ui, "sweep_profile", h, me, id),
            }
            ui.label(tr("Path"));
            curve_combo(ed, ui, "sweep_path", h, me, &mut w.path);
            ed.angle(ui, "Twist", &mut w.twist, -3600.0..=3600.0);
            ed.check(ui, &mut w.cut, "Cut (remove material)");
            if !w.cut {
                ed.check(ui, &mut w.merge, "Merge result");
            }
            ui.weak(tr("The profile is carried along the path, square to it. A circular profile needs no sketch."));
        }
        FeatureKind::Loft(l) => {
            ui.label(tr("Profiles, in order"));
            sketch_list(ed, ui, "loft_add", h, me, &mut l.profiles);
            ui.label(tr("Guide curves"));
            sketch_list(ed, ui, "loft_guide", h, me, &mut l.guides);
            ui.horizontal(|ui| {
                ui.label(tr("Start tangency"));
                ed.track(ui.add(number(&mut l.start_tangent).speed(0.05).range(0.0..=5.0)));
            });
            ui.horizontal(|ui| {
                ui.label(tr("End tangency"));
                ed.track(ui.add(number(&mut l.end_tangent).speed(0.05).range(0.0..=5.0)));
            });
            ed.check(ui, &mut l.cut, "Cut (remove material)");
            if !l.cut {
                ed.check(ui, &mut l.merge, "Merge result");
            }
            ui.weak(tr("Tangency 0 leaves the end free; larger values pull the surface square out of the profile."));
        }
        FeatureKind::Helix(d) => {
            ui.label(format!("Circle from: {}", name_of(h, d.sketch)));
            let kinds = [
                HelixSize::PitchTurns { pitch: 5.0, turns: 4.0 },
                HelixSize::HeightPitch {
                    height: 20.0,
                    pitch: 5.0,
                },
                HelixSize::HeightTurns {
                    height: 20.0,
                    turns: 4.0,
                },
                HelixSize::Spiral { pitch: 5.0, turns: 4.0 },
            ];
            egui::ComboBox::from_id_salt("helix_kind")
                .selected_text(d.size.name())
                .show_ui(ui, |ui| {
                    for k in kinds {
                        if ui.selectable_label(d.size.name() == k.name(), k.name()).clicked()
                            && d.size.name() != k.name()
                        {
                            // Keep the shape: carry pitch and turns over.
                            let (pitch, turns) = d.size.pitch_turns();
                            let pitch = if pitch > 0.0 { pitch } else { 5.0 };
                            d.size = match k {
                                HelixSize::PitchTurns { .. } => HelixSize::PitchTurns { pitch, turns },
                                HelixSize::HeightPitch { .. } => HelixSize::HeightPitch {
                                    height: pitch * turns,
                                    pitch,
                                },
                                HelixSize::HeightTurns { .. } => HelixSize::HeightTurns {
                                    height: pitch * turns,
                                    turns,
                                },
                                HelixSize::Spiral { .. } => HelixSize::Spiral { pitch, turns },
                            };
                            ed.changed = true;
                        }
                    }
                });
            let turns = |ed: &mut Ed, ui: &mut egui::Ui, t: &mut f64| {
                ui.horizontal(|ui| {
                    ui.label(tr("Revolutions"));
                    ed.track(ui.add(number(t).speed(0.05).range(0.01..=500.0)));
                });
            };
            match &mut d.size {
                HelixSize::PitchTurns { pitch, turns: t } => {
                    ed.length(ui, "Pitch", pitch, 0.001);
                    turns(ed, ui, t);
                }
                HelixSize::HeightPitch { height, pitch } => {
                    ed.length(ui, "Height", height, 0.001);
                    ed.length(ui, "Pitch", pitch, 0.001);
                }
                HelixSize::HeightTurns { height, turns: t } => {
                    ed.length(ui, "Height", height, 0.001);
                    turns(ed, ui, t);
                }
                HelixSize::Spiral { pitch, turns: t } => {
                    ed.length(ui, "Pitch (radial)", pitch, 0.001);
                    turns(ed, ui, t);
                }
            }
            if !matches!(d.size, HelixSize::Spiral { .. }) {
                ed.angle(ui, "Taper angle", &mut d.taper, -80.0..=80.0);
            }
            ed.angle(ui, "Start angle", &mut d.start, -360.0..=360.0);
            ui.horizontal(|ui| {
                ed.track(ui.radio_value(&mut d.ccw, false, tr("Clockwise")));
                ed.track(ui.radio_value(&mut d.ccw, true, tr("Counterclockwise")));
            });
            ed.check(ui, &mut d.reverse, "Reverse direction");
            ui.weak(tr("Use the helix as the path of a sweep (a spring) or of a curve pattern."));
        }
        FeatureKind::Thread(t) => {
            face_line(ed, ui, "Cylindrical face (a rod or a hole)", &mut t.face, p);
            size_editor(ed, ui, "thread_size", &mut t.standard, &mut t.size);
            if let Some(s) = holes::find(t.standard, &t.size) {
                ui.weak(format!(
                    "Major Ø{:.2} mm, pitch {:.3} mm, tap drill Ø{:.2} mm",
                    s.major, s.pitch, s.tap_drill
                ));
            }
            let mut full = t.length.is_none();
            ed.check(ui, &mut full, "Full length of the face");
            if full != t.length.is_none() {
                t.length = if full { None } else { Some(10.0) };
            }
            if let Some(l) = &mut t.length {
                ed.length(ui, "Length", l, 0.01);
            }
            ed.length(ui, "Offset from the start", &mut t.offset, 0.0);
            ed.check(ui, &mut t.flip, "Start from the other end");
            ed.check(ui, &mut t.right, "Right-handed");
            ui.weak(tr("A modelled ISO 68 thread: printable, and watertight. Rods are threaded outside, holes inside."));
        }
        FeatureKind::VariableFillet { edge, radii } => {
            ui.horizontal(|ui| {
                ui.label(tr("Edge"));
                if ed.button(ui, "Use selection", !p.edges.is_empty()) {
                    *edge = p.edges[0].clone();
                }
            });
            ui.label(tr("Radius at points along the edge (0 = start, 1 = end)"));
            let mut remove = None;
            for (i, r) in radii.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ed.track(ui.add(number(&mut r[0]).speed(0.01).range(0.0..=1.0).prefix("at ")));
                    ed.track(ui.add(length(&mut r[1]).speed(0.05).range(0.01..=1000.0)));
                    if ui.small_button("✖").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove.filter(|_| radii.len() > 2) {
                radii.remove(i);
                ed.changed = true;
            }
            if ed.button(ui, "Add a point", true) {
                let last = radii.last().map_or(1.0, |r| r[1]);
                radii.push([0.5, last]);
                radii.sort_by(|a, b| a[0].total_cmp(&b[0]));
            }
        }
        FeatureKind::FaceFillet { a, b, radius } => {
            faces_line(ed, ui, "faces in set 1", a, p);
            faces_line(ed, ui, "faces in set 2", b, p);
            ed.length(ui, "Radius", radius, 0.001);
            ui.weak(tr("Rounds every edge where a face of set 1 meets a face of set 2."));
        }
        FeatureKind::Combine { op, main, tools } => {
            egui::ComboBox::from_id_salt("combine_op")
                .selected_text(op.name())
                .show_ui(ui, |ui| {
                    for o in [CombineOp::Add, CombineOp::Subtract, CombineOp::Common] {
                        ed.track(ui.selectable_value(op, o, o.name()));
                    }
                });
            face_line(ed, ui, "Main body (pick one of its faces)", main, p);
            faces_line(ed, ui, "bodies to combine (one face each)", tools, p);
        }
        FeatureKind::Split { tool, bodies } => {
            let mut sketch = matches!(tool, SplitTool::Sketch(_));
            ui.horizontal(|ui| {
                ed.track(ui.radio_value(&mut sketch, false, tr("By a plane")));
                ed.track(ui.radio_value(&mut sketch, true, tr("By a sketch")));
            });
            if sketch != matches!(tool, SplitTool::Sketch(_)) {
                *tool = match (sketch, above(h, me, FeatureKind::is_sketch).first()) {
                    (true, Some(&(id, _))) => SplitTool::Sketch(id),
                    _ => SplitTool::Plane(PlaneRef::Std(StdPlane::Right)),
                };
            }
            match tool {
                SplitTool::Plane(pl) => plane_editor(ed, ui, h, pl, p, "Plane"),
                SplitTool::Sketch(id) => sketch_combo(ed, ui, "split_sketch", h, me, id),
            }
            bodies_line(ed, ui, bodies, p);
            ui.weak(tr("A sketch splits off what lies inside its outlines, straight through the part."));
        }
        FeatureKind::DeleteBody { bodies, keep } => {
            faces_line(ed, ui, "bodies (one face each)", bodies, p);
            ui.horizontal(|ui| {
                ed.track(ui.radio_value(keep, false, tr("Delete these")));
                ed.track(ui.radio_value(keep, true, tr("Keep only these")));
            });
        }
        FeatureKind::MoveBody(m) => {
            bodies_line(ed, ui, &mut m.bodies, p);
            ui.label(tr("Translate"));
            for (i, n) in ["ΔX", "ΔY", "ΔZ"].iter().enumerate() {
                signed(ed, ui, n, &mut m.translate[i]);
            }
            let mut rotate = m.rotate.is_some();
            ed.check(ui, &mut rotate, "Rotate");
            if rotate != m.rotate.is_some() {
                m.rotate = rotate.then(|| (AxisRef::Std(2), 90f64.to_radians()));
            }
            if let Some((axis, angle)) = &mut m.rotate {
                axis_editor(ed, ui, h, axis, p, "About");
                ed.angle(ui, "Angle", angle, -360.0..=360.0);
            }
            let mut mate = m.mate.is_some();
            ed.check(ui, &mut mate, "Mate a face against a plane");
            if mate != m.mate.is_some() {
                m.mate = match (mate, p.faces.first()) {
                    (true, Some(f)) => Some((f.clone(), PlaneRef::Std(StdPlane::Top))),
                    _ => None,
                };
                if mate && m.mate.is_none() {
                    ui.weak(tr("Select the flat face to mate first."));
                }
            }
            if let Some((face, plane)) = &mut m.mate {
                face_line(ed, ui, "Face", face, p);
                plane_editor(ed, ui, h, plane, p, "Against");
            }
            ed.check(ui, &mut m.copy, "Copy (keep the original)");
        }
        FeatureKind::ScaleBody { bodies, about, factors } => {
            bodies_line(ed, ui, bodies, p);
            ui.horizontal_wrapped(|ui| {
                ui.label(match about {
                    PointRef::Origin => "About: part origin".to_string(),
                    PointRef::Vertex(_) => "About: a vertex".to_string(),
                    PointRef::Feature(id) => format!("About: {}", name_of(h, *id)),
                });
                if ed.button(ui, "Use selection", p.point.is_some()) {
                    *about = p.point.clone().unwrap().0;
                }
                if ed.button(ui, "Origin", true) {
                    *about = PointRef::Origin;
                }
            });
            let mut uniform = factors[0] == factors[1] && factors[1] == factors[2];
            ed.check(ui, &mut uniform, "Uniform");
            let scale = |ed: &mut Ed, ui: &mut egui::Ui, label: &str, v: &mut f64| {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ed.track(ui.add(number(v).speed(0.01).range(0.001..=1000.0)));
                });
            };
            if uniform {
                let mut f = factors[0];
                scale(ed, ui, "Factor", &mut f);
                *factors = [f; 3];
            } else {
                for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
                    scale(ed, ui, n, &mut factors[i]);
                }
            }
        }
        FeatureKind::SketchPattern { seeds, sketch } => {
            seeds_editor(ed, ui, h, me, seeds);
            ui.label(tr("Points from"));
            sketch_combo(ed, ui, "pattern_sketch", h, me, sketch);
            ui.weak(tr("A copy goes at every sketch point, placed as the seed sits relative to its center."));
        }
        FeatureKind::CurvePattern {
            seeds,
            curve,
            count,
            spacing,
            align,
        } => {
            seeds_editor(ed, ui, h, me, seeds);
            ui.label(tr("Along"));
            curve_combo(ed, ui, "pattern_curve", h, me, curve);
            ed.count(ui, "Instances", count);
            let mut equal = spacing.is_none();
            ed.check(ui, &mut equal, "Equal spacing over the whole curve");
            if equal != spacing.is_none() {
                *spacing = if equal { None } else { Some(10.0) };
            }
            if let Some(s) = spacing {
                ed.length(ui, "Spacing", s, 0.001);
            }
            ed.check(ui, align, "Turn with the curve");
        }
        FeatureKind::Dome { face, height, reverse } => {
            face_line(ed, ui, "Flat face", face, p);
            ed.length(ui, "Height", height, 0.001);
            ed.check(ui, reverse, "Reverse (sink into the part)");
            ui.weak(tr("A round face gets a spherical cap; other faces a smooth elliptical one."));
        }
        FeatureKind::Wrap {
            sketch,
            face,
            kind,
            depth,
        } => {
            ui.label(tr("Sketch"));
            sketch_combo(ed, ui, "wrap_sketch", h, me, sketch);
            face_line(ed, ui, "Cylindrical face", face, p);
            ui.horizontal(|ui| {
                for k in [WrapKind::Emboss, WrapKind::Deboss, WrapKind::Scribe] {
                    ed.track(ui.radio_value(kind, k, k.name()));
                }
            });
            ed.length(ui, "Depth", depth, 0.001);
            ui.weak(tr("Sketch on a plane parallel to the cylinder's axis; the sketch rolls onto the surface without stretching."));
        }
        FeatureKind::Fit { faces, clearance } => {
            faces_line(ed, ui, "faces to move", faces, p);
            ed.length(ui, "Clearance", clearance, 0.001);
            ui.label(tr("Presets (per side)"));
            egui::Grid::new("fit_presets").show(ui, |ui| {
                ui.label("");
                for fit in ["Press", "Slip", "Loose"] {
                    ui.label(fit);
                }
                ui.end_row();
                for (process, values) in FIT_PRESETS {
                    ui.label(process);
                    for v in values {
                        if ed.button(ui, &crate::ui::properties::units().length(v), true) {
                            *clearance = v;
                        }
                    }
                    ui.end_row();
                }
            });
            ui.weak(tr("Holes grow and pins, tabs and slots' walls move in by the clearance. Fit one part of each pair."));
        }
        other => {
            ui.label(format!("{} has no properties to edit.", other.prefix()));
        }
    }
}

/// Clearance per side (mm) for press, slip and loose fits, by printing process and material.
const FIT_PRESETS: [(&str, [f64; 3]); 4] = [
    ("FDM: PLA, PETG", [0.05, 0.15, 0.3]),
    ("FDM: ABS, ASA, nylon", [0.1, 0.2, 0.35]),
    ("Resin (SLA, DLP)", [0.025, 0.075, 0.15]),
    ("Powder (SLS, MJF)", [0.1, 0.2, 0.3]),
];

/// A slip fit for the part's material.
fn slip_fit(material: &str) -> f64 {
    let m = material.to_ascii_lowercase();
    let row = if m.contains("resin") {
        2
    } else if ["abs", "asa", "nylon", "pc"].iter().any(|k| m.contains(k)) {
        1
    } else {
        0
    };
    FIT_PRESETS[row].1[1]
}

/// Sketches and helices picked for a command: the one selected in the feature tree first, then
/// those added with Ctrl+click.
fn picked(app: &App, keep: impl Fn(&FeatureKind) -> bool) -> Vec<FeatureId> {
    app.chosen_features()
        .into_iter()
        .filter(|id| app.doc.history().is_active(*id) && app.doc.history().get(*id).is_some_and(|f| keep(&f.kind)))
        .collect()
}

fn as_curve(app: &App, id: FeatureId) -> CurveRef {
    if app
        .doc
        .history()
        .get(id)
        .is_some_and(|f| matches!(f.kind, FeatureKind::Helix(_)))
    {
        CurveRef::Helix(id)
    } else {
        CurveRef::Sketch(id)
    }
}

/// Does the sketch hold a closed outline (a profile) rather than an open path?
fn is_profile(app: &App, id: FeatureId) -> bool {
    match app.doc.history().get(id).map(|f| &f.kind) {
        Some(FeatureKind::Sketch { sketch, .. }) => sketch.regions().is_ok_and(|r| !r.is_empty()),
        _ => false,
    }
}

fn create(app: &mut App, kind: FeatureKind) {
    if app.sketch.is_some() {
        app.exit_sketch(true);
    }
    match app.doc.edit(|h| h.add(kind)) {
        Ok(id) => {
            app.selected = Some(id);
            app.selection.clear();
            app.status = match app.doc.errors().get(&id) {
                Some(e) => e.clone(),
                None => "Ready".into(),
            };
        }
        Err(e) => app.status = e.to_string(),
    }
}

fn first_face(app: &App, pred: impl Fn(&threebcad_kernel::topology::TopoFace) -> bool) -> Option<FaceRef> {
    app.selection.iter().find_map(|r| match *r {
        ModelRef::Face { body, face } => {
            let f = app.doc.bodies().get(body)?.topo.faces.get(face)?;
            pred(f).then(|| app.doc.face_ref(body, face)).flatten()
        }
        _ => None,
    })
}

fn seeds(app: &App) -> Seeds {
    let h = app.doc.history();
    match app.selected {
        Some(id)
            if h.get(id)
                .is_some_and(|f| !f.kind.is_sketch() && !matches!(f.kind, FeatureKind::Helix(_))) =>
        {
            Seeds::Features(vec![id])
        }
        _ => Seeds::Bodies,
    }
}

fn small(ui: &mut egui::Ui, icon: &str, label: &str, tip: &str) -> bool {
    icons::small(ui, icon, label, true, false).on_hover_text(tip).clicked()
}

fn tile(ui: &mut egui::Ui, icon: &str, label: &str, tip: &str) -> bool {
    icons::tile(ui, icon, label, true, false).on_hover_text(tip).clicked()
}

fn stack(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        add(ui);
    });
}

/// The sweep and loft commands, next to Extrude and Revolve.
pub(crate) fn sweep_loft(app: &mut App, ui: &mut egui::Ui) {
    stack(ui, |ui| {
        if small(
            ui,
            "sweep",
            "Swept Boss/Base",
            "Select a closed profile sketch, then Ctrl+click a path sketch or helix (a path alone sweeps a circle)",
        ) {
            sweep(app, false);
        }
        if small(
            ui,
            "loft",
            "Lofted Boss/Base",
            "Select two or more profile sketches in order (Ctrl+click), then any guide curves",
        ) {
            loft(app, false);
        }
        if small(
            ui,
            "helix",
            "Helix/Spiral",
            "Select a sketch with a circle: the helix starts on it",
        ) {
            helix(app);
        }
    });
    stack(ui, |ui| {
        if small(ui, "sweep", "Swept Cut", "Like Swept Boss, removing material") {
            sweep(app, true);
        }
        if small(ui, "loft", "Lofted Cut", "Like Lofted Boss, removing material") {
            loft(app, true);
        }
    });
}

fn sweep(app: &mut App, cut: bool) {
    if app.sketch.is_some() {
        app.exit_sketch(true);
    }
    let chosen = picked(app, |k| k.is_sketch() || matches!(k, FeatureKind::Helix(_)));
    let (profile, path) = match chosen[..] {
        [one] if !is_profile(app, one) => (SweepProfile::Circle(5.0), as_curve(app, one)),
        [a, b, ..] if is_profile(app, a) => (SweepProfile::Sketch(a), as_curve(app, b)),
        [a, b, ..] if is_profile(app, b) => (SweepProfile::Sketch(b), as_curve(app, a)),
        _ => {
            app.status = tr("Sweep: select the profile sketch, then Ctrl+click the path (a sketch or a helix)").into();
            return;
        }
    };
    create(
        app,
        FeatureKind::Sweep(Sweep {
            profile,
            path,
            twist: 0.0,
            cut,
            merge: true,
        }),
    );
}

fn loft(app: &mut App, cut: bool) {
    if app.sketch.is_some() {
        app.exit_sketch(true);
    }
    let chosen = picked(app, FeatureKind::is_sketch);
    let (profiles, guides): (Vec<FeatureId>, Vec<FeatureId>) = chosen.into_iter().partition(|&id| is_profile(app, id));
    if profiles.len() < 2 {
        app.status = tr("Loft: select two or more closed profile sketches (Ctrl+click), in order").into();
        return;
    }
    create(
        app,
        FeatureKind::Loft(Loft {
            profiles,
            guides,
            start_tangent: 0.0,
            end_tangent: 0.0,
            cut,
            merge: true,
        }),
    );
}

fn helix(app: &mut App) {
    if app.sketch.is_some() {
        app.exit_sketch(true);
    }
    let Some(&s) = picked(app, FeatureKind::is_sketch).first() else {
        app.status = tr("Helix: select a sketch with a circle first").into();
        return;
    };
    create(
        app,
        FeatureKind::Helix(HelixDef {
            sketch: s,
            size: HelixSize::PitchTurns { pitch: 5.0, turns: 4.0 },
            taper: 0.0,
            ccw: true,
            start: 0.0,
            reverse: false,
        }),
    );
}

/// The fillet variants, thread, dome and wrap.
pub(crate) fn shaping(app: &mut App, ui: &mut egui::Ui) {
    stack(ui, |ui| {
        if small(
            ui,
            "variable_fillet",
            "Variable Fillet",
            "Select one straight edge: its radius runs from 1 to 3 mm",
        ) {
            let Some(edge) = app.selected_edges(false).into_iter().next() else {
                app.status = tr("Variable Fillet: select a straight edge first").into();
                return;
            };
            create(
                app,
                FeatureKind::VariableFillet {
                    edge,
                    radii: vec![[0.0, 1.0], [1.0, 3.0]],
                },
            );
        }
        if small(
            ui,
            "face_fillet",
            "Face Fillet",
            "Select faces: the first is set 1, the rest set 2; rounds where the sets meet",
        ) {
            let faces = app.selected_faces();
            if faces.len() < 2 {
                app.status = tr("Face Fillet: select two faces or more (Ctrl+click)").into();
                return;
            }
            create(
                app,
                FeatureKind::FaceFillet {
                    a: vec![faces[0].clone()],
                    b: faces[1..].to_vec(),
                    radius: 2.0,
                },
            );
        }
    });
    stack(ui, |ui| {
        if small(
            ui,
            "thread",
            "Thread",
            "Select a cylindrical face: a rod gets an outside thread, a hole an inside one",
        ) {
            let Some(face) = first_face(app, |f| f.cylinder.is_some()) else {
                app.status = tr("Thread: select a cylindrical face (a rod or a hole) first").into();
                return;
            };
            // The nearest standard size to the cylinder.
            let r = app
                .selection
                .iter()
                .find_map(|r| match *r {
                    ModelRef::Face { body, face } => app.doc.bodies().get(body)?.topo.faces.get(face)?.cylinder,
                    _ => None,
                })
                .map_or(3.0, |c| c.2);
            let size = holes::table(HoleStandard::Iso)
                .iter()
                .min_by(|a, b| {
                    let fit = |s: &holes::HoleSize| (s.major / 2.0 - r).abs().min((s.tap_drill / 2.0 - r).abs());
                    fit(a).total_cmp(&fit(b))
                })
                .map_or("M6".to_string(), |s| s.name.to_string());
            create(
                app,
                FeatureKind::Thread(ThreadDef {
                    face,
                    standard: HoleStandard::Iso,
                    size,
                    length: None,
                    offset: 0.0,
                    flip: false,
                    right: true,
                }),
            );
        }
        if small(ui, "dome", "Dome", "Select a flat face to raise into a smooth cap") {
            let Some(face) = first_face(app, |f| f.plane.is_some()) else {
                app.status = tr("Dome: select a flat face first").into();
                return;
            };
            create(
                app,
                FeatureKind::Dome {
                    face,
                    height: 2.0,
                    reverse: false,
                },
            );
        }
        if small(
            ui,
            "wrap",
            "Wrap",
            "Select a sketch (on a plane parallel to the axis), then Ctrl+click a cylindrical face",
        ) {
            let sketch = picked(app, FeatureKind::is_sketch).first().copied();
            let face = first_face(app, |f| f.cylinder.is_some());
            match (sketch, face) {
                (Some(sketch), Some(face)) => create(
                    app,
                    FeatureKind::Wrap {
                        sketch,
                        face,
                        kind: WrapKind::Emboss,
                        depth: 1.0,
                    },
                ),
                _ => app.status = tr("Wrap: select a sketch, then Ctrl+click the cylindrical face to wrap onto").into(),
            }
        }
    });
    stack(ui, |ui| {
        if small(
            ui,
            "fit",
            "Fit",
            "Select the faces of a hole, pin, tab or slot: they move in by a press, slip or loose clearance",
        ) {
            let faces = app.selected_faces();
            if faces.is_empty() {
                app.status = tr("Fit: select the faces of a hole, pin, tab or slot (Ctrl+click for more)").into();
                return;
            }
            let clearance = slip_fit(&app.doc.material().name);
            create(app, FeatureKind::Fit { faces, clearance });
        }
    });
}

/// Patterns driven by a sketch's points or along a curve.
pub(crate) fn patterns(app: &mut App, ui: &mut egui::Ui) {
    stack(ui, |ui| {
        if small(
            ui,
            "sketch_pattern",
            "Sketch Pattern",
            "Select the feature to repeat, then Ctrl+click a sketch of points",
        ) {
            let seeds = seeds(app);
            let Some(&sketch) = picked(app, FeatureKind::is_sketch).first() else {
                app.status = tr("Sketch Pattern: select the feature, then Ctrl+click a sketch of points").into();
                return;
            };
            create(app, FeatureKind::SketchPattern { seeds, sketch });
        }
        if small(
            ui,
            "curve_pattern",
            "Curve Pattern",
            "Select the feature to repeat, then Ctrl+click a sketch curve or helix",
        ) {
            let seeds = seeds(app);
            let Some(&c) = picked(app, |k| k.is_sketch() || matches!(k, FeatureKind::Helix(_))).first() else {
                app.status = tr("Curve Pattern: select the feature, then Ctrl+click a curve (sketch or helix)").into();
                return;
            };
            let curve = as_curve(app, c);
            create(
                app,
                FeatureKind::CurvePattern {
                    seeds,
                    curve,
                    count: 4,
                    spacing: None,
                    align: false,
                },
            );
        }
    });
}

/// Multibody tools.
pub(crate) fn bodies(app: &mut App, ui: &mut egui::Ui) {
    if tile(
        ui,
        "combine",
        "Combine",
        "Select a face of the main body, then Ctrl+click a face of each other body",
    ) {
        let faces = app.selected_faces();
        if faces.len() < 2 {
            app.status = tr("Combine: select a face on each body (Ctrl+click), the main body first").into();
        } else {
            create(
                app,
                FeatureKind::Combine {
                    op: CombineOp::Add,
                    main: faces[0].clone(),
                    tools: faces[1..].to_vec(),
                },
            );
        }
    }
    stack(ui, |ui| {
        if small(
            ui,
            "split",
            "Split",
            "Split the bodies by a selected plane or sketch (default Right Plane)",
        ) {
            let sketch = picked(app, FeatureKind::is_sketch).first().copied();
            let plane = app.selection.iter().find_map(|&r| app.plane_ref_of(r).map(|x| x.1));
            let tool = match (sketch, plane) {
                (Some(s), _) => SplitTool::Sketch(s),
                (None, Some(p)) => SplitTool::Plane(p),
                _ => SplitTool::Plane(PlaneRef::Std(StdPlane::Right)),
            };
            create(
                app,
                FeatureKind::Split {
                    tool,
                    bodies: Vec::new(),
                },
            );
        }
        if small(
            ui,
            "delete_body",
            "Delete/Keep Body",
            "Select a face of each body to delete",
        ) {
            let faces = app.selected_faces();
            if faces.is_empty() {
                app.status = tr("Delete/Keep Body: select a face on each body first").into();
            } else {
                create(
                    app,
                    FeatureKind::DeleteBody {
                        bodies: faces,
                        keep: false,
                    },
                );
            }
        }
        if small(
            ui,
            "move_body",
            "Move/Copy Body",
            "Move the selected bodies (or all) by values, a rotation or a mate",
        ) {
            let bodies = app.selected_faces();
            create(
                app,
                FeatureKind::MoveBody(MoveBody {
                    bodies,
                    translate: [10.0, 0.0, 0.0],
                    rotate: None,
                    mate: None,
                    copy: false,
                }),
            );
        }
    });
    stack(ui, |ui| {
        if small(
            ui,
            "scale",
            "Scale",
            "Scale the selected bodies (or all) about the origin",
        ) {
            let bodies = app.selected_faces();
            create(
                app,
                FeatureKind::ScaleBody {
                    bodies,
                    about: PointRef::Origin,
                    factors: [1.5; 3],
                },
            );
        }
    });
}
