//! The v0.4 features, end to end through a part document. Each test checks a physical property
//! (volume, extent, body count) against a hand calculation.

use std::f64::consts::PI;

use threebcad_core::*;
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::print_mesh;
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

fn vol(doc: &PartDocument) -> f64 {
    let meshes: Vec<_> = doc.bodies().iter().map(|b| print_mesh(&b.solid, 0.005)).collect();
    mass_properties(&meshes).volume
}

/// Threads are hundreds of helical faces: a coarser mesh keeps the check quick.
fn rough_vol(doc: &PartDocument) -> f64 {
    let meshes: Vec<_> = doc.bodies().iter().map(|b| print_mesh(&b.solid, 0.05)).collect();
    mass_properties(&meshes).volume
}

fn body_vol(doc: &PartDocument, i: usize) -> f64 {
    mass_properties(&[print_mesh(&doc.bodies()[i].solid, 0.005)]).volume
}

fn bbox(doc: &PartDocument) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for b in doc.bodies() {
        let (a, c) = threebcad_kernel::boolean::bbox(&b.solid);
        for i in 0..3 {
            lo[i] = lo[i].min(a[i]);
            hi[i] = hi[i].max(c[i]);
        }
    }
    (lo, hi)
}

fn ok(doc: &PartDocument) {
    assert!(doc.errors().is_empty(), "{:?}", doc.errors());
}

fn near(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1.0)
}

fn polygon(points: &[[f64; 2]]) -> Sketch {
    let mut s = Sketch::default();
    let ids: Vec<_> = points.iter().map(|p| s.add_point(p[0], p[1])).collect();
    for i in 0..ids.len() {
        s.add_line(ids[i], ids[(i + 1) % ids.len()]);
    }
    s
}

fn square(c: [f64; 2], half: f64) -> Sketch {
    polygon(&[
        [c[0] - half, c[1] - half],
        [c[0] + half, c[1] - half],
        [c[0] + half, c[1] + half],
        [c[0] - half, c[1] + half],
    ])
}

/// An open polyline.
fn chain(points: &[[f64; 2]]) -> Sketch {
    let mut s = Sketch::default();
    let ids: Vec<_> = points.iter().map(|p| s.add_point(p[0], p[1])).collect();
    for w in ids.windows(2) {
        s.add_line(w[0], w[1]);
    }
    s
}

fn lifted(z: f64) -> Plane {
    Plane {
        origin: [0.0, 0.0, z],
        ..Plane::TOP
    }
}

/// A block centred on the origin in x and y, from z = 0 up to `h`.
fn block(doc: &mut PartDocument, half: f64, h: f64) -> FeatureId {
    doc.edit(|hist| {
        let s = hist.add_sketch(square([0.0, 0.0], half), Plane::TOP);
        hist.add_extrude(s, h).unwrap()
    })
}

fn rod(doc: &mut PartDocument, r: f64, h: f64) -> FeatureId {
    doc.edit(|hist| {
        let s = hist.add_sketch(Sketch::circle([0.0, 0.0], r), Plane::TOP);
        hist.add_extrude(s, h).unwrap()
    })
}

fn face(doc: &PartDocument, body: usize, pred: impl Fn(&threebcad_kernel::topology::TopoFace) -> bool) -> FaceRef {
    let f = doc.bodies()[body].topo.faces.iter().position(pred).expect("face");
    doc.face_ref(body, f).unwrap()
}

fn flat(doc: &PartDocument, body: usize, normal: [f64; 3], at: f64) -> FaceRef {
    face(doc, body, |f| {
        f.plane.is_some_and(|p| {
            let n = p.normal();
            let d = n[0] * normal[0] + n[1] * normal[1] + n[2] * normal[2];
            let o = p.origin[0] * normal[0] + p.origin[1] * normal[1] + p.origin[2] * normal[2];
            d > 0.999 && (o - at).abs() < 1e-6
        })
    })
}

fn round(doc: &PartDocument, body: usize, r: f64) -> FaceRef {
    face(doc, body, |f| f.cylinder.is_some_and(|c| (c.2 - r).abs() < 1e-3))
}

#[test]
fn a_circle_swept_along_a_bent_path_is_a_pipe() {
    let mut doc = PartDocument::new("Part");
    // Up 10, a quarter turn of radius 10, then 10 along x (on the front plane: x, z).
    let mut s = Sketch::default();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 10.0);
    let c = s.add_point(10.0, 10.0);
    let d = s.add_point(10.0, 20.0);
    let e = s.add_point(20.0, 20.0);
    s.add_line(a, b);
    s.add_arc(c, d, b);
    s.add_line(d, e);
    doc.edit(|h| {
        let p = h.add_sketch(s, Plane::FRONT);
        h.add(FeatureKind::Sweep(Sweep {
            profile: SweepProfile::Circle(4.0),
            path: CurveRef::Sketch(p),
            twist: 0.0,
            cut: false,
            merge: true,
        }))
        .unwrap();
    });
    ok(&doc);
    let length = 20.0 + PI / 2.0 * 10.0;
    let expected = PI * 4.0 * length;
    assert!(near(vol(&doc), expected, 0.01), "{} vs {expected}", vol(&doc));
    let (lo, hi) = bbox(&doc);
    assert!(
        near(hi[0], 20.0, 1e-3) && near(hi[2], 22.0, 1e-3) && near(lo[2], 0.0, 1e-3),
        "{lo:?} {hi:?}"
    );
}

#[test]
fn a_spring_is_a_circle_swept_along_a_helix() {
    let mut doc = PartDocument::new("Part");
    let helix = doc.edit(|h| {
        let s = h.add_sketch(Sketch::circle([0.0, 0.0], 10.0), Plane::TOP);
        h.add(FeatureKind::Helix(HelixDef {
            sketch: s,
            size: HelixSize::PitchTurns { pitch: 5.0, turns: 3.0 },
            taper: 0.0,
            ccw: true,
            start: 0.0,
            reverse: false,
        }))
        .unwrap()
    });
    ok(&doc);
    let curve = &doc.built().curves[&helix];
    let top = curve.last().unwrap();
    assert!(
        near(top[2], 15.0, 1e-6) && near(top[0].hypot(top[1]), 10.0, 1e-6),
        "{top:?}"
    );
    doc.edit(|h| {
        h.add(FeatureKind::Sweep(Sweep {
            profile: SweepProfile::Circle(2.0),
            path: CurveRef::Helix(helix),
            twist: 0.0,
            cut: false,
            merge: true,
        }))
        .unwrap();
    });
    ok(&doc);
    // Pappus along the helix: the wire's area times the helix length.
    let length = 3.0 * (2.0 * PI * 10.0f64).hypot(5.0);
    let expected = PI * length;
    assert!(near(vol(&doc), expected, 0.02), "{} vs {expected}", vol(&doc));
    assert_eq!(doc.bodies().len(), 1);
}

#[test]
fn a_tapered_helix_and_a_spiral_change_their_radius() {
    let mut doc = PartDocument::new("Part");
    let (cone, spiral) = doc.edit(|h| {
        let s = h.add_sketch(Sketch::circle([0.0, 0.0], 10.0), Plane::TOP);
        let cone = h
            .add(FeatureKind::Helix(HelixDef {
                sketch: s,
                size: HelixSize::HeightPitch {
                    height: 20.0,
                    pitch: 4.0,
                },
                taper: -10f64.to_radians(),
                ccw: false,
                start: 0.0,
                reverse: false,
            }))
            .unwrap();
        let spiral = h
            .add(FeatureKind::Helix(HelixDef {
                sketch: s,
                size: HelixSize::Spiral { pitch: 2.0, turns: 3.0 },
                taper: 0.0,
                ccw: true,
                start: 0.0,
                reverse: false,
            }))
            .unwrap();
        (cone, spiral)
    });
    ok(&doc);
    let end = *doc.built().curves[&cone].last().unwrap();
    let r_end = 10.0 - 20.0 * 10f64.to_radians().tan();
    assert!(
        near(end[2], 20.0, 1e-6) && near(end[0].hypot(end[1]), r_end, 1e-3),
        "{end:?}"
    );
    let end = *doc.built().curves[&spiral].last().unwrap();
    assert!(end[2].abs() < 1e-9 && near(end[0].hypot(end[1]), 16.0, 1e-3), "{end:?}");
}

#[test]
fn a_loft_between_two_squares_is_a_frustum() {
    let mut doc = PartDocument::new("Part");
    doc.edit(|h| {
        let a = h.add_sketch(square([0.0, 0.0], 5.0), Plane::TOP);
        let b = h.add_sketch(square([0.0, 0.0], 2.0), lifted(10.0));
        h.add(FeatureKind::Loft(Loft {
            profiles: vec![a, b],
            guides: Vec::new(),
            start_tangent: 0.0,
            end_tangent: 0.0,
            cut: false,
            merge: true,
        }))
        .unwrap();
    });
    ok(&doc);
    let expected = 10.0 / 3.0 * (100.0 + 16.0 + 40.0);
    assert!(near(vol(&doc), expected, 0.005), "{} vs {expected}", vol(&doc));
}

/// A loft that starts on a face of the part merges into it (the cap would be coplanar with the face).
#[test]
fn a_loft_from_a_face_merges_into_the_part() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    doc.edit(|h| {
        let a = h.add_sketch(Sketch::circle([0.0, 0.0], 3.0), lifted(10.0));
        let b = h.add_sketch(Sketch::circle([0.0, 0.0], 2.0), lifted(20.0));
        h.add(FeatureKind::Loft(Loft {
            profiles: vec![a, b],
            guides: Vec::new(),
            start_tangent: 0.0,
            end_tangent: 0.0,
            cut: false,
            merge: true,
        }))
        .unwrap();
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1);
    // The block and a 10 mm cone frustum from r 3 to r 2.
    let expected = 1000.0 + PI * 10.0 / 3.0 * (9.0 + 6.0 + 4.0);
    assert!(near(vol(&doc), expected, 0.01), "{} vs {expected}", vol(&doc));
}

#[test]
fn a_loft_through_three_profiles_from_square_to_circle() {
    let mut doc = PartDocument::new("Part");
    doc.edit(|h| {
        let a = h.add_sketch(square([0.0, 0.0], 5.0), Plane::TOP);
        let b = h.add_sketch(Sketch::circle([0.0, 0.0], 4.0), lifted(10.0));
        let c = h.add_sketch(Sketch::circle([0.0, 0.0], 2.0), lifted(20.0));
        h.add(FeatureKind::Loft(Loft {
            profiles: vec![a, b, c],
            guides: Vec::new(),
            start_tangent: 1.0,
            end_tangent: 0.0,
            cut: false,
            merge: true,
        }))
        .unwrap();
    });
    ok(&doc);
    let (lo, hi) = bbox(&doc);
    assert!(
        near(lo[2], 0.0, 1e-3) && near(hi[2], 20.0, 1e-3) && near(hi[0], 5.0, 0.02),
        "{lo:?} {hi:?}"
    );
    // Above the inscribed cone stack; the start tangency bulges it past the circumscribed one a little.
    let v = vol(&doc);
    let inner = 10.0 / 3.0 * PI * (16.0 + 8.0 + 4.0) + 10.0 / 3.0 * PI * (16.0 + 4.0 + 8.0);
    let outer = 10.0 / 3.0 * (100.0 + 64.0 + 80.0) + 10.0 / 3.0 * PI * (16.0 + 4.0 + 8.0);
    assert!(v > inner && v < 1.1 * outer, "{v}");
}

#[test]
fn an_external_thread_on_a_rod_and_an_internal_one_in_a_hole() {
    // M10 on a 10 mm rod, 20 long.
    let mut doc = PartDocument::new("Part");
    rod(&mut doc, 5.0, 20.0);
    let side = round(&doc, 0, 5.0);
    let before = rough_vol(&doc);
    let t = doc.edit(|h| {
        h.add(FeatureKind::Thread(ThreadDef {
            face: side,
            standard: HoleStandard::Iso,
            size: "M10".into(),
            length: None,
            offset: 0.0,
            flip: false,
            right: true,
        }))
        .unwrap()
    });
    ok(&doc);
    let after = rough_vol(&doc);
    // Material between the major and minor diameters: about half of that ring is cut away.
    let minor = 5.0 - 5.0 * 3f64.sqrt() / 16.0 * 1.5;
    let ring = PI * (25.0 - minor * minor) * 20.0;
    let removed = before - after;
    assert!(removed > 0.3 * ring && removed < 0.7 * ring, "{removed} of {ring}");
    assert!(doc.built().threads.contains_key(&t));
    assert_eq!(doc.bodies().len(), 1);

    // An 8.5 mm tap-drill hole through a block, tapped M10.
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 10.0, 10.0);
    doc.edit(|h| {
        let s = h.add_sketch(Sketch::circle([0.0, 0.0], 4.25), lifted(10.0));
        let mut e = Extrude::cut(s, 10.0);
        e.end = End::ThroughAll;
        h.add(FeatureKind::Extrude(e)).unwrap();
    });
    ok(&doc);
    let hole = round(&doc, 0, 4.25);
    let before = rough_vol(&doc);
    doc.edit(|h| {
        h.add(FeatureKind::Thread(ThreadDef {
            face: hole,
            standard: HoleStandard::Iso,
            size: "M10".into(),
            length: None,
            offset: 0.0,
            flip: false,
            right: true,
        }))
        .unwrap()
    });
    ok(&doc);
    let removed = before - rough_vol(&doc);
    let ring = PI * (25.0 - 4.25 * 4.25) * 10.0;
    assert!(removed > 0.2 * ring && removed < 0.8 * ring, "{removed} of {ring}");
}

#[test]
fn a_thread_that_does_not_fit_says_why() {
    let mut doc = PartDocument::new("Part");
    rod(&mut doc, 3.0, 10.0);
    let side = round(&doc, 0, 3.0);
    let t = doc.edit(|h| {
        h.add(FeatureKind::Thread(ThreadDef {
            face: side,
            standard: HoleStandard::Iso,
            size: "M10".into(),
            length: None,
            offset: 0.0,
            flip: false,
            right: true,
        }))
        .unwrap()
    });
    assert!(doc.errors()[&t].contains("thinner"), "{:?}", doc.errors());
}

#[test]
fn a_variable_fillet_grows_along_its_edge() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    let before = vol(&doc);
    let b = &doc.bodies()[0];
    // The top edge along y at x = 5.
    let e = b
        .topo
        .edges
        .iter()
        .position(|e| {
            matches!(e.curve, threebcad_kernel::topology::EdgeCurve::Line { a, b }
                if (a[0] - 5.0).abs() < 1e-9 && (b[0] - 5.0).abs() < 1e-9 && a[2] > 9.0 && b[2] > 9.0)
        })
        .unwrap();
    let edge = doc.edge_ref(0, e).unwrap();
    doc.edit(|h| {
        h.add(FeatureKind::VariableFillet {
            edge,
            radii: vec![[0.0, 1.0], [1.0, 3.0]],
        })
        .unwrap()
    });
    ok(&doc);
    // (1 − π/4) ∫ r² along the edge, r linear from 1 to 3.
    let expected = (1.0 - PI / 4.0) * 10.0 * (1.0 + 3.0 + 9.0) / 3.0;
    let removed = before - vol(&doc);
    assert!(near(removed, expected, 0.03), "{removed} vs {expected}");
}

/// truck often fails on the skinned tool; the rebuild tries several builds of it. Every edge of
/// a cube, with a peak in the middle, must go through.
#[test]
fn a_variable_fillet_works_on_every_edge_of_a_cube() {
    let make = || {
        let mut doc = PartDocument::new("Part");
        block(&mut doc, 15.0, 30.0);
        doc
    };
    let before = vol(&make());
    let edges = make().bodies()[0].topo.edges.len();
    assert_eq!(edges, 12);
    for e in 0..edges {
        let mut doc = make();
        let edge = doc.edge_ref(0, e).unwrap();
        doc.edit(|h| {
            h.add(FeatureKind::VariableFillet {
                edge,
                radii: vec![[0.0, 1.0], [0.5, 3.0], [1.0, 1.0]],
            })
            .unwrap()
        });
        ok(&doc);
        // Between the 1 mm and 3 mm cross-sections' worth of material, (1 − π/4)·r²·30.
        let removed = before - vol(&doc);
        let k = (1.0 - PI / 4.0) * 30.0;
        assert!(removed > k * 1.0 && removed < k * 9.0, "edge {e}: removed {removed}");
    }
}

#[test]
fn a_face_fillet_rounds_where_two_faces_meet() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    let before = vol(&doc);
    let top = flat(&doc, 0, [0.0, 0.0, 1.0], 10.0);
    let side = flat(&doc, 0, [1.0, 0.0, 0.0], 5.0);
    doc.edit(|h| {
        h.add(FeatureKind::FaceFillet {
            a: vec![top],
            b: vec![side],
            radius: 2.0,
        })
        .unwrap()
    });
    ok(&doc);
    let expected = (1.0 - PI / 4.0) * 4.0 * 10.0;
    assert!(near(before - vol(&doc), expected, 0.01), "{}", before - vol(&doc));
}

/// Two 10 mm blocks overlapping by 5 in x, kept as separate bodies.
fn two_blocks() -> PartDocument {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    doc.edit(|h| {
        let s = h.add_sketch(square([5.0, 0.0], 5.0), Plane::TOP);
        let mut e = Extrude::boss(s, 10.0);
        e.merge = false;
        h.add(FeatureKind::Extrude(e)).unwrap();
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 2);
    doc
}

fn combine(op: CombineOp) -> PartDocument {
    let mut doc = two_blocks();
    let main = flat(&doc, 0, [-1.0, 0.0, 0.0], 5.0);
    let tool = flat(&doc, 1, [1.0, 0.0, 0.0], 10.0);
    doc.edit(|h| {
        h.add(FeatureKind::Combine {
            op,
            main,
            tools: vec![tool],
        })
        .unwrap()
    });
    ok(&doc);
    doc
}

#[test]
fn combine_adds_subtracts_and_keeps_the_common_part() {
    let doc = combine(CombineOp::Add);
    assert_eq!(doc.bodies().len(), 1);
    assert!(near(vol(&doc), 1500.0, 1e-3), "{}", vol(&doc));
    let doc = combine(CombineOp::Subtract);
    assert_eq!(doc.bodies().len(), 1);
    assert!(near(vol(&doc), 500.0, 1e-3), "{}", vol(&doc));
    let (_, hi) = bbox(&doc);
    // Booleans leave the cut face within a few microns.
    assert!(hi[0].abs() < 0.01, "{hi:?}");
    let doc = combine(CombineOp::Common);
    assert_eq!(doc.bodies().len(), 1);
    assert!(near(vol(&doc), 500.0, 1e-3), "{}", vol(&doc));
}

#[test]
fn split_by_a_plane_then_keep_one_half() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    doc.edit(|h| {
        h.add(FeatureKind::Split {
            tool: SplitTool::Plane(PlaneRef::Std(StdPlane::Right)),
            bodies: Vec::new(),
        })
        .unwrap()
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 2);
    for i in 0..2 {
        assert!(near(body_vol(&doc, i), 500.0, 1e-3), "{}", body_vol(&doc, i));
    }
    let right = (0..2)
        .find(|&i| threebcad_kernel::boolean::bbox(&doc.bodies()[i].solid).0[0] > -1e-6)
        .unwrap();
    let pick = flat(&doc, right, [1.0, 0.0, 0.0], 5.0);
    doc.edit(|h| {
        h.add(FeatureKind::DeleteBody {
            bodies: vec![pick.clone()],
            keep: true,
        })
        .unwrap()
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1);
    let (lo, _) = bbox(&doc);
    assert!(lo[0] > -1e-6, "{lo:?}");
}

/// A plane on the body's own bottom face cuts nothing: no paper-thin body comes off.
#[test]
fn a_split_plane_on_a_face_cuts_nothing() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    let t = doc.edit(|h| {
        h.add(FeatureKind::Split {
            tool: SplitTool::Plane(PlaneRef::Std(StdPlane::Top)),
            bodies: Vec::new(),
        })
        .unwrap()
    });
    assert!(doc.errors()[&t].contains("does not cut"), "{:?}", doc.errors());
    assert_eq!(doc.bodies().len(), 1);
    assert!(near(vol(&doc), 1000.0, 1e-3));
}

#[test]
fn split_by_a_sketch_cuts_out_a_core() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    doc.edit(|h| {
        let s = h.add_sketch(Sketch::circle([0.0, 0.0], 2.0), Plane::TOP);
        h.add(FeatureKind::Split {
            tool: SplitTool::Sketch(s),
            bodies: Vec::new(),
        })
        .unwrap()
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 2);
    let core = PI * 4.0 * 10.0;
    let mut vols: Vec<f64> = (0..2).map(|i| body_vol(&doc, i)).collect();
    vols.sort_by(f64::total_cmp);
    assert!(
        near(vols[0], core, 0.01) && near(vols[1], 1000.0 - core, 0.01),
        "{vols:?}"
    );
}

#[test]
fn move_copy_rotate_and_mate_bodies() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    let any = flat(&doc, 0, [0.0, 0.0, 1.0], 10.0);
    let m = doc.edit(|h| {
        h.add(FeatureKind::MoveBody(MoveBody {
            bodies: vec![any],
            translate: [20.0, 0.0, 0.0],
            rotate: None,
            mate: None,
            copy: true,
        }))
        .unwrap()
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 2);
    let (lo, hi) = bbox(&doc);
    assert!(near(lo[0], -5.0, 1e-6) && near(hi[0], 25.0, 1e-6), "{lo:?} {hi:?}");
    assert!(near(vol(&doc), 2000.0, 1e-3));
    // Turned a quarter about z, no copy: the block is square in plan, so turn a slab instead.
    doc.edit(|h| {
        if let Some(FeatureKind::MoveBody(mb)) = h.get_mut(m).map(|f| &mut f.kind) {
            mb.copy = false;
            mb.translate = [0.0; 3];
            mb.rotate = Some((AxisRef::Std(1), PI / 2.0));
        }
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1);
    let (lo, hi) = bbox(&doc);
    // About the y axis: z 0..10 becomes x 0..10, x −5..5 becomes z 5..−5.
    assert!(
        near(lo[0], 0.0, 1e-6) && near(hi[0], 10.0, 1e-6) && near(lo[2], -5.0, 1e-6),
        "{lo:?} {hi:?}"
    );
}

#[test]
fn mate_puts_a_face_against_a_plane() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    let bottom = flat(&doc, 0, [0.0, 0.0, -1.0], 0.0);
    let plane = doc.edit(|h| {
        h.add(FeatureKind::RefPlane(RefPlaneDef::Offset {
            base: PlaneRef::Std(StdPlane::Top),
            distance: 30.0,
            flip: false,
        }))
        .unwrap()
    });
    doc.edit(|h| {
        h.add(FeatureKind::MoveBody(MoveBody {
            bodies: vec![bottom.clone()],
            translate: [0.0; 3],
            rotate: None,
            mate: Some((bottom, PlaneRef::Feature(plane))),
            copy: false,
        }))
        .unwrap()
    });
    ok(&doc);
    // The bottom face (normal −z) against the plane z = 30 (normal +z): it now faces down onto it
    // from above, so the block sits on z = 30.
    let (lo, hi) = bbox(&doc);
    assert!(near(lo[2], 30.0, 1e-6) && near(hi[2], 40.0, 1e-6), "{lo:?} {hi:?}");
}

#[test]
fn scale_uniform_and_non_uniform_about_the_origin() {
    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    let s = doc.edit(|h| {
        h.add(FeatureKind::ScaleBody {
            bodies: Vec::new(),
            about: PointRef::Origin,
            factors: [2.0, 2.0, 2.0],
        })
        .unwrap()
    });
    ok(&doc);
    assert!(near(vol(&doc), 8000.0, 1e-3), "{}", vol(&doc));
    doc.edit(|h| {
        if let Some(FeatureKind::ScaleBody { factors, .. }) = h.get_mut(s).map(|f| &mut f.kind) {
            *factors = [1.0, 0.5, 3.0];
        }
    });
    ok(&doc);
    assert!(near(vol(&doc), 1500.0, 1e-3), "{}", vol(&doc));
    let (lo, hi) = bbox(&doc);
    assert!(near(lo[1], -2.5, 1e-6) && near(hi[2], 30.0, 1e-6), "{lo:?} {hi:?}");
}

/// A 40 x 10 x 2 plate with a 1 mm pin at the origin, 3 tall.
fn plate_with_pin() -> (PartDocument, FeatureId) {
    let mut doc = PartDocument::new("Part");
    doc.edit(|h| {
        let s = h.add_sketch(
            polygon(&[[-5.0, -5.0], [35.0, -5.0], [35.0, 5.0], [-5.0, 5.0]]),
            Plane::TOP,
        );
        h.add_extrude(s, 2.0).unwrap();
    });
    let pin = doc.edit(|h| {
        let s = h.add_sketch(Sketch::circle([0.0, 0.0], 1.0), lifted(2.0));
        h.add_extrude(s, 3.0).unwrap()
    });
    ok(&doc);
    (doc, pin)
}

#[test]
fn a_sketch_pattern_puts_a_copy_on_each_point() {
    let (mut doc, pin) = plate_with_pin();
    let before = vol(&doc);
    let pin_vol = PI * 3.0;
    doc.edit(|h| {
        let mut s = Sketch::default();
        s.add_point(10.0, 0.0);
        s.add_point(20.0, 3.0);
        s.add_point(30.0, -3.0);
        let s = h.add_sketch(s, lifted(2.0));
        h.add(FeatureKind::SketchPattern {
            seeds: Seeds::Features(vec![pin]),
            sketch: s,
        })
        .unwrap()
    });
    ok(&doc);
    assert!(near(vol(&doc) - before, 3.0 * pin_vol, 0.01), "{}", vol(&doc) - before);
}

#[test]
fn a_curve_pattern_spaces_copies_along_a_path() {
    let (mut doc, pin) = plate_with_pin();
    let before = vol(&doc);
    let pin_vol = PI * 3.0;
    let p = doc.edit(|h| {
        let s = h.add_sketch(chain(&[[0.0, 0.0], [30.0, 0.0]]), lifted(2.0));
        h.add(FeatureKind::CurvePattern {
            seeds: Seeds::Features(vec![pin]),
            curve: CurveRef::Sketch(s),
            count: 4,
            spacing: None,
            align: false,
        })
        .unwrap()
    });
    ok(&doc);
    assert!(near(vol(&doc) - before, 3.0 * pin_vol, 0.01), "{}", vol(&doc) - before);
    // Fixed spacing: 3 copies 7 apart.
    doc.edit(|h| {
        if let Some(FeatureKind::CurvePattern { spacing, .. }) = h.get_mut(p).map(|f| &mut f.kind) {
            *spacing = Some(7.0);
        }
    });
    ok(&doc);
    // Too far: the error names the instance.
    doc.edit(|h| {
        if let Some(FeatureKind::CurvePattern { spacing, .. }) = h.get_mut(p).map(|f| &mut f.kind) {
            *spacing = Some(15.0);
        }
    });
    assert!(doc.errors()[&p].contains("off the end"), "{:?}", doc.errors());
}

#[test]
fn a_dome_caps_a_rod_and_a_block() {
    let mut doc = PartDocument::new("Part");
    rod(&mut doc, 5.0, 10.0);
    let top = flat(&doc, 0, [0.0, 0.0, 1.0], 10.0);
    let before = vol(&doc);
    doc.edit(|h| {
        h.add(FeatureKind::Dome {
            face: top,
            height: 2.0,
            reverse: false,
        })
        .unwrap()
    });
    ok(&doc);
    let (r, hh) = (5.0f64, 2.0f64);
    let big = (r * r + hh * hh) / (2.0 * hh);
    let cap = PI * hh * hh * (3.0 * big - hh) / 3.0;
    assert!(near(vol(&doc) - before, cap, 0.02), "{} vs {cap}", vol(&doc) - before);
    assert_eq!(doc.bodies().len(), 1);
    let (_, hi) = bbox(&doc);
    assert!(near(hi[2], 12.0, 1e-3), "{hi:?}");

    let mut doc = PartDocument::new("Part");
    block(&mut doc, 5.0, 10.0);
    let top = flat(&doc, 0, [0.0, 0.0, 1.0], 10.0);
    doc.edit(|h| {
        h.add(FeatureKind::Dome {
            face: top,
            height: 3.0,
            reverse: true,
        })
        .unwrap()
    });
    ok(&doc);
    let removed = 1000.0 - vol(&doc);
    assert!(removed > 100.0 && removed < 300.0, "{removed}");
}

#[test]
fn wrap_embosses_and_debosses_a_rod() {
    for (kind, sign) in [(WrapKind::Emboss, 1.0), (WrapKind::Deboss, -1.0)] {
        let mut doc = PartDocument::new("Part");
        rod(&mut doc, 10.0, 30.0);
        let side = round(&doc, 0, 10.0);
        let before = vol(&doc);
        doc.edit(|h| {
            // On the right plane (y, z) through the axis: 6 around by 6 tall.
            let s = h.add_sketch(square([0.0, 15.0], 3.0), Plane::RIGHT);
            h.add(FeatureKind::Wrap {
                sketch: s,
                face: side,
                kind,
                depth: 1.0,
            })
            .unwrap()
        });
        ok(&doc);
        let change = vol(&doc) - before;
        let (r0, r1) = if sign > 0.0 { (10.0, 11.0) } else { (9.0, 10.0) };
        let expected = sign * (r1 * r1 - r0 * r0) / 2.0 * 0.6 * 6.0;
        assert!(near(change, expected, 0.03), "{kind:?}: {change} vs {expected}");
    }
}

/// A whole circle wraps as one curved edge: it must be fitted finely enough to stay on the cylinder.
#[test]
fn wrap_rolls_a_large_circle_onto_a_rod() {
    for (kind, sign) in [(WrapKind::Emboss, 1.0), (WrapKind::Deboss, -1.0)] {
        let mut doc = PartDocument::new("Part");
        rod(&mut doc, 10.0, 40.0);
        let side = round(&doc, 0, 10.0);
        let before = vol(&doc);
        doc.edit(|h| {
            let s = h.add_sketch(Sketch::circle([0.0, 20.0], 4.0), Plane::FRONT);
            h.add(FeatureKind::Wrap {
                sketch: s,
                face: side,
                kind,
                depth: 1.0,
            })
            .unwrap()
        });
        ok(&doc);
        // The flat area A rolls through A / R of angle: (r1² − r0²) / 2 · A / R.
        let (r0, r1) = if sign > 0.0 { (10.0, 11.0) } else { (9.0, 10.0) };
        let expected = sign * (r1 * r1 - r0 * r0) / 2.0 * (PI * 16.0) / 10.0;
        let change = vol(&doc) - before;
        assert!(near(change, expected, 0.03), "{kind:?}: {change} vs {expected}");
    }
}
