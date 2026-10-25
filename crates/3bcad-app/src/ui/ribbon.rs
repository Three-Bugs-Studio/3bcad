//! CommandManager ribbon.

use eframe::egui;
use threebcad_core::FeatureKind;
use threebcad_io::Slicer;
use threebcad_sketch::Sketch;

use crate::app::{App, ExportFormat};

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
            ui.separator();
            ui.selectable_value(&mut app.tab, RibbonTab::Features, "Features");
            ui.selectable_value(&mut app.tab, RibbonTab::Sketch, "Sketch");
            ui.selectable_value(&mut app.tab, RibbonTab::Print, "3D Print");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(app.doc.can_redo(), egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Y")
                    .clicked()
                {
                    app.doc.redo();
                }
                if ui
                    .add_enabled(app.doc.can_undo(), egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    app.doc.undo();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| match app.tab {
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
        .filter(|&id| h.is_active(id) && matches!(h.get(id).map(|f| &f.kind), Some(FeatureKind::Sketch(_))));
    let extrude = ui
        .add_enabled(sketch.is_some(), egui::Button::new("Extruded Boss/Base"))
        .on_disabled_hover_text("Select an active sketch in the FeatureManager first");
    if let (true, Some(id)) = (extrude.clicked(), sketch) {
        match app.doc.edit(|h| h.add_extrude(id, 10.0)) {
            Ok(new) => app.selected = Some(new),
            Err(e) => app.status = e.to_string(),
        }
    }
    ui.separator();
    if ui.button("Box").on_hover_text("Rectangle sketch + extrude").clicked() {
        add_primitive(app, Sketch::rectangle(20.0, 20.0), 20.0);
    }
    if ui.button("Cylinder").on_hover_text("Circle sketch + extrude").clicked() {
        add_primitive(app, Sketch::circle([0.0, 0.0], 10.0), 20.0);
    }
}

fn add_primitive(app: &mut App, sketch: Sketch, depth: f64) {
    let r = app.doc.edit(|h| {
        let s = h.add_sketch(sketch);
        h.add_extrude(s, depth)
    });
    match r {
        Ok(id) => app.selected = Some(id),
        Err(e) => app.status = e.to_string(),
    }
}

fn sketch(app: &mut App, ui: &mut egui::Ui) {
    let mut add = |sketch| {
        let id = app.doc.edit(|h| h.add_sketch(sketch));
        app.selected = Some(id);
    };
    if ui.button("Corner Rectangle").clicked() {
        add(Sketch::rectangle(40.0, 30.0));
    }
    if ui.button("Circle").clicked() {
        add(Sketch::circle([0.0, 0.0], 10.0));
    }
    ui.separator();
    ui.weak("Sketches go on the Front (XY) plane; edit dimensions in the PropertyManager.");
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
