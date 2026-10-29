//! Union / subtract / intersect on top of `truck-shapeops`, made safe for a CAD rebuild: no panics
//! escape, results are sanity-checked by volume, and coplanar-face failures are retried with the
//! tool grown by a micron.

use std::panic::{catch_unwind, AssertUnwindSafe};

use anyhow::{anyhow, Result};
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

/// Volume meshed at `rel` x the bounding-box diagonal, clamped to `min..=max` mm.
fn volume_at(s: &Solid, rel: f64, min: f64, max: f64) -> Option<f64> {
    let (lo, hi) = bbox(s);
    let diag = (0..3).map(|i| (hi[i] - lo[i]).powi(2)).sum::<f64>().sqrt();
    let tol = (diag * rel).clamp(min, max);
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
    let s = checked(Op::Union, a, b)?;
    Ok((s.boundaries().len() < a.boundaries().len() + b.boundaries().len()).then_some(s))
}

/// `a − b`, split into its pieces (empty when nothing is left).
pub fn subtract(a: &Solid, b: &Solid) -> Result<Vec<Solid>> {
    if !overlaps(&bbox(a), &bbox(b), 0.0) {
        return Ok(vec![a.clone()]);
    }
    match checked(Op::Subtract, a, b) {
        Ok(s) => Ok(split(s)),
        Err(e) => {
            // The tool swallowed the body whole: nothing is left.
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
    }
}

/// `a ∩ b`, split into pieces.
pub fn intersect(a: &Solid, b: &Solid) -> Result<Vec<Solid>> {
    if !overlaps(&bbox(a), &bbox(b), 0.0) {
        return Ok(Vec::new());
    }
    Ok(split(checked(Op::Intersect, a, b)?))
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
