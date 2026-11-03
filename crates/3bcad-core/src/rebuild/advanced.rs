//! The v0.4 features: sweeps, lofts, helices, threads, variable and face fillets, multibody tools
//! (combine, split, delete/keep, move/copy, scale), sketch- and curve-driven patterns, domes and
//! wraps.

use threebcad_kernel::loft::{self as kloft, Section};
use threebcad_kernel::sweep::{self as ksweep, Helix, PathSeg, SweepPath};
use threebcad_kernel::tool::Shape;
use threebcad_kernel::wrap::{self as kwrap, Cylinder};

use super::*;

/// The chain walked the other way.
fn reverse_chain(c: &[ProfileSegment]) -> Vec<ProfileSegment> {
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
            ProfileSegment::Circle { center, radius, ccw } => ProfileSegment::Circle {
                center,
                radius,
                ccw: !ccw,
            },
            ProfileSegment::Bezier([a, b, c, d]) => ProfileSegment::Bezier([d, c, b, a]),
        })
        .collect()
}

/// A sketch chain as a 3D path.
fn path_of(chain: &[ProfileSegment], plane: &Plane) -> Vec<PathSeg> {
    let w = |p: [f64; 2]| plane.to_world(p);
    let n = plane.normal();
    let mut out = Vec::new();
    for s in chain {
        match *s {
            ProfileSegment::Line { from, to } => out.push(PathSeg::Line(w(from), w(to))),
            ProfileSegment::Arc { center, from, to, ccw } => {
                let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
                let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
                let mut sweep = (a1 - a0).rem_euclid(TAU);
                if sweep < 1e-12 {
                    sweep = TAU;
                }
                if !ccw {
                    sweep -= TAU;
                }
                out.push(PathSeg::Arc {
                    center: w(center),
                    normal: n,
                    from: w(from),
                    sweep,
                });
            }
            ProfileSegment::Circle { center, radius, ccw } => {
                let from = w([center[0] + radius, center[1]]);
                let half = if ccw { TAU / 2.0 } else { -TAU / 2.0 };
                let mid = PathSeg::Arc {
                    center: w(center),
                    normal: n,
                    from,
                    sweep: half,
                }
                .end();
                out.push(PathSeg::Arc {
                    center: w(center),
                    normal: n,
                    from,
                    sweep: half,
                });
                out.push(PathSeg::Arc {
                    center: w(center),
                    normal: n,
                    from: mid,
                    sweep: half,
                });
            }
            ProfileSegment::Bezier(b) => out.push(PathSeg::Bezier(b.map(w))),
        }
    }
    out
}

/// Points along a path, about every half millimetre (at least 64).
pub(super) fn path_points(path: &SweepPath) -> Vec<P3> {
    match path {
        SweepPath::Helix(h) => h.polyline(72),
        SweepPath::Chain(segs) => {
            let mut pts = Vec::new();
            for s in segs {
                let k = ((s.length() * 2.0).ceil() as usize).clamp(8, 400);
                for i in 0..k {
                    pts.push(s.point(i as f64 / k as f64));
                }
            }
            if let Some(last) = segs.last() {
                pts.push(last.end());
            }
            pts
        }
    }
}

fn bbox_center(solids: &[Solid]) -> Option<P3> {
    let mut it = solids.iter().map(boolean::bbox);
    let (mut lo, mut hi) = it.next()?;
    for (a, b) in it {
        lo = std::array::from_fn(|i| lo[i].min(a[i]));
        hi = std::array::from_fn(|i| hi[i].max(b[i]));
    }
    Some(scale3(add3(lo, hi), 0.5))
}

fn mat_mul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    threebcad_kernel::sweep::mul(a, b)
}

/// Rotation taking unit `from` onto unit `to` about the line through `p`.
fn mat_align(p: P3, from: P3, to: P3) -> [[f64; 4]; 4] {
    let ax = cross(from, to);
    let s = norm(ax);
    let c = dot(from, to);
    if s < 1e-12 {
        if c > 0.0 {
            return mat_translate([0.0; 3]);
        }
        // Half a turn about any axis square to `from`.
        let t = if from[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        return mat_rotate(p, unit(cross(from, t)), std::f64::consts::PI);
    }
    mat_rotate(p, scale3(ax, 1.0 / s), s.atan2(c))
}

impl Eval {
    pub(super) fn advanced(&mut self, h: &History, kind: FeatureKind) -> Result<()> {
        match kind {
            FeatureKind::Helix(d) => self.helix(h, &d),
            FeatureKind::Sweep(w) => self.sweep(h, &w),
            FeatureKind::Loft(l) => self.loft(h, &l),
            FeatureKind::Thread(t) => self.thread(&t),
            FeatureKind::VariableFillet { edge, radii } => self.variable_fillet(&edge, &radii),
            FeatureKind::FaceFillet { a, b, radius } => self.face_fillet(&a, &b, radius),
            FeatureKind::Combine { op, main, tools } => self.combine(op, &main, &tools),
            FeatureKind::Split { tool, bodies } => self.split(h, &tool, &bodies),
            FeatureKind::DeleteBody { bodies, keep } => self.delete_bodies(&bodies, keep),
            FeatureKind::MoveBody(m) => self.move_bodies(&m),
            FeatureKind::ScaleBody { bodies, about, factors } => self.scale_bodies(&bodies, &about, factors),
            FeatureKind::SketchPattern { seeds, sketch } => self.sketch_pattern(h, &seeds, sketch),
            FeatureKind::CurvePattern {
                seeds,
                curve,
                count,
                spacing,
                align,
            } => self.curve_pattern(h, &seeds, curve, count, spacing, align),
            FeatureKind::Dome { face, height, reverse } => self.dome(&face, height, reverse),
            FeatureKind::Wrap {
                sketch,
                face,
                kind,
                depth,
            } => self.wrap(h, sketch, &face, kind, depth),
            other => bail!("{} is not a v0.4 feature", other.prefix()),
        }
    }

    fn helix(&mut self, h: &History, d: &HelixDef) -> Result<()> {
        let (sketch, plane) = self.sketch(h, d.sketch)?;
        let (center, radius) = sketch
            .entity_ids()
            .find_map(|e| match sketch.segments(e).first() {
                Some(Segment::Circle { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow!("Draw a circle in the sketch: the helix starts on it"))?;
        let (pitch, turns) = d.size.pitch_turns();
        ensure!(turns > 1e-9, "the helix needs at least some revolutions");
        let spiral = matches!(d.size, HelixSize::Spiral { .. });
        if !spiral {
            ensure!(pitch > 1e-9, "the pitch must be positive");
        }
        let n = plane.normal();
        let axis = if d.reverse { scale3(n, -1.0) } else { n };
        let x0 = mat_rotate([0.0; 3], n, d.start);
        let x = unit(apply_mat(&x0, plane.x));
        let grow = match d.size {
            HelixSize::Spiral { pitch, .. } => pitch,
            _ => pitch * d.taper.tan(),
        };
        let helix = Helix {
            origin: plane.to_world(center),
            axis,
            x,
            radius,
            pitch: if spiral { 0.0 } else { pitch },
            turns,
            grow,
            // Seen from the end of the axis.
            ccw: d.ccw,
        };
        ensure!(
            radius + grow * turns > 1e-6,
            "the taper closes the helix before its end"
        );
        self.s.curves.insert(self.id, helix.polyline(72));
        self.s.helices.insert(self.id, helix);
        Ok(())
    }

    /// The path of a curve reference, and the sketch plane it was drawn on (if any).
    fn curve_path(&self, h: &History, c: CurveRef, near: Option<P3>) -> Result<SweepPath> {
        match c {
            CurveRef::Helix(id) => self
                .s
                .helices
                .get(&id)
                .cloned()
                .map(SweepPath::Helix)
                .ok_or_else(|| anyhow!("the helix failed to build")),
            CurveRef::Sketch(id) => {
                let (sketch, plane) = self.sketch(h, id)?;
                let chains = chains_of(sketch);
                ensure!(chains.len() == 1, "the path sketch must hold one connected curve");
                let mut chain = chains.into_iter().next().unwrap();
                // Start at the end nearer the profile.
                if let Some(p) = near {
                    let (a, b) = (
                        plane.to_world(offset::seg_start(&chain[0])),
                        plane.to_world(offset::seg_end(chain.last().unwrap())),
                    );
                    if norm(sub(b, p)) < norm(sub(a, p)) {
                        chain = reverse_chain(&chain);
                    }
                }
                Ok(SweepPath::Chain(path_of(&chain, &plane)))
            }
        }
    }

    fn sweep(&mut self, h: &History, w: &Sweep) -> Result<()> {
        let near = match w.profile {
            SweepProfile::Sketch(id) => {
                let (sketch, plane) = self.sketch(h, id)?;
                let rs = regions_of(sketch)?;
                let pts: Vec<P3> = rs
                    .iter()
                    .flat_map(|r| offset::sample_loop(&r.outer, 16))
                    .map(|p| plane.to_world(p))
                    .collect();
                Some(scale3(
                    pts.iter().fold([0.0; 3], |a, p| add3(a, *p)),
                    1.0 / pts.len().max(1) as f64,
                ))
            }
            SweepProfile::Circle(_) => None,
        };
        let path = self.curve_path(h, w.path, near)?;
        let (regions, plane) = match w.profile {
            SweepProfile::Sketch(id) => {
                let (sketch, plane) = self.sketch(h, id)?;
                let rs = regions_of(sketch)?;
                ensure!(!rs.is_empty(), "the profile sketch has no closed outline");
                (rs, plane)
            }
            SweepProfile::Circle(d) => {
                ensure!(d > 1e-6, "the circle's diameter must be positive");
                let (p, t) = path.start();
                (
                    vec![threebcad_kernel::tool::disc([0.0, 0.0], d / 2.0)],
                    Plane::from_origin_normal(p, t),
                )
            }
        };
        let solids = ksweep::sweep(&regions, &plane, &path, w.twist)?;
        let (rs, pl, pa, tw) = (regions.clone(), plane, path.clone(), w.twist);
        let shape = Shape::new(solids, plane, move |p| match ksweep::face_at(&rs, &pl, &pa, tw, p) {
            Some((true, _, _)) => ToolFace::Start,
            Some((_, true, _)) => ToolFace::End,
            Some((_, _, local)) => ToolFace::Side(threebcad_kernel::tool::nearest_segment(&rs, local)),
            None => ToolFace::Side(0),
        });
        let step = Step {
            feature: self.id,
            instance: 0,
            op: if w.cut { OpKind::Sub } else { OpKind::Add },
            tool: Tool::Shape(shape),
        };
        self.apply(step, w.merge)
    }

    fn loft(&mut self, h: &History, l: &Loft) -> Result<()> {
        ensure!(l.profiles.len() >= 2, "Select two profiles or more");
        let mut sections = Vec::new();
        for &id in &l.profiles {
            let (sketch, plane) = self.sketch(h, id)?;
            let rs = regions_of(sketch)?;
            ensure!(rs.len() == 1, "each loft profile must be one closed outline");
            sections.push(Section {
                region: rs.into_iter().next().unwrap(),
                plane,
            });
        }
        // An end profile on a planar model face: move it a micron into the material for a boss (out of
        // it for a cut), so truck never has to intersect the cap with the coplanar face.
        let last = sections.len() - 1;
        for i in [0, last] {
            let pl = sections[i].plane;
            let n = pl.normal();
            let outward = self.s.bodies.iter().flat_map(|b| b.topo.faces.iter()).find_map(|f| {
                let fp = f.plane?;
                let coplanar = dot(fp.normal(), n).abs() > 1.0 - 1e-9 && dot(sub(fp.origin, pl.origin), n).abs() < 1e-6;
                let t = f.tris.first()?;
                coplanar.then(|| unit(cross(sub(t[1], t[0]), sub(t[2], t[0]))))
            });
            if let Some(o) = outward {
                let shift = if l.cut { GROW } else { -GROW };
                sections[i].plane.origin = add3(pl.origin, scale3(o, shift));
            }
        }
        let first_center = {
            let pts: Vec<P3> = offset::sample_loop(&sections[0].region.outer, 16)
                .iter()
                .map(|&p| sections[0].plane.to_world(p))
                .collect();
            scale3(pts.iter().fold([0.0; 3], |a, p| add3(a, *p)), 1.0 / pts.len() as f64)
        };
        let mut guides = Vec::new();
        for &g in &l.guides {
            let (sketch, plane) = self.sketch(h, g)?;
            let chains = chains_of(sketch);
            ensure!(chains.len() == 1, "each guide sketch must hold one connected curve");
            let mut pts: Vec<P3> = rib_samples(&chains[0]).iter().map(|&p| plane.to_world(p)).collect();
            if norm(sub(pts[pts.len() - 1], first_center)) < norm(sub(pts[0], first_center)) {
                pts.reverse();
            }
            guides.push(pts);
        }
        let solid = kloft::loft(&sections, &guides, l.start_tangent, l.end_tangent)?;
        let (p0, pn) = (sections[0].plane, sections[sections.len() - 1].plane);
        let first = sections[0].region.clone();
        let shape = Shape::new(vec![solid], p0, move |p| {
            let on = |pl: &Plane| dot(sub(p, pl.origin), pl.normal()).abs() < 1e-4;
            if on(&p0) {
                ToolFace::Start
            } else if on(&pn) {
                ToolFace::End
            } else {
                ToolFace::Side(threebcad_kernel::tool::nearest_segment(
                    std::slice::from_ref(&first),
                    p0.to_local(p),
                ))
            }
        });
        let step = Step {
            feature: self.id,
            instance: 0,
            op: if l.cut { OpKind::Sub } else { OpKind::Add },
            tool: Tool::Shape(shape),
        };
        self.apply(step, l.merge)
    }

    fn thread(&mut self, t: &ThreadDef) -> Result<()> {
        let size = holes::find(t.standard, &t.size).ok_or_else(|| anyhow!("unknown thread size {}", t.size))?;
        let (bi, f) = self.resolve_face(&t.face)?;
        let body = &self.s.bodies[bi];
        let face = &body.topo.faces[f];
        let (c, d, r) = face
            .cylinder
            .ok_or_else(|| anyhow!("select a cylindrical face (a rod or a hole) to thread"))?;
        let d = unit(d);
        let (mut smin, mut smax) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in face.tris.iter().flatten() {
            let s = dot(sub(*p, c), d);
            smin = smin.min(s);
            smax = smax.max(s);
        }
        let tris: Vec<[P3; 3]> = body.topo.faces.iter().flat_map(|f| f.tris.iter().copied()).collect();
        // Material just inside the face: a rod (external thread); else a hole.
        let radial = {
            let q = sub(face.sample, c);
            unit(sub(q, scale3(d, dot(q, d))))
        };
        let at = |s: f64, rr: f64| add3(add3(c, scale3(d, s)), scale3(radial, rr));
        let mid = (smin + smax) / 2.0;
        let internal = !inside(at(mid, r * 0.97), &tris);
        let pitch = size.pitch;
        let major = size.major / 2.0;
        let minor = major - 5.0 * 3f64.sqrt() / 2.0 * pitch / 8.0;
        if internal {
            ensure!(
                r < major,
                "the hole ({:.2} mm) is wider than the thread {}",
                2.0 * r,
                t.size
            );
            ensure!(
                r > minor - 0.2 * pitch,
                "the hole ({:.2} mm) is too small for {}: drill {:.2} mm",
                2.0 * r,
                t.size,
                size.tap_drill
            );
        } else {
            ensure!(
                r > minor,
                "the rod ({:.2} mm) is thinner than the thread's minor diameter",
                2.0 * r
            );
            ensure!(
                r < major + 0.2 * pitch,
                "the rod ({:.2} mm) is too thick for {}",
                2.0 * r,
                t.size
            );
        }
        let probe_r = if internal {
            (r + major) / 2.0
        } else {
            (minor + r.min(major)) / 2.0
        };
        let free = |s: f64| !inside(at(s, probe_r), &tris);
        let len_face = smax - smin;
        let length = t.length.unwrap_or(len_face - t.offset).min(len_face - t.offset);
        ensure!(length > 0.25 * pitch, "the thread is shorter than a quarter pitch");
        // Thread from the start end (smax, or smin when flipped).
        let (mut a, mut b) = if t.flip {
            (smin + t.offset, smin + t.offset + length)
        } else {
            (smax - t.offset - length, smax - t.offset)
        };
        let tol = 1e-3;
        let free_a = (a - smin).abs() < tol && free(smin - 0.5 * pitch);
        let free_b = (smax - b).abs() < tol && free(smax + 0.5 * pitch);
        // An end against a shoulder stops half a pitch short of it.
        if !free_a && (a - smin).abs() < tol {
            a += 0.5 * pitch;
        }
        if !free_b && (smax - b).abs() < tol {
            b -= 0.5 * pitch;
        }
        ensure!(b - a > 0.25 * pitch, "the thread does not fit on the face");
        let spec = threebcad_kernel::thread::ThreadSpec {
            origin: c,
            axis: d,
            x: threebcad_kernel::thread::across(d),
            pitch,
            major,
            wall: r,
            internal,
            right: t.right,
            a,
            b,
            free_a,
            free_b,
        };
        let solid = threebcad_kernel::thread::thread(&body.solid, &spec)?;
        self.direct(bi, solid);
        self.s.threads.insert(self.id, size.thread(t.standard));
        Ok(())
    }

    fn variable_fillet(&mut self, edge: &crate::naming::EdgeRef, radii: &[[f64; 2]]) -> Result<()> {
        let (b, e) = self.resolve_edge(edge)?;
        let topo = self.s.bodies[b].topo.clone();
        let (start, dir) = match topo.edges[e].curve {
            EdgeCurve::Line { a, b } => (a, unit(sub(b, a))),
            _ => bail!("a variable fillet needs a straight edge"),
        };
        let plane = Plane::from_origin_normal(start, dir);
        // truck accepts some builds of the same tool and not others: try each in turn.
        let mut last = None;
        for attempt in 0..edge_ops::VARIABLE_FILLET_TRIES {
            let (solid, add) = edge_ops::variable_fillet(&topo, e, radii, attempt)?;
            let shape = Shape::new(vec![solid], plane, |_| ToolFace::Side(0));
            let step = Step {
                feature: self.id,
                instance: 0,
                op: if add { OpKind::Add } else { OpKind::Sub },
                tool: Tool::Shape(shape),
            };
            let before = self.s.bodies.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.apply(step, true)))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("the geometry kernel could not compute this fillet")));
            match result {
                Ok(()) => return Ok(()),
                Err(err) => {
                    self.s.bodies = before;
                    last = Some(err);
                }
            }
        }
        Err(last.expect("at least one attempt"))
    }

    fn face_fillet(&mut self, a: &[crate::naming::FaceRef], b: &[crate::naming::FaceRef], radius: f64) -> Result<()> {
        ensure!(!a.is_empty() && !b.is_empty(), "Select faces for both face sets");
        let fa: Vec<(usize, usize)> = a.iter().map(|f| self.resolve_face(f)).collect::<Result<_>>()?;
        let fb: Vec<(usize, usize)> = b.iter().map(|f| self.resolve_face(f)).collect::<Result<_>>()?;
        let mut picked = Vec::new();
        for (bi, body) in self.s.bodies.iter().enumerate() {
            for (ei, e) in body.topo.edges.iter().enumerate() {
                if e.sides.len() != 2 {
                    continue;
                }
                let (x, y) = ((bi, e.sides[0].face), (bi, e.sides[1].face));
                if (fa.contains(&x) && fb.contains(&y)) || (fa.contains(&y) && fb.contains(&x)) {
                    picked.push((bi, ei));
                }
            }
        }
        ensure!(
            !picked.is_empty(),
            "the two face sets do not meet along an edge (fillets across a gap are not supported yet)"
        );
        self.edge_feature_at(picked, Treatment::Fillet { radius }, false, false)
    }

    /// Body indices named by a face of each (deduplicated, in order).
    fn bodies_of(&self, faces: &[crate::naming::FaceRef]) -> Result<Vec<usize>> {
        let mut out: Vec<usize> = Vec::new();
        for f in faces {
            let (b, _) = self.resolve_face(f)?;
            if !out.contains(&b) {
                out.push(b);
            }
        }
        Ok(out)
    }

    /// Replaces bodies `old` (indices) with `new` solids made from them.
    fn replace_bodies(&mut self, old: &[usize], new: Vec<Solid>) {
        let mut olds: Vec<Body> = Vec::new();
        let mut idx: Vec<usize> = old.to_vec();
        idx.sort_unstable();
        for &i in idx.iter().rev() {
            olds.push(self.s.bodies.remove(i));
        }
        let mut csg: Vec<Step> = olds.last().map(|b| b.csg.clone()).unwrap_or_default();
        csg.push(Step {
            feature: self.id,
            instance: 0,
            op: OpKind::Direct,
            tool: csg.first().map(|s| s.tool.clone()).unwrap_or(Tool::Prism {
                regions: Vec::new(),
                plane: Plane::TOP,
                z0: 0.0,
                z1: 0.0,
                draft: None,
            }),
        });
        let src: Vec<(&Topology, &[FaceTag])> = olds.iter().map(|b| (&*b.topo, &b.tags[..])).collect();
        let bodies: Vec<Body> = new.into_iter().map(|s| self.make_body(s, &src, csg.clone())).collect();
        let at = idx
            .first()
            .copied()
            .unwrap_or(self.s.bodies.len())
            .min(self.s.bodies.len());
        for (k, b) in bodies.into_iter().enumerate() {
            self.s.bodies.insert(at + k, b);
        }
    }

    fn combine(
        &mut self,
        op: CombineOp,
        main: &crate::naming::FaceRef,
        tools: &[crate::naming::FaceRef],
    ) -> Result<()> {
        let (m, _) = self.resolve_face(main)?;
        let tools: Vec<usize> = self.bodies_of(tools)?.into_iter().filter(|&t| t != m).collect();
        ensure!(!tools.is_empty(), "Select the other bodies to combine with");
        let mut acc = vec![self.s.bodies[m].solid.clone()];
        for &t in &tools {
            let ts = &self.s.bodies[t].solid;
            let mut next = Vec::new();
            for a in &acc {
                match op {
                    CombineOp::Add => match boolean::union(a, ts)? {
                        Some(u) => next.push(u),
                        None => bail!("the bodies do not touch, so they cannot be added"),
                    },
                    CombineOp::Subtract => next.extend(boolean::subtract(a, ts)?),
                    CombineOp::Common => next.extend(boolean::intersect(a, ts)?),
                }
            }
            acc = next;
        }
        ensure!(!acc.is_empty(), "nothing is left of the body");
        let mut all = vec![m];
        all.extend(&tools);
        self.replace_bodies(&all, acc);
        Ok(())
    }

    fn split(&mut self, h: &History, tool: &SplitTool, bodies: &[crate::naming::FaceRef]) -> Result<()> {
        let targets = if bodies.is_empty() {
            (0..self.s.bodies.len()).collect()
        } else {
            self.bodies_of(bodies)?
        };
        ensure!(!targets.is_empty(), "there is no body to split");
        let (lo, hi) = self.body_bbox().ok_or_else(|| anyhow!("there is no body to split"))?;
        let reach = norm(sub(hi, lo)) + 10.0;
        // A plane that leaves a body wholly on one side (touching it at most, like the plane of its
        // own bottom face) does not cut it; truck would fail or shave off a paper-thin body.
        let mut beside: Vec<usize> = Vec::new();
        let cutter: Vec<Solid> = match tool {
            SplitTool::Plane(p) => {
                let plane = self.plane_of(p)?;
                let n = plane.normal();
                for &t in &targets {
                    let (blo, bhi) = boolean::bbox(&self.s.bodies[t].solid);
                    let eps = 1e-6 * (1.0 + norm(sub(bhi, blo)));
                    let d: Vec<f64> = (0..8)
                        .map(|k| {
                            let c = [0, 1, 2].map(|i| if k >> i & 1 == 0 { blo[i] } else { bhi[i] });
                            dot(sub(c, plane.origin), n)
                        })
                        .collect();
                    if d.iter().all(|&x| x <= eps) || d.iter().all(|&x| x >= -eps) {
                        beside.push(t);
                    }
                }
                let c = plane.to_local(scale3(add3(lo, hi), 0.5));
                let sq = threebcad_kernel::tool::polygon(&[
                    [c[0] - reach, c[1] - reach],
                    [c[0] + reach, c[1] - reach],
                    [c[0] + reach, c[1] + reach],
                    [c[0] - reach, c[1] + reach],
                ])?;
                Tool::Prism {
                    regions: vec![sq],
                    plane,
                    z0: 0.0,
                    z1: 2.0 * reach,
                    draft: None,
                }
                .solids()?
            }
            SplitTool::Sketch(id) => {
                let (sketch, plane) = self.sketch(h, *id)?;
                let regions = regions_of(sketch)?;
                ensure!(!regions.is_empty(), "the split sketch has no closed outline");
                Tool::Prism {
                    regions,
                    plane,
                    z0: -2.0 * reach,
                    z1: 2.0 * reach,
                    draft: None,
                }
                .solids()?
            }
        };
        let mut new = Vec::new();
        let mut touched = Vec::new();
        for &t in targets.iter().filter(|t| !beside.contains(t)) {
            let s = self.s.bodies[t].solid.clone();
            let mut inside = Vec::new();
            let mut rest = vec![s.clone()];
            for c in &cutter {
                inside.extend(boolean::intersect(&s, c)?);
                rest = rest
                    .iter()
                    .map(|r| boolean::subtract(r, c))
                    .collect::<Result<Vec<_>>>()?
                    .concat();
            }
            // A cutter face lying on a face of the body leaves paper-thin pieces: they are not parts.
            let whole = boolean::volume(&s).unwrap_or(0.0).abs();
            let solid_piece = |p: &Solid| boolean::volume(p).is_some_and(|v| v.abs() > 1e-6 * whole.max(1e-9));
            inside.retain(solid_piece);
            rest.retain(solid_piece);
            if inside.is_empty() || rest.is_empty() {
                continue;
            }
            touched.push(t);
            new.extend(inside);
            new.extend(rest);
        }
        ensure!(!touched.is_empty(), "the split does not cut any body");
        self.replace_bodies(&touched, new);
        Ok(())
    }

    fn delete_bodies(&mut self, faces: &[crate::naming::FaceRef], keep: bool) -> Result<()> {
        ensure!(!faces.is_empty(), "Select the bodies");
        let picked = self.bodies_of(faces)?;
        let mut i = 0;
        self.s.bodies.retain(|_| {
            let k = picked.contains(&i) == keep;
            i += 1;
            k
        });
        Ok(())
    }

    fn move_bodies(&mut self, m: &MoveBody) -> Result<()> {
        let targets: Vec<usize> = if m.bodies.is_empty() {
            (0..self.s.bodies.len()).collect()
        } else {
            self.bodies_of(&m.bodies)?
        };
        ensure!(!targets.is_empty(), "there is no body to move");
        let mut mat = mat_translate(m.translate);
        if let Some((axis, angle)) = &m.rotate {
            let (o, d) = self.axis_of(axis)?;
            mat = mat_mul(&mat_rotate(o, unit(d), *angle), &mat);
        }
        if let Some((face, plane)) = &m.mate {
            let (b, f) = self.resolve_face(face)?;
            ensure!(targets.contains(&b), "the mated face must be on a moved body");
            let fp = self.s.bodies[b].topo.faces[f]
                .plane
                .ok_or_else(|| anyhow!("the mated face must be flat"))?;
            let target = self.plane_of(plane)?;
            let lin = |v: P3| sub(apply_mat(&mat, v), apply_mat(&mat, [0.0; 3]));
            let (o, n) = (apply_mat(&mat, fp.origin), unit(lin(fp.normal())));
            // Face against the plane: normals opposed, then onto it.
            let want = scale3(target.normal(), -1.0);
            let r = mat_align(o, n, want);
            let o2 = apply_mat(&r, o);
            let shift = scale3(target.normal(), dot(sub(target.origin, o2), target.normal()));
            mat = mat_mul(&mat_translate(shift), &mat_mul(&r, &mat));
        }
        let mut added = Vec::new();
        for (k, &t) in targets.iter().enumerate() {
            let old = self.s.bodies[t].clone();
            let solid = boolean::transformed(&old.solid, mat);
            let topo = analyze(&solid);
            let tags = if old.tags.len() == topo.faces.len() {
                old.tags
                    .iter()
                    .map(|tg| FaceTag {
                        instance: if m.copy {
                            tg.instance + 1000 * (k as u32 + 1)
                        } else {
                            tg.instance
                        },
                        ..*tg
                    })
                    .collect()
            } else {
                naming::inherit_tags(&topo, &[], self.id)
            };
            let csg = old
                .csg
                .iter()
                .map(|s| Step {
                    tool: s.tool.transformed(mat),
                    ..s.clone()
                })
                .collect();
            let body = Body { solid, topo, tags, csg };
            if m.copy {
                added.push(body);
            } else {
                self.s.bodies[t] = body;
            }
        }
        self.s.bodies.extend(added);
        Ok(())
    }

    fn scale_bodies(&mut self, faces: &[crate::naming::FaceRef], about: &PointRef, f: [f64; 3]) -> Result<()> {
        ensure!(f.iter().all(|v| *v > 1e-6), "scale factors must be positive");
        let targets: Vec<usize> = if faces.is_empty() {
            (0..self.s.bodies.len()).collect()
        } else {
            self.bodies_of(faces)?
        };
        ensure!(!targets.is_empty(), "there is no body to scale");
        let p = self.point_of(about)?;
        let mat = [
            [f[0], 0.0, 0.0, 0.0],
            [0.0, f[1], 0.0, 0.0],
            [0.0, 0.0, f[2], 0.0],
            [p[0] * (1.0 - f[0]), p[1] * (1.0 - f[1]), p[2] * (1.0 - f[2]), 1.0],
        ];
        for &t in &targets {
            let old = self.s.bodies[t].clone();
            let solid = boolean::transformed(&old.solid, mat);
            let topo = analyze(&solid);
            let tags = if old.tags.len() == topo.faces.len() {
                old.tags.clone()
            } else {
                naming::inherit_tags(&topo, &[], self.id)
            };
            let mut csg = old.csg.clone();
            // Tools cannot be scaled: later replays must not rebuild through this.
            csg.push(Step {
                feature: self.id,
                instance: 0,
                op: OpKind::Direct,
                tool: csg.first().map(|s| s.tool.clone()).unwrap_or(Tool::Prism {
                    regions: Vec::new(),
                    plane: Plane::TOP,
                    z0: 0.0,
                    z1: 0.0,
                    draft: None,
                }),
            });
            self.s.bodies[t] = Body { solid, topo, tags, csg };
        }
        Ok(())
    }

    /// Where the seeds are: the middle of their tools (features) or of the bodies.
    fn seeds_center(&self, seeds: &Seeds) -> Result<P3> {
        let solids: Vec<Solid> = match seeds {
            Seeds::Features(fs) => {
                let mut out = Vec::new();
                for f in fs {
                    for s in self.s.ops.get(f).into_iter().flatten() {
                        out.extend(s.tool.solids()?);
                    }
                }
                out
            }
            Seeds::Bodies => self.s.bodies.iter().map(|b| b.solid.clone()).collect(),
        };
        bbox_center(&solids).ok_or_else(|| anyhow!("the selected features add or remove no material"))
    }

    fn sketch_pattern(&mut self, h: &History, seeds: &Seeds, sketch_id: FeatureId) -> Result<()> {
        let (sketch, plane) = self.sketch(h, sketch_id)?;
        let pts: Vec<[f64; 2]> = sketch
            .point_ids()
            .filter(|&p| sketch.entities_at(p).is_empty())
            .map(|p| sketch.point(p))
            .collect();
        ensure!(!pts.is_empty(), "Place sketch points where the copies go");
        let c = self.seeds_center(seeds)?;
        let cl = plane.to_local(c);
        let mats: Vec<_> = pts
            .iter()
            .filter(|p| (p[0] - cl[0]).hypot(p[1] - cl[1]) > 1e-6)
            .map(|p| mat_translate(sub(plane.to_world(*p), plane.to_world(cl))))
            .collect();
        ensure!(!mats.is_empty(), "every sketch point is on the seed");
        self.pattern(seeds, &mats, false)
    }

    fn curve_pattern(
        &mut self,
        h: &History,
        seeds: &Seeds,
        curve: CurveRef,
        count: usize,
        spacing: Option<f64>,
        align: bool,
    ) -> Result<()> {
        ensure!(count >= 2, "the pattern needs two instances or more");
        let c = self.seeds_center(seeds)?;
        let path = self.curve_path(h, curve, Some(c))?;
        let pts = path_points(&path);
        ensure!(pts.len() >= 2, "the curve is too short");
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum.last().unwrap() + norm(sub(w[1], w[0])));
        }
        let total = *cum.last().unwrap();
        let closed = norm(sub(pts[0], pts[pts.len() - 1])) < 1e-6;
        let step = match spacing {
            Some(s) => s,
            None if closed => total / count as f64,
            None => total / (count - 1) as f64,
        };
        ensure!(step > 1e-6, "the spacing must be positive");
        let at = |s: f64| -> (P3, P3) {
            let i = cum.partition_point(|&v| v <= s).clamp(1, pts.len() - 1);
            let f = ((s - cum[i - 1]) / (cum[i] - cum[i - 1]).max(1e-300)).clamp(0.0, 1.0);
            let p = add3(scale3(pts[i - 1], 1.0 - f), scale3(pts[i], f));
            (p, unit(sub(pts[i], pts[i - 1])))
        };
        let (p0, t0) = at(0.0);
        let mut mats = Vec::new();
        for k in 1..count {
            let s = step * k as f64;
            ensure!(
                s <= total + 1e-6,
                "instance {} would run off the end of the curve",
                k + 1
            );
            let (p, t) = at(s.min(total));
            let mv = mat_translate(sub(p, p0));
            mats.push(if align { mat_mul(&mat_align(p, t0, t), &mv) } else { mv });
        }
        self.pattern(seeds, &mats, false)
    }

    /// The outer loop of a planar face, in its plane's frame.
    fn face_outline(&self, b: usize, f: usize, plane: &Plane) -> Result<Vec<ProfileSegment>> {
        let topo = &self.s.bodies[b].topo;
        let edges: Vec<&topology::TopoEdge> = topo
            .edges
            .iter()
            .filter(|e| e.sides.iter().any(|s| s.face == f))
            .collect();
        ensure!(!edges.is_empty(), "the face has no edges");
        let loc = |p: P3| plane.to_local(p);
        // Each edge as segments from its first to its last point.
        let segs_of = |e: &topology::TopoEdge| -> Vec<ProfileSegment> {
            match e.curve {
                EdgeCurve::Line { a, b } => vec![ProfileSegment::Line {
                    from: loc(a),
                    to: loc(b),
                }],
                EdgeCurve::Circle { center, .. } if e.vertices[0] == e.vertices[1] => {
                    let c = loc(center);
                    let r = norm(sub(e.points[0], center));
                    vec![ProfileSegment::Circle {
                        center: c,
                        radius: r,
                        ccw: true,
                    }]
                }
                EdgeCurve::Circle { center, .. } => {
                    let (c, a, m, z) = (
                        loc(center),
                        loc(e.points[0]),
                        loc(e.mid),
                        loc(*e.points.last().unwrap()),
                    );
                    let ang = |q: [f64; 2]| (q[1] - c[1]).atan2(q[0] - c[0]);
                    let ccw = (ang(m) - ang(a)).rem_euclid(TAU) < (ang(z) - ang(a)).rem_euclid(TAU);
                    vec![ProfileSegment::Arc {
                        center: c,
                        from: a,
                        to: z,
                        ccw,
                    }]
                }
                EdgeCurve::Other => e
                    .points
                    .windows(2)
                    .map(|w| ProfileSegment::Line {
                        from: loc(w[0]),
                        to: loc(w[1]),
                    })
                    .collect(),
            }
        };
        let mut pieces: Vec<Vec<ProfileSegment>> = edges.iter().map(|e| segs_of(e)).collect();
        // A whole circle on its own.
        if let Some(i) = pieces
            .iter()
            .position(|p| matches!(p[..], [ProfileSegment::Circle { .. }]))
        {
            ensure!(pieces.len() == 1, "dome a face without holes");
            return Ok(pieces.swap_remove(i));
        }
        let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-4;
        let mut loops: Vec<Vec<ProfileSegment>> = Vec::new();
        while let Some(mut lp) = pieces.pop() {
            loop {
                let end = offset::seg_end(lp.last().unwrap());
                if close(end, offset::seg_start(&lp[0])) {
                    break;
                }
                let Some(k) = pieces.iter().position(|p| {
                    close(offset::seg_start(&p[0]), end) || close(offset::seg_end(p.last().unwrap()), end)
                }) else {
                    bail!("the face's edges do not form a closed loop");
                };
                let p = pieces.swap_remove(k);
                if close(offset::seg_start(&p[0]), end) {
                    lp.extend(p);
                } else {
                    lp.extend(reverse_chain(&p));
                }
            }
            loops.push(lp);
        }
        ensure!(loops.len() == 1, "dome a face without holes");
        let mut lp = loops.pop().unwrap();
        // Snap joints exactly.
        let n = lp.len();
        for i in 0..n {
            let e = offset::seg_end(&lp[i]);
            match &mut lp[(i + 1) % n] {
                ProfileSegment::Line { from, .. } | ProfileSegment::Arc { from, .. } => *from = e,
                _ => {}
            }
        }
        if offset::signed_area(&lp) < 0.0 {
            lp = reverse_chain(&lp);
        }
        Ok(lp)
    }

    fn dome(&mut self, face: &crate::naming::FaceRef, height: f64, reverse: bool) -> Result<()> {
        ensure!(height > 1e-6, "the dome height must be positive");
        let (b, f) = self.resolve_face(face)?;
        let fp = self.s.bodies[b].topo.faces[f]
            .plane
            .ok_or_else(|| anyhow!("only flat faces can be domed"))?;
        let outline = self.face_outline(b, f, &fp)?;
        ensure!(
            outline.len() <= 64,
            "the face's outline is too detailed to dome ({} pieces); dome a face bounded by lines and arcs",
            outline.len()
        );
        // A circular face: an exact cap built into the body (a boolean cannot join the two on the
        // circle they share).
        let circle = match outline[..] {
            [ProfileSegment::Circle { center, radius, .. }] => Some((center, radius)),
            _ if outline.iter().all(|s| matches!(s, ProfileSegment::Arc { .. })) => {
                let ProfileSegment::Arc { center, from, .. } = outline[0] else {
                    unreachable!()
                };
                let same = outline.iter().all(|s| matches!(*s, ProfileSegment::Arc { center: c, .. } if (c[0] - center[0]).hypot(c[1] - center[1]) < 1e-6));
                same.then(|| (center, (from[0] - center[0]).hypot(from[1] - center[1])))
            }
            _ => None,
        };
        if let Some((c, r)) = circle {
            let h = if reverse { -height } else { height };
            let body = &self.s.bodies[b].solid;
            let solid = threebcad_kernel::round::dome_circle(body, fp.to_world(c), fp.normal(), r, h)?;
            self.direct(b, solid);
            return Ok(());
        }
        let solid = threebcad_kernel::dome::dome(&outline, &fp, if reverse { -height } else { height })?;
        let shape = Shape::new(vec![solid], fp, move |p| {
            if dot(sub(p, fp.origin), fp.normal()).abs() < 1e-4 {
                ToolFace::Start
            } else {
                ToolFace::Side(0)
            }
        });
        let step = Step {
            feature: self.id,
            instance: 0,
            op: if reverse { OpKind::Sub } else { OpKind::Add },
            tool: Tool::Shape(shape),
        };
        self.apply(step, true)
    }

    fn wrap(
        &mut self,
        h: &History,
        sketch_id: FeatureId,
        face: &crate::naming::FaceRef,
        kind: WrapKind,
        depth: f64,
    ) -> Result<()> {
        ensure!(depth > 1e-6, "the wrap depth must be positive");
        let (sketch, plane) = self.sketch(h, sketch_id)?;
        let mut regions = regions_of(sketch)?;
        ensure!(!regions.is_empty(), "the wrap sketch has no closed outline");
        let (b, f) = self.resolve_face(face)?;
        let tf = &self.s.bodies[b].topo.faces[f];
        let (c, d, r) = tf.cylinder.ok_or_else(|| anyhow!("wrap onto a cylindrical face"))?;
        let tris: Vec<[P3; 3]> = self.s.bodies[b]
            .topo
            .faces
            .iter()
            .flat_map(|f| f.tris.iter().copied())
            .collect();
        let radial = {
            let q = sub(tf.sample, c);
            unit(sub(q, scale3(unit(d), dot(q, unit(d)))))
        };
        let probe = add3(
            add3(c, scale3(unit(d), dot(sub(tf.sample, c), unit(d)))),
            scale3(radial, r * 0.97),
        );
        ensure!(
            inside(probe, &tris),
            "wrap onto the outside of a cylinder (a rod or a boss)"
        );
        let eps = (0.1 * depth).min(0.05);
        let (inner, outer, op) = match kind {
            WrapKind::Emboss => (r - eps, r + depth, OpKind::Add),
            WrapKind::Deboss => (r - depth, r + eps, OpKind::Sub),
            WrapKind::Scribe => {
                // A narrow groove along the outline.
                let w = (0.2 * depth).clamp(0.1, 0.5);
                regions = regions
                    .iter()
                    .map(|rg| offset::thin_region(rg, w / 2.0, w / 2.0))
                    .collect::<Result<Vec<_>>>()?
                    .concat();
                (r - depth, r + eps, OpKind::Sub)
            }
        };
        ensure!(inner > 0.05 * r, "the wrap is deeper than the cylinder");
        let cyl = Cylinder {
            center: c,
            axis: unit(d),
            radius: r,
        };
        let solids = kwrap::wrap(&regions, &plane, &cyl, inner, outer)?;
        let rs = regions.clone();
        let shape = Shape::new(solids, plane, move |p| {
            let q = sub(p, c);
            let rr = norm(sub(q, scale3(unit(d), dot(q, unit(d)))));
            if (rr - outer).abs() < 1e-3 {
                ToolFace::End
            } else if (rr - inner).abs() < 1e-3 {
                ToolFace::Start
            } else {
                ToolFace::Side(threebcad_kernel::tool::nearest_segment(&rs, plane.to_local(p)))
            }
        });
        let step = Step {
            feature: self.id,
            instance: 0,
            op,
            tool: Tool::Shape(shape),
        };
        self.apply(step, true)
    }
}
