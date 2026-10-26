//! PropertyManager: parameters of the selected feature, or the active sketch tool and selection.

use eframe::egui::{self, Color32, RichText};
use threebcad_core::{FeatureId, FeatureKind};
use threebcad_sketch::tools::Item;
use threebcad_sketch::Status;

use crate::app::App;
use crate::ui::sketch_mode::{fmt_value, SketchSession, Tool};

/// Undo-merge key: one undo step per parameter being dragged.
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
        if let Some((_, name)) = app.sketch_target() {
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

fn feature_panel(app: &mut App, ctx: &egui::Context) {
    let Some(id) = app.selected else { return };
    let Some(feature) = app.doc.history().get(id) else {
        return;
    };
    let name = feature.name.clone();
    let error = app.doc.errors().get(&id).cloned();
    let status = app.doc.sketch_status(id);
    // Copy the parameters out so the document can be edited after the UI pass.
    let (dims, depth, source) = match &feature.kind {
        FeatureKind::Sketch { sketch, .. } => {
            let dims: Vec<_> = sketch
                .constraints()
                .iter()
                .enumerate()
                .filter_map(|(i, c)| Some((i, c.name(), c.dimension()?, c.is_angular())))
                .collect();
            (Some(dims), None, None)
        }
        FeatureKind::ExtrudeBoss { sketch, depth } => {
            let source = app.doc.history().get(*sketch).map(|f| f.name.clone());
            (None, Some(*depth), source)
        }
    };

    let mut close = false;
    let mut edit = None;
    let mut edit_sketch = false;
    egui::SidePanel::right("property_manager")
        .resizable(true)
        .default_width(260.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&name);
                close = ui.small_button("✖").on_hover_text("Close").clicked();
            });
            ui.separator();
            if let Some(e) = &error {
                ui.colored_label(Color32::LIGHT_RED, e);
                ui.separator();
            }
            if let Some(mut depth) = depth {
                ui.label(format!("From: {}", source.as_deref().unwrap_or("?")));
                ui.horizontal(|ui| {
                    ui.label("Depth");
                    let r = ui.add(
                        egui::DragValue::new(&mut depth)
                            .speed(0.5)
                            .range(-10_000.0..=10_000.0)
                            .suffix(" mm"),
                    );
                    if r.changed() {
                        edit = Some((0, depth));
                    }
                });
            }
            if let Some(dims) = dims {
                if let Some((s, dof)) = status {
                    ui.label(match s {
                        Status::UnderDefined => format!("Under defined ({dof} DOF)"),
                        Status::FullyDefined => "Fully defined".to_string(),
                        Status::OverDefined => "Over defined".to_string(),
                    });
                }
                edit_sketch = ui.button("✏ Edit Sketch").clicked();
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
                            edit = Some((i, if is_angle { shown.to_radians() } else { shown }));
                        }
                    });
                }
            }
        });

    if close {
        app.selected = None;
    }
    if edit_sketch {
        app.edit_sketch(id);
    }
    if let Some((param, value)) = edit {
        app.doc
            .edit_merging(Some(key(id, param)), |h| match h.get_mut(id).map(|f| &mut f.kind) {
                Some(FeatureKind::ExtrudeBoss { depth, .. }) => *depth = value,
                Some(FeatureKind::Sketch { sketch, .. }) => sketch.set_dimension(param, value),
                None => {}
            });
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
