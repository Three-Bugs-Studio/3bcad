use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use glam::Vec3;
use threebcad_core::{Feature, FeatureId, FeatureKind, PartDocument};
use threebcad_io::Slicer;
use threebcad_kernel::mesher::{self, PrintMesh};
use threebcad_kernel::Plane;
use threebcad_render::{MeshVertex, OrbitCamera, SceneGeometry};

use crate::ui::{self, ribbon::RibbonTab, sketch_mode::SketchSession};

/// From the workspace `version`; release tags must match it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASE_NOTES_URL: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/releases/tag/v",
    env!("CARGO_PKG_VERSION")
);
pub const MSAA: u32 = 4;
pub const DEPTH_BITS: u8 = 24;
/// Viewport tessellation: max chord deviation in mm.
const DISPLAY_TOL: f64 = 0.05;
const EDGE_COLOR: [f32; 3] = [0.08, 0.08, 0.1];
const PLANE_COLOR: [f32; 3] = [0.25, 0.45, 0.85];
pub const VIEWPORT_BG: egui::Color32 = egui::Color32::from_rgb(214, 220, 230);

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

/// The three default planes (Z up).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefPlane {
    Front,
    Top,
    Right,
}

impl RefPlane {
    pub const ALL: [RefPlane; 3] = [RefPlane::Front, RefPlane::Top, RefPlane::Right];

    pub fn name(self) -> &'static str {
        match self {
            RefPlane::Front => "Front Plane",
            RefPlane::Top => "Top Plane",
            RefPlane::Right => "Right Plane",
        }
    }

    pub fn plane(self) -> Plane {
        match self {
            RefPlane::Front => Plane::FRONT,
            RefPlane::Top => Plane::TOP,
            RefPlane::Right => Plane::RIGHT,
        }
    }
}

/// Something picked in the 3D model (outside sketch mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelRef {
    Face { body: usize, face: usize },
    Edge { body: usize, edge: usize },
    Vertex { body: usize, vertex: usize },
    Plane(RefPlane),
}

impl ModelRef {
    pub fn describe(self) -> String {
        match self {
            ModelRef::Face { body, face } => format!("Face {} of body {}", face + 1, body + 1),
            ModelRef::Edge { body, edge } => format!("Edge {} of body {}", edge + 1, body + 1),
            ModelRef::Vertex { body, vertex } => format!("Vertex {} of body {}", vertex + 1, body + 1),
            ModelRef::Plane(p) => p.name().to_string(),
        }
    }
}

pub struct App {
    pub doc: PartDocument,
    pub camera: OrbitCamera,
    /// Feature selected in the FeatureManager.
    pub selected: Option<FeatureId>,
    pub tab: RibbonTab,
    pub status: String,
    /// Export tessellation: max chord deviation in mm.
    pub export_tol: f64,
    pub slicer: Slicer,
    pub geometry: Arc<SceneGeometry>,
    pub geometry_version: u64,
    /// Pick id -> model item.
    pub refs: Vec<ModelRef>,
    /// Planar faces, for sketching on them.
    pub face_planes: HashMap<ModelRef, Plane>,
    pub selection: Vec<ModelRef>,
    pub hover: Option<ModelRef>,
    pub plane_visible: HashMap<RefPlane, bool>,
    pub states: Arc<Vec<u32>>,
    pub states_version: u64,
    pub sketch: Option<SketchSession>,
    built_for: Option<(u64, Vec<bool>)>,
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
            geometry_version: 0,
            refs: Vec::new(),
            face_planes: HashMap::new(),
            selection: Vec::new(),
            hover: None,
            plane_visible: RefPlane::ALL.into_iter().map(|p| (p, true)).collect(),
            states: Arc::default(),
            states_version: 0,
            sketch: None,
            built_for: None,
        };
        app.sync_geometry();
        app.zoom_to_fit();
        Ok(app)
    }

    /// Rebuilds the scene (meshes, pick ids, planes) when the document or plane visibility changed.
    fn sync_geometry(&mut self) {
        let key = (
            self.doc.revision(),
            RefPlane::ALL.iter().map(|p| self.plane_visible[p]).collect::<Vec<_>>(),
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let mut g = SceneGeometry::default();
        g.add_grid(100.0, 5.0);
        let mut refs = Vec::new();
        let mut face_planes = HashMap::new();
        for (body, (_, solid)) in self.doc.bodies().iter().enumerate() {
            let m = mesher::render_mesh(solid, DISPLAY_TOL);
            let base = refs.len() as u32;
            for (face, plane) in m.faces.iter().enumerate() {
                let r = ModelRef::Face { body, face };
                refs.push(r);
                if let Some(p) = plane {
                    face_planes.insert(r, *p);
                }
            }
            g.triangles.extend(
                m.positions
                    .iter()
                    .zip(&m.normals)
                    .zip(&m.face_of)
                    .map(|((&pos, &nor), &f)| MeshVertex { pos, nor, id: base + f }),
            );
            for (edge, poly) in m.edges.iter().enumerate() {
                let id = refs.len() as u32;
                refs.push(ModelRef::Edge { body, edge });
                for w in poly.windows(2) {
                    g.line(w[0], w[1], EDGE_COLOR, id);
                }
            }
            for (vertex, &p) in m.vertices.iter().enumerate() {
                g.points.push((p, refs.len() as u32));
                refs.push(ModelRef::Vertex { body, vertex });
            }
        }
        // Planes sized to the model, like SolidWorks.
        let half = g
            .bounds()
            .map_or(60.0, |(lo, hi)| (lo.abs().max(hi.abs()).max_element() * 1.2).max(40.0)) as f64;
        for rp in RefPlane::ALL.into_iter().filter(|p| self.plane_visible[p]) {
            let p = rp.plane();
            let corners =
                [[-half, -half], [half, -half], [half, half], [-half, half]].map(|c| p.to_world(c).map(|v| v as f32));
            g.add_plane(corners, PLANE_COLOR, refs.len() as u32);
            refs.push(ModelRef::Plane(rp));
        }
        self.geometry = Arc::new(g);
        self.geometry_version += 1;
        self.refs = refs;
        self.face_planes = face_planes;
        // Face/edge indices may now mean something else; reference planes stay valid.
        self.selection.retain(|r| matches!(r, ModelRef::Plane(_)));
        self.hover = None;
        self.built_for = Some(key);
    }

    /// Recomputes the highlight states buffer from hover and selection; true when it changed.
    fn sync_states(&mut self) -> bool {
        let states: Vec<u32> = self
            .refs
            .iter()
            .map(|r| {
                if self.selection.contains(r) {
                    2
                } else if self.hover == Some(*r) {
                    1
                } else {
                    0
                }
            })
            .collect();
        let changed = *self.states != states;
        if changed {
            self.states = Arc::new(states);
            self.states_version += 1;
        }
        changed
    }

    pub fn zoom_to_fit(&mut self) {
        if let Some((min, max)) = self.geometry.bounds() {
            self.camera.fit(min, max);
        } else {
            self.camera.fit(Vec3::splat(-50.0), Vec3::splat(50.0));
        }
    }

    /// The plane a new sketch would go on: a selected reference plane or planar face.
    pub fn sketch_target(&self) -> Option<(Plane, String)> {
        self.selection.iter().find_map(|r| match r {
            ModelRef::Plane(p) => Some((p.plane(), p.name().to_string())),
            face @ ModelRef::Face { .. } => self.face_planes.get(face).map(|p| (*p, face.describe())),
            _ => None,
        })
    }

    /// Starts a new sketch on the selected plane/face, or edits the selected sketch feature.
    pub fn start_sketch(&mut self) {
        if let Some((plane, name)) = self.sketch_target() {
            self.sketch = Some(SketchSession::new(None, Default::default(), plane, name));
        } else if let Some(id) = self.selected {
            self.edit_sketch(id);
            return;
        } else {
            self.status = "Select a plane or a planar face to sketch on".into();
            return;
        }
        self.enter_sketch_view();
    }

    pub fn edit_sketch(&mut self, id: FeatureId) {
        let Some(Feature {
            name,
            kind: FeatureKind::Sketch { sketch, plane },
        }) = self.doc.history().get(id)
        else {
            self.status = "Select a plane, a planar face or a sketch".into();
            return;
        };
        self.sketch = Some(SketchSession::new(Some(id), sketch.clone(), *plane, name.clone()));
        self.enter_sketch_view();
    }

    fn enter_sketch_view(&mut self) {
        let Some(s) = &self.sketch else { return };
        self.camera.normal_to(
            Vec3::from(s.plane.normal().map(|v| v as f32)),
            Vec3::from(s.plane.x.map(|v| v as f32)),
        );
        self.tab = RibbonTab::Sketch;
        self.selection.clear();
        self.status = format!("Editing sketch on {}", s.plane_name);
    }

    /// Leaves sketch mode, keeping (OK) or discarding (Cancel) the edits as one undo step.
    pub fn exit_sketch(&mut self, keep: bool) {
        let Some(session) = self.sketch.take() else { return };
        self.tab = RibbonTab::Features;
        if !keep || !session.changed() {
            self.status = "Sketch closed".into();
            return;
        }
        let new_sketch = session.sketch;
        match session.feature {
            Some(id) => self.doc.edit(|h| {
                if let Some(Feature {
                    kind: FeatureKind::Sketch { sketch, .. },
                    ..
                }) = h.get_mut(id)
                {
                    *sketch = new_sketch;
                }
            }),
            None => self.selected = Some(self.doc.edit(|h| h.add_sketch(new_sketch, session.plane))),
        }
        self.status = "Sketch saved".into();
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

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (undo, redo, escape, delete) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Delete),
            )
        });
        let typing = ctx.wants_keyboard_input();
        if let Some(s) = &mut self.sketch {
            if undo {
                s.undo();
            }
            if redo {
                s.redo();
            }
            if escape && !typing {
                s.escape();
            }
            if delete && !typing {
                s.delete_selection();
            }
        } else {
            if undo {
                self.doc.undo();
            }
            if redo {
                self.doc.redo();
            }
            if escape && !typing {
                self.selection.clear();
            }
        }
        if !typing && ctx.input(|i| i.key_pressed(Key::F)) {
            self.zoom_to_fit();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.shortcuts(ctx);
        if self.selected.is_some_and(|id| self.doc.history().get(id).is_none()) {
            self.selected = None;
        }
        if self.camera.animate(ctx.input(|i| i.stable_dt).min(0.05)) {
            ctx.request_repaint();
        }

        ui::ribbon::show(self, ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} bodies  |  MMGS (mm)", self.doc.bodies().len()));
                    if let Some(s) = &self.sketch {
                        ui.separator();
                        let (text, color) = s.status_text();
                        ui.colored_label(color, text);
                    }
                });
            });
        });
        ui::tree_view::show(self, ctx);
        ui::properties::show(self, ctx);
        self.sync_geometry();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(VIEWPORT_BG))
            .show(ctx, |ui| ui::viewport::show(self, ui));
        // Hover is found while painting the viewport, so show its highlight on the next frame.
        if self.sync_states() {
            ctx.request_repaint();
        }
    }
}
