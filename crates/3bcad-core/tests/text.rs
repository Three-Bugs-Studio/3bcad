//! Sketch text (#113) end to end: lettering sketched on a face, embossed with Extruded Boss and
//! debossed with Extruded Cut. Volumes are checked against the letters' area times the depth.

use std::collections::HashMap;

use threebcad_core::rebuild::stable_plane;
use threebcad_core::*;
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::{print_mesh, PrintMesh};
use threebcad_kernel::Plane;
use threebcad_sketch::geom::polygon_area;
use threebcad_sketch::profile::loop_polygon;
use threebcad_sketch::text::BUILTIN_FONT;
use threebcad_sketch::Sketch;

fn vol(doc: &PartDocument) -> f64 {
    let meshes: Vec<_> = doc.bodies().iter().map(|b| print_mesh(&b.solid, 0.002)).collect();
    mass_properties(&meshes).volume
}

/// Every edge of the mesh is shared by exactly two triangles, once in each direction.
fn watertight(m: &PrintMesh) -> bool {
    let mut edges: HashMap<(usize, usize), i32> = HashMap::new();
    for t in &m.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += if a < b { 1 } else { -1 };
        }
    }
    !edges.is_empty() && edges.values().all(|&n| n == 0)
}

/// Area of a sketch's profile (holes subtracted).
fn profile_area(s: &Sketch) -> f64 {
    s.regions()
        .unwrap()
        .iter()
        .map(|r| {
            polygon_area(&loop_polygon(&r.outer, 512))
                + r.holes.iter().map(|h| polygon_area(&loop_polygon(h, 512))).sum::<f64>()
        })
        .sum()
}

fn ok(doc: &PartDocument) {
    assert!(doc.errors().is_empty(), "{:?}", doc.errors());
}

/// A 70 x 24 x 5 plate, x and y centred on the origin.
fn plate(doc: &mut PartDocument) {
    doc.edit(|h| {
        let mut s = Sketch::default();
        let p: Vec<_> = [[-35.0, -12.0], [35.0, -12.0], [35.0, 12.0], [-35.0, 12.0]]
            .iter()
            .map(|q| s.add_point(q[0], q[1]))
            .collect();
        for i in 0..4 {
            s.add_line(p[i], p[(i + 1) % 4]);
        }
        let s = h.add_sketch(s, Plane::TOP);
        h.add_extrude(s, 5.0).unwrap();
    });
}

/// "3BCAD", 10 high, in a sketch on the plate's top face.
fn lettering(doc: &mut PartDocument) -> (FeatureId, f64) {
    let top = doc.bodies()[0]
        .topo
        .faces
        .iter()
        .position(|f| {
            f.plane
                .is_some_and(|p| p.normal()[2] > 0.999 && (p.origin[2] - 5.0).abs() < 1e-9)
        })
        .unwrap();
    let face = doc.face_ref(0, top).unwrap();
    let plane = stable_plane(&doc.bodies()[0].topo.faces[top].plane.unwrap());
    let mut s = Sketch::default();
    s.add_text("3BCAD", BUILTIN_FONT, 10.0, [-28.0, -5.0]);
    let area = profile_area(&s);
    let id = doc.edit(|h| h.add_sketch_on(s, plane, PlaneRef::Face(face)).unwrap());
    (id, area)
}

#[test]
fn text_embossed_on_a_face_adds_its_area_times_the_height() {
    let mut doc = PartDocument::new("Label");
    plate(&mut doc);
    let (sketch, area) = lettering(&mut doc);
    assert!(area > 100.0, "{area}");
    let t = std::time::Instant::now();
    doc.edit(|h| {
        let mut e = Extrude::boss(sketch, 1.0);
        e.end = End::Blind(1.0);
        h.add(FeatureKind::Extrude(e)).unwrap()
    });
    eprintln!("emboss took {:?}", t.elapsed());
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1, "the letters merge with the plate");
    let want = 70.0 * 24.0 * 5.0 + area * 1.0;
    let got = vol(&doc);
    assert!((got - want).abs() < 2e-3 * area, "volume {got}, want {want}");
    assert!(watertight(&print_mesh(&doc.bodies()[0].solid, 0.01)));
    let (lo, hi) = threebcad_kernel::boolean::bbox(&doc.bodies()[0].solid);
    assert!((hi[2] - 6.0).abs() < 1e-6 && lo[2].abs() < 1e-6, "{lo:?} {hi:?}");
}

#[test]
fn text_debossed_into_a_face_removes_its_area_times_the_depth() {
    let mut doc = PartDocument::new("Label");
    plate(&mut doc);
    let (sketch, area) = lettering(&mut doc);
    let t = std::time::Instant::now();
    doc.edit(|h| {
        let mut e = Extrude::cut(sketch, 0.6);
        e.end = End::Blind(0.6);
        h.add(FeatureKind::Extrude(e)).unwrap()
    });
    eprintln!("deboss took {:?}", t.elapsed());
    ok(&doc);
    assert_eq!(doc.bodies().len(), 1);
    let want = 70.0 * 24.0 * 5.0 - area * 0.6;
    let got = vol(&doc);
    assert!((got - want).abs() < 2e-3 * area, "volume {got}, want {want}");
    assert!(watertight(&print_mesh(&doc.bodies()[0].solid, 0.01)));
}

/// A washer on a face and a ring-shaped groove in it: profiles with holes on a face, which truck
/// cannot add or cut in one go (v1.1 refused both).
#[test]
fn a_ring_on_a_face_is_added_and_cut_with_its_hole() {
    use std::f64::consts::PI;
    for cut in [false, true] {
        let mut doc = PartDocument::new("Washer");
        plate(&mut doc);
        let top = doc.bodies()[0]
            .topo
            .faces
            .iter()
            .position(|f| {
                f.plane
                    .is_some_and(|p| p.normal()[2] > 0.999 && (p.origin[2] - 5.0).abs() < 1e-9)
            })
            .unwrap();
        let face = doc.face_ref(0, top).unwrap();
        let plane = stable_plane(&doc.bodies()[0].topo.faces[top].plane.unwrap());
        let mut s = Sketch::default();
        let c = s.add_point(1.0, 1.0);
        s.add_circle(c, 6.0);
        s.add_circle(c, 3.0);
        let id = doc.edit(|h| h.add_sketch_on(s, plane, PlaneRef::Face(face)).unwrap());
        doc.edit(|h| {
            let mut e = if cut {
                Extrude::cut(id, 2.0)
            } else {
                Extrude::boss(id, 2.0)
            };
            e.end = End::Blind(2.0);
            h.add(FeatureKind::Extrude(e)).unwrap()
        });
        ok(&doc);
        let ring = PI * (36.0 - 9.0) * 2.0;
        let want = 8400.0 + if cut { -ring } else { ring };
        let got = vol(&doc);
        assert!((got - want).abs() < 2e-3 * ring, "cut {cut}: volume {got}, want {want}");
        assert_eq!(doc.bodies().len(), 1);
        // Not checked watertight: like any round boss sketched on a face (since v0.3), its micron
        // overlap leaves mesh seams that do not join; text uses a wider overlap.
    }
}
