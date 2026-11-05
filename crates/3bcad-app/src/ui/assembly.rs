//! Assembly mode: the Assembly ribbon tab, the component tree with its mates and exploded view,
//! the Properties panel for what is picked, and the interference and bill of materials windows.

use std::time::Duration;

use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText};
use threebcad_kernel::mate::{Geom, MateKind, Pose};
use threebcad_kernel::topology::P3;

use crate::app::{App, ExportFormat, ModelRef, VIEWPORT_BG};
use crate::assembly::{insertable, Session};
use crate::ui::icons;
use crate::ui::properties::{length, number, units};

const ERROR: Color32 = Color32::from_rgb(190, 50, 30);

/// One frame of assembly mode, after the file shortcuts and prompts.
pub fn frame(app: &mut App, ctx: &egui::Context) {
    shortcuts(app, ctx);
    let dt = f64::from(ctx.input(|i| i.stable_dt).min(0.05));
    if let Some(s) = &mut app.asm {
        if s.animate(dt) {
            ctx.request_repaint();
        }
        if s.refresh() {
            app.status = "Parts changed on disk: the assembly shows them as they are now".into();
        }
    }
    // Keep looking for parts changed in another window.
    ctx.request_repaint_after(Duration::from_secs(1));
    if app.camera.animate(dt as f32) {
        ctx.request_repaint();
    }
    let picked_before = app.selection.clone();
    crate::ui::ribbon::show(app, ctx);
    status_bar(app, ctx);
    tree(app, ctx);
    properties(app, ctx);
    app.sync_assembly();
    windows(app, ctx);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(VIEWPORT_BG))
        .show(ctx, |ui| crate::ui::viewport::show(app, ui));
    if app.selection != picked_before {
        // A face picked in the viewport picks its component.
        if let Some(s) = &mut app.asm {
            if let Some(&r) = app.selection.last() {
                s.selected = s.component_of(r);
                s.mate = None;
                s.step = None;
            }
        }
        ctx.request_repaint();
    }
    if app.sync_states() {
        ctx.request_repaint();
    }
}

fn shortcuts(app: &mut App, ctx: &egui::Context) {
    let typing = ctx.wants_keyboard_input();
    let (undo, redo, delete, escape) = ctx.input_mut(|i| {
        (
            !typing && i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
            !typing && i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)),
            i.key_pressed(Key::Delete),
            i.key_pressed(Key::Escape),
        )
    });
    if undo {
        app.undo();
    }
    if redo {
        app.redo();
    }
    if !typing && delete {
        delete_selected(app);
    }
    if !typing && escape {
        app.selection.clear();
        if let Some(s) = &mut app.asm {
            (s.selected, s.mate, s.step) = (None, None, None);
        }
    }
    if !typing && ctx.input(|i| i.key_pressed(Key::F)) {
        app.zoom_to_fit();
    }
    if ctx.input(|i| i.key_pressed(Key::F1)) {
        app.show_shortcuts = !app.show_shortcuts;
    }
}

fn delete_selected(app: &mut App) {
    let Some(s) = &mut app.asm else { return };
    if let Some(m) = s.mate.take() {
        s.doc.remove_mate(m);
        app.status = "Mate deleted".into();
    } else if let Some(k) = s.step.take() {
        s.doc.remove_explode_step(k);
        app.status = "Explode step deleted".into();
    } else if let Some(id) = s.selected.take() {
        s.doc.remove(id);
        app.selection.clear();
        app.status = "Component deleted, with its mates".into();
    }
}

/// The two faces picked for a mate, as mates hold them, and their shapes.
fn mate_pair(app: &App) -> Option<[(threebcad_core::assembly::MateFace, Geom); 2]> {
    let s = app.asm.as_ref()?;
    let [a, b] = app.selection[..] else { return None };
    let pick = |r: ModelRef| Some((s.mate_face(r)?, shape(s, r)?));
    let (a, b) = (pick(a)?, pick(b)?);
    (a.0.component != b.0.component).then_some([a, b])
}

/// Whether a picked face is flat or round (the geometry is only a stand-in for `MateKind::fits`).
fn shape(s: &Session, r: ModelRef) -> Option<Geom> {
    let ModelRef::Face { body, face } = r else { return None };
    let &(l, b) = s.bodies.get(body)?;
    let leaves = s.doc.leaves();
    let f = leaves.get(l)?.part.bodies().get(b)?.topo.faces.get(face)?;
    match (f.cylinder, f.plane) {
        (Some(_), _) => Some(Geom::Cylinder {
            p: [0.0; 3],
            d: [0.0, 0.0, 1.0],
            r: 1.0,
        }),
        (None, Some(_)) => Some(Geom::Plane {
            p: [0.0; 3],
            n: [0.0, 0.0, 1.0],
        }),
        _ => None,
    }
}

/// Mate buttons, three high, for the kinds that fit the two picked faces.
fn mate_buttons(app: &mut App, ui: &mut egui::Ui) {
    let pair = mate_pair(app);
    for kinds in MateKind::ALL.chunks(3) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for &kind in kinds {
                let fits = pair.as_ref().is_some_and(|[a, b]| kind.fits(&a.1, &b.1));
                let tip = format!("{} mate between the two picked faces", kind.name());
                if icons::small(ui, "mate", kind.name(), fits, false)
                    .on_hover_text(tip)
                    .clicked()
                {
                    add_mate(app, kind);
                }
            }
        });
    }
}

fn add_mate(app: &mut App, kind: MateKind) {
    let Some([(a, _), (b, _)]) = mate_pair(app) else {
        app.status = "Pick two faces on two different components (Ctrl+click the second)".into();
        return;
    };
    let Some(s) = &mut app.asm else { return };
    app.status = match s.doc.add_mate(kind, a, b) {
        Ok(m) => {
            s.mate = Some(m);
            s.selected = None;
            match s.doc.problem(m) {
                Some(why) => format!("{} added, but {why}", s.doc.contents().mates[m].name),
                None => format!("{} added", s.doc.contents().mates[m].name),
            }
        }
        Err(e) => format!("Cannot mate: {e:#}"),
    };
    app.selection.clear();
}

/// The Assembly ribbon tab.
pub fn tab(app: &mut App, ui: &mut egui::Ui) {
    if icons::tile(ui, "insert", "Insert", true, false)
        .on_hover_text("Insert a part or an assembly from a file")
        .clicked()
    {
        app.insert_dialog();
    }
    let recent = insertable(&app.files.settings.recent, app.files.path.as_deref());
    ui.vertical(|ui| {
        ui.menu_button("Recent", |ui| {
            if recent.is_empty() {
                ui.weak("No recent parts");
            }
            for p in &recent {
                let name = p
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                    ui.close_menu();
                    app.insert_component(p);
                }
            }
        })
        .response
        .on_hover_text("Insert a part opened recently");
    });
    ui.separator();
    mate_buttons(app, ui);
    ui.separator();
    let selected = app.asm.as_ref().and_then(|s| s.selected);
    let fixed = selected
        .and_then(|id| app.asm.as_ref()?.doc.component(id))
        .map(|c| c.fixed);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        let label = if fixed == Some(true) { "Float" } else { "Fix" };
        if icons::small(ui, "fix", label, fixed.is_some(), false)
            .on_hover_text("A fixed component stays put; mates move the others to it")
            .clicked()
        {
            if let (Some(s), Some(id), Some(f)) = (&mut app.asm, selected, fixed) {
                s.doc.set_fixed(id, !f);
            }
        }
        if icons::small(ui, "edit_part", "Edit Part", selected.is_some(), false)
            .on_hover_text("Open the component's part to change it")
            .clicked()
        {
            if let Some(id) = selected {
                app.edit_component(id);
            }
        }
        if icons::small(ui, "delete_body", "Delete", selected.is_some(), false)
            .on_hover_text("Remove the component and its mates (Delete)")
            .clicked()
        {
            delete_selected(app);
        }
    });
    ui.separator();
    if icons::tile(ui, "interference", "Interference", true, false)
        .on_hover_text("Find parts that overlap (parts that only touch are fine)")
        .clicked()
    {
        if let Some(s) = &mut app.asm {
            let n = s.check_interference();
            app.status = match n {
                0 => "No interference".into(),
                n => format!("{n} pairs of parts interfere"),
            };
        }
    }
    explode_tiles(app, ui);
    if icons::tile(ui, "bom", "BOM", true, false)
        .on_hover_text("Bill of materials")
        .clicked()
    {
        if let Some(s) = &mut app.asm {
            s.show_bom = !s.show_bom;
        }
    }
    ui.separator();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        if icons::small(ui, "export", "STEP", true, false)
            .on_hover_text("Export the assembly as STEP: each part once, every component placed and named")
            .clicked()
        {
            app.export(ExportFormat::Step);
        }
        if icons::small(ui, "export", "3MF", true, false)
            .on_hover_text("Export one object per part, laid out on the bed for the slicer")
            .clicked()
        {
            app.export(ExportFormat::ThreeMf);
        }
        let slicer = app.files.settings.slicer.name();
        if icons::small(ui, "slicer", "Send to Slicer", true, false)
            .on_hover_text(format!("Open the parts in {slicer}, laid out on the bed"))
            .clicked()
        {
            app.send_to_slicer();
        }
    });
}

fn explode_tiles(app: &mut App, ui: &mut egui::Ui) {
    let Some(s) = &mut app.asm else { return };
    let steps = s.doc.contents().explode.len();
    // The picked component, or the components of the picked faces; out along a picked flat face.
    let mut picked: Vec<u32> = s.selected.into_iter().collect();
    for c in app.selection.iter().filter_map(|&r| s.component_of(r)) {
        if !picked.contains(&c) {
            picked.push(c);
        }
    }
    let direction = app
        .selection
        .iter()
        .find_map(|&r| s.face_normal(r))
        .unwrap_or([0.0, 0.0, 1.0]);
    if icons::tile(ui, "explode", "Explode Step", !picked.is_empty(), false)
        .on_hover_text(
            "Move the picked components out, along a picked flat face or else up; set the direction and              distance in Properties",
        )
        .clicked()
    {
        s.doc.add_explode_step(picked, direction, 30.0);
        s.step = Some(steps);
        s.mate = None;
        s.explode_to = (steps + 1) as f64;
        app.status = "Explode step added".into();
    }
    let exploded = s.explode_to > 0.0;
    if icons::tile(
        ui,
        "explode",
        if exploded { "Collapse" } else { "Explode" },
        steps > 0,
        exploded,
    )
    .on_hover_text("Play the exploded view, or put the parts back")
    .clicked()
    {
        s.explode_to = if exploded { 0.0 } else { steps as f64 };
    }
}

fn status_bar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(&app.status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let u = app.units().system;
                ui.menu_button(format!("{} ({})", u.short(), u.length_unit()), |ui| {
                    crate::files::units_ui(app, ui)
                })
                .response
                .on_hover_text("Unit system");
                let Some(s) = &app.asm else { return };
                ui.label(format!("{} components", s.doc.contents().components.len()));
                let problems = s.doc.problems().len();
                if problems > 0 {
                    ui.separator();
                    let text = match problems {
                        1 => "⚠ 1 mate does not hold".to_string(),
                        n => format!("⚠ {n} mates do not hold"),
                    };
                    ui.colored_label(ERROR, text);
                }
            });
        });
    });
}

/// The assembly tree: components (sub-assemblies opened up), mates and the exploded view.
pub fn tree(app: &mut App, ctx: &egui::Context) {
    let mut edit = None;
    egui::SidePanel::left("assembly_tree")
        .resizable(true)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.heading("Assembly Tree");
            ui.separator();
            let Some(s) = &mut app.asm else { return };
            ui.label(RichText::new(&s.doc.name).strong());
            egui::ScrollArea::vertical().show(ui, |ui| {
                if s.doc.contents().components.is_empty() {
                    ui.weak("Insert a part to start: the first one is fixed at the origin.");
                }
                let comps = s.doc.contents().components.clone();
                for c in &comps {
                    let error = s.doc.file_error(c.id).map(str::to_string);
                    let mut text = RichText::new(format!("{}{}", if c.fixed { "(f) " } else { "" }, c.name));
                    if error.is_some() {
                        text = text.color(ERROR);
                    }
                    let resp = ui.horizontal(|ui| {
                        icons::inline(
                            ui,
                            if s.doc.sub_assembly(c.id).is_some() {
                                "assembly"
                            } else {
                                "extrude"
                            },
                            true,
                        );
                        ui.selectable_label(s.selected == Some(c.id), text)
                    });
                    let mut resp = resp.inner;
                    resp = match &error {
                        Some(why) => resp.on_hover_text(format!("Could not read {}: {why}", c.file.display())),
                        None => resp.on_hover_text(c.file.display().to_string()),
                    };
                    if resp.clicked() {
                        (s.selected, s.mate, s.step) = (Some(c.id), None, None);
                        app.selection.clear();
                    }
                    if resp.double_clicked() {
                        edit = Some(c.id);
                    }
                    resp.context_menu(|ui| {
                        if ui.button(if c.fixed { "Float" } else { "Fix" }).clicked() {
                            s.doc.set_fixed(c.id, !c.fixed);
                            ui.close_menu();
                        }
                        if ui.button("Edit Part").clicked() {
                            edit = Some(c.id);
                            ui.close_menu();
                        }
                        if ui.button("Delete").clicked() {
                            s.doc.remove(c.id);
                            s.selected = None;
                            ui.close_menu();
                        }
                    });
                    if let Some(sub) = s.doc.sub_assembly(c.id) {
                        ui.indent(c.id, |ui| {
                            for l in sub.leaves() {
                                ui.weak(l.name);
                            }
                        });
                    }
                }
                ui.add_space(6.0);
                let mates = s.doc.contents().mates.clone();
                egui::CollapsingHeader::new(format!("Mates ({})", mates.len()))
                    .default_open(true)
                    .show(ui, |ui| {
                        for (i, m) in mates.iter().enumerate() {
                            let problem = s.doc.problem(i).map(str::to_string);
                            let mut text = RichText::new(format!("{} ({})", m.name, m.kind.name()));
                            if problem.is_some() {
                                text = text.color(ERROR);
                            }
                            let resp = ui.selectable_label(s.mate == Some(i), text);
                            let resp = match &problem {
                                Some(why) => resp.on_hover_text(why),
                                None => resp,
                            };
                            if resp.clicked() {
                                (s.selected, s.mate, s.step) = (None, Some(i), None);
                            }
                            resp.context_menu(|ui| {
                                if ui.button("Flip").clicked() {
                                    s.doc.set_mate(i, m.value, !m.flip, None);
                                    ui.close_menu();
                                }
                                if ui.button("Delete").clicked() {
                                    s.doc.remove_mate(i);
                                    s.mate = None;
                                    ui.close_menu();
                                }
                            });
                        }
                    });
                let steps = s.doc.contents().explode.clone();
                egui::CollapsingHeader::new(format!("Exploded View ({} steps)", steps.len()))
                    .default_open(true)
                    .show(ui, |ui| {
                        if steps.is_empty() {
                            ui.weak("Select a component and click Explode Step.");
                            return;
                        }
                        let mut t = s.explode;
                        if ui
                            .add(egui::Slider::new(&mut t, 0.0..=steps.len() as f64).text("steps"))
                            .changed()
                        {
                            (s.explode, s.explode_to) = (t, t);
                        }
                        for (k, st) in steps.iter().enumerate() {
                            let label = format!(
                                "Step {}: {} parts, {}",
                                k + 1,
                                st.components.len(),
                                units().length(st.distance)
                            );
                            if ui.selectable_label(s.step == Some(k), label).clicked() {
                                (s.selected, s.mate, s.step) = (None, None, Some(k));
                            }
                        }
                    });
            });
        });
    if let Some(id) = edit {
        app.edit_component(id);
    }
}

/// Undo-merge key for a value dragged in the panel.
fn key(kind: u64, index: usize) -> u64 {
    (kind << 32) | index as u64
}

pub fn properties(app: &mut App, ctx: &egui::Context) {
    let pair = mate_pair(app).is_some();
    egui::SidePanel::right("assembly_properties")
        .resizable(true)
        .default_width(260.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if pair {
                    ui.heading("Mate");
                    ui.separator();
                    for r in app.selection.clone() {
                        ui.label(app.describe(r));
                    }
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| mate_buttons(app, ui));
                    return;
                }
                let Some(s) = &mut app.asm else { return };
                if let Some(i) = s.mate {
                    mate_panel(s, ui, i);
                } else if let Some(k) = s.step {
                    step_panel(s, ui, k);
                } else if let Some(id) = s.selected {
                    if component_panel(s, ui, id) {
                        app.edit_component(id);
                    }
                } else {
                    ui.heading("Assembly");
                    ui.separator();
                    ui.weak(
                        "Click a component to move or fix it. To mate, click a face, Ctrl+click a face of \
                         another component, then pick a mate. Drag a component to move it, Shift+drag to turn it.",
                    );
                }
            });
        });
}

fn mate_panel(s: &mut Session, ui: &mut egui::Ui, i: usize) {
    let Some(m) = s.doc.contents().mates.get(i).cloned() else {
        return;
    };
    ui.heading(&m.name);
    ui.separator();
    ui.label(format!("{} mate", m.kind.name()));
    for f in [&m.a, &m.b] {
        let name = s.doc.component(f.component).map_or("?", |c| c.name.as_str());
        ui.weak(format!("Face of {name}"));
    }
    let (mut value, mut flip) = (m.value, m.flip);
    match m.kind {
        MateKind::Distance => {
            ui.horizontal(|ui| {
                ui.label("Distance");
                ui.add(length(&mut value).speed(0.1));
            });
        }
        MateKind::Angle => {
            ui.horizontal(|ui| {
                ui.label("Angle");
                ui.add(number(&mut value).speed(0.5).range(0.0..=180.0).suffix("°"));
            });
        }
        _ => {}
    }
    if m.kind != MateKind::Lock {
        ui.checkbox(&mut flip, "Flip alignment")
            .on_hover_text("Faces facing the same way instead of each other, or the other side");
    }
    if value != m.value || flip != m.flip {
        let dragging = ui.ctx().input(|i| i.pointer.any_down());
        s.doc.set_mate(i, value, flip, dragging.then_some(key(1, i)));
    }
    if let Some(why) = s.doc.problem(i) {
        ui.colored_label(ERROR, format!("Does not hold: {why}"));
    }
    ui.add_space(6.0);
    if ui.button("Delete Mate").clicked() {
        s.doc.remove_mate(i);
        s.mate = None;
    }
}

fn step_panel(s: &mut Session, ui: &mut egui::Ui, k: usize) {
    let Some(st) = s.doc.contents().explode.get(k).cloned() else {
        return;
    };
    ui.heading(format!("Explode Step {}", k + 1));
    ui.separator();
    for id in &st.components {
        ui.weak(s.doc.component(*id).map_or("?", |c| c.name.as_str()).to_string());
    }
    let mut distance = st.distance;
    ui.horizontal(|ui| {
        ui.label("Distance");
        ui.add(length(&mut distance).speed(0.5));
    });
    let mut direction: P3 = st.direction;
    ui.horizontal(|ui| {
        ui.label("Direction");
        for (label, d) in [
            ("+X", [1.0, 0.0, 0.0]),
            ("−X", [-1.0, 0.0, 0.0]),
            ("+Y", [0.0, 1.0, 0.0]),
            ("−Y", [0.0, -1.0, 0.0]),
            ("+Z", [0.0, 0.0, 1.0]),
            ("−Z", [0.0, 0.0, -1.0]),
        ] {
            if ui.selectable_label(direction == d, label).clicked() {
                direction = d;
            }
        }
    });
    if distance != st.distance || direction != st.direction {
        let dragging = ui.ctx().input(|i| i.pointer.any_down());
        s.doc
            .set_explode_step(k, direction, distance, dragging.then_some(key(2, k)));
        s.explode_to = s.doc.contents().explode.len() as f64;
    }
    ui.add_space(6.0);
    if ui.button("Delete Step").clicked() {
        s.doc.remove_explode_step(k);
        s.step = None;
    }
}

/// True when the user asks to edit the component's part.
fn component_panel(s: &mut Session, ui: &mut egui::Ui, id: u32) -> bool {
    let Some(c) = s.doc.component(id).cloned() else {
        return false;
    };
    ui.heading(&c.name);
    ui.separator();
    ui.weak(c.file.display().to_string());
    if let Some(why) = s.doc.file_error(id) {
        ui.colored_label(ERROR, format!("Could not read it: {why}"));
    }
    let mut name = c.name.clone();
    ui.horizontal(|ui| {
        ui.label("Name");
        if ui.text_edit_singleline(&mut name).lost_focus() && name != c.name && !name.trim().is_empty() {
            s.doc.rename(id, name.trim().to_string());
        }
    });
    let mut fixed = c.fixed;
    if ui
        .checkbox(&mut fixed, "Fixed")
        .on_hover_text("A fixed component stays put; mates move the others to it")
        .changed()
    {
        s.doc.set_fixed(id, fixed);
    }
    ui.add_space(4.0);
    ui.label("Position");
    let mut t = c.pose.t;
    for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(*axis);
            ui.add_enabled(!c.fixed, length(&mut t[k]).speed(0.5));
        });
    }
    let mut pose = Pose { t, ..c.pose };
    ui.horizontal(|ui| {
        ui.label("Turn 90°");
        for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
            let mut w = [0.0; 3];
            w[k] = std::f64::consts::FRAC_PI_2;
            if ui.add_enabled(!c.fixed, egui::Button::new(*axis)).clicked() {
                pose = pose.rotated(w, pose.t);
            }
        }
    });
    if pose != c.pose {
        let dragging = ui.ctx().input(|i| i.pointer.any_down());
        s.doc.set_pose(id, pose, dragging.then_some(key(3, id as usize)));
    }
    ui.add_space(6.0);
    let edit = ui
        .button("Edit Part")
        .on_hover_text("Open the part to change it")
        .clicked();
    if ui.button("Delete Component").clicked() {
        s.doc.remove(id);
        s.selected = None;
    }
    edit
}

fn windows(app: &mut App, ctx: &egui::Context) {
    crate::ui::help::window(app, ctx);
    let Some(s) = &mut app.asm else { return };
    let mut open = s.show_clashes;
    let mut again = false;
    egui::Window::new("Interference")
        .open(&mut open)
        .default_width(320.0)
        .show(ctx, |ui| {
            match s.current_clashes() {
                None => {
                    ui.weak("The assembly changed since it was checked.");
                }
                Some([]) => {
                    ui.colored_label(
                        Color32::from_rgb(30, 130, 50),
                        "No parts interfere. Parts that only touch are fine.",
                    );
                }
                Some(found) => {
                    egui::Grid::new("clashes").striped(true).show(ui, |ui| {
                        ui.strong("Parts");
                        ui.strong("Shared volume");
                        ui.end_row();
                        for c in found {
                            ui.label(format!("{}  ×  {}", c.names.0, c.names.1));
                            ui.label(units().volume(c.volume));
                            ui.end_row();
                        }
                    });
                    ui.weak("They are shown in red.");
                }
            }
            again = ui.button("Check Again").clicked();
        });
    s.show_clashes = open;
    if again {
        app.status = match s.check_interference() {
            0 => "No interference".into(),
            n => format!("{n} pairs of parts interfere"),
        };
    }
    let mut open = s.show_bom;
    let mut export = false;
    egui::Window::new("Bill of Materials")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.bom_indented, "Indented")
                    .on_hover_text("List the parts of each sub-assembly under it");
                export = ui.button("Export CSV…").clicked();
            });
            ui.separator();
            egui::Grid::new("bom").striped(true).show(ui, |ui| {
                for h in ["Item", "Part number", "Name", "Description", "Material", "Qty"] {
                    ui.strong(h);
                }
                ui.end_row();
                for r in s.doc.bom(s.bom_indented) {
                    ui.label(format!("{}{}", "  ".repeat(r.level), r.item));
                    ui.label(r.part_number);
                    ui.label(r.name);
                    ui.label(r.description);
                    ui.label(r.material);
                    ui.label(r.quantity.to_string());
                    ui.end_row();
                }
            });
            ui.weak("Part number and description come from each part's File > Properties.");
        });
    s.show_bom = open;
    if export {
        app.export_bom();
    }
}
