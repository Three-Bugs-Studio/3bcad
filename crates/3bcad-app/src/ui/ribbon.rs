//! CommandManager ribbon. Each command creates its feature from the current selection with
//! sensible defaults; the PropertyManager then edits it.

use eframe::egui;
use threebcad_core::*;
use threebcad_io::Slicer;
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

use crate::app::{App, ExportFormat, ModelRef, RELEASE_NOTES_URL, VERSION};
use crate::ui::sketch_mode::Tool;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RibbonTab {
    Features,
    Sketch,
    Evaluate,
    Print,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::top("ribbon").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.strong("3BCAD");
            ui.hyperlink_to(format!("v{VERSION}"), RELEASE_NOTES_URL)
                .on_hover_text("What's new in this version");
            ui.separator();
            ui.selectable_value(&mut app.tab, RibbonTab::Features, "Features");
            ui.selectable_value(&mut app.tab, RibbonTab::Sketch, "Sketch");
            ui.selectable_value(&mut app.tab, RibbonTab::Evaluate, "Evaluate");
            ui.selectable_value(&mut app.tab, RibbonTab::Print, "3D Print");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (can_undo, can_redo) = (
                    app.doc.can_undo() || app.sketch.is_some(),
                    app.doc.can_redo() || app.sketch.is_some(),
                );
                if ui
                    .add_enabled(can_redo, egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Y")
                    .clicked()
                {
                    match &mut app.sketch {
                        Some(s) => s.redo(),
                        None => app.doc.redo(),
                    }
                }
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    match &mut app.sketch {
                        Some(s) => s.undo(),
                        None => app.doc.undo(),
                    }
                }
            });
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| match app.tab {
            RibbonTab::Features => features(app, ui),
            RibbonTab::Sketch => sketch(app, ui),
            RibbonTab::Evaluate => evaluate(app, ui),
            RibbonTab::Print => print(app, ui),
        });
        ui.add_space(4.0);
    });
}

/// The sketch a sketch-based feature would use: the open one (closed first) or the selected one.
fn source_sketch(app: &mut App) -> Option<FeatureId> {
    if app.sketch.is_some() {
        app.exit_sketch(true);
    }
    let h = app.doc.history();
    app.selected
        .filter(|&id| h.is_active(id) && h.get(id).is_some_and(|f| f.kind.is_sketch()))
}

/// Adds a feature, selects it and reports errors in the status bar.
fn create(app: &mut App, kind: FeatureKind) {
    match app.doc.edit(|h| h.add(kind)) {
        Ok(id) => {
            app.selected = Some(id);
            app.selection.clear();
            if let Some(e) = app.doc.errors().get(&id) {
                app.status = e.clone();
            }
        }
        Err(e) => app.status = e.to_string(),
    }
}

fn sketch_feature(app: &mut App, make: impl FnOnce(FeatureId) -> FeatureKind) {
    match source_sketch(app) {
        Some(s) => create(app, make(s)),
        None => app.status = "Select a sketch (in the FeatureManager or the graphics area) first".into(),
    }
}

/// Seeds for a pattern or mirror: the selected feature, or every body when nothing is selected.
fn seeds(app: &App) -> Seeds {
    match app.selected {
        Some(id) if !app.doc.history().get(id).is_some_and(|f| f.kind.is_sketch()) => Seeds::Features(vec![id]),
        _ => Seeds::Bodies,
    }
}

fn first_axis(app: &App) -> Option<AxisRef> {
    app.selection.iter().find_map(|&r| app.axis_ref_of(r))
}

fn first_plane(app: &App) -> Option<PlaneRef> {
    app.selection.iter().find_map(|&r| app.plane_ref_of(r).map(|(_, p)| p))
}

fn button(ui: &mut egui::Ui, label: &str, tip: &str) -> bool {
    ui.button(label).on_hover_text(tip).clicked()
}

fn features(app: &mut App, ui: &mut egui::Ui) {
    let has_sketch = app.sketch.is_some()
        || app
            .selected
            .is_some_and(|id| app.doc.history().get(id).is_some_and(|f| f.kind.is_sketch()));
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(has_sketch, egui::Button::new("⬆ Extruded Boss/Base"))
                .on_disabled_hover_text("Select a sketch first")
                .clicked()
            {
                sketch_feature(app, |s| FeatureKind::Extrude(Extrude::boss(s, 10.0)));
            }
            if ui
                .add_enabled(has_sketch, egui::Button::new("↻ Revolved Boss/Base"))
                .on_disabled_hover_text("Select a sketch with a centerline first")
                .clicked()
            {
                sketch_feature(app, |s| FeatureKind::Revolve(Revolve::new(s, false)));
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(has_sketch, egui::Button::new("⬇ Extruded Cut"))
                .on_disabled_hover_text("Select a sketch first")
                .clicked()
            {
                sketch_feature(app, |s| FeatureKind::Extrude(Extrude::cut(s, 10.0)));
            }
            if ui
                .add_enabled(has_sketch, egui::Button::new("↺ Revolved Cut"))
                .on_disabled_hover_text("Select a sketch with a centerline first")
                .clicked()
            {
                sketch_feature(app, |s| FeatureKind::Revolve(Revolve::new(s, true)));
            }
        });
    });
    ui.separator();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if button(
                ui,
                "○ Fillet",
                "Round the selected edges (or every edge of selected faces)",
            ) {
                let edges = app.selected_edges(true);
                if edges.is_empty() {
                    app.status = "Fillet: select edges or faces first".into();
                } else {
                    create(
                        app,
                        FeatureKind::Fillet {
                            edges,
                            radius: 2.0,
                            propagate: true,
                        },
                    );
                }
            }
            if button(
                ui,
                "◊ Chamfer",
                "Bevel the selected edges (or every edge of selected faces)",
            ) {
                let edges = app.selected_edges(true);
                if edges.is_empty() {
                    app.status = "Chamfer: select edges or faces first".into();
                } else {
                    create(
                        app,
                        FeatureKind::Chamfer {
                            edges,
                            kind: ChamferKind::EqualDistance,
                            d1: 1.0,
                            d2: 1.0,
                            angle: 45f64.to_radians(),
                            flip: false,
                        },
                    );
                }
            }
            if button(ui, "Shell", "Hollow the body; the selected faces are removed") {
                let faces = app.selected_faces();
                create(
                    app,
                    FeatureKind::Shell {
                        faces,
                        thickness: 1.0,
                        outward: false,
                    },
                );
            }
        });
        ui.horizontal(|ui| {
            if button(
                ui,
                "Hole Wizard",
                "Select a sketch of points, or a flat face (a hole goes at its center)",
            ) {
                hole_wizard(app);
            }
            if button(
                ui,
                "Draft",
                "Tilt the selected side walls about the Top Plane (pick the neutral plane after)",
            ) {
                let faces = app.selected_faces();
                if faces.is_empty() {
                    app.status = "Draft: select the side faces to draft".into();
                } else {
                    let neutral = app
                        .selection
                        .iter()
                        .find_map(|&r| match r {
                            ModelRef::Plane(p) => Some(PlaneRef::Std(p)),
                            ModelRef::Feature(_) => app.plane_ref_of(r).map(|x| x.1),
                            _ => None,
                        })
                        .unwrap_or(PlaneRef::Std(StdPlane::Top));
                    create(
                        app,
                        FeatureKind::Draft {
                            neutral,
                            faces,
                            angle: 3f64.to_radians(),
                        },
                    );
                }
            }
            if ui
                .add_enabled(has_sketch, egui::Button::new("Rib"))
                .on_hover_text("Select a sketch with an open line between walls")
                .clicked()
            {
                sketch_feature(app, |s| FeatureKind::Rib {
                    sketch: s,
                    thickness: 2.0,
                    both_sides: true,
                    flip: false,
                });
            }
        });
    });
    ui.separator();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if button(
                ui,
                "Linear Pattern",
                "Repeat the selected feature (or all bodies) along an edge/axis (default X)",
            ) {
                let axis = first_axis(app).unwrap_or(AxisRef::Std(0));
                let seeds = seeds(app);
                create(
                    app,
                    FeatureKind::LinearPattern(LinearPattern {
                        seeds,
                        dir1: PatternDir {
                            axis,
                            spacing: 20.0,
                            count: 3,
                            reverse: false,
                        },
                        dir2: None,
                        skip: Vec::new(),
                    }),
                );
            }
            if button(
                ui,
                "Circular Pattern",
                "Repeat the selected feature (or all bodies) around an axis (default Z)",
            ) {
                let axis = first_axis(app).unwrap_or(AxisRef::Std(2));
                let seeds = seeds(app);
                create(
                    app,
                    FeatureKind::CircularPattern(CircularPattern {
                        seeds,
                        axis,
                        count: 4,
                        angle: std::f64::consts::TAU,
                        equal: true,
                        reverse: false,
                        skip: Vec::new(),
                    }),
                );
            }
        });
        ui.horizontal(|ui| {
            if button(
                ui,
                "↔ Mirror",
                "Mirror the selected feature (or all bodies) about a selected plane (default Right)",
            ) {
                let plane = first_plane(app).unwrap_or(PlaneRef::Std(StdPlane::Right));
                let seeds = seeds(app);
                create(
                    app,
                    FeatureKind::Mirror(Mirror {
                        seeds,
                        plane,
                        merge: true,
                    }),
                );
            }
            ui.menu_button("Reference Geometry", |ui| {
                if ui
                    .button("Plane")
                    .on_hover_text("Offset from a plane/face, 3 vertices, two planes (mid) or plane + edge (angle)")
                    .clicked()
                {
                    ref_plane(app);
                    ui.close_menu();
                }
                if ui
                    .button("Axis")
                    .on_hover_text("From an edge, a cylinder, two vertices or two planes")
                    .clicked()
                {
                    ref_axis(app);
                    ui.close_menu();
                }
                if ui
                    .button("Point")
                    .on_hover_text("At a vertex, a face center or an arc center")
                    .clicked()
                {
                    ref_point(app);
                    ui.close_menu();
                }
                if ui
                    .button("Coordinate System")
                    .on_hover_text("At a selected vertex (or the origin)")
                    .clicked()
                {
                    let origin = app
                        .selection
                        .iter()
                        .find_map(|&r| app.point_ref_of(r))
                        .unwrap_or(PointRef::Origin);
                    create(
                        app,
                        FeatureKind::CoordSys(CoordSysDef {
                            origin,
                            x: None,
                            z: None,
                        }),
                    );
                    ui.close_menu();
                }
            });
        });
    });
    ui.separator();
    ui.vertical(|ui| {
        if ui
            .button("Box")
            .on_hover_text("Rectangle sketch + extrude on the Top Plane")
            .clicked()
        {
            add_primitive(app, Sketch::rectangle(20.0, 20.0), 20.0);
        }
        if ui
            .button("Cylinder")
            .on_hover_text("Circle sketch + extrude on the Top Plane")
            .clicked()
        {
            add_primitive(app, Sketch::circle([0.0, 0.0], 10.0), 20.0);
        }
    });
}

fn hole_wizard(app: &mut App) {
    if let Some(s) = source_sketch(app) {
        create(
            app,
            FeatureKind::Hole(Hole {
                sketch: s,
                kind: HoleKind::Simple,
                standard: HoleStandard::Iso,
                size: "M6".into(),
                end: End::ThroughAll,
            }),
        );
        return;
    }
    let Some((plane, on, center)) = app.selection.iter().find_map(|&r| {
        let ModelRef::Face { body, face } = r else { return None };
        let (plane, on) = app.plane_ref_of(r)?;
        let c = app.doc.bodies().get(body)?.topo.faces.get(face)?.center();
        Some((plane, on, plane.to_local(c)))
    }) else {
        app.status = "Hole Wizard: select a flat face or a sketch of points".into();
        return;
    };
    let mut pts = Sketch::default();
    pts.add_point(center[0], center[1]);
    let r = app.doc.edit(|h| -> anyhow::Result<FeatureId> {
        let s = h.add_sketch_on(pts, plane, on)?;
        h.add(FeatureKind::Hole(Hole {
            sketch: s,
            kind: HoleKind::Simple,
            standard: HoleStandard::Iso,
            size: "M6".into(),
            end: End::ThroughAll,
        }))
    });
    match r {
        Ok(id) => {
            app.selected = Some(id);
            app.selection.clear();
            app.status = "Hole placed at the face center; edit its sketch to move or add holes".into();
        }
        Err(e) => app.status = e.to_string(),
    }
}

fn ref_plane(app: &mut App) {
    let planes: Vec<PlaneRef> = app
        .selection
        .iter()
        .filter_map(|&r| app.plane_ref_of(r).map(|x| x.1))
        .collect();
    let points: Vec<PointRef> = app.selection.iter().filter_map(|&r| app.point_ref_of(r)).collect();
    let edge = app.selection.iter().find_map(|&r| match r {
        ModelRef::Edge { .. } | ModelRef::Feature(_) => app.axis_ref_of(r),
        _ => None,
    });
    let def = if points.len() >= 3 {
        RefPlaneDef::ThreePoint([points[0].clone(), points[1].clone(), points[2].clone()])
    } else if planes.len() >= 2 {
        RefPlaneDef::Mid(planes[0].clone(), planes[1].clone())
    } else if let (Some(base), Some(axis)) = (planes.first(), edge) {
        RefPlaneDef::Angle {
            base: base.clone(),
            axis,
            angle: 45f64.to_radians(),
        }
    } else {
        RefPlaneDef::Offset {
            base: planes.first().cloned().unwrap_or(PlaneRef::Std(StdPlane::Top)),
            distance: 10.0,
            flip: false,
        }
    };
    create(app, FeatureKind::RefPlane(def));
}

fn ref_axis(app: &mut App) {
    let planes: Vec<PlaneRef> = app
        .selection
        .iter()
        .filter_map(|&r| app.plane_ref_of(r).map(|x| x.1))
        .collect();
    let points: Vec<PointRef> = app.selection.iter().filter_map(|&r| app.point_ref_of(r)).collect();
    let def = app.selection.iter().find_map(|&r| match r {
        ModelRef::Edge { body, edge } => app.doc.edge_ref(body, edge).map(RefAxisDef::Edge),
        ModelRef::Face { body, face } => {
            let cyl = app.doc.bodies().get(body)?.topo.faces.get(face)?.cylinder.is_some();
            cyl.then(|| app.doc.face_ref(body, face).map(RefAxisDef::Cylinder))
                .flatten()
        }
        _ => None,
    });
    let def = def
        .or_else(|| (points.len() >= 2).then(|| RefAxisDef::TwoPoints(points[0].clone(), points[1].clone())))
        .or_else(|| (planes.len() >= 2).then(|| RefAxisDef::TwoPlanes(planes[0].clone(), planes[1].clone())));
    match def {
        Some(d) => create(app, FeatureKind::RefAxis(d)),
        None => app.status = "Axis: select an edge, a cylindrical face, two vertices or two planes".into(),
    }
}

fn ref_point(app: &mut App) {
    let def = app.selection.iter().find_map(|&r| match r {
        ModelRef::Vertex { body, vertex } => app.doc.vertex_ref(body, vertex).map(RefPointDef::Vertex),
        ModelRef::Face { body, face } => app.doc.face_ref(body, face).map(RefPointDef::FaceCenter),
        ModelRef::Edge { body, edge } => app.doc.edge_ref(body, edge).map(RefPointDef::ArcCenter),
        _ => None,
    });
    create(app, FeatureKind::RefPoint(def.unwrap_or(RefPointDef::Coords([0.0; 3]))));
}

fn add_primitive(app: &mut App, sketch: Sketch, depth: f64) {
    if app.sketch.is_some() {
        app.exit_sketch(true);
    }
    let r = app.doc.edit(|h| {
        let s = h.add_sketch(sketch, Plane::TOP);
        h.add_extrude(s, depth)
    });
    match r {
        Ok(id) => app.selected = Some(id),
        Err(e) => app.status = e.to_string(),
    }
}

const DRAW_TOOLS: &[&[Tool]] = &[
    &[Tool::Line, Tool::Centerline],
    &[Tool::CornerRect, Tool::CenterRect, Tool::Rect3Pt],
    &[Tool::Circle, Tool::PerimeterCircle],
    &[Tool::CenterArc, Tool::TangentArc, Tool::Arc3Pt],
    &[Tool::Spline, Tool::Slot, Tool::CenterSlot, Tool::Polygon, Tool::Point],
];

const EDIT_TOOLS: &[&[Tool]] = &[
    &[Tool::Fillet, Tool::Chamfer],
    &[Tool::Trim, Tool::Extend, Tool::Offset],
    &[Tool::Mirror, Tool::LinearPattern, Tool::CircularPattern],
];

fn sketch(app: &mut App, ui: &mut egui::Ui) {
    if app.sketch.is_none() {
        if ui
            .button("✏ Sketch")
            .on_hover_text("Select a plane or planar face (or a sketch to edit), then click")
            .clicked()
        {
            app.start_sketch();
        }
        ui.weak(
            "Pick the Front, Top or Right plane, a reference plane or a flat face in the FeatureManager or the graphics area first.",
        );
        return;
    }
    if ui.button("✔ Exit Sketch").clicked() {
        app.exit_sketch(true);
        return;
    }
    if ui
        .button("Convert Entities")
        .on_hover_text("Copy the edges of the face this sketch is on into the sketch")
        .clicked()
    {
        app.convert_entities();
    }
    let Some(session) = &mut app.sketch else { return };
    ui.separator();
    let groups = std::iter::once(&[Tool::Select, Tool::SmartDimension][..])
        .chain(DRAW_TOOLS.iter().copied())
        .chain(EDIT_TOOLS.iter().copied());
    for tools in groups {
        for &t in tools {
            if ui.selectable_label(session.tool == t, t.label()).clicked() {
                session.set_tool(t);
            }
        }
        ui.separator();
    }
    ui.checkbox(&mut session.opts.show_relations, "Display relations");
}

fn evaluate(app: &mut App, ui: &mut egui::Ui) {
    if button(ui, "Measure", "Distances, angles, radii and lengths of the selection") {
        app.show_measure = true;
    }
    if button(ui, "Mass Properties", "Volume, mass, center of mass and moments") {
        app.show_mass = true;
    }
    if button(ui, "What's Wrong", "Rebuild errors and warnings") {
        app.show_whats_wrong = true;
    }
    ui.separator();
    if button(ui, "↻ Rebuild", "Rebuild every feature from scratch (Ctrl+Q)") {
        app.doc.force_rebuild();
        app.status = format!("Rebuilt {} features", app.doc.built().evaluated);
    }
}

fn print(app: &mut App, ui: &mut egui::Ui) {
    if ui.button("Export STL").clicked() {
        app.export(ExportFormat::Stl);
    }
    if ui.button("Export 3MF").clicked() {
        app.export(ExportFormat::ThreeMf);
    }
    if ui.button("Export STEP").clicked() {
        app.export(ExportFormat::Step);
    }
    ui.separator();
    ui.label("Chord tolerance");
    ui.add(
        egui::DragValue::new(&mut app.export_tol)
            .speed(0.001)
            .range(0.001..=1.0)
            .suffix(" mm"),
    );
    ui.separator();
    egui::ComboBox::from_id_salt("slicer")
        .selected_text(app.slicer.name())
        .show_ui(ui, |ui| {
            for s in Slicer::ALL {
                ui.selectable_value(&mut app.slicer, s, s.name());
            }
        });
    if ui.button("Send to Slicer").clicked() {
        app.send_to_slicer();
    }
}
