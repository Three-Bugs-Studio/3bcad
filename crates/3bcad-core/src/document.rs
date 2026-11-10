use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, ensure, Context};
use serde::{Deserialize, Serialize};

use threebcad_kernel::topology::P3;
use threebcad_kernel::Plane;
use threebcad_sketch::{Sketch, Status};

use crate::naming::{self, EdgeRef, FaceRef, VertexRef};
use crate::rebuild::{self, CacheEntry};
use crate::{Body, FeatureId, History, Material, Paint, Rebuild, Units};

const UNDO_LIMIT: usize = 100;

/// Tag at the top of every part file, so other JSON is not mistaken for a part.
const FORMAT: &str = "3bcad-part";
/// Bump when the saved structure changes, and teach `migrate` to read the old one.
/// 2 (v0.6): the Fit feature and the print orientation. 3 (v0.7): custom properties.
/// 4 (v0.9): equations, configurations, appearances and sketch dimension numbers.
pub const FORMAT_VERSION: u64 = 4;

/// The outward normal of the face a new part prints on: its bottom.
pub const DEFAULT_PRINT_DOWN: P3 = [0.0, 0.0, -1.0];

/// The value of a custom property, matched without regard to case.
pub fn property<'a>(properties: &'a [(String, String)], name: &str) -> Option<&'a str> {
    properties
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

/// What a `.3bcad` file holds: the feature history, not the geometry it builds.
#[derive(Serialize, Deserialize)]
struct PartFile {
    format: String,
    version: u64,
    name: String,
    units: Units,
    material: Material,
    #[serde(default = "default_print_down")]
    print_down: P3,
    /// Part number, description and the like, for the bill of materials.
    #[serde(default)]
    properties: Vec<(String, String)>,
    history: History,
}

fn default_print_down() -> P3 {
    DEFAULT_PRINT_DOWN
}

/// Brings an older file's JSON up to `FORMAT_VERSION`, one version at a time.
fn migrate(file: serde_json::Value, version: u64) -> anyhow::Result<serde_json::Value> {
    match version {
        FORMAT_VERSION => Ok(file),
        // Versions 2 to 4 only added things: older files read as they are.
        1..=3 => migrate(file, version + 1),
        v => bail!("this part was saved in format version {v}, which this 3BCAD cannot read"),
    }
}

/// A part: its feature history, the bodies it builds, and undo/redo.
pub struct PartDocument {
    pub name: String,
    units: Units,
    material: Material,
    /// Outward normal of the face that rests on the printer bed.
    print_down: P3,
    properties: Vec<(String, String)>,
    /// Changed since it was last saved or opened.
    dirty: bool,
    history: History,
    undo: Vec<History>,
    redo: Vec<History>,
    merge_key: Option<u64>,
    built: Rebuild,
    cache: Vec<CacheEntry>,
    revision: u64,
    /// Each active equation's value or error, from the last rebuild.
    solved: Vec<Result<f64, String>>,
}

impl PartDocument {
    pub fn new(name: impl Into<String>) -> Self {
        let mut doc = Self {
            name: name.into(),
            units: Units::default(),
            material: Material::default(),
            print_down: DEFAULT_PRINT_DOWN,
            properties: Vec::new(),
            dirty: false,
            history: History::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            merge_key: None,
            built: Rebuild::default(),
            cache: Vec::new(),
            revision: 0,
            solved: Vec::new(),
        };
        doc.rebuild();
        doc
    }

    /// Starter part: a 30 mm cube and a 20 mm diameter cylinder.
    pub fn demo() -> Self {
        let mut doc = Self::new("Part1");
        doc.edit(|h| -> anyhow::Result<()> {
            let s = h.add_sketch(Sketch::rectangle(30.0, 30.0), Plane::TOP);
            h.add_extrude(s, 30.0)?;
            let c = h.add_sketch(Sketch::circle([55.0, 15.0], 10.0), Plane::TOP);
            h.add_extrude(c, 40.0)?;
            Ok(())
        })
        .expect("demo part builds");
        doc.undo.clear();
        doc.dirty = false;
        doc
    }

    /// The part as `.3bcad` JSON (pretty-printed, so files diff well in version control).
    pub fn to_json(&self) -> String {
        let file = PartFile {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            name: self.name.clone(),
            units: self.units,
            material: self.material.clone(),
            print_down: self.print_down,
            properties: self.properties.clone(),
            history: self.history.clone(),
        };
        serde_json::to_string_pretty(&file).expect("a part always serialises")
    }

    /// Reads `.3bcad` JSON, refusing damaged files and files from a newer 3BCAD.
    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(text).context("the file is not valid JSON")?;
        ensure!(
            value.get("format").and_then(|f| f.as_str()) == Some(FORMAT),
            "this is not a 3BCAD part file"
        );
        let version = value
            .get("version")
            .and_then(|v| v.as_u64())
            .context("the file has no format version")?;
        ensure!(
            version <= FORMAT_VERSION,
            "this part was saved by a newer 3BCAD (format version {version}); this version reads up to {FORMAT_VERSION}. Update 3BCAD to open it"
        );
        let file: PartFile = serde_json::from_value(migrate(value, version)?).context("the part file is damaged")?;
        let mut history = file.history;
        history.validate().context("the part file is damaged")?;
        history.relink();
        let mut doc = Self::new(file.name);
        doc.units = file.units;
        doc.material = file.material;
        doc.print_down = file.print_down;
        doc.properties = file.properties;
        doc.history = history;
        doc.rebuild();
        Ok(doc)
    }

    /// Writes the part next to `path` first and then moves it into place, so a failed save never
    /// destroys the previous file.
    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        self.write_to(path)?;
        self.dirty = false;
        Ok(())
    }

    /// Saves a copy without marking the part saved (autosave).
    pub fn write_to(&self, path: &Path) -> anyhow::Result<()> {
        let tmp = path.with_extension("3bcad.tmp");
        std::fs::write(&tmp, self.to_json()).with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::from_json(&text)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the part as having unsaved changes (a recovered autosave).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn units(&self) -> Units {
        self.units
    }

    /// An empty part in `units` (Settings > units of new parts), with nothing to save yet.
    pub fn new_in(name: impl Into<String>, units: Units) -> Self {
        let mut doc = Self::new(name);
        doc.units = units;
        doc
    }

    pub fn set_units(&mut self, units: Units) {
        if units != self.units {
            self.dirty = true;
            self.units = units;
            // Plain numbers in equations are in document units.
            self.rebuild();
        }
    }

    pub fn material(&self) -> &Material {
        &self.material
    }

    pub fn set_material(&mut self, material: Material) {
        self.dirty |= material != self.material;
        self.material = material;
    }

    pub fn print_down(&self) -> P3 {
        self.print_down
    }

    pub fn set_print_down(&mut self, down: P3) {
        self.dirty |= down != self.print_down;
        self.print_down = down;
    }

    /// Custom properties (name, value), in the order the user gave them.
    pub fn properties(&self) -> &[(String, String)] {
        &self.properties
    }

    pub fn set_properties(&mut self, properties: Vec<(String, String)>) {
        self.dirty |= properties != self.properties;
        self.properties = properties;
    }

    pub fn property(&self, name: &str) -> Option<&str> {
        property(&self.properties, name)
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn bodies(&self) -> &[Body] {
        &self.built.bodies
    }

    pub fn built(&self) -> &Rebuild {
        &self.built
    }

    pub fn errors(&self) -> &HashMap<FeatureId, String> {
        &self.built.errors
    }

    pub fn warnings(&self) -> &HashMap<FeatureId, String> {
        &self.built.warnings
    }

    /// Definition status and degrees of freedom of a sketch above the rollback bar.
    pub fn sketch_status(&self, id: FeatureId) -> Option<(Status, usize)> {
        self.built.sketch_status.get(&id).copied()
    }

    /// Bumps on every rebuild; viewers compare it to know when to re-mesh.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn edit<R>(&mut self, f: impl FnOnce(&mut History) -> R) -> R {
        self.edit_merging(None, f)
    }

    /// Consecutive edits with the same `key` collapse into one undo step,
    /// so dragging a dimension does not flood the undo stack.
    pub fn edit_merging<R>(&mut self, key: Option<u64>, f: impl FnOnce(&mut History) -> R) -> R {
        if key.is_none() || key != self.merge_key {
            self.undo.push(self.history.clone());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.merge_key = key;
        self.dirty = true;
        let r = f(&mut self.history);
        self.history.relink();
        self.rebuild();
        r
    }

    /// Ends a run of merged edits: the next one is a new undo step, even with the same key.
    pub fn end_merge(&mut self) {
        self.merge_key = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.history, prev));
            self.merge_key = None;
            self.dirty = true;
            self.rebuild();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.history, next));
            self.merge_key = None;
            self.dirty = true;
            self.rebuild();
        }
    }

    /// Rebuilds every feature from scratch (Ctrl+Q), ignoring cached results.
    pub fn force_rebuild(&mut self) {
        self.cache.clear();
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.solved = self.history.apply_equations(self.units);
        self.built = rebuild::rebuild(&mut self.history, &mut self.cache);
        self.revision += 1;
    }

    /// Each of `History::active_equations`' value (document units, degrees) or error.
    pub fn equation_results(&self) -> &[Result<f64, String>] {
        &self.solved
    }

    /// Makes configuration `i` the active one (one undo step).
    pub fn activate(&mut self, i: usize) {
        if i != self.history.active_configuration() && i < self.history.configurations().len() {
            self.edit(|h| h.activate(i));
        }
    }

    /// A copy of the part built in configuration `i`, for exporting it.
    pub fn in_configuration(&self, i: usize) -> anyhow::Result<PartDocument> {
        let mut doc = Self::from_json(&self.to_json())?;
        doc.history.activate(i);
        doc.rebuild();
        Ok(doc)
    }

    /// Gives `target` a colour, or takes its colour away (`None`). One colour per face, per body
    /// and per feature: an older one for the same face or body is replaced.
    pub fn set_appearance(&mut self, target: Paint, color: Option<[u8; 3]>, merge: Option<u64>) {
        let at = |r: &FaceRef| self.resolve_face(r);
        let same = |p: &Paint| match (p, &target) {
            (Paint::Face(a), Paint::Face(b)) => a == b || at(a).is_some() && at(a) == at(b),
            (Paint::Body(a), Paint::Body(b)) => a == b || at(a).is_some() && at(a).map(|x| x.0) == at(b).map(|x| x.0),
            (a, b) => a == b,
        };
        let mut list: Vec<_> = self
            .history
            .appearances()
            .iter()
            .filter(|(p, _)| !same(p))
            .cloned()
            .collect();
        list.extend(color.map(|c| (target.clone(), c)));
        self.edit_merging(merge, |h| *h.appearances_mut() = list);
    }

    /// The colour of each face of body `body` (`None`: the material colour).
    pub fn face_colors(&self, body: usize) -> Vec<Option<[u8; 3]>> {
        let Some(b) = self.bodies().get(body) else {
            return Vec::new();
        };
        let paints = self.history.appearances();
        let mut out: Vec<Option<[u8; 3]>> = b
            .tags
            .iter()
            .map(|t| {
                paints.iter().rev().find_map(|(p, c)| match p {
                    Paint::Feature(f) if *f == t.feature => Some(*c),
                    _ => None,
                })
            })
            .collect();
        for (p, c) in paints {
            if let Paint::Body(r) = p {
                if self.resolve_face(r).is_some_and(|(b, _)| b == body) {
                    out.iter_mut().for_each(|o| *o = Some(*c));
                }
            }
        }
        for (p, c) in paints {
            if let Paint::Face(r) = p {
                if let Some((_, f)) = self.resolve_face(r).filter(|(b, _)| *b == body) {
                    out[f] = Some(*c);
                }
            }
        }
        out
    }

    /// Persistent reference to a face of the current bodies.
    pub fn face_ref(&self, body: usize, face: usize) -> Option<FaceRef> {
        naming::face_ref(self.bodies(), body, face)
    }

    pub fn edge_ref(&self, body: usize, edge: usize) -> Option<EdgeRef> {
        naming::edge_ref(self.bodies(), body, edge)
    }

    pub fn vertex_ref(&self, body: usize, vertex: usize) -> Option<VertexRef> {
        naming::vertex_ref(self.bodies(), body, vertex)
    }

    pub fn resolve_face(&self, r: &FaceRef) -> Option<(usize, usize)> {
        naming::resolve_face(self.bodies(), r)
    }

    pub fn resolve_edge(&self, r: &EdgeRef) -> Option<(usize, usize)> {
        naming::resolve_edge(self.bodies(), r)
    }

    /// Reference planes and coordinate systems built by features.
    pub fn plane_of(&self, id: FeatureId) -> Option<Plane> {
        self.built.planes.get(&id).copied()
    }

    pub fn axis_of(&self, id: FeatureId) -> Option<(P3, P3)> {
        self.built.axes.get(&id).copied()
    }

    pub fn point_of(&self, id: FeatureId) -> Option<P3> {
        self.built.points.get(&id).copied()
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use threebcad_kernel::tool::ToolFace;
    use threebcad_kernel::topology::EdgeCurve;

    use super::*;
    use crate::feature::*;

    #[test]
    fn damaged_or_newer_files_are_refused() {
        let doc = PartDocument::demo();
        let json = doc.to_json();
        let err = |text: &str| format!("{:#}", PartDocument::from_json(text).err().expect("refused"));
        assert!(err(&json[..json.len() / 2]).contains("not valid JSON"));
        assert!(err("{\"format\": \"something-else\"}").contains("not a 3BCAD part"));
        let version = format!("\"version\": {FORMAT_VERSION}");
        assert!(err(&json.replacen(&version, "\"version\": 99", 1)).contains("newer 3BCAD"));
        // A v0.5 file (format 1, no print orientation) opens, printed on its bottom.
        let mut v1: serde_json::Value = serde_json::from_str(&json.replacen(&version, "\"version\": 1", 1)).unwrap();
        v1.as_object_mut().unwrap().remove("print_down");
        assert_eq!(
            PartDocument::from_json(&v1.to_string()).unwrap().print_down(),
            DEFAULT_PRINT_DOWN
        );
        // A feature pointing at a sketch that is not in the file.
        let bad = json.replacen("\"sketch\": 0", "\"sketch\": 7", 1);
        assert_ne!(bad, json);
        assert!(err(&bad).contains("damaged"));
        // A sketch line pointing at a point that does not exist.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut v2 = v.clone();
        let nodes = v2["history"]["graph"]["nodes"].as_array_mut().unwrap();
        nodes[0]["kind"]["Sketch"]["sketch"]["entities"][0]["geom"]["Line"]["a"] = 999.into();
        assert!(err(&v2.to_string()).contains("damaged"));
        // Units and material are kept; a save clears the dirty flag.
        let mut d = PartDocument::demo();
        d.set_units(crate::Units {
            system: crate::UnitSystem::Ips,
            decimals: 3,
        });
        d.set_print_down([1.0, 0.0, 0.0]);
        d.set_properties(vec![("Part number".into(), "BRK-001".into())]);
        assert!(d.is_dirty());
        let path = std::env::temp_dir().join(format!("3bcad-doc-{}.3bcad", std::process::id()));
        d.save(&path).unwrap();
        assert!(!d.is_dirty());
        let back = PartDocument::open(&path).unwrap();
        assert_eq!(back.units(), d.units());
        assert_eq!(back.print_down(), [1.0, 0.0, 0.0]);
        assert_eq!(back.property("part NUMBER"), Some("BRK-001"));
        assert_eq!(back.history().len(), d.history().len());
        std::fs::remove_file(path).ok();
    }

    /// Volume from a fine mesh (holes are small next to the part, so a coarse one is too rough).
    fn vol(doc: &PartDocument) -> f64 {
        let meshes: Vec<_> = doc
            .bodies()
            .iter()
            .map(|b| threebcad_kernel::mesher::print_mesh(&b.solid, 0.005))
            .collect();
        threebcad_kernel::mass::mass_properties(&meshes).volume
    }

    fn cube(doc: &mut PartDocument, size: f64) -> FeatureId {
        doc.edit(|h| {
            let s = h.add_sketch(Sketch::rectangle(size, size), Plane::TOP);
            h.add_extrude(s, size).unwrap()
        })
    }

    /// The top face (z = `z`) of the first body.
    fn top_face(doc: &PartDocument, z: f64) -> FaceRef {
        let b = &doc.bodies()[0];
        let f = b
            .topo
            .faces
            .iter()
            .position(|f| {
                f.plane
                    .is_some_and(|p| p.normal()[2] > 0.5 && (p.origin[2] - z).abs() < 1e-6)
            })
            .expect("top face");
        doc.face_ref(0, f).unwrap()
    }

    fn edge_at(doc: &PartDocument, pred: impl Fn(P3, P3) -> bool) -> EdgeRef {
        let b = &doc.bodies()[0];
        let e = b
            .topo
            .edges
            .iter()
            .position(|e| matches!(e.curve, EdgeCurve::Line { a, b } if pred(a, b)))
            .expect("edge");
        doc.edge_ref(0, e).unwrap()
    }

    #[test]
    fn demo_builds_two_bodies() {
        let doc = PartDocument::demo();
        assert_eq!(doc.bodies().len(), 2);
        assert!(doc.errors().is_empty());
        let names: Vec<_> = doc.history().iter().map(|(_, f)| f.name.as_str()).collect();
        assert_eq!(names, ["Sketch1", "Boss-Extrude1", "Sketch2", "Boss-Extrude2"]);
        let sketch1 = doc.history().iter().next().unwrap().0;
        assert_eq!(doc.sketch_status(sketch1), Some((Status::FullyDefined, 0)));
    }

    #[test]
    fn sketch_with_a_hole_on_the_front_plane() {
        let mut doc = PartDocument::new("Part");
        let mut sketch = Sketch::rectangle(20.0, 20.0);
        let c = sketch.add_point(10.0, 10.0);
        sketch.add_circle(c, 5.0);
        let id = doc.edit(|h| {
            let s = h.add_sketch(sketch, Plane::FRONT);
            h.add_extrude(s, 4.0).map(|_| s)
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert_eq!(doc.bodies().len(), 1);
        // The circle is free to move and resize: under-defined.
        assert!(matches!(
            doc.sketch_status(id.unwrap()),
            Some((Status::UnderDefined, 3))
        ));
        let m = threebcad_kernel::mesher::render_mesh(&doc.bodies()[0].solid, 0.1);
        assert!(
            m.positions.iter().all(|p| p[1] <= 1e-4 && p[1] >= -4.0 - 1e-4),
            "grows towards -Y"
        );
    }

    #[test]
    fn open_sketch_reports_why_it_cannot_extrude() {
        let mut doc = PartDocument::new("Part");
        let mut sketch = Sketch::default();
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(5.0, 0.0);
        sketch.add_line(a, b);
        let e = doc.edit(|h| {
            let s = h.add_sketch(sketch, Plane::TOP);
            h.add_extrude(s, 4.0)
        });
        assert!(doc.errors()[&e.unwrap()].starts_with("Open contour"));
    }

    #[test]
    fn rollback_suppresses_later_features_and_inserts_at_bar() {
        let mut doc = PartDocument::demo();
        doc.edit(|h| h.set_rollback(2));
        assert_eq!(doc.bodies().len(), 1);
        let s = doc.edit(|h| h.add_sketch(Sketch::circle([0.0, 0.0], 3.0), Plane::TOP));
        assert_eq!(doc.history().iter().position(|(id, _)| id == s), Some(2));
        assert!(doc.edit(|h| h.add_extrude(s, 5.0)).is_ok());
        // The new cylinder overlaps the cube, so it merges into it.
        assert_eq!(doc.bodies().len(), 1);
        // A sketch below the bar cannot be extruded.
        let below = doc.history().iter().nth(4).unwrap().0;
        assert!(doc.edit(|h| h.add_extrude(below, 5.0)).is_err());
    }

    #[test]
    fn deleting_a_sketch_cascades_and_undo_restores() {
        let mut doc = PartDocument::demo();
        let sketch1 = doc.history().iter().next().unwrap().0;
        assert_eq!(doc.edit(|h| h.remove(sketch1)), 2);
        assert_eq!(
            (doc.history().len(), doc.history().rollback(), doc.bodies().len()),
            (2, 2, 1)
        );
        doc.undo();
        assert_eq!((doc.history().len(), doc.bodies().len()), (4, 2));
        doc.redo();
        assert_eq!(doc.history().len(), 2);
    }

    #[test]
    fn dimension_drag_is_one_undo_step_and_rebuilds() {
        let mut doc = PartDocument::demo();
        let extrude = doc.history().iter().nth(1).unwrap().0;
        for depth in [31.0, 32.0, 33.0] {
            doc.edit_merging(Some(1), |h| {
                if let Some(FeatureKind::Extrude(e)) = h.get_mut(extrude).map(|f| &mut f.kind) {
                    e.end = End::Blind(depth);
                }
            });
        }
        // A second drag of the same dimension is its own step.
        doc.end_merge();
        doc.edit_merging(Some(1), |h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(extrude).map(|f| &mut f.kind) {
                e.end = End::Blind(34.0);
            }
        });
        doc.undo();
        assert!(
            matches!(&doc.history().get(extrude).unwrap().kind, FeatureKind::Extrude(e) if e.end == End::Blind(33.0))
        );
        doc.undo();
        assert!(!doc.can_undo());
        assert!(
            matches!(&doc.history().get(extrude).unwrap().kind, FeatureKind::Extrude(e) if e.end == End::Blind(30.0))
        );
    }

    #[test]
    fn a_reference_warns_when_its_twins_change() {
        // Two slots cut a block in three, so its top face is three twins. A sketch sits on the middle.
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 30.0);
        let slot = |x: f64| polygon_sketch(&[[x, -1.0], [x + 2.0, -1.0], [x + 2.0, 31.0], [x, 31.0]]);
        let (first, second) = (slot(8.0), slot(18.0));
        let cut = |h: &mut History, sketch: Sketch| {
            let s = h.add_sketch(sketch, Plane::TOP);
            let mut e = Extrude::cut(s, 0.0);
            e.end = End::MidPlane(100.0);
            h.add(FeatureKind::Extrude(e)).unwrap()
        };
        let first = doc.edit(|h| {
            let c = cut(h, first);
            cut(h, second);
            c
        });
        assert_eq!(doc.bodies().len(), 3);
        let (b, f) = doc
            .bodies()
            .iter()
            .enumerate()
            .find_map(|(b, body)| {
                let middle = |f: &threebcad_kernel::topology::TopoFace| {
                    f.plane
                        .is_some_and(|p| p.normal()[2] > 0.5 && (p.origin[2] - 30.0).abs() < 1e-6)
                        && (10.0..18.0).contains(&f.sample[0])
                };
                body.topo.faces.iter().position(middle).map(|f| (b, f))
            })
            .expect("the middle of the top face");
        let top = doc.face_ref(b, f).unwrap();
        let sketch = doc.edit(|h| {
            h.add_sketch_on(Sketch::circle([14.0, 15.0], 1.0), Plane::TOP, PlaneRef::Face(top))
                .unwrap()
        });
        assert!(doc.warnings().is_empty(), "{:?}", doc.warnings());
        // With the first slot gone the left and middle merge: index 1 is now the right-hand piece.
        doc.edit(|h| h.remove(first));
        assert!(
            doc.warnings()[&sketch].contains("split differently"),
            "{:?}",
            doc.warnings()
        );
    }

    #[test]
    fn bad_dimension_reports_error_on_feature() {
        let mut doc = PartDocument::demo();
        let extrude = doc.history().iter().nth(1).unwrap().0;
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(extrude).map(|f| &mut f.kind) {
                e.end = End::Blind(0.0);
            }
        });
        assert!(doc.errors().contains_key(&extrude));
        assert_eq!(doc.bodies().len(), 1);
    }

    #[test]
    fn cut_blind_and_through_all_from_a_face() {
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 10.0);
        let top = top_face(&doc, 10.0);
        // A Ø6 circle centred on the top face, cut 4 deep, then a second one through all.
        let (cut, sketch2) = doc.edit(|h| {
            let s = h
                .add_sketch_on(Sketch::circle([5.0, 5.0], 3.0), Plane::TOP, PlaneRef::Face(top.clone()))
                .unwrap();
            let c = h.add(FeatureKind::Extrude(Extrude::cut(s, 4.0))).unwrap();
            (c, s)
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (1000.0 - PI * 9.0 * 4.0)).abs() < 2.0, "{}", vol(&doc));
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(cut).map(|f| &mut f.kind) {
                e.end = End::ThroughAll;
            }
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (1000.0 - PI * 9.0 * 10.0)).abs() < 2.0, "{}", vol(&doc));
        // Flip side to cut keeps only the cylinder.
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(cut).map(|f| &mut f.kind) {
                e.flip_side = true;
            }
        });
        assert!((vol(&doc) - PI * 90.0).abs() < 2.0, "{}", vol(&doc));
        // The sketch on the face follows it when the cube grows.
        let base = doc.history().iter().nth(1).unwrap().0;
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(base).map(|f| &mut f.kind) {
                e.end = End::Blind(15.0);
            }
        });
        let Some(FeatureKind::Sketch { plane, .. }) = doc.history().get(sketch2).map(|f| &f.kind) else {
            panic!()
        };
        assert!((plane.origin[2] - 15.0).abs() < 1e-6, "{plane:?}");
    }

    #[test]
    fn touching_boss_merges_unless_told_not_to() {
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 10.0);
        let top = top_face(&doc, 10.0);
        let boss = doc.edit(|h| {
            let s = h
                .add_sketch_on(Sketch::circle([5.0, 5.0], 2.0), Plane::TOP, PlaneRef::Face(top))
                .unwrap();
            h.add_extrude(s, 5.0).unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert_eq!(doc.bodies().len(), 1);
        assert!((vol(&doc) - (1000.0 + PI * 4.0 * 5.0)).abs() < 1.0, "{}", vol(&doc));
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(boss).map(|f| &mut f.kind) {
                e.merge = false;
            }
        });
        assert_eq!(doc.bodies().len(), 2);
    }

    #[test]
    fn mid_plane_direction_two_and_draft() {
        let mut doc = PartDocument::new("Part");
        let e = cube(&mut doc, 10.0);
        let set = |doc: &mut PartDocument, f: &dyn Fn(&mut Extrude)| {
            doc.edit(|h| {
                if let Some(FeatureKind::Extrude(x)) = h.get_mut(e).map(|f| &mut f.kind) {
                    f(x);
                }
            })
        };
        set(&mut doc, &|x| x.end = End::MidPlane(10.0));
        let (lo, hi) = threebcad_kernel::boolean::bbox(&doc.bodies()[0].solid);
        assert!((lo[2] + 5.0).abs() < 1e-6 && (hi[2] - 5.0).abs() < 1e-6);
        set(&mut doc, &|x| {
            x.end = End::Blind(4.0);
            x.end2 = Some(End::Blind(6.0));
        });
        let (lo, hi) = threebcad_kernel::boolean::bbox(&doc.bodies()[0].solid);
        assert!((lo[2] + 6.0).abs() < 1e-6 && (hi[2] - 4.0).abs() < 1e-6);
        set(&mut doc, &|x| {
            x.end2 = None;
            x.end = End::Blind(5.0);
            x.draft = (0.2f64).atan();
        });
        assert!(
            (vol(&doc) - 5.0 / 3.0 * (100.0 + 64.0 + 80.0)).abs() < 0.01,
            "{}",
            vol(&doc)
        );
        set(&mut doc, &|x| {
            x.draft = 0.0;
            x.reverse = true;
        });
        let (lo, _) = threebcad_kernel::boolean::bbox(&doc.bodies()[0].solid);
        assert!((lo[2] + 5.0).abs() < 1e-6);
    }

    #[test]
    fn thin_features_from_closed_and_open_profiles() {
        let mut doc = PartDocument::new("Part");
        let e = cube(&mut doc, 10.0);
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(x)) = h.get_mut(e).map(|f| &mut f.kind) {
                x.thin = Some(Thin {
                    kind: ThinKind::OneDirection,
                    t1: 1.0,
                    t2: 0.0,
                    reverse: true,
                });
            }
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (100.0 - 64.0) * 10.0).abs() < 1e-3, "{}", vol(&doc));
        let mut open = Sketch::default();
        let (a, b, c) = (
            open.add_point(0.0, 0.0),
            open.add_point(10.0, 0.0),
            open.add_point(10.0, 10.0),
        );
        open.add_line(a, b);
        open.add_line(b, c);
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let s = h.add_sketch(open, Plane::TOP);
            let mut x = Extrude::boss(s, 5.0);
            x.thin = Some(Thin {
                kind: ThinKind::MidPlane,
                t1: 2.0,
                t2: 0.0,
                reverse: false,
            });
            h.add(FeatureKind::Extrude(x)).unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - 40.0 * 5.0).abs() < 1e-3, "{}", vol(&doc));
    }

    #[test]
    fn revolve_a_rectangle_about_a_centerline() {
        // On the Front plane: rectangle from x = 3..5, z = 0..10 and a vertical centerline at x = 0.
        let mut s = Sketch::default();
        let p = [[3.0, 0.0], [5.0, 0.0], [5.0, 10.0], [3.0, 10.0]].map(|q| s.add_point(q[0], q[1]));
        for i in 0..4 {
            s.add_line(p[i], p[(i + 1) % 4]);
        }
        let (c0, c1) = (s.add_point(0.0, 0.0), s.add_point(0.0, 10.0));
        let axis = s.add_line(c0, c1);
        s.set_construction(axis, true);
        let mut doc = PartDocument::new("Part");
        let rev = doc.edit(|h| {
            let id = h.add_sketch(s, Plane::FRONT);
            h.add(FeatureKind::Revolve(Revolve::new(id, false))).unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!(
            (vol(&doc) - PI * 16.0 * 10.0).abs() / (PI * 160.0) < 0.01,
            "{}",
            vol(&doc)
        );
        doc.edit(|h| {
            if let Some(FeatureKind::Revolve(r)) = h.get_mut(rev).map(|f| &mut f.kind) {
                r.angle = PI / 2.0;
            }
        });
        assert!((vol(&doc) - PI * 160.0 / 4.0).abs() < 1.0, "{}", vol(&doc));
    }

    #[test]
    fn a_fillet_rounds_a_cylinder_rim_and_a_hole() {
        // A 20 x 20 x 10 block with a 6 mm hole, and a fillet on the hole's top rim.
        let mut doc = PartDocument::new("Part");
        let base = cube(&mut doc, 20.0);
        let c = Sketch::circle([10.0, 10.0], 3.0);
        doc.edit(|h| {
            let s = h.add_sketch(c, Plane::TOP);
            let mut e = Extrude::cut(s, 20.0);
            e.end = End::ThroughAll;
            e.reverse = true;
            h.add(FeatureKind::Extrude(e)).unwrap();
        });
        let _ = base;
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let before = vol(&doc);
        let b = &doc.bodies()[0];
        let e = b
            .topo
            .edges
            .iter()
            .position(|e| matches!(e.curve, EdgeCurve::Circle { center, .. } if (center[2] - 20.0).abs() < 1e-6))
            .expect("hole rim");
        let edge = doc.edge_ref(0, e).unwrap();
        let fillet = doc.edit(|h| {
            h.add(FeatureKind::Fillet {
                edges: vec![edge],
                radius: 1.0,
                propagate: true,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        // Pappus: the corner's area (1 - π/4) at its centroid, 0.2234 mm out from the rim.
        let removed = before - vol(&doc);
        let expected = (1.0 - PI / 4.0) * 2.0 * PI * (3.0 + 0.2234);
        assert!((removed - expected).abs() < 0.05, "{removed} vs {expected}");
        // Too large for the plate: a clear error.
        doc.edit(|h| {
            if let Some(FeatureKind::Fillet { radius, .. }) = h.get_mut(fillet).map(|f| &mut f.kind) {
                *radius = 25.0;
            }
        });
        assert!(doc.errors()[&fillet].contains("too large"), "{:?}", doc.errors());
    }

    #[test]
    fn fillet_chamfer_survive_a_dimension_change() {
        let mut doc = PartDocument::new("Part");
        let base = cube(&mut doc, 10.0);
        let edge = edge_at(&doc, |a, b| {
            (a[0] - 10.0).abs() < 1e-9 && (b[0] - 10.0).abs() < 1e-9 && a[2] > 9.0 && b[2] > 9.0
        });
        let fillet = doc.edit(|h| {
            h.add(FeatureKind::Fillet {
                edges: vec![edge],
                radius: 2.0,
                propagate: false,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (1000.0 - (4.0 - PI) * 10.0)).abs() < 0.3, "{}", vol(&doc));
        // Taller cube: the fillet still finds its (moved) edge by name.
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(base).map(|f| &mut f.kind) {
                e.end = End::Blind(15.0);
            }
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (1500.0 - (4.0 - PI) * 10.0)).abs() < 0.3, "{}", vol(&doc));
        // Too big a radius is a clear error on the fillet, not a crash.
        doc.edit(|h| {
            if let Some(FeatureKind::Fillet { radius, .. }) = h.get_mut(fillet).map(|f| &mut f.kind) {
                *radius = 30.0;
            }
        });
        assert!(doc.errors()[&fillet].contains("too large"), "{:?}", doc.errors());
        // Swap it for a chamfer.
        doc.edit(|h| h.remove(fillet));
        let e2 = edge_at(&doc, |a, b| {
            (a[0] - 10.0).abs() < 1e-9 && (b[0] - 10.0).abs() < 1e-9 && a[2] > 14.0 && b[2] > 14.0
        });
        doc.edit(|h| {
            h.add(FeatureKind::Chamfer {
                edges: vec![e2],
                kind: ChamferKind::EqualDistance,
                d1: 1.0,
                d2: 1.0,
                angle: 0.0,
                flip: false,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (1500.0 - 0.5 * 10.0)).abs() < 0.01, "{}", vol(&doc));
    }

    #[test]
    fn shell_a_box_inward_and_outward() {
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 10.0);
        let top = top_face(&doc, 10.0);
        let shell = doc.edit(|h| {
            h.add(FeatureKind::Shell {
                faces: vec![top],
                thickness: 1.0,
                outward: false,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (1000.0 - 8.0 * 8.0 * 9.0)).abs() < 1e-3, "{}", vol(&doc));
        doc.edit(|h| {
            if let Some(FeatureKind::Shell { outward, .. }) = h.get_mut(shell).map(|f| &mut f.kind) {
                *outward = true;
            }
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!(
            (vol(&doc) - (12.0 * 12.0 * 11.0 - 1000.0)).abs() < 1e-3,
            "{}",
            vol(&doc)
        );
    }

    #[test]
    fn a_shell_with_no_open_face_leaves_a_void() {
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 10.0);
        doc.edit(|h| {
            h.add(FeatureKind::Shell {
                faces: Vec::new(),
                thickness: 1.0,
                outward: false,
            })
            .unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (1000.0 - 512.0)).abs() < 1e-3, "{}", vol(&doc));
    }

    #[test]
    fn a_revolved_cylinder_shells_around_its_axis() {
        // The profile has an edge on the axis, which must not move with the walls.
        let mut s = polygon_sketch(&[[0.0, 0.0], [5.0, 0.0], [5.0, 10.0], [0.0, 10.0]]);
        let (c0, c1) = (s.add_point(0.0, -2.0), s.add_point(0.0, 12.0));
        let axis = s.add_line(c0, c1);
        s.set_construction(axis, true);
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let id = h.add_sketch(s, Plane::FRONT);
            h.add(FeatureKind::Revolve(Revolve::new(id, false))).unwrap();
            h.add(FeatureKind::Shell {
                faces: Vec::new(),
                thickness: 1.0,
                outward: false,
            })
            .unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let expected = PI * (25.0 * 10.0 - 16.0 * 8.0);
        assert!((vol(&doc) - expected).abs() / expected < 0.01, "{}", vol(&doc));
    }

    #[test]
    fn a_through_hole_needs_material_for_its_counterbore() {
        // A 5 mm plate: the M5 counterbore is 5.4 deep and would leave no seat.
        for (size, fits) in [("M5", false), ("M4", true)] {
            let mut doc = PartDocument::new("Part");
            doc.edit(|h| {
                let s = h.add_sketch(Sketch::rectangle(40.0, 40.0), Plane::TOP);
                h.add_extrude(s, 5.0).unwrap();
            });
            let top = top_face(&doc, 5.0);
            let mut pts = Sketch::default();
            pts.add_point(20.0, 20.0);
            doc.edit(|h| {
                let s = h.add_sketch_on(pts, Plane::TOP, PlaneRef::Face(top)).unwrap();
                h.add(FeatureKind::Hole(Hole {
                    sketch: s,
                    kind: HoleKind::Counterbore,
                    standard: HoleStandard::Iso,
                    size: size.into(),
                    end: End::ThroughAll,
                }))
                .unwrap();
            });
            assert_eq!(doc.errors().is_empty(), fits, "{size}: {:?}", doc.errors());
        }
    }

    #[test]
    fn twin_faces_keep_their_identity_when_the_bodies_swap_places() {
        // A cut through the middle makes two bodies, whose top faces are twins.
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 30.0);
        let slot = polygon_sketch(&[[12.0, -1.0], [18.0, -1.0], [18.0, 31.0], [12.0, 31.0]]);
        doc.edit(|h| {
            let s = h.add_sketch(slot, Plane::TOP);
            let mut e = Extrude::cut(s, 0.0);
            e.end = End::MidPlane(100.0);
            h.add(FeatureKind::Extrude(e)).unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert_eq!(doc.bodies().len(), 2);
        let tops: Vec<FaceRef> = (0..2)
            .map(|b| {
                let f = doc.bodies()[b].topo.faces.iter().position(|f| {
                    f.plane
                        .is_some_and(|p| p.normal()[2] > 0.5 && (p.origin[2] - 30.0).abs() < 1e-6)
                });
                doc.face_ref(b, f.expect("top face")).unwrap()
            })
            .collect();
        let swapped: Vec<_> = doc.bodies().iter().rev().cloned().collect();
        for (b, r) in tops.iter().enumerate() {
            let (now, _) = crate::naming::resolve_face(&swapped, r).unwrap();
            assert_eq!(1 - now, b);
        }
    }

    #[test]
    fn a_huge_pattern_is_refused_instead_of_freezing_the_app() {
        let mut doc = PartDocument::new("Part");
        let e = cube(&mut doc, 10.0);
        let dir = |axis, count| PatternDir {
            axis: AxisRef::Std(axis),
            spacing: 20.0,
            count,
            reverse: false,
        };
        let p = doc.edit(|h| {
            h.add(FeatureKind::LinearPattern(LinearPattern {
                seeds: Seeds::Features(vec![e]),
                dir1: dir(0, 500),
                dir2: Some(dir(1, 500)),
                skip: vec![],
            }))
            .unwrap()
        });
        assert!(doc.errors()[&p].contains("at most"), "{:?}", doc.errors());
    }

    #[test]
    fn hole_wizard_counterbore_and_tapped() {
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 20.0);
        let top = top_face(&doc, 20.0);
        let mut pts = Sketch::default();
        pts.add_point(10.0, 10.0);
        let hole = doc.edit(|h| {
            let s = h.add_sketch_on(pts, Plane::TOP, PlaneRef::Face(top)).unwrap();
            h.add(FeatureKind::Hole(Hole {
                sketch: s,
                kind: HoleKind::Counterbore,
                standard: HoleStandard::Iso,
                size: "M6".into(),
                end: End::ThroughAll,
            }))
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let m6 = crate::holes::find(HoleStandard::Iso, "M6").unwrap();
        let expected = 8000.0
            - PI * (m6.clearance / 2.0).powi(2) * (20.0 - m6.cbore_depth)
            - PI * (m6.cbore_diameter / 2.0).powi(2) * m6.cbore_depth;
        assert!((vol(&doc) - expected).abs() < 3.0, "{} vs {expected}", vol(&doc));
        doc.edit(|h| {
            if let Some(FeatureKind::Hole(x)) = h.get_mut(hole).map(|f| &mut f.kind) {
                x.kind = HoleKind::Tapped;
                x.end = End::Blind(10.0);
            }
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert_eq!(doc.built().threads[&hole].designation, "M6x1");
        assert!((vol(&doc) - (8000.0 - PI * 6.25 * 10.0)).abs() < 2.0, "{}", vol(&doc));
    }

    #[test]
    fn linear_circular_pattern_and_mirror() {
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 40.0);
        let top = top_face(&doc, 40.0);
        let cut = doc.edit(|h| {
            let s = h
                .add_sketch_on(Sketch::circle([5.0, 5.0], 2.0), Plane::TOP, PlaneRef::Face(top))
                .unwrap();
            h.add(FeatureKind::Extrude(Extrude::cut(s, 5.0))).unwrap()
        });
        let hole_v = PI * 4.0 * 5.0;
        let lp = doc.edit(|h| {
            h.add(FeatureKind::LinearPattern(LinearPattern {
                seeds: Seeds::Features(vec![cut]),
                dir1: PatternDir {
                    axis: AxisRef::Std(0),
                    spacing: 10.0,
                    count: 3,
                    reverse: false,
                },
                dir2: Some(PatternDir {
                    axis: AxisRef::Std(1),
                    spacing: 10.0,
                    count: 2,
                    reverse: false,
                }),
                skip: vec![(2, 1)],
            }))
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?} {:?}", doc.errors(), doc.warnings());
        assert!((vol(&doc) - (64000.0 - 5.0 * hole_v)).abs() < 3.0, "{}", vol(&doc));
        // Swap for a circular pattern about a vertical axis through the cube's center.
        doc.edit(|h| {
            h.remove(lp);
            let pt = h
                .add(FeatureKind::RefPoint(RefPointDef::Coords([20.0, 20.0, 0.0])))
                .unwrap();
            let pt2 = h
                .add(FeatureKind::RefPoint(RefPointDef::Coords([20.0, 20.0, 40.0])))
                .unwrap();
            let ax2 = h
                .add(FeatureKind::RefAxis(RefAxisDef::TwoPoints(
                    PointRef::Feature(pt),
                    PointRef::Feature(pt2),
                )))
                .unwrap();
            h.add(FeatureKind::CircularPattern(CircularPattern {
                seeds: Seeds::Features(vec![cut]),
                axis: AxisRef::Feature(ax2),
                count: 4,
                angle: std::f64::consts::TAU,
                equal: true,
                reverse: false,
                skip: vec![2],
            }))
            .unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - (64000.0 - 3.0 * hole_v)).abs() < 3.0, "{}", vol(&doc));
        // Mirror the whole body about a plane at x = 50: two cubes.
        doc.edit(|h| {
            let p = h
                .add(FeatureKind::RefPlane(RefPlaneDef::Offset {
                    base: PlaneRef::Std(StdPlane::Right),
                    distance: 50.0,
                    flip: false,
                }))
                .unwrap();
            h.add(FeatureKind::Mirror(Mirror {
                seeds: Seeds::Bodies,
                plane: PlaneRef::Feature(p),
                merge: false,
            }))
            .unwrap();
        });
        assert_eq!(doc.bodies().len(), 2);
        assert!(
            (vol(&doc) - 2.0 * (64000.0 - 3.0 * hole_v)).abs() < 6.0,
            "{}",
            vol(&doc)
        );
        let (_, hi) = threebcad_kernel::boolean::bbox(&doc.bodies()[1].solid);
        assert!((hi[0] - 100.0).abs() < 1e-6);
    }

    /// A closed polygon as a sketch.
    fn polygon_sketch(points: &[[f64; 2]]) -> Sketch {
        let mut s = Sketch::default();
        let p: Vec<_> = points.iter().map(|q| s.add_point(q[0], q[1])).collect();
        for i in 0..p.len() {
            s.add_line(p[i], p[(i + 1) % p.len()]);
        }
        s
    }

    #[test]
    fn draft_finds_the_right_seed_of_a_mirror() {
        // A plate with a square and a hexagonal boss, both mirrored and merged into the plate.
        let mut doc = PartDocument::new("Part");
        let plate = polygon_sketch(&[[-40.0, -20.0], [40.0, -20.0], [40.0, 20.0], [-40.0, 20.0]]);
        let square = polygon_sketch(&[[10.0, -5.0], [20.0, -5.0], [20.0, 5.0], [10.0, 5.0]]);
        let hexagon: Vec<[f64; 2]> = (0..6)
            .map(|i| {
                let a = PI / 3.0 * i as f64;
                [30.0 + 5.0 * a.cos(), 5.0 * a.sin()]
            })
            .collect();
        let hexagon = polygon_sketch(&hexagon);
        let mirror = doc.edit(|h| {
            let s = h.add_sketch(plate, Plane::TOP);
            h.add_extrude(s, 5.0).unwrap();
            let s = h.add_sketch(square, Plane::TOP);
            let square = h.add_extrude(s, 10.0).unwrap();
            let s = h.add_sketch(hexagon, Plane::TOP);
            let hexagon = h.add_extrude(s, 10.0).unwrap();
            h.add(FeatureKind::Mirror(Mirror {
                seeds: Seeds::Features(vec![square, hexagon]),
                plane: PlaneRef::Std(StdPlane::Right),
                merge: true,
            }))
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let before = vol(&doc);
        // Wall 5 exists only on the hexagon; the first seed has four walls.
        let (body, face) = doc
            .bodies()
            .iter()
            .enumerate()
            .find_map(|(b, body)| {
                let wall = |t: &crate::naming::FaceTag| t.feature == mirror && t.face == ToolFace::Side(5);
                body.tags.iter().position(wall).map(|f| (b, f))
            })
            .expect("the mirrored hexagon's wall");
        let wall = doc.face_ref(body, face).unwrap();
        doc.edit(|h| {
            h.add(FeatureKind::Draft {
                neutral: PlaneRef::Std(StdPlane::Top),
                faces: vec![wall],
                angle: 0.1,
            })
            .unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - before).abs() > 1.0, "the wall did not move");
    }

    #[test]
    fn a_failed_feature_leaves_every_body_as_it_was() {
        // A hollow ring (its void refuses booleans) and a block to its right.
        let mut ring = Sketch::circle([10.0, 0.0], 3.0);
        let c = ring.add_point(10.0, 0.0);
        ring.add_circle(c, 2.0);
        let (a0, a1) = (ring.add_point(0.0, -5.0), ring.add_point(0.0, 5.0));
        let axis = ring.add_line(a0, a1);
        ring.set_construction(axis, true);
        let block = polygon_sketch(&[[20.0, -5.0], [30.0, -5.0], [30.0, 5.0], [20.0, 5.0]]);
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let s = h.add_sketch(ring, Plane::FRONT);
            h.add(FeatureKind::Revolve(Revolve::new(s, false))).unwrap();
            let s = h.add_sketch(block, Plane::TOP);
            h.add_extrude(s, 4.0).unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert_eq!(doc.bodies().len(), 2);
        let before = vol(&doc);
        // One slot through the block, one through the ring: the second tool fails.
        let cutter = |x0: f64, x1: f64| [[x0, -1.0], [x1, -1.0], [x1, 1.0], [x0, 1.0]];
        let mut cuts = polygon_sketch(&cutter(24.0, 26.0));
        let extra = cutter(9.0, 11.0).map(|q| cuts.add_point(q[0], q[1]));
        for i in 0..4 {
            cuts.add_line(extra[i], extra[(i + 1) % 4]);
        }
        let cut = doc.edit(|h| {
            let s = h.add_sketch(cuts, Plane::TOP);
            let mut e = Extrude::cut(s, 20.0);
            e.end = End::MidPlane(20.0);
            h.add(FeatureKind::Extrude(e)).unwrap()
        });
        assert!(doc.errors()[&cut].contains("void"), "{:?}", doc.errors());
        assert_eq!(doc.bodies().len(), 2);
        assert!((vol(&doc) - before).abs() < 1e-6, "{} vs {before}", vol(&doc));
    }

    #[test]
    fn suppress_and_incremental_rebuild() {
        let mut doc = PartDocument::demo();
        let first = doc.history().iter().next().unwrap().0;
        doc.edit(|h| h.set_suppressed(first, true));
        assert_eq!(doc.bodies().len(), 1, "the sketch and its extrude are out");
        assert!(doc.errors().is_empty());
        doc.undo();
        assert_eq!(doc.bodies().len(), 2);
        // Editing the last feature only re-evaluates that one.
        let last = doc.history().iter().last().unwrap().0;
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(last).map(|f| &mut f.kind) {
                e.end = End::Blind(41.0);
            }
        });
        assert_eq!(doc.built().evaluated, 1);
        doc.force_rebuild();
        assert_eq!(doc.built().evaluated, 4);
    }

    #[test]
    fn reference_planes_and_rib() {
        // L-bracket on the Front plane, 20 deep; a diagonal rib in the inner corner, mid-plane.
        let mut l = Sketch::default();
        let p = [
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 2.0],
            [2.0, 2.0],
            [2.0, 20.0],
            [0.0, 20.0],
        ]
        .map(|q| l.add_point(q[0], q[1]));
        for i in 0..6 {
            l.add_line(p[i], p[(i + 1) % 6]);
        }
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let s = h.add_sketch(l, Plane::FRONT);
            let mut e = Extrude::boss(s, 20.0);
            e.reverse = true; // grow towards +Y
            h.add(FeatureKind::Extrude(e)).unwrap();
        });
        let before = vol(&doc);
        let mid = doc.edit(|h| {
            h.add(FeatureKind::RefPlane(RefPlaneDef::Offset {
                base: PlaneRef::Std(StdPlane::Front),
                distance: -10.0,
                flip: false,
            }))
            .unwrap()
        });
        let plane = doc.plane_of(mid).unwrap();
        assert!(
            (plane.origin[1] - 10.0).abs() < 1e-9,
            "Front normal is -Y, so -10 moves to +Y"
        );
        let mut rib = Sketch::default();
        let (a, b) = (rib.add_point(2.0, 12.0), rib.add_point(12.0, 2.0));
        rib.add_line(a, b);
        doc.edit(|h| {
            let s = h.add_sketch_on(rib, plane, PlaneRef::Feature(mid)).unwrap();
            h.add(FeatureKind::Rib {
                sketch: s,
                thickness: 2.0,
                both_sides: true,
                flip: false,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        // The triangle between the line and the corner (legs 10), 2 thick.
        let added = vol(&doc) - before;
        assert!((added - 0.5 * 10.0 * 10.0 * 2.0).abs() < 0.5, "{added}");
    }

    #[test]
    fn a_rib_runs_on_to_the_nearest_wall_and_stops_at_the_floor() {
        // The same L; a level rib line that stops short of the upright and hangs over the open end.
        let mut l = Sketch::default();
        let p = [
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 2.0],
            [2.0, 2.0],
            [2.0, 20.0],
            [0.0, 20.0],
        ]
        .map(|q| l.add_point(q[0], q[1]));
        for i in 0..6 {
            l.add_line(p[i], p[(i + 1) % 6]);
        }
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let s = h.add_sketch(l, Plane::FRONT);
            let mut e = Extrude::boss(s, 20.0);
            e.reverse = true;
            h.add(FeatureKind::Extrude(e)).unwrap();
        });
        let before = vol(&doc);
        let mut rib = Sketch::default();
        let (a, b) = (rib.add_point(6.0, 8.0), rib.add_point(12.0, 8.0));
        rib.add_line(a, b);
        doc.edit(|h| {
            let mid = h
                .add(FeatureKind::RefPlane(RefPlaneDef::Offset {
                    base: PlaneRef::Std(StdPlane::Front),
                    distance: -10.0,
                    flip: false,
                }))
                .unwrap();
            let plane = Plane {
                origin: [0.0, 10.0, 0.0],
                ..Plane::FRONT
            };
            let s = h.add_sketch_on(rib, plane, PlaneRef::Feature(mid)).unwrap();
            h.add(FeatureKind::Rib {
                sketch: s,
                thickness: 2.0,
                both_sides: true,
                flip: false,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        // From the upright (x = 2) to the line's end (x = 12), from the floor (y = 2) up to 8.
        let added = vol(&doc) - before;
        assert!((added - 10.0 * 6.0 * 2.0).abs() < 0.5, "{added}");
        let (lo, hi) = threebcad_kernel::boolean::bbox(&doc.bodies()[0].solid);
        assert!(lo[0] > -1e-6 && hi[0] < 20.0 + 1e-6, "{lo:?} {hi:?}");
    }

    fn faces_where(doc: &PartDocument, keep: impl Fn(&threebcad_kernel::topology::TopoFace) -> bool) -> Vec<FaceRef> {
        let topo = &doc.bodies()[0].topo;
        (0..topo.faces.len())
            .filter(|&i| keep(&topo.faces[i]))
            .filter_map(|i| doc.face_ref(0, i))
            .collect()
    }

    /// Whether the first body has a cylindrical face of radius `r`.
    fn has_radius(doc: &PartDocument, r: f64) -> bool {
        doc.bodies()[0]
            .topo
            .faces
            .iter()
            .any(|f| f.cylinder.is_some_and(|c| (c.2 - r).abs() < 1e-6))
    }

    #[test]
    fn fit_grows_a_hole_and_moves_a_flat_face() {
        // A 20 mm cube with a 6 mm hole through it.
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 20.0);
        doc.edit(|h| {
            let s = h.add_sketch(Sketch::circle([10.0, 10.0], 3.0), Plane::TOP);
            let mut e = Extrude::cut(s, 20.0);
            e.end = End::ThroughAll;
            e.reverse = true;
            h.add(FeatureKind::Extrude(e)).unwrap();
        });
        let before = vol(&doc);
        // One of the hole's two half faces is enough.
        let hole = faces_where(&doc, |f| f.cylinder.is_some())[..1].to_vec();
        doc.edit(|h| {
            h.add(FeatureKind::Fit {
                faces: hole,
                clearance: 0.2,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!(has_radius(&doc, 3.2) && !has_radius(&doc, 3.0));
        let grown = PI * (3.2f64.powi(2) - 9.0) * 20.0;
        assert!(
            (before - vol(&doc) - grown).abs() < 0.02 * grown,
            "{} vs {grown}",
            before - vol(&doc)
        );

        // The top face, hole and all, moves 0.5 mm down; the hole wall is left alone.
        let before = vol(&doc);
        let top = top_face(&doc, 20.0);
        doc.edit(|h| {
            h.add(FeatureKind::Fit {
                faces: vec![top],
                clearance: 0.5,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let (_, hi) = threebcad_kernel::boolean::bbox(&doc.bodies()[0].solid);
        assert!((hi[2] - 19.5).abs() < 1e-2, "{hi:?}");
        let moved = (400.0 - PI * 3.2f64.powi(2)) * 0.5;
        assert!(
            (before - vol(&doc) - moved).abs() < 0.02 * moved,
            "{} vs {moved}",
            before - vol(&doc)
        );
    }

    #[test]
    fn fit_shrinks_a_pin_standing_on_a_plate() {
        // A 20 x 20 x 2 plate with a 6 mm pin rising to 12.
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let s = h.add_sketch(Sketch::rectangle(20.0, 20.0), Plane::TOP);
            h.add_extrude(s, 2.0).unwrap();
            let c = h.add_sketch(Sketch::circle([10.0, 10.0], 3.0), Plane::TOP);
            h.add_extrude(c, 12.0).unwrap();
        });
        assert_eq!(doc.bodies().len(), 1);
        let before = vol(&doc);
        let pin = faces_where(&doc, |f| f.cylinder.is_some());
        let fit = doc.edit(|h| {
            h.add(FeatureKind::Fit {
                faces: pin,
                clearance: 0.2,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!(has_radius(&doc, 2.8));
        let shrunk = PI * (9.0 - 2.8f64.powi(2)) * 10.0;
        assert!(
            (before - vol(&doc) - shrunk).abs() < 0.02 * shrunk,
            "{} vs {shrunk}",
            before - vol(&doc)
        );
        // A clearance as big as the pin is refused.
        doc.edit(|h| {
            if let Some(FeatureKind::Fit { clearance, .. }) = h.get_mut(fit).map(|f| &mut f.kind) {
                *clearance = 3.0;
            }
        });
        assert!(doc.errors()[&fit].contains("radius"), "{:?}", doc.errors());
    }

    #[test]
    fn fit_on_one_wall_leaves_the_coaxial_hole_in_the_other() {
        // A U bracket (two 10 mm walls) with one 4 mm hole along X through both walls.
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let u = [
                [0.0, 0.0],
                [30.0, 0.0],
                [30.0, 10.0],
                [20.0, 10.0],
                [20.0, 3.0],
                [10.0, 3.0],
                [10.0, 10.0],
                [0.0, 10.0],
            ];
            let s = h.add_sketch(polygon_sketch(&u), Plane::TOP);
            h.add_extrude(s, 10.0).unwrap();
            let c = h.add_sketch(Sketch::circle([6.5, 5.0], 2.0), Plane::RIGHT);
            let mut e = Extrude::cut(c, 10.0);
            e.end = End::ThroughAll;
            e.reverse = true;
            h.add(FeatureKind::Extrude(e)).unwrap();
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let before = vol(&doc);
        let left = faces_where(&doc, |f| {
            f.cylinder.is_some() && f.tris.iter().flatten().all(|p| p[0] < 10.0 + 1e-6)
        });
        assert!(!left.is_empty());
        doc.edit(|h| {
            h.add(FeatureKind::Fit {
                faces: left[..1].to_vec(),
                clearance: 0.2,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let one_wall = PI * (2.2f64.powi(2) - 4.0) * 10.0;
        assert!(
            (before - vol(&doc) - one_wall).abs() < 0.05 * one_wall,
            "{} vs {one_wall}",
            before - vol(&doc)
        );
        assert!(has_radius(&doc, 2.0) && has_radius(&doc, 2.2));
    }

    #[test]
    fn fit_shrinks_a_square_tab_on_all_four_sides() {
        // A 20 x 20 x 2 plate with a 4 x 4 tab rising to 10.
        let mut doc = PartDocument::new("Part");
        doc.edit(|h| {
            let s = h.add_sketch(Sketch::rectangle(20.0, 20.0), Plane::TOP);
            h.add_extrude(s, 2.0).unwrap();
            let t = h.add_sketch(
                polygon_sketch(&[[8.0, 8.0], [12.0, 8.0], [12.0, 12.0], [8.0, 12.0]]),
                Plane::TOP,
            );
            h.add_extrude(t, 10.0).unwrap();
        });
        let before = vol(&doc);
        let sides = faces_where(&doc, |f| {
            f.plane.is_some_and(|p| p.normal()[2].abs() < 1e-9) && f.tris.iter().flatten().all(|p| p[2] > 2.0 - 1e-9)
        });
        assert_eq!(sides.len(), 4);
        doc.edit(|h| {
            h.add(FeatureKind::Fit {
                faces: sides,
                clearance: 0.2,
            })
            .unwrap()
        });
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        let removed = (16.0 - 3.6 * 3.6) * 8.0;
        assert!(
            (before - vol(&doc) - removed).abs() < 1e-3,
            "{} vs {removed}",
            before - vol(&doc)
        );
    }

    fn x_size(doc: &PartDocument) -> f64 {
        let (lo, hi) = threebcad_kernel::boolean::bbox(&doc.bodies()[0].solid);
        hi[0] - lo[0]
    }

    #[test]
    fn equations_drive_sketch_and_feature_dimensions() {
        use crate::Equation;
        let mut doc = PartDocument::new("Part");
        cube(&mut doc, 10.0);
        let names: Vec<_> = doc
            .history()
            .dimensions()
            .into_iter()
            .map(|d| (d.name, d.label))
            .collect();
        assert!(names.contains(&("D1@Sketch1".into(), "Distance".into())), "{names:?}");
        assert!(
            names.contains(&("D1@Boss-Extrude1".into(), "Depth".into())),
            "{names:?}"
        );
        // Listed so that each uses one further down: they are solved in dependency order.
        let eqs = |wall: &str| {
            vec![
                Equation::new("\"D2@Sketch1\"", "\"D1@Sketch1\" / 2"),
                Equation::new("D1@Sketch1", "wall * 10"),
                Equation::new("D1@Boss-Extrude1", "5 mm"),
                Equation::new("wall", wall),
            ]
        };
        doc.edit(|h| h.set_equations(eqs("2")));
        assert_eq!(doc.equation_results(), [Ok(10.0), Ok(20.0), Ok(5.0), Ok(2.0)]);
        assert!(doc.errors().is_empty(), "{:?}", doc.errors());
        assert!((vol(&doc) - 20.0 * 10.0 * 5.0).abs() < 1e-3, "{}", vol(&doc));
        doc.edit(|h| h.set_equations(eqs("3")));
        assert!((vol(&doc) - 30.0 * 15.0 * 5.0).abs() < 1e-3, "{}", vol(&doc));
        // Saved and read back, the equations still drive the part.
        let mut back = PartDocument::from_json(&doc.to_json()).unwrap();
        assert_eq!(back.history().equations(), doc.history().equations());
        assert!((x_size(&back) - 30.0).abs() < 1e-6);
        // In inches `wall = 3` is 3 in, so the sketch is 30 in wide; `5 mm` stays 5 mm.
        back.set_units(crate::Units {
            system: crate::UnitSystem::Ips,
            decimals: 3,
        });
        assert!((x_size(&back) - 762.0).abs() < 1e-6, "{}", x_size(&back));
        let (lo, hi) = threebcad_kernel::boolean::bbox(&back.bodies()[0].solid);
        assert!((hi[2] - lo[2] - 5.0).abs() < 1e-6);
        // A cycle is an error on its equation and leaves the geometry as it was.
        let mut cyclic = eqs("3");
        cyclic[2].expr = "\"D1@Boss-Extrude1\" + 1".into();
        doc.edit(|h| h.set_equations(cyclic));
        assert!(doc.equation_results()[2].as_ref().unwrap_err().contains("circular"));
        assert!((vol(&doc) - 30.0 * 15.0 * 5.0).abs() < 1e-3, "{}", vol(&doc));
        // A deleted dimension is reported, not taken for a variable.
        doc.edit(|h| h.set_equations(vec![Equation::new("D9@Sketch1", "4")]));
        assert!(doc.equation_results()[0].as_ref().unwrap_err().contains("no dimension"));
    }

    #[test]
    fn configurations_switch_dimensions_and_suppression() {
        use crate::Equation;
        let mut doc = PartDocument::demo();
        let cube_sketch = doc.history().iter().next().unwrap().0;
        let cylinder = doc.history().iter().nth(3).unwrap().0;
        let cube_v = 27000.0;
        let cyl = |h: f64| PI * 100.0 * h;
        let close = |doc: &PartDocument, v: f64| (vol(doc) - v).abs() < 0.01 * v;
        assert!(close(&doc, cube_v + cyl(40.0)), "{}", vol(&doc));
        // "Short": the cylinder 10 high and no cube.
        let short = doc.edit(|h| {
            let i = h.add_configuration("Short");
            h.activate(i);
            h.configuration_mut(i).unwrap().equations = vec![Equation::new("D1@Boss-Extrude2", "10")];
            h.set_suppressed(cube_sketch, true);
            i
        });
        assert_eq!(doc.history().configuration_name(), "Short");
        assert_eq!(doc.bodies().len(), 1);
        assert!(close(&doc, cyl(10.0)), "{}", vol(&doc));
        // Back to Default: the cube and the 40 mm cylinder are back.
        doc.activate(0);
        assert!(!doc.history().get(cube_sketch).unwrap().suppressed);
        assert!(close(&doc, cube_v + cyl(40.0)), "{}", vol(&doc));
        // Suppressing in Default leaves Short alone.
        doc.edit(|h| h.set_suppressed(cylinder, true));
        assert!(close(&doc, cube_v), "{}", vol(&doc));
        let exported = doc.in_configuration(short).unwrap();
        assert!(close(&exported, cyl(10.0)), "{}", vol(&exported));
        assert!(close(&doc, cube_v), "the open part keeps its configuration");
        // Saved with Short active, it opens in Short.
        doc.activate(short);
        let back = PartDocument::from_json(&doc.to_json()).unwrap();
        assert_eq!(back.history().configurations().len(), 2);
        assert_eq!(back.history().configuration_name(), "Short");
        assert!(close(&back, cyl(10.0)), "{}", vol(&back));
        // A deleted feature is forgotten by every configuration, so a new one is not suppressed.
        doc.activate(0);
        doc.edit(|h| h.remove(cube_sketch));
        let s = doc.edit(|h| h.add_sketch(Sketch::rectangle(5.0, 5.0), Plane::TOP));
        doc.activate(short);
        assert!(!doc.history().get(s).unwrap().suppressed);
        // Deleting the active configuration falls back to another one.
        doc.edit(|h| h.remove_configuration(short));
        assert_eq!(doc.history().configuration_name(), "Default");
    }

    #[test]
    fn appearances_follow_their_faces_through_rebuilds() {
        let mut doc = PartDocument::new("Part");
        let extrude = cube(&mut doc, 10.0);
        let (red, blue, green) = ([255, 0, 0], [0, 0, 255], [0, 200, 0]);
        let top = top_face(&doc, 10.0);
        doc.set_appearance(Paint::Face(top.clone()), Some(red), None);
        doc.set_appearance(Paint::Feature(extrude), Some(blue), None);
        let colored_z = |doc: &PartDocument, c: [u8; 3]| -> Vec<f64> {
            let b = &doc.bodies()[0];
            doc.face_colors(0)
                .iter()
                .zip(&b.topo.faces)
                .filter(|(k, _)| **k == Some(c))
                .map(|(_, f)| f.plane.map_or(f64::NAN, |p| p.origin[2]))
                .collect()
        };
        assert_eq!(colored_z(&doc, red), [10.0]);
        assert_eq!(colored_z(&doc, blue).len(), 5);
        // A taller cube: the colour stays on the (moved) top face.
        doc.edit(|h| {
            if let Some(FeatureKind::Extrude(e)) = h.get_mut(extrude).map(|f| &mut f.kind) {
                e.end = End::Blind(15.0);
            }
        });
        assert_eq!(colored_z(&doc, red), [15.0]);
        // A body colour beats the feature's; a new face colour replaces the old one.
        doc.set_appearance(Paint::Body(top.clone()), Some(green), None);
        assert_eq!(colored_z(&doc, green).len(), 5);
        doc.set_appearance(Paint::Face(top.clone()), Some(blue), None);
        assert_eq!(colored_z(&doc, blue), [15.0]);
        assert_eq!(doc.history().appearances().len(), 3);
        let back = PartDocument::from_json(&doc.to_json()).unwrap();
        assert_eq!(back.face_colors(0), doc.face_colors(0));
        doc.set_appearance(Paint::Face(top), None, None);
        assert_eq!(colored_z(&doc, green).len(), 6);
        // Undo brings the face colour back.
        doc.undo();
        assert_eq!(colored_z(&doc, blue), [15.0]);
    }
}
