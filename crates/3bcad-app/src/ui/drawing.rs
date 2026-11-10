//! Drawing mode: the Drawing ribbon tab, the sheet tree, the Properties panel for what is picked,
//! and the sheet itself, panned and zoomed like paper on a desk.

use std::time::Duration;
use crate::i18n::tr;

use eframe::egui::{
    self, Align2, Color32, FontId, Key, KeyboardShortcut, Modifiers, Pos2, Rect, RichText, Sense, Stroke,
};
use threebcad_core::drawing::{DimKind, Dir, Note, Orientation, Pick, SheetSize, ViewKind};
use threebcad_core::DrawingDocument;
use threebcad_io::drawing::{dash_pattern, Align, Dash, Layer, Page, Shape, P2};

use crate::app::App;
use crate::drawing::{drawable, Grab, Item, Session, Tool};
use crate::ui::icons;
use crate::ui::properties::length;

const ERROR: Color32 = Color32::from_rgb(190, 50, 30);
const DESK: Color32 = Color32::from_rgb(150, 156, 166);
const INK: Color32 = Color32::from_rgb(20, 20, 24);
const PICKED: Color32 = Color32::from_rgb(40, 120, 230);
const HOVER: Color32 = Color32::from_rgb(240, 140, 20);
/// Scales offered for sheets and views (paper mm per model mm).
const SCALES: [f64; 11] = [10.0, 5.0, 2.0, 1.0, 0.5, 0.4, 0.2, 0.1, 0.05, 0.02, 0.01];
/// How near a click must be to geometry (screen points).
const REACH: f32 = 8.0;

/// One frame of drawing mode, after the file shortcuts and prompts.
pub fn frame(app: &mut App, ctx: &egui::Context) {
    shortcuts(app, ctx);
    if let Some(s) = &mut app.drw {
        if s.refresh() {
            app.status = tr("The part changed on disk: the views show it as it is now").into();
        }
    }
    // Keep looking for the part changing in another window.
    ctx.request_repaint_after(Duration::from_secs(1));
    crate::ui::ribbon::show(app, ctx);
    status_bar(app, ctx);
    tree(app, ctx);
    properties(app, ctx);
    crate::ui::help::window(app, ctx);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(DESK))
        .show(ctx, |ui| canvas(app, ui));
}

fn shortcuts(app: &mut App, ctx: &egui::Context) {
    let typing = ctx.wants_keyboard_input();
    let (undo, redo) = ctx.input_mut(|i| {
        (
            !typing && i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
            !typing && i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)),
        )
    });
    if undo {
        app.undo();
    }
    if redo {
        app.redo();
    }
    if typing {
        return;
    }
    let (delete, escape, fit, help) = ctx.input(|i| {
        (
            i.key_pressed(Key::Delete),
            i.key_pressed(Key::Escape),
            i.key_pressed(Key::F),
            i.key_pressed(Key::F1),
        )
    });
    if delete {
        delete_selected(app);
    }
    if let Some(s) = &mut app.drw {
        if escape {
            if s.tool == Tool::Select {
                s.selected = Item::Sheet;
            }
            s.tool = Tool::Select;
        }
        if fit {
            s.fit = true;
        }
    }
    if help {
        app.show_shortcuts = !app.show_shortcuts;
    }
}

fn delete_selected(app: &mut App) {
    let Some(s) = &mut app.drw else { return };
    let sheet = s.sheet();
    app.status = match s.selected {
        Item::View(id) => {
            s.doc.remove_view(sheet, id);
            "View deleted, with the views and dimensions made from it".into()
        }
        Item::Dim(i) => {
            s.doc.edit(None, |c| {
                if i < c.sheets[sheet].dims.len() {
                    c.sheets[sheet].dims.remove(i);
                }
            });
            "Dimension deleted".into()
        }
        Item::Note(i) => {
            s.doc.edit(None, |c| {
                if i < c.sheets[sheet].notes.len() {
                    c.sheets[sheet].notes.remove(i);
                }
            });
            "Note deleted".into()
        }
        Item::Sheet => return,
    };
    s.selected = Item::Sheet;
}

fn view_name(kind: &ViewKind) -> String {
    match kind {
        ViewKind::Model(o) => format!("{} View", o.name()),
        ViewKind::Projected { dir, .. } => format!("Projected View ({})", dir.name().to_lowercase()),
        ViewKind::Section { label, .. } => format!("Section {label}-{label}"),
        ViewKind::Detail { label, .. } => format!("Detail {label}"),
    }
}

fn view_icon(kind: &ViewKind) -> &'static str {
    match kind {
        ViewKind::Model(_) => "model_view",
        ViewKind::Projected { .. } => "projected_view",
        ViewKind::Section { .. } => "section_view",
        ViewKind::Detail { .. } => "detail_view",
    }
}

enum Action {
    Model,
    Recent(std::path::PathBuf),
    EditPart,
    Delete,
    Export(&'static str),
}

pub fn tab(app: &mut App, ui: &mut egui::Ui) {
    let mut action = None;
    let recent = drawable(&app.files.settings.recent);
    let Some(s) = &mut app.drw else { return };
    let has_part = s.doc.part().is_some();
    if icons::tile(ui, "insert", "Model", true, false)
        .on_hover_text(tr("Choose the part this drawing shows"))
        .clicked()
    {
        action = Some(Action::Model);
    }
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.menu_button(tr("Recent"), |ui| {
            if recent.is_empty() {
                ui.weak(tr("No recent parts"));
            }
            for p in &recent {
                let name = p
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                    ui.close_menu();
                    action = Some(Action::Recent(p.clone()));
                }
            }
        })
        .response
        .on_hover_text(tr("Show a part opened recently"));
        if icons::small(ui, "edit_part", "Edit Part", has_part, false)
            .on_hover_text(tr("Open the part to change it; the drawing shows the change when you come back"))
            .clicked()
        {
            action = Some(Action::EditPart);
        }
    });
    ui.separator();
    let sheet = s.sheet();
    let [w, h] = s.doc.contents().sheets[sheet].size.size();
    if icons::tile(ui, "std_views", "Standard Views", has_part, false)
        .on_hover_text(tr("The front view with the top and side views projected from it"))
        .clicked()
    {
        s.doc.standard_views(sheet);
    }
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.add_enabled_ui(has_part, |ui| {
            ui.menu_button(tr("Model View"), |ui| {
                for o in Orientation::ALL {
                    if ui.button(o.name()).clicked() {
                        ui.close_menu();
                        if let Some(id) = s.doc.add_model_view(sheet, o, [w * 0.7, h * 0.6]) {
                            s.selected = Item::View(id);
                        }
                    }
                }
            })
            .response
            .on_hover_text(tr("A view of the part from one side, or isometric"));
        });
        let base = match s.selected {
            Item::View(id) => s.doc.contents().sheets[sheet]
                .view(id)
                .filter(|v| !matches!(v.kind, ViewKind::Detail { .. }))
                .map(|v| v.id),
            _ => None,
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            ui.label(tr("Projected"));
            for (d, glyph) in [(Dir::Left, "⬅"), (Dir::Up, "⬆"), (Dir::Down, "⬇"), (Dir::Right, "➡")] {
                if ui
                    .add_enabled(base.is_some(), egui::Button::new(glyph).small())
                    .on_hover_text(format!(
                        "Project the selected view and place it {}",
                        d.name().to_lowercase()
                    ))
                    .clicked()
                {
                    if let Some(id) = base.and_then(|b| s.doc.add_projected(sheet, b, d)) {
                        s.selected = Item::View(id);
                    }
                }
            }
        });
    });
    ui.separator();
    let tool = s.tool.clone();
    for (vertical, label, tip) in [
        (true, "Section", "Cut a view along a vertical line: click where it goes"),
        (
            false,
            "Section —",
            "Cut a view along a horizontal line: click where it goes",
        ),
    ] {
        let on = tool == Tool::Section { vertical };
        if icons::tile(ui, "section_view", label, has_part, on)
            .on_hover_text(tip)
            .clicked()
        {
            s.tool = if on { Tool::Select } else { Tool::Section { vertical } };
        }
    }
    let on = tool == Tool::Detail;
    if icons::tile(ui, "detail_view", "Detail", has_part, on)
        .on_hover_text(tr("Show a circle of a view larger: click its middle"))
        .clicked()
    {
        s.tool = if on { Tool::Select } else { Tool::Detail };
    }
    ui.separator();
    let on = matches!(tool, Tool::Dimension(_));
    if icons::tile(ui, "dimension", "Smart Dimension", has_part, on)
        .on_hover_text(tr("Click an edge, a circle, or two vertices of a view"))
        .clicked()
    {
        s.tool = if on { Tool::Select } else { Tool::Dimension(Vec::new()) };
    }
    if icons::tile(ui, "model_items", "Model Items", has_part, false)
        .on_hover_text(tr("Bring in the dimensions of the part's sketches and extrusions"))
        .clicked()
    {
        let n = s.doc.import_model_dimensions(sheet);
        app.status = match n {
            0 => "No more model dimensions fit the views on this sheet".into(),
            1 => "Added 1 model dimension".into(),
            n => format!("Added {n} model dimensions"),
        };
    }
    let on = tool == Tool::Note;
    if icons::tile(ui, "note", "Note", true, on)
        .on_hover_text(tr("Add a note: click what it points at, or empty sheet"))
        .clicked()
    {
        s.tool = if on { Tool::Select } else { Tool::Note };
    }
    ui.separator();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        if icons::small(ui, "sheet", "Add Sheet", true, false)
            .on_hover_text(tr("A new sheet with the same size, scale and title block"))
            .clicked()
        {
            s.sheet = s.doc.add_sheet();
            s.selected = Item::Sheet;
            s.fit = true;
        }
        if icons::small(ui, "delete_body", "Delete", s.selected != Item::Sheet, false)
            .on_hover_text(tr("Delete the selected view, dimension or note (Delete)"))
            .clicked()
        {
            action = Some(Action::Delete);
        }
    });
    ui.separator();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for (ext, label, tip) in [
            ("pdf", "PDF", "Every sheet as one PDF, printed at true scale"),
            ("svg", "SVG", "An SVG file per sheet"),
            ("dxf", "DXF", "A DXF file per sheet, a layer per kind of line"),
        ] {
            if icons::small(ui, "export", label, true, false)
                .on_hover_text(tip)
                .clicked()
            {
                action = Some(Action::Export(ext));
            }
        }
    });
    match action {
        Some(Action::Model) => app.drawing_model_dialog(),
        Some(Action::Recent(p)) => app.drawing_model(&p),
        Some(Action::EditPart) => app.edit_drawing_part(),
        Some(Action::Delete) => delete_selected(app),
        Some(Action::Export(ext)) => app.export_drawing(ext),
        None => return,
    }
    // A file dialog blocks the frame; show what it changed without waiting for the mouse.
    ui.ctx().request_repaint();
}

fn status_bar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let hint = app.drw.as_ref().map_or("", |s| s.tool.hint());
            ui.label(if hint.is_empty() { app.status.as_str() } else { hint });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let u = app.units().system;
                ui.menu_button(format!("{} ({})", u.short(), u.length_unit()), |ui| {
                    crate::files::units_ui(app, ui)
                })
                .response
                .on_hover_text(tr("Unit system for dimensions"));
                let Some(s) = &mut app.drw else { return };
                let n = s.doc.contents().sheets.len();
                ui.label(format!("Sheet {} of {n}", s.sheet() + 1));
                if let Some(why) = s.doc.part_error() {
                    ui.separator();
                    ui.colored_label(ERROR, tr("⚠ The part could not be read"))
                        .on_hover_text(why);
                }
            });
        });
    });
}

pub fn tree(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::left("drawing_tree")
        .resizable(true)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.heading(tr("Drawing Tree"));
            ui.separator();
            let Some(s) = &mut app.drw else { return };
            ui.label(RichText::new(&s.doc.name).strong());
            match (&s.doc.contents().model, s.doc.part_error()) {
                (None, _) => {
                    ui.weak(tr("No part yet: Drawing tab > Model"));
                }
                (Some(m), err) => {
                    let name = m
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let mut text = RichText::new(format!("Part: {name}"));
                    if err.is_some() {
                        text = text.color(ERROR);
                    }
                    ui.label(text)
                        .on_hover_text(err.unwrap_or(&m.display().to_string()).to_string());
                }
            }
            ui.separator();
            let current = s.sheet();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let sheets = s.doc.contents().sheets.clone();
                for (i, sh) in sheets.iter().enumerate() {
                    let picked = i == current && s.selected == Item::Sheet;
                    let resp = ui.horizontal(|ui| {
                        icons::inline(ui, "sheet", true);
                        ui.selectable_label(picked, format!("{} ({})", sh.name, sh.size.name()))
                    });
                    if resp.inner.clicked() {
                        if s.sheet != i {
                            s.fit = true;
                        }
                        (s.sheet, s.selected) = (i, Item::Sheet);
                    }
                    if i != current {
                        continue;
                    }
                    ui.indent(i, |ui| {
                        for v in &sh.views {
                            let r = ui.horizontal(|ui| {
                                icons::inline(ui, view_icon(&v.kind), true);
                                ui.selectable_label(s.selected == Item::View(v.id), view_name(&v.kind))
                            });
                            if r.inner.clicked() {
                                s.selected = Item::View(v.id);
                            }
                            r.inner.context_menu(|ui| {
                                if ui.button(tr("Delete")).clicked() {
                                    s.doc.remove_view(current, v.id);
                                    s.selected = Item::Sheet;
                                    ui.close_menu();
                                }
                            });
                        }
                        for (k, d) in sh.dims.iter().enumerate() {
                            let text = s.doc.dimension_text(d).unwrap_or_else(|| "?".into());
                            let r = ui.horizontal(|ui| {
                                icons::inline(ui, "dimension", true);
                                ui.selectable_label(
                                    s.selected == Item::Dim(k),
                                    format!(
                                        "{text} ({}{})",
                                        d.kind.name().to_lowercase(),
                                        if d.model { ", model" } else { "" }
                                    ),
                                )
                            });
                            if r.inner.clicked() {
                                s.selected = Item::Dim(k);
                            }
                        }
                        for (k, n) in sh.notes.iter().enumerate() {
                            let first = n.text.lines().next().unwrap_or("").to_string();
                            let r = ui.horizontal(|ui| {
                                icons::inline(ui, "note", true);
                                ui.selectable_label(s.selected == Item::Note(k), format!("Note: {first}"))
                            });
                            if r.inner.clicked() {
                                s.selected = Item::Note(k);
                            }
                        }
                    });
                }
            });
        });
}

/// An undo-merge key for values changed in the panel: typing or dragging one stays one step.
fn panel_key(ui: &egui::Ui, what: u64) -> Option<u64> {
    let busy = ui.ctx().input(|i| i.pointer.any_down()) || ui.ctx().wants_keyboard_input();
    busy.then_some((9 << 40) | what)
}

fn scale_combo(ui: &mut egui::Ui, id: &str, scale: &mut f64) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(threebcad_core::drawing::scale_text(*scale))
        .show_ui(ui, |ui| {
            for s in SCALES {
                ui.selectable_value(scale, s, threebcad_core::drawing::scale_text(s));
            }
        });
}

pub fn properties(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::right("drawing_properties")
        .resizable(true)
        .default_width(270.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let Some(s) = &mut app.drw else { return };
                // A value typed or dragged in the panel is one undo step, ending when it is let go.
                if panel_key(ui, 0).is_none() {
                    s.doc.end_merge();
                }
                let sheet = s.sheet();
                match s.selected {
                    Item::Sheet => sheet_panel(s, ui, sheet),
                    Item::View(id) => view_panel(s, ui, sheet, id),
                    Item::Dim(i) => dim_panel(s, ui, sheet, i),
                    Item::Note(i) => note_panel(s, ui, sheet, i),
                }
            });
        });
}

fn sheet_panel(s: &mut Session, ui: &mut egui::Ui, i: usize) {
    let c = s.doc.contents();
    let (mut sh, mut cells) = (c.sheets[i].clone(), c.title_block.clone());
    let many = c.sheets.len() > 1;
    ui.heading(&sh.name);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(tr("Name"));
        ui.text_edit_singleline(&mut sh.name);
    });
    ui.horizontal(|ui| {
        ui.label(tr("Size"));
        egui::ComboBox::from_id_salt("sheet_size")
            .selected_text(sh.size.name())
            .show_ui(ui, |ui| {
                for z in SheetSize::ALL {
                    let [w, h] = z.size();
                    ui.selectable_value(&mut sh.size, z, format!("{} ({w:.0} × {h:.0} mm)", z.name()));
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label(tr("Scale"));
        scale_combo(ui, "sheet_scale", &mut sh.scale);
    });
    ui.label(tr("Projection"));
    ui.radio_value(&mut sh.third_angle, false, tr("First angle (ISO)"))
        .on_hover_text(tr("The view seen from the left goes on the right, the top view below"));
    ui.radio_value(&mut sh.third_angle, true, tr("Third angle (ANSI)"))
        .on_hover_text(tr("The view seen from the right goes on the right, the top view above"));
    ui.add_space(6.0);
    ui.strong(tr("Title block"));
    ui.weak(tr("{Name} fills in from File > Properties of the drawing or the part, or {Sheet}, {Scale}, {Projection}, {Model}, {Material}."));
    let mut gone = None;
    for (k, (label, text)) in cells.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add_sized([78.0, 20.0], egui::TextEdit::singleline(label));
            ui.add_sized([130.0, 20.0], egui::TextEdit::singleline(text));
            if ui.small_button("✖").on_hover_text(tr("Remove")).clicked() {
                gone = Some(k);
            }
        });
    }
    if let Some(k) = gone {
        cells.remove(k);
    }
    if ui.button(tr("+ Cell")).clicked() {
        cells.push(("Label".into(), String::new()));
    }
    let c = s.doc.contents();
    if sh != c.sheets[i] || cells != c.title_block {
        let fit = sh.size != c.sheets[i].size;
        let key = panel_key(ui, i as u64);
        s.doc.edit(key, |c| {
            c.sheets[i] = sh;
            c.title_block = cells;
        });
        s.fit |= fit;
    }
    ui.add_space(6.0);
    if many && ui.button(tr("Delete Sheet")).clicked() {
        s.doc.remove_sheet(i);
        (s.selected, s.fit) = (Item::Sheet, true);
    }
}

fn view_panel(s: &mut Session, ui: &mut egui::Ui, sheet: usize, id: u32) {
    let Some(v) = s.doc.contents().sheets[sheet].view(id).cloned() else {
        s.selected = Item::Sheet;
        return;
    };
    let sheet_scale = s.doc.contents().sheets[sheet].scale;
    let mut e = v.clone();
    ui.heading(view_name(&v.kind));
    ui.separator();
    match &mut e.kind {
        ViewKind::Model(o) => {
            ui.horizontal(|ui| {
                ui.label(tr("Looks at"));
                egui::ComboBox::from_id_salt("orientation")
                    .selected_text(o.name())
                    .show_ui(ui, |ui| {
                        for x in Orientation::ALL {
                            ui.selectable_value(o, x, x.name());
                        }
                    });
            });
            let mut own = e.scale.is_some();
            ui.checkbox(&mut own, tr("Own scale"))
                .on_hover_text(tr("Else the sheet's scale"));
            match (own, &mut e.scale) {
                (true, Some(sc)) => {
                    ui.horizontal(|ui| {
                        ui.label(tr("Scale"));
                        scale_combo(ui, "view_scale", sc);
                    });
                }
                (true, None) => e.scale = Some(sheet_scale),
                (false, _) => e.scale = None,
            }
        }
        ViewKind::Projected { .. } => {
            ui.weak(tr("Projected from its parent and lined up with it, at the parent's scale."));
        }
        ViewKind::Section {
            label,
            vertical,
            at,
            flip,
            ..
        } => {
            ui.horizontal(|ui| {
                ui.label(tr("Label"));
                ui.add(egui::TextEdit::singleline(label).desired_width(40.0));
            });
            ui.horizontal(|ui| {
                ui.radio_value(vertical, true, tr("Vertical line"));
                ui.radio_value(vertical, false, tr("Horizontal line"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Line at"));
                ui.add(length(at).speed(0.2));
            });
            ui.checkbox(flip, tr("Look the other way"));
            ui.weak(format!("Hatch: {}, from the part's material", s.doc.hatch().name()));
        }
        ViewKind::Detail {
            label, center, radius, ..
        } => {
            ui.horizontal(|ui| {
                ui.label(tr("Label"));
                ui.add(egui::TextEdit::singleline(label).desired_width(40.0));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Radius"));
                ui.add(length(radius).speed(0.1).range(0.1..=10_000.0));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Centre"));
                ui.add(length(&mut center[0]).speed(0.2));
                ui.add(length(&mut center[1]).speed(0.2));
            });
            if let Some(sc) = &mut e.scale {
                ui.horizontal(|ui| {
                    ui.label(tr("Scale"));
                    scale_combo(ui, "detail_scale", sc);
                });
            }
        }
    }
    if !matches!(e.kind, ViewKind::Section { .. }) {
        ui.checkbox(&mut e.hidden, tr("Hidden lines (dashed)"));
    }
    ui.checkbox(&mut e.marks, tr("Centre marks and centrelines"));
    if e != v {
        let key = panel_key(ui, 1 << 20 | u64::from(id));
        s.doc.edit(key, |c| {
            if let Some(x) = c.sheets[sheet].views.iter_mut().find(|x| x.id == id) {
                *x = e;
            }
        });
    }
    ui.add_space(6.0);
    if ui.button(tr("Delete View")).clicked() {
        s.doc.remove_view(sheet, id);
        s.selected = Item::Sheet;
    }
}

fn dim_panel(s: &mut Session, ui: &mut egui::Ui, sheet: usize, i: usize) {
    let Some(d) = s.doc.contents().sheets[sheet].dims.get(i).cloned() else {
        s.selected = Item::Sheet;
        return;
    };
    ui.heading(tr("Dimension"));
    ui.separator();
    match s.doc.dimension_text(&d) {
        Some(t) => ui.label(RichText::new(t).size(18.0)),
        None => ui.colored_label(ERROR, tr("What it measures is gone from the part")),
    };
    ui.weak(if d.model {
        "From the part's sketch or feature: change it in the part."
    } else {
        "Measured off the view: it follows the part's changes."
    });
    let mut kind = d.kind;
    if DimKind::LINEAR.contains(&kind) {
        ui.horizontal(|ui| {
            for k in DimKind::LINEAR {
                ui.radio_value(&mut kind, k, k.name());
            }
        });
    }
    if kind != d.kind {
        s.doc.edit(None, |c| c.sheets[sheet].dims[i].kind = kind);
    }
    ui.add_space(6.0);
    if ui.button(tr("Delete Dimension")).clicked() {
        s.doc.edit(None, |c| {
            c.sheets[sheet].dims.remove(i);
        });
        s.selected = Item::Sheet;
    }
}

fn note_panel(s: &mut Session, ui: &mut egui::Ui, sheet: usize, i: usize) {
    let Some(n) = s.doc.contents().sheets[sheet].notes.get(i).cloned() else {
        s.selected = Item::Sheet;
        return;
    };
    let mut e = n.clone();
    ui.heading(tr("Note"));
    ui.separator();
    ui.add(
        egui::TextEdit::multiline(&mut e.text)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    if e.leader.is_some() {
        if ui.button(tr("Remove Leader")).clicked() {
            e.leader = None;
        }
    } else {
        ui.weak(tr("No leader: add the note with the Note tool on a view to point at it."));
    }
    if e != n {
        let key = panel_key(ui, 2 << 20 | i as u64);
        s.doc.edit(key, |c| c.sheets[sheet].notes[i] = e);
    }
    ui.add_space(6.0);
    if ui.button(tr("Delete Note")).clicked() {
        s.doc.edit(None, |c| {
            c.sheets[sheet].notes.remove(i);
        });
        s.selected = Item::Sheet;
    }
}

/// What is under the mouse in Select: a dimension's text, a note, then a view.
fn hit(doc: &DrawingDocument, sheet: usize, p: P2, reach: f64) -> Item {
    let sh = &doc.contents().sheets[sheet];
    for (i, d) in sh.dims.iter().enumerate().rev() {
        if doc
            .dimension_at(d)
            .is_some_and(|t| (t[0] - p[0]).hypot(t[1] - p[1]) < reach.max(4.0))
        {
            return Item::Dim(i);
        }
    }
    for (i, n) in sh.notes.iter().enumerate().rev() {
        let (lo, hi) = DrawingDocument::note_rect(n);
        if (lo[0]..=hi[0]).contains(&p[0]) && (lo[1]..=hi[1]).contains(&p[1]) {
            return Item::Note(i);
        }
    }
    doc.view_at(sheet, p).map_or(Item::Sheet, Item::View)
}

fn canvas(app: &mut App, ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let Some(s) = &mut app.drw else { return };
    if s.fit {
        s.zoom_to_fit(rect);
    }
    let sheet = s.sheet();
    let painter = ui.painter_at(rect);
    let page = s.doc.page(sheet);
    let [w, h] = page.size;
    let corner = |p: P2| s.to_screen(rect, p);
    let paper = Rect::from_two_pos(corner([0.0, 0.0]), corner([w, h]));
    painter.rect_filled(
        paper.translate(egui::vec2(3.0, 3.0)),
        0.0,
        Color32::from_black_alpha(60),
    );
    painter.rect_filled(paper, 0.0, Color32::WHITE);
    paint_page(&painter, &page, |p| s.to_screen(rect, p), s.zoom);

    let reach = f64::from(REACH / s.zoom);
    let mouse = resp.hover_pos().map(|m| s.to_sheet(rect, m));
    // Highlight what is picked, and what a click would pick.
    let outline = |painter: &egui::Painter, pts: &[P2], color: Color32| {
        let pts: Vec<Pos2> = pts.iter().map(|&p| s.to_screen(rect, p)).collect();
        match pts.len() {
            0 => {}
            1 => {
                painter.circle_stroke(pts[0], 5.0, Stroke::new(2.0_f32, color));
            }
            _ => {
                painter.add(egui::Shape::line(pts, Stroke::new(2.5_f32, color)));
            }
        }
    };
    let box_at = |painter: &egui::Painter, lo: P2, hi: P2, color: Color32| {
        let r = Rect::from_two_pos(s.to_screen(rect, lo), s.to_screen(rect, hi)).expand(3.0);
        painter.rect_stroke(r, 2.0, Stroke::new(1.5_f32, color), egui::StrokeKind::Outside);
    };
    let sh = s.doc.contents().sheets[sheet].clone();
    let mark_item = |painter: &egui::Painter, item: Item, color: Color32| match item {
        Item::View(id) => {
            if let Some((lo, hi)) = s.doc.view_rect(id) {
                box_at(painter, lo, hi, color);
            }
        }
        Item::Dim(i) => {
            if let Some(t) = sh.dims.get(i).and_then(|d| s.doc.dimension_at(d)) {
                box_at(painter, [t[0] - 4.0, t[1] - 3.0], [t[0] + 4.0, t[1] + 3.0], color);
            }
        }
        Item::Note(i) => {
            if let Some(n) = sh.notes.get(i) {
                let (lo, hi) = DrawingDocument::note_rect(n);
                box_at(painter, lo, hi, color);
            }
        }
        Item::Sheet => {}
    };
    mark_item(&painter, s.selected, PICKED);
    if let Tool::Dimension(picks) = &s.tool {
        for (view, p) in picks {
            outline(&painter, &s.doc.outline(*view, p), PICKED);
        }
    }
    let wants_geometry = matches!(s.tool, Tool::Dimension(_) | Tool::Note);
    let mut hover_pick = None;
    if let Some(m) = mouse {
        if wants_geometry {
            hover_pick = s.doc.pick(sheet, m, reach);
            if let Some((view, p)) = &hover_pick {
                outline(&painter, &s.doc.outline(*view, p), HOVER);
            }
        } else if s.tool == Tool::Select && s.grab.is_none() {
            let item = hit(&s.doc, sheet, m, reach);
            if item != s.selected {
                mark_item(&painter, item, HOVER);
            }
        } else if let Some(view) = s.doc.view_at(sheet, m) {
            mark_item(&painter, Item::View(view), HOVER);
        }
    }

    // Wheel zoom about the mouse, middle or right drag (or a drag on empty sheet) to pan.
    if let (Some(m), true) = (resp.hover_pos(), resp.hovered()) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let at = s.to_sheet(rect, m);
            s.zoom = (s.zoom * (scroll * 0.002).exp()).clamp(0.05, 200.0);
            let moved = s.to_sheet(rect, m);
            s.center = [s.center[0] + at[0] - moved[0], s.center[1] + at[1] - moved[1]];
        }
    }
    let pan = |s: &mut Session| {
        let d = resp.drag_delta();
        s.center[0] -= f64::from(d.x / s.zoom);
        s.center[1] += f64::from(d.y / s.zoom);
    };
    if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
        pan(s);
    }
    if resp.drag_started_by(egui::PointerButton::Primary) && s.tool == Tool::Select {
        if let Some(start) = resp.interact_pointer_pos() {
            let at = s.to_sheet(rect, start - resp.drag_delta());
            let item = hit(&s.doc, sheet, at, reach);
            if item != Item::Sheet {
                s.selected = item;
                let key = s.next_drag();
                s.grab = Some(Grab { item, last: at, key });
            }
        }
    }
    if resp.dragged_by(egui::PointerButton::Primary) {
        let pos = resp.interact_pointer_pos().map(|p| s.to_sheet(rect, p));
        match (s.grab.as_mut(), pos) {
            (Some(g), Some(now)) => {
                let d = [now[0] - g.last[0], now[1] - g.last[1]];
                g.last = now;
                let (item, key) = (g.item, g.key);
                s.doc.edit(Some(key), |c| {
                    let sh = &mut c.sheets[sheet];
                    let shift = |p: &mut P2| *p = [p[0] + d[0], p[1] + d[1]];
                    match item {
                        Item::View(id) => {
                            if let Some(v) = sh.views.iter_mut().find(|v| v.id == id) {
                                shift(&mut v.at);
                            }
                        }
                        Item::Dim(i) => shift(&mut sh.dims[i].at),
                        Item::Note(i) => shift(&mut sh.notes[i].at),
                        Item::Sheet => {}
                    }
                });
            }
            _ => pan(s),
        }
    }
    if resp.drag_stopped() && s.grab.take().is_some() {
        s.doc.end_merge();
    }
    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos().map(|m| s.to_sheet(rect, m)) {
            let msg = click(s, sheet, p, reach, hover_pick);
            if let Some(msg) = msg {
                app.status = msg;
            }
        }
    }
    if resp.double_clicked() {
        if let Some(s) = &mut app.drw {
            if s.tool == Tool::Select && s.selected == Item::Sheet {
                s.fit = true;
            }
        }
    }
}

/// A click on the sheet with the current tool; returns a status message.
fn click(s: &mut Session, sheet: usize, p: P2, reach: f64, picked: Option<(u32, Pick)>) -> Option<String> {
    match s.tool.clone() {
        Tool::Select => {
            s.selected = hit(&s.doc, sheet, p, reach);
            None
        }
        Tool::Section { vertical } => {
            let view = s.doc.view_at(sheet, p)?;
            let at = s.doc.to_view(view, p)?;
            s.tool = Tool::Select;
            match s.doc.add_section(sheet, view, vertical, at) {
                Some(id) => {
                    s.selected = Item::View(id);
                    Some("Section view added: drag it along its line, or set the line in Properties".into())
                }
                None => Some("Sections are cut from model, projected or section views".into()),
            }
        }
        Tool::Detail => {
            let view = s.doc.view_at(sheet, p)?;
            let at = s.doc.to_view(view, p)?;
            let (lo, hi) = s.doc.view_rect(view)?;
            let scale = s.doc.view_scale(view)?;
            let radius = ((hi[0] - lo[0]).min(hi[1] - lo[1]) / scale * 0.25).max(1.0);
            s.tool = Tool::Select;
            match s.doc.add_detail(sheet, view, at, radius) {
                Some(id) => {
                    s.selected = Item::View(id);
                    Some("Detail view added: set its size and scale in Properties".into())
                }
                None => Some("Details are taken from model, projected or section views".into()),
            }
        }
        Tool::Dimension(mut picks) => {
            let Some((view, pick)) = picked else {
                return Some("Nothing to dimension there: click an edge, a circle or a vertex".into());
            };
            // Both points come from one view.
            picks.retain(|(v, _)| *v == view);
            let whole = matches!(pick, Pick::Circle(_) | Pick::Edge(..)) && picks.is_empty();
            if matches!(pick, Pick::Edge(..)) && !picks.is_empty() {
                return Some("Click a vertex for the second point".into());
            }
            picks.push((view, pick));
            if !whole && picks.len() < 2 {
                s.tool = Tool::Dimension(picks);
                return None;
            }
            let picks: Vec<Pick> = picks.into_iter().map(|(_, k)| k).collect();
            let pts: Vec<P2> = picks.iter().flat_map(|k| s.doc.outline(view, k)).collect();
            let n = pts.len().max(1) as f64;
            let mid = pts.iter().fold([0.0, 0.0], |a, q| [a[0] + q[0] / n, a[1] + q[1] / n]);
            let top = pts.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max);
            let text = if matches!(picks[..], [Pick::Circle(_)]) {
                [p[0] + 10.0, p[1] + 10.0]
            } else if pts.len() == 2 && (pts[0][0] - pts[1][0]).abs() < 1e-6 {
                // A vertical distance: beside it, on the side clicked.
                [pts[0][0] + if p[0] < pts[0][0] { -10.0 } else { 10.0 }, mid[1]]
            } else {
                [mid[0], top.max(mid[1]) + 10.0]
            };
            s.tool = Tool::Dimension(Vec::new());
            match s.doc.add_dimension(sheet, view, &picks, text) {
                Some(i) => {
                    s.selected = Item::Dim(i);
                    Some("Dimension added: drag its text to place it".into())
                }
                None => Some("Those cannot be dimensioned together".into()),
            }
        }
        Tool::Note => {
            let leader = s.doc.view_at(sheet, p).and_then(|v| {
                // The leader snaps to a vertex; on an edge or circle it ends where clicked.
                let at = picked
                    .as_ref()
                    .filter(|(pv, k)| *pv == v && matches!(k, Pick::Vertex(_)))
                    .and_then(|(_, k)| s.doc.outline(v, k).first().copied());
                Some((v, s.doc.to_view(v, at.unwrap_or(p))?))
            });
            let at = if leader.is_some() {
                [p[0] + 15.0, p[1] + 12.0]
            } else {
                p
            };
            s.doc.edit(None, |c| {
                c.sheets[sheet].notes.push(Note {
                    text: "NOTE".into(),
                    at,
                    leader,
                })
            });
            s.tool = Tool::Select;
            s.selected = Item::Note(s.doc.contents().sheets[sheet].notes.len() - 1);
            Some("Note added: type its text in Properties".into())
        }
    }
}

/// Paints a page's marks: line widths and dashes in sheet mm, scaled like the paper.
fn paint_page(painter: &egui::Painter, page: &Page, map: impl Fn(P2) -> Pos2, zoom: f32) {
    for m in &page.marks {
        let color = if m.layer == Layer::Hidden {
            Color32::from_rgb(70, 70, 80)
        } else {
            INK
        };
        match &m.shape {
            Shape::Line(pts) => {
                let pts: Vec<Pos2> = pts.iter().map(|&p| map(p)).collect();
                let stroke = Stroke::new((m.layer.width() as f32 * zoom).max(0.7), color);
                match m.layer.dash() {
                    Dash::Solid => {
                        painter.add(egui::Shape::line(pts, stroke));
                    }
                    d => {
                        let pat: Vec<f32> = dash_pattern(d).iter().map(|&x| (x as f32 * zoom).max(1.5)).collect();
                        let on: Vec<f32> = pat.iter().step_by(2).copied().collect();
                        let off: Vec<f32> = pat.iter().skip(1).step_by(2).copied().collect();
                        painter.extend(egui::Shape::dashed_line_with_offset(&pts, stroke, &on, &off, 0.0));
                    }
                }
            }
            Shape::Fill(pts) => {
                let pts: Vec<Pos2> = pts.iter().map(|&p| map(p)).collect();
                painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
            }
            Shape::Text {
                at,
                height,
                text,
                align,
            } => {
                let size = *height as f32 * zoom / 0.72;
                if size < 3.0 {
                    continue;
                }
                let anchor = match align {
                    Align::Left => Align2::LEFT_BOTTOM,
                    Align::Center => Align2::CENTER_BOTTOM,
                    Align::Right => Align2::RIGHT_BOTTOM,
                };
                // egui places the text box's bottom (below the baseline) at the point.
                let at = map(*at) + egui::vec2(0.0, size * 0.22);
                painter.text(at, anchor, text, FontId::proportional(size), color);
            }
        }
    }
}
