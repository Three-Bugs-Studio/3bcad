use std::io;
use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use threebcad_core::{FeatureId, PartDocument};
use threebcad_io::Slicer;
use threebcad_kernel::mesher::{self, PrintMesh};
use threebcad_render::{LineVertex, MeshVertex, OrbitCamera, SceneGeometry};

use crate::ui::{self, ribbon::RibbonTab};

pub const MSAA: u32 = 4;
pub const DEPTH_BITS: u8 = 24;
/// Viewport tessellation: max chord deviation in mm.
const DISPLAY_TOL: f64 = 0.05;
const EDGE_COLOR: [f32; 3] = [0.08, 0.08, 0.1];
const VIEWPORT_BG: egui::Color32 = egui::Color32::from_rgb(214, 220, 230);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Stl,
    ThreeMf,
    Step,
}

impl ExportFormat {
    fn ext(self) -> &'static str {
        match self {
            Self::Stl => "stl",
            Self::ThreeMf => "3mf",
            Self::Step => "step",
        }
    }
}

pub struct App {
    pub doc: PartDocument,
    pub camera: OrbitCamera,
    pub selected: Option<FeatureId>,
    pub tab: RibbonTab,
    pub status: String,
    /// Export tessellation: max chord deviation in mm.
    pub export_tol: f64,
    pub slicer: Slicer,
    pub geometry: Arc<SceneGeometry>,
    pub geometry_version: u64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, &'static str> {
        let rs = cc.wgpu_render_state.as_ref().ok_or("3BCAD needs the wgpu renderer")?;
        threebcad_render::install(rs, eframe::egui_wgpu::depth_format_from_bits(DEPTH_BITS, 0), MSAA);
        let mut app = Self {
            doc: PartDocument::demo(),
            camera: OrbitCamera::default(),
            selected: None,
            tab: RibbonTab::Features,
            status: "Ready".into(),
            export_tol: 0.01,
            slicer: Slicer::OrcaSlicer,
            geometry: Arc::default(),
            geometry_version: u64::MAX,
        };
        app.sync_geometry();
        app.zoom_to_fit();
        Ok(app)
    }

    /// Re-tessellates when the document has rebuilt since the last frame.
    fn sync_geometry(&mut self) {
        if self.geometry_version == self.doc.revision() {
            return;
        }
        let mut g = SceneGeometry::default();
        g.add_grid(100.0, 5.0);
        for (_, body) in self.doc.bodies() {
            let m = mesher::render_mesh(body, DISPLAY_TOL);
            g.triangles.extend(
                m.positions
                    .iter()
                    .zip(&m.normals)
                    .map(|(&pos, &nor)| MeshVertex { pos, nor }),
            );
            g.lines
                .extend(m.edges.iter().map(|&pos| LineVertex { pos, color: EDGE_COLOR }));
        }
        self.geometry = Arc::new(g);
        self.geometry_version = self.doc.revision();
    }

    pub fn zoom_to_fit(&mut self) {
        if let Some((min, max)) = self.geometry.bounds() {
            self.camera.fit(min, max);
        }
    }

    pub fn export(&mut self, format: ExportFormat) {
        let ext = format.ext();
        let Some(path) = rfd::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(format!("{}.{ext}", self.doc.name))
            .save_file()
        else {
            return;
        };
        self.status = match self.write(format, &path) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    pub fn send_to_slicer(&mut self) {
        let path = std::env::temp_dir().join(format!("{}.3mf", self.doc.name));
        let name = self.slicer.name();
        self.status = match self
            .write(ExportFormat::ThreeMf, &path)
            .and_then(|()| self.slicer.launch(&path))
        {
            Ok(_) => format!("Opened {} in {name}", path.display()),
            Err(e) => format!("Could not open {name}: {e}"),
        };
    }

    fn write(&self, format: ExportFormat, path: &Path) -> io::Result<()> {
        let bodies = self.doc.bodies();
        if bodies.is_empty() {
            return Err(io::Error::other("the part has no bodies"));
        }
        let meshes = || -> Vec<PrintMesh> {
            bodies
                .iter()
                .map(|(_, s)| mesher::print_mesh(s, self.export_tol))
                .collect()
        };
        match format {
            ExportFormat::Stl => threebcad_io::write_stl(path, &meshes()),
            ExportFormat::ThreeMf => threebcad_io::write_3mf(path, &meshes()),
            ExportFormat::Step => threebcad_io::write_step(path, bodies.iter().map(|(_, s)| s)),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (undo, redo) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)),
            )
        });
        if undo {
            self.doc.undo();
        }
        if redo {
            self.doc.redo();
        }
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::F)) {
            self.zoom_to_fit();
        }
        if self.selected.is_some_and(|id| self.doc.history().get(id).is_none()) {
            self.selected = None;
        }

        ui::ribbon::show(self, ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} bodies  |  MMGS (mm)", self.doc.bodies().len()));
                });
            });
        });
        ui::tree_view::show(self, ctx);
        ui::properties::show(self, ctx);
        self.sync_geometry();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(VIEWPORT_BG))
            .show(ctx, |ui| ui::viewport::show(self, ui));
    }
}
