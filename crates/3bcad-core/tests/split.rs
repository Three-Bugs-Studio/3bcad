//! Split for the printer bed (#114): planes cut a part into pieces, and pins (or dowels) with a
//! fit clearance line them up again. Checked by the pieces' volumes, where the pins sit, and that
//! each pin and its hole share an axis with the clearance between their radii and floors.

use std::f64::consts::PI;

use threebcad_core::*;
use threebcad_kernel::boolean;
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::print_mesh;
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

fn vol(s: &threebcad_kernel::Solid) -> f64 {
    mass_properties(&[print_mesh(s, 0.002)]).volume
}

/// The circles where a solid's mesh meets the plane x = `x`: (centre (y, z), radius) of each, the
/// radius being the furthest mesh point from the centre (a flat face may have points inside).
fn rings(s: &threebcad_kernel::Solid, x: f64, apart: f64) -> Vec<([f64; 2], f64)> {
    let m = print_mesh(s, 0.01);
    let mut groups: Vec<Vec<[f64; 2]>> = Vec::new();
    for v in m.vertices.iter().filter(|v| (v[0] - x).abs() < 1e-6) {
        let p = [v[1], v[2]];
        match groups
            .iter_mut()
            .find(|g| (g[0][0] - p[0]).hypot(g[0][1] - p[1]) < apart)
        {
            Some(g) => g.push(p),
            None => groups.push(vec![p]),
        }
    }
    let mut out: Vec<([f64; 2], f64)> = groups
        .iter()
        .map(|g| {
            let c = g
                .iter()
                .fold([0.0; 2], |a, p| [a[0] + p[0], a[1] + p[1]])
                .map(|v| v / g.len() as f64);
            (
                c,
                g.iter().map(|p| (p[0] - c[0]).hypot(p[1] - c[1])).fold(0.0, f64::max),
            )
        })
        .collect();
    out.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
    out
}

/// Pins and holes on the same axes, with the clearance between their radii.
fn coaxial(pins: &[([f64; 2], f64)], holes: &[([f64; 2], f64)], r: f64, c: f64) {
    assert_eq!(pins.len(), holes.len(), "{pins:?} {holes:?}");
    for (p, h) in pins.iter().zip(holes) {
        assert!((p.0[0] - h.0[0]).hypot(p.0[1] - h.0[1]) < 1e-3, "{p:?} {h:?}");
        assert!(
            (p.1 - r).abs() < 0.01 && (h.1 - (r + c)).abs() < 0.01,
            "radii {} {}",
            p.1,
            h.1
        );
    }
}

fn ok(doc: &PartDocument) {
    assert!(doc.errors().is_empty(), "{:?}", doc.errors());
    assert!(doc.warnings().is_empty(), "{:?}", doc.warnings());
}

/// A 120 x 40 x 20 bar from x = -60 to 60, y = -20 to 20, z = 0 to 20.
fn bar(doc: &mut PartDocument) {
    doc.edit(|h| {
        let mut s = Sketch::default();
        let p: Vec<_> = [[-60.0, -20.0], [60.0, -20.0], [60.0, 20.0], [-60.0, 20.0]]
            .iter()
            .map(|q| s.add_point(q[0], q[1]))
            .collect();
        for i in 0..4 {
            s.add_line(p[i], p[(i + 1) % 4]);
        }
        let s = h.add_sketch(s, Plane::TOP);
        h.add_extrude(s, 20.0).unwrap();
    });
}

/// A plane square to x at `x`.
fn plane_at(doc: &mut PartDocument, x: f64) -> PlaneRef {
    let id = doc.edit(|h| {
        h.add(FeatureKind::RefPlane(RefPlaneDef::Offset {
            base: PlaneRef::Std(StdPlane::Right),
            distance: x,
            flip: false,
        }))
        .unwrap()
    });
    PlaneRef::Feature(id)
}

fn split(doc: &mut PartDocument, planes: Vec<PlaneRef>, pins: Option<Pins>) {
    doc.edit(|h| {
        h.add(FeatureKind::Split {
            tool: SplitTool::Plane(planes[0].clone()),
            bodies: Vec::new(),
            planes: planes[1..].to_vec(),
            pins,
        })
        .unwrap()
    });
}

/// Bodies sorted by where they lie along x.
fn by_x(doc: &PartDocument) -> Vec<&threebcad_kernel::Solid> {
    let mut b: Vec<_> = doc.bodies().iter().map(|b| &b.solid).collect();
    b.sort_by(|a, c| boolean::bbox(a).0[0].total_cmp(&boolean::bbox(c).0[0]));
    b
}

#[test]
fn two_planes_cut_a_bar_into_three_pieces() {
    let mut doc = PartDocument::new("Bar");
    bar(&mut doc);
    let p1 = plane_at(&mut doc, -20.0);
    let p2 = plane_at(&mut doc, 25.0);
    split(&mut doc, vec![p1, p2], None);
    ok(&doc);
    let pieces = by_x(&doc);
    assert_eq!(pieces.len(), 3);
    let area = 40.0 * 20.0;
    for (s, len) in pieces.iter().zip([40.0, 45.0, 35.0]) {
        assert!(
            (vol(s) - area * len).abs() < 1e-3 * area * len,
            "{} vs {}",
            vol(s),
            area * len
        );
    }
    assert!((boolean::bbox(pieces[1]).0[0] + 20.0).abs() < 1e-6);
    assert!((boolean::bbox(pieces[1]).1[0] - 25.0).abs() < 1e-6);
}

#[test]
fn pins_stand_on_one_piece_and_fit_holes_in_the_other() {
    let mut doc = PartDocument::new("Bar");
    bar(&mut doc);
    let (d, len, c) = (5.0, 10.0, 0.15);
    let pins = Pins {
        diameter: d,
        length: len,
        count: 2,
        clearance: c,
        dowels: false,
    };
    split(&mut doc, vec![PlaneRef::Std(StdPlane::Right)], Some(pins));
    ok(&doc);
    let pieces = by_x(&doc);
    assert_eq!(pieces.len(), 2);
    let half = 60.0 * 40.0 * 20.0;
    let pin = PI * (d / 2.0).powi(2) * len / 2.0;
    let hole = PI * (d / 2.0 + c).powi(2) * (len / 2.0 + c);
    // The piece on the +x side carries the pins; the other has the holes.
    let (left, right) = (vol(pieces[0]), vol(pieces[1]));
    assert!((right - (half + 2.0 * pin)).abs() < 2e-3 * pin, "right {right}");
    assert!((left - (half - 2.0 * hole)).abs() < 2e-3 * hole, "left {left}");
    // The pins reach 5 into the left piece, on the axes of its holes, 0.15 narrower all round and
    // 0.15 short of their floors.
    let (lo, _) = boolean::bbox(pieces[1]);
    assert!((lo[0] + len / 2.0).abs() < 1e-6, "pins stick out {}", -lo[0]);
    let tips = rings(pieces[1], -len / 2.0, 2.0 * d);
    coaxial(&tips, &rings(pieces[0], -len / 2.0 - c, 2.0 * d), d / 2.0, c);
    // On the cut face with room round them, and apart from each other.
    let margin = d / 2.0 + c + 1.5;
    for (q, _) in &tips {
        assert!(
            q[0].abs() <= 20.0 - margin + 1e-6 && q[1] >= margin - 1e-6 && q[1] <= 20.0 - margin + 1e-6,
            "{q:?}"
        );
    }
    let (a, b) = (tips[0].0, tips[1].0);
    assert!((a[0] - b[0]).hypot(a[1] - b[1]) >= 2.0 * margin, "{a:?} {b:?}");
}

#[test]
fn dowels_are_separate_bodies_in_holes_on_both_sides() {
    let mut doc = PartDocument::new("Bar");
    bar(&mut doc);
    let (d, len, c) = (6.0, 12.0, 0.2);
    let pins = Pins {
        diameter: d,
        length: len,
        count: 2,
        clearance: c,
        dowels: true,
    };
    split(&mut doc, vec![PlaneRef::Std(StdPlane::Right)], Some(pins));
    ok(&doc);
    assert_eq!(doc.bodies().len(), 4, "two pieces and two dowels");
    let mut vols: Vec<f64> = doc.bodies().iter().map(|b| vol(&b.solid)).collect();
    vols.sort_by(f64::total_cmp);
    let dowel = PI * (d / 2.0).powi(2) * len;
    let half = 60.0 * 40.0 * 20.0 - 2.0 * PI * (d / 2.0 + c).powi(2) * (len / 2.0 + c);
    for v in &vols[..2] {
        assert!((v - dowel).abs() < 2e-3 * dowel, "dowel {v}");
    }
    for v in &vols[2..] {
        assert!((v - half).abs() < 1e-3 * half, "piece {v} vs {half}");
    }
    // Each dowel spans the joint, 6 into each piece, in holes 0.2 wider all round and 0.2 deeper.
    let pieces = by_x(&doc);
    let (left, dowels, right) = (pieces[0], &pieces[1..3], pieces[3]);
    for s in dowels {
        let (lo, hi) = boolean::bbox(s);
        assert!(
            (lo[0] + len / 2.0).abs() < 1e-6 && (hi[0] - len / 2.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
    }
    let ends: Vec<_> = dowels.iter().flat_map(|s| rings(s, len / 2.0, 2.0 * d)).collect();
    let mut ends = ends;
    ends.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
    coaxial(&ends, &rings(right, len / 2.0 + c, 2.0 * d), d / 2.0, c);
    coaxial(&ends, &rings(left, -len / 2.0 - c, 2.0 * d), d / 2.0, c);
}

#[test]
fn a_thin_cut_face_gets_no_pins_and_says_so() {
    let mut doc = PartDocument::new("Plate");
    doc.edit(|h| {
        let s = h.add_sketch(Sketch::rectangle(100.0, 40.0), Plane::TOP);
        h.add_extrude(s, 3.0).unwrap();
    });
    let p = plane_at(&mut doc, 50.0);
    split(&mut doc, vec![p], Some(Pins::new(0.15)));
    assert!(doc.errors().is_empty(), "{:?}", doc.errors());
    assert_eq!(doc.bodies().len(), 2, "the split still happens");
    let w = doc.warnings().values().next().expect("a warning");
    assert!(w.contains("0 of 2 pins"), "{w}");
}
