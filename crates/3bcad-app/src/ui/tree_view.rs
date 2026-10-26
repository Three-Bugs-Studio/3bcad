//! FeatureManager design tree: reference planes, features with sketch status, rollback bar.

use eframe::egui::{self, Color32, RichText, Sense};
use threebcad_core::{FeatureId, FeatureKind};
use threebcad_sketch::Status;

use crate::app::{App, ModelRef, RefPlane};

const ROLLBACK_KEY: u64 = u64::MAX;

enum Action {
    Rollback(usize),
    Delete(FeatureId),
    Edit(FeatureId),
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::left("feature_manager")
        .resizable(true)
        .default_width(230.0)
        .show(ctx, |ui| {
            ui.heading("FeatureManager");
            ui.separator();
            ui.label(RichText::new(&app.doc.name).strong());
            for rp in RefPlane::ALL {
                ui.horizontal(|ui| {
                    let visible = app.plane_visible.get_mut(&rp).unwrap();
                    ui.checkbox(visible, "").on_hover_text("Show / hide");
                    let selected = app.selection.contains(&ModelRef::Plane(rp));
                    if ui.selectable_label(selected, format!("▱  {}", rp.name())).clicked() {
                        app.selection = vec![ModelRef::Plane(rp)];
                        app.selected = None;
                    }
                });
            }
            ui.label("    ⊕  Origin");

            let h = app.doc.history();
            let rollback = h.rollback();
            let rows: Vec<_> = h
                .iter()
                .map(|(id, f)| {
                    let (icon, prefix) = match f.kind {
                        FeatureKind::Sketch { .. } => (
                            "✏",
                            match app.doc.sketch_status(id).map(|s| s.0) {
                                Some(Status::UnderDefined) => "(-) ",
                                Some(Status::OverDefined) => "(+) ",
                                _ => "",
                            },
                        ),
                        FeatureKind::ExtrudeBoss { .. } => ("⬆", ""),
                    };
                    let is_sketch = matches!(f.kind, FeatureKind::Sketch { .. });
                    (
                        id,
                        format!("{icon}  {prefix}{}", f.name),
                        app.doc.errors().get(&id).cloned(),
                        is_sketch,
                    )
                })
                .collect();

            let mut rects = Vec::new();
            let mut bar = None;
            let mut action = None;
            for (i, (id, text, error, is_sketch)) in rows.iter().enumerate() {
                if i == rollback {
                    bar = Some(rollback_bar(ui));
                }
                let mut label = RichText::new(text);
                if i >= rollback {
                    label = label.color(Color32::GRAY).italics();
                } else if error.is_some() {
                    label = label.color(Color32::LIGHT_RED);
                }
                let editing = app.sketch.as_ref().is_some_and(|s| s.feature == Some(*id));
                let mut resp = ui.selectable_label(app.selected == Some(*id) || editing, label);
                if let Some(e) = error {
                    resp = resp.on_hover_text(e);
                }
                if resp.clicked() {
                    app.selected = Some(*id);
                    app.selection.clear();
                }
                if resp.double_clicked() && *is_sketch {
                    action = Some(Action::Edit(*id));
                }
                resp.context_menu(|ui| {
                    if *is_sketch && ui.button("Edit Sketch").clicked() {
                        action = Some(Action::Edit(*id));
                        ui.close_menu();
                    }
                    if ui.button("Roll Back to Here").clicked() {
                        action = Some(Action::Rollback(i));
                        ui.close_menu();
                    }
                    if ui.button("Delete (with children)").clicked() {
                        action = Some(Action::Delete(*id));
                        ui.close_menu();
                    }
                });
                rects.push(resp.rect);
            }
            if rollback == rows.len() {
                bar = Some(rollback_bar(ui));
            }

            if let Some(y) = bar
                .filter(|b| b.dragged())
                .and_then(|b| b.interact_pointer_pos())
                .map(|p| p.y)
            {
                let n = rects.iter().filter(|r| r.center().y < y).count();
                if n != rollback {
                    action = Some(Action::Rollback(n));
                }
            }
            match action {
                Some(Action::Rollback(n)) => app.doc.edit_merging(Some(ROLLBACK_KEY), |h| h.set_rollback(n)),
                Some(Action::Delete(id)) => {
                    app.doc.edit(|h| h.remove(id));
                    app.selected = None;
                }
                Some(Action::Edit(id)) => {
                    if app.sketch.is_some() {
                        app.exit_sketch(true);
                    }
                    app.edit_sketch(id);
                }
                None => {}
            }
        });
}

fn rollback_bar(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), Sense::drag());
    let color = if resp.hovered() || resp.dragged() {
        Color32::from_rgb(255, 200, 60)
    } else {
        Color32::from_rgb(60, 130, 230)
    };
    ui.painter().rect_filled(rect.shrink2(egui::vec2(0.0, 2.0)), 2.0, color);
    resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
        .on_hover_text("Rollback bar: drag to build the part only up to here")
}
