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
use threebcad_kernel::topology::EdgeCurve;
use threebcad_kernel::Plane;
use threebcad_render::{MeshVertex, OrbitCamera, SceneGeometry};
use threebcad_sketch::Constraint;

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
pub const VIEWPORT_BG: egui::Color32 = egui::Color32::from_rgb(214, 220, 230);

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
    /// g/cm³
    pub density: f64,
    pub mass: Option<(u64, MassProperties)>,
    /// Feature being renamed in the tree and the text typed so far.
    pub renaming: Option<(FeatureId, String)>,
    known_problems: usize,
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
            density: 1.24,
            mass: None,
            renaming: None,
            known_problems: 0,
            built_for: None,
        };
        app.sync_geometry();
        app.zoom_to_fit();
        Ok(app)
    }

    pub fn describe(&self, r: ModelRef) -> String {
        match r {
            ModelRef::Face { body, face } => format!("Face {} of body {}", face + 1, body + 1),
            ModelRef::Edge { body, edge } => format!("Edge {} of body {}", edge + 1, body + 1),
            ModelRef::Vertex { body, vertex } => format!("Vertex {} of body {}", vertex + 1, body + 1),
            ModelRef::Plane(p) => p.name().to_string(),
            ModelRef::Feature(id) => self.doc.history().get(id).map_or("?".into(), |f| f.name.clone()),
        }
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
        for (body, b) in self.doc.bodies().iter().enumerate() {
            let m = mesher::render_mesh(&b.solid, DISPLAY_TOL);
            let base = refs.len() as u32;
            refs.extend((0..m.faces.len()).map(|face| ModelRef::Face { body, face }));
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
                _ => continue,
            }
            refs.push(ModelRef::Feature(id));
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

    pub fn selected_faces(&self) -> Vec<FaceRef> {
        self.selection
            .iter()
            .filter_map(|r| match *r {
                ModelRef::Face { body, face } => self.doc.face_ref(body, face),
                _ => None,
            })
            .collect()
    }

    /// Selected edges, plus every edge of selected faces (like SolidWorks' Fillet).
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
            self.status = "Select a plane or a planar face to sketch on".into();
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
            self.status = "Select a plane, a planar face or a sketch".into();
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
        self.status = format!("Editing sketch on {}", s.plane_name);
    }

    /// Leaves sketch mode, keeping (OK) or discarding (Cancel) the edits as one undo step.
    pub fn exit_sketch(&mut self, keep: bool) {
        let Some(session) = self.sketch.take() else { return };
        self.tab = RibbonTab::Features;
        let on = self.sketch_on.take();
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
            None => {
                let plane = session.plane;
                let id = self.doc.edit(|h| match on {
                    Some(on) => h.add_sketch_on(new_sketch.clone(), plane, on),
                    None => Ok(h.add_sketch(new_sketch.clone(), plane)),
                });
                match id {
                    Ok(id) => self.selected = Some(id),
                    Err(e) => self.status = e.to_string(),
                }
            }
        }
        self.status = "Sketch saved".into();
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
            self.status = "Convert Entities: sketch on a model face first".into();
            return;
        };
        let Some(b) = self.doc.bodies().get(body) else { return };
        let edges: Vec<_> = b
            .topo
            .edges
            .iter()
            .filter(|e| e.sides.iter().any(|s| s.face == face))
            .cloned()
            .collect();
        let Some(s) = &mut self.sketch else { return };
        let plane = s.plane;
        s.checkpoint();
        let sk = &mut s.sketch;
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
        for e in &edges {
            match e.curve {
                EdgeCurve::Line { a, b } => {
                    let (pa, pb) = (point(sk, a), point(sk, b));
                    sk.add_line(pa, pb);
                }
                EdgeCurve::Circle { center, radius, .. } => {
                    let same = |c: [f64; 3], r: f64| {
                        (r - radius).abs() < 1e-6 && (0..3).all(|i| (c[i] - center[i]).abs() < 1e-6)
                    };
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
        s.refresh();
        self.status = format!("Converted {added} edges of the face");
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

    pub fn write(&self, format: ExportFormat, path: &Path) -> io::Result<()> {
        let bodies = self.doc.bodies();
        if bodies.is_empty() {
            return Err(io::Error::other("the part has no bodies"));
        }
        let meshes = || -> Vec<PrintMesh> {
            bodies
                .iter()
                .map(|b| mesher::print_mesh(&b.solid, self.export_tol))
                .collect()
        };
        match format {
            ExportFormat::Stl => threebcad_io::write_stl(path, &meshes()),
            ExportFormat::ThreeMf => threebcad_io::write_3mf(path, &meshes()),
            ExportFormat::Step => threebcad_io::write_step(path, bodies.iter().map(|b| &b.solid)),
        }
    }

    /// Mass properties of all bodies, cached per rebuild.
    pub fn mass_properties(&mut self) -> Option<&MassProperties> {
        if self.doc.bodies().is_empty() {
            return None;
        }
        if self.mass.as_ref().is_none_or(|(rev, _)| *rev != self.doc.revision()) {
            let meshes: Vec<_> = self
                .doc
                .bodies()
                .iter()
                .map(|b| mesher::print_mesh(&b.solid, MASS_TOL))
                .collect();
            self.mass = Some((self.doc.revision(), mass::mass_properties(&meshes)));
        }
        self.mass.as_ref().map(|(_, m)| m)
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (undo, redo, escape, delete, rebuild, rename) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Delete),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Q))
                    || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::B)),
                i.key_pressed(Key::F2),
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
                self.renaming = None;
            }
            if rebuild {
                self.doc.force_rebuild();
                self.status = format!("Rebuilt {} features", self.doc.built().evaluated);
            }
            if rename && !typing {
                if let Some(id) = self.selected {
                    let name = self.doc.history().get(id).map(|f| f.name.clone()).unwrap_or_default();
                    self.renaming = Some((id, name));
                }
            }
        }
        if !typing && ctx.input(|i| i.key_pressed(Key::F)) {
            self.zoom_to_fit();
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.show_shortcuts = !self.show_shortcuts;
        }
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
                            .on_hover_text("What's Wrong")
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
        ui::help::window(self, ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(VIEWPORT_BG))
            .show(ctx, |ui| ui::viewport::show(self, ui));
        // Hover is found while painting the viewport, so show its highlight on the next frame.
        if self.sync_states() {
            ctx.request_repaint();
        }
    }
}
