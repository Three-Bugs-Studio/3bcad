//! FeatureManager design tree: reference planes, features with sketch status, rollback bar,
//! rename (F2), suppress, drag-to-reorder.

use eframe::egui::{self, Color32, RichText, Sense};
use threebcad_core::{FeatureId, FeatureKind};
use threebcad_sketch::Status;

use crate::app::{App, ModelRef, RefPlane};
use crate::ui::{icons, theme};

const ROLLBACK_KEY: u64 = u64::MAX;

enum Action {
    Rollback(usize),
    Delete(FeatureId),
    EditSketch(FeatureId),
    SketchOn(FeatureId),
    Suppress(FeatureId, bool),
    Move(FeatureId, usize),
    Rename(FeatureId, String),
}

struct Row {
    id: FeatureId,
    text: String,
    icon: &'static str,
    error: Option<String>,
    warning: Option<String>,
    is_sketch: bool,
    /// A reference plane or coordinate system: you can sketch on it.
    is_plane: bool,
    suppressed: bool,
    own_suppressed: bool,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::left("feature_manager")
        .resizable(true)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.heading("FeatureManager");
            ui.separator();
            ui.label(RichText::new(&app.doc.name).strong());
            for rp in RefPlane::ALL {
                ui.horizontal(|ui| {
                    let visible = app.plane_visible.get_mut(&rp).unwrap();
                    ui.checkbox(visible, "").on_hover_text("Show / hide");
                    let selected = app.selection.contains(&ModelRef::Plane(rp));
                    let resp = ui
                        .horizontal(|ui| {
                            icons::inline(ui, "plane", true);
                            ui.selectable_label(selected, rp.name())
                        })
                        .inner
                        .on_hover_text("Double-click to sketch on it");
                    if resp.clicked() {
                        app.selection = vec![ModelRef::Plane(rp)];
                        app.selected = None;
                    }
                    let mut sketch = resp.double_clicked();
                    resp.context_menu(|ui| {
                        if ui.button("✏ Sketch on it").clicked() {
                            sketch = true;
                            ui.close_menu();
                        }
                    });
                    if sketch {
                        app.sketch_on(ModelRef::Plane(rp));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                icons::inline(ui, "origin", true);
                ui.label("Origin");
            });

            let h = app.doc.history();
            let rollback = h.rollback();
            let rows: Vec<Row> = h
                .iter()
                .map(|(id, f)| {
                    let prefix = match f.kind {
                        FeatureKind::Sketch { .. } => match app.doc.sketch_status(id).map(|s| s.0) {
                            Some(Status::UnderDefined) => "(-) ",
                            Some(Status::OverDefined) => "(+) ",
                            _ => "",
                        },
                        _ => "",
                    };
                    Row {
                        id,
                        text: format!("{prefix}{}", f.name),
                        icon: icons::of_feature(&f.kind),
                        error: app.doc.errors().get(&id).cloned(),
                        warning: app.doc.warnings().get(&id).cloned(),
                        is_sketch: f.kind.is_sketch(),
                        is_plane: matches!(f.kind, FeatureKind::RefPlane(_) | FeatureKind::CoordSys(_)),
                        suppressed: h.is_suppressed(id),
                        own_suppressed: f.suppressed,
                    }
                })
                .collect();

            let mut rects = Vec::new();
            let mut bar = None;
            let mut action = None;
            for (i, row) in rows.iter().enumerate() {
                if i == rollback {
                    bar = Some(rollback_bar(ui));
                }
                let id = row.id;
                if let Some((rid, text)) = &mut app.renaming {
                    if *rid == id {
                        let (rect, closed) = rename_box(ui, text);
                        rects.push(rect);
                        if let Some(save) = closed {
                            if save {
                                action = Some(Action::Rename(id, text.clone()));
                            }
                            app.renaming = None;
                        }
                        continue;
                    }
                }
                let mut label = RichText::new(&row.text);
                if i >= rollback || row.suppressed {
                    label = label.color(theme::MUTED).italics();
                } else if row.error.is_some() {
                    label = label.color(theme::ERROR);
                } else if row.warning.is_some() {
                    label = label.color(theme::WARNING);
                }
                let editing = app.sketch.as_ref().is_some_and(|s| s.feature == Some(id));
                let selected = app.selected == Some(id) || editing || app.selection.contains(&ModelRef::Feature(id));
                let inactive = i >= rollback || row.suppressed;
                let mut resp = feature_row(ui, id, row.icon, !inactive, selected, label);
                if let Some(e) = row.error.as_ref().or(row.warning.as_ref()) {
                    resp = resp.on_hover_text(e);
                }
                if resp.clicked() {
                    app.selected = Some(id);
                    app.selection.clear();
                    if matches!(
                        app.doc.history().get(id).map(|f| &f.kind),
                        Some(
                            FeatureKind::RefPlane(_)
                                | FeatureKind::RefAxis(_)
                                | FeatureKind::RefPoint(_)
                                | FeatureKind::CoordSys(_)
                        )
                    ) {
                        app.selection = vec![ModelRef::Feature(id)];
                    }
                }
                if resp.double_clicked() && row.is_sketch {
                    action = Some(Action::EditSketch(id));
                }
                if resp.double_clicked() && row.is_plane {
                    action = Some(Action::SketchOn(id));
                }
                if let Some(dragged) = resp.dnd_release_payload::<FeatureId>() {
                    if *dragged != id {
                        action = Some(Action::Move(*dragged, i));
                    }
                }
                if resp.dnd_hover_payload::<FeatureId>().is_some() {
                    ui.painter().hline(
                        resp.rect.x_range(),
                        resp.rect.top(),
                        egui::Stroke::new(2.0_f32, Color32::from_rgb(60, 130, 230)),
                    );
                }
                resp.context_menu(|ui| {
                    if ui.button("Edit Feature").clicked() {
                        app.selected = Some(id);
                        ui.close_menu();
                    }
                    if row.is_plane && ui.button("✏ Sketch on it").clicked() {
                        action = Some(Action::SketchOn(id));
                        ui.close_menu();
                    }
                    if row.is_sketch && ui.button("Edit Sketch").clicked() {
                        action = Some(Action::EditSketch(id));
                        ui.close_menu();
                    }
                    if ui.button("Rename (F2)").clicked() {
                        app.renaming = Some((
                            id,
                            app.doc.history().get(id).map(|f| f.name.clone()).unwrap_or_default(),
                        ));
                        ui.close_menu();
                    }
                    let label = if row.own_suppressed { "Unsuppress" } else { "Suppress" };
                    if ui.button(label).clicked() {
                        action = Some(Action::Suppress(id, !row.own_suppressed));
                        ui.close_menu();
                    }
                    ui.separator();
                    if i > 0 && ui.button("Move Up").clicked() {
                        action = Some(Action::Move(id, i - 1));
                        ui.close_menu();
                    }
                    if i + 1 < rows.len() && ui.button("Move Down").clicked() {
                        action = Some(Action::Move(id, i + 1));
                        ui.close_menu();
                    }
                    if ui.button("Roll Back to Here").clicked() {
                        action = Some(Action::Rollback(i));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Delete (with children)").clicked() {
                        action = Some(Action::Delete(id));
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
                Some(Action::SketchOn(id)) => app.sketch_on(ModelRef::Feature(id)),
                Some(Action::EditSketch(id)) => {
                    if app.sketch.is_some() {
                        app.exit_sketch(true);
                    }
                    app.edit_sketch(id);
                }
                Some(Action::Suppress(id, on)) => {
                    app.doc.edit(|h| h.set_suppressed(id, on));
                    app.status = format!(
                        "{} {}",
                        if on { "Suppressed" } else { "Unsuppressed" },
                        app.describe(ModelRef::Feature(id))
                    );
                }
                // Try on a copy first so a refused move or name leaves no undo step behind.
                Some(Action::Move(id, to)) => match app.doc.history().clone().move_to(id, to) {
                    Ok(()) => {
                        let _ = app.doc.edit(|h| h.move_to(id, to));
                    }
                    Err(e) => app.status = e.to_string(),
                },
                Some(Action::Rename(id, name)) => match app.doc.history().clone().rename(id, &name) {
                    Ok(()) => {
                        let _ = app.doc.edit(|h| h.rename(id, &name));
                    }
                    Err(e) => app.status = e.to_string(),
                },
                None => {}
            }
        });
}

/// A feature's row. Rows are drag sources; dropping one on another moves it there. The label senses
/// clicks and drags itself: `dnd_drag_source` would cover it in a drag-only widget that eats clicks.
fn feature_row(
    ui: &mut egui::Ui,
    id: FeatureId,
    icon: &str,
    enabled: bool,
    selected: bool,
    label: RichText,
) -> egui::Response {
    let resp = ui
        .horizontal(|ui| {
            icons::inline(ui, icon, enabled);
            ui.selectable_label(selected, label)
        })
        .inner
        .interact(Sense::click_and_drag());
    resp.dnd_set_drag_payload(id);
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    resp
}

/// The name box of a feature being renamed: its rect and, once it closes, whether to save.
fn rename_box(ui: &mut egui::Ui, text: &mut String) -> (egui::Rect, Option<bool>) {
    let r = ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
    let closed = r.lost_focus().then(|| ui.input(|i| i.key_pressed(egui::Key::Enter)));
    // Asked for after the check: asked for first, the box would never lose focus.
    if closed.is_none() {
        r.request_focus();
    }
    (r.rect, closed)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, text: &mut String) -> Option<bool> {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut closed = None;
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| closed = rename_box(ui, text).1);
        });
        closed
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn enter_saves_the_new_name_and_escape_does_not() {
        for (k, saved) in [(egui::Key::Enter, true), (egui::Key::Escape, false)] {
            let ctx = egui::Context::default();
            let mut text = "Boss".to_owned();
            assert_eq!(frame(&ctx, vec![], &mut text), None);
            assert_eq!(frame(&ctx, vec![], &mut text), None);
            let typed = vec![egui::Event::Text("X".into())];
            assert_eq!(frame(&ctx, typed, &mut text), None);
            assert_eq!(text, "BossX");
            assert_eq!(frame(&ctx, vec![key(k)], &mut text), Some(saved));
        }
    }

    /// One frame of two rows: their responses, and what was dropped on the second.
    fn rows(ctx: &egui::Context, events: Vec<egui::Event>) -> ([egui::Response; 2], Option<FeatureId>) {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut out = None;
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut row = |n| feature_row(ui, FeatureId::new(n), "plane", true, false, "Sketch1".into());
                let (a, b) = (row(0), row(1));
                let dropped = b.dnd_release_payload::<FeatureId>().map(|d| *d);
                out = Some(([a, b], dropped));
            });
        });
        out.unwrap()
    }

    fn button(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn a_row_takes_clicks_and_right_clicks() {
        let ctx = egui::Context::default();
        let p = rows(&ctx, vec![]).0[0].rect.center();
        rows(&ctx, vec![egui::Event::PointerMoved(p)]);
        rows(&ctx, vec![button(p, egui::PointerButton::Primary, true)]);
        let (clicked, _) = rows(&ctx, vec![button(p, egui::PointerButton::Primary, false)]);
        assert!(clicked[0].clicked());
        rows(&ctx, vec![button(p, egui::PointerButton::Secondary, true)]);
        let (clicked, _) = rows(&ctx, vec![button(p, egui::PointerButton::Secondary, false)]);
        assert!(clicked[0].secondary_clicked());
    }

    #[test]
    fn dropping_a_row_on_another_hands_over_its_feature() {
        let ctx = egui::Context::default();
        let [a, b] = rows(&ctx, vec![]).0;
        let (from, to) = (a.rect.center(), b.rect.center() + egui::vec2(0.0, 2.0));
        rows(&ctx, vec![egui::Event::PointerMoved(from)]);
        rows(&ctx, vec![button(from, egui::PointerButton::Primary, true)]);
        rows(&ctx, vec![egui::Event::PointerMoved(to)]);
        rows(&ctx, vec![egui::Event::PointerMoved(to)]);
        let (released, dropped) = rows(&ctx, vec![button(to, egui::PointerButton::Primary, false)]);
        assert_eq!(dropped, Some(FeatureId::new(0)));
        assert!(!released[0].clicked());
    }
}
