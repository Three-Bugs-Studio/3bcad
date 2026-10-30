//! CommandManager ribbon. Each command creates its feature from the current selection with
//! sensible defaults; the PropertyManager then edits it.

use eframe::egui;
use threebcad_core::*;
use threebcad_io::Slicer;
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

use crate::app::{App, ExportFormat, ModelRef, RELEASE_NOTES_URL, VERSION};
use crate::ui::sketch_mode::Tool;
use crate::ui::{icons, theme};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RibbonTab {
    Features,
    Sketch,
    Evaluate,
    Print,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::top("ribbon")
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::symmetric(6, 3)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("3BCAD");
                ui.hyperlink_to(format!("v{VERSION}"), RELEASE_NOTES_URL)
                    .on_hover_text("What's new in this version");
                ui.separator();
                ui.label(&app.doc.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::small(ui, "help", "Shortcuts", true, app.show_shortcuts)
                        .on_hover_text("Keyboard shortcuts (F1)")
                        .clicked()
                    {
                        app.show_shortcuts = !app.show_shortcuts;
                    }
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
            // CommandManager: the current tab's commands, then the tabs underneath, like SolidWorks.
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(250, 250, 250))
                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(210, 210, 210)))
                .corner_radius(3)
                .inner_margin(egui::Margin::symmetric(4, 3))
                .show(ui, |ui| {
                    ui.set_min_height(64.0);
                    ui.horizontal_wrapped(|ui| match app.tab {
                        RibbonTab::Features => features(app, ui),
                        RibbonTab::Sketch => sketch(app, ui),
                        RibbonTab::Evaluate => evaluate(app, ui),
                        RibbonTab::Print => print(app, ui),
                    });
                });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (t, label) in [
                    (RibbonTab::Features, "Features"),
                    (RibbonTab::Sketch, "Sketch"),
                    (RibbonTab::Evaluate, "Evaluate"),
                    (RibbonTab::Print, "3D Print"),
                ] {
                    if icons::tab(ui, label, app.tab == t).clicked() {
                        app.tab = t;
                    }
                }
            });
        });
}

/// A CommandManager tile; true when clicked.
fn tile(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool, tip: &str) -> bool {
    icons::tile(ui, icon, label, enabled, false)
        .on_hover_text(tip)
        .clicked()
}

/// A small one-line command; true when clicked.
fn small(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool, tip: &str) -> bool {
    icons::small(ui, icon, label, enabled, false)
        .on_hover_text(tip)
        .clicked()
}

/// Commands stacked three high, like SolidWorks' small CommandManager buttons.
fn stack(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        add(ui);
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

fn features(app: &mut App, ui: &mut egui::Ui) {
    let has_sketch = app.sketch.is_some()
        || app
            .selected
            .is_some_and(|id| app.doc.history().get(id).is_some_and(|f| f.kind.is_sketch()));
    let need_sketch = |tip: &'static str| if has_sketch { tip } else { "Select a sketch first" };
    if tile(
        ui,
        "extrude",
        "Extruded Boss/Base",
        has_sketch,
        need_sketch("Extrude the selected sketch"),
    ) {
        sketch_feature(app, |s| FeatureKind::Extrude(Extrude::boss(s, 10.0)));
    }
    if tile(
        ui,
        "revolve",
        "Revolved Boss/Base",
        has_sketch,
        need_sketch("Revolve the selected sketch about its centerline"),
    ) {
        sketch_feature(app, |s| FeatureKind::Revolve(Revolve::new(s, false)));
    }
    ui.separator();
    if tile(
        ui,
        "cut",
        "Extruded Cut",
        has_sketch,
        need_sketch("Cut the selected sketch into the part"),
    ) {
        sketch_feature(app, |s| FeatureKind::Extrude(Extrude::cut(s, 10.0)));
    }
    if tile(
        ui,
        "hole",
        "Hole Wizard",
        true,
        "Select a sketch of points, or a flat face (a hole goes at its center)",
    ) {
        hole_wizard(app);
    }
    if tile(
        ui,
        "revolve_cut",
        "Revolved Cut",
        has_sketch,
        need_sketch("Cut by revolving the selected sketch about its centerline"),
    ) {
        sketch_feature(app, |s| FeatureKind::Revolve(Revolve::new(s, true)));
    }
    ui.separator();
    if tile(
        ui,
        "fillet",
        "Fillet",
        true,
        "Round the selected edges (or every edge of selected faces)",
    ) {
        let edges = app.selected_edges(true);
        if edges.is_empty() {
            app.status = "Fillet: select edges or faces first (Ctrl+click for more)".into();
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
    stack(ui, |ui| {
        if small(
            ui,
            "chamfer",
            "Chamfer",
            true,
            "Bevel the selected edges (or every edge of selected faces)",
        ) {
            let edges = app.selected_edges(true);
            if edges.is_empty() {
                app.status = "Chamfer: select edges or faces first (Ctrl+click for more)".into();
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
        if small(
            ui,
            "shell",
            "Shell",
            true,
            "Hollow the body; the selected faces are removed",
        ) {
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
        if small(
            ui,
            "draft",
            "Draft",
            true,
            "Tilt the selected side walls about the Top Plane (or a selected plane)",
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
    });
    stack(ui, |ui| {
        if small(
            ui,
            "rib",
            "Rib",
            has_sketch,
            need_sketch("Grow a wall from an open sketch line"),
        ) {
            sketch_feature(app, |s| FeatureKind::Rib {
                sketch: s,
                thickness: 2.0,
                both_sides: true,
                flip: false,
            });
        }
    });
    ui.separator();
    if tile(
        ui,
        "lpattern",
        "Linear Pattern",
        true,
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
    stack(ui, |ui| {
        if small(
            ui,
            "cpattern",
            "Circular Pattern",
            true,
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
        if small(
            ui,
            "mirror",
            "Mirror",
            true,
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
    });
    ui.separator();
    stack(ui, |ui| {
        if small(
            ui,
            "plane",
            "Plane",
            true,
            "Offset from a plane/face, 3 vertices, two planes (mid) or plane + edge (angle)",
        ) {
            ref_plane(app);
        }
        if small(
            ui,
            "axis",
            "Axis",
            true,
            "From an edge, a cylinder, two vertices or two planes",
        ) {
            ref_axis(app);
        }
        if small(
            ui,
            "point",
            "Point",
            true,
            "At a vertex, a face center or an arc center",
        ) {
            ref_point(app);
        }
    });
    stack(ui, |ui| {
        if small(
            ui,
            "csys",
            "Coordinate System",
            true,
            "At a selected vertex (or the origin)",
        ) {
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
        }
    });
    ui.separator();
    stack(ui, |ui| {
        if small(ui, "box", "Box", true, "Rectangle sketch + extrude on the Top Plane") {
            add_primitive(app, Sketch::rectangle(20.0, 20.0), 20.0);
        }
        if small(
            ui,
            "cylinder",
            "Cylinder",
            true,
            "Circle sketch + extrude on the Top Plane",
        ) {
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

/// Sketch tools in columns of up to three, grouped like SolidWorks' Sketch tab.
const TOOL_COLUMNS: &[&[Tool]] = &[
    &[Tool::Line, Tool::Centerline, Tool::Point],
    &[Tool::CornerRect, Tool::CenterRect, Tool::Rect3Pt],
    &[Tool::Circle, Tool::PerimeterCircle, Tool::Polygon],
    &[Tool::CenterArc, Tool::TangentArc, Tool::Arc3Pt],
    &[Tool::Spline, Tool::Slot, Tool::CenterSlot],
    &[Tool::Fillet, Tool::Chamfer, Tool::Offset],
    &[Tool::Trim, Tool::Extend, Tool::Mirror],
    &[Tool::LinearPattern, Tool::CircularPattern],
];

fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => "select",
        Tool::SmartDimension => "dimension",
        Tool::Line => "line",
        Tool::Centerline => "centerline",
        Tool::CornerRect => "rect",
        Tool::CenterRect => "rect_center",
        Tool::Rect3Pt => "rect_3pt",
        Tool::Circle => "circle",
        Tool::PerimeterCircle => "circle_perim",
        Tool::CenterArc => "arc",
        Tool::TangentArc => "arc_tangent",
        Tool::Arc3Pt => "arc_3pt",
        Tool::Spline => "spline",
        Tool::Slot | Tool::CenterSlot => "slot",
        Tool::Polygon => "polygon",
        Tool::Point => "sketch_point",
        Tool::Fillet => "sketch_fillet",
        Tool::Chamfer => "sketch_chamfer",
        Tool::Trim => "trim",
        Tool::Extend => "extend",
        Tool::Offset => "offset",
        Tool::Mirror => "sketch_mirror",
        Tool::LinearPattern => "sketch_lpattern",
        Tool::CircularPattern => "sketch_cpattern",
    }
}

/// "Sketch on": one click starts a sketch on a standard plane, a reference plane or the selection.
fn plane_chooser(app: &mut App, ui: &mut egui::Ui) {
    let target = app.sketch_target().map(|(_, name, _)| name);
    let tip = match &target {
        Some(name) => format!("Sketch on {name}"),
        None => "Select a flat face or a plane first, or pick a plane on the right".into(),
    };
    if tile(ui, "sketch", "Sketch", target.is_some(), &tip) {
        app.start_sketch();
    }
    let mut pick = None;
    stack(ui, |ui| {
        for p in StdPlane::ALL {
            let tip = match p {
                StdPlane::Front => "Sketch on the XZ plane, looking from the front",
                StdPlane::Top => "Sketch on the XY plane, the build plate",
                StdPlane::Right => "Sketch on the YZ plane, looking from the right",
            };
            if small(ui, "plane", p.name(), true, tip) {
                pick = Some(ModelRef::Plane(p));
            }
        }
    });
    let ref_planes: Vec<(FeatureId, String)> = app
        .doc
        .history()
        .iter()
        .filter(|(id, f)| {
            matches!(f.kind, FeatureKind::RefPlane(_) | FeatureKind::CoordSys(_)) && app.doc.plane_of(*id).is_some()
        })
        .map(|(id, f)| (id, f.name.clone()))
        .collect();
    for col in ref_planes.chunks(3) {
        stack(ui, |ui| {
            for (id, name) in col {
                if small(ui, "plane", name, true, "Sketch on this reference plane") {
                    pick = Some(ModelRef::Feature(*id));
                }
            }
        });
    }
    ui.separator();
    let editable = app
        .selected
        .filter(|&id| app.doc.history().get(id).is_some_and(|f| f.kind.is_sketch()));
    let tip = if editable.is_some() {
        "Open the selected sketch"
    } else {
        "Select a sketch in the FeatureManager first"
    };
    if tile(ui, "sketch", "Edit Sketch", editable.is_some(), tip) {
        if let Some(id) = editable {
            app.edit_sketch(id);
        }
    }
    if let Some(r) = pick {
        app.sketch_on(r);
    }
}

fn sketch(app: &mut App, ui: &mut egui::Ui) {
    if app.sketch.is_none() {
        plane_chooser(app, ui);
        return;
    }
    if tile(
        ui,
        "exit_sketch",
        "Exit Sketch",
        true,
        "Save the sketch and leave sketch mode",
    ) {
        app.exit_sketch(true);
        return;
    }
    if tile(
        ui,
        "convert",
        "Convert Entities",
        true,
        "Copy the edges of the face this sketch is on into the sketch",
    ) {
        app.convert_entities();
    }
    let Some(session) = &mut app.sketch else { return };
    ui.separator();
    for t in [Tool::Select, Tool::SmartDimension] {
        if icons::tile(ui, tool_icon(t), t.label(), true, session.tool == t)
            .on_hover_text(t.hint())
            .clicked()
        {
            session.set_tool(t);
        }
    }
    ui.separator();
    for col in TOOL_COLUMNS {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for &t in *col {
                if icons::small(ui, tool_icon(t), t.label(), true, session.tool == t)
                    .on_hover_text(t.hint())
                    .clicked()
                {
                    session.set_tool(t);
                }
            }
        });
    }
    ui.separator();
    ui.checkbox(&mut session.opts.show_relations, "Display relations");
}

fn evaluate(app: &mut App, ui: &mut egui::Ui) {
    if tile(
        ui,
        "measure",
        "Measure",
        true,
        "Distances, angles, radii and lengths of the selection",
    ) {
        app.show_measure = true;
    }
    if tile(
        ui,
        "mass",
        "Mass Properties",
        true,
        "Volume, mass, center of mass and moments",
    ) {
        app.show_mass = true;
    }
    if tile(ui, "whatswrong", "What's Wrong", true, "Rebuild errors and warnings") {
        app.show_whats_wrong = true;
    }
    ui.separator();
    if tile(
        ui,
        "rebuild",
        "Rebuild",
        true,
        "Rebuild every feature from scratch (Ctrl+Q)",
    ) {
        app.doc.force_rebuild();
        app.status = format!("Rebuilt {} features", app.doc.built().evaluated);
    }
}

fn print(app: &mut App, ui: &mut egui::Ui) {
    for (format, label) in [
        (ExportFormat::Stl, "Export STL"),
        (ExportFormat::ThreeMf, "Export 3MF"),
        (ExportFormat::Step, "Export STEP"),
    ] {
        if tile(ui, "export", label, true, "Save the part to a file") {
            app.export(format);
        }
    }
    ui.separator();
    ui.vertical(|ui| {
        ui.label("Chord tolerance");
        ui.add(
            egui::DragValue::new(&mut app.export_tol)
                .speed(0.001)
                .range(0.001..=1.0)
                .suffix(" mm"),
        );
        egui::ComboBox::from_id_salt("slicer")
            .selected_text(app.slicer.name())
            .show_ui(ui, |ui| {
                for s in Slicer::ALL {
                    ui.selectable_value(&mut app.slicer, s, s.name());
                }
            });
    });
    if tile(
        ui,
        "slicer",
        "Send to Slicer",
        true,
        "Open the part in the chosen slicer",
    ) {
        app.send_to_slicer();
    }
}
