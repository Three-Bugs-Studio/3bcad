//! Fillet and chamfer tools for one edge: the corner region between the edge's two faces, swept
//! along a straight edge (prism) or turned about a circular edge's axis (revolution). Convex edges
//! subtract the tool, concave edges add it.

use std::f64::consts::TAU;

use anyhow::{bail, ensure, Result};

use crate::offset::{seg_end, seg_start};
use crate::operations::{ProfileRegion, ProfileSegment};
use crate::solid::Plane;
use crate::tool::Tool;
use crate::topology::{cross, dot, norm, sub, unit, EdgeCurve, TopoEdge, Topology, P3};

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

/// Cuts a closed profile off at `x = min`, keeping the part to the right of it.
fn clip_left(lp: Vec<ProfileSegment>, min: f64) -> Result<Vec<ProfileSegment>> {
    let cross = |a: P2, b: P2| [min, a[1] + (min - a[0]) / (b[0] - a[0]) * (b[1] - a[1])];
    let mut out = Vec::new();
    let (mut entry, mut exit) = (None, None);
    for s in lp {
        let (a, b) = (seg_start(&s), seg_end(&s));
        let (a_in, b_in) = (a[0] >= min, b[0] >= min);
        if !matches!(s, ProfileSegment::Line { .. }) {
            ensure!(a_in && b_in, "the size is too large: it reaches the axis of the edge");
            out.push(s);
            continue;
        }
        if !a_in && !b_in {
            continue;
        }
        let from = if a_in { a } else { cross(a, b) };
        let to = if b_in { b } else { cross(a, b) };
        if !a_in {
            match exit.take() {
                Some(e) => out.push(ProfileSegment::Line { from: e, to: from }),
                None => entry = Some(from),
            }
        }
        out.push(ProfileSegment::Line { from, to });
        if !b_in {
            exit = Some(to);
        }
    }
    if let (Some(from), Some(to)) = (exit, entry) {
        out.push(ProfileSegment::Line { from, to });
    }
    Ok(out)
}

/// A partial revolved tool runs this far (radians) past the arc, so its ends never lie on the arc's
/// end vertices.
const ARC_MARGIN: f64 = 0.01;

/// Angles of an arc's ends around `center`, from its midpoint, in the direction a tool revolves.
fn arc_ends(topo: &Topology, e: &TopoEdge, center: P3, axis: P3) -> (f64, f64) {
    let x = unit(sub(e.mid, center));
    let t = cross(axis, x);
    let angle = |v: usize| {
        let d = sub(topo.vertices[v], center);
        dot(d, t).atan2(dot(d, x))
    };
    let (a, b) = (angle(e.vertices[0]), angle(e.vertices[1]));
    (a.min(b), a.max(b))
}

/// Whether two arcs lie on the same circle. Each arc's circle is fitted on its own, and on a rim
/// the kernel approximates (after a boolean) the fits differ by a few µm.
pub fn same_circle(c1: P3, r1: f64, c2: P3, r2: f64) -> bool {
    let tol = 1e-5 * (1.0 + r1.max(r2));
    norm(sub(c1, c2)) < tol && (r1 - r2).abs() < tol
}

/// Whether the arcs on `edge`'s circle close the whole circle (a cylinder's rim is several arcs).
pub fn full_circle(topo: &Topology, edge: usize) -> bool {
    let EdgeCurve::Circle { center, axis, radius } = topo.edges[edge].curve else {
        return false;
    };
    let swept: f64 = topo
        .edges
        .iter()
        .filter(
            |e| matches!(e.curve, EdgeCurve::Circle { center: c, radius: r, .. } if same_circle(c, r, center, radius)),
        )
        .map(|e| {
            if e.vertices[0] == e.vertices[1] {
                return TAU;
            }
            let (a, b) = arc_ends(topo, e, center, axis);
            b - a
        })
        .sum();
    swept > TAU - 1e-3
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
            // Move the corner from the origin out to the edge's radius.
            for s in &mut lp {
                let shift = |p: &mut P2| p[0] += radius;
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
            // The tool's reach behind the corner must stop short of the axis.
            let lp = clip_left(lp, radius * 0.05)?;
            let (a0, a1) = if full_circle(topo, edge) {
                (0.0, TAU)
            } else {
                let (a, b) = arc_ends(topo, e, center, y);
                (a - ARC_MARGIN, b + ARC_MARGIN)
            };
            Tool::Revolution {
                regions: vec![ProfileRegion {
                    outer: lp,
                    holes: Vec::new(),
                }],
                plane: Plane { origin: center, x, y },
                axis: ([0.0, 0.0], [0.0, 1.0]),
                a0,
                a1,
            }
        }
        EdgeCurve::Other => bail!("only straight and circular edges can be filleted or chamfered"),
    };
    Ok(EdgeTool { tool, add: !convex })
}

/// How many differently built variable-fillet tools [`variable_fillet`] can make.
pub const VARIABLE_FILLET_TRIES: usize = 9;

/// A fillet tool whose radius changes along a straight edge: `radii` are `[position 0..1, radius]`
/// (sorted by position; see [`radius_law`] between them). Returns the tool solid and whether it adds material (concave edge).
///
/// truck's booleans often fail to intersect this skinned tool with the faces it grazes, and which
/// build it accepts is hard to predict, so `attempt` (0..[`VARIABLE_FILLET_TRIES`]) picks one of
/// several builds of the same fillet (the sections, the crossing margin and the run-outs past the
/// ends differ; the shape on the part does not):
/// the caller tries them in turn.
pub fn variable_fillet(
    topo: &Topology,
    edge: usize,
    radii: &[[f64; 2]],
    attempt: usize,
) -> Result<(truck_modeling::Solid, bool)> {
    use crate::skin::{self, Band, Caps, Ends};
    ensure!(!radii.is_empty(), "give the fillet at least one radius");
    let mut radii = radii.to_vec();
    radii.sort_by(|a, b| a[0].total_cmp(&b[0]));
    ensure!(radii.iter().all(|r| r[1] > 0.0), "fillet radii must be positive");
    let e = topo
        .edges
        .get(edge)
        .ok_or_else(|| anyhow::anyhow!("the edge no longer exists"))?;
    ensure!(e.sides.len() == 2, "the edge must be between two faces");
    let EdgeCurve::Line { a: start, b: end } = e.curve else {
        bail!("a variable fillet needs a straight edge");
    };
    let (a, b) = (&e.sides[0], &e.sides[1]);
    let convex = dot(a.inward, b.normal) < 0.0;
    let attempt = attempt % VARIABLE_FILLET_TRIES;
    let run_out = [RunOut::Clipped, RunOut::Eased, RunOut::Flat][attempt % 3];
    let (stations, cross_by) = [(12, CROSS), (24, CROSS), (12, 10.0 * CROSS)][attempt / 3];
    let r_at = radius_law(&radii, run_out);
    let max_r = radii.iter().map(|r| r[1]).fold(0.0, f64::max);
    let behind = if convex { 1.5 * max_r } else { 0.25 * max_r };
    let len = norm(sub(end, start));
    let dir = unit(sub(end, start));
    let x = unit(sub(a.inward, dir.map(|d| d * dot(a.inward, dir))));
    let y = cross(dir, x);
    let to2 = |v: P3| [dot(v, x), dot(v, y)];
    let m = if convex { behind } else { 0.0 };
    // Stations along the edge (in edge fractions, run-outs included): every control point and
    // enough in between.
    let out = m / len.max(1e-12);
    let mut ts: Vec<f64> = (0..=stations).map(|i| i as f64 / stations as f64).collect();
    ts.extend(radii.iter().map(|r| r[0].clamp(0.0, 1.0)));
    if out > 0.0 {
        ts.extend([-out, 1.0 + out]);
    }
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|p, q| (*p - *q).abs() < 1e-6);
    let mut sections = Vec::new();
    let mut params = Vec::new();
    for &along in &ts {
        let r = r_at(along);
        let (lp, reach) = corner_profile(
            to2(a.inward),
            to2(b.inward),
            Treatment::Fillet { radius: r },
            convex,
            behind,
            cross_by,
        )?;
        if (0.0..=1.0).contains(&along) {
            let mid = std::array::from_fn(|i| start[i] + dir[i] * along * len);
            check_reach(topo, mid, a, b, reach).or_else(|_| check_reach(topo, e.mid, a, b, reach))?;
        }
        let plane = Plane {
            origin: std::array::from_fn(|i| start[i] + dir[i] * along * len),
            x,
            y,
        };
        let mat = plane.matrix();
        sections.push(vec![skin::loop_pieces(&lp)
            .0
            .iter()
            .map(|p| p.transformed(&mat))
            .collect()]);
        params.push(along * len);
    }
    let solid = skin::skin(
        &[Band {
            sections,
            params,
            ends: Ends::default(),
        }],
        Caps::Planar,
    )?;
    Ok((solid, !convex))
}

/// How the radius continues past the ends of the edge, where the tool is outside the part.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RunOut {
    /// Keeps the end slope, clipped at the floor.
    Clipped,
    /// Keeps the end slope, easing off smoothly below the floor.
    Eased,
    /// Stays at the end radius.
    Flat,
}

/// The radius law through `points` ([fraction, radius], sorted): a C¹ piecewise cubic (Hermite,
/// with Fritsch–Carlson slopes at inner points so it does not overshoot between them). With two
/// points it is the straight ramp between them. Before the first point and after the last it stays
/// flat, unless that point is at the end of the edge: then (unless `run_out` is flat) it keeps the
/// end slope, down to half the smallest radius, so the tool has no kink where it crosses the face at
/// that end. A kink makes the interpolated tool overshoot, and truck then often cannot intersect it
/// with the faces. Past the ends the tool is outside the part, so `run_out` never changes the result.
fn radius_law(points: &[[f64; 2]], run_out: RunOut) -> impl Fn(f64) -> f64 + '_ {
    let n = points.len();
    let floor = 0.5 * points.iter().map(|r| r[1]).fold(f64::INFINITY, f64::min);
    let secant = |i: usize| (points[i + 1][1] - points[i][1]) / (points[i + 1][0] - points[i][0]).max(1e-12);
    let mut slopes = vec![0.0; n];
    if n > 1 {
        slopes[0] = secant(0);
        slopes[n - 1] = secant(n - 2);
    }
    for (i, slope) in slopes.iter_mut().enumerate().take(n.saturating_sub(1)).skip(1) {
        let (d0, d1) = (secant(i - 1), secant(i));
        // A local peak or valley is flat there; otherwise the harmonic mean keeps it monotone.
        *slope = if d0 * d1 <= 0.0 {
            0.0
        } else {
            2.0 / (1.0 / d0 + 1.0 / d1)
        };
    }
    let slope_start = run_out != RunOut::Flat && n > 1 && points[0][0] <= 1e-6;
    let slope_end = run_out != RunOut::Flat && n > 1 && points[n - 1][0] >= 1.0 - 1e-6;
    move |t: f64| {
        let r = if t <= points[0][0] {
            if slope_start {
                points[0][1] + slopes[0] * (t - points[0][0])
            } else {
                points[0][1]
            }
        } else if t >= points[n - 1][0] {
            if slope_end {
                points[n - 1][1] + slopes[n - 1] * (t - points[n - 1][0])
            } else {
                points[n - 1][1]
            }
        } else {
            let i = points.windows(2).position(|w| t <= w[1][0]).unwrap_or(n - 2);
            let ([t0, r0], [t1, r1]) = (points[i], points[i + 1]);
            let h = (t1 - t0).max(1e-12);
            let u = (t - t0) / h;
            let (u2, u3) = (u * u, u * u * u);
            (2.0 * u3 - 3.0 * u2 + 1.0) * r0
                + (u3 - 2.0 * u2 + u) * h * slopes[i]
                + (-2.0 * u3 + 3.0 * u2) * r1
                + (u3 - u2) * h * slopes[i + 1]
        };
        if r >= floor {
            r
        } else if run_out == RunOut::Eased {
            // Eases off exponentially instead of clipping: still smooth.
            floor * ((r - floor) / floor).exp()
        } else {
            floor
        }
    }
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
        // The face is measured on its mesh, whose chords sit up to `topo.tol` off a curved face.
        let tol = 1e-3 * (1.0 + reach) + topo.tol;
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
    fn a_wide_chamfer_keeps_its_size_near_the_axis() {
        // A chamfer of R5 by d removes a triangle of area d^2/2 at radius 5 - d/3.
        let cyl = extrude_circle([0.0, 0.0], 5.0, 10.0).unwrap();
        let topo = analyze(&cyl, 0.01);
        let e = topo
            .edges
            .iter()
            .position(|e| matches!(e.curve, EdgeCurve::Circle { center, .. } if (center[2] - 10.0).abs() < 1e-9))
            .expect("top circular edge");
        for d in [2.5, 3.0] {
            let c = edge_tool(&topo, e, Treatment::Chamfer { d1: d, d2: d }, false).unwrap();
            let removed = volume(&cyl).unwrap() - volume(&apply(&cyl, &c)).unwrap();
            let expected = d * d / 2.0 * TAU * (5.0 - d / 3.0);
            assert!(
                (removed - expected).abs() / expected < 0.01,
                "{d}: {removed} vs {expected}"
            );
        }
    }

    #[test]
    fn a_tube_rim_chamfers_at_the_apps_mesh_tolerance() {
        // Exercise 173: the app meshes at ~0.065 mm for a 130 mm part; a 1 mm chamfer on the
        // rim of a R17 tube with a R12.5 bore was refused as "does not fit".
        let tube = subtract(
            &extrude_circle([0.0, 0.0], 17.0, 76.0).unwrap(),
            &extrude_circle([0.0, 0.0], 12.5, 80.0).unwrap(),
        )
        .unwrap()
        .remove(0);
        let mut topo = analyze(&tube, 0.065);
        let rims: Vec<usize> = (0..topo.edges.len())
            .filter(
                |&i| matches!(topo.edges[i].curve, EdgeCurve::Circle { center, .. } if (center[2] - 76.0).abs() < 1e-6),
            )
            .collect();
        assert!(rims.len() >= 2, "outer and bore top rims");
        let chamfer = |topo: &Topology, d: f64| {
            rims.iter()
                .map(|&e| edge_tool(topo, e, Treatment::Chamfer { d1: d, d2: d }, false))
                .collect::<Result<Vec<_>>>()
        };
        for d in [1.0, 2.0] {
            chamfer(&topo, d).unwrap();
        }
        // Without the mesh allowance the bore rim is refused: the bug this guards.
        topo.tol = 0.0;
        assert!(chamfer(&topo, 1.0).is_err());
    }

    #[test]
    fn a_rim_of_separately_fitted_arcs_is_a_full_circle() {
        // Exercise 173: three 120° arcs of an R17.001 rim whose fitted centres and radii differ by
        // ~1.5 µm; only one third got chamfered.
        let r = 17.001;
        let at = |deg: f64| {
            let a = deg.to_radians();
            [r * a.cos(), r * a.sin(), 76.0]
        };
        let arc = |v: [usize; 2], mid: f64, center: P3, radius: f64| TopoEdge {
            curve: EdgeCurve::Circle {
                center,
                axis: [0.0, 0.0, 1.0],
                radius,
            },
            points: Vec::new(),
            length: 0.0,
            mid: at(mid),
            tangent: [0.0; 3],
            sides: Vec::new(),
            vertices: v,
        };
        let topo = Topology {
            vertices: vec![at(0.0), at(120.0), at(240.0)],
            edges: vec![
                arc([0, 1], 60.0, [-4.8e-7, 3.0e-7, 76.0], 17.0010005),
                arc([1, 2], 180.0, [1.08e-6, 0.0, 76.0], 17.0010011),
                arc([2, 0], 300.0, [0.0, -1.2e-6, 76.0], 17.0009995),
            ],
            ..Topology::default()
        };
        assert!((0..3).all(|e| full_circle(&topo, e)));
        // A concentric circle 0.1 mm larger is a different circle.
        assert!(!same_circle([0.0; 3], 17.0, [0.0; 3], 17.1));
    }

    #[test]
    fn chamfering_a_corner_arc_stays_on_the_corner() {
        // A 20 x 20 plate, 2 thick, with one rounded corner of radius 2 at (20, 20).
        let plate = ProfileRegion {
            outer: vec![
                ProfileSegment::Line {
                    from: [0.0, 0.0],
                    to: [20.0, 0.0],
                },
                ProfileSegment::Line {
                    from: [20.0, 0.0],
                    to: [20.0, 18.0],
                },
                ProfileSegment::Arc {
                    center: [18.0, 18.0],
                    from: [20.0, 18.0],
                    to: [18.0, 20.0],
                    ccw: true,
                },
                ProfileSegment::Line {
                    from: [18.0, 20.0],
                    to: [0.0, 20.0],
                },
                ProfileSegment::Line {
                    from: [0.0, 20.0],
                    to: [0.0, 0.0],
                },
            ],
            holes: Vec::new(),
        };
        let solid = Tool::Prism {
            regions: vec![plate],
            plane: Plane::TOP,
            z0: 0.0,
            z1: 2.0,
            draft: None,
        }
        .solids()
        .unwrap()
        .remove(0);
        let topo = analyze(&solid, 0.01);
        let e = topo
            .edges
            .iter()
            .position(|e| matches!(e.curve, EdgeCurve::Circle { center, .. } if (center[2] - 2.0).abs() < 1e-9))
            .expect("the top edge of the corner");
        assert!(!full_circle(&topo, e));
        let c = edge_tool(&topo, e, Treatment::Chamfer { d1: 0.5, d2: 0.5 }, false).unwrap();
        let removed = volume(&solid).unwrap() - volume(&apply(&solid, &c)).unwrap();
        // The triangle's area times the arc length through its centroid.
        let expected = 0.125 * (2.0 - 0.5 / 3.0) * PI / 2.0;
        assert!((removed - expected).abs() < 0.05, "{removed} vs {expected}");
    }

    #[test]
    // A fillet on a circular edge would need a torus tool, which truck-shapeops cannot intersect
    // tangentially; those are built directly by `round::round_circle` (tested there).
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

#[cfg(test)]
mod variable_tests {
    use super::*;
    use crate::boolean::{subtract, volume};
    use crate::operations::extrude_polygon;
    use crate::topology::analyze;

    #[test]
    fn a_variable_fillet_runs_from_one_radius_to_another() {
        let cube = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 10.0).unwrap();
        let topo = analyze(&cube, 0.05);
        let e = topo
            .edges
            .iter()
            .position(|e| matches!(e.curve, EdgeCurve::Line { a, b } if (a[0] - 10.0).abs() < 1e-9 && (b[0] - 10.0).abs() < 1e-9 && (a[2] - 10.0).abs() < 1e-9 && (b[2] - 10.0).abs() < 1e-9))
            .unwrap();
        let (tool, add) = variable_fillet(&topo, e, &[[0.0, 1.0], [1.0, 3.0]], 0).unwrap();
        assert!(!add);
        let out = subtract(&cube, &tool).unwrap();
        assert_eq!(out.len(), 1);
        let removed = 1000.0 - volume(&out[0]).unwrap();
        // (4 − π)/4 · r² integrated for r from 1 to 3 over 10 mm: (4 − π)/4 · 10 · 13/3.
        let expected = (4.0 - std::f64::consts::PI) / 4.0 * 10.0 * 13.0 / 3.0;
        assert!((removed - expected).abs() / expected < 0.03, "{removed} vs {expected}");
    }
}
