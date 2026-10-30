//! Reference parts for the regression test (`tests/parts.rs`) and the benchmarks. Each one
//! exercises a mix of features the way a user would build them.

use std::f64::consts::TAU;

use threebcad_core::*;
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

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

fn block(doc: &mut PartDocument, w: f64, d: f64, h: f64) -> FeatureId {
    doc.edit(|hist| {
        let s = hist.add_sketch(Sketch::rectangle(w, d), Plane::TOP);
        hist.add_extrude(s, h).unwrap()
    })
}

/// 60 x 40 x 5 plate, a Ø6 cut patterned 3 x 2, an M4 counterbore in the middle.
pub fn plate_holes() -> PartDocument {
    let mut doc = PartDocument::new("plate_holes");
    block(&mut doc, 60.0, 40.0, 5.0);
    let top = top_face(&doc, 5.0);
    doc.edit(|h| {
        let s = h
            .add_sketch_on(
                Sketch::circle([10.0, 10.0], 3.0),
                Plane::TOP,
                PlaneRef::Face(top.clone()),
            )
            .unwrap();
        let mut cut = Extrude::cut(s, 0.0);
        cut.end = End::ThroughAll;
        let c = h.add(FeatureKind::Extrude(cut)).unwrap();
        h.add(FeatureKind::LinearPattern(LinearPattern {
            seeds: Seeds::Features(vec![c]),
            dir1: PatternDir {
                axis: AxisRef::Std(0),
                spacing: 20.0,
                count: 3,
                reverse: false,
            },
            dir2: Some(PatternDir {
                axis: AxisRef::Std(1),
                spacing: 20.0,
                count: 2,
                reverse: false,
            }),
            skip: vec![],
        }))
        .unwrap();
        let mut pts = Sketch::default();
        pts.add_point(30.0, 20.0);
        let ps = h.add_sketch_on(pts, Plane::TOP, PlaneRef::Face(top)).unwrap();
        h.add(FeatureKind::Hole(Hole {
            sketch: ps,
            kind: HoleKind::Counterbore,
            standard: HoleStandard::Iso,
            size: "M4".into(),
            end: End::ThroughAll,
        }))
        .unwrap();
    });
    doc
}

/// L-bracket on the Front plane with a chamfered outer edge and a rib in the inner corner.
pub fn bracket() -> PartDocument {
    let mut l = Sketch::default();
    let p = [
        [0.0, 0.0],
        [30.0, 0.0],
        [30.0, 4.0],
        [4.0, 4.0],
        [4.0, 30.0],
        [0.0, 30.0],
    ]
    .map(|q| l.add_point(q[0], q[1]));
    for i in 0..6 {
        l.add_line(p[i], p[(i + 1) % 6]);
    }
    let mut doc = PartDocument::new("bracket");
    doc.edit(|h| {
        let s = h.add_sketch(l, Plane::FRONT);
        let mut e = Extrude::boss(s, 20.0);
        e.reverse = true;
        h.add(FeatureKind::Extrude(e)).unwrap();
    });
    // The outer corner edge along Y at x = 30, z = 0.
    let b = &doc.bodies()[0];
    let e = b
        .topo
        .edges
        .iter()
        .position(|e| {
            matches!(e.curve, threebcad_kernel::topology::EdgeCurve::Line { a, b }
                if (a[0] - 30.0).abs() < 1e-9 && (b[0] - 30.0).abs() < 1e-9 && a[2].abs() < 1e-9 && b[2].abs() < 1e-9)
        })
        .unwrap();
    let edge = doc.edge_ref(0, e).unwrap();
    let mut rib = Sketch::default();
    let (a, b) = (rib.add_point(4.0, 16.0), rib.add_point(16.0, 4.0));
    rib.add_line(a, b);
    doc.edit(|h| {
        h.add(FeatureKind::Chamfer {
            edges: vec![edge],
            kind: ChamferKind::EqualDistance,
            d1: 2.0,
            d2: 2.0,
            angle: 0.0,
            flip: false,
        })
        .unwrap();
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
            thickness: 3.0,
            both_sides: true,
            flip: false,
        })
        .unwrap();
    });
    doc
}

/// 40 x 30 x 20 box shelled 2 mm with the top removed.
pub fn shelled_box() -> PartDocument {
    let mut doc = PartDocument::new("shelled_box");
    block(&mut doc, 40.0, 30.0, 20.0);
    let top = top_face(&doc, 20.0);
    doc.edit(|h| {
        h.add(FeatureKind::Shell {
            faces: vec![top],
            thickness: 2.0,
            outward: false,
        })
        .unwrap()
    });
    doc
}

/// A revolved stepped flange with four holes on a bolt circle.
pub fn flange() -> PartDocument {
    let mut s = Sketch::default();
    let pts = [
        [5.0, 0.0],
        [30.0, 0.0],
        [30.0, 6.0],
        [12.0, 6.0],
        [12.0, 20.0],
        [5.0, 20.0],
    ]
    .map(|q| s.add_point(q[0], q[1]));
    for i in 0..6 {
        s.add_line(pts[i], pts[(i + 1) % 6]);
    }
    let (c0, c1) = (s.add_point(0.0, 0.0), s.add_point(0.0, 20.0));
    let axis = s.add_line(c0, c1);
    s.set_construction(axis, true);
    let mut doc = PartDocument::new("flange");
    doc.edit(|h| {
        let sk = h.add_sketch(s, Plane::FRONT);
        h.add(FeatureKind::Revolve(Revolve::new(sk, false))).unwrap();
        let hole = h.add_sketch(Sketch::circle([21.0, 0.0], 2.5), Plane::TOP);
        let mut cut = Extrude::cut(hole, 0.0);
        cut.reverse = true;
        cut.end = End::Blind(6.0);
        let c = h.add(FeatureKind::Extrude(cut)).unwrap();
        h.add(FeatureKind::CircularPattern(CircularPattern {
            seeds: Seeds::Features(vec![c]),
            axis: AxisRef::Std(2),
            count: 4,
            angle: TAU,
            equal: true,
            reverse: false,
            skip: vec![],
        }))
        .unwrap();
    });
    doc
}

/// A block with a boss on its top face, mirrored as a separate body.
pub fn mirrored() -> PartDocument {
    let mut doc = PartDocument::new("mirrored");
    block(&mut doc, 20.0, 20.0, 10.0);
    let top = top_face(&doc, 10.0);
    doc.edit(|h| {
        let s = h
            .add_sketch_on(Sketch::circle([10.0, 10.0], 4.0), Plane::TOP, PlaneRef::Face(top))
            .unwrap();
        h.add_extrude(s, 8.0).unwrap();
        let p = h
            .add(FeatureKind::RefPlane(RefPlaneDef::Offset {
                base: PlaneRef::Std(StdPlane::Right),
                distance: 30.0,
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
    doc
}

/// A named way to build a reference part.
pub type Builder = (&'static str, fn() -> PartDocument);

pub fn all() -> Vec<Builder> {
    vec![
        ("plate_holes", plate_holes as fn() -> PartDocument),
        ("bracket", bracket),
        ("shelled_box", shelled_box),
        ("flange", flange),
        ("mirrored", mirrored),
    ]
}
