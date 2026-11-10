//! The 3D Print tab: export with a mesh quality, Send to Slicer, the printer and how the part sits
//! on its bed, and the printability checks.

use eframe::egui::{self, Color32, RichText};
use crate::i18n::tr;
use threebcad_core::DEFAULT_PRINT_DOWN;
use threebcad_io::printer::{self, PRINTERS};
use threebcad_io::Slicer;
use threebcad_kernel::topology::{unit, EdgeCurve};

use crate::app::{App, ExportFormat, ModelRef};
use crate::print::{OVERHANG, THIN, TOO_THIN};
use crate::ui::icons;
use crate::ui::properties::{length, number, units};

/// Export quality presets: chord tolerance (mm) and angle tolerance (degrees).
const QUALITIES: [(&str, f64, f64); 2] = [("Coarse", 0.1, 30.0), ("Fine", 0.01, 10.0)];
const CUSTOM: &str = "Custom";

fn tile(ui: &mut egui::Ui, icon: &str, label: &str, on: bool, tip: &str) -> bool {
    icons::tile(ui, icon, label, true, on).on_hover_text(tip).clicked()
}

fn color(c: [u8; 4]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn tab(app: &mut App, ui: &mut egui::Ui) {
    export_configuration(app, ui);
    for (format, label) in [
        (ExportFormat::Stl, "Export STL"),
        (ExportFormat::ThreeMf, "Export 3MF"),
        (ExportFormat::Step, "Export STEP"),
    ] {
        let tip = if format == ExportFormat::Step {
            "Save the exact part (model position) for other CAD programs"
        } else {
            "Save the part as it sits on the printer's bed, meshed at the quality on the right"
        };
        if tile(ui, "export", label, false, tip) {
            app.export(format);
        }
    }
    ui.separator();
    quality(app, ui);
    ui.separator();
    slicer(app, ui);
    ui.separator();
    bed(app, ui);
    ui.separator();
    checks(app, ui);
}

/// Which configuration the exports are made in, for a part that has several.
fn export_configuration(app: &mut App, ui: &mut egui::Ui) {
    let h = app.doc.history();
    if h.configurations().len() < 2 {
        return;
    }
    let names: Vec<String> = h.configurations().iter().map(|c| c.name.clone()).collect();
    let chosen = app
        .export_config
        .filter(|&i| i < names.len())
        .unwrap_or(h.active_configuration());
    ui.vertical(|ui| {
        ui.label(tr("Export configuration"));
        egui::ComboBox::from_id_salt("export_configuration")
            .selected_text(&names[chosen])
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(i == chosen, n).clicked() {
                        app.export_config = Some(i);
                    }
                }
            })
            .response
            .on_hover_text(tr("Exports and Send to Slicer build the part in this configuration"));
    });
    ui.separator();
}

fn quality(app: &mut App, ui: &mut egui::Ui) {
    let s = &mut app.files.settings;
    let mut changed = false;
    ui.vertical(|ui| {
        let current = QUALITIES
            .iter()
            .find(|q| q.1 == s.chord && q.2 == s.angle_deg)
            .map_or(CUSTOM, |q| q.0);
        egui::ComboBox::from_id_salt("export_quality")
            .selected_text(format!("Quality: {current}"))
            .show_ui(ui, |ui| {
                for (name, chord, angle) in QUALITIES {
                    if ui.selectable_label(current == name, name).clicked() {
                        (s.chord, s.angle_deg) = (chord, angle);
                        changed = true;
                    }
                }
            });
        ui.horizontal(|ui| {
            ui.label(tr("Chord"));
            changed |= ui
                .add(length(&mut s.chord).speed(0.001).range(0.001..=1.0))
                .on_hover_text(tr("Furthest a triangle may lie from the true surface"))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label(tr("Angle"));
            changed |= ui
                .add(number(&mut s.angle_deg).speed(0.5).range(1.0..=60.0).suffix("°"))
                .on_hover_text(tr("Most a curved surface may turn within one triangle"))
                .changed();
        });
    });
    app.files.settings_changed |= changed;
    // Meshing a big part takes a moment, so count only once a value is set, not while dragging.
    let dragging = ui.ctx().input(|i| i.pointer.any_down());
    let empty = app.doc.bodies().is_empty() && app.doc.built().meshes.is_empty();
    if app.export_triangles().is_none() && !dragging && !empty {
        app.export_objects();
    }
    ui.vertical(|ui| {
        ui.add_space(14.0);
        match app.export_triangles() {
            Some(n) => ui.label(format!("{n} triangles")),
            None => ui.weak(tr("… triangles")),
        }
        .on_hover_text(tr("Triangles in an STL or 3MF export at this quality"));
    });
}

fn slicer(app: &mut App, ui: &mut egui::Ui) {
    let s = &mut app.files.settings;
    let before = s.slicer;
    let (mut pick, mut forget) = (false, false);
    ui.vertical(|ui| {
        egui::ComboBox::from_id_salt("slicer")
            .selected_text(s.slicer.name())
            .show_ui(ui, |ui| {
                for sl in Slicer::ALL {
                    ui.selectable_value(&mut s.slicer, sl, sl.name());
                }
            });
        let path = s.slicer_paths[s.slicer as usize].clone();
        let tip = match &path {
            Some(p) => format!("Using {}", p.display()),
            None => "Using the default install location".into(),
        };
        ui.horizontal(|ui| {
            pick = ui.small_button(tr("Locate…")).on_hover_text(tip).clicked();
            if path.is_some()
                && ui
                    .small_button(tr("Default"))
                    .on_hover_text(tr("Forget the located program"))
                    .clicked()
            {
                s.slicer_paths[s.slicer as usize] = None;
                forget = true;
            }
        });
    });
    app.files.settings_changed |= app.files.settings.slicer != before || forget;
    if pick {
        locate(app, false);
    }
    if tile(
        ui,
        "slicer",
        "Send to Slicer",
        false,
        "Open the part in the chosen slicer, on its bed",
    ) {
        app.send_to_slicer();
    }
}

fn bed(app: &mut App, ui: &mut egui::Ui) {
    let s = &mut app.files.settings;
    let mut changed = false;
    ui.vertical(|ui| {
        egui::ComboBox::from_id_salt("printer")
            .selected_text(s.printer.clone())
            .show_ui(ui, |ui| {
                for p in PRINTERS {
                    if ui.selectable_label(s.printer == p.name, p.name).clicked() {
                        s.printer = p.name.into();
                        s.bed = p.bed;
                        changed = true;
                    }
                }
            });
        ui.horizontal(|ui| {
            for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                if ui
                    .add(length(&mut s.bed[i]).speed(1.0).range(10.0..=2000.0))
                    .on_hover_text(format!("Build volume {axis}"))
                    .changed()
                {
                    s.printer = "Custom".into();
                    changed = true;
                }
            }
        });
        let r = &app.print;
        let over = printer::overflow(r.size, s.bed);
        let u = units();
        if over == [0.0; 3] {
            ui.colored_label(
                Color32::from_rgb(30, 130, 50),
                format!(
                    "✔ Fits: {} × {} × {}",
                    u.length(r.size[0]),
                    u.length(r.size[1]),
                    u.length(r.size[2])
                ),
            );
        } else {
            let axes: Vec<String> = ["X", "Y", "Z"]
                .iter()
                .zip(over)
                .filter(|(_, o)| *o > 0.0)
                .map(|(a, o)| format!("{} too long in {a}", u.length(o)))
                .collect();
            ui.colored_label(crate::ui::theme::ERROR, format!("⚠ {}", axes.join(", ")));
        }
    });
    app.files.settings_changed |= changed;
    ui.vertical(|ui| {
        if icons::small(ui, "lay_flat", "Lay Flat", true, false)
            .on_hover_text(tr("Put the selected flat face on the bed, or point a selected edge or axis up"))
            .clicked()
        {
            lay_flat(app);
        }
        if icons::small(
            ui,
            "rebuild",
            "Reset",
            app.doc.print_down() != DEFAULT_PRINT_DOWN,
            false,
        )
        .on_hover_text(tr("Print the part on its bottom again"))
        .clicked()
        {
            app.doc.set_print_down(DEFAULT_PRINT_DOWN);
            app.status = tr("The part rests on its bottom").into();
        }
    });
}

/// The bed face from the selection: a flat face goes down, a straight edge or axis points up.
fn lay_flat(app: &mut App) {
    let current = app.doc.print_down();
    let down = app.selection.iter().find_map(|&r| match r {
        ModelRef::Face { body, face } => app
            .doc
            .bodies()
            .get(body)?
            .topo
            .faces
            .get(face)?
            .plane
            .map(|p| p.normal()),
        ModelRef::Edge { body, edge } => match app.doc.bodies().get(body)?.topo.edges.get(edge)?.curve {
            EdgeCurve::Line { a, b } => Some(unit([a[0] - b[0], a[1] - b[1], a[2] - b[2]])),
            _ => None,
        },
        ModelRef::Feature(id) => app.doc.axis_of(id).map(|(_, d)| d.map(|c| -c)),
        _ => None,
    });
    let Some(mut down) = down else {
        app.status = tr("Lay Flat: select a flat face to put on the bed (or an edge or axis to point up)").into();
        return;
    };
    // A line has no up of its own: keep the end nearer the current top on top.
    if !app.selection.iter().any(|r| matches!(r, ModelRef::Face { .. }))
        && down.iter().zip(current).map(|(a, b)| a * b).sum::<f64>() < 0.0
    {
        down = down.map(|c| -c);
    }
    app.doc.set_print_down(down);
    app.selection.clear();
    app.status = tr("The part is laid out on the bed").into();
}

fn checks(app: &mut App, ui: &mut egui::Ui) {
    if tile(
        ui,
        "overhang",
        "Overhangs",
        app.show_overhangs,
        "Colour faces that need support when printed as laid out",
    ) {
        app.show_overhangs = !app.show_overhangs;
    }
    if tile(
        ui,
        "thin_wall",
        "Thin Walls",
        app.show_thin,
        "Colour walls too thin for the nozzle (red) or thinner than the thinnest wall (orange)",
    ) {
        app.show_thin = !app.show_thin;
    }
    let s = &mut app.files.settings;
    let mut changed = false;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(tr("Overhang"));
            changed |= ui
                .add(number(&mut s.overhang_deg).speed(0.5).range(1.0..=89.0).suffix("°"))
                .on_hover_text(tr("The steepest overhang, from vertical, the printer manages without support"))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label(tr("Nozzle"));
            changed |= ui.add(length(&mut s.nozzle).speed(0.01).range(0.1..=2.0)).changed();
        });
        ui.horizontal(|ui| {
            ui.label(tr("Min wall"));
            changed |= ui
                .add(length(&mut s.min_wall).speed(0.01).range(0.1..=10.0))
                .on_hover_text(tr("Usually two or three nozzle widths"))
                .changed();
        });
    });
    app.files.settings_changed |= changed;
    let r = app.print;
    ui.vertical(|ui| {
        if app.show_overhangs {
            ui.label(RichText::new(format!("■ needs support: {}", units().area(r.overhang))).color(color(OVERHANG)));
        }
        if app.show_thin {
            ui.label(
                RichText::new(format!("■ thinner than the nozzle: {}", units().area(r.too_thin)))
                    .color(color(TOO_THIN)),
            );
            ui.label(RichText::new(format!("■ thin: {}", units().area(r.thin))).color(color(THIN)));
        }
    });
}

/// Lets the user point at the slicer's program; when `retry`, sends the part once it is found.
fn locate(app: &mut App, retry: bool) {
    let slicer = app.files.settings.slicer;
    let mut dialog = rfd::FileDialog::new().set_title(format!("Where is {}?", slicer.name()));
    if cfg!(windows) {
        dialog = dialog.add_filter("Program", &["exe"]);
    }
    let Some(path) = dialog.pick_file() else { return };
    app.files.settings.slicer_paths[slicer as usize] = Some(path);
    app.files.settings_changed = true;
    if retry {
        app.send_to_slicer();
    }
}

/// Asks where a slicer is after Send to Slicer could not find it.
pub fn slicer_prompt(app: &mut App, ctx: &egui::Context) {
    let Some(slicer) = app.slicer_missing else { return };
    let mut answer = None;
    egui::Modal::new(egui::Id::new("slicer_missing")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading(format!("{} was not found", slicer.name()));
        ui.label(format!(
            "3BCAD looked in its usual install folder and on the PATH. Point to the {} program to use it from now on.",
            slicer.name()
        ));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(tr("Locate…")).clicked() {
                answer = Some(true);
            }
            if ui.button(tr("Cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                answer = Some(false);
            }
        });
    });
    if let Some(locate_it) = answer {
        app.slicer_missing = None;
        if locate_it {
            app.files.settings.slicer = slicer;
            locate(app, true);
        }
    }
}
