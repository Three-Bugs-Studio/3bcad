use crate::i18n::{tr, trf};
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use glam::Vec3;
use threebcad_core::rebuild::stable_plane;
use threebcad_core::{
    AxisRef, EdgeRef, FaceRef, Feature, FeatureId, FeatureKind, PartDocument, PlaneRef, PointRef, VertexRef,
};
use threebcad_io::Slicer;
use threebcad_kernel::mass::{self, MassProperties};
use threebcad_kernel::mesher::{self, PrintMesh};
use threebcad_kernel::topology::{EdgeCurve, TopoEdge, Topology};
use threebcad_kernel::Plane;
use threebcad_render::{MeshVertex, OrbitCamera, SceneGeometry};
use threebcad_sketch::{Constraint, Sketch};

use crate::print::{Objects, PrintReport, PrintView};
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
/// Mass properties tessellation (mm), shown in the window.
pub const MASS_TOL: f64 = 0.005;
const EDGE_COLOR: [f32; 3] = [0.08, 0.08, 0.1];
const PLANE_COLOR: [f32; 3] = [0.25, 0.45, 0.85];
const REF_COLOR: [f32; 3] = [0.85, 0.5, 0.1];
/// Helices and other 3D curves, drawn like sketch geometry.
const SKETCH_CURVE_COLOR: [f32; 3] = [0.1, 0.35, 0.8];

pub use threebcad_core::StdPlane as RefPlane;

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

/// Something picked in the 3D model (outside sketch mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelRef {
    Face {
        body: usize,
        face: usize,
    },
    Edge {
        body: usize,
        edge: usize,
    },
    Vertex {
        body: usize,
        vertex: usize,
    },
    Plane(RefPlane),
    /// A reference plane, axis, point or coordinate system feature.
    Feature(FeatureId),
}

pub struct App {
    /// The part being edited, or (while an assembly is open) the last part, kept out of sight.
    pub doc: PartDocument,
    /// The open assembly; parts are edited in their own files.
    pub asm: Option<crate::assembly::Session>,
    /// The open drawing; its part is read from its own file.
    pub drw: Option<crate::drawing::Session>,
    pub camera: OrbitCamera,
    /// Feature selected in the feature tree.
    pub selected: Option<FeatureId>,
    pub tab: RibbonTab,
    pub status: String,
    /// The 3D Print tab's overlays, and what its checks found.
    pub show_overhangs: bool,
    pub show_thin: bool,
    pub print: PrintReport,
    /// A slicer Send to Slicer could not find, waiting for the user to locate it.
    pub slicer_missing: Option<Slicer>,
    /// Export meshes, before they are placed on the bed, and what they were made for.
    export_cache: Option<(ExportKey, Arc<Objects>)>,
    pub geometry: Arc<SceneGeometry>,
    pub geometry_version: u64,
    /// Pick id -> model item.
    pub refs: Vec<ModelRef>,
    pub selection: Vec<ModelRef>,
    pub hover: Option<ModelRef>,
    pub plane_visible: HashMap<RefPlane, bool>,
    pub states: Arc<Vec<u32>>,
    pub states_version: u64,
    pub sketch: Option<SketchSession>,
    /// What a new (not yet saved) sketch should follow.
    pub sketch_on: Option<PlaneRef>,
    pub show_whats_wrong: bool,
    pub show_measure: bool,
    pub show_mass: bool,
    pub show_shortcuts: bool,
    pub show_about: bool,
    /// The crash report the last run left, until the user closes the notice.
    pub crashed: Option<std::path::PathBuf>,
    /// File > Settings, while open.
    pub settings_window: Option<crate::ui::settings::SettingsWindow>,
    /// The theme, interface size and language last handed to egui.
    pub settings_applied: Option<(crate::ui::theme::Theme, f32, crate::i18n::Language)>,
    /// Display style, section view and the viewport's other state.
    pub view: crate::ui::viewport::ViewState,
    /// The Equations window (and configurations), while open.
    pub equations: Option<crate::ui::design::Equations>,
    /// The Appearance window, while open.
    pub appearance: Option<crate::ui::design::Appearance>,
    /// The configuration exports are made in (`None`: the active one).
    pub export_config: Option<usize>,
    pub mass: Option<(u64, MassProperties)>,
    /// The Measure window's lines, and the model revision and selection they are for.
    pub measured: Option<(u64, Vec<ModelRef>, Vec<String>)>,
    /// Feature being renamed in the tree and the text typed so far.
    pub renaming: Option<(FeatureId, String)>,
    pub files: crate::files::Files,
    /// Checking for, and installing, a newer release.
    pub update: crate::update::Updater,
    known_problems: usize,
    built_for: Option<(u64, Vec<bool>, Option<PrintView>)>,
    /// The bodies' display meshes and the part revision they are for.
    body_meshes: Option<(u64, Arc<Vec<mesher::RenderMesh>>)>,
    /// Meshes being made on a worker thread (large parts take seconds; the UI keeps running).
    meshing: Option<Meshing>,
    /// Zoom to fit once the meshes being made arrive.
    fit_when_meshed: bool,
    egui_ctx: egui::Context,
}

/// Display meshes made on a worker thread for one part revision.
struct Meshing {
    revision: u64,
    result: std::sync::mpsc::Receiver<Vec<mesher::RenderMesh>>,
    /// Bodies done, and how many there are.
    done: Arc<std::sync::atomic::AtomicUsize>,
    bodies: usize,
    started: std::time::Instant,
}

/// Parts with more triangles than this draw a coarse copy while the view moves.
const LOD_TRIANGLES: usize = 300_000;

/// What export meshes are made for: the part's name (the objects carry it), its revision, and the chord and angle tolerance.
type ExportKey = (String, u64, [f64; 2]);

impl App {
    /// `crashed`: the crash report the last run left, to offer it to the user.
    pub fn new(cc: &eframe::CreationContext<'_>, crashed: Option<std::path::PathBuf>) -> Result<Self, &'static str> {
        let rs = cc.wgpu_render_state.as_ref().ok_or("3BCAD needs the wgpu renderer")?;
        threebcad_render::install(rs, eframe::egui_wgpu::depth_format_from_bits(DEPTH_BITS, 0), MSAA);
        // The theme, size and language come from the settings on the first frame.
        let mut app = Self {
            doc: PartDocument::demo(),
            asm: None,
            drw: None,
            camera: OrbitCamera::default(),
            selected: None,
            tab: RibbonTab::Features,
            status: "Ready".into(),
            show_overhangs: false,
            show_thin: false,
            print: PrintReport::default(),
            slicer_missing: None,
            export_cache: None,
            geometry: Arc::default(),
            geometry_version: 0,
            refs: Vec::new(),
            selection: Vec::new(),
            hover: None,
            plane_visible: RefPlane::ALL.into_iter().map(|p| (p, true)).collect(),
            states: Arc::default(),
            states_version: 0,
            sketch: None,
            sketch_on: None,
            show_whats_wrong: false,
            show_measure: false,
            show_mass: false,
            show_shortcuts: false,
            show_about: false,
            settings_window: None,
            crashed,
            settings_applied: None,
            view: Default::default(),
            equations: None,
            appearance: None,
            export_config: None,
            mass: None,
            measured: None,
            renaming: None,
            files: crate::files::Files::new(),
            update: crate::update::Updater::new(),
            known_problems: 0,
            built_for: None,
            body_meshes: None,
            meshing: None,
            fit_when_meshed: false,
            egui_ctx: cc.egui_ctx.clone(),
        };
        app.camera.ortho = !app.files.settings.perspective;
        app.view.style = app.files.settings.display_style;
        app.sync_geometry();
        app.zoom_to_fit();
        // `3bcad <file>`: what a file association or "Open with" passes.
        if let Some(path) = std::env::args_os()
            .skip(1)
            .find(|a| !a.to_string_lossy().starts_with('-'))
        {
            app.request(crate::files::Pending::Open(path.into()));
        }
        crate::update::cleanup();
        if app.files.settings.check_updates {
            app.update.check(false);
        }
        Ok(app)
    }

    pub fn describe(&self, r: ModelRef) -> String {
        if let Some(s) = &self.asm {
            return s.describe(r);
        }
        match r {
            ModelRef::Face { body, face } => trf("Face {} of body {}", &[&(face + 1), &(body + 1)]),
            ModelRef::Edge { body, edge } => trf("Edge {} of body {}", &[&(edge + 1), &(body + 1)]),
            ModelRef::Vertex { body, vertex } => trf("Vertex {} of body {}", &[&(vertex + 1), &(body + 1)]),
            ModelRef::Plane(p) => p.name().to_string(),
            ModelRef::Feature(id) => self.doc.history().get(id).map_or("?".into(), |f| f.name.clone()),
        }
    }

    /// The 3D Print tab's view of the part, while that tab is open (and no sketch is).
    pub fn print_view(&self) -> Option<PrintView> {
        let s = &self.files.settings;
        (self.tab == RibbonTab::Print && self.sketch.is_none()).then(|| PrintView {
            down: self.doc.print_down(),
            bed: s.bed,
            overhang: self.show_overhangs.then(|| s.overhang_deg.to_radians()),
            thin: self.show_thin.then_some((s.nozzle, s.min_wall)),
        })
    }

    /// Rebuilds the scene (meshes, pick ids, planes) when the document, plane visibility or the
    /// print view changed.
    pub(crate) fn sync_geometry(&mut self) {
        if self.asm.is_some() {
            return self.sync_assembly();
        }
        let view = self.print_view();
        let key = (
            self.doc.revision(),
            RefPlane::ALL.iter().map(|p| self.plane_visible[p]).collect::<Vec<_>>(),
            view,
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let Some(meshes) = self.body_meshes() else {
            // Still meshing: the last geometry stays until the new one is ready.
            return;
        };
        let refit = self.built_for.as_ref().is_some_and(|k| k.2.is_some() != view.is_some());
        let mut g = SceneGeometry::default();
        if view.is_none() {
            g.add_grid(100.0, 5.0);
        }
        let mut refs = Vec::new();
        for (body, m) in meshes.iter().enumerate() {
            let colors = self.doc.face_colors(body);
            let base = refs.len() as u32;
            refs.extend((0..m.faces.len()).map(|face| ModelRef::Face { body, face }));
            g.triangles.extend(
                m.positions
                    .iter()
                    .zip(&m.normals)
                    .zip(&m.face_of)
                    .map(|((&pos, &nor), &f)| MeshVertex {
                        pos,
                        nor,
                        id: base + f,
                        tint: match colors.get(f as usize) {
                            Some(&Some([r, g, b])) => [r, g, b, 255],
                            _ => [0; 4],
                        },
                    }),
            );
            for (edge, poly) in m.edges.iter().enumerate() {
                let id = refs.len() as u32;
                refs.push(ModelRef::Edge { body, edge });
                for w in poly.windows(2) {
                    g.edge(w[0], w[1], EDGE_COLOR, id);
                }
            }
            for (vertex, &p) in m.vertices.iter().enumerate() {
                g.points.push((p, refs.len() as u32));
                refs.push(ModelRef::Vertex { body, vertex });
            }
        }
        for (id, mesh) in &self.doc.built().meshes {
            if self.doc.history().is_suppressed(*id) {
                continue;
            }
            let pick = refs.len() as u32;
            refs.push(ModelRef::Feature(*id));
            for t in &mesh.triangles {
                let [a, b, c] = t.map(|i| mesh.vertices[i]);
                let n = threebcad_kernel::topology::cross(
                    threebcad_kernel::topology::sub(b, a),
                    threebcad_kernel::topology::sub(c, a),
                );
                let nor = threebcad_kernel::topology::unit(n).map(|v| v as f32);
                g.triangles.extend([a, b, c].map(|p| MeshVertex {
                    pos: p.map(|v| v as f32),
                    nor,
                    id: pick,
                    tint: [0; 4],
                }));
            }
        }
        if self.files.settings.lod {
            g.build_lod(LOD_TRIANGLES);
        }
        match view {
            // On the bed the part moves, so the model-space planes and references stay hidden.
            Some(v) => self.print = crate::print::lay_out(&mut g, &v),
            None => self.add_references(&mut g, &mut refs),
        }
        self.geometry = Arc::new(g);
        self.geometry_version += 1;
        self.refs = refs;
        // Face/edge indices may now mean something else; planes and features stay valid.
        self.selection
            .retain(|r| matches!(r, ModelRef::Plane(_) | ModelRef::Feature(_)));
        self.hover = None;
        self.built_for = Some(key);
        self.mass = None;
        // Open What's Wrong when a rebuild brings new problems.
        let problems = self.doc.errors().len();
        if problems > self.known_problems {
            self.show_whats_wrong = true;
        }
        self.known_problems = problems;
        if refit || std::mem::take(&mut self.fit_when_meshed) {
            self.zoom_to_fit();
        }
    }

    /// The bodies' display meshes for this revision, or `None` while a worker thread makes them.
    fn body_meshes(&mut self) -> Option<Arc<Vec<mesher::RenderMesh>>> {
        let revision = self.doc.revision();
        if let Some((r, m)) = &self.body_meshes {
            if *r == revision {
                return Some(m.clone());
            }
        }
        if self.meshing.as_ref().is_none_or(|m| m.revision != revision) {
            let solids: Vec<_> = self.doc.bodies().iter().map(|b| b.solid.clone()).collect();
            let (tx, result) = std::sync::mpsc::channel();
            let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let (count, ctx) = (done.clone(), self.egui_ctx.clone());
            let bodies = solids.len();
            std::thread::spawn(move || {
                let meshes = solids
                    .iter()
                    .map(|s| {
                        let m = mesher::render_mesh(s, DISPLAY_TOL);
                        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        m
                    })
                    .collect();
                // The UI may have moved on to another revision: then nobody listens.
                let _ = tx.send(meshes);
                ctx.request_repaint();
            });
            self.meshing = Some(Meshing {
                revision,
                result,
                done,
                bodies,
                started: std::time::Instant::now(),
            });
        }
        let m = self.meshing.as_ref()?;
        match m.result.try_recv() {
            Ok(meshes) => {
                let meshes = Arc::new(meshes);
                self.body_meshes = Some((revision, meshes.clone()));
                self.meshing = None;
                Some(meshes)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            // The worker panicked (a face truck cannot mesh): show the part without it.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                log::error!("meshing revision {revision} failed");
                self.meshing = None;
                self.body_meshes = Some((revision, Arc::default()));
                self.status = crate::i18n::tr("The part could not be meshed for display").into();
                self.body_meshes.as_ref().map(|(_, m)| m.clone())
            }
        }
    }

    /// "Meshing 3 / 12 bodies", when a worker has been at it for a moment.
    pub fn meshing_progress(&self) -> Option<String> {
        let m = self.meshing.as_ref()?;
        (m.started.elapsed().as_millis() > 300).then(|| {
            let done = m.done.load(std::sync::atomic::Ordering::Relaxed);
            crate::i18n::trf("Meshing {} / {} bodies…", &[&done, &m.bodies])
        })
    }

    /// Reference planes and the reference geometry features above the rollback bar.
    fn add_references(&self, g: &mut SceneGeometry, refs: &mut Vec<ModelRef>) {
        // Planes sized to the model.
        let half = g
            .bounds()
            .map_or(60.0, |(lo, hi)| (lo.abs().max(hi.abs()).max_element() * 1.2).max(40.0)) as f64;
        let f32x3 = |p: [f64; 3]| p.map(|v| v as f32);
        for rp in RefPlane::ALL.into_iter().filter(|p| self.plane_visible[p]) {
            let p = rp.plane();
            let corners = [[-half, -half], [half, -half], [half, half], [-half, half]].map(|c| f32x3(p.to_world(c)));
            g.add_plane(corners, PLANE_COLOR, refs.len() as u32);
            refs.push(ModelRef::Plane(rp));
        }
        // Reference geometry features above the rollback bar.
        let built = self.doc.built();
        for (id, f) in self.doc.history().iter() {
            if self.doc.history().is_suppressed(id) {
                continue;
            }
            let pick = refs.len() as u32;
            match f.kind {
                FeatureKind::RefPlane(_) => {
                    let Some(p) = built.planes.get(&id) else { continue };
                    let h = half * 0.6;
                    let corners = [[-h, -h], [h, -h], [h, h], [-h, h]].map(|c| f32x3(p.to_world(c)));
                    g.add_plane(corners, REF_COLOR, pick);
                }
                FeatureKind::RefAxis(_) => {
                    let Some(&(o, d)) = built.axes.get(&id) else { continue };
                    let at = |t: f64| f32x3([o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t]);
                    g.line(at(-half), at(half), REF_COLOR, pick);
                }
                FeatureKind::RefPoint(_) => {
                    let Some(&p) = built.points.get(&id) else { continue };
                    g.points.push((f32x3(p), pick));
                }
                FeatureKind::CoordSys(_) => {
                    let Some(p) = built.planes.get(&id) else { continue };
                    let n = p.normal();
                    let s = half * 0.25;
                    for (axis, color) in [(p.x, [0.85, 0.2, 0.2]), (p.y, [0.2, 0.7, 0.25]), (n, [0.2, 0.35, 0.9])] {
                        let tip = [0, 1, 2].map(|i| p.origin[i] + axis[i] * s);
                        g.line(f32x3(p.origin), f32x3(tip), color, pick);
                    }
                }
                FeatureKind::Helix(_) => {
                    let Some(pts) = built.curves.get(&id) else { continue };
                    for w in pts.windows(2) {
                        g.line(f32x3(w[0]), f32x3(w[1]), SKETCH_CURVE_COLOR, pick);
                    }
                }
                _ => continue,
            }
            refs.push(ModelRef::Feature(id));
        }
    }

    /// Recomputes the highlight states buffer from hover and selection; true when it changed.
    pub(crate) fn sync_states(&mut self) -> bool {
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
        if self.meshing.is_some() {
            self.fit_when_meshed = true;
        }
        if let Some((min, max)) = self.geometry.bounds() {
            self.camera.fit(min, max);
        } else {
            self.camera.fit(Vec3::splat(-50.0), Vec3::splat(50.0));
        }
    }

    /// The plane a selection item stands for, and how a sketch on it should follow it.
    pub fn plane_ref_of(&self, r: ModelRef) -> Option<(Plane, PlaneRef)> {
        match r {
            ModelRef::Plane(p) => Some((p.plane(), PlaneRef::Std(p))),
            ModelRef::Face { body, face } => {
                let p = self.doc.bodies().get(body)?.topo.faces.get(face)?.plane?;
                Some((stable_plane(&p), PlaneRef::Face(self.doc.face_ref(body, face)?)))
            }
            ModelRef::Feature(id) => match self.doc.history().get(id)?.kind {
                FeatureKind::RefPlane(_) | FeatureKind::CoordSys(_) => {
                    Some((self.doc.plane_of(id)?, PlaneRef::Feature(id)))
                }
                _ => None,
            },
            _ => None,
        }
    }

    pub fn axis_ref_of(&self, r: ModelRef) -> Option<AxisRef> {
        match r {
            ModelRef::Edge { body, edge } => Some(AxisRef::Edge(self.doc.edge_ref(body, edge)?)),
            ModelRef::Face { body, face } => Some(AxisRef::Face(self.doc.face_ref(body, face)?)),
            ModelRef::Feature(id) if self.doc.axis_of(id).is_some() => Some(AxisRef::Feature(id)),
            _ => None,
        }
    }

    pub fn point_ref_of(&self, r: ModelRef) -> Option<PointRef> {
        match r {
            ModelRef::Vertex { body, vertex } => Some(PointRef::Vertex(self.doc.vertex_ref(body, vertex)?)),
            ModelRef::Feature(id) if self.doc.point_of(id).is_some() => Some(PointRef::Feature(id)),
            _ => None,
        }
    }

    /// The feature selected in the feature tree, then those added with Ctrl+click, in order.
    pub fn chosen_features(&self) -> Vec<FeatureId> {
        let mut out: Vec<FeatureId> = self.selected.into_iter().collect();
        for r in &self.selection {
            if let ModelRef::Feature(id) = *r {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }

    pub fn selected_faces(&self) -> Vec<FaceRef> {
        self.selection
            .iter()
            .filter_map(|r| match *r {
                ModelRef::Face { body, face } => self.doc.face_ref(body, face),
                _ => None,
            })
            .collect()
    }

    /// Selected edges, plus every edge of selected faces (what Fillet expects).
    pub fn selected_edges(&self, with_faces: bool) -> Vec<EdgeRef> {
        let mut out = Vec::new();
        for r in &self.selection {
            match *r {
                ModelRef::Edge { body, edge } => out.extend(self.doc.edge_ref(body, edge)),
                ModelRef::Face { body, face } if with_faces => {
                    let Some(b) = self.doc.bodies().get(body) else { continue };
                    for (i, e) in b.topo.edges.iter().enumerate() {
                        if e.sides.iter().any(|s| s.face == face) {
                            out.extend(self.doc.edge_ref(body, i));
                        }
                    }
                }
                _ => {}
            }
        }
        out.dedup();
        out
    }

    pub fn selected_vertices(&self) -> Vec<VertexRef> {
        self.selection
            .iter()
            .filter_map(|r| match *r {
                ModelRef::Vertex { body, vertex } => self.doc.vertex_ref(body, vertex),
                _ => None,
            })
            .collect()
    }

    /// The plane a new sketch would go on: a selected plane, planar face or reference plane.
    pub fn sketch_target(&self) -> Option<(Plane, String, PlaneRef)> {
        self.selection
            .iter()
            .find_map(|&r| self.plane_ref_of(r).map(|(p, pr)| (p, self.describe(r), pr)))
    }

    /// Starts a new sketch on the selected plane/face, or edits the selected sketch feature.
    pub fn start_sketch(&mut self) {
        if let Some((plane, name, on)) = self.sketch_target() {
            self.sketch_on = (!matches!(on, PlaneRef::Std(_))).then_some(on);
            self.sketch = Some(SketchSession::new(None, Default::default(), plane, name));
        } else if let Some(id) = self.selected {
            self.edit_sketch(id);
            return;
        } else {
            self.status = tr("Select a plane or a planar face to sketch on").into();
            return;
        }
        self.enter_sketch_view();
    }

    pub fn edit_sketch(&mut self, id: FeatureId) {
        let Some(Feature {
            name,
            kind: FeatureKind::Sketch { sketch, plane, .. },
            ..
        }) = self.doc.history().get(id)
        else {
            self.status = tr("Select a plane, a planar face or a sketch").into();
            return;
        };
        self.sketch = Some(SketchSession::new(Some(id), sketch.clone(), *plane, name.clone()));
        self.sketch_on = None;
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
        self.status = trf("Editing sketch on {}", &[&s.plane_name]);
    }

    /// Leaves sketch mode, keeping (OK) or discarding (Cancel) the edits as one undo step.
    pub fn exit_sketch(&mut self, keep: bool) {
        let Some(session) = self.sketch.take() else { return };
        self.tab = RibbonTab::Features;
        let on = self.sketch_on.take();
        if !keep || !session.changed() {
            self.status = tr("Sketch closed").into();
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
            None => {
                let plane = session.plane;
                let (id, refused) = self.doc.edit(|h| match on {
                    Some(on) => match h.add_sketch_on(new_sketch.clone(), plane, on) {
                        Ok(id) => (id, None),
                        // The face is gone or rolled back: keep the drawing, on the face's plane.
                        Err(e) => (h.add_sketch(new_sketch, plane), Some(e)),
                    },
                    None => (h.add_sketch(new_sketch, plane), None),
                });
                self.selected = Some(id);
                if let Some(e) = refused {
                    self.status = trf("Sketch saved, but not linked to its face: {}", &[&e]);
                    return;
                }
            }
        }
        self.status = tr("Sketch saved").into();
    }

    /// The face the open sketch lies on, if any.
    fn sketch_face(&self) -> Option<(usize, usize)> {
        let on = match self.sketch.as_ref()?.feature {
            Some(id) => match &self.doc.history().get(id)?.kind {
                FeatureKind::Sketch { on, .. } => on.clone(),
                _ => None,
            },
            None => self.sketch_on.clone(),
        };
        match on? {
            PlaneRef::Face(f) => self.doc.resolve_face(&f),
            _ => None,
        }
    }

    /// Convert Entities: copies the edges of the sketch's face (or the selected model edges)
    /// into the sketch as fixed geometry.
    pub fn convert_entities(&mut self) {
        let Some((body, face)) = self.sketch_face() else {
            self.status = tr("Convert Entities: sketch on a model face first").into();
            return;
        };
        let Some(b) = self.doc.bodies().get(body) else { return };
        let edges = face_edges(&b.topo, face);
        let Some(s) = &mut self.sketch else { return };
        s.checkpoint();
        let added = add_edges(&mut s.sketch, &edges, s.plane);
        s.refresh();
        self.status = trf("Converted {} edges of the face", &[&added]);
    }

    /// A planar face's outline as a sketch on the face's plane.
    pub fn face_sketch(&self, body: usize, face: usize) -> Option<Sketch> {
        let topo = &self.doc.bodies().get(body)?.topo;
        let plane = topo.faces.get(face)?.plane?;
        let mut sk = Sketch::default();
        add_edges(&mut sk, &face_edges(topo, face), plane);
        Some(sk)
    }

    pub fn export(&mut self, format: ExportFormat) {
        let ext = format.ext();
        let Some(path) = rfd::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(format!("{}.{ext}", self.doc_name()))
            .save_file()
        else {
            return;
        };
        self.status = match self.write(format, &path) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => trf("Export failed: {}", &[&e]),
        };
    }

    pub fn send_to_slicer(&mut self) {
        let path = std::env::temp_dir().join(format!("{}.3mf", self.doc_name()));
        let slicer = self.files.settings.slicer;
        let located = self.files.settings.slicer_paths[slicer as usize].clone();
        let name = slicer.name();
        self.status = match self.write(ExportFormat::ThreeMf, &path) {
            Err(e) => trf("Could not write {}: {}", &[&path.display(), &e]),
            Ok(()) => match slicer.launch(&path, located.as_deref()) {
                Ok(()) => format!("Opened {} in {name}", path.display()),
                Err(e) => {
                    if e.kind() == io::ErrorKind::NotFound {
                        self.slicer_missing = Some(slicer);
                    }
                    trf("Could not open {}: {}", &[&name, &e])
                }
            },
        };
    }

    fn export_key(&self) -> ExportKey {
        let s = &self.files.settings;
        (self.doc.name.clone(), self.doc.revision(), [s.chord, s.angle_deg])
    }

    /// The bodies meshed for export (not yet placed on the bed), made again only when the part or
    /// the export quality changed.
    pub fn export_objects(&mut self) -> Arc<Objects> {
        let key = self.export_key();
        match &self.export_cache {
            Some((k, objects)) if *k == key => objects.clone(),
            _ => {
                let s = &self.files.settings;
                let objects = Arc::new(crate::print::export_meshes(&self.doc, s.chord, s.angle_deg));
                self.export_cache = Some((key, objects.clone()));
                objects
            }
        }
    }

    /// The export meshes' triangle count, if they are made for the current settings.
    pub fn export_triangles(&self) -> Option<usize> {
        let (k, objects) = self.export_cache.as_ref()?;
        (*k == self.export_key()).then(|| objects.iter().map(|(_, m)| m.triangles.len()).sum())
    }

    /// Drops everything cached for the open part: another part's revisions count from the start
    /// again, so they could match.
    pub(crate) fn forget_caches(&mut self) {
        self.equations = None;
        self.appearance = None;
        self.export_config = None;
        self.built_for = None;
        // Another document's revision numbers mean nothing for this one.
        self.body_meshes = None;
        self.meshing = None;
        self.export_cache = None;
        self.mass = None;
        self.measured = None;
    }

    /// STL and 3MF are written as the part sits on the printer's bed; STEP in model space. A part
    /// is written in its export configuration.
    pub fn write(&mut self, format: ExportFormat, path: &Path) -> io::Result<()> {
        if self.asm.is_some() {
            return self.write_assembly(format, path);
        }
        let h = self.doc.history();
        let active = h.active_configuration();
        let n = h.configurations().len();
        let Some(i) = self.export_config.filter(|&i| i != active && i < n) else {
            return self.write_part(format, path);
        };
        // A copy of the part built in that configuration stands in for the open one meanwhile.
        let other = self.doc.in_configuration(i).map_err(io::Error::other)?;
        let open = std::mem::replace(&mut self.doc, other);
        self.export_cache = None;
        let written = self.write_part(format, path);
        self.doc = open;
        self.export_cache = None;
        written
    }

    fn write_part(&mut self, format: ExportFormat, path: &Path) -> io::Result<()> {
        let bodies = self.doc.bodies();
        if bodies.is_empty() && (self.doc.built().meshes.is_empty() || format == ExportFormat::Step) {
            return Err(io::Error::other("the part has no bodies"));
        }
        let colors: Vec<Vec<Option<[u8; 3]>>> = (0..bodies.len()).map(|b| self.doc.face_colors(b)).collect();
        if format == ExportFormat::Step {
            let solids = bodies.iter().map(|b| &b.solid);
            return threebcad_io::write_step(path, &self.doc.name, solids, &colors.concat());
        }
        let (down, bed) = (self.doc.print_down(), self.files.settings.bed);
        let objects = crate::print::on_bed(&self.export_objects(), down, bed);
        let named: Vec<(&str, &PrintMesh)> = objects.iter().map(|(n, m)| (n.as_str(), m)).collect();
        if format == ExportFormat::Stl {
            return threebcad_io::write_stl(path, &named);
        }
        let s = &self.files.settings;
        let material = self.doc.material();
        let info = threebcad_io::threemf::Info {
            title: self.doc.name.clone(),
            material: material.name.clone(),
            color: material.color,
            colors,
            settings: vec![
                ("Printer".into(), s.printer.clone()),
                ("NozzleDiameter".into(), format!("{}", s.nozzle)),
                ("OverhangAngle".into(), format!("{}", s.overhang_deg)),
            ],
        };
        threebcad_io::write_3mf(path, &named, &info)
    }

    /// Mass properties of all bodies, cached per rebuild.
    pub fn mass_properties(&mut self) -> Option<&MassProperties> {
        if self.doc.bodies().is_empty() && self.doc.built().meshes.is_empty() {
            return None;
        }
        if self.mass.as_ref().is_none_or(|(rev, _)| *rev != self.doc.revision()) {
            let meshes: Vec<_> = self
                .doc
                .bodies()
                .iter()
                .map(|b| mesher::print_mesh(&b.solid, MASS_TOL))
                .chain(self.doc.built().meshes.iter().map(|(_, m)| (**m).clone()))
                .collect();
            self.mass = Some((self.doc.revision(), mass::mass_properties(&meshes)));
        }
        self.mass.as_ref().map(|(_, m)| m)
    }

    /// Hands the theme, interface size and language to egui when they change, and keeps the
    /// interface size egui's own Ctrl + plus / minus set.
    fn apply_settings(&mut self, ctx: &egui::Context) {
        let s = &mut self.files.settings;
        if self.settings_applied.is_some() && (ctx.zoom_factor() - s.ui_scale).abs() > 1e-3 {
            s.ui_scale = ctx.zoom_factor();
            self.files.settings_changed = true;
            self.settings_applied = Some((s.theme, s.ui_scale, s.language));
        }
        let want = (s.theme, s.ui_scale, s.language);
        if self.settings_applied != Some(want) {
            ui::theme::apply(ctx, s.theme);
            ctx.set_zoom_factor(s.ui_scale);
            if self.settings_applied.is_none_or(|(_, _, lang)| lang != s.language) {
                crate::i18n::set(s.language);
                crate::i18n::install_fonts(ctx, s.language);
            }
            self.settings_applied = Some(want);
        }
        ui::theme::track(ctx, s.theme);
    }

    /// The commands whose shortcuts were pressed this frame, and the fixed keys: Esc, Delete,
    /// Ctrl+Q (also a rebuild) and the arrow keys, which turn the view.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.settings_window.as_ref().is_some_and(|w| w.capture.is_some()) {
            return;
        }
        use crate::commands::Command;
        let typing = ctx.wants_keyboard_input();
        for c in pressed_commands(ctx, &self.files.settings.keys) {
            self.run(ctx, c);
        }
        let (escape, delete, rebuild) = ctx.input_mut(|i| {
            (
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Delete),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Q)),
            )
        });
        if rebuild {
            self.run(ctx, Command::Rebuild);
        }
        if self.drw.is_none() && !typing && ctx.memory(|m| m.focused().is_none()) {
            self.arrow_keys(ctx);
        }
        if self.asm.is_some() || self.drw.is_some() || typing {
            return;
        }
        match &mut self.sketch {
            Some(s) => {
                if escape {
                    s.escape();
                }
                if delete {
                    s.delete_selection();
                }
            }
            None => {
                if escape {
                    self.selection.clear();
                    self.renaming = None;
                    self.view.shortcut_bar = None;
                }
            }
        }
    }

    /// Arrows turn the view 15 degrees (Shift: 90), Ctrl+arrows pan, Page Up / Down zoom.
    fn arrow_keys(&mut self, ctx: &egui::Context) {
        let (step, pan) = ctx.input(|i| {
            let quarter = if i.modifiers.shift {
                std::f32::consts::FRAC_PI_2
            } else {
                15f32.to_radians()
            };
            (quarter, i.modifiers.command)
        });
        for (key, x, y) in [
            (Key::ArrowLeft, -1.0, 0.0),
            (Key::ArrowRight, 1.0, 0.0),
            (Key::ArrowUp, 0.0, -1.0),
            (Key::ArrowDown, 0.0, 1.0),
        ] {
            if ctx.input(|i| i.key_pressed(key)) {
                if pan {
                    self.camera.pan(x * 60.0, y * 60.0, 600.0);
                } else {
                    self.camera.turn(x * step, y * step);
                }
                ctx.request_repaint();
            }
        }
        for (key, scroll) in [(Key::PageUp, 240.0), (Key::PageDown, -240.0)] {
            if ctx.input(|i| i.key_pressed(key)) {
                self.camera.zoom(scroll);
                ctx.request_repaint();
            }
        }
    }

    /// Runs a command (a shortcut, the shortcut bar or a gesture).
    pub fn run(&mut self, ctx: &egui::Context, c: crate::commands::Command) {
        use crate::commands::Command;
        use threebcad_render::StandardView as V;
        let view = |app: &mut Self, v: V| {
            let (from, up) = v.direction();
            app.camera.look_from(from, up);
            ctx.request_repaint();
        };
        let in_3d = self.drw.is_none();
        match c {
            Command::New => self.request(crate::files::Pending::New),
            Command::Open => self.open_dialog(),
            Command::Save => {
                self.save();
            }
            Command::SaveAs => {
                self.save_as();
            }
            Command::Undo => match &mut self.sketch {
                Some(s) => s.undo(),
                None => {
                    self.undo();
                    self.status = crate::i18n::tr("Undone").into();
                }
            },
            Command::Redo => match &mut self.sketch {
                Some(s) => s.redo(),
                None => {
                    self.redo();
                    self.status = crate::i18n::tr("Redone").into();
                }
            },
            Command::Rebuild if self.asm.is_none() && self.drw.is_none() && self.sketch.is_none() => {
                self.doc.force_rebuild();
                self.status = crate::i18n::trf("Rebuilt {} features", &[&self.doc.built().evaluated]);
            }
            Command::Rename if self.asm.is_none() && self.drw.is_none() => {
                if let Some(id) = self.selected {
                    let name = self.doc.history().get(id).map(|f| f.name.clone()).unwrap_or_default();
                    self.renaming = Some((id, name));
                }
            }
            Command::ZoomToFit if in_3d => self.zoom_to_fit(),
            Command::Front if in_3d => view(self, V::Front),
            Command::Back if in_3d => view(self, V::Back),
            Command::Left if in_3d => view(self, V::Left),
            Command::Right if in_3d => view(self, V::Right),
            Command::Top if in_3d => view(self, V::Top),
            Command::Bottom if in_3d => view(self, V::Bottom),
            Command::Isometric if in_3d => view(self, V::Isometric),
            Command::NormalTo if in_3d => self.normal_to_selection(),
            Command::Perspective if in_3d => self.camera.ortho = !self.camera.ortho,
            Command::DisplayStyle if in_3d => {
                self.view.style = self.view.style.next();
                self.status = crate::i18n::tr(self.view.style.name()).into();
            }
            Command::SectionView if in_3d => self.toggle_section(),
            Command::ShortcutBar if in_3d => self.view.shortcut_bar = ctx.pointer_hover_pos(),
            Command::Settings => {
                if self.settings_window.take().is_none() {
                    self.settings_window = Some(Default::default());
                }
            }
            Command::ShortcutsWindow => self.show_shortcuts = !self.show_shortcuts,
            _ => {}
        }
    }

    /// Normal To: looks straight at the selected planar face or plane, else the sketch's plane.
    pub fn normal_to_selection(&mut self) {
        let plane = match &self.sketch {
            Some(s) => Some(s.plane),
            None => self
                .selection
                .iter()
                .find_map(|r| self.plane_ref_of(*r).map(|(p, _)| p)),
        };
        match plane {
            Some(p) => {
                let f = |v: [f64; 3]| Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32);
                self.camera.normal_to(f(p.normal()), f(p.x));
            }
            None => self.status = crate::i18n::tr("Select a planar face or a plane first").into(),
        }
    }

    pub fn toggle_section(&mut self) {
        self.view.section = match self.view.section {
            Some(_) => None,
            None => Some(crate::ui::viewport::Section::through(self.geometry.bounds())),
        };
    }

    /// Starts a sketch on a plane, a planar face or a reference plane, closing any open sketch first.
    pub fn sketch_on(&mut self, r: ModelRef) {
        if self.sketch.is_some() {
            self.exit_sketch(true);
        }
        self.selected = None;
        self.selection = vec![r];
        self.start_sketch();
    }
}

fn face_edges(topo: &Topology, face: usize) -> Vec<TopoEdge> {
    topo.edges
        .iter()
        .filter(|e| e.sides.iter().any(|s| s.face == face))
        .cloned()
        .collect()
}

/// Adds model edges to a sketch as fixed geometry; returns how many were added.
fn add_edges(sk: &mut Sketch, edges: &[TopoEdge], plane: Plane) -> usize {
    let mut points: Vec<([f64; 2], threebcad_sketch::PointId)> = Vec::new();
    let mut point = |sk: &mut threebcad_sketch::Sketch, p: [f64; 3]| {
        let l = plane.to_local(p);
        if let Some(&(_, id)) = points.iter().find(|(q, _)| (q[0] - l[0]).hypot(q[1] - l[1]) < 1e-6) {
            return id;
        }
        let id = sk.add_point(l[0], l[1]);
        sk.constrain(Constraint::Fix(id, l));
        points.push((l, id));
        id
    };
    let mut circles: Vec<([f64; 3], f64)> = Vec::new();
    let mut added = 0;
    for e in edges {
        match e.curve {
            EdgeCurve::Line { a, b } => {
                let (pa, pb) = (point(sk, a), point(sk, b));
                sk.add_line(pa, pb);
            }
            EdgeCurve::Circle { center, radius, .. } => {
                let same =
                    |c: [f64; 3], r: f64| (r - radius).abs() < 1e-6 && (0..3).all(|i| (c[i] - center[i]).abs() < 1e-6);
                // truck splits a circle into arcs: put the circle back together.
                let around: f64 = edges
                    .iter()
                    .filter(|o| matches!(o.curve, EdgeCurve::Circle { center: c, radius: r, .. } if same(c, r)))
                    .map(|o| o.length)
                    .sum();
                if around >= std::f64::consts::TAU * radius * (1.0 - 1e-3) {
                    if circles.iter().any(|&(c, r)| same(c, r)) {
                        continue;
                    }
                    circles.push((center, radius));
                    let c = point(sk, center);
                    let ent = sk.add_circle(c, radius);
                    sk.constrain(Constraint::Radius(ent, radius));
                } else {
                    let (first, last) = (e.points[0], *e.points.last().unwrap());
                    let mid = e.points[e.points.len() / 2];
                    let (lf, lm, ll) = (plane.to_local(first), plane.to_local(mid), plane.to_local(last));
                    // Sketch arcs run counter-clockwise from start to end.
                    let turn = (lm[0] - lf[0]) * (ll[1] - lf[1]) - (lm[1] - lf[1]) * (ll[0] - lf[0]);
                    let (s0, s1) = if turn > 0.0 { (first, last) } else { (last, first) };
                    let c = point(sk, center);
                    let (p0, p1) = (point(sk, s0), point(sk, s1));
                    sk.add_arc(c, p0, p1);
                }
            }
            EdgeCurve::Other => {
                let ids: Vec<_> = e
                    .points
                    .iter()
                    .step_by(4)
                    .chain(e.points.last())
                    .map(|&p| point(sk, p))
                    .collect();
                for w in ids.windows(2) {
                    if w[0] != w[1] {
                        sk.add_line(w[0], w[1]);
                    }
                }
            }
        }
        added += 1;
    }
    added
}

/// The commands whose shortcuts were pressed this frame. While a text field is being typed in,
/// its own keys stay its own: letters, and Ctrl+Z / Ctrl+Y (its undo).
fn pressed_commands(ctx: &egui::Context, overrides: &crate::commands::Overrides) -> Vec<crate::commands::Command> {
    use crate::commands::Command;
    let typing = ctx.wants_keyboard_input();
    let keys = crate::commands::in_order(overrides);
    ctx.input_mut(|i| {
        keys.iter()
            .filter(|(c, k)| !typing || (k.modifiers.command && !matches!(c, Command::Undo | Command::Redo)))
            .filter(|(_, k)| i.consume_shortcut(k))
            .map(|(c, _)| *c)
            .collect()
    })
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_settings(ctx);
        self.shortcuts(ctx);
        self.files_frame(ctx);
        crate::files::properties_window(self, ctx);
        ui::settings::window(self, ctx);
        ui::update::window(self, ctx);
        ui::help::about(self, ctx);
        ui::help::crash_notice(self, ctx);
        if self.asm.is_some() {
            return ui::assembly::frame(self, ctx);
        }
        if self.drw.is_some() {
            return ui::drawing::frame(self, ctx);
        }
        if self.selected.is_some_and(|id| self.doc.history().get(id).is_none()) {
            self.selected = None;
        }
        if self.camera.animate(ctx.input(|i| i.stable_dt).min(0.05)) {
            ctx.request_repaint();
        }

        // The panels are laid out before the viewport takes a click, so a selection made there shows
        // (and resizes the viewport for the Properties panel) on the next frame: ask for it now.
        let picked_before = (self.selected, self.selection.clone());
        ui::ribbon::show(self, ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match self.meshing_progress() {
                    Some(progress) => {
                        ui.spinner();
                        ui.label(progress);
                    }
                    None => {
                        ui.label(&self.status);
                    }
                }
                if self.meshing.is_some() {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let u = self.doc.units().system;
                    ui.menu_button(format!("{} ({})", u.short(), u.length_unit()), |ui| {
                        crate::files::units_ui(self, ui)
                    })
                    .response
                    .on_hover_text(tr("Unit system"));
                    ui.label(format!("{} bodies", self.doc.bodies().len()));
                    let (errs, warns) = (self.doc.errors().len(), self.doc.warnings().len());
                    if errs + warns > 0 {
                        ui.separator();
                        let text = format!("⚠ {errs} errors, {warns} warnings");
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(text).color(egui::Color32::from_rgb(190, 60, 30)),
                                )
                                .frame(false),
                            )
                            .on_hover_text(tr("What's Wrong"))
                            .clicked()
                        {
                            self.show_whats_wrong = true;
                        }
                    }
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
        ui::evaluate::windows(self, ctx);
        ui::design::equations_window(self, ctx);
        ui::design::appearance_window(self, ctx);
        ui::help::window(self, ctx);
        ui::print::slicer_prompt(self, ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::ui::theme::sky().1))
            .show(ctx, |ui| ui::viewport::show(self, ui));
        // Hover is found while painting the viewport, so show its highlight on the next frame.
        if self.sync_states() || picked_before != (self.selected, self.selection.clone()) {
            ctx.request_repaint();
        }
    }

    /// A normal exit (saved, or the user chose not to save) leaves nothing to recover.
    fn on_exit(&mut self) {
        self.files.clear_recovery();
        crate::logfile::closed_normally();
        // Restart into an update that was just installed.
        if let Some(exe) = self.files.restart.take() {
            if let Err(e) = std::process::Command::new(&exe).spawn() {
                log::error!("could not start {}: {e}", exe.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::commands::Command;

    fn frame(ctx: &egui::Context, focused: bool, text: &mut String, key: Key, modifiers: Modifiers) -> Vec<Command> {
        let press = egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let input = egui::RawInput {
            events: vec![press],
            ..Default::default()
        };
        let mut commands = Vec::new();
        let _ = ctx.run(input, |ctx| {
            commands = pressed_commands(ctx, &Default::default());
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui.text_edit_singleline(text);
                if focused {
                    r.request_focus();
                }
            });
        });
        commands
    }

    #[test]
    fn a_text_field_keeps_its_own_keys() {
        let ctx = egui::Context::default();
        let mut text = String::new();
        assert_eq!(
            frame(&ctx, false, &mut text, Key::Z, Modifiers::COMMAND),
            [Command::Undo]
        );
        assert_eq!(
            frame(&ctx, false, &mut text, Key::F, Modifiers::NONE),
            [Command::ZoomToFit]
        );
        frame(&ctx, true, &mut text, Key::A, Modifiers::NONE);
        // Typing: Ctrl+Z is the field's undo and F is a letter, but Ctrl+S still saves.
        assert!(frame(&ctx, true, &mut text, Key::Z, Modifiers::COMMAND).is_empty());
        assert!(frame(&ctx, true, &mut text, Key::F, Modifiers::NONE).is_empty());
        assert_eq!(
            frame(&ctx, true, &mut text, Key::S, Modifiers::COMMAND),
            [Command::Save]
        );
        // Ctrl+Shift+S is Save As, not Save.
        let both = Modifiers::COMMAND.plus(Modifiers::SHIFT);
        assert_eq!(frame(&ctx, false, &mut text, Key::S, both), [Command::SaveAs]);
    }
}
