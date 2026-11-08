//! Keyboard Shortcuts window (F1 or the ? button in the ribbon) and the About window.

use eframe::egui::{self, RichText};

use crate::app::{App, VERSION};

const REPOSITORY: &str = "https://github.com/Three-Bugs-Studio/3bcad";

/// (section, [(keys, what they do)]).
const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "View",
        &[
            ("Left drag (or middle drag)", "Orbit"),
            ("Right drag (or Ctrl + middle drag)", "Pan"),
            ("Mouse wheel", "Zoom"),
            ("F, or double-click empty space", "Zoom to fit"),
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
    egui::Window::new("Keyboard Shortcuts")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            for (section, rows) in SHORTCUTS {
                ui.label(RichText::new(*section).strong());
                egui::Grid::new(*section)
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for (keys, what) in *rows {
                            ui.label(RichText::new(*keys).monospace());
                            ui.label(*what);
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
    egui::Window::new("About 3BCAD")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .default_width(400.0)
        .show(ctx, |ui| {
            ui.heading(format!("3BCAD {VERSION}"));
            ui.label("Open-source parametric CAD for parts, assemblies, drawings and 3D printing.");
            ui.add_space(6.0);
            ui.label(RichText::new("Made by Three Bugs Studio").strong());
            ui.label("Created by Duong Phu Dong, with the 3BCAD contributors.");
            ui.hyperlink_to(REPOSITORY, REPOSITORY);
            ui.add_space(6.0);
            ui.label("Copyright © 2026 Three Bugs Studio and the 3BCAD contributors.");
            ui.label("Licensed under the MIT license or the Apache License 2.0, at your option.");
            ui.weak(
                "Building on 3BCAD? Keep the license files and NOTICE, give your version its own name, and \
                 say it is based on 3BCAD by Three Bugs Studio (see TRADEMARKS.md).",
            );
            ui.add_space(6.0);
            ui.weak("Built with truck (B-Rep kernel), egui and wgpu.");
        });
    app.show_about = open;
}
