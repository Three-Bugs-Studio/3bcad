//! Keyboard Shortcuts window (F1 or the ? button in the ribbon).

use eframe::egui::{self, RichText};

use crate::app::App;

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
