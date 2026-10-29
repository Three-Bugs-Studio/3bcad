//! Evaluating the feature tree into bodies. Each solid feature turns into kernel tools (a prism or
//! a revolution) that add to, cut from or intersect the bodies; the steps are recorded per body so
//! patterns, mirrors, Shell and Draft can replay or move them.

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::Arc;

use anyhow::{anyhow, bail, ensure, Result};
use threebcad_kernel::boolean::{self, GROW};
use threebcad_kernel::edge_ops::{self, Treatment};
use threebcad_kernel::operations::{ProfileRegion, ProfileSegment};
use threebcad_kernel::topology::{self, cross, dot, norm, sub, unit, EdgeCurve, Topology, P3};
use threebcad_kernel::{offset, Draft, Plane, Solid, Tool, ToolFace};
use threebcad_sketch::{Geometry, Segment, Sketch, Status};

use crate::feature::*;
use crate::holes::{self, ThreadSpec};
use crate::naming::{self, FaceTag, Named};
use crate::{FeatureId, History};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Add,
    Sub,
    /// Keep only what is inside the tool (cut with "flip side to cut").
    Keep,
}

/// One tool applied to a body.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub feature: FeatureId,
    pub instance: u32,
    pub op: OpKind,
    pub tool: Tool,
}

#[derive(Clone, Debug)]
pub struct Body {
    pub solid: Solid,
    pub topo: Arc<Topology>,
    /// Persistent name of each face (parallel to `topo.faces`).
    pub tags: Vec<FaceTag>,
    /// How the body was made, oldest first.
    pub csg: Vec<Step>,
}

impl Named for Body {
    fn topo(&self) -> &Topology {
        &self.topo
    }

    fn tags(&self) -> &[FaceTag] {
        &self.tags
    }
}

/// Result of evaluating the tree up to the rollback bar.
#[derive(Clone, Default)]
pub struct Rebuild {
    pub bodies: Vec<Body>,
    pub errors: HashMap<FeatureId, String>,
    pub warnings: HashMap<FeatureId, String>,
    /// Definition status and remaining degrees of freedom of each built sketch.
    pub sketch_status: HashMap<FeatureId, (Status, usize)>,
    /// Reference planes and coordinate system frames.
    pub planes: HashMap<FeatureId, Plane>,
    /// Reference axes: point and unit direction.
    pub axes: HashMap<FeatureId, (P3, P3)>,
    pub points: HashMap<FeatureId, P3>,
    /// The steps each solid feature applied (patterns and mirrors replay them).
    pub ops: HashMap<FeatureId, Vec<Step>>,
    pub threads: HashMap<FeatureId, ThreadSpec>,
    /// Features evaluated in this rebuild (the rest came from the cache).
    pub evaluated: usize,
}

/// One cached step of a rebuild: the feature as it was evaluated and the state after it.
#[derive(Clone)]
pub struct CacheEntry {
    id: FeatureId,
    feature: crate::Feature,
    suppressed: bool,
    after: Rebuild,
}

/// Tessellation tolerance used for naming and measuring (mm).
fn topo_tol(s: &Solid) -> f64 {
    let (lo, hi) = boolean::bbox(s);
    let diag = norm(sub(hi, lo));
    (diag * 5e-4).clamp(0.01, 0.2)
}

fn analyze(s: &Solid) -> Arc<Topology> {
    Arc::new(topology::analyze(s, topo_tol(s)))
}

/// A face's plane with a stable origin (the world origin projected onto it), so sketches on it
/// keep their coordinates when the face changes size.
pub fn stable_plane(p: &Plane) -> Plane {
    let n = p.normal();
    let o = p.origin;
    let d = dot(o, n);
    Plane::from_origin_normal(n.map(|v| v * d), n)
}

fn to_kernel(s: &Segment) -> ProfileSegment {
    match *s {
        Segment::Line { from, to } => ProfileSegment::Line { from, to },
        Segment::Arc { center, from, to, ccw } => ProfileSegment::Arc { center, from, to, ccw },
        Segment::Circle { center, radius, ccw } => ProfileSegment::Circle { center, radius, ccw },
        Segment::Bezier(b) => ProfileSegment::Bezier(b),
    }
}

fn regions_of(s: &Sketch) -> Result<Vec<ProfileRegion>> {
    Ok(s.regions()
        .map_err(anyhow::Error::msg)?
        .iter()
        .map(|r| ProfileRegion {
            outer: r.outer.iter().map(to_kernel).collect(),
            holes: r.holes.iter().map(|h| h.iter().map(to_kernel).collect()).collect(),
        })
        .collect())
}

/// Open contours of a sketch (non-construction), joined end to end.
fn chains_of(s: &Sketch) -> Vec<Vec<ProfileSegment>> {
    let mut pieces: Vec<Vec<ProfileSegment>> = s
        .entity_ids()
        .filter(|&e| !s.entity(e).construction)
        .map(|e| s.segments(e).iter().map(to_kernel).collect::<Vec<_>>())
        .filter(|v: &Vec<_>| !v.is_empty())
        .collect();
    let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6;
    let flip = |c: &[ProfileSegment]| -> Vec<ProfileSegment> {
        c.iter()
            .rev()
            .map(|s| match *s {
                ProfileSegment::Line { from, to } => ProfileSegment::Line { from: to, to: from },
                ProfileSegment::Arc { center, from, to, ccw } => ProfileSegment::Arc {
                    center,
                    from: to,
                    to: from,
                    ccw: !ccw,
                },
                ProfileSegment::Bezier([a, b, c, d]) => ProfileSegment::Bezier([d, c, b, a]),
                ref other => other.clone(),
            })
            .collect()
    };
    let mut out = Vec::new();
    while let Some(mut chain) = pieces.pop() {
        loop {
            let (start, end) = (offset::seg_start(&chain[0]), offset::seg_end(chain.last().unwrap()));
            let found = pieces.iter().position(|p| {
                let (ps, pe) = (offset::seg_start(&p[0]), offset::seg_end(p.last().unwrap()));
                close(ps, end) || close(pe, end) || close(pe, start) || close(ps, start)
            });
            let Some(i) = found else { break };
            let p = pieces.remove(i);
            let (ps, pe) = (offset::seg_start(&p[0]), offset::seg_end(p.last().unwrap()));
            if close(ps, end) {
                chain.extend(p);
            } else if close(pe, end) {
                chain.extend(flip(&p));
            } else if close(pe, start) {
                chain.splice(0..0, p);
            } else {
                chain.splice(0..0, flip(&p));
            }
        }
        out.push(chain);
    }
    out
}

fn mat_translate(t: P3) -> [[f64; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

/// Rotation by `angle` about the line through `p` along unit `d` (column-major).
fn mat_rotate(p: P3, d: P3, angle: f64) -> [[f64; 4]; 4] {
    let (s, c) = angle.sin_cos();
    let [x, y, z] = d;
    let r = [
        [
            c + x * x * (1.0 - c),
            x * y * (1.0 - c) - z * s,
            x * z * (1.0 - c) + y * s,
        ],
        [
            y * x * (1.0 - c) + z * s,
            c + y * y * (1.0 - c),
            y * z * (1.0 - c) - x * s,
        ],
        [
            z * x * (1.0 - c) - y * s,
            z * y * (1.0 - c) + x * s,
            c + z * z * (1.0 - c),
        ],
    ];
    let rp: P3 = std::array::from_fn(|i| (0..3).map(|j| r[i][j] * p[j]).sum());
    let t: P3 = std::array::from_fn(|i| p[i] - rp[i]);
    [
        [r[0][0], r[1][0], r[2][0], 0.0],
        [r[0][1], r[1][1], r[2][1], 0.0],
        [r[0][2], r[1][2], r[2][2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

/// Reflection through a plane.
fn mat_mirror(p: &Plane) -> [[f64; 4]; 4] {
    let n = unit(p.normal());
    let d = dot(p.origin, n);
    let r: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| f64::from(i == j) - 2.0 * n[i] * n[j]));
    [
        [r[0][0], r[1][0], r[2][0], 0.0],
        [r[0][1], r[1][1], r[2][1], 0.0],
        [r[0][2], r[1][2], r[2][2], 0.0],
        [2.0 * d * n[0], 2.0 * d * n[1], 2.0 * d * n[2], 1.0],
    ]
}

fn apply_mat(m: &[[f64; 4]; 4], p: P3) -> P3 {
    std::array::from_fn(|r| (0..3).map(|c| m[c][r] * p[c]).sum::<f64>() + m[3][r])
}

fn add3(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(a: P3, s: f64) -> P3 {
    a.map(|v| v * s)
}

/// Ray `o + t d` against triangles: smallest `t > min_t`.
fn ray_hit(o: P3, d: P3, tris: impl Iterator<Item = [P3; 3]>, min_t: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for [a, b, c] in tris {
        let (e1, e2) = (sub(b, a), sub(c, a));
        let p = cross(d, e2);
        let det = dot(e1, p);
        if det.abs() < 1e-14 {
            continue;
        }
        let s = sub(o, a);
        let u = dot(s, p) / det;
        let q = cross(s, e1);
        let v = dot(d, q) / det;
        let t = dot(e2, q) / det;
        if u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9 && t > min_t && best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    }
    best
}

/// A solid with its topology and face names.
type Tagged = (Solid, Arc<Topology>, Vec<FaceTag>);

struct Eval {
    s: Rebuild,
    id: FeatureId,
}

impl Eval {
    fn warn(&mut self, msg: impl Into<String>) {
        self.s.warnings.entry(self.id).or_insert_with(|| msg.into());
    }

    fn body_bbox(&self) -> Option<(P3, P3)> {
        let mut it = self
            .s
            .bodies
            .iter()
            .flat_map(|b| b.topo.faces.iter().flat_map(|f| f.tris.iter().flatten().copied()));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| {
            (
                std::array::from_fn(|i| lo[i].min(p[i])),
                std::array::from_fn(|i| hi[i].max(p[i])),
            )
        }))
    }

    fn resolve_face(&self, r: &crate::naming::FaceRef) -> Result<(usize, usize)> {
        naming::resolve_face(&self.s.bodies, r)
            .ok_or_else(|| anyhow!("Missing reference: a face this feature uses no longer exists"))
    }

    fn resolve_edge(&self, r: &crate::naming::EdgeRef) -> Result<(usize, usize)> {
        naming::resolve_edge(&self.s.bodies, r)
            .ok_or_else(|| anyhow!("Missing reference: an edge this feature uses no longer exists"))
    }

    fn resolve_vertex(&self, r: &crate::naming::VertexRef) -> Result<(usize, usize)> {
        naming::resolve_vertex(&self.s.bodies, r)
            .ok_or_else(|| anyhow!("Missing reference: a vertex this feature uses no longer exists"))
    }

    fn plane_of(&self, p: &PlaneRef) -> Result<Plane> {
        match p {
            PlaneRef::Std(s) => Ok(s.plane()),
            PlaneRef::Face(f) => {
                let (b, i) = self.resolve_face(f)?;
                self.s.bodies[b].topo.faces[i]
                    .plane
                    .map(|p| stable_plane(&p))
                    .ok_or_else(|| anyhow!("the referenced face is not flat"))
            }
            PlaneRef::Feature(id) => self
                .s
                .planes
                .get(id)
                .copied()
                .ok_or_else(|| anyhow!("the referenced plane failed to build")),
        }
    }

    fn axis_of(&self, a: &AxisRef) -> Result<(P3, P3)> {
        match a {
            AxisRef::Std(i) => {
                let mut d = [0.0; 3];
                d[(*i as usize).min(2)] = 1.0;
                Ok(([0.0; 3], d))
            }
            AxisRef::Edge(e) => {
                let (b, i) = self.resolve_edge(e)?;
                match self.s.bodies[b].topo.edges[i].curve {
                    EdgeCurve::Line { a, b } => Ok((a, unit(sub(b, a)))),
                    EdgeCurve::Circle { center, axis, .. } => Ok((center, axis)),
                    EdgeCurve::Other => bail!("the edge is neither straight nor circular"),
                }
            }
            AxisRef::Face(f) => {
                let (b, i) = self.resolve_face(f)?;
                let face = &self.s.bodies[b].topo.faces[i];
                if let Some((c, d, _)) = face.cylinder {
                    Ok((c, d))
                } else if let Some(p) = face.plane {
                    Ok((face.center(), p.normal()))
                } else {
                    bail!("the face has no axis")
                }
            }
            AxisRef::Feature(id) => self
                .s
                .axes
                .get(id)
                .copied()
                .ok_or_else(|| anyhow!("the referenced axis failed to build")),
        }
    }

    fn point_of(&self, p: &PointRef) -> Result<P3> {
        match p {
            PointRef::Origin => Ok([0.0; 3]),
            PointRef::Vertex(v) => {
                let (b, i) = self.resolve_vertex(v)?;
                Ok(self.s.bodies[b].topo.vertices[i])
            }
            PointRef::Feature(id) => self
                .s
                .points
                .get(id)
                .copied()
                .ok_or_else(|| anyhow!("the referenced point failed to build")),
        }
    }

    /// Distance along `dir` (unit) from the profile to where an end condition stops.
    fn depth(&self, end: &End, plane: &Plane, regions: &[ProfileRegion], dir: P3) -> Result<f64> {
        let samples: Vec<P3> = regions
            .iter()
            .flat_map(|r| offset::sample_loop(&r.outer, 32))
            .map(|p| plane.to_world(p))
            .collect();
        let center = scale3(
            samples.iter().fold([0.0; 3], |a, p| add3(a, *p)),
            1.0 / samples.len().max(1) as f64,
        );
        let d = match end {
            End::Blind(d) | End::MidPlane(d) => *d,
            End::ThroughAll => {
                let (lo, hi) = self
                    .body_bbox()
                    .ok_or_else(|| anyhow!("Through All needs a body to go through"))?;
                let corners = (0..8).map(|k| {
                    [
                        if k & 1 == 0 { lo[0] } else { hi[0] },
                        if k & 2 == 0 { lo[1] } else { hi[1] },
                        if k & 4 == 0 { lo[2] } else { hi[2] },
                    ]
                });
                let reach = corners
                    .map(|c| dot(sub(c, plane.origin), dir))
                    .fold(f64::NEG_INFINITY, f64::max);
                ensure!(reach > 0.0, "there is no material in that direction");
                // Past every body, with margin for the profile's own offset across the plane.
                reach + 1.0
            }
            End::UpToNext => {
                let tris = || {
                    self.s
                        .bodies
                        .iter()
                        .flat_map(|b| b.topo.faces.iter().flat_map(|f| f.tris.iter().copied()))
                };
                samples
                    .iter()
                    .chain(std::iter::once(&center))
                    .filter_map(|p| ray_hit(*p, dir, tris(), 1e-4))
                    .fold(None, |a: Option<f64>, t| Some(a.map_or(t, |a| a.max(t))))
                    .ok_or_else(|| anyhow!("Up To Next: nothing in that direction"))?
            }
            End::UpToVertex(v) => {
                let (b, i) = self.resolve_vertex(v)?;
                dot(sub(self.s.bodies[b].topo.vertices[i], plane.origin), dir)
            }
            End::UpToSurface(f) | End::OffsetFromSurface(f, _) => {
                let (b, i) = self.resolve_face(f)?;
                let face = &self.s.bodies[b].topo.faces[i];
                let t = match face.plane {
                    Some(fp) => {
                        let n = fp.normal();
                        let den = dot(dir, n);
                        ensure!(den.abs() > 1e-9, "the surface is parallel to the extrusion");
                        dot(sub(fp.origin, center), n) / den
                    }
                    None => ray_hit(center, dir, face.tris.iter().copied(), -1e9)
                        .ok_or_else(|| anyhow!("the extrusion does not reach that surface"))?,
                };
                let off = if let End::OffsetFromSurface(_, o) = end {
                    *o
                } else {
                    0.0
                };
                t - off
            }
        };
        ensure!(
            d > 1e-6,
            "{} gives no depth in this direction (try Reverse Direction)",
            end.name()
        );
        Ok(d)
    }

    fn sketch<'h>(&self, h: &'h History, id: FeatureId) -> Result<(&'h Sketch, Plane)> {
        match h.get(id).map(|f| &f.kind) {
            Some(FeatureKind::Sketch { sketch, plane, .. }) => Ok((sketch, *plane)),
            _ => bail!("Referenced sketch is missing"),
        }
    }

    /// Builds each solid of a tool with its face tags.
    fn tool_bodies(&self, tool: &Tool, instance: u32) -> Result<Vec<Tagged>> {
        tool.solids()?
            .into_iter()
            .enumerate()
            .map(|(r, s)| {
                let t = analyze(&s);
                let tags = naming::tool_tags(&t, tool, self.id, instance, r as u32);
                Ok((s, t, tags))
            })
            .collect()
    }

    fn make_body(&self, solid: Solid, sources: &[(&Topology, &[FaceTag])], csg: Vec<Step>) -> Body {
        let topo = analyze(&solid);
        let tags = naming::inherit_tags(&topo, sources, self.id);
        Body { solid, topo, tags, csg }
    }

    /// Applies a tool to the bodies.
    fn apply(&mut self, step: Step, merge: bool) -> Result<()> {
        let pieces = self.tool_bodies(&step.tool, step.instance)?;
        for (tool, ttopo, ttags) in pieces {
            match step.op {
                OpKind::Add => {
                    let mut merged: Option<Body> = None;
                    let mut keep = Vec::new();
                    for b in std::mem::take(&mut self.s.bodies) {
                        if !merge {
                            keep.push(b);
                            continue;
                        }
                        let (cur_solid, cur_topo, cur_tags) = match &merged {
                            Some(m) => (m.solid.clone(), m.topo.clone(), m.tags.clone()),
                            None => (tool.clone(), ttopo.clone(), ttags.clone()),
                        };
                        match boolean::union(&b.solid, &cur_solid) {
                            Ok(Some(u)) => {
                                let mut csg = b.csg.clone();
                                if let Some(m) = &merged {
                                    csg.extend(m.csg.iter().cloned());
                                } else {
                                    csg.push(step.clone());
                                }
                                let src: [(&Topology, &[FaceTag]); 2] = [(&b.topo, &b.tags), (&cur_topo, &cur_tags)];
                                merged = Some(self.make_body(u, &src, csg));
                            }
                            Ok(None) => keep.push(b),
                            Err(e) => {
                                self.s.bodies = keep;
                                self.s.bodies.push(b);
                                return Err(e);
                            }
                        }
                    }
                    self.s.bodies = keep;
                    let body = merged.unwrap_or(Body {
                        solid: tool,
                        topo: ttopo,
                        tags: ttags,
                        csg: vec![step.clone()],
                    });
                    self.s.bodies.push(body);
                }
                OpKind::Sub | OpKind::Keep => {
                    ensure!(!self.s.bodies.is_empty(), "there is no body to cut");
                    let mut out = Vec::new();
                    for b in std::mem::take(&mut self.s.bodies) {
                        let res = if step.op == OpKind::Sub {
                            boolean::subtract(&b.solid, &tool)
                        } else {
                            boolean::intersect(&b.solid, &tool)
                        };
                        let parts = match res {
                            Ok(p) => p,
                            Err(e) => {
                                out.push(b);
                                out.extend(std::mem::take(&mut self.s.bodies));
                                self.s.bodies = out;
                                return Err(e);
                            }
                        };
                        let untouched = step.op == OpKind::Sub
                            && parts.len() == 1
                            && boolean::bbox(&parts[0]) == boolean::bbox(&b.solid)
                            && parts[0].face_iter().count() == b.solid.face_iter().count();
                        if untouched {
                            out.push(b);
                            continue;
                        }
                        let mut csg = b.csg.clone();
                        csg.push(step.clone());
                        for p in parts {
                            let src: [(&Topology, &[FaceTag]); 2] = [(&b.topo, &b.tags), (&ttopo, &ttags)];
                            out.push(self.make_body(p, &src, csg.clone()));
                        }
                    }
                    self.s.bodies = out;
                }
            }
        }
        self.s.ops.entry(self.id).or_default().push(step);
        Ok(())
    }

    fn run(&mut self, h: &mut History) -> Result<()> {
        let id = self.id;
        let kind = h
            .get(id)
            .map(|f| f.kind.clone())
            .ok_or_else(|| anyhow!("missing feature"))?;
        for r in kind.references() {
            if let Some(e) = self.s.errors.get(&r) {
                let name = h.get(r).map_or("?", |f| f.name.as_str());
                bail!("{name} has an error: {e}");
            }
        }
        match kind {
            FeatureKind::Sketch { on, .. } => {
                let support = on.as_ref().map(|p| self.plane_of(p)).transpose()?;
                let Some(FeatureKind::Sketch { sketch, plane, .. }) = h.get_mut(id).map(|f| &mut f.kind) else {
                    unreachable!()
                };
                if let Some(p) = support {
                    *plane = p;
                }
                let report = sketch.solve();
                let analysis = sketch.analyze();
                self.s.sketch_status.insert(id, (analysis.status, analysis.dof));
                ensure!(
                    report.converged,
                    "Sketch cannot be solved: over-defined or conflicting (residual {:.1e})",
                    report.residual
                );
                Ok(())
            }
            FeatureKind::Extrude(e) => self.extrude(h, &e),
            FeatureKind::Revolve(r) => self.revolve(h, &r),
            FeatureKind::Fillet {
                edges,
                radius,
                propagate,
            } => self.edge_feature(&edges, Treatment::Fillet { radius }, false, propagate),
            FeatureKind::Chamfer {
                edges,
                kind,
                d1,
                d2,
                angle,
                flip,
            } => {
                let d2 = match kind {
                    ChamferKind::EqualDistance => d1,
                    ChamferKind::DistanceDistance => d2,
                    ChamferKind::AngleDistance => d1 * angle.tan(),
                };
                self.edge_feature(&edges, Treatment::Chamfer { d1, d2 }, flip, false)
            }
            FeatureKind::Shell {
                faces,
                thickness,
                outward,
            } => self.shell(&faces, thickness, outward),
            FeatureKind::Hole(hole) => self.hole(h, &hole),
            FeatureKind::LinearPattern(p) => {
                let (_, d1) = self.axis_of(&p.dir1.axis)?;
                let d1 = scale3(d1, if p.dir1.reverse { -1.0 } else { 1.0 });
                let (n2, d2, s2) = match &p.dir2 {
                    Some(d) => {
                        let (_, v) = self.axis_of(&d.axis)?;
                        (d.count.max(1), scale3(v, if d.reverse { -1.0 } else { 1.0 }), d.spacing)
                    }
                    None => (1, [0.0; 3], 0.0),
                };
                ensure!(p.dir1.count >= 1, "the pattern needs at least one instance");
                let mut mats = Vec::new();
                for i in 0..p.dir1.count {
                    for j in 0..n2 {
                        if (i, j) == (0, 0) || p.skip.contains(&(i, j)) {
                            continue;
                        }
                        mats.push(mat_translate(add3(
                            scale3(d1, p.dir1.spacing * i as f64),
                            scale3(d2, s2 * j as f64),
                        )));
                    }
                }
                self.pattern(&p.seeds, &mats, false)
            }
            FeatureKind::CircularPattern(p) => {
                let (o, d) = self.axis_of(&p.axis)?;
                ensure!(p.count >= 1, "the pattern needs at least one instance");
                let full = (p.angle.abs() - TAU).abs() < 1e-9;
                let step = if !p.equal {
                    p.angle
                } else if full || p.count < 2 {
                    p.angle / p.count as f64
                } else {
                    p.angle / (p.count - 1) as f64
                };
                let step = if p.reverse { -step } else { step };
                let mats: Vec<_> = (1..p.count)
                    .filter(|k| !p.skip.contains(k))
                    .map(|k| mat_rotate(o, d, step * k as f64))
                    .collect();
                self.pattern(&p.seeds, &mats, false)
            }
            FeatureKind::Mirror(m) => {
                let plane = self.plane_of(&m.plane)?;
                self.pattern(&m.seeds, &[mat_mirror(&plane)], m.merge)
            }
            FeatureKind::RefPlane(def) => {
                let p = match def {
                    RefPlaneDef::Offset { base, distance, flip } => {
                        let b = self.plane_of(&base)?;
                        let mut p = b;
                        p.origin = add3(b.origin, scale3(b.normal(), distance));
                        if flip {
                            p.y = p.y.map(|v| -v);
                        }
                        p
                    }
                    RefPlaneDef::Angle { base, axis, angle } => {
                        let b = self.plane_of(&base)?;
                        let (o, d) = self.axis_of(&axis)?;
                        ensure!(dot(d, b.normal()).abs() < 1e-6, "the axis must lie in the plane");
                        let m = mat_rotate(o, d, angle);
                        let lin = |v: P3| sub(apply_mat(&m, v), apply_mat(&m, [0.0; 3]));
                        Plane {
                            origin: apply_mat(&m, b.origin),
                            x: lin(b.x),
                            y: lin(b.y),
                        }
                    }
                    RefPlaneDef::ThreePoint(pts) => {
                        let [a, b, c] = [
                            self.point_of(&pts[0])?,
                            self.point_of(&pts[1])?,
                            self.point_of(&pts[2])?,
                        ];
                        let n = cross(sub(b, a), sub(c, a));
                        ensure!(norm(n) > 1e-9, "the three points are in a line");
                        let x = unit(sub(b, a));
                        Plane {
                            origin: a,
                            x,
                            y: cross(unit(n), x),
                        }
                    }
                    RefPlaneDef::Mid(a, b) => {
                        let (pa, pb) = (self.plane_of(&a)?, self.plane_of(&b)?);
                        let (na, mut nb) = (pa.normal(), pb.normal());
                        if dot(na, nb) < 0.0 {
                            nb = nb.map(|v| -v);
                        }
                        let n = unit(add3(na, nb));
                        let on_b = sub(pa.origin, scale3(nb, dot(sub(pa.origin, pb.origin), nb)));
                        Plane::from_origin_normal(scale3(add3(pa.origin, on_b), 0.5), n)
                    }
                };
                self.s.planes.insert(id, p);
                Ok(())
            }
            FeatureKind::RefAxis(def) => {
                let a = match def {
                    RefAxisDef::TwoPoints(a, b) => {
                        let (a, b) = (self.point_of(&a)?, self.point_of(&b)?);
                        ensure!(norm(sub(b, a)) > 1e-9, "the two points are the same");
                        (a, unit(sub(b, a)))
                    }
                    RefAxisDef::Edge(e) => self.axis_of(&AxisRef::Edge(e))?,
                    RefAxisDef::Cylinder(f) => self.axis_of(&AxisRef::Face(f))?,
                    RefAxisDef::TwoPlanes(a, b) => {
                        let (pa, pb) = (self.plane_of(&a)?, self.plane_of(&b)?);
                        let d = cross(pa.normal(), pb.normal());
                        ensure!(norm(d) > 1e-9, "the planes are parallel");
                        let d = unit(d);
                        // A point on both planes: solve in the span of the two normals.
                        let (n1, n2) = (pa.normal(), pb.normal());
                        let (c1, c2) = (dot(n1, pa.origin), dot(n2, pb.origin));
                        let n12 = dot(n1, n2);
                        let det = 1.0 - n12 * n12;
                        let (k1, k2) = ((c1 - c2 * n12) / det, (c2 - c1 * n12) / det);
                        (add3(scale3(n1, k1), scale3(n2, k2)), d)
                    }
                };
                self.s.axes.insert(id, a);
                Ok(())
            }
            FeatureKind::RefPoint(def) => {
                let p = match def {
                    RefPointDef::Vertex(v) => self.point_of(&PointRef::Vertex(v))?,
                    RefPointDef::FaceCenter(f) => {
                        let (b, i) = self.resolve_face(&f)?;
                        self.s.bodies[b].topo.faces[i].center()
                    }
                    RefPointDef::ArcCenter(e) => {
                        let (b, i) = self.resolve_edge(&e)?;
                        match self.s.bodies[b].topo.edges[i].curve {
                            EdgeCurve::Circle { center, .. } => center,
                            _ => bail!("the edge is not an arc or circle"),
                        }
                    }
                    RefPointDef::Coords(c) => c,
                };
                self.s.points.insert(id, p);
                Ok(())
            }
            FeatureKind::CoordSys(c) => {
                let o = self.point_of(&c.origin)?;
                let z = match &c.z {
                    Some(a) => self.axis_of(a)?.1,
                    None => [0.0, 0.0, 1.0],
                };
                let x = match &c.x {
                    Some(a) => self.axis_of(a)?.1,
                    None => [1.0, 0.0, 0.0],
                };
                let x = sub(x, scale3(z, dot(x, z)));
                ensure!(norm(x) > 1e-9, "the X and Z directions are parallel");
                let x = unit(x);
                self.s.planes.insert(
                    id,
                    Plane {
                        origin: o,
                        x,
                        y: cross(unit(z), x),
                    },
                );
                Ok(())
            }
            FeatureKind::Draft { neutral, faces, angle } => self.draft(&neutral, &faces, angle),
            FeatureKind::Rib {
                sketch,
                thickness,
                both_sides,
                flip,
            } => self.rib(h, sketch, thickness, both_sides, flip),
        }
    }

    fn extrude(&mut self, h: &History, e: &Extrude) -> Result<()> {
        let (sketch, plane) = self.sketch(h, e.sketch)?;
        let regions = match &e.thin {
            None => regions_of(sketch)?,
            Some(thin) => {
                let (inside, outside) = match thin.kind {
                    ThinKind::OneDirection if thin.reverse => (thin.t1, 0.0),
                    ThinKind::OneDirection => (0.0, thin.t1),
                    ThinKind::MidPlane => (thin.t1 / 2.0, thin.t1 / 2.0),
                    ThinKind::TwoDirection => (thin.t1, thin.t2),
                };
                match regions_of(sketch) {
                    Ok(rs) => rs
                        .iter()
                        .map(|r| offset::thin_region(r, inside, outside))
                        .collect::<Result<Vec<_>>>()?
                        .concat(),
                    Err(_) => chains_of(sketch)
                        .iter()
                        .map(|c| offset::thicken_chain(c, outside, inside))
                        .collect::<Result<_>>()?,
                }
            }
        };
        ensure!(!regions.is_empty(), "Sketch has no closed profile");
        let sign = if e.cut { -1.0 } else { 1.0 } * if e.reverse { -1.0 } else { 1.0 };
        let n = plane.normal();
        let (z0, z1) = if let End::MidPlane(d) = e.end {
            ensure!(d > 1e-6, "Mid Plane needs a positive depth");
            (-d / 2.0, d / 2.0)
        } else {
            let d1 = self.depth(&e.end, &plane, &regions, scale3(n, sign))?;
            let d2 = match &e.end2 {
                Some(end2) => self.depth(end2, &plane, &regions, scale3(n, -sign))?,
                None => 0.0,
            };
            let (a, b) = (sign * d1, -sign * d2);
            (a.min(b), a.max(b))
        };
        ensure!(
            e.draft == 0.0 || e.end2.is_none(),
            "Draft with Direction 2 is not supported yet"
        );
        let draft = (e.draft != 0.0).then(|| Draft {
            neutral: 0.0,
            angles: vec![e.draft * sign],
        });
        let op = if !e.cut {
            OpKind::Add
        } else if e.flip_side {
            OpKind::Keep
        } else {
            OpKind::Sub
        };
        let one_sided = e.end2.is_none() && !matches!(e.end, End::MidPlane(_));
        // A sketch on a model face: start the tool a micron behind the face so truck never has to
        // intersect two coplanar faces (the micron is inside the part for a boss, outside for a cut).
        let on_face = one_sided
            && self.s.bodies.iter().any(|b| {
                b.topo.faces.iter().filter_map(|f| f.plane).any(|fp| {
                    dot(fp.normal(), n).abs() > 1.0 - 1e-9 && dot(sub(fp.origin, plane.origin), n).abs() < 1e-6
                })
            });
        let (z0, z1) = match (on_face, sign > 0.0) {
            (true, true) => (z0 - GROW, z1),
            (true, false) => (z0, z1 + GROW),
            _ => (z0, z1),
        };
        let tool = Tool::Prism {
            regions,
            plane,
            z0,
            z1,
            draft,
        };
        let step = Step {
            feature: self.id,
            instance: 0,
            op,
            tool,
        };
        self.apply(step, e.merge)
    }

    fn revolve(&mut self, h: &History, r: &Revolve) -> Result<()> {
        let (sketch, plane) = self.sketch(h, r.sketch)?;
        let regions = regions_of(sketch)?;
        let line_of = |e: threebcad_sketch::EntityId| -> Result<([f64; 2], [f64; 2])> {
            match sketch.entity(e).geom {
                Geometry::Line { a, b } => {
                    let (a, b) = (sketch.point(a), sketch.point(b));
                    Ok((a, [b[0] - a[0], b[1] - a[1]]))
                }
                _ => bail!("the revolve axis must be a line"),
            }
        };
        let (o, d) = match &r.axis {
            Some(RevolveAxis::SketchLine(e)) => line_of(*e)?,
            None => {
                let e = sketch
                    .entity_ids()
                    .find(|&e| sketch.entity(e).construction && sketch.entity(e).is_line())
                    .ok_or_else(|| anyhow!("Draw a centerline in the sketch to revolve around"))?;
                line_of(e)?
            }
            Some(RevolveAxis::Axis(a)) => {
                let (p, d) = self.axis_of(a)?;
                let n = plane.normal();
                ensure!(
                    dot(d, n).abs() < 1e-6 && dot(sub(p, plane.origin), n).abs() < 1e-6,
                    "the axis must lie in the sketch plane"
                );
                let (pl, ql) = (plane.to_local(p), plane.to_local(add3(p, d)));
                (pl, [ql[0] - pl[0], ql[1] - pl[1]])
            }
        };
        ensure!(
            r.angle.abs() + r.angle2.abs() > 1e-9,
            "the revolve angle must be non-zero"
        );
        let s = if r.reverse { -1.0 } else { 1.0 };
        let (a0, a1) = (-s * r.angle2, s * r.angle);
        let tool = Tool::Revolution {
            regions,
            plane,
            axis: (o, d),
            a0,
            a1: if (a1 - a0).abs() > TAU { a0 + TAU * s } else { a1 },
        };
        let step = Step {
            feature: self.id,
            instance: 0,
            op: if r.cut { OpKind::Sub } else { OpKind::Add },
            tool,
        };
        self.apply(step, r.merge)
    }

    fn edge_feature(
        &mut self,
        edges: &[crate::naming::EdgeRef],
        t: Treatment,
        flip: bool,
        propagate: bool,
    ) -> Result<()> {
        ensure!(!edges.is_empty(), "Select edges");
        let mut picked: Vec<(usize, usize)> = edges.iter().map(|e| self.resolve_edge(e)).collect::<Result<_>>()?;
        if propagate {
            // Add neighbours that continue a picked edge without a corner.
            let mut i = 0;
            while i < picked.len() {
                let (b, e) = picked[i];
                let topo = &self.s.bodies[b].topo;
                let me = &topo.edges[e];
                for (k, other) in topo.edges.iter().enumerate() {
                    if picked.contains(&(b, k)) {
                        continue;
                    }
                    let shared = me.vertices.iter().find(|v| other.vertices.contains(v));
                    let Some(&v) = shared else { continue };
                    let p = topo.vertices[v];
                    let dir_at = |ed: &topology::TopoEdge| {
                        let (first, last) = (ed.points[0], *ed.points.last().unwrap());
                        if norm(sub(first, p)) < norm(sub(last, p)) {
                            unit(sub(ed.points[1], first))
                        } else {
                            unit(sub(ed.points[ed.points.len() - 2], last))
                        }
                    };
                    if dot(dir_at(me), dir_at(other)) < -0.999 {
                        picked.push((b, k));
                    }
                }
                i += 1;
            }
        }
        // Tools come from the topology before any of them is applied.
        let mut tools: Vec<(usize, edge_ops::EdgeTool, usize)> = Vec::new();
        for &(b, e) in &picked {
            let topo = &self.s.bodies[b].topo;
            // The arcs of one circle make the same revolved tool: keep one.
            if let EdgeCurve::Circle { center, radius, .. } = topo.edges[e].curve {
                let dup = tools.iter().any(|(tb, _, te)| {
                    *tb == b
                        && matches!(self.s.bodies[b].topo.edges[*te].curve, EdgeCurve::Circle { center: c, radius: r, .. }
                            if norm(sub(c, center)) < 1e-6 && (r - radius).abs() < 1e-6)
                });
                if dup {
                    continue;
                }
            }
            tools.push((b, edge_ops::edge_tool(topo, e, t, flip)?, e));
        }
        for (b, et, e) in tools {
            let op = if et.add { OpKind::Add } else { OpKind::Sub };
            let step = Step {
                feature: self.id,
                instance: 0,
                op,
                tool: et.tool.clone(),
            };
            match self.apply(step, true) {
                Ok(()) => {}
                Err(first) if matches!(t, Treatment::Fillet { .. }) => {
                    // Curved faces: cross them more steeply and try again.
                    let topo = self.s.bodies.get(b).map(|bb| bb.topo.clone());
                    let retry = topo
                        .and_then(|tp| edge_ops::edge_tool_crossing(&tp, e, t, flip, 1e-2).ok())
                        .map(|et2| Step {
                            feature: self.id,
                            instance: 0,
                            op,
                            tool: et2.tool,
                        });
                    match retry {
                        Some(s2) => self.apply(s2, true).map_err(|_| first)?,
                        None => return Err(first),
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn base_of(&self, b: usize) -> Result<(Step, Vec<Step>)> {
        let csg = &self.s.bodies[b].csg;
        let (first, rest) = csg.split_first().ok_or_else(|| anyhow!("the body has no history"))?;
        ensure!(
            first.op == OpKind::Add && rest.iter().all(|s| s.op == OpKind::Sub),
            "Shell and Draft work on bodies made of one boss followed by cuts, holes, fillets and chamfers"
        );
        Ok((first.clone(), rest.to_vec()))
    }

    fn shell(&mut self, faces: &[crate::naming::FaceRef], t: f64, outward: bool) -> Result<()> {
        ensure!(t > 0.0, "the wall thickness must be positive");
        let picks: Vec<(usize, usize)> = faces.iter().map(|f| self.resolve_face(f)).collect::<Result<_>>()?;
        let b = match picks.first() {
            Some(&(b, _)) => b,
            None if self.s.bodies.len() == 1 => 0,
            None => bail!("Select the faces to remove (they pick the body)"),
        };
        ensure!(
            picks.iter().all(|&(pb, _)| pb == b),
            "all removed faces must be on one body"
        );
        let (base, subs) = self.base_of(b)?;
        let removed: Vec<ToolFace> = picks
            .iter()
            .map(|&(_, f)| {
                let tag = self.s.bodies[b].tags[f];
                ensure!(
                    tag.feature == base.feature && tag.instance == base.instance,
                    "Shell can only remove faces of the first boss of the body"
                );
                Ok(tag.face)
            })
            .collect::<Result<_>>()?;
        let margin = t + 1.0;
        // The base prism with each side moved by `side(seg)` and the ends by `start` / `end`
        // (positive = into the material).
        let reshaped = |side: &dyn Fn(usize) -> f64, start: f64, end: f64| -> Result<Tool> {
            match &base.tool {
                Tool::Prism {
                    regions,
                    plane,
                    z0,
                    z1,
                    draft,
                } => {
                    let mut seg = 0;
                    let regions = regions
                        .iter()
                        .map(|r| {
                            let mut lp = |l: &Vec<ProfileSegment>| {
                                let d: Vec<f64> = (0..l.len()).map(|k| side(seg + k)).collect();
                                seg += l.len();
                                offset::offset_loop(l, &d)
                            };
                            Ok(ProfileRegion {
                                outer: lp(&r.outer)?,
                                holes: r.holes.iter().map(&mut lp).collect::<Result<_>>()?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let (lo, hi) = (z0.min(*z1), z0.max(*z1));
                    Ok(Tool::Prism {
                        regions,
                        plane: *plane,
                        z0: lo + start,
                        z1: hi - end,
                        draft: draft.clone(),
                    })
                }
                Tool::Revolution { .. } => {
                    ensure!(removed.is_empty(), "Shell cannot remove faces of a revolved body yet");
                    base.tool.offset(-(start))
                }
            }
        };
        let is_removed = |f: ToolFace| removed.contains(&f);
        let side_removed = |k: usize| is_removed(ToolFace::Side(k));
        let (start_r, end_r) = (is_removed(ToolFace::Start), is_removed(ToolFace::End));
        let carve = |tool: &Tool, cuts: &[Tool]| -> Result<Vec<Solid>> {
            let mut pieces = tool.solids()?;
            for c in cuts {
                let cs = c.solids()?;
                pieces = pieces
                    .iter()
                    .map(|p| {
                        cs.iter().try_fold(vec![p.clone()], |acc, c| {
                            acc.iter()
                                .map(|a| boolean::subtract(a, c))
                                .collect::<Result<Vec<_>>>()
                                .map(|v| v.concat())
                        })
                    })
                    .collect::<Result<Vec<_>>>()?
                    .concat();
            }
            Ok(pieces)
        };
        let sub_tools: Vec<Tool> = subs.iter().map(|s| s.tool.clone()).collect();
        let body = self.s.bodies[b].solid.clone();
        let result: Vec<Solid> = if outward {
            let outer = reshaped(
                &|k| if side_removed(k) { 0.0 } else { -t },
                if start_r { 0.0 } else { -t },
                if end_r { 0.0 } else { -t },
            )?;
            let shrunk: Vec<Tool> = sub_tools.iter().filter_map(|c| c.offset(-t).ok()).collect();
            let ext = reshaped(
                &|k| if side_removed(k) { -margin } else { 0.0 },
                if start_r { -margin } else { 0.0 },
                if end_r { -margin } else { 0.0 },
            )?;
            let mut out = carve(&outer, &shrunk)?;
            for e in carve(&ext, &sub_tools)? {
                out = out
                    .iter()
                    .map(|o| boolean::subtract(o, &e))
                    .collect::<Result<Vec<_>>>()?
                    .concat();
            }
            out
        } else {
            let inner = reshaped(
                &|k| if side_removed(k) { -margin } else { t },
                if start_r { -margin } else { t },
                if end_r { -margin } else { t },
            )?;
            let grown: Vec<Tool> = sub_tools.iter().map(|c| c.offset(t)).collect::<Result<_>>()?;
            let mut out = vec![body];
            for i in carve(&inner, &grown)? {
                out = out
                    .iter()
                    .map(|o| boolean::subtract(o, &i))
                    .collect::<Result<Vec<_>>>()?
                    .concat();
            }
            out
        };
        ensure!(!result.is_empty(), "the wall thickness removes the whole body");
        let old = self.s.bodies.remove(b);
        let mut csg = old.csg.clone();
        csg.push(Step {
            feature: self.id,
            instance: 0,
            op: OpKind::Sub,
            tool: base.tool.clone(),
        });
        for s in result {
            let src: [(&Topology, &[FaceTag]); 1] = [(&old.topo, &old.tags)];
            let body = self.make_body(s, &src, csg.clone());
            self.s.bodies.push(body);
        }
        Ok(())
    }

    fn hole(&mut self, h: &History, hole: &Hole) -> Result<()> {
        let (sketch, plane) = self.sketch(h, hole.sketch)?;
        let size = holes::find(hole.standard, &hole.size).ok_or_else(|| anyhow!("unknown hole size {}", hole.size))?;
        let points: Vec<[f64; 2]> = sketch
            .point_ids()
            .filter(|&p| sketch.entities_at(p).is_empty())
            .map(|p| sketch.point(p))
            .collect();
        ensure!(!points.is_empty(), "Add sketch points to place the holes");
        // Drill into the material: flip the plane so its normal points down the hole.
        let down = Plane {
            origin: plane.origin,
            x: plane.x,
            y: plane.y.map(|v| -v),
        };
        let pts: Vec<[f64; 2]> = points.iter().map(|p| [p[0], -p[1]]).collect();
        let n = down.normal();
        let regions_at =
            |r: f64| -> Vec<ProfileRegion> { pts.iter().map(|&c| threebcad_kernel::tool::disc(c, r)).collect() };
        let depth = match &hole.end {
            End::ThroughAll => {
                let all: Vec<ProfileRegion> = regions_at(size.clearance / 2.0);
                self.depth(&End::ThroughAll, &down, &all, n)?
            }
            other => self.depth(other, &down, &regions_at(1.0), n)?,
        };
        let r = size.drill(hole.kind) / 2.0;
        // Start above the face so the tool never lies in it.
        let top = -0.01;
        // Half profile (radius, depth) of a stepped hole, turned about the hole's axis as one tool:
        // truck handles that far better than stacked cylinders.
        let profile: Option<Vec<[f64; 2]>> = match hole.kind {
            HoleKind::Counterbore => {
                ensure!(size.cbore_depth < depth, "the hole is shallower than its counterbore");
                let rc = size.cbore_diameter / 2.0;
                Some(vec![
                    [0.0, top],
                    [rc, top],
                    [rc, size.cbore_depth],
                    [r, size.cbore_depth],
                    [r, depth],
                    [0.0, depth],
                ])
            }
            HoleKind::Countersink => {
                let half = (size.csk_angle / 2.0).to_radians();
                let rc = size.csk_diameter / 2.0;
                let cone = (rc - r) / half.tan();
                ensure!(cone < depth, "the hole is shallower than its countersink");
                Some(vec![
                    [0.0, top],
                    [rc - top * half.tan(), top],
                    [r, cone],
                    [r, depth],
                    [0.0, depth],
                ])
            }
            HoleKind::Tapped | HoleKind::Simple => None,
        };
        if hole.kind == HoleKind::Tapped {
            self.s.threads.insert(self.id, size.thread(hole.standard));
        }
        let tools: Vec<Tool> = match profile {
            None => vec![Tool::Prism {
                regions: regions_at(r),
                plane: down,
                z0: top,
                z1: depth,
                draft: None,
            }],
            Some(pts_rz) => pts
                .iter()
                .map(|&c| {
                    Ok(Tool::Revolution {
                        regions: vec![threebcad_kernel::tool::polygon(&pts_rz)?],
                        plane: Plane {
                            origin: down.to_world(c),
                            x: down.x,
                            y: n,
                        },
                        axis: ([0.0, 0.0], [0.0, 1.0]),
                        a0: 0.0,
                        a1: TAU,
                    })
                })
                .collect::<Result<_>>()?,
        };
        for tool in tools {
            self.apply(
                Step {
                    feature: self.id,
                    instance: 0,
                    op: OpKind::Sub,
                    tool,
                },
                true,
            )?;
        }
        Ok(())
    }

    fn pattern(&mut self, seeds: &Seeds, mats: &[[[f64; 4]; 4]], merge: bool) -> Result<()> {
        match seeds {
            Seeds::Features(fs) => {
                ensure!(!fs.is_empty(), "Select features to repeat");
                let steps: Vec<Step> = fs
                    .iter()
                    .flat_map(|f| self.s.ops.get(f).cloned().unwrap_or_default())
                    .collect();
                ensure!(!steps.is_empty(), "the selected features add or remove no material");
                let mut failed = Vec::new();
                for (k, m) in mats.iter().enumerate() {
                    for s in &steps {
                        let step = Step {
                            feature: self.id,
                            instance: k as u32 + 1,
                            op: s.op,
                            tool: s.tool.transformed(*m),
                        };
                        if self.apply(step, true).is_err() {
                            failed.push(k + 2);
                        }
                    }
                }
                failed.dedup();
                if !failed.is_empty() {
                    let list: Vec<String> = failed.iter().map(|k| k.to_string()).collect();
                    self.warn(format!(
                        "instance(s) {} could not be built and were skipped",
                        list.join(", ")
                    ));
                }
                Ok(())
            }
            Seeds::Bodies => {
                let originals = self.s.bodies.clone();
                ensure!(!originals.is_empty(), "there are no bodies to repeat");
                for (k, m) in mats.iter().enumerate() {
                    for b in &originals {
                        let solid = boolean::transformed(&b.solid, *m);
                        let topo = analyze(&solid);
                        let tags = b
                            .tags
                            .iter()
                            .map(|t| FaceTag {
                                instance: t.instance + 1000 * (k as u32 + 1),
                                ..*t
                            })
                            .collect::<Vec<_>>();
                        // Mirroring/rotating keeps the face order, so tags carry over one to one.
                        let tags = if tags.len() == topo.faces.len() {
                            tags
                        } else {
                            naming::inherit_tags(&topo, &[], self.id)
                        };
                        let csg: Vec<Step> = b
                            .csg
                            .iter()
                            .map(|s| Step {
                                tool: s.tool.transformed(*m),
                                ..s.clone()
                            })
                            .collect();
                        let body = Body { solid, topo, tags, csg };
                        if merge {
                            let mut merged = false;
                            for other in self.s.bodies.iter_mut() {
                                if let Ok(Some(u)) = boolean::union(&other.solid, &body.solid) {
                                    let topo = analyze(&u);
                                    let src: [(&Topology, &[FaceTag]); 2] =
                                        [(&other.topo, &other.tags), (&body.topo, &body.tags)];
                                    let tags = naming::inherit_tags(&topo, &src, self.id);
                                    other.csg.extend(body.csg.iter().cloned());
                                    *other = Body {
                                        solid: u,
                                        topo,
                                        tags,
                                        csg: other.csg.clone(),
                                    };
                                    merged = true;
                                    break;
                                }
                            }
                            if !merged {
                                self.s.bodies.push(body);
                            }
                        } else {
                            self.s.bodies.push(body);
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Replays a body's history with some steps changed; keeps the piece nearest the old body.
    fn replay(&self, csg: &[Step], near: P3) -> Result<Body> {
        let mut bodies: Vec<Tagged> = Vec::new();
        for (i, s) in csg.iter().enumerate() {
            let sub_eval = Eval {
                s: Rebuild::default(),
                id: s.feature,
            };
            let pieces = sub_eval.tool_bodies(&s.tool, s.instance)?;
            if i == 0 {
                bodies = pieces;
                continue;
            }
            let mut next = Vec::new();
            for (solid, topo, tags) in &bodies {
                for (ts, tt, tg) in &pieces {
                    let res = match s.op {
                        OpKind::Add => boolean::union(solid, ts)?.into_iter().collect(),
                        OpKind::Sub => boolean::subtract(solid, ts)?,
                        OpKind::Keep => boolean::intersect(solid, ts)?,
                    };
                    for r in res {
                        let rt = analyze(&r);
                        let src: [(&Topology, &[FaceTag]); 2] = [(topo, tags), (tt, tg)];
                        let rtags = naming::inherit_tags(&rt, &src, s.feature);
                        next.push((r, rt, rtags));
                    }
                }
            }
            bodies = next;
        }
        let (solid, topo, tags) = bodies
            .into_iter()
            .min_by(|a, b| {
                let d = |t: &Topology| t.faces.first().map_or(f64::INFINITY, |f| norm(sub(f.sample, near)));
                d(&a.1).total_cmp(&d(&b.1))
            })
            .ok_or_else(|| anyhow!("nothing is left of the body"))?;
        Ok(Body {
            solid,
            topo,
            tags,
            csg: csg.to_vec(),
        })
    }

    fn draft(&mut self, neutral: &PlaneRef, faces: &[crate::naming::FaceRef], angle: f64) -> Result<()> {
        ensure!(!faces.is_empty(), "Select faces to draft");
        let np = self.plane_of(neutral)?;
        let picks: Vec<(usize, usize)> = faces.iter().map(|f| self.resolve_face(f)).collect::<Result<_>>()?;
        let b = picks[0].0;
        ensure!(
            picks.iter().all(|&(pb, _)| pb == b),
            "all drafted faces must be on one body"
        );
        let mut csg = self.s.bodies[b].csg.clone();
        for &(_, f) in &picks {
            let tag = self.s.bodies[b].tags[f];
            let ToolFace::Side(seg) = tag.face else {
                bail!("only side walls of an extrusion can be drafted");
            };
            let step = csg
                .iter_mut()
                .find(|s| {
                    s.feature == tag.feature && s.instance == tag.instance && matches!(s.tool, Tool::Prism { .. })
                })
                .ok_or_else(|| anyhow!("only side walls of an extrusion can be drafted"))?;
            let total: usize = step
                .tool
                .regions()
                .iter()
                .map(|r| r.outer.len() + r.holes.iter().map(Vec::len).sum::<usize>())
                .sum();
            if let Tool::Prism { plane, draft, .. } = &mut step.tool {
                let n = plane.normal();
                let along = dot(np.normal(), n);
                ensure!(
                    along.abs() > 1.0 - 1e-6,
                    "the neutral plane must be parallel to the extrusion's sketch"
                );
                let neutral_z = dot(sub(np.origin, plane.origin), n);
                let d = draft.get_or_insert(Draft {
                    neutral: neutral_z,
                    angles: vec![0.0; total],
                });
                if d.angles.len() != total {
                    d.angles = vec![d.angles[0]; total];
                }
                d.neutral = neutral_z;
                d.angles[seg] = angle * along.signum();
            }
        }
        let near = self.s.bodies[b].topo.faces.first().map_or([0.0; 3], |f| f.sample);
        let mut body = self.replay(&csg, near)?;
        body.csg.push(Step {
            feature: self.id,
            instance: 0,
            op: OpKind::Sub,
            tool: csg[0].tool.clone(),
        });
        body.csg = csg;
        self.s.bodies[b] = body;
        Ok(())
    }

    fn rib(&mut self, h: &History, sketch_id: FeatureId, thickness: f64, both: bool, flip: bool) -> Result<()> {
        ensure!(thickness > 0.0, "the rib thickness must be positive");
        let (sketch, plane) = self.sketch(h, sketch_id)?;
        let chains = chains_of(sketch);
        ensure!(!chains.is_empty(), "Draw an open line for the rib");
        let (lo, hi) = self
            .body_bbox()
            .ok_or_else(|| anyhow!("a rib needs a part to grow into"))?;
        let reach = norm(sub(hi, lo)) * 2.0 + 1.0;
        let center = scale3(add3(lo, hi), 0.5);
        let c2 = plane.to_local(center);
        let mut regions = Vec::new();
        for chain in &chains {
            // Grow towards the nearest wall (a ray from the line's middle), unless flipped.
            let s = &chain[0];
            let (a, b) = (offset::seg_start(s), offset::seg_end(s));
            let left = [-(b[1] - a[1]), b[0] - a[0]];
            let m = plane.to_world([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
            let dir = unit(add3(scale3(plane.x, left[0]), scale3(plane.y, left[1])));
            let tris = || {
                self.s
                    .bodies
                    .iter()
                    .flat_map(|b| b.topo.faces.iter().flat_map(|f| f.tris.iter().copied()))
            };
            let towards_left = match (ray_hit(m, dir, tris(), 1e-6), ray_hit(m, dir.map(|v| -v), tris(), 1e-6)) {
                (Some(l), Some(r)) => l <= r,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => left[0] * (c2[0] - a[0]) + left[1] * (c2[1] - a[1]) > 0.0,
            };
            let (l, r) = if towards_left != flip {
                (reach, 0.0)
            } else {
                (0.0, reach)
            };
            regions.push(offset::thicken_chain(chain, l, r)?);
        }
        let (z0, z1) = if both {
            (-thickness / 2.0, thickness / 2.0)
        } else {
            (0.0, thickness)
        };
        let tool = Tool::Prism {
            regions,
            plane,
            z0,
            z1,
            draft: None,
        };
        // shortcut: the rib stops at the part's bounding box (shrunk 0.1 mm, hidden inside the outer walls), not at the nearest
        // wall; fine for ribs between walls, too much material for open-sided parts.
        let inset = 0.1;
        let bx = Tool::Prism {
            regions: vec![threebcad_kernel::tool::polygon(&[
                [lo[0] + inset, lo[1] + inset],
                [hi[0] - inset, lo[1] + inset],
                [hi[0] - inset, hi[1] - inset],
                [lo[0] + inset, hi[1] - inset],
            ])?],
            plane: Plane::TOP,
            z0: lo[2] + inset,
            z1: hi[2] - inset,
            draft: None,
        };
        let bx = bx.solids()?.remove(0);
        let mut ribs = Vec::new();
        for s in tool.solids()? {
            ribs.extend(boolean::intersect(&s, &bx)?);
        }
        ensure!(!ribs.is_empty(), "the rib does not reach the part");
        for r in ribs {
            let t = analyze(&r);
            let tags = naming::tool_tags(&t, &tool, self.id, 0, 0);
            let mut merged = false;
            for i in 0..self.s.bodies.len() {
                if let Ok(Some(u)) = boolean::union(&self.s.bodies[i].solid, &r) {
                    let old = &self.s.bodies[i];
                    let mut csg = old.csg.clone();
                    csg.push(Step {
                        feature: self.id,
                        instance: 0,
                        op: OpKind::Add,
                        tool: tool.clone(),
                    });
                    let src: [(&Topology, &[FaceTag]); 2] = [(&old.topo, &old.tags), (&t, &tags)];
                    self.s.bodies[i] = self.make_body(u, &src, csg);
                    merged = true;
                    break;
                }
            }
            ensure!(merged, "the rib does not touch the part");
        }
        Ok(())
    }
}

/// Evaluates every feature above the rollback bar, reusing cached results for the unchanged
/// features at the start of the tree.
pub fn rebuild(h: &mut History, cache: &mut Vec<CacheEntry>) -> Rebuild {
    let order: Vec<FeatureId> = h.iter().map(|(id, _)| id).take(h.rollback()).collect();
    let suppressed: Vec<bool> = order.iter().map(|&id| h.is_suppressed(id)).collect();
    let reuse = cache
        .iter()
        .zip(order.iter().zip(&suppressed))
        .take_while(|(c, (&id, &sup))| c.id == id && c.suppressed == sup && h.get(id) == Some(&c.feature))
        .count();
    cache.truncate(reuse);
    let mut state = cache.last().map(|c| c.after.clone()).unwrap_or_default();
    state.evaluated = 0;
    for (i, &id) in order.iter().enumerate().skip(reuse) {
        if !suppressed[i] {
            let mut ev = Eval {
                s: std::mem::take(&mut state),
                id,
            };
            let result = ev.run(h);
            state = ev.s;
            if let Err(e) = result {
                state.errors.insert(id, e.to_string());
            }
            state.evaluated += 1;
        }
        if let Some(f) = h.get(id) {
            cache.push(CacheEntry {
                id,
                feature: f.clone(),
                suppressed: suppressed[i],
                after: state.clone(),
            });
        }
    }
    state
}
