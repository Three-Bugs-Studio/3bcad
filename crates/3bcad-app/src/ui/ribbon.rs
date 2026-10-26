//! CommandManager ribbon.

use eframe::egui;
use threebcad_core::FeatureKind;
use threebcad_io::Slicer;
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

use crate::app::{App, ExportFormat, RELEASE_NOTES_URL, VERSION};
use crate::ui::sketch_mode::Tool;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RibbonTab {
    Features,
    Sketch,
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
            RibbonTab::Print => print(app, ui),
        });
        ui.add_space(4.0);
    });
}

fn features(app: &mut App, ui: &mut egui::Ui) {
    let h = app.doc.history();
    let sketch = app
        .selected
        .filter(|&id| h.is_active(id) && matches!(h.get(id).map(|f| &f.kind), Some(FeatureKind::Sketch { .. })));
    let enabled = sketch.is_some() || app.sketch.is_some();
    let extrude = ui
        .add_enabled(enabled, egui::Button::new("Extruded Boss/Base"))
        .on_disabled_hover_text("Select a sketch (click it in the FeatureManager or the graphics area) first");
    if extrude.clicked() {
        // Like SolidWorks: extruding from inside a sketch closes the sketch first.
        if app.sketch.is_some() {
            app.exit_sketch(true);
        }
        if let Some(id) = app.selected {
            match app.doc.edit(|h| h.add_extrude(id, 10.0)) {
                Ok(new) => app.selected = Some(new),
                Err(e) => app.status = e.to_string(),
            }
        }
    }
    ui.separator();
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
    let Some(session) = &mut app.sketch else {
        if ui
            .button("✏ Sketch")
            .on_hover_text("Select a plane or planar face (or a sketch to edit), then click")
            .clicked()
        {
            app.start_sketch();
        }
        ui.weak(
            "Pick the Front, Top or Right plane (or a flat face) in the FeatureManager or the graphics area first.",
        );
        return;
    };
    if ui.button("✔ Exit Sketch").clicked() {
        app.exit_sketch(true);
        return;
    }
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
