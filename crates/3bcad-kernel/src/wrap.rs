//! Wrapping sketch outlines onto a cylinder: the sketch is rolled onto the surface without
//! stretching (its x along the circumference, its y along the axis) and the outline is grown
//! radially into a solid between two radii, which the feature adds (emboss) or cuts (deboss).
//!
//! The solid is built directly: its two caps are patches of exact cylinders trimmed by the
//! wrapped outline, its walls ruled surfaces between the outline at the two radii.

use std::f64::consts::TAU;

use anyhow::{bail, ensure, Result};
use truck_modeling::*;

use crate::operations::ProfileRegion;
use crate::round::oriented;
use crate::skin::{self, arc_piece, Piece};
use crate::solid::Plane;
use crate::topology::{cross, dot, norm, sub, unit, P3};

/// A cylinder: a point on its axis, the unit axis and the radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cylinder {
    pub center: P3,
    pub axis: P3,
    pub radius: f64,
}

struct Map {
    c: P3,
    a: P3,
    /// From the axis towards the sketch.
    e0: P3,
    /// Around the cylinder (increasing angle).
    x: P3,
    r: f64,
}

impl Map {
    /// (arc length around, along the axis) of a world point on the sketch plane.
    fn flat(&self, p: P3) -> [f64; 2] {
        let d = sub(p, self.c);
        [dot(d, self.x), dot(d, self.a)]
    }

    fn at(&self, q: [f64; 2], rho: f64) -> P3 {
        let phi = q[0] / self.r;
        let (s, c) = phi.sin_cos();
        std::array::from_fn(|i| self.c[i] + self.a[i] * q[1] + rho * (c * self.e0[i] + s * self.x[i]))
    }

    fn normal(&self, q: [f64; 2]) -> P3 {
        let phi = q[0] / self.r;
        let (s, c) = phi.sin_cos();
        std::array::from_fn(|i| c * self.e0[i] + s * self.x[i])
    }
}

fn pt(p: P3) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn curve(p: &Piece) -> Curve {
    p.curve()
}

/// The solid of `regions` (in `plane`) wrapped onto `cyl` between radii `inner` and `outer`.
pub fn wrap(regions: &[ProfileRegion], plane: &Plane, cyl: &Cylinder, inner: f64, outer: f64) -> Result<Vec<Solid>> {
    ensure!(!regions.is_empty(), "the sketch has no closed outline to wrap");
    ensure!(outer > inner && inner > 0.0, "the wrap depth does not fit the cylinder");
    let a = unit(cyl.axis);
    ensure!(
        dot(plane.normal(), a).abs() < 1e-3,
        "sketch the wrap on a plane parallel to the cylinder's axis"
    );
    let n = plane.normal();
    // From the axis towards the sketch plane.
    let towards = sub(plane.origin, cyl.center);
    let side = if dot(towards, n) >= 0.0 { 1.0 } else { -1.0 };
    let e0 = unit(n.map(|v| v * side));
    let map = Map {
        c: cyl.center,
        a,
        e0,
        x: cross(a, e0),
        r: cyl.radius,
    };
    let to_world = plane.matrix();
    // Is the sketch's own winding kept by the roll-out?
    let flip = {
        let (u, v) = (
            map.flat(plane.to_world([1.0, 0.0])),
            map.flat(plane.to_world([0.0, 1.0])),
        );
        let o = map.flat(plane.origin);
        let (u, v) = ([u[0] - o[0], u[1] - o[1]], [v[0] - o[0], v[1] - o[1]]);
        u[0] * v[1] - u[1] * v[0] < 0.0
    };
    let mut out = Vec::new();
    for r in regions {
        // Loops as pieces in the rolled-out (arc length, axial) frame, the region on their left.
        let loops: Vec<Vec<Piece>> = std::iter::once(&r.outer)
            .chain(&r.holes)
            .map(|lp| {
                let pcs: Vec<Piece> = skin::loop_pieces(lp)
                    .0
                    .iter()
                    .map(|p| p.transformed(&to_world))
                    .collect();
                if flip {
                    pcs.iter()
                        .rev()
                        .map(|p| {
                            let (k0, k1) = (p.knots[0], *p.knots.last().unwrap());
                            Piece {
                                cps: p.cps.iter().rev().copied().collect(),
                                knots: p.knots.iter().rev().map(|k| k0 + k1 - k).collect(),
                            }
                        })
                        .collect()
                } else {
                    pcs
                }
            })
            .collect();
        let flat_of = |p: &Piece, t: f64| -> [f64; 2] {
            let (k0, k1) = (p.knots[0], *p.knots.last().unwrap());
            map.flat(p.eval(k0 + (k1 - k0) * t))
        };
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for lp in &loops {
            for p in lp {
                for i in 0..=16 {
                    let q = flat_of(p, i as f64 / 16.0);
                    for d in 0..2 {
                        lo[d] = lo[d].min(q[d]);
                        hi[d] = hi[d].max(q[d]);
                    }
                }
            }
        }
        ensure!(
            (hi[0] - lo[0]) / cyl.radius < TAU - 0.05,
            "the sketch is wider than the cylinder's circumference"
        );
        // Vertices and edges at both radii.
        let mut shell: Vec<Face> = Vec::new();
        let mut cap_wires: [Vec<Wire>; 2] = [Vec::new(), Vec::new()];
        for lp in &loops {
            let starts: Vec<[f64; 2]> = lp.iter().map(|p| flat_of(p, 0.0)).collect();
            let vin: Vec<Vertex> = starts.iter().map(|&q| builder::vertex(pt(map.at(q, inner)))).collect();
            let vout: Vec<Vertex> = starts.iter().map(|&q| builder::vertex(pt(map.at(q, outer)))).collect();
            let k = lp.len();
            let radial: Vec<Edge> = (0..k)
                .map(|i| Edge::new(&vin[i], &vout[i], Curve::Line(Line(vin[i].point(), vout[i].point()))))
                .collect();
            let mut ein = Vec::new();
            let mut eout = Vec::new();
            for (i, p) in lp.iter().enumerate() {
                // Enough spans that the fitted edges lie on the caps (a whole circle needs many more
                // than a short line); both radii share them, for the ruled wall between.
                let fit = |rho: f64, spans: usize| skin::fit_cubic(&|t| map.at(flat_of(p, t), rho), spans);
                let mut spans = 8;
                while spans < 256 {
                    let c = fit(outer, spans);
                    let n = 4 * spans;
                    let err = (0..n)
                        .map(|i| {
                            let t = (i as f64 + 0.5) / n as f64;
                            norm(sub(c.eval(t), map.at(flat_of(p, t), outer)))
                        })
                        .fold(0.0, f64::max);
                    if err < 1e-5 * (1.0 + outer) {
                        break;
                    }
                    spans *= 2;
                }
                let (ci, co) = (fit(inner, spans), fit(outer, spans));
                let j = (i + 1) % k;
                let e_in = Edge::new(&vin[i], &vin[j], curve(&ci));
                let e_out = Edge::new(&vout[i], &vout[j], curve(&co));
                // The wall: ruled between the two.
                let rows: Vec<Vec<Vector4>> = ci
                    .cps
                    .iter()
                    .zip(&co.cps)
                    .map(|(x, y)| {
                        vec![
                            Vector4::new(x[0], x[1], x[2], x[3]),
                            Vector4::new(y[0], y[1], y[2], y[3]),
                        ]
                    })
                    .collect();
                let surface = Surface::NurbsSurface(NurbsSurface::new(BSplineSurface::new(
                    (KnotVec::from(ci.knots.clone()), KnotVec::from(vec![0.0, 0.0, 1.0, 1.0])),
                    rows,
                )));
                let wire: Wire = vec![e_in.clone(), radial[j].clone(), e_out.inverse(), radial[i].inverse()].into();
                let mid = flat_of(p, 0.5);
                let tangent = {
                    let (q0, q1) = (flat_of(p, 0.49), flat_of(p, 0.51));
                    unit(sub(map.at(q1, outer), map.at(q0, outer)))
                };
                let outward = cross(tangent, map.normal(mid));
                shell.push(oriented(vec![wire], surface, |_| outward)?);
                ein.push(e_in);
                eout.push(e_out);
            }
            // Outer cap: the loop as it runs; inner cap: backwards (it faces the axis).
            cap_wires[1].push(eout.into_iter().collect());
            cap_wires[0].push(ein.iter().rev().map(|e| e.inverse()).collect());
        }
        let margin = 0.02 * (hi[0] - lo[0] + hi[1] - lo[1]) + 1e-3;
        for (side, rho) in [(0usize, inner), (1, outer)] {
            let a0 = (lo[0] - margin) / cyl.radius;
            let sweep = (hi[0] - lo[0] + 2.0 * margin) / cyl.radius;
            let ring = |s: f64| -> Piece {
                let c: P3 = std::array::from_fn(|i| map.c[i] + map.a[i] * s);
                arc_piece(c, map.e0, map.x, rho, a0, sweep)
            };
            let (r0, r1) = (ring(lo[1] - margin), ring(hi[1] + margin));
            let rows: Vec<Vec<Vector4>> = r0
                .cps
                .iter()
                .zip(&r1.cps)
                .map(|(x, y)| {
                    vec![
                        Vector4::new(x[0], x[1], x[2], x[3]),
                        Vector4::new(y[0], y[1], y[2], y[3]),
                    ]
                })
                .collect();
            let surface = Surface::NurbsSurface(NurbsSurface::new(BSplineSurface::new(
                (KnotVec::from(r0.knots.clone()), KnotVec::from(vec![0.0, 0.0, 1.0, 1.0])),
                rows,
            )));
            let wires = std::mem::take(&mut cap_wires[side]);
            let sign = if side == 1 { 1.0 } else { -1.0 };
            let c = map.c;
            let a = map.a;
            shell.push(oriented(wires, surface, |p| {
                let d = sub(p, c);
                let radial = sub(d, a.map(|v| v * dot(d, a)));
                unit(radial).map(|v| v * sign)
            })?);
        }
        let shell: Shell = shell.into();
        if shell.shell_condition() != ShellCondition::Closed {
            bail!("the wrapped outline does not close");
        }
        let solid = Solid::try_new(vec![shell]).map_err(|e| anyhow::anyhow!("the wrapped shape is invalid: {e:?}"))?;
        ensure!(norm(sub(map.at([0.0, 0.0], inner), map.c)) > 0.0, "bad cylinder");
        out.push(solid);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::volume;
    use crate::mesher::print_mesh;
    use crate::tool::polygon;

    #[test]
    fn a_wrapped_square_is_a_curved_tile() {
        // Rod of radius 10 along z; the sketch on the plane x = 10 (tangent to the rod).
        let plane = Plane {
            origin: [10.0, 0.0, 0.0],
            x: [0.0, 1.0, 0.0],
            y: [0.0, 0.0, 1.0],
        };
        let sq = polygon(&[[-3.0, 2.0], [3.0, 2.0], [3.0, 8.0], [-3.0, 8.0]]).unwrap();
        let cyl = Cylinder {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            radius: 10.0,
        };
        let s = wrap(&[sq], &plane, &cyl, 10.0, 11.0).unwrap().remove(0);
        // The tile spans 6 mm of arc at r = 10: angle 0.6; volume (r1² − r0²)/2 · angle · height.
        let expected = (121.0 - 100.0) / 2.0 * 0.6 * 6.0;
        let v = volume(&s).unwrap();
        assert!((v - expected).abs() / expected < 1e-2, "{v} vs {expected}");
        assert!(!print_mesh(&s, 0.01).triangles.is_empty());
        let (lo, hi) = crate::boolean::bbox(&s);
        assert!(
            (lo[2] - 2.0).abs() < 1e-3 && (hi[2] - 8.0).abs() < 1e-3,
            "{lo:?} {hi:?}"
        );
    }

    #[test]
    fn a_ring_with_a_hole_wraps_too() {
        let plane = Plane {
            origin: [0.0, -10.0, 0.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 0.0, 1.0],
        };
        let mut r = crate::tool::disc([0.0, 5.0], 3.0);
        r.holes.push(vec![crate::operations::ProfileSegment::Circle {
            center: [0.0, 5.0],
            radius: 1.0,
            ccw: false,
        }]);
        let cyl = Cylinder {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            radius: 10.0,
        };
        let s = wrap(&[r], &plane, &cyl, 9.5, 10.2).unwrap().remove(0);
        let v = volume(&s).unwrap();
        // Area 8π at about r = 9.85 over 0.7 mm.
        let expected = 8.0 * std::f64::consts::PI * 0.7 * (9.85 / 10.0);
        assert!((v - expected).abs() / expected < 2e-2, "{v} vs {expected}");
    }
}
