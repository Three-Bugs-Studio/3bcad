//! Union / subtract / intersect on top of `truck-shapeops`, made safe for a CAD rebuild: no panics
//! escape, results are sanity-checked by volume, and coplanar-face failures are retried with the
//! tool grown by a micron.

use std::panic::{catch_unwind, AssertUnwindSafe};

use anyhow::{anyhow, ensure, Result};
use truck_meshalgo::prelude::*;
use truck_modeling::*;

/// Mesh tolerance of the boolean intersection curves (mm).
const CURVE_TOL: f64 = 0.05;
/// How far a tool is grown when the plain operation fails on touching/coplanar faces (mm).
pub const GROW: f64 = 1e-3;

/// Axis-aligned bounds of a solid's vertices and curve samples.
pub fn bbox(s: &Solid) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut add = |p: Point3| {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    };
    for e in s.edge_iter() {
        let c = e.curve();
        let (t0, t1) = c.range_tuple();
        for k in 0..=8 {
            add(c.subs(t0 + (t1 - t0) * k as f64 / 8.0));
        }
    }
    s.vertex_iter().for_each(|v| add(v.point()));
    (lo, hi)
}

fn overlaps(a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3]), margin: f64) -> bool {
    (0..3).all(|i| a.0[i] <= b.1[i] + margin && b.0[i] <= a.1[i] + margin)
}

/// Signed volume from a tessellation fine enough for about 0.1 % on typical parts; `None` when
/// truck cannot mesh the solid.
pub fn volume(s: &Solid) -> Option<f64> {
    volume_at(s, 1e-3, 1e-3, 0.05)
}

/// For the boolean sanity checks, which allow 1 % slack: a 5x coarser mesh, since meshing is most
/// of their cost.
fn rough_volume(s: &Solid) -> Option<f64> {
    volume_at(s, 5e-3, 5e-3, 0.25)
}

/// `rel` x the bounding-box diagonal, clamped to `min..=max` mm.
fn mesh_tol(s: &Solid, rel: f64, min: f64, max: f64) -> f64 {
    let (lo, hi) = bbox(s);
    let diag = (0..3).map(|i| (hi[i] - lo[i]).powi(2)).sum::<f64>().sqrt();
    (diag * rel).clamp(min, max)
}

/// Volume meshed at `mesh_tol`.
fn volume_at(s: &Solid, rel: f64, min: f64, max: f64) -> Option<f64> {
    let tol = mesh_tol(s, rel, min, max);
    catch_unwind(AssertUnwindSafe(|| {
        let mut poly = s.triangulation(tol).to_polygon();
        poly.put_together_same_attrs(TOLERANCE);
        let p = poly.positions();
        poly.faces()
            .triangle_iter()
            .map(|t| {
                let [a, b, c] = t.map(|v| p[v.pos]);
                a.to_vec().dot(b.to_vec().cross(c.to_vec())) / 6.0
            })
            .sum::<f64>()
    }))
    .ok()
    .filter(|v: &f64| v.is_finite())
}

/// Grows a solid by `d` along each axis (non-uniform scale about its center).
fn grown(s: &Solid, d: f64) -> Solid {
    let (lo, hi) = bbox(s);
    let c = Point3::new((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0);
    let f = |i: usize| 1.0 + 2.0 * d / (hi[i] - lo[i]).max(1e-6);
    builder::scaled(s, c, Vector3::new(f(0), f(1), f(2)))
}

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Union,
    Subtract,
    Intersect,
}

fn raw(op: Op, a: &Solid, b: &Solid) -> Option<Solid> {
    catch_unwind(AssertUnwindSafe(|| match op {
        Op::Union => truck_shapeops::or(a, b, CURVE_TOL),
        Op::Intersect => truck_shapeops::and(a, b, CURVE_TOL),
        Op::Subtract => {
            let mut nb = b.clone();
            nb.not();
            truck_shapeops::and(a, &nb, CURVE_TOL)
        }
    }))
    .ok()
    .flatten()
    .filter(|s| !s.boundaries().is_empty())
}

/// A mesh at the tolerance of `rough_volume`.
fn rough_mesh(s: &Solid) -> Option<PolygonMesh> {
    let tol = mesh_tol(s, 5e-3, 5e-3, 0.25);
    catch_unwind(AssertUnwindSafe(|| s.triangulation(tol).to_polygon())).ok()
}

/// How many times the closed mesh `m` wraps around `p`: 1 inside, 0 outside, 1/2 on its surface.
fn winding(p: Point3, m: &PolygonMesh) -> f64 {
    let v = m.positions();
    let angle: f64 = m
        .faces()
        .triangle_iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| v[i.pos] - p);
            let (la, lb, lc) = (a.magnitude(), b.magnitude(), c.magnitude());
            // The solid angle of the triangle (Van Oosterom and Strackee).
            2.0 * a
                .dot(b.cross(c))
                .atan2(la * lb * lc + a.dot(b) * lc + b.dot(c) * la + c.dot(a) * lb)
        })
        .sum();
    (angle / (4.0 * std::f64::consts::PI)).abs()
}

/// Lowest and highest winding of `s`'s vertices in `m`.
fn winding_range(s: &Solid, m: &PolygonMesh) -> (f64, f64) {
    s.vertex_iter()
        .map(|v| winding(v.point(), m))
        .fold((f64::INFINITY, 0.0), |(lo, hi), w| (lo.min(w), hi.max(w)))
}

/// Where two solids sit when their surfaces do not cross.
enum Apart {
    Disjoint,
    AInsideB,
    BInsideA,
}

/// truck's own inside test cannot be trusted here: its `and` panics when one solid holds the other.
/// `None` when the surfaces cross or the solids touch in a way that fits none of the cases.
fn apart(a: &Solid, b: &Solid) -> Option<Apart> {
    let (ma, mb) = (rough_mesh(a)?, rough_mesh(b)?);
    if ma.collide_with(&mb).is_some() {
        return None;
    }
    let ((a_lo, a_hi), (b_lo, b_hi)) = (winding_range(a, &mb), winding_range(b, &ma));
    if b_lo > 0.75 {
        Some(Apart::BInsideA)
    } else if a_lo > 0.75 {
        Some(Apart::AInsideB)
    } else if a_hi < 0.6 && b_hi < 0.6 {
        // Vertices on the other's surface (1/2) are fine: the solids touch but share no volume.
        Some(Apart::Disjoint)
    } else {
        None
    }
}

/// shortcut: truck intersects a solid's shells one after another, which is wrong for an inner void,
/// so a body with a void cannot be changed. Boolean each shell separately if that is ever needed.
fn no_voids(a: &Solid, b: &Solid) -> Result<()> {
    ensure!(
        a.boundaries().len() == 1 && b.boundaries().len() == 1,
        "bodies with an internal void cannot be cut or merged yet"
    );
    Ok(())
}

/// Runs `op`, checks the volume is plausible, and retries with the tool grown when it is not.
fn checked(op: Op, a: &Solid, b: &Solid) -> Result<Solid> {
    let (va, vb) = (
        rough_volume(a).ok_or_else(|| anyhow!("a body cannot be meshed"))?,
        rough_volume(b).ok_or_else(|| anyhow!("the feature's tool cannot be meshed"))?,
    );
    let slack = 0.01 * va.max(vb) + 1e-6;
    let plausible = |v: f64| match op {
        Op::Union => v >= va.max(vb) - slack && v <= va + vb + slack,
        Op::Subtract => v <= va + slack && v >= va - vb - slack,
        Op::Intersect => v <= va.min(vb) + slack && v >= -slack,
    };
    for grow in [0.0, GROW, 10.0 * GROW] {
        let tool = if grow == 0.0 { b.clone() } else { grown(b, grow) };
        if let Some(s) = raw(op, a, &tool) {
            if rough_volume(&s).is_some_and(plausible) {
                return Ok(s);
            }
        }
    }
    Err(anyhow!(
        "the boolean operation failed (truck could not intersect these faces); try moving the feature slightly"
    ))
}

/// One solid per closed shell.
pub fn split(s: Solid) -> Vec<Solid> {
    s.into_boundaries().into_iter().map(|sh| Solid::new(vec![sh])).collect()
}

/// `a ∪ b`; `None` when they do not touch (the caller keeps two bodies).
pub fn union(a: &Solid, b: &Solid) -> Result<Option<Solid>> {
    if !overlaps(&bbox(a), &bbox(b), 2.0 * GROW) {
        return Ok(None);
    }
    no_voids(a, b)?;
    let s = checked(Op::Union, a, b)?;
    Ok((s.boundaries().len() < a.boundaries().len() + b.boundaries().len()).then_some(s))
}

/// `a − b`, split into its pieces (empty when nothing is left).
pub fn subtract(a: &Solid, b: &Solid) -> Result<Vec<Solid>> {
    if !overlaps(&bbox(a), &bbox(b), 0.0) {
        return Ok(vec![a.clone()]);
    }
    no_voids(a, b)?;
    match checked(Op::Subtract, a, b) {
        Ok(s) => Ok(split(s)),
        // truck cannot subtract solids whose faces never cross.
        Err(e) => match apart(a, b) {
            Some(Apart::Disjoint) => Ok(vec![a.clone()]),
            Some(Apart::AInsideB) => Ok(Vec::new()),
            Some(Apart::BInsideA) => {
                let mut void = b.clone();
                void.not();
                let mut shells = a.boundaries().clone();
                shells.extend(void.into_boundaries());
                Ok(vec![Solid::new(shells)])
            }
            None => {
                // The tool swallowed the body whole, sharing some of its faces: nothing is left.
                let swallowed = raw(Op::Intersect, a, b)
                    .and_then(|s| rough_volume(&s))
                    .zip(rough_volume(a))
                    .is_some_and(|(v, va)| (v - va).abs() < 1e-3 * va.abs() + 1e-6);
                if swallowed {
                    Ok(Vec::new())
                } else {
                    Err(e)
                }
            }
        },
    }
}

/// `a ∩ b`, split into pieces.
pub fn intersect(a: &Solid, b: &Solid) -> Result<Vec<Solid>> {
    if !overlaps(&bbox(a), &bbox(b), 0.0) {
        return Ok(Vec::new());
    }
    no_voids(a, b)?;
    match checked(Op::Intersect, a, b) {
        Ok(s) => Ok(split(s)),
        Err(e) => match apart(a, b) {
            Some(Apart::Disjoint) => Ok(Vec::new()),
            Some(Apart::AInsideB) => Ok(vec![a.clone()]),
            Some(Apart::BInsideA) => Ok(vec![b.clone()]),
            None => Err(e),
        },
    }
}

/// Applies a 4x4 (column-major) transform, turning mirrored solids right side out.
pub fn transformed(s: &Solid, m: [[f64; 4]; 4]) -> Solid {
    let mat = Matrix4::from_cols(
        Vector4::from(m[0]),
        Vector4::from(m[1]),
        Vector4::from(m[2]),
        Vector4::from(m[3]),
    );
    let mut out = builder::transformed(s, mat);
    if mat.determinant() < 0.0 {
        out.not();
    }
    out
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::operations::{extrude_circle, extrude_polygon};

    fn cube(x0: f64, z0: f64, s: f64) -> Solid {
        let b = extrude_polygon(&[[x0, 0.0], [x0 + s, 0.0], [x0 + s, s], [x0, s]], s).unwrap();
        builder::translated(&b, Vector3::new(0.0, 0.0, z0))
    }

    fn cyl(r: f64, z0: f64, z1: f64) -> Solid {
        builder::translated(
            &extrude_circle([5.0, 5.0], r, z1 - z0).unwrap(),
            Vector3::new(0.0, 0.0, z0),
        )
    }

    #[test]
    fn box_minus_cylinder_matches_analytic_volume() {
        let r = subtract(&cube(0.0, 0.0, 10.0), &cyl(3.0, -1.0, 11.0)).unwrap();
        assert_eq!(r.len(), 1);
        let v = volume(&r[0]).unwrap();
        assert!((v - (1000.0 - PI * 90.0)).abs() < 2.0, "{v}");
    }

    #[test]
    fn coplanar_and_touching_cases_are_retried() {
        let b = cube(0.0, 0.0, 10.0);
        // Same footprint stacked: coplanar side faces.
        let s = union(&b, &cube(0.0, 10.0, 10.0))
            .unwrap()
            .expect("touching bodies merge");
        assert!((volume(&s).unwrap() - 2000.0).abs() < 1.0);
        // Blind hole starting exactly on the top face.
        let h = subtract(&b, &cyl(3.0, 5.0, 10.0)).unwrap();
        assert!((volume(&h[0]).unwrap() - (1000.0 - PI * 45.0)).abs() < 2.0);
        // Far apart: no union, and subtracting does nothing.
        assert!(union(&b, &cube(30.0, 0.0, 5.0)).unwrap().is_none());
        assert_eq!(subtract(&b, &cube(30.0, 0.0, 5.0)).unwrap().len(), 1);
    }

    #[test]
    fn a_cut_can_split_a_body_and_intersect_works() {
        let slab = extrude_polygon(&[[4.0, -1.0], [6.0, -1.0], [6.0, 11.0], [4.0, 11.0]], 12.0).unwrap();
        let slab = builder::translated(&slab, Vector3::new(0.0, 0.0, -1.0));
        let parts = subtract(&cube(0.0, 0.0, 10.0), &slab).unwrap();
        assert_eq!(parts.len(), 2);
        let i = intersect(&cube(0.0, 0.0, 10.0), &cube(5.0, 5.0, 10.0)).unwrap();
        assert!((volume(&i[0]).unwrap() - 250.0).abs() < 0.5);
    }

    #[test]
    fn a_tool_in_a_notch_misses_the_body() {
        // The bounding boxes overlap, but the tool sits in the notch of an L.
        let l = extrude_polygon(
            &[
                [0.0, 0.0],
                [10.0, 0.0],
                [10.0, 2.0],
                [2.0, 2.0],
                [2.0, 10.0],
                [0.0, 10.0],
            ],
            5.0,
        )
        .unwrap();
        let notch = builder::translated(&cube(5.0, 0.0, 3.0), Vector3::new(0.0, 4.0, 0.0));
        let r = subtract(&l, &notch).unwrap();
        assert_eq!(r.len(), 1);
        assert!((volume(&r[0]).unwrap() - volume(&l).unwrap()).abs() < 1e-6);
        assert!(intersect(&l, &notch).unwrap().is_empty());
    }

    #[test]
    fn a_tool_inside_the_body_leaves_a_void() {
        let inner = builder::translated(&cube(2.0, 2.0, 2.0), Vector3::new(0.0, 2.0, 0.0));
        let r = subtract(&cube(0.0, 0.0, 10.0), &inner).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].boundaries().len(), 2);
        assert!((volume(&r[0]).unwrap() - 992.0).abs() < 0.01);
    }

    #[test]
    fn nested_solids_intersect_and_subtract_without_truck() {
        let inner = builder::translated(&cube(2.0, 2.0, 2.0), Vector3::new(0.0, 2.0, 0.0));
        let outer = cube(0.0, 0.0, 10.0);
        let i = intersect(&outer, &inner).unwrap();
        assert!((volume(&i[0]).unwrap() - 8.0).abs() < 0.01);
        assert!(subtract(&inner, &outer).unwrap().is_empty());
    }

    #[test]
    fn a_body_with_a_void_refuses_further_booleans() {
        let inner = builder::translated(&cube(2.0, 2.0, 2.0), Vector3::new(0.0, 2.0, 0.0));
        let hollow = subtract(&cube(0.0, 0.0, 10.0), &inner).unwrap().remove(0);
        let e = subtract(&hollow, &cyl(1.0, -1.0, 11.0)).unwrap_err();
        assert!(e.to_string().contains("void"), "{e}");
        // Far away it is simply left alone.
        assert_eq!(subtract(&hollow, &cube(30.0, 0.0, 5.0)).unwrap().len(), 1);
    }

    #[test]
    fn mirroring_keeps_volume_positive() {
        let m = [
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let s = transformed(&cube(1.0, 0.0, 2.0), m);
        assert!((volume(&s).unwrap() - 8.0).abs() < 1e-6);
        assert!(bbox(&s).1[0] <= -1.0 + 1e-9);
    }
}
