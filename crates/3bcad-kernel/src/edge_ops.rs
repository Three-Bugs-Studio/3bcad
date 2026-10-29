//! Fillet and chamfer tools for one edge: the corner region between the edge's two faces, swept
//! along a straight edge (prism) or turned about a circular edge's axis (revolution). Convex edges
//! subtract the tool, concave edges add it.

use std::f64::consts::TAU;

use anyhow::{bail, ensure, Result};

use crate::operations::{ProfileRegion, ProfileSegment};
use crate::solid::Plane;
use crate::tool::Tool;
use crate::topology::{cross, dot, norm, sub, unit, EdgeCurve, Topology, P3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Treatment {
    Fillet {
        radius: f64,
    },
    /// Distances along the first and second face.
    Chamfer {
        d1: f64,
        d2: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeTool {
    pub tool: Tool,
    /// Concave edge: material is added.
    pub add: bool,
}

/// Fillet tools are pulled a hair across the faces (radius ±0.1 %) so truck intersects them
/// instead of touching them tangentially.
pub const CROSS: f64 = 1e-3;

type P2 = [f64; 2];

fn v2add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn v2scale(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

fn v2unit(a: P2) -> P2 {
    let l = a[0].hypot(a[1]).max(1e-300);
    [a[0] / l, a[1] / l]
}

fn polygon_area(lp: &[ProfileSegment]) -> f64 {
    crate::offset::signed_area(lp)
}

fn reversed(lp: Vec<ProfileSegment>) -> Vec<ProfileSegment> {
    lp.into_iter()
        .rev()
        .map(|s| match s {
            ProfileSegment::Line { from, to } => ProfileSegment::Line { from: to, to: from },
            ProfileSegment::Arc { center, from, to, ccw } => ProfileSegment::Arc {
                center,
                from: to,
                to: from,
                ccw: !ccw,
            },
            other => other,
        })
        .collect()
}

/// The corner profile in a 2D frame where the edge is at the origin, face A runs along `ta` and
/// face B along `tb`. `inside_k` is how far the profile may reach behind the corner.
fn corner_profile(
    ta: P2,
    tb: P2,
    t: Treatment,
    convex: bool,
    behind: f64,
    cross_by: f64,
) -> Result<(Vec<ProfileSegment>, f64)> {
    let cos = (ta[0] * tb[0] + ta[1] * tb[1]).clamp(-1.0, 1.0);
    let theta = cos.acos();
    ensure!(
        theta > 1e-3 && theta < std::f64::consts::PI - 1e-3,
        "the faces at this edge are tangent; nothing to round"
    );
    let bis = v2unit(v2add(ta, tb));
    let q = v2scale(bis, -behind);
    let lp = match t {
        Treatment::Chamfer { d1, d2 } => {
            ensure!(d1 > 0.0 && d2 > 0.0, "chamfer distances must be positive");
            let p0 = v2scale(ta, d1);
            let p1 = v2scale(tb, d2);
            let u = v2unit([p0[0] - p1[0], p0[1] - p1[1]]);
            let m = behind;
            let p0e = v2add(p0, v2scale(u, m));
            let p1e = v2add(p1, v2scale(u, -m));
            vec![
                ProfileSegment::Line { from: p0e, to: p1e },
                ProfileSegment::Line { from: p1e, to: q },
                ProfileSegment::Line { from: q, to: p0e },
            ]
        }
        Treatment::Fillet { radius } => {
            ensure!(radius > 0.0, "fillet radius must be positive");
            let s = radius / (theta / 2.0).sin();
            let c = v2scale(bis, s);
            let rr = radius * if convex { 1.0 + cross_by } else { 1.0 - cross_by };
            let tdist = s * (theta / 2.0).cos();
            let on = |p: P2| v2add(c, v2scale(v2unit([p[0] - c[0], p[1] - c[1]]), rr));
            let (q0, q1) = (on(v2scale(ta, tdist)), on(v2scale(tb, tdist)));
            // The arc that bulges towards the corner.
            let ang = |p: P2| (p[1] - c[1]).atan2(p[0] - c[0]);
            let t_ang = ang(v2add(c, v2scale(bis, -1.0)));
            let ccw = (t_ang - ang(q0)).rem_euclid(TAU) < (ang(q1) - ang(q0)).rem_euclid(TAU);
            vec![
                ProfileSegment::Arc {
                    center: c,
                    from: q0,
                    to: q1,
                    ccw,
                },
                ProfileSegment::Line { from: q1, to: q },
                ProfileSegment::Line { from: q, to: q0 },
            ]
        }
    };
    let reach = match t {
        Treatment::Chamfer { d1, d2 } => d1.max(d2),
        Treatment::Fillet { radius } => radius / (theta / 2.0).tan(),
    };
    Ok((if polygon_area(&lp) < 0.0 { reversed(lp) } else { lp }, reach))
}

/// Tool that rounds or bevels `edge`; `flip` swaps which face gets `d1` for a chamfer.
pub fn edge_tool(topo: &Topology, edge: usize, t: Treatment, flip: bool) -> Result<EdgeTool> {
    edge_tool_crossing(topo, edge, t, flip, CROSS)
}

/// `edge_tool` with the fillet radius pushed `cross_by` (relative) across the faces; a larger value
/// helps truck with shallow intersections (curved faces) at the cost of a tiny step.
pub fn edge_tool_crossing(topo: &Topology, edge: usize, t: Treatment, flip: bool, cross_by: f64) -> Result<EdgeTool> {
    let e = topo
        .edges
        .get(edge)
        .ok_or_else(|| anyhow::anyhow!("the edge no longer exists"))?;
    ensure!(e.sides.len() == 2, "the edge must be between two faces");
    let (a, b) = if flip {
        (&e.sides[1], &e.sides[0])
    } else {
        (&e.sides[0], &e.sides[1])
    };
    let convex = dot(a.inward, b.normal) < 0.0;
    let size = match t {
        Treatment::Fillet { radius } => radius,
        Treatment::Chamfer { d1, d2 } => d1.max(d2),
    };
    // Convex corners reach into air, so a generous margin is harmless; concave ones reach into
    // material and must stay inside thin walls.
    let behind = if convex { 1.5 * size } else { 0.25 * size };
    let tool = match e.curve {
        EdgeCurve::Line { a: start, b: end } => {
            let dir = unit(sub(end, start));
            let x = unit(sub(a.inward, dir.map(|d| d * dot(a.inward, dir))));
            let y = cross(dir, x);
            let to2 = |v: P3| [dot(v, x), dot(v, y)];
            let (lp, reach) = corner_profile(to2(a.inward), to2(b.inward), t, convex, behind, cross_by)?;
            check_reach(topo, e.mid, a, b, reach)?;
            let len = norm(sub(end, start));
            let m = if convex { behind } else { 0.0 };
            Tool::Prism {
                regions: vec![ProfileRegion {
                    outer: lp,
                    holes: Vec::new(),
                }],
                plane: Plane { origin: start, x, y },
                z0: -m,
                z1: len + m,
                draft: None,
            }
        }
        EdgeCurve::Circle { center, axis, radius } => {
            let x = unit(sub(e.mid, center));
            let y = unit(sub(axis, x.map(|v| v * dot(axis, x))));
            let to2 = |v: P3| [dot(v, x), dot(v, y)];
            let (mut lp, reach) = corner_profile(to2(a.inward), to2(b.inward), t, convex, behind, cross_by)?;
            check_reach(topo, e.mid, a, b, reach)?;
            // Move the corner from the origin out to the edge's radius; stay off the axis.
            for s in &mut lp {
                let shift = |p: &mut P2| p[0] = (p[0] + radius).max(radius * 0.05);
                match s {
                    ProfileSegment::Line { from, to } => {
                        shift(from);
                        shift(to);
                    }
                    ProfileSegment::Arc { center, from, to, .. } => {
                        center[0] += radius;
                        shift(from);
                        shift(to);
                    }
                    _ => {}
                }
            }
            Tool::Revolution {
                regions: vec![ProfileRegion {
                    outer: lp,
                    holes: Vec::new(),
                }],
                plane: Plane { origin: center, x, y },
                axis: ([0.0, 0.0], [0.0, 1.0]),
                a0: 0.0,
                a1: TAU,
            }
        }
        EdgeCurve::Other => bail!("only straight and circular edges can be filleted or chamfered"),
    };
    Ok(EdgeTool { tool, add: !convex })
}

/// The rounded/bevelled band must fit on both faces.
fn check_reach(
    topo: &Topology,
    mid: P3,
    a: &crate::topology::EdgeSide,
    b: &crate::topology::EdgeSide,
    reach: f64,
) -> Result<()> {
    for s in [a, b] {
        let p = [0, 1, 2].map(|i| mid[i] + s.inward[i] * reach);
        let tol = 1e-3 * (1.0 + reach);
        ensure!(
            topo.faces[s.face].distance(p) < tol,
            "the size is too large: the {} mm band does not fit on face {}",
            (reach * 100.0).round() / 100.0,
            s.face + 1
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::boolean::{subtract, union, volume};
    use crate::operations::{extrude_circle, extrude_polygon};
    use crate::topology::analyze;

    fn apply(s: &truck_modeling::Solid, et: &EdgeTool) -> truck_modeling::Solid {
        let tool = &et.tool.solids().unwrap()[0];
        if et.add {
            union(s, tool).unwrap().unwrap()
        } else {
            subtract(s, tool).unwrap().remove(0)
        }
    }

    fn top_edge(topo: &Topology) -> usize {
        // An edge of the top face (z = 10) along y at x = 10.
        topo.edges
            .iter()
            .position(|e| matches!(e.curve, EdgeCurve::Line { a, b } if (a[0] - 10.0).abs() < 1e-9 && (b[0] - 10.0).abs() < 1e-9 && (a[2] - 10.0).abs() < 1e-9 && (b[2] - 10.0).abs() < 1e-9))
            .unwrap()
    }

    #[test]
    fn fillet_and_chamfer_a_box_edge() {
        let cube = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 10.0).unwrap();
        let topo = analyze(&cube, 0.05);
        let e = top_edge(&topo);
        let f = edge_tool(&topo, e, Treatment::Fillet { radius: 2.0 }, false).unwrap();
        assert!(!f.add);
        let v = volume(&apply(&cube, &f)).unwrap();
        let expected = 1000.0 - (4.0 - PI) * 10.0;
        assert!((v - expected).abs() < 0.2, "fillet {v} vs {expected}");
        let c = edge_tool(&topo, e, Treatment::Chamfer { d1: 2.0, d2: 3.0 }, false).unwrap();
        let v = volume(&apply(&cube, &c)).unwrap();
        assert!((v - (1000.0 - 3.0 * 10.0)).abs() < 1e-3, "chamfer {v}");
        assert!(edge_tool(&topo, e, Treatment::Fillet { radius: 12.0 }, false)
            .unwrap_err()
            .to_string()
            .contains("too large"));
    }

    #[test]
    // shortcut: a fillet on a circular edge (torus tool) still fails inside truck-shapeops; the core
    // reports it as a feature error. Add the fillet case here once truck intersects tori reliably.
    fn chamfer_a_circular_edge_of_a_cylinder() {
        let cyl = extrude_circle([0.0, 0.0], 5.0, 10.0).unwrap();
        let topo = analyze(&cyl, 0.01);
        let e = topo
            .edges
            .iter()
            .position(|e| {
                matches!(e.curve, EdgeCurve::Circle { center, .. } if (center[2] - 10.0).abs() < 1e-9)
                    && e.sides.len() == 2
            })
            .expect("top circular edge");
        let c = edge_tool(&topo, e, Treatment::Chamfer { d1: 1.0, d2: 1.0 }, false).unwrap();
        let v = volume(&apply(&cyl, &c)).unwrap();
        let ring = 0.5 * TAU * (5.0 - 1.0 / 3.0);
        assert!((v - (PI * 250.0 - ring)).abs() < 0.005 * v, "chamfer {v}");
    }

    #[test]
    fn concave_edge_gets_material_added() {
        // L-bracket: inner corner along y at x = 2, z = 2.
        let l = extrude_polygon(
            &[
                [0.0, 0.0],
                [10.0, 0.0],
                [10.0, 2.0],
                [2.0, 2.0],
                [2.0, 10.0],
                [0.0, 10.0],
            ],
            10.0,
        )
        .unwrap();
        // That profile is on the XY plane; the concave edge runs along z at (2, 2).
        let topo = analyze(&l, 0.05);
        let e = topo
            .edges
            .iter()
            .position(|e| matches!(e.curve, EdgeCurve::Line { a, b } if (a[0] - 2.0).abs() < 1e-9 && (a[1] - 2.0).abs() < 1e-9 && (b[0] - 2.0).abs() < 1e-9))
            .unwrap();
        let c = edge_tool(&topo, e, Treatment::Chamfer { d1: 1.0, d2: 1.0 }, false).unwrap();
        assert!(c.add);
        let v = volume(&apply(&l, &c)).unwrap();
        assert!((v - (36.0 * 10.0 + 0.5 * 10.0)).abs() < 0.05, "{v}");
    }
}
