//! Design intent: the Equations window (global variables, equations and configurations), the
//! configuration picker in the feature tree, and the Appearance window.

use std::collections::HashMap;
use crate::i18n::tr;

use eframe::egui::{self, TextEdit};
use threebcad_core::{Equation, FaceRef, FeatureId, History, Paint};

use crate::app::{App, ModelRef};
use crate::ui::theme;

/// Colour picker drags merge into one undo step under this key.
const APPEARANCE_KEY: u64 = 0xA99E_A12A_0000_0001;

/// An equation being edited; `here` keeps it to the active configuration.
#[derive(Clone, Default)]
struct Row {
    name: String,
    expr: String,
    here: bool,
}

/// The Equations window: its rows, and the active configuration's name as typed.
pub struct Equations {
    rows: Vec<Row>,
    name: String,
    /// The configuration and equations the rows were read from.
    seen: (usize, Vec<Equation>),
}

impl Equations {
    /// The part's equations, with the active configuration's in place of those they replace.
    pub fn of(h: &History) -> Self {
        let over = h
            .configurations()
            .get(h.active_configuration())
            .map_or(&[][..], |c| &c.equations[..]);
        let row = |e: &Equation, here| Row {
            name: e.name.clone(),
            expr: e.expr.clone(),
            here,
        };
        let mut rows: Vec<Row> = h
            .equations()
            .iter()
            .map(|e| match over.iter().find(|o| o.target() == e.target()) {
                Some(o) => row(o, true),
                None => row(e, false),
            })
            .collect();
        rows.extend(
            over.iter()
                .filter(|o| !h.equations().iter().any(|e| e.target() == o.target()))
                .map(|o| row(o, true)),
        );
        Self {
            rows,
            name: h.configuration_name().to_string(),
            seen: (h.active_configuration(), h.active_equations()),
        }
    }
}

/// Writes the rows back: those kept to this configuration become its equations, the rest the
/// part's. A part equation this configuration replaces stays, for the other configurations.
fn apply(h: &mut History, rows: &[Row]) {
    let configured = !h.configurations().is_empty();
    let (mut own, mut all) = (Vec::new(), Vec::new());
    for r in rows
        .iter()
        .filter(|r| !(r.name.trim().is_empty() && r.expr.trim().is_empty()))
    {
        let e = Equation::new(r.name.trim(), r.expr.trim());
        if configured && r.here {
            own.push(e);
        } else {
            all.push(e);
        }
    }
    let replaced: Vec<Equation> = h
        .equations()
        .iter()
        .filter(|e| own.iter().any(|o| o.target() == e.target()) && !all.iter().any(|a| a.target() == e.target()))
        .cloned()
        .collect();
    all.extend(replaced);
    h.set_equations(all);
    let active = h.active_configuration();
    if let Some(c) = h.configuration_mut(active) {
        c.equations = own;
    }
}

enum Act {
    Apply,
    Switch(usize),
    Add,
    Delete,
    Rename,
}

fn configuration_names(h: &History) -> Vec<String> {
    if h.configurations().is_empty() {
        vec!["Default".into()]
    } else {
        h.configurations().iter().map(|c| c.name.clone()).collect()
    }
}

pub fn equations_window(app: &mut App, ctx: &egui::Context) {
    let Some(mut w) = app.equations.take() else {
        return;
    };
    let units = app.doc.units();
    let h = app.doc.history();
    // Changed elsewhere (the tree, undo) or applied: show what the part has now.
    if w.seen != (h.active_configuration(), h.active_equations()) {
        w = Equations::of(h);
    }
    let results: HashMap<String, Result<f64, String>> = h
        .active_equations()
        .iter()
        .map(|e| e.target().to_string())
        .zip(app.doc.equation_results().iter().cloned())
        .collect();
    let dims = h.dimensions();
    let names = configuration_names(h);
    let configured = !h.configurations().is_empty();
    let active = h.active_configuration().min(names.len() - 1);
    let mut open = true;
    let mut act = None;
    egui::Window::new(tr("Equations"))
        .open(&mut open)
        .default_width(640.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Configuration"));
                egui::ComboBox::from_id_salt("equations_configuration")
                    .selected_text(&names[active])
                    .show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(i == active, n).clicked() && i != active {
                                act = Some(Act::Switch(i));
                            }
                        }
                    });
                let rename = ui
                    .add_enabled(configured, TextEdit::singleline(&mut w.name).desired_width(120.0))
                    .on_hover_text(tr("Rename this configuration"));
                if rename.lost_focus() && !w.name.trim().is_empty() && w.name.trim() != names[active] {
                    act = Some(Act::Rename);
                }
                if ui
                    .button(tr("+ New"))
                    .on_hover_text(tr("A new configuration like this one"))
                    .clicked()
                {
                    act = Some(Act::Add);
                }
                if names.len() > 1 && ui.button(tr("Delete")).clicked() {
                    act = Some(Act::Delete);
                }
            });
            ui.weak(
                "Suppress features in the tree to leave them out of this configuration; tick \"This \
                 configuration only\" for an equation that sets a dimension here alone.",
            );
            ui.separator();
            ui.weak(
                "Variables: wall = 2.  Dimensions: D1@Boss-Extrude1 = wall * 10 (+ Dimension lists their \
                 names).  Units: mm cm m in ft deg rad.  Functions: sin cos tan sqrt abs min max round.",
            );
            let mut gone = None;
            egui::Grid::new("equations")
                .num_columns(if configured { 5 } else { 4 })
                .striped(true)
                .show(ui, |ui| {
                    ui.strong(tr("Name"));
                    ui.strong(tr("Expression"));
                    ui.strong(tr("Value"));
                    if configured {
                        ui.strong(tr("This configuration only"));
                    }
                    ui.end_row();
                    for (i, r) in w.rows.iter_mut().enumerate() {
                        // A grid cell is only as wide as its column was: size the fields outright.
                        let h = ui.spacing().interact_size.y;
                        ui.add_sized([170.0, h], TextEdit::singleline(&mut r.name).hint_text(tr("wall")));
                        ui.add_sized([200.0, h], TextEdit::singleline(&mut r.expr).hint_text(tr("2 mm")));
                        let target = Equation::new(&r.name, "").target().to_string();
                        let unit = dims
                            .iter()
                            .find(|d| d.name == target)
                            .map_or("", |d| d.quantity.unit(units));
                        match results.get(&target) {
                            Some(Ok(v)) => ui.label(format!("{v:.*} {unit}", units.decimals)),
                            Some(Err(e)) => ui.colored_label(theme::ERROR, e),
                            None => ui.weak(tr("(Apply)")),
                        };
                        if configured {
                            ui.checkbox(&mut r.here, "");
                        }
                        if ui.small_button("✖").on_hover_text(tr("Remove")).clicked() {
                            gone = Some(i);
                        }
                        ui.end_row();
                    }
                });
            if let Some(i) = gone {
                w.rows.remove(i);
            }
            ui.horizontal(|ui| {
                if ui.button(tr("+ Variable")).clicked() {
                    w.rows.push(Row::default());
                }
                ui.menu_button(tr("+ Dimension"), |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        if dims.is_empty() {
                            ui.weak(tr("The part has no dimensions yet"));
                        }
                        for d in &dims {
                            let v = format!("{:.*}", units.decimals, d.quantity.shown(d.value, units));
                            let text = format!("{}   {} = {v} {}", d.name, d.label, d.quantity.unit(units));
                            if ui.button(text).clicked() {
                                w.rows.push(Row {
                                    name: d.name.clone(),
                                    expr: v,
                                    here: false,
                                });
                                ui.close_menu();
                            }
                        }
                    });
                });
                ui.separator();
                if ui.button(tr("Apply")).clicked() {
                    act = Some(Act::Apply);
                }
            });
        });
    let doc = &mut app.doc;
    match act {
        None => {}
        Some(Act::Apply) => {
            doc.edit(|h| apply(h, &w.rows));
            let failed = doc.equation_results().iter().filter(|r| r.is_err()).count();
            app.status = match failed {
                0 => "Equations applied".into(),
                n => format!("{n} equations have errors"),
            };
        }
        // Edits not applied yet go with the configuration they were typed in.
        Some(Act::Switch(i)) => doc.edit(|h| {
            apply(h, &w.rows);
            h.activate(i);
        }),
        Some(Act::Add) => doc.edit(|h| {
            apply(h, &w.rows);
            let i = h.add_configuration(&format!("Configuration {}", names.len() + 1));
            h.activate(i);
        }),
        Some(Act::Delete) => doc.edit(|h| h.remove_configuration(active)),
        Some(Act::Rename) => doc.edit(|h| {
            if let Some(c) = h.configuration_mut(active) {
                c.name = w.name.trim().to_string();
            }
        }),
    }
    if open {
        let reload = !matches!(act, None | Some(Act::Apply));
        app.equations = Some(if reload { Equations::of(app.doc.history()) } else { w });
    }
}

/// The configuration combo at the top of the feature tree.
pub fn configuration_picker(app: &mut App, ui: &mut egui::Ui) {
    let h = app.doc.history();
    let names = configuration_names(h);
    let active = h.active_configuration().min(names.len() - 1);
    let (mut pick, mut edit) = (None, false);
    ui.horizontal(|ui| {
        ui.label(tr("Configuration"));
        egui::ComboBox::from_id_salt("tree_configuration")
            .selected_text(&names[active])
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(i == active, n).clicked() {
                        pick = Some(i);
                    }
                }
                ui.separator();
                edit = ui.button(tr("New / Edit Configurations…")).clicked();
            })
            .response
            .on_hover_text(tr("Variants of the part with their own dimensions and suppressed features"));
    });
    if let Some(i) = pick.filter(|&i| i != active) {
        app.doc.activate(i);
        app.status = format!("Configuration {}", names[i]);
    }
    if edit {
        app.equations = Some(Equations::of(app.doc.history()));
    }
}

/// What the Appearance window colours.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Face,
    Body,
    Feature,
}

/// The Appearance window: a face (with its body and the feature that made it) or a feature, and the
/// colour being picked.
pub struct Appearance {
    face: Option<FaceRef>,
    feature: Option<FeatureId>,
    scope: Scope,
    color: [u8; 3],
}

impl Appearance {
    /// For a face of a body, its body (`body_only`) or the feature selected in the tree.
    pub fn of(app: &App, face: Option<(usize, usize)>, feature: Option<FeatureId>, body_only: bool) -> Option<Self> {
        let doc = &app.doc;
        let fref = face.and_then(|(b, f)| doc.face_ref(b, f));
        let made_by = face.and_then(|(b, f)| Some(doc.bodies().get(b)?.tags.get(f)?.feature));
        let feature = feature.or(made_by);
        let scope = match (&fref, body_only) {
            (Some(_), true) => Scope::Body,
            (Some(_), false) => Scope::Face,
            (None, _) => Scope::Feature,
        };
        if fref.is_none() && feature.is_none() {
            return None;
        }
        let color = face
            .and_then(|(b, f)| doc.face_colors(b).get(f).copied().flatten())
            .or_else(|| {
                doc.history()
                    .appearances()
                    .iter()
                    .find(|(p, _)| feature.is_some_and(|id| *p == Paint::Feature(id)))
                    .map(|a| a.1)
            })
            .unwrap_or(doc.material().color);
        Some(Self {
            face: fref,
            feature,
            scope,
            color,
        })
    }

    /// For the picked face, else the feature selected in the tree.
    pub fn of_selection(app: &App) -> Option<Self> {
        let face = app.selection.iter().find_map(|r| match *r {
            ModelRef::Face { body, face } => Some((body, face)),
            _ => None,
        });
        Self::of(app, face, if face.is_some() { None } else { app.selected }, false)
    }

    fn target(&self) -> Option<Paint> {
        match self.scope {
            Scope::Face => self.face.clone().map(Paint::Face),
            Scope::Body => self.face.clone().map(Paint::Body),
            Scope::Feature => self.feature.map(Paint::Feature),
        }
    }
}

pub fn appearance_window(app: &mut App, ctx: &egui::Context) {
    let Some(mut a) = app.appearance.take() else {
        return;
    };
    let feature = a
        .feature
        .and_then(|id| app.doc.history().get(id))
        .map(|f| f.name.clone());
    let before = a.scope;
    let (mut open, mut set, mut clear) = (true, false, false);
    egui::Window::new(tr("Appearance"))
        .open(&mut open)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let face = a.face.is_some();
                ui.add_enabled_ui(face, |ui| ui.radio_value(&mut a.scope, Scope::Face, tr("Face")));
                ui.add_enabled_ui(face, |ui| ui.radio_value(&mut a.scope, Scope::Body, tr("Body")));
                if let Some(name) = &feature {
                    ui.radio_value(&mut a.scope, Scope::Feature, format!("Feature ({name})"));
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Colour"));
                set = ui.color_edit_button_srgb(&mut a.color).changed();
                clear = ui.button(tr("Remove colour")).clicked();
            });
            ui.weak(
                "A face shows its own colour, else its body's, else its feature's, else the material's. \
                 3MF and STEP exports carry them.",
            );
        });
    if a.scope != before {
        app.doc.end_merge();
    }
    if let Some(target) = a.target() {
        if set {
            app.doc.set_appearance(target, Some(a.color), Some(APPEARANCE_KEY));
        } else if clear {
            app.doc.set_appearance(target, None, None);
            app.status = tr("Colour removed").into();
        }
    }
    if open {
        app.appearance = Some(a);
    } else {
        app.doc.end_merge();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_split_between_the_part_and_its_configuration() {
        let mut h = History::default();
        h.set_equations(vec![Equation::new("wall", "2"), Equation::new("len", "10")]);
        let i = h.add_configuration("Long");
        h.activate(i);
        // `len = 50` in Long only: the part keeps `len = 10` for Default.
        let mut w = Equations::of(&h);
        w.rows[1].expr = "50".into();
        w.rows[1].here = true;
        apply(&mut h, &w.rows);
        assert_eq!(h.equations(), [Equation::new("wall", "2"), Equation::new("len", "10")]);
        assert_eq!(h.configurations()[i].equations, [Equation::new("len", "50")]);
        let mut w = Equations::of(&h);
        assert!(w.rows[1].here && w.rows[1].expr == "50", "Long shows its own");
        h.activate(0);
        assert_eq!(Equations::of(&h).rows[1].expr, "10", "Default shows the part's");
        // Unticked, `len = 50` is the part's, for every configuration.
        h.activate(i);
        w.rows[1].here = false;
        apply(&mut h, &w.rows);
        assert_eq!(h.equations(), [Equation::new("wall", "2"), Equation::new("len", "50")]);
        assert!(h.configurations()[i].equations.is_empty());
        // Blank rows are dropped.
        let mut w = Equations::of(&h);
        w.rows.push(Row::default());
        apply(&mut h, &w.rows);
        assert_eq!(h.equations().len(), 2);
    }
}
