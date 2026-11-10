//! The Help menu, the Keyboard Shortcuts window (F1 or the ? button in the ribbon), the About
//! window and the notice after a crash.

use eframe::egui::{self, RichText};

use crate::app::{App, VERSION};
use crate::i18n::{tr, trf};

const REPOSITORY: &str = "https://github.com/Three-Bugs-Studio/3bcad";
const USER_GUIDE: &str = "https://three-bugs-studio.github.io/3bcad/";
const BUG_REPORT: &str = "https://github.com/Three-Bugs-Studio/3bcad/issues/new?template=bug_report.yml";

/// The Help menu in the title row.
pub fn menu(app: &mut App, ui: &mut egui::Ui) {
    use crate::commands::{key_of, text, Command};
    let key = |c| key_of(&app.files.settings.keys, c).map_or_else(String::new, |k| text(&k));
    let f1 = key(Command::ShortcutsWindow);
    ui.menu_button(tr("Help"), |ui| {
        if ui
            .add(egui::Button::new(tr("Keyboard Shortcuts")).shortcut_text(f1))
            .clicked()
        {
            ui.close_menu();
            app.show_shortcuts = true;
        }
        ui.hyperlink_to(tr("User Guide and Tutorials"), USER_GUIDE);
        ui.hyperlink_to(tr("Report a Bug"), BUG_REPORT);
        if ui
            .button(tr("Open Log Folder"))
            .on_hover_text(tr("3bcad.log and crash reports, to attach to a bug report"))
            .clicked()
        {
            ui.close_menu();
            if let Err(e) = crate::logfile::open_folder() {
                app.status = trf("Could not open the log folder: {}", &[&e]);
            }
        }
        ui.separator();
        if ui
            .add_enabled(!app.update.busy(), egui::Button::new(tr("Check for Updates…")))
            .clicked()
        {
            ui.close_menu();
            app.update.check(true);
            app.status = tr("Checking for updates…").into();
        }
        if ui.button(tr("About 3BCAD…")).clicked() {
            ui.close_menu();
            app.show_about = true;
        }
    });
}

/// After a crash: where its report is, and how to send it.
pub fn crash_notice(app: &mut App, ctx: &egui::Context) {
    let Some(report) = app.crashed.clone() else {
        return;
    };
    let mut open = true;
    egui::Window::new(tr("3BCAD closed unexpectedly"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .show(ctx, |ui| {
            ui.label(tr("Sorry, the last time it ran, 3BCAD stopped without closing. Your work may be in the autosave it offers."));
            ui.label(trf("A crash report was saved: {}", &[&report.display()]));
            ui.label(tr("Attaching it to a bug report helps fix the cause."));
            ui.horizontal(|ui| {
                if ui.button(tr("Open Log Folder")).clicked() {
                    let _ = crate::logfile::open_folder();
                }
                ui.hyperlink_to(tr("Report a Bug"), BUG_REPORT);
            });
        });
    if !open {
        app.crashed = None;
    }
}

/// (section, [(keys, what they do)]).
const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "View",
        &[
            ("Left drag (or middle drag)", "Orbit"),
            ("Right drag (or Ctrl + middle drag)", "Pan"),
            ("Mouse wheel", "Zoom"),
            ("F, or double-click empty space", "Zoom to fit"),
            (
                "Ctrl+1 … Ctrl+7",
                "Front, Back, Left, Right, Top, Bottom, Isometric view",
            ),
            ("Ctrl+8", "Normal to the selected face or plane"),
            ("Click the view cube", "Look from that face, edge or corner"),
            ("P", "Perspective / orthographic"),
            ("D", "Next display style"),
            ("S", "The tab's commands at the mouse"),
            ("Arrow keys (Shift: 90°)", "Turn the view 15°"),
            ("Ctrl + arrow keys, Page Up / Down", "Pan, zoom"),
            (
                "Ctrl+, (File > Settings)",
                "Settings, where every shortcut can be changed",
            ),
        ],
    ),
    (
        "Selecting",
        &[
            ("Click", "Select a face, edge, vertex or plane"),
            (
                "Ctrl + click",
                "Add to / remove from the selection (Fillet, Chamfer, Measure…)",
            ),
            ("Esc", "Clear the selection"),
        ],
    ),
    (
        "Sketching",
        &[
            ("Sketch tab > Front / Top / Right", "Start a sketch on that plane"),
            ("Double-click a plane in the feature tree", "Start a sketch on it"),
            ("Double-click a sketch", "Edit it"),
            ("Hold Ctrl while drawing", "Turn off snapping and automatic relations"),
            ("Esc, double-click or right-click", "End a line chain / cancel the tool"),
            ("Delete", "Delete the selected sketch geometry"),
            ("Double-click a dimension", "Change its value"),
        ],
    ),
    (
        "Features and tree",
        &[
            ("Ctrl+Z / Ctrl+Y", "Undo / Redo (inside a sketch: sketch edits)"),
            ("F2", "Rename the selected feature"),
            ("Right-click a feature", "Edit, suppress, move, roll back or delete"),
            ("Drag a feature in the tree", "Reorder it"),
            ("Drag the blue bar", "Roll back the part"),
            ("Ctrl+Q (or Ctrl+B)", "Rebuild every feature"),
            ("F1", "Show or hide this window"),
        ],
    ),
    (
        "Assemblies",
        &[
            ("Click a face, Ctrl+click another part's face", "Pick them for a mate"),
            ("Drag a component", "Move it as far as its mates let it"),
            ("Shift+drag a component", "Turn it"),
            ("Double-click a component in the tree", "Edit its part"),
            ("Delete", "Delete the selected component, mate or explode step"),
        ],
    ),
    (
        "Drawings",
        &[
            ("Mouse wheel, middle or right drag", "Zoom, pan the sheet"),
            ("F, or double-click empty sheet", "Fit the sheet"),
            (
                "Drag a view, dimension or note",
                "Move it (projected views stay lined up)",
            ),
            ("Esc", "Stop the tool, then clear the selection"),
            ("Delete", "Delete the selected view, dimension or note"),
        ],
    ),
];

pub fn window(app: &mut App, ctx: &egui::Context) {
    if !app.show_shortcuts {
        return;
    }
    let mut open = true;
    egui::Window::new(tr("Keyboard Shortcuts"))
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            for (section, rows) in SHORTCUTS {
                ui.label(RichText::new(tr(section)).strong());
                egui::Grid::new(*section)
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for (keys, what) in *rows {
                            ui.label(RichText::new(tr(keys)).monospace());
                            ui.label(tr(what));
                            ui.end_row();
                        }
                    });
                ui.add_space(6.0);
            }
        });
    app.show_shortcuts = open;
}

/// Who made 3BCAD, its license and where it lives (click the 3BCAD name, or File > About).
pub fn about(app: &mut App, ctx: &egui::Context) {
    if !app.show_about {
        return;
    }
    let mut open = true;
    egui::Window::new(tr("About 3BCAD"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .default_width(400.0)
        .show(ctx, |ui| {
            ui.heading(format!("3BCAD {VERSION}"));
            ui.label(tr(
                "Open-source parametric CAD for parts, assemblies, drawings and 3D printing.",
            ));
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Made by Three Bugs Studio")).strong());
            ui.label(tr("Created by Duong Phu Dong, with the 3BCAD contributors."));
            ui.hyperlink_to(REPOSITORY, REPOSITORY);
            ui.add_space(6.0);
            ui.label(tr("Copyright © 2026 Three Bugs Studio and the 3BCAD contributors."));
            ui.label(tr(
                "Licensed under the MIT license or the Apache License 2.0, at your option.",
            ));
            ui.weak(
                "Building on 3BCAD? Keep the license files and NOTICE, give your version its own name, and \
                 say it is based on 3BCAD by Three Bugs Studio (see TRADEMARKS.md).",
            );
            ui.add_space(6.0);
            ui.weak(tr("Built with truck (B-Rep kernel), egui and wgpu."));
        });
    app.show_about = open;
}
