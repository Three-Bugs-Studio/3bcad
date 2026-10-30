use std::collections::HashMap;

use threebcad_kernel::topology::P3;
use threebcad_kernel::Plane;
use threebcad_sketch::{Sketch, Status};

use crate::naming::{self, EdgeRef, FaceRef, VertexRef};
use crate::rebuild::{self, CacheEntry};
use crate::{Body, FeatureId, History, Rebuild};

const UNDO_LIMIT: usize = 100;

/// A part: its feature history, the bodies it builds, and undo/redo.
// shortcut: assemblies and save/load are not modelled yet.
pub struct PartDocument {
    pub name: String,
    history: History,
    undo: Vec<History>,
    redo: Vec<History>,
    merge_key: Option<u64>,
    built: Rebuild,
    cache: Vec<CacheEntry>,
    revision: u64,
}

impl PartDocument {
    pub fn new(name: impl Into<String>) -> Self {
        let mut doc = Self {
            name: name.into(),
            history: History::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            merge_key: None,
            built: Rebuild::default(),
            cache: Vec::new(),
            revision: 0,
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
        doc
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
        let r = f(&mut self.history);
        self.history.relink();
        self.rebuild();
        r
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
            self.rebuild();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.history, next));
            self.merge_key = None;
            self.rebuild();
        }
    }

    /// Rebuilds every feature from scratch (Ctrl+Q), ignoring cached results.
    pub fn force_rebuild(&mut self) {
        self.cache.clear();
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.built = rebuild::rebuild(&mut self.history, &mut self.cache);
        self.revision += 1;
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
        doc.undo();
        assert!(!doc.can_undo());
        assert!(
            matches!(&doc.history().get(extrude).unwrap().kind, FeatureKind::Extrude(e) if e.end == End::Blind(30.0))
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
}
