//! Feature tools: the solid a boss adds or a cut removes, kept as a description (profile, plane and
//! extent) instead of a finished B-Rep so patterns, mirrors, shells and drafts can rebuild it.

use std::f64::consts::TAU;

use anyhow::{bail, ensure, Result};
use truck_modeling::*;

use crate::boolean;
use crate::offset::{self, seg_end, seg_start};
use crate::operations::{self, ProfileRegion, ProfileSegment};
use crate::solid::Plane;

/// Side walls lean by `angles[i]` (radians, positive leans into the material as the extent grows),
/// one angle for every segment, or one for all of them; `neutral` is where walls keep their place.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub neutral: f64,
    pub angles: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tool {
    /// Profile swept along the plane normal from `z0` to `z1`.
    Prism {
        regions: Vec<ProfileRegion>,
        plane: Plane,
        z0: f64,
        z1: f64,
        draft: Option<Draft>,
    },
    /// Profile turned about the in-plane line through `axis.0` along `axis.1`, from angle `a0` to `a1`.
    Revolution {
        regions: Vec<ProfileRegion>,
        plane: Plane,
        axis: ([f64; 2], [f64; 2]),
        a0: f64,
        a1: f64,
    },
}

/// Which face of a tool a point lies on; segments are counted over every loop of every region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolFace {
    Start,
    End,
    Side(usize),
}

fn matrix(p: &Plane) -> Matrix4 {
    let m = p.matrix();
    Matrix4::from_cols(
        Vector4::from(m[0]),
        Vector4::from(m[1]),
        Vector4::from(m[2]),
        Vector4::from(m[3]),
    )
}

fn loops(regions: &[ProfileRegion]) -> impl Iterator<Item = &Vec<ProfileSegment>> {
    regions.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes))
}

fn segments(regions: &[ProfileRegion]) -> impl Iterator<Item = &ProfileSegment> {
    loops(regions).flatten()
}

fn fix_orientation(mut s: Solid) -> Result<Solid> {
    let v = boolean::volume(&s).ok_or_else(|| anyhow::anyhow!("the feature's shape cannot be meshed"))?;
    ensure!(v.abs() > 1e-9, "the feature has no volume");
    if v < 0.0 {
        s.not();
    }
    Ok(s)
}

fn dist_to_segment(p: [f64; 2], s: &ProfileSegment) -> f64 {
    let d = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    match *s {
        ProfileSegment::Line { from, to } => {
            let (ux, uy) = (to[0] - from[0], to[1] - from[1]);
            let l2 = (ux * ux + uy * uy).max(1e-300);
            let t = (((p[0] - from[0]) * ux + (p[1] - from[1]) * uy) / l2).clamp(0.0, 1.0);
            d(p, [from[0] + t * ux, from[1] + t * uy])
        }
        ProfileSegment::Arc { center, from, to, ccw } => {
            let r = d(from, center);
            let ang = |q: [f64; 2]| (q[1] - center[1]).atan2(q[0] - center[0]);
            let (a0, a1, ap) = (ang(from), ang(to), ang(p));
            let (s, e) = if ccw { (a0, a1) } else { (a1, a0) };
            let inside = (ap - s).rem_euclid(TAU) <= (e - s).rem_euclid(TAU);
            if inside {
                (d(p, center) - r).abs()
            } else {
                d(p, from).min(d(p, to))
            }
        }
        ProfileSegment::Circle { center, radius, .. } => (d(p, center) - radius).abs(),
        ProfileSegment::Bezier(b) => offset::sample_loop(&[s.clone()], 8)
            .iter()
            .chain(std::iter::once(&b[3]))
            .map(|q| d(p, *q))
            .fold(f64::INFINITY, f64::min),
    }
}

fn nearest_segment(regions: &[ProfileRegion], p: [f64; 2]) -> usize {
    segments(regions)
        .enumerate()
        .map(|(i, s)| (dist_to_segment(p, s), i))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(0, |(_, i)| i)
}

impl Tool {
    pub fn plane(&self) -> &Plane {
        match self {
            Tool::Prism { plane, .. } | Tool::Revolution { plane, .. } => plane,
        }
    }

    pub fn regions(&self) -> &[ProfileRegion] {
        match self {
            Tool::Prism { regions, .. } | Tool::Revolution { regions, .. } => regions,
        }
    }

    /// One solid per region.
    pub fn solids(&self) -> Result<Vec<Solid>> {
        ensure!(!self.regions().is_empty(), "nothing to build");
        match self {
            Tool::Prism {
                regions,
                plane,
                z0,
                z1,
                draft,
            } => {
                let (z0, z1) = (z0.min(*z1), z0.max(*z1));
                ensure!(z1 - z0 > TOLERANCE, "the extrusion has no depth");
                let drafted = draft.as_ref().filter(|d| d.angles.iter().any(|a| a.abs() > 1e-12));
                let Some(draft) = drafted else {
                    let mut p = *plane;
                    p.origin = plane.to_world_3d([0.0, 0.0], z0);
                    return operations::extrude(regions, z1 - z0, &p);
                };
                let total = segments(regions).count();
                let angle = |i: usize| {
                    if draft.angles.len() == total {
                        draft.angles[i]
                    } else {
                        draft.angles[0]
                    }
                };
                let at = |z: f64| -> Result<Vec<Vec<Vec<ProfileSegment>>>> {
                    let mut i = 0;
                    regions
                        .iter()
                        .map(|r| {
                            std::iter::once(&r.outer)
                                .chain(&r.holes)
                                .map(|lp| {
                                    let d: Vec<f64> = (0..lp.len())
                                        .map(|k| (z - draft.neutral) * angle(i + k).tan())
                                        .collect();
                                    i += lp.len();
                                    offset::offset_loop(lp, &d)
                                })
                                .collect()
                        })
                        .collect()
                };
                let (bottom, top) = (at(z0)?, at(z1)?);
                bottom
                    .iter()
                    .zip(&top)
                    .map(|(b, t)| {
                        let wb: Vec<Wire> = b.iter().map(|lp| operations::wire_at(lp, z0)).collect::<Result<_>>()?;
                        let wt: Vec<Wire> = t.iter().map(|lp| operations::wire_at(lp, z1)).collect::<Result<_>>()?;
                        let mut shell = Shell::new();
                        for (x, y) in wb.iter().zip(&wt) {
                            let side = builder::try_wire_homotopy(x, y)
                                .map_err(|e| anyhow::anyhow!("cannot build drafted walls: {e:?}"))?;
                            shell.extend(side);
                        }
                        let mut cap = builder::try_attach_plane(&wb)
                            .map_err(|e| anyhow::anyhow!("cannot build profile face: {e:?}"))?;
                        cap.invert();
                        shell.push(cap);
                        shell.push(
                            builder::try_attach_plane(&wt)
                                .map_err(|e| anyhow::anyhow!("cannot build profile face: {e:?}"))?,
                        );
                        ensure!(
                            shell.shell_condition() == ShellCondition::Closed,
                            "the draft angle folds the walls; use a smaller angle"
                        );
                        fix_orientation(builder::transformed(&Solid::new(vec![shell]), matrix(plane)))
                    })
                    .collect()
            }
            Tool::Revolution {
                regions,
                plane,
                axis: (o, u),
                a0,
                a1,
            } => {
                let ulen = u[0].hypot(u[1]);
                ensure!(ulen > 1e-12, "the revolve axis has no direction");
                let u = [u[0] / ulen, u[1] / ulen];
                let side = |p: [f64; 2]| (p[0] - o[0]) * u[1] - (p[1] - o[1]) * u[0];
                let pts: Vec<[f64; 2]> = loops(regions).flat_map(|lp| offset::sample_loop(lp, 64)).collect();
                let (lo, hi) = pts
                    .iter()
                    .map(|&p| side(p))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), s| (a.min(s), b.max(s)));
                ensure!(lo > -1e-9 || hi < 1e-9, "the profile crosses the revolve axis");
                let sweep = (a1 - a0).clamp(-TAU, TAU);
                ensure!(sweep.abs() > 1e-9, "the revolve angle must be non-zero");
                let origin = Point3::new(o[0], o[1], 0.0);
                let axis = Vector3::new(u[0], u[1], 0.0);
                regions
                    .iter()
                    .map(|r| {
                        let solid = revolve_region(r, origin, axis, *a0, sweep)?;
                        fix_orientation(builder::transformed(&solid, matrix(plane)))
                    })
                    .collect()
            }
        }
    }

    /// The tool moved by a rigid transform (rotation, translation or mirror), column-major.
    pub fn transformed(&self, m: [[f64; 4]; 4]) -> Tool {
        let lin = |v: [f64; 3]| -> [f64; 3] { std::array::from_fn(|r| (0..3).map(|c| m[c][r] * v[c]).sum()) };
        let pt = |v: [f64; 3]| -> [f64; 3] {
            let l = lin(v);
            std::array::from_fn(|r| l[r] + m[3][r])
        };
        let p = self.plane();
        let np = Plane {
            origin: pt(p.origin),
            x: lin(p.x),
            y: lin(p.y),
        };
        // A mirror flips the plane's normal relative to the tool: walk the extent backwards.
        let flipped = {
            let n = lin(p.normal());
            let nn = np.normal();
            n[0] * nn[0] + n[1] * nn[1] + n[2] * nn[2] < 0.0
        };
        match self.clone() {
            Tool::Prism {
                regions, z0, z1, draft, ..
            } => {
                if flipped {
                    Tool::Prism {
                        regions,
                        plane: np,
                        z0: -z1,
                        z1: -z0,
                        draft: draft.map(|d| Draft {
                            neutral: -d.neutral,
                            angles: d.angles.iter().map(|a| -a).collect(),
                        }),
                    }
                } else {
                    Tool::Prism {
                        regions,
                        plane: np,
                        z0,
                        z1,
                        draft,
                    }
                }
            }
            Tool::Revolution {
                regions, axis, a0, a1, ..
            } => {
                let (a0, a1) = if flipped { (-a1, -a0) } else { (a0, a1) };
                Tool::Revolution {
                    regions,
                    plane: np,
                    axis,
                    a0,
                    a1,
                }
            }
        }
    }

    /// The tool grown (`t > 0`) or shrunk on every side by `t`.
    pub fn offset(&self, t: f64) -> Result<Tool> {
        Ok(match self.clone() {
            Tool::Prism {
                regions,
                plane,
                z0,
                z1,
                draft,
            } => {
                let (z0, z1) = (z0.min(z1), z0.max(z1));
                Tool::Prism {
                    regions: offset::offset_regions(&regions, -t)?,
                    plane,
                    z0: z0 - t,
                    z1: z1 + t,
                    draft,
                }
            }
            Tool::Revolution {
                regions,
                plane,
                axis,
                a0,
                a1,
            } => {
                ensure!(
                    (a1 - a0).abs() >= TAU - 1e-9,
                    "only full revolutions can be offset (used by Shell)"
                );
                Tool::Revolution {
                    regions: offset::offset_regions(&regions, -t)?,
                    plane,
                    axis,
                    a0,
                    a1,
                }
            }
        })
    }

    /// Which tool face a world point lies on (the point must be on the tool's surface).
    pub fn face_at(&self, p: [f64; 3]) -> ToolFace {
        let plane = self.plane();
        let d = [p[0] - plane.origin[0], p[1] - plane.origin[1], p[2] - plane.origin[2]];
        let n = plane.normal();
        let w = d[0] * n[0] + d[1] * n[1] + d[2] * n[2];
        let local = plane.to_local(p);
        match self {
            Tool::Prism { regions, z0, z1, .. } => {
                let tol = 1e-5 * (1.0 + (z1 - z0).abs()) + 2.0 * boolean::GROW;
                if (w - z0.min(*z1)).abs() < tol {
                    ToolFace::Start
                } else if (w - z0.max(*z1)).abs() < tol {
                    ToolFace::End
                } else {
                    ToolFace::Side(nearest_segment(regions, local))
                }
            }
            Tool::Revolution {
                regions,
                axis: (o, u),
                a0,
                a1,
                ..
            } => {
                let ul = u[0].hypot(u[1]);
                let u = [u[0] / ul, u[1] / ul];
                // `s`: in-plane direction from the axis to the profile (angle 0); u × s is ±normal.
                let sign = regions
                    .iter()
                    .flat_map(|r| offset::sample_loop(&r.outer, 16))
                    .map(|q| (q[0] - o[0]) * u[1] - (q[1] - o[1]) * u[0])
                    .sum::<f64>()
                    .signum();
                let s = [u[1] * sign, -u[0] * sign];
                let us = u[0] * s[1] - u[1] * s[0];
                let rel = [local[0] - o[0], local[1] - o[1]];
                let h = rel[0] * u[0] + rel[1] * u[1];
                let along_s = rel[0] * s[0] + rel[1] * s[1];
                let theta = (w * us).atan2(along_s);
                let r = along_s.hypot(w);
                let full = (a1 - a0).abs() >= TAU - 1e-9;
                let near = |a: f64| {
                    ((theta - a + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0).abs() * r.max(1e-9)
                        < 1e-4 + 2.0 * boolean::GROW
                };
                if !full && near(*a0) {
                    return ToolFace::Start;
                }
                if !full && near(*a1) {
                    return ToolFace::End;
                }
                let q = [o[0] + u[0] * h + s[0] * r, o[1] + u[1] * h + s[1] * r];
                ToolFace::Side(nearest_segment(regions, q))
            }
        }
    }
}

/// Profile segment as a rational curve in the z = 0 plane: (control points, weights, knots, degree).
fn segment_nurbs(s: &ProfileSegment) -> Option<(Vec<Point3>, Vec<f64>, Vec<f64>, usize)> {
    match *s {
        ProfileSegment::Line { from, to } => Some((
            vec![Point3::new(from[0], from[1], 0.0), Point3::new(to[0], to[1], 0.0)],
            vec![1.0, 1.0],
            vec![0.0, 0.0, 1.0, 1.0],
            1,
        )),
        ProfileSegment::Arc { center, from, to, ccw } => {
            let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
            let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
            let mut sweep = (a1 - a0).rem_euclid(TAU);
            if !ccw {
                sweep -= TAU;
            }
            let r = (from[0] - center[0]).hypot(from[1] - center[1]);
            let (pts, w, k) = circle_net(sweep, |a| {
                Point3::new(center[0] + r * (a0 + a).cos(), center[1] + r * (a0 + a).sin(), 0.0)
            });
            Some((pts, w, k, 2))
        }
        _ => None,
    }
}

/// Rational quadratic control net of a circular arc of `sweep` radians: `at(angle)` gives the point
/// on the circle; middle control points sit on the tangent intersections (`at` scaled out).
fn circle_net(sweep: f64, at: impl Fn(f64) -> Point3) -> (Vec<Point3>, Vec<f64>, Vec<f64>) {
    let n = ((sweep.abs() / (std::f64::consts::PI / 2.0)).ceil() as usize).max(1);
    let d = sweep / n as f64;
    let wm = (d / 2.0).cos();
    let mut pts = vec![at(0.0)];
    let mut w = vec![1.0];
    let mut knots = vec![0.0; 3];
    for k in 0..n {
        let (s, m, e) = (d * k as f64, d * (k as f64 + 0.5), d * (k as f64 + 1.0));
        // The tangent intersection lies on the bisector at distance r / cos(d/2) from the center.
        let center = Point3::from_vec((at(s).to_vec() + at(s + std::f64::consts::PI).to_vec()) / 2.0);
        let mid = center + (at(m) - center) / wm;
        pts.push(mid);
        w.push(wm);
        pts.push(at(e));
        w.push(1.0);
        if k + 1 < n {
            knots.extend([(k + 1) as f64 / n as f64; 2]);
        }
    }
    knots.extend([1.0; 3]);
    (pts, w, knots)
}

/// A full circle as two half arcs, so every revolved edge has two distinct ends.
fn split_circles(lp: &[ProfileSegment]) -> Vec<ProfileSegment> {
    lp.iter()
        .flat_map(|s| match *s {
            ProfileSegment::Circle { center, radius, ccw } => {
                let a = [center[0] + radius, center[1]];
                let b = [center[0] - radius, center[1]];
                vec![
                    ProfileSegment::Arc {
                        center,
                        from: a,
                        to: b,
                        ccw,
                    },
                    ProfileSegment::Arc {
                        center,
                        from: b,
                        to: a,
                        ccw,
                    },
                ]
            }
            ref other => vec![other.clone()],
        })
        .collect()
}

/// Turns a region about the axis through `o` along `axis` from angle `a0` by `sweep`.
///
/// truck's own `rsweep` makes revolved-curve surfaces that truck-shapeops cannot intersect, so the
/// B-Rep is built here instead: every profile segment turned through each section (≤ 120°) is an
/// exact rational tensor-product surface (Piegl & Tiller's revolved surface), and faces share
/// their edges so the shell closes.
fn revolve_region(r: &ProfileRegion, o: Point3, axis: Vector3, a0: f64, sweep: f64) -> Result<Solid> {
    let axis = axis.normalize();
    let full = sweep.abs() >= TAU - 1e-9;
    let sections = ((sweep.abs() / (TAU / 3.0) - 1e-9).ceil() as usize).max(1);
    let step = sweep / sections as f64;
    let angles: Vec<f64> = (0..=sections).map(|j| a0 + step * j as f64).collect();
    let rot = |p: Point3, a: f64| {
        let m = Matrix4::from_translation(o.to_vec())
            * Matrix4::from_axis_angle(axis, Rad(a))
            * Matrix4::from_translation(-o.to_vec());
        m.transform_point(p)
    };
    let p3 = |p: [f64; 2]| Point3::new(p[0], p[1], 0.0);
    let nurbs_curve = |pts: &[Point3], w: &[f64], knots: &[f64]| {
        let cps = pts
            .iter()
            .zip(w)
            .map(|(p, &w)| Vector4::new(p.x * w, p.y * w, p.z * w, w))
            .collect();
        Curve::NurbsCurve(NurbsCurve::new(BSplineCurve::new(KnotVec::from(knots.to_vec()), cps)))
    };
    let loops: Vec<Vec<ProfileSegment>> = std::iter::once(&r.outer)
        .chain(&r.holes)
        .map(|lp| split_circles(lp))
        .collect();

    let mut shell = Shell::new();
    let mut start_cap = Vec::new();
    let mut end_cap = Vec::new();
    for lp in &loops {
        let n = lp.len();
        let nets: Vec<_> = lp
            .iter()
            .map(|s| segment_nurbs(s).ok_or_else(|| anyhow::anyhow!("splines cannot be revolved yet")))
            .collect::<Result<_>>()?;
        let dist = |p: [f64; 2]| {
            let d = p3(p) - o;
            (d - axis * d.dot(axis)).magnitude()
        };
        let on_axis: Vec<bool> = lp.iter().map(|s| dist(seg_start(s)) < 1e-9).collect();
        let lies_on_axis = |s: usize| on_axis[s] && on_axis[(s + 1) % n];
        // Vertex k (start of segment k) at each section angle; a full turn reuses the first ring and
        // a vertex on the axis is a single vertex (a cone tip or the center of a disc).
        let verts: Vec<Vec<Vertex>> = lp
            .iter()
            .zip(&on_axis)
            .map(|(s, &on)| {
                if on {
                    return vec![builder::vertex(p3(seg_start(s))); sections + 1];
                }
                let mut ring: Vec<Vertex> = angles[..sections]
                    .iter()
                    .map(|&a| builder::vertex(rot(p3(seg_start(s)), a)))
                    .collect();
                ring.push(if full {
                    ring[0].clone()
                } else {
                    builder::vertex(rot(p3(seg_start(s)), angles[sections]))
                });
                ring
            })
            .collect();
        // Arc traced by vertex k through section j (none for a vertex on the axis).
        let arcs: Vec<Vec<Option<Edge>>> = (0..n)
            .map(|k| {
                (0..sections)
                    .map(|j| {
                        (!on_axis[k]).then(|| {
                            let start = rot(p3(seg_start(&lp[k])), angles[j]);
                            let (pts, w, knots) = circle_net(step, |a| rot(start, a));
                            Edge::new(&verts[k][j], &verts[k][j + 1], nurbs_curve(&pts, &w, &knots))
                        })
                    })
                    .collect()
            })
            .collect();
        // Segment s turned to section boundary j; a segment lying on the axis stays one edge.
        let seams: Vec<Vec<Edge>> = (0..n)
            .map(|s| {
                if lies_on_axis(s) {
                    let line = Curve::Line(Line(p3(seg_start(&lp[s])), p3(seg_end(&lp[s]))));
                    return vec![Edge::new(&verts[s][0], &verts[(s + 1) % n][0], line); sections + 1];
                }
                let mut ring: Vec<Edge> = (0..sections + usize::from(!full))
                    .map(|j| {
                        let (pts, w, knots, _) = nets[s].clone();
                        let pts: Vec<Point3> = pts.iter().map(|p| rot(*p, angles[j])).collect();
                        let curve = match lp[s] {
                            ProfileSegment::Line { .. } => Curve::Line(Line(pts[0], pts[1])),
                            _ => nurbs_curve(&pts, &w, &knots),
                        };
                        Edge::new(&verts[s][j], &verts[(s + 1) % n][j], curve)
                    })
                    .collect();
                if full {
                    ring.push(ring[0].clone());
                }
                ring
            })
            .collect();
        for s in (0..n).filter(|&s| !lies_on_axis(s)) {
            let (cp, cw, ck, _) = &nets[s];
            for j in 0..sections {
                let mut knots_v = Vec::new();
                let rows: Vec<Vec<Vector4>> = cp
                    .iter()
                    .zip(cw)
                    .map(|(p, &wp)| {
                        let start = rot(*p, angles[j]);
                        let (pts, w, k) = circle_net(step, |a| rot(start, a));
                        knots_v = k;
                        pts.iter()
                            .zip(&w)
                            .map(|(q, &wq)| Vector4::new(q.x * wp * wq, q.y * wp * wq, q.z * wp * wq, wp * wq))
                            .collect()
                    })
                    .collect();
                let mut surface = Surface::NurbsSurface(NurbsSurface::new(BSplineSurface::new(
                    (KnotVec::from(ck.clone()), KnotVec::from(knots_v)),
                    rows,
                )));
                // A line across the axis sweeps a flat ring or disc: give it an exact plane, which
                // truck intersects far more reliably than a NURBS that collapses at the center.
                if let ProfileSegment::Line { from, to } = lp[s] {
                    let d = p3(to) - p3(from);
                    if d.dot(axis).abs() < 1e-12 * d.magnitude().max(1.0) {
                        let far = if dist(from) > dist(to) { from } else { to };
                        let p = rot(p3(far), angles[j]);
                        let radial = (p - o) - axis * (p - o).dot(axis);
                        let rot_dir = axis.cross(radial) * step.signum();
                        let dd = rot(p3(to), angles[j]) - rot(p3(from), angles[j]);
                        let n = dd.cross(rot_dir);
                        if n.magnitude2() > 0.0 {
                            let e1 = dd.normalize();
                            let e2 = n.normalize().cross(e1);
                            surface = Surface::Plane(truck_modeling::Plane::new(p, p + e1, p + e2));
                        }
                    }
                }
                let e = (s + 1) % n;
                let wire: Wire = [
                    Some(seams[s][j].clone()),
                    arcs[e][j].clone(),
                    Some(seams[s][j + 1].inverse()),
                    arcs[s][j].as_ref().map(Edge::inverse),
                ]
                .into_iter()
                .flatten()
                .collect();
                shell.push(Face::try_new(vec![wire], surface).map_err(|e| anyhow::anyhow!("revolve face: {e:?}"))?);
            }
        }
        if !full {
            start_cap.push((0..n).rev().map(|s| seams[s][0].inverse()).collect::<Wire>());
            end_cap.push((0..n).map(|s| seams[s][sections].clone()).collect::<Wire>());
        }
    }
    if !full {
        for wires in [start_cap, end_cap] {
            shell.push(builder::try_attach_plane(&wires).map_err(|e| anyhow::anyhow!("revolve cap: {e:?}"))?);
        }
    }
    ensure!(
        shell.shell_condition() == ShellCondition::Closed,
        "the revolved shape does not close"
    );
    Ok(Solid::new(vec![shell]))
}

/// A polygon region from points (any winding).
pub fn polygon(points: &[[f64; 2]]) -> Result<ProfileRegion> {
    ensure!(points.len() >= 3, "a polygon needs three points");
    let mut pts = points.to_vec();
    let area: f64 = pts
        .iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    if area.abs() < 1e-12 {
        bail!("the polygon has no area");
    }
    if area < 0.0 {
        pts.reverse();
    }
    Ok(ProfileRegion {
        outer: (0..pts.len())
            .map(|i| ProfileSegment::Line {
                from: pts[i],
                to: pts[(i + 1) % pts.len()],
            })
            .collect(),
        holes: Vec::new(),
    })
}

/// A circle region.
pub fn disc(center: [f64; 2], radius: f64) -> ProfileRegion {
    ProfileRegion {
        outer: vec![ProfileSegment::Circle {
            center,
            radius,
            ccw: true,
        }],
        holes: Vec::new(),
    }
}

/// Every segment's start and end, for callers that need the profile's corner points.
pub fn corners(regions: &[ProfileRegion]) -> Vec<[f64; 2]> {
    segments(regions).flat_map(|s| [seg_start(s), seg_end(s)]).collect()
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::mesher::print_mesh;

    fn vol(s: &Solid) -> f64 {
        boolean::volume(s).unwrap()
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> ProfileRegion {
        polygon(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]]).unwrap()
    }

    #[test]
    fn revolved_rectangle_is_a_tube_or_a_cylinder() {
        // Front plane: x across, z up; revolve about the z axis (local y).
        for (r0, expected) in [(3.0, PI * (25.0 - 9.0) * 10.0), (0.0, PI * 25.0 * 10.0)] {
            for (a0, a1) in [(0.0, TAU), (0.0, -TAU)] {
                let t = Tool::Revolution {
                    regions: vec![rect(r0, 0.0, 5.0, 10.0)],
                    plane: Plane::FRONT,
                    axis: ([0.0, 0.0], [0.0, 1.0]),
                    a0,
                    a1,
                };
                let s = &t.solids().unwrap()[0];
                assert!((vol(s) - expected).abs() / expected < 0.01, "{r0} {a1}: {}", vol(s));
                let m = print_mesh(s, 0.01);
                assert!(!m.triangles.is_empty());
            }
        }
        let quarter = Tool::Revolution {
            regions: vec![rect(3.0, 0.0, 5.0, 10.0)],
            plane: Plane::FRONT,
            axis: ([0.0, 0.0], [0.0, 1.0]),
            a0: 0.0,
            a1: PI / 2.0,
        };
        let s = &quarter.solids().unwrap()[0];
        assert!((vol(s) - PI * 160.0 / 4.0).abs() < 1.0);
        let crossing = Tool::Revolution {
            regions: vec![rect(-1.0, 0.0, 5.0, 10.0)],
            plane: Plane::FRONT,
            axis: ([0.0, 0.0], [0.0, 1.0]),
            a0: 0.0,
            a1: TAU,
        };
        assert!(crossing.solids().is_err());
    }

    #[test]
    fn drafted_prism_is_a_frustum() {
        // 10x10 square, 5 high, walls leaning in so the top is 8x8: tan(a) = 1/5.
        let t = Tool::Prism {
            regions: vec![rect(0.0, 0.0, 10.0, 10.0)],
            plane: Plane::TOP,
            z0: 0.0,
            z1: 5.0,
            draft: Some(Draft {
                neutral: 0.0,
                angles: vec![(0.2f64).atan()],
            }),
        };
        let s = &t.solids().unwrap()[0];
        let expected = 5.0 / 3.0 * (100.0 + 64.0 + 80.0);
        assert!((vol(s) - expected).abs() < 1e-3, "{}", vol(s));
        // A drafted circle is a cone frustum.
        let c = Tool::Prism {
            regions: vec![disc([0.0, 0.0], 4.0)],
            plane: Plane::TOP,
            z0: 0.0,
            z1: 3.0,
            draft: Some(Draft {
                neutral: 0.0,
                angles: vec![(2.0f64 / 3.0).atan()],
            }),
        };
        let expected = PI * 3.0 / 3.0 * (16.0 + 4.0 + 8.0);
        assert!((vol(&c.solids().unwrap()[0]) - expected).abs() / expected < 0.01);
    }

    #[test]
    fn mirrored_and_moved_tools_land_in_the_right_place() {
        let t = Tool::Prism {
            regions: vec![rect(1.0, 0.0, 3.0, 2.0)],
            plane: Plane::TOP,
            z0: 0.0,
            z1: 4.0,
            draft: None,
        };
        let mirror_x = [
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let m = t.transformed(mirror_x).solids().unwrap().remove(0);
        let (lo, hi) = boolean::bbox(&m);
        assert!(
            (lo[0] + 3.0).abs() < 1e-9
                && (hi[0] + 1.0).abs() < 1e-9
                && lo[2].abs() < 1e-9
                && (hi[2] - 4.0).abs() < 1e-9
        );
        assert!((vol(&m) - 16.0).abs() < 1e-6);
        let mirror_z = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let m = t.transformed(mirror_z).solids().unwrap().remove(0);
        let (lo, hi) = boolean::bbox(&m);
        assert!((lo[2] + 4.0).abs() < 1e-9 && hi[2].abs() < 1e-9);
        // Grown by 1: (2+2)x(2+2)x(4+2).
        let g = t.offset(1.0).unwrap().solids().unwrap().remove(0);
        assert!((vol(&g) - 96.0).abs() < 1e-6);
    }

    #[test]
    fn points_map_back_to_tool_faces() {
        let t = Tool::Prism {
            regions: vec![rect(0.0, 0.0, 10.0, 10.0)],
            plane: Plane::TOP,
            z0: 0.0,
            z1: 5.0,
            draft: None,
        };
        assert_eq!(t.face_at([5.0, 5.0, 0.0]), ToolFace::Start);
        assert_eq!(t.face_at([5.0, 5.0, 5.0]), ToolFace::End);
        assert_eq!(t.face_at([5.0, 0.0, 2.0]), ToolFace::Side(0));
        assert_eq!(t.face_at([10.0, 5.0, 2.0]), ToolFace::Side(1));
        let r = Tool::Revolution {
            regions: vec![rect(3.0, 0.0, 5.0, 10.0)],
            plane: Plane::FRONT,
            axis: ([0.0, 0.0], [0.0, 1.0]),
            a0: 0.0,
            a1: TAU,
        };
        // Outer wall (segment 1: x = 5) seen at 90 degrees around the axis.
        assert_eq!(r.face_at([0.0, 5.0, 4.0]), ToolFace::Side(1));
        assert_eq!(r.face_at([0.0, -3.0, 4.0]), ToolFace::Side(3));
        assert_eq!(r.face_at([4.0, 0.0, 10.0]), ToolFace::Side(2));
    }
}
