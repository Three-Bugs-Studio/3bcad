//! The open drawing: which sheet is on screen and how it is panned and zoomed, what is picked,
//! the tool in use, and the drawing exports.

use std::path::{Path, PathBuf};
use crate::i18n::tr;
use std::time::{Duration, Instant};

use eframe::egui::{Pos2, Rect};
use threebcad_core::drawing::Pick;
use threebcad_core::DrawingDocument;
use threebcad_io::drawing::P2;

use crate::app::App;

/// How often the part file is checked for changes.
const REFRESH: Duration = Duration::from_secs(1);

/// Something picked on the sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Sheet,
    View(u32),
    Dim(usize),
    Note(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tool {
    Select,
    Section {
        vertical: bool,
    },
    Detail,
    /// The points picked so far, each with its view.
    Dimension(Vec<(u32, Pick)>),
    Note,
}

impl Tool {
    pub fn hint(&self) -> &'static str {
        match self {
            Tool::Select => "",
            Tool::Section { vertical: true } => "Click a view where the vertical section line goes",
            Tool::Section { vertical: false } => "Click a view where the horizontal section line goes",
            Tool::Detail => "Click the middle of the area of a view to show larger",
            Tool::Dimension(p) if p.is_empty() => "Click an edge, a circle or a first vertex",
            Tool::Dimension(_) => "Click the second vertex",
            Tool::Note => "Click what the note points at (or empty sheet for no leader)",
        }
    }
}

/// An item being dragged on the sheet, and where the mouse was (sheet mm).
pub struct Grab {
    pub item: Item,
    pub last: P2,
    pub key: u64,
}

pub struct Session {
    pub doc: DrawingDocument,
    pub sheet: usize,
    pub selected: Item,
    pub tool: Tool,
    /// Screen points per sheet mm, and the sheet point at the middle of the canvas.
    pub zoom: f32,
    pub center: P2,
    /// Zoom to fit on the next frame.
    pub fit: bool,
    pub grab: Option<Grab>,
    checked: Instant,
    drags: u64,
}

impl Session {
    pub fn new(doc: DrawingDocument) -> Self {
        Self {
            doc,
            sheet: 0,
            selected: Item::Sheet,
            tool: Tool::Select,
            zoom: 1.0,
            center: [0.0, 0.0],
            fit: true,
            grab: None,
            checked: Instant::now(),
            drags: 0,
        }
    }

    /// Re-reads the part if it changed on disk, at most once a second; true when it did.
    pub fn refresh(&mut self) -> bool {
        if self.checked.elapsed() < REFRESH {
            return false;
        }
        self.checked = Instant::now();
        self.doc.refresh()
    }

    /// The sheet shown, kept in range after sheets are removed or undone.
    pub fn sheet(&mut self) -> usize {
        self.sheet = self.sheet.min(self.doc.contents().sheets.len() - 1);
        self.sheet
    }

    pub fn to_screen(&self, rect: Rect, p: P2) -> Pos2 {
        let c = rect.center();
        Pos2::new(
            c.x + (p[0] - self.center[0]) as f32 * self.zoom,
            c.y - (p[1] - self.center[1]) as f32 * self.zoom,
        )
    }

    pub fn to_sheet(&self, rect: Rect, p: Pos2) -> P2 {
        let c = rect.center();
        [
            self.center[0] + f64::from((p.x - c.x) / self.zoom),
            self.center[1] - f64::from((p.y - c.y) / self.zoom),
        ]
    }

    /// Fits the whole sheet in `rect`.
    pub fn zoom_to_fit(&mut self, rect: Rect) {
        let sheet = self.sheet();
        let [w, h] = self.doc.contents().sheets[sheet].size.size();
        self.zoom = (rect.width() / w as f32).min(rect.height() / h as f32) * 0.94;
        self.center = [w / 2.0, h / 2.0];
        self.fit = false;
    }

    /// A new undo-merge key for a drag.
    pub fn next_drag(&mut self) -> u64 {
        self.drags += 1;
        (7 << 40) | self.drags
    }
}

/// Files that can be shown on a drawing from the recent list: parts.
pub fn drawable(recent: &[PathBuf]) -> Vec<PathBuf> {
    recent
        .iter()
        .filter(|p| p.exists() && p.extension().is_some_and(|e| e == crate::files::EXT))
        .cloned()
        .collect()
}

impl App {
    /// Shows a part on the open drawing.
    pub(crate) fn drawing_model(&mut self, file: &Path) {
        let Some(s) = self.drw.as_mut() else { return };
        self.status = match s.doc.set_model(file) {
            Ok(()) => {
                self.files.settings.add_recent(file);
                self.files.settings_changed = true;
                if s.doc.contents().sheets.iter().all(|sh| sh.views.is_empty()) {
                    let sheet = s.sheet();
                    s.doc.standard_views(sheet);
                    "Placed the front view and two projected views: drag them, or add more from the Drawing tab".into()
                } else {
                    format!("The views now show {}", file.display())
                }
            }
            Err(e) => format!("Could not use the part: {e:#}"),
        };
    }

    pub(crate) fn drawing_model_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("3BCAD part", &[crate::files::EXT])
            .pick_file()
        {
            self.drawing_model(&path);
        }
    }

    /// Opens the drawing's part to change it, with a way back to the drawing.
    pub(crate) fn edit_drawing_part(&mut self) {
        let Some(model) = self.drw.as_ref().and_then(|s| s.doc.contents().model.clone()) else {
            return;
        };
        match &self.files.path {
            Some(me) => {
                self.files.return_to = Some(me.clone());
                self.request(crate::files::Pending::Open(model));
            }
            None => self.status = tr("Save the drawing first, so 3BCAD can bring you back to it").into(),
        }
    }

    /// Exports every sheet: one PDF, or an SVG or DXF file per sheet.
    pub(crate) fn export_drawing(&mut self, ext: &str) {
        let Some(s) = self.drw.as_ref() else { return };
        let (what, name) = match ext {
            "pdf" => ("PDF", "PDF (every sheet, true scale)"),
            "svg" => ("SVG", "SVG (a file per sheet)"),
            _ => ("DXF", "DXF (a file per sheet, layered)"),
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter(name, &[ext])
            .set_file_name(format!("{}.{ext}", s.doc.name))
            .save_file()
        else {
            return;
        };
        let pages = s.doc.pages();
        self.status = match threebcad_io::drawing::export(&path, &pages) {
            Ok(files) if files.len() == 1 => format!("Exported {what}: {}", files[0].display()),
            Ok(files) => format!("Exported {} {what} files next to {}", files.len(), path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }
}
