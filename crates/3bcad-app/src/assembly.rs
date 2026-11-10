//! The open assembly: its scene, picking faces for mates, dragging components with the mouse,
//! interference results and the assembly exports.

use std::collections::{HashMap, HashSet};
use crate::i18n::tr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{Vec2, Vec3};
use threebcad_core::assembly::{self, MateFace};
use threebcad_core::AssemblyDocument;
use threebcad_kernel::mate::Pose;
use threebcad_kernel::mesher::{self, PrintMesh, RenderMesh};
use threebcad_kernel::print as checks;
use threebcad_kernel::topology::P3;
use threebcad_render::{pick, MeshVertex, SceneGeometry, NO_ID};

use crate::app::{App, ModelRef};
use crate::print::Objects;

/// Viewport tessellation (mm), as for parts.
const DISPLAY_TOL: f64 = 0.05;
const EDGE_COLOR: [f32; 3] = [0.08, 0.08, 0.1];
/// Over a component's own colour: the one selected, and parts that interfere.
const SELECTED: [u8; 4] = [60, 140, 240, 255];
const CLASH: [u8; 4] = [220, 40, 30, 255];
/// Space between parts laid out on the bed (mm).
const BED_GAP: f64 = 5.0;
/// How often component files are checked for changes.
const REFRESH: Duration = Duration::from_secs(1);
/// Exploded view steps per second while it animates.
const EXPLODE_SPEED: f64 = 1.5;
/// Radians a component turns per pixel of a Shift+drag.
const TURN_PER_PIXEL: f32 = 0.01;

/// Two parts that share volume.
pub struct Clash {
    pub names: (String, String),
    pub leaves: (usize, usize),
    pub volume: f64,
}

/// A component being dragged.
pub struct Grab {
    pub component: u32,
    pub start: Pose,
    /// The point under the mouse when the drag began, and the component's middle.
    pub at: Vec3,
    pub center: P3,
    pub cursor: Vec2,
    pub key: u64,
}

pub struct Session {
    pub doc: AssemblyDocument,
    /// For each body in the scene: which leaf (as in `doc.leaves()`) and which body of its part.
    pub bodies: Vec<(usize, usize)>,
    /// The component picked in the tree or the viewport, the mate or explode step picked in the tree.
    pub selected: Option<u32>,
    pub mate: Option<usize>,
    pub step: Option<usize>,
    /// Where the exploded view is (0 collapsed, up to the number of steps) and where it is going.
    pub explode: f64,
    pub explode_to: f64,
    /// Interference found, and the assembly revision it was found for.
    pub clashes: Option<(u64, Vec<Clash>)>,
    pub show_clashes: bool,
    pub show_bom: bool,
    pub bom_indented: bool,
    pub grab: Option<Grab>,
    meshes: HashMap<u64, Arc<Vec<RenderMesh>>>,
    built_for: Option<(u64, u64, Option<u32>, bool)>,
    checked: Instant,
    drags: u64,
}

impl Session {
    pub fn new(doc: AssemblyDocument) -> Self {
        Self {
            doc,
            bodies: Vec::new(),
            selected: None,
            mate: None,
            step: None,
            explode: 0.0,
            explode_to: 0.0,
            clashes: None,
            show_clashes: false,
            show_bom: false,
            bom_indented: true,
            grab: None,
            meshes: HashMap::new(),
            built_for: None,
            checked: Instant::now(),
            drags: 0,
        }
    }

    /// The leaf and body a picked face belongs to.
    fn leaf_of(&self, r: ModelRef) -> Option<(usize, usize, usize)> {
        match r {
            ModelRef::Face { body, face } => self.bodies.get(body).map(|&(l, b)| (l, b, face)),
            _ => None,
        }
    }

    /// The component a picked face belongs to.
    pub fn component_of(&self, r: ModelRef) -> Option<u32> {
        let (l, ..) = self.leaf_of(r)?;
        self.doc.leaves().get(l).map(|l| l.component)
    }

    /// A picked face as a mate holds it.
    pub fn mate_face(&self, r: ModelRef) -> Option<MateFace> {
        let (l, b, face) = self.leaf_of(r)?;
        let leaves = self.doc.leaves();
        let leaf = leaves.get(l)?;
        Some(MateFace {
            component: leaf.component,
            leaf: leaf.leaf,
            face: leaf.part.face_ref(b, face)?,
        })
    }

    /// The outward normal of a picked flat face, where the part sits.
    pub fn face_normal(&self, r: ModelRef) -> Option<P3> {
        let (l, b, face) = self.leaf_of(r)?;
        let leaves = self.doc.leaves();
        let leaf = leaves.get(l)?;
        let plane = leaf.part.bodies().get(b)?.topo.faces.get(face)?.plane?;
        Some(leaf.pose.turn(plane.normal()))
    }

    pub fn describe(&self, r: ModelRef) -> String {
        match self.leaf_of(r) {
            Some((l, _, face)) => {
                let name = self.doc.leaves().get(l).map_or_else(|| "?".into(), |l| l.name.clone());
                format!("Face {} of {name}", face + 1)
            }
            None => "?".into(),
        }
    }

    /// The exploded view moves towards where it is going; true while it is moving.
    pub fn animate(&mut self, dt: f64) -> bool {
        let gap = self.explode_to - self.explode;
        if gap == 0.0 {
            return false;
        }
        let step = EXPLODE_SPEED * dt;
        self.explode = if gap.abs() <= step {
            self.explode_to
        } else {
            self.explode + step * gap.signum()
        };
        true
    }

    /// Re-reads parts changed on disk, at most once a second; true when one was.
    pub fn refresh(&mut self) -> bool {
        if self.checked.elapsed() < REFRESH {
            return false;
        }
        self.checked = Instant::now();
        let changed = self.doc.refresh();
        if changed {
            self.clashes = None;
        }
        changed
    }

    /// The clashes for the assembly as it is now, if they were looked for.
    pub fn current_clashes(&self) -> Option<&[Clash]> {
        match &self.clashes {
            Some((rev, c)) if *rev == self.doc.revision() => Some(c),
            _ => None,
        }
    }

    pub fn check_interference(&mut self) -> usize {
        let names: Vec<String> = self.doc.leaves().iter().map(|l| l.name.clone()).collect();
        let found: Vec<Clash> = self
            .doc
            .interferences()
            .into_iter()
            .map(|(a, b, x)| Clash {
                names: (names[a].clone(), names[b].clone()),
                leaves: (a, b),
                volume: x.volume,
            })
            .collect();
        let n = found.len();
        self.clashes = Some((self.doc.revision(), found));
        self.show_clashes = true;
        n
    }
}

impl App {
    /// The assembly scene: every part where it sits (and where the exploded view moves it), in
    /// its material's colour.
    pub(crate) fn sync_assembly(&mut self) {
        let Some(s) = self.asm.as_mut() else { return };
        // A component picked by its faces is not tinted: the picked faces must show.
        let tinted = s.selected.filter(|_| self.selection.is_empty());
        let key = (
            s.doc.revision(),
            s.explode.to_bits(),
            tinted,
            s.show_clashes && s.current_clashes().is_some(),
        );
        if s.built_for == Some(key) {
            return;
        }
        let clashing: HashSet<usize> = match (s.show_clashes, s.current_clashes()) {
            (true, Some(c)) => c.iter().flat_map(|c| [c.leaves.0, c.leaves.1]).collect(),
            _ => HashSet::new(),
        };
        let mut g = SceneGeometry::default();
        g.add_grid(100.0, 5.0);
        let mut refs = Vec::new();
        let mut bodies = Vec::new();
        let mut meshes = HashMap::new();
        let f32x3 = |p: P3| p.map(|v| v as f32);
        let f64x3 = |p: [f32; 3]| p.map(f64::from);
        for (li, l) in s.doc.leaves().iter().enumerate() {
            let pose = l.pose.moved(s.doc.explode_offset(l.component, s.explode));
            let m = s.meshes.get(&l.generation).cloned().unwrap_or_else(|| {
                Arc::new(
                    l.part
                        .bodies()
                        .iter()
                        .map(|b| mesher::render_mesh(&b.solid, DISPLAY_TOL))
                        .collect(),
                )
            });
            meshes.insert(l.generation, m.clone());
            let tint = if clashing.contains(&li) {
                Some(CLASH)
            } else if tinted == Some(l.component) {
                Some(SELECTED)
            } else {
                None
            };
            let material = l.part.material().color;
            for (bi, m) in m.iter().enumerate() {
                // The part's face colours, else its material's.
                let colors = l.part.face_colors(bi);
                let face_tint = |f: u32| {
                    let [r, g, b] = colors.get(f as usize).copied().flatten().unwrap_or(material);
                    [r, g, b, 255]
                };
                let body = bodies.len();
                bodies.push((li, bi));
                let base = refs.len() as u32;
                refs.extend((0..m.faces.len()).map(|face| ModelRef::Face { body, face }));
                g.triangles.extend(
                    m.positions
                        .iter()
                        .zip(&m.normals)
                        .zip(&m.face_of)
                        .map(|((&p, &n), &f)| MeshVertex {
                            pos: f32x3(pose.apply(f64x3(p))),
                            nor: f32x3(pose.turn(f64x3(n))),
                            id: base + f,
                            tint: tint.unwrap_or_else(|| face_tint(f)),
                        }),
                );
                for poly in &m.edges {
                    for w in poly.windows(2) {
                        g.edge(
                            f32x3(pose.apply(f64x3(w[0]))),
                            f32x3(pose.apply(f64x3(w[1]))),
                            EDGE_COLOR,
                            NO_ID,
                        );
                    }
                }
            }
        }
        s.meshes = meshes;
        s.bodies = bodies;
        s.built_for = Some(key);
        self.geometry = Arc::new(g);
        self.geometry_version += 1;
        // Faces are numbered afresh; a pick of two faces for a mate survives a drag of something else.
        let keep = self.refs.len() == refs.len();
        self.refs = refs;
        if !keep {
            self.selection.clear();
        }
        self.hover = None;
    }

    /// Mouse drags in the viewport move (Shift: turn) the component under the mouse, as far as its
    /// mates let it. True while a drag is the mouse's, not the camera's.
    pub(crate) fn drag_component(&mut self, resp: &eframe::egui::Response, rect: eframe::egui::Rect) -> bool {
        use eframe::egui::PointerButton;
        let Some(s) = self.asm.as_mut() else { return false };
        let size = Vec2::new(rect.width(), rect.height());
        let cursor = |p: eframe::egui::Pos2| Vec2::new(p.x - rect.min.x, p.y - rect.min.y);
        if resp.drag_started_by(PointerButton::Primary) {
            let Some(at) = resp.interact_pointer_pos().map(cursor) else {
                return false;
            };
            let hit = pick(&self.geometry, &self.camera, size, at, 2.0);
            let Some((hit, r)) = hit.and_then(|h| Some((h, *self.refs.get(h.id as usize)?))) else {
                return false;
            };
            let Some(id) = s.component_of(r) else { return false };
            let Some(c) = s.doc.component(id) else { return false };
            if c.fixed {
                self.status = format!("{} is fixed: float it to move it", c.name);
                return false;
            }
            if s.explode != 0.0 {
                self.status = tr("Collapse the exploded view to move components").into();
                return false;
            }
            s.drags += 1;
            let center = s
                .doc
                .leaves()
                .iter()
                .filter(|l| l.component == id)
                .flat_map(|l| {
                    l.part
                        .bodies()
                        .iter()
                        .flat_map(|b| b.topo.vertices.iter().map(|&v| l.pose.apply(v)))
                })
                .fold(None, |acc: Option<(P3, P3)>, p| {
                    let (lo, hi) = acc.unwrap_or((p, p));
                    Some((
                        std::array::from_fn(|k| lo[k].min(p[k])),
                        std::array::from_fn(|k| hi[k].max(p[k])),
                    ))
                })
                .map_or(c.pose.t, |(lo, hi)| std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0));
            s.grab = Some(Grab {
                component: id,
                start: c.pose,
                at: hit.point,
                center,
                cursor: at,
                key: s.drags,
            });
            s.selected = Some(id);
        }
        let Some(grab) = &s.grab else { return false };
        if resp.dragged_by(PointerButton::Primary) {
            if let Some(now) = resp.interact_pointer_pos().map(cursor) {
                let rotate = resp.ctx.input(|i| i.modifiers.shift);
                let target = if rotate {
                    let d = (now - grab.cursor) * TURN_PER_PIXEL;
                    let (right, up) = (self.camera.rotation * Vec3::X, self.camera.rotation * Vec3::Y);
                    let w = up * d.x + right * d.y;
                    grab.start.rotated(w.to_array().map(f64::from), grab.center)
                } else {
                    // Along the plane facing the camera through the point first grabbed.
                    let (o, dir) = self.camera.ray(now, size);
                    let n = self.camera.rotation * Vec3::Z;
                    let t = (grab.at - o).dot(n) / dir.dot(n);
                    let moved = o + dir * t - grab.at;
                    grab.start.moved(moved.to_array().map(f64::from))
                };
                let (id, key) = (grab.component, grab.key);
                s.doc.drag(id, target, key);
            }
        }
        if resp.drag_stopped() {
            s.doc.end_merge();
            s.grab = None;
        }
        true
    }

    /// STEP: each part once, and every component placed by its name. 3MF: one object per part,
    /// each on its print face, set out on the bed for the slicer.
    pub(crate) fn write_assembly(&mut self, format: crate::app::ExportFormat, path: &Path) -> io::Result<()> {
        let Some(s) = self.asm.as_ref() else {
            return Err(io::Error::other("no assembly is open"));
        };
        let leaves = s.doc.leaves();
        if leaves.is_empty() {
            return Err(io::Error::other("the assembly has no parts"));
        }
        if format == crate::app::ExportFormat::Step {
            let mut files: Vec<&Path> = Vec::new();
            for l in &leaves {
                if !files.contains(&l.file) {
                    files.push(l.file);
                }
            }
            let names: Vec<String> = files
                .iter()
                .map(|f| {
                    f.file_stem()
                        .map_or_else(|| "part".into(), |n| n.to_string_lossy().into_owned())
                })
                .collect();
            let parts: Vec<(&str, Vec<&threebcad_kernel::Solid>)> = files
                .iter()
                .zip(&names)
                .map(|(f, n)| {
                    let part = leaves.iter().find(|l| l.file == *f).expect("a leaf uses it").part;
                    (n.as_str(), part.bodies().iter().map(|b| &b.solid).collect())
                })
                .collect();
            let instances: Vec<(&str, usize, Pose)> = leaves
                .iter()
                .map(|l| {
                    (
                        l.name.as_str(),
                        files.iter().position(|f| *f == l.file).unwrap_or(0),
                        l.pose,
                    )
                })
                .collect();
            return threebcad_io::write_step_assembly(path, &s.doc.name, &parts, &instances);
        }
        let st = &self.files.settings;
        let objects: Vec<(String, PrintMesh, P3)> = leaves
            .iter()
            .map(|l| {
                let mut mesh = PrintMesh::default();
                for b in l.part.bodies() {
                    let m = mesher::print_mesh_within(&b.solid, st.chord, st.angle_deg.to_radians());
                    let base = mesh.vertices.len();
                    mesh.vertices.extend(m.vertices);
                    mesh.triangles.extend(m.triangles.iter().map(|t| t.map(|i| i + base)));
                }
                (l.name.clone(), mesh, l.part.print_down())
            })
            .collect();
        let laid = bed_layout(objects, st.bed);
        let named: Vec<(&str, &PrintMesh)> = laid.iter().map(|(n, m)| (n.as_str(), m)).collect();
        if format == crate::app::ExportFormat::Stl {
            return threebcad_io::write_stl(path, &named);
        }
        let first = leaves[0].part.material();
        let info = threebcad_io::threemf::Info {
            title: s.doc.name.clone(),
            material: first.name.clone(),
            color: first.color,
            colors: Vec::new(),
            settings: vec![
                ("Printer".into(), st.printer.clone()),
                ("NozzleDiameter".into(), format!("{}", st.nozzle)),
            ],
        };
        threebcad_io::write_3mf(path, &named, &info)
    }

    /// The bill of materials as a CSV file.
    pub(crate) fn export_bom(&mut self) {
        let Some(s) = self.asm.as_ref() else { return };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(format!("{} BOM.csv", s.doc.name))
            .save_file()
        else {
            return;
        };
        let csv = assembly::bom_csv(&s.doc.bom(s.bom_indented));
        self.status = match std::fs::write(&path, csv) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    /// Opens a component's part (or sub-assembly) to edit it, with a way back to this assembly.
    pub(crate) fn edit_component(&mut self, id: u32) {
        let Some(s) = self.asm.as_ref() else { return };
        let Some(c) = s.doc.component(id) else { return };
        let file = c.file.clone();
        match &self.files.path {
            Some(me) => {
                self.files.return_to = Some(me.clone());
                self.request(crate::files::Pending::Open(file));
            }
            None => self.status = tr("Save the assembly first, so 3BCAD can bring you back to it").into(),
        }
    }

    /// Adds a part or assembly file as a component.
    pub(crate) fn insert_component(&mut self, file: &Path) {
        let Some(s) = self.asm.as_mut() else { return };
        self.status = match s.doc.insert(file) {
            Ok(id) => {
                s.selected = Some(id);
                let name = s.doc.component(id).map_or_else(String::new, |c| c.name.clone());
                self.files.settings.add_recent(file);
                self.files.settings_changed = true;
                format!("Inserted {name}")
            }
            Err(e) => format!("Could not insert: {e:#}"),
        };
        self.sync_assembly();
        self.zoom_to_fit();
    }

    pub(crate) fn insert_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("3BCAD part or assembly", &[crate::files::EXT, assembly::EXT])
            .pick_file()
        {
            self.insert_component(&path);
        }
    }
}

/// Files that can go into an assembly from the recent list (parts and other assemblies).
pub fn insertable(recent: &[PathBuf], own: Option<&Path>) -> Vec<PathBuf> {
    recent
        .iter()
        .filter(|p| Some(p.as_path()) != own && p.exists())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e == crate::files::EXT || e == assembly::EXT)
        })
        .cloned()
        .collect()
}

/// Each object stood on its print face (`down`, its part's outward normal) and set out on the bed
/// in rows, `BED_GAP` apart, the whole layout centred on the bed.
// shortcut: rows in the given order, no rotation to pack tighter; the slicer's Arrange packs a full
// plate better and spills to more plates.
pub fn bed_layout(objects: Vec<(String, PrintMesh, P3)>, bed: [f64; 3]) -> Objects {
    let mut out: Objects = Vec::new();
    let (mut x, mut y, mut row) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut width, mut depth) = (0.0_f64, 0.0_f64);
    for (name, mesh, down) in objects {
        let r = checks::lay_flat(down);
        let mut vertices: Vec<P3> = mesh.vertices.iter().map(|&v| checks::rotate(&r, v)).collect();
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in &vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        if !lo[0].is_finite() {
            continue;
        }
        let (w, d) = (hi[0] - lo[0], hi[1] - lo[1]);
        if x > 0.0 && x + w > bed[0] {
            x = 0.0;
            y += row + BED_GAP;
            row = 0.0;
        }
        for v in &mut vertices {
            *v = [v[0] - lo[0] + x, v[1] - lo[1] + y, v[2] - lo[2]];
        }
        width = width.max(x + w);
        depth = depth.max(y + d);
        x += w + BED_GAP;
        row = row.max(d);
        out.push((
            name,
            PrintMesh {
                vertices,
                triangles: mesh.triangles,
                face_of: mesh.face_of,
            },
        ));
    }
    let shift = [(bed[0] - width) / 2.0, (bed[1] - depth) / 2.0];
    for (_, m) in &mut out {
        for v in &mut m.vertices {
            v[0] += shift[0];
            v[1] += shift[1];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use threebcad_kernel::operations::extrude_polygon;

    use super::*;

    fn block(w: f64, d: f64, h: f64) -> PrintMesh {
        mesher::print_mesh(
            &extrude_polygon(&[[0.0, 0.0], [w, 0.0], [w, d], [0.0, d]], h).unwrap(),
            0.01,
        )
    }

    fn bounds(m: &PrintMesh) -> (P3, P3) {
        let lo = std::array::from_fn(|k| m.vertices.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min));
        let hi = std::array::from_fn(|k| m.vertices.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max));
        (lo, hi)
    }

    #[test]
    fn parts_are_laid_out_side_by_side_on_their_print_faces() {
        let down = [0.0, 0.0, -1.0];
        // A 100 mm wide bed: two 40 mm blocks fit in a row, the third starts the next row.
        let objects = vec![
            ("a".to_string(), block(40.0, 10.0, 5.0), down),
            ("b".to_string(), block(40.0, 20.0, 5.0), down),
            // Printed on its side: 30 tall becomes 30 deep.
            ("c".to_string(), block(20.0, 10.0, 30.0), [0.0, -1.0, 0.0]),
        ];
        let laid = bed_layout(objects, [100.0, 100.0, 100.0]);
        let b: Vec<(P3, P3)> = laid.iter().map(|(_, m)| bounds(m)).collect();
        for (lo, _) in &b {
            assert!(lo[2].abs() < 1e-9, "every part stands on the bed");
        }
        assert!((b[1].0[0] - b[0].1[0] - BED_GAP).abs() < 1e-9, "b is beside a");
        assert!(
            (b[2].0[1] - b[1].1[1] - BED_GAP).abs() < 1e-9,
            "c starts a new row behind the deeper b"
        );
        assert!((b[2].1[2] - 10.0).abs() < 1e-9 && (b[2].1[1] - b[2].0[1] - 30.0).abs() < 1e-9);
        // The layout (85 wide, 55 deep) sits in the middle of the bed.
        let lo = [b[0].0[0], b[0].0[1]];
        assert!((lo[0] - 7.5).abs() < 1e-9 && (lo[1] - 22.5).abs() < 1e-9, "{lo:?}");
    }
}
