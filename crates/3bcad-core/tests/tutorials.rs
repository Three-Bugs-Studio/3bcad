//! The user guide's tutorials (`docs/tutorials/`), built feature by feature with the values and
//! defaults the guide tells the reader to use. Each test checks the finished part against a hand
//! calculation and checks that the default (Fine) STL / 3MF export is watertight, so a change that
//! breaks a tutorial shows up here before a reader finds it.

use std::collections::HashMap;
use std::f64::consts::PI;

use threebcad_core::rebuild::stable_plane;
use threebcad_core::*;
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::{print_mesh, print_mesh_within, PrintMesh};
use threebcad_kernel::topology::{EdgeCurve, TopoFace};
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

/// The 3D Print tab's default export quality ("Fine"): what Export STL / 3MF write.
fn export_meshes(doc: &PartDocument) -> Vec<PrintMesh> {
    doc.bodies()
        .iter()
        .map(|b| print_mesh_within(&b.solid, 0.01, 10f64.to_radians()))
        .collect()
}

fn vol(doc: &PartDocument) -> f64 {
    let meshes: Vec<_> = doc.bodies().iter().map(|b| print_mesh(&b.solid, 0.005)).collect();
    mass_properties(&meshes).volume
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

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// Closed and consistently oriented: every directed edge once, and its reverse once. This is what
/// a slicer needs from an STL or 3MF file.
fn assert_watertight(meshes: &[PrintMesh]) {
    for (i, m) in meshes.iter().enumerate() {
        let mut edges = HashMap::new();
        for t in &m.triangles {
            for k in 0..3 {
                *edges.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        for (&(a, b), &n) in &edges {
            assert_eq!(n, 1, "body {i}: edge {a}-{b} used {n} times");
            assert_eq!(edges.get(&(b, a)), Some(&1), "body {i}: edge {a}-{b} has no twin");
        }
    }
}

/// A closed polyline, the way the Line tool chains it.
fn polygon(points: &[[f64; 2]]) -> Sketch {
    let mut s = Sketch::default();
    let ids: Vec<_> = points.iter().map(|p| s.add_point(p[0], p[1])).collect();
    for i in 0..ids.len() {
        s.add_line(ids[i], ids[(i + 1) % ids.len()]);
    }
    s
}

/// Center Rectangle on the origin.
fn center_rect(s: &mut Sketch, w: f64, h: f64) {
    let p = [[-w, -h], [w, -h], [w, h], [-w, h]].map(|[x, y]| s.add_point(x / 2.0, y / 2.0));
    for i in 0..4 {
        s.add_line(p[i], p[(i + 1) % 4]);
    }
}

/// Circles on the origin, as one sketch (a ring when there are two).
fn circles(radii: &[f64]) -> Sketch {
    let mut s = Sketch::default();
    let c = s.add_point(0.0, 0.0);
    for &r in radii {
        s.add_circle(c, r);
    }
    s
}

fn face_index(doc: &PartDocument, pred: impl Fn(&TopoFace) -> bool) -> usize {
    doc.bodies()[0].topo.faces.iter().position(pred).expect("face")
}

fn face(doc: &PartDocument, pred: impl Fn(&TopoFace) -> bool) -> FaceRef {
    doc.face_ref(0, face_index(doc, pred)).unwrap()
}

/// The flat face with outward `normal` at signed distance `at` from the origin.
fn is_flat(f: &TopoFace, normal: [f64; 3], at: f64) -> bool {
    f.plane.is_some_and(|p| {
        let n = p.normal();
        let d: f64 = (0..3).map(|i| n[i] * normal[i]).sum();
        let o: f64 = (0..3).map(|i| p.origin[i] * normal[i]).sum();
        d > 0.999 && (o - at).abs() < 1e-6
    })
}

/// The plane and reference Sketch uses for a selected flat face of the first body.
fn face_plane(doc: &PartDocument, f: usize) -> (Plane, PlaneRef) {
    let plane = stable_plane(&doc.bodies()[0].topo.faces[f].plane.unwrap());
    (plane, PlaneRef::Face(doc.face_ref(0, f).unwrap()))
}

fn extrude(h: &mut History, sketch: FeatureId, depth: f64) {
    let mut e = Extrude::boss(sketch, 10.0);
    e.end = End::Blind(depth);
    h.add(FeatureKind::Extrude(e)).unwrap();
}

/// A straight edge parallel to Y through (x, z): the edges of a profile extruded from the Front Plane.
fn y_edge(doc: &PartDocument, x: f64, z: f64) -> EdgeRef {
    let e = doc.bodies()[0]
        .topo
        .edges
        .iter()
        .position(|e| {
            matches!(e.curve, EdgeCurve::Line { a, b }
                if near(a[0], x, 1e-6) && near(b[0], x, 1e-6) && near(a[2], z, 1e-6) && near(b[2], z, 1e-6))
        })
        .unwrap_or_else(|| panic!("no edge along Y at x = {x}, z = {z}"));
    doc.edge_ref(0, e).unwrap()
}

/// Phone stand: a side profile on the Front Plane extruded 60 mm, a 3 mm fillet in the inside
/// corner of the ledge, and a 1 mm chamfer on the top ridge and the lip's two top edges. Laid on
/// its side it prints without support.
#[test]
fn tutorial_phone_stand() {
    // Front Plane: sketch x is world X, sketch y is world Z; it extrudes towards -Y.
    let profile = [
        [0.0, 0.0],
        [50.0, 0.0],
        [50.0, 70.0],
        [19.0, 10.0],
        [5.0, 10.0],
        [5.0, 18.0],
        [0.0, 18.0],
    ];
    let mut doc = PartDocument::new("phone-stand");
    doc.edit(|h| {
        let s = h.add_sketch(polygon(&profile), Plane::FRONT);
        extrude(h, s, 60.0);
    });
    ok(&doc);
    let corner = y_edge(&doc, 19.0, 10.0);
    let edges = vec![
        y_edge(&doc, 50.0, 70.0),
        y_edge(&doc, 0.0, 18.0),
        y_edge(&doc, 5.0, 18.0),
    ];
    doc.edit(|h| {
        h.add(FeatureKind::Fillet {
            edges: vec![corner],
            radius: 3.0,
            propagate: true,
        })
        .unwrap();
        h.add(FeatureKind::Chamfer {
            edges,
            kind: ChamferKind::EqualDistance,
            d1: 1.0,
            d2: 1.0,
            angle: 45f64.to_radians(),
            flip: false,
        })
        .unwrap();
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1);

    // Shoelace area of the profile.
    let n = profile.len();
    let area: f64 = (0..n)
        .map(|i| {
            let (p, q) = (profile[i], profile[(i + 1) % n]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        / 2.0;
    assert!(near(area, 1470.0, 1e-9), "{area}");
    // The face the phone leans on rises 60 over 31: the ridge is a 27° corner, the ledge's inside
    // corner 117° of open space.
    let ridge = 31f64.atan2(60.0);
    let inside = PI / 2.0 + ridge;
    // A fillet of radius r fills r²(cot(a/2) − (π − a)/2) of a corner a wide; a d x d chamfer cuts
    // ½d² sin(a) off a corner of angle a.
    let fillet = 9.0 * (1.0 / (inside / 2.0).tan() - (PI - inside) / 2.0);
    let chamfers = 0.5 * ridge.sin() + 0.5 + 0.5;
    let expected = (area + fillet - chamfers) * 60.0;
    let v = vol(&doc);
    assert!(near(v, expected, 2e-4 * expected), "{v} vs {expected}");
    let round = doc.bodies()[0]
        .topo
        .faces
        .iter()
        .find_map(|f| f.cylinder)
        .expect("the fillet");
    assert!(
        near(round.2, 3.0, 1e-2) && near(round.1[1].abs(), 1.0, 1e-9),
        "{round:?}"
    );

    // 50 deep, 60 wide, and the chamfer takes the ridge down to 1 mm along the leaning face.
    let (lo, hi) = bbox(&doc);
    let top = 70.0 - 60.0 / 31f64.hypot(60.0);
    assert!(near(lo[0], 0.0, 1e-6) && near(hi[0], 50.0, 1e-6), "{lo:?} {hi:?}");
    // The edge tools leave a micron proud of the side faces.
    assert!(near(hi[1] - lo[1], 60.0, 0.01), "{lo:?} {hi:?}");
    assert!(near(lo[2], 0.0, 1e-6) && near(hi[2], top, 1e-6), "{hi:?} vs top {top}");

    let meshes = export_meshes(&doc);
    assert_watertight(&meshes);
    // Laid flat on its side (Lay Flat on the face at y = 0), nothing overhangs past 45°.
    let m = &meshes[0];
    let tris: Vec<_> = m.triangles.iter().map(|t| t.map(|i| m.vertices[i])).collect();
    let hangs = threebcad_kernel::print::overhangs(&tris, [0.0, -1.0, 0.0], 45f64.to_radians());
    assert!(
        !hangs.contains(&true),
        "{} triangles overhang",
        hangs.iter().filter(|h| **h).count()
    );
}

/// Enclosure: an 80 x 60 x 30 box shelled 2 mm with a Ø11 cable hole in the front wall, and a
/// lid in its own part: an 80 x 60 x 2 plate, a 76 x 56 x 4 block on it pocketed 72 x 52 to make
/// a lip that fits inside the box, a Ø5.5 hole for an LED, and a slip Fit of 0.15 mm on the lip.
///
/// The holes are Extruded Cuts: Hole Wizard holes export with open edges (see the guide's
/// troubleshooting page), and a boss sketched as a hollow rectangle fails, so the lip is a block
/// and a pocket.
#[test]
fn tutorial_enclosure_with_lid() {
    let mut box_ = PartDocument::new("enclosure");
    box_.edit(|h| {
        let mut s = Sketch::default();
        center_rect(&mut s, 80.0, 60.0);
        let s = h.add_sketch(s, Plane::TOP);
        extrude(h, s, 30.0);
    });
    let top = face(&box_, |f| is_flat(f, [0.0, 0.0, 1.0], 30.0));
    box_.edit(|h| {
        h.add(FeatureKind::Shell {
            faces: vec![top],
            thickness: 2.0,
            outward: false,
        })
        .unwrap()
    });
    ok(&box_);
    // On the front face the sketch origin is the part origin's projection, on the bottom edge:
    // the hole is 20 to the right and 15 up. Through All would drill the back wall too, so it is
    // a blind cut 10 deep that ends in the air inside.
    let (plane, on) = face_plane(&box_, face_index(&box_, |f| is_flat(f, [0.0, -1.0, 0.0], 30.0)));
    box_.edit(|h| {
        let s = h.add_sketch_on(Sketch::circle([20.0, 15.0], 5.5), plane, on).unwrap();
        h.add(FeatureKind::Extrude(Extrude::cut(s, 10.0))).unwrap();
    });
    ok(&box_);
    assert_eq!(box_.bodies().len(), 1);
    let shell = 80.0 * 60.0 * 30.0 - 76.0 * 56.0 * 28.0;
    let cable = PI * 5.5 * 5.5 * 2.0;
    let v = vol(&box_);
    assert!(near(v, shell - cable, 2e-4 * shell), "{v} vs {}", shell - cable);
    let (lo, hi) = bbox(&box_);
    assert!(near(lo[0], -40.0, 1e-6) && near(hi[1], 30.0, 1e-6) && near(hi[2], 30.0, 1e-6));
    // The hole's wall is Ø11 about (x = 20, z = 15), and only in the front wall.
    let wall: Vec<[f64; 3]> = box_.bodies()[0]
        .topo
        .faces
        .iter()
        .filter(|f| f.plane.is_none())
        .flat_map(|f| f.tris.iter().flatten().copied())
        .collect();
    assert!(!wall.is_empty());
    for p in wall {
        assert!(
            near((p[0] - 20.0).hypot(p[2] - 15.0), 5.5, 0.01) && (-30.0 - 1e-6..=-28.0 + 1e-6).contains(&p[1]),
            "{p:?}"
        );
    }
    assert_watertight(&export_meshes(&box_));

    let mut lid = PartDocument::new("enclosure-lid");
    let rect = |w: f64, h: f64| {
        let mut s = Sketch::default();
        center_rect(&mut s, w, h);
        s
    };
    lid.edit(|h| {
        let s = h.add_sketch(rect(80.0, 60.0), Plane::TOP);
        extrude(h, s, 2.0);
    });
    let (plane, on) = face_plane(&lid, face_index(&lid, |f| is_flat(f, [0.0, 0.0, 1.0], 2.0)));
    lid.edit(|h| {
        // The box's 76 x 56 opening.
        let s = h.add_sketch_on(rect(76.0, 56.0), plane, on).unwrap();
        extrude(h, s, 4.0);
    });
    let (plane, on) = face_plane(&lid, face_index(&lid, |f| is_flat(f, [0.0, 0.0, 1.0], 6.0)));
    lid.edit(|h| {
        let s = h.add_sketch_on(rect(72.0, 52.0), plane, on).unwrap();
        let mut e = Extrude::cut(s, 10.0);
        e.end = End::Blind(4.0);
        h.add(FeatureKind::Extrude(e)).unwrap();
    });
    ok(&lid);
    let (plane, on) = face_plane(
        &lid,
        face_index(&lid, |f| {
            is_flat(f, [0.0, 0.0, 1.0], 2.0) && f.tris.iter().flatten().all(|p| p[0].abs() < 36.0 + 1e-6)
        }),
    );
    lid.edit(|h| {
        let s = h.add_sketch_on(Sketch::circle([0.0, 0.0], 2.75), plane, on).unwrap();
        let mut e = Extrude::cut(s, 10.0);
        e.end = End::ThroughAll;
        h.add(FeatureKind::Extrude(e)).unwrap();
    });
    ok(&lid);
    let lip: Vec<_> = (0..lid.bodies()[0].topo.faces.len())
        .filter(|&i| {
            let f = &lid.bodies()[0].topo.faces[i];
            f.plane.is_some_and(|p| p.normal()[2].abs() < 1e-9)
                && f.tris.iter().flatten().all(|p| p[2] > 2.0 - 1e-9)
                && f.tris
                    .iter()
                    .flatten()
                    .all(|p| near(p[0].abs(), 38.0, 1e-6) || near(p[1].abs(), 28.0, 1e-6))
        })
        .map(|i| lid.face_ref(0, i).unwrap())
        .collect();
    assert_eq!(lip.len(), 4, "the lip's four outer faces");
    // Fit's default for PLA is the slip preset.
    lid.edit(|h| {
        h.add(FeatureKind::Fit {
            faces: lip,
            clearance: 0.15,
        })
        .unwrap()
    });
    ok(&lid);
    assert_eq!(lid.bodies().len(), 1);
    let plate = 80.0 * 60.0 * 2.0;
    let ring = (75.7 * 55.7 - 72.0 * 52.0) * 4.0;
    let led = PI * 2.75 * 2.75 * 2.0;
    let v = vol(&lid);
    let expected = plate + ring - led;
    assert!(near(v, expected, 2e-4 * expected), "{v} vs {expected}");
    // The fitted lip leaves 0.15 mm all round inside the box's 76 x 56 opening.
    let outer = |axis: usize| {
        lid.bodies()[0]
            .topo
            .faces
            .iter()
            .filter(|f| {
                f.plane.is_some_and(|p| p.normal()[axis] > 0.999) && f.tris.iter().flatten().all(|p| p[2] > 2.0 - 1e-9)
            })
            .map(|f| f.plane.unwrap().origin[axis])
            .fold(f64::NEG_INFINITY, f64::max)
    };
    assert!(near(38.0 - outer(0), 0.15, 1e-6) && near(28.0 - outer(1), 0.15, 1e-6));
    let (lo, hi) = bbox(&lid);
    assert!(near(lo[2], 0.0, 1e-6) && near(hi[2], 6.0, 1e-6) && near(hi[0], 40.0, 1e-6));
    assert_watertight(&export_meshes(&lid));
}

/// Bottle cap: a hexagon (Ø32 across the corners) 18 tall, shelled 3 mm with the top face removed;
/// a ring with the Ø17.5 bore (the M20 tap drill) from 7 to 17 mm up, sketched on a plane offset
/// 7 mm from the Top Plane; and an M20 x 2.5 thread in the bore.
///
/// The bore stops short of the floor and of the rim, so the thread runs out at both ends: the
/// kernel cannot thread a blind hole, nor add material to a threaded end. The cup is a hexagon,
/// and the ring's outside crosses the cup's flat walls, because truck fails on a ring inside a
/// round cup (coaxial cylinders) and on a ring face buried whole in a wall.
#[test]
fn tutorial_threaded_bottle_cap() {
    // Polygon tool, 6 sides, inscribed: the first corner where you click, here on +X.
    let r = 16.0;
    let hexagon: Vec<[f64; 2]> = (0..6)
        .map(|k| {
            let a = PI / 3.0 * k as f64;
            [r * a.cos(), r * a.sin()]
        })
        .collect();
    let mut doc = PartDocument::new("bottle-cap");
    doc.edit(|h| {
        let s = h.add_sketch(polygon(&hexagon), Plane::TOP);
        extrude(h, s, 18.0);
    });
    let top = face(&doc, |f| is_flat(f, [0.0, 0.0, 1.0], 18.0));
    doc.edit(|h| {
        h.add(FeatureKind::Shell {
            faces: vec![top],
            thickness: 3.0,
            outward: false,
        })
        .unwrap();
        let plane = h
            .add(FeatureKind::RefPlane(RefPlaneDef::Offset {
                base: PlaneRef::Std(StdPlane::Top),
                distance: 7.0,
                flip: false,
            }))
            .unwrap();
        let lifted = Plane {
            origin: [0.0, 0.0, 7.0],
            ..Plane::TOP
        };
        let s = h
            .add_sketch_on(circles(&[12.0, 8.75]), lifted, PlaneRef::Feature(plane))
            .unwrap();
        extrude(h, s, 10.0);
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1);
    // A hexagon of corner radius R covers (3√3/2)R²; the shell moves its flats in by 3.
    let hex = |corner: f64| 1.5 * 3f64.sqrt() * corner * corner;
    let apothem = r * (PI / 6.0).cos() - 3.0;
    let pocket = hex(apothem / (PI / 6.0).cos());
    let cup = hex(r) * 18.0 - pocket * 15.0;
    // In the cup the ring is the Ø24 disc less the six caps the flats cut off it, less the bore.
    let cap = 144.0 * (apothem / 12.0).acos() - apothem * (144.0 - apothem * apothem).sqrt();
    let ring = (PI * 144.0 - 6.0 * cap - PI * 8.75 * 8.75) * 10.0;
    let before = mass_properties(&export_meshes(&doc)).volume;
    assert!(
        near(before, cup + ring, 1e-3 * (cup + ring)),
        "{before} vs {}",
        cup + ring
    );

    let bore = face(&doc, |f| f.cylinder.is_some_and(|c| near(c.2, 8.75, 1e-3)));
    let t = doc.edit(|h| {
        h.add(FeatureKind::Thread(ThreadDef {
            face: bore,
            standard: HoleStandard::Iso,
            size: "M20".into(),
            length: None,
            offset: 0.0,
            flip: false,
            right: true,
        }))
        .unwrap()
    });
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1);
    assert_eq!(doc.built().threads[&t].designation, "M20x2.5");
    let meshes = export_meshes(&doc);
    assert_watertight(&meshes);
    // The thread cuts into the band between the bore and the major diameter along the whole
    // bore. Seen along the axis, the bolt's tooth fills 7/16 of that band on average (3P/4 wide at
    // the minor diameter, P/8 at the major).
    let removed = before - mass_properties(&meshes).volume;
    let band = PI * (10.0f64.powi(2) - 8.75f64.powi(2)) * 10.0;
    assert!(removed > 0.35 * band && removed < 0.5 * band, "{removed} of {band}");
    let (lo, hi) = bbox(&doc);
    assert!(
        near(lo[2], 0.0, 1e-6) && near(hi[2], 18.0, 1e-6) && near(hi[0], 16.0, 1e-6),
        "{lo:?} {hi:?}"
    );
}
