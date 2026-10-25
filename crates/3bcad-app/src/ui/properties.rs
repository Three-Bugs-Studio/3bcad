//! PropertyManager: parameters of the selected feature.

use eframe::egui;
use threebcad_core::{FeatureId, FeatureKind};
use threebcad_sketch::Constraint;

use crate::app::App;

/// Undo-merge key: one undo step per parameter being dragged.
fn key(id: FeatureId, param: usize) -> u64 {
    ((id.index() as u64) << 16) | param as u64
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(id) = app.selected else { return };
    let Some(feature) = app.doc.history().get(id) else {
        return;
    };
    let name = feature.name.clone();
    let error = app.doc.errors().get(&id).cloned();
    // Copy the parameters out so the document can be edited after the UI pass.
    let (dims, depth, source) = match &feature.kind {
        FeatureKind::Sketch(s) => {
            let dims: Vec<_> = s
                .constraints
                .iter()
                .enumerate()
                .filter_map(|(i, c)| Some((i, c.name(), c.dimension()?, matches!(c, Constraint::Angle(..)))))
                .collect();
            (dims, None, None)
        }
        FeatureKind::ExtrudeBoss { sketch, depth } => {
            let source = app.doc.history().get(*sketch).map(|f| f.name.clone());
            (Vec::new(), Some(*depth), source)
        }
    };

    let mut close = false;
    let mut edit = None;
    egui::SidePanel::right("property_manager")
        .resizable(true)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&name);
                close = ui.small_button("✖").on_hover_text("Close").clicked();
            });
            ui.separator();
            if let Some(e) = &error {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
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
            if !dims.is_empty() {
                ui.label("Dimensions");
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
    if let Some((param, value)) = edit {
        app.doc
            .edit_merging(Some(key(id, param)), |h| match h.get_mut(id).map(|f| &mut f.kind) {
                Some(FeatureKind::ExtrudeBoss { depth, .. }) => *depth = value,
                Some(FeatureKind::Sketch(s)) => s.constraints[param].set_dimension(value),
                None => {}
            });
    }
}
