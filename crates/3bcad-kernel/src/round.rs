//! Fillets on circular edges built directly into the B-Rep. A torus tool meets the plane and the
//! cylinder of the edge tangentially, which truck's booleans cannot intersect; instead the flat
//! face's circle and the cylinder's rim are pulled back by the radius and a band of exact torus
//! faces (NURBS surfaces of revolution) is stitched between them, sharing their edges.
//!
//! Handles a whole circle between a plane across the axis and a coaxial cylinder: rod ends, hole
//! rims and the root of a boss (concave, material added).

use std::collections::HashMap;
use std::f64::consts::{FRAC_1_SQRT_2, PI, TAU};

use anyhow::{anyhow, bail, ensure, Result};
use truck_modeling::*;

use crate::skin::{arc_piece, Piece};
use crate::topology::{cross, dot, norm, sub, unit, P3};

fn p3(p: Point3) -> P3 {
    [p.x, p.y, p.z]
}

fn pt(p: P3) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

struct Frame {
    o: P3,
    /// Outward normal of the flat face (the "up" of the corner).
    n: P3,
    x: P3,
    y: P3,
}

impl Frame {
    /// Radius, height above the flat face and angle of a point.
    fn polar(&self, p: P3) -> (f64, f64, f64) {
        let d = sub(p, self.o);
        let h = dot(d, self.n);
        let (u, v) = (dot(d, self.x), dot(d, self.y));
        (u.hypot(v), h, v.atan2(u))
    }

    fn at(&self, r: f64, h: f64, a: f64) -> P3 {
        let (s, c) = a.sin_cos();
        std::array::from_fn(|i| self.o[i] + self.n[i] * h + r * (c * self.x[i] + s * self.y[i]))
    }

    fn arc(&self, r: f64, h: f64, a0: f64, sweep: f64) -> Piece {
        let c: P3 = std::array::from_fn(|i| self.o[i] + self.n[i] * h);
        arc_piece(c, self.x, self.y, r, a0, sweep)
    }
}

fn edge_points(e: &Edge) -> Vec<P3> {
    let c = e.curve();
    let (t0, t1) = c.range_tuple();
    (0..=6).map(|k| p3(c.subs(t0 + (t1 - t0) * k as f64 / 6.0))).collect()
}

/// A face on `surface` with the given (outward-oriented) boundaries, facing `outward(p)` at the
/// middle of the surface.
pub(crate) fn oriented(wires: Vec<Wire>, surface: Surface, outward: impl Fn(P3) -> P3) -> Result<Face> {
    // Our surfaces all run over 0..1 in both directions.
    let (u, v) = (0.5, 0.5);
    let sn = surface.normal(u, v);
    let o = outward(p3(surface.subs(u, v)));
    let face = if sn.x * o[0] + sn.y * o[1] + sn.z * o[2] > 0.0 {
        Face::try_new(wires, surface)
    } else {
        Face::try_new(wires.iter().map(|w| w.inverse()).collect(), surface).map(|f| f.inverse())
    };
    face.map_err(|e| anyhow!("fillet face: {e:?}"))
}

/// Rounds the whole circle of `radius` about `axis` through `center` with a fillet of `rho`.
pub fn round_circle(solid: &Solid, center: P3, axis: P3, radius: f64, rho: f64) -> Result<Solid> {
    ensure!(rho > 0.0, "fillet radius must be positive");
    let axis = unit(axis);
    let tol = 1e-4 * radius.max(1.0) + 1e-3 * rho;
    let on_circle = |p: P3| {
        let d = sub(p, center);
        let h = dot(d, axis);
        (norm(sub(d, axis.map(|v| v * h))) - radius).abs() < tol && h.abs() < tol
    };
    let on_edge = |e: &Edge| edge_points(e).into_iter().all(on_circle);

    // The shell, the flat face and its loop on the circle.
    let (si, fi, li) = solid
        .boundaries()
        .iter()
        .enumerate()
        .find_map(|(si, sh)| {
            sh.face_iter().enumerate().find_map(|(fi, f)| {
                if !matches!(f.surface(), Surface::Plane(_)) {
                    return None;
                }
                f.boundaries()
                    .iter()
                    .position(|w| w.edge_iter().all(on_edge))
                    .map(|li| (si, fi, li))
            })
        })
        .ok_or_else(|| anyhow!("the edge is not a whole circle on a flat face"))?;
    let shell = &solid.boundaries()[si];
    let flat = &shell[fi];
    let Surface::Plane(plane) = flat.oriented_surface() else {
        unreachable!()
    };
    let nv = plane.normal();
    let n = [nv.x, nv.y, nv.z];
    ensure!(
        dot(n, axis).abs() > 1.0 - 1e-6,
        "the flat face is not square to the circle's axis"
    );
    let t = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let x = unit(cross(cross(n, t), n));
    let f = Frame {
        o: center,
        n,
        x,
        y: cross(n, x),
    };
    let lp = &flat.boundaries()[li];
    // Old circle edges in their absolute direction, with their angle spans.
    let mut spans: Vec<(Edge, f64, f64)> = Vec::new();
    for e in lp.edge_iter() {
        let a = e.absolute_clone();
        let (_, _, a0) = f.polar(p3(a.front().point()));
        let (_, _, a1) = f.polar(p3(a.back().point()));
        let (_, _, am) = f.polar(edge_points(&a)[3]);
        // Signed sweep from a0 to a1 passing through am.
        let ccw = (a1 - a0).rem_euclid(TAU);
        let via = (am - a0).rem_euclid(TAU);
        let sweep = if via < ccw { ccw } else { ccw - TAU };
        spans.push((a, a0, sweep));
    }
    ensure!(spans.len() >= 2, "the circle has too few edges");
    // Does the loop run counter-clockwise about n (an outer boundary: the face lies inside)?
    let turn: f64 = lp
        .edge_iter()
        .map(|e| {
            let s = spans.iter().find(|s| s.0.is_same(e)).unwrap().2;
            if e.orientation() {
                s
            } else {
                -s
            }
        })
        .sum();
    let d_a = if turn > 0.0 { -1.0 } else { 1.0 };
    let r_flat = radius + d_a * rho;
    ensure!(r_flat > 1e-6, "the size is too large: it reaches the axis");

    // The cylinder faces on the other side of the circle edges.
    let circle_ids: Vec<_> = spans.iter().map(|s| s.0.id()).collect();
    let walls: Vec<usize> = shell
        .face_iter()
        .enumerate()
        .filter(|&(i, face)| i != fi && face.edge_iter().any(|e| circle_ids.contains(&e.id())))
        .map(|(i, _)| i)
        .collect();
    ensure!(!walls.is_empty(), "the circle has no wall");
    let mut d_b = 0.0;
    for &w in &walls {
        for e in shell[w].edge_iter() {
            for p in edge_points(&e) {
                let (r, h, _) = f.polar(p);
                ensure!(
                    (r - radius).abs() < tol,
                    "the wall at the circle is not a cylinder around its axis"
                );
                if h.abs() > tol {
                    let s = h.signum();
                    ensure!(d_b == 0.0 || d_b == s, "the wall runs both ways from the circle");
                    d_b = s;
                }
            }
        }
    }
    ensure!(d_b != 0.0, "the wall has no height");
    let convex = d_b < 0.0;
    let h_wall = d_b * rho;

    // Room on the flat face and on the wall.
    for (k, w) in flat.boundaries().iter().enumerate() {
        if k == li {
            continue;
        }
        for e in w.edge_iter() {
            for p in edge_points(e) {
                let (r, _, _) = f.polar(p);
                let into = (r - radius) * d_a;
                ensure!(
                    into <= 0.0 || into > rho + tol,
                    "the size is too large: the fillet does not fit on the flat face"
                );
            }
        }
    }

    // New vertices: on the flat face and on the wall, at each old vertex's angle.
    let mut vmap: HashMap<VertexID, (Vertex, Vertex)> = HashMap::new();
    for (e, _, _) in &spans {
        for v in [e.front(), e.back()] {
            vmap.entry(v.id()).or_insert_with(|| {
                let (_, _, a) = f.polar(p3(v.point()));
                (
                    builder::vertex(pt(f.at(r_flat, 0.0, a))),
                    builder::vertex(pt(f.at(radius, h_wall, a))),
                )
            });
        }
    }
    // Replacement circle edges (absolute direction kept) on the flat face and on the wall.
    let mut flat_edges: HashMap<EdgeID, Edge> = HashMap::new();
    let mut wall_edges: HashMap<EdgeID, Edge> = HashMap::new();
    for (e, a0, sweep) in &spans {
        let (vf0, vw0) = &vmap[&e.front().id()];
        let (vf1, vw1) = &vmap[&e.back().id()];
        flat_edges.insert(e.id(), Edge::new(vf0, vf1, f.arc(r_flat, 0.0, *a0, *sweep).curve()));
        wall_edges.insert(e.id(), Edge::new(vw0, vw1, f.arc(radius, h_wall, *a0, *sweep).curve()));
    }
    // Wall edges running up from an old vertex get shorter.
    let mut side_edges: HashMap<EdgeID, Edge> = HashMap::new();
    for &w in &walls {
        for e in shell[w].edge_iter() {
            if circle_ids.contains(&e.id()) || side_edges.contains_key(&e.id()) {
                continue;
            }
            let a = e.absolute_clone();
            let (front, back) = (a.front(), a.back());
            let (moved_front, moved_back) = (vmap.get(&front.id()), vmap.get(&back.id()));
            if moved_front.is_none() && moved_back.is_none() {
                continue;
            }
            ensure!(
                edge_points(&a).iter().all(|&p| {
                    let (r, _, ang) = f.polar(p);
                    let (_, _, a0) = f.polar(p3(front.point()));
                    (r - radius).abs() < tol && ((ang - a0 + PI).rem_euclid(TAU) - PI).abs() < 1e-6
                }),
                "the wall's seam is not straight"
            );
            let far = if moved_front.is_some() { back } else { front };
            let (_, hf, _) = f.polar(p3(far.point()));
            ensure!(
                hf * d_b > rho + tol,
                "the size is too large: the fillet does not fit on the wall"
            );
            let new = match (moved_front, moved_back) {
                (Some((_, vw)), None) => Edge::new(vw, back, Curve::Line(Line(vw.point(), back.point()))),
                (None, Some((_, vw))) => Edge::new(front, vw, Curve::Line(Line(front.point(), vw.point()))),
                _ => bail!("the wall is too short"),
            };
            side_edges.insert(a.id(), new);
        }
    }
    let swap = |e: &Edge, map: &HashMap<EdgeID, Edge>| -> Option<Edge> {
        map.get(&e.id())
            .map(|n| if e.orientation() { n.clone() } else { n.inverse() })
    };
    let rebuild = |face: &Face, maps: &[&HashMap<EdgeID, Edge>]| -> Result<Face> {
        let wires: Vec<Wire> = face
            .boundaries()
            .iter()
            .map(|w| {
                w.edge_iter()
                    .map(|e| maps.iter().find_map(|m| swap(e, m)).unwrap_or(e.clone()))
                    .collect()
            })
            .collect();
        crate::topology::face_like(face, wires).map_err(|e| anyhow!("fillet face: {e:?}"))
    };

    // Meridian quarter arcs at each old vertex, from the flat face down (or up) to the wall.
    let mut meridians: HashMap<VertexID, Edge> = HashMap::new();
    let merid_cps = |a: f64| -> Vec<[f64; 4]> {
        let hom = |p: P3, w: f64| [p[0] * w, p[1] * w, p[2] * w, w];
        vec![
            hom(f.at(r_flat, 0.0, a), 1.0),
            hom(f.at(radius, 0.0, a), FRAC_1_SQRT_2),
            hom(f.at(radius, h_wall, a), 1.0),
        ]
    };
    for (id, (vf, vw)) in &vmap {
        let (_, _, a) = f.polar(p3(vf.point()));
        let piece = Piece {
            cps: merid_cps(a),
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        };
        meridians.insert(*id, Edge::new(vf, vw, piece.curve()));
    }

    let mut faces: Vec<Face> = Vec::new();
    for (i, face) in shell.face_iter().enumerate() {
        if i == fi {
            faces.push(rebuild(face, &[&flat_edges])?);
        } else if walls.contains(&i) {
            // The wall gets its own exact cylinder over just its span: truck cannot find the new
            // edges' parameters on the cut's full-turn surface reliably.
            let shorter = rebuild(face, &[&wall_edges, &side_edges])?;
            let pts: Vec<P3> = shorter
                .boundaries()
                .iter()
                .flat_map(|w| w.edge_iter().flat_map(edge_points).collect::<Vec<_>>())
                .collect();
            let ref_a = face
                .edge_iter()
                .find_map(|e| spans.iter().find(|s| s.0.is_same(&e)).map(|s| s.1 + s.2 / 2.0))
                .unwrap();
            let (mut lo, mut hi, mut h0, mut h1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for &p in &pts {
                let (_, h, a) = f.polar(p);
                let off = (a - ref_a + PI).rem_euclid(TAU) - PI;
                lo = lo.min(off);
                hi = hi.max(off);
                h0 = h0.min(h);
                h1 = h1.max(h);
            }
            let (ma, mh) = (1e-3, 1e-3 * (h1 - h0));
            let (a0, sweep) = (ref_a + lo - ma, hi - lo + 2.0 * ma);
            let (lo_ring, hi_ring) = (f.arc(radius, h0 - mh, a0, sweep), f.arc(radius, h1 + mh, a0, sweep));
            let rows: Vec<Vec<Vector4>> = lo_ring
                .cps
                .iter()
                .zip(&hi_ring.cps)
                .map(|(c, d)| {
                    vec![
                        Vector4::new(c[0], c[1], c[2], c[3]),
                        Vector4::new(d[0], d[1], d[2], d[3]),
                    ]
                })
                .collect();
            let surface = Surface::NurbsSurface(NurbsSurface::new(BSplineSurface::new(
                (
                    KnotVec::from(lo_ring.knots.clone()),
                    KnotVec::from(vec![0.0, 0.0, 1.0, 1.0]),
                ),
                rows,
            )));
            let out_r = d_a * d_b;
            faces.push(oriented(shorter.boundaries(), surface, |p| {
                let (r, h, a) = f.polar(p);
                let _ = (r, h);
                f.at(out_r, 0.0, a)
                    .iter()
                    .zip(&f.o)
                    .map(|(x, o)| x - o)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap()
            })?);
        } else {
            faces.push(face.clone());
        }
    }
    // Torus faces, one per old circle edge.
    for e in lp.edge_iter() {
        let (_, a0, sweep) = spans.iter().find(|s| s.0.is_same(e)).unwrap();
        // The torus uses the flat edge against the flat face and the wall edge against the wall.
        let fe = swap(e, &flat_edges).unwrap().inverse();
        let we = swap(e, &wall_edges).unwrap();
        // The wall used the old edge the other way round from the flat face, so `we` (as the flat
        // face used it) runs the way the torus needs it.
        let vid = |v: &Vertex| -> VertexID {
            *vmap
                .iter()
                .find(|(_, (vf, vw))| vf.id() == v.id() || vw.id() == v.id())
                .unwrap()
                .0
        };
        let m_end = meridians[&vid(fe.back())].clone();
        let m_start = meridians[&vid(we.back())].inverse();
        let wire: Wire = vec![fe.clone(), m_end, we.clone(), m_start].into();
        // Surface of revolution: u around (the old edge's direction), v down the meridian.
        let ring = |r: f64, h: f64| f.arc(r, h, *a0, *sweep);
        let mer = merid_cps(0.0);
        let (rs, hs): (Vec<f64>, Vec<f64>) = mer
            .iter()
            .map(|c| {
                let (r, h, _) = f.polar([c[0] / c[3], c[1] / c[3], c[2] / c[3]]);
                (r, h)
            })
            .unzip();
        let rings: Vec<Piece> = (0..3).map(|j| ring(rs[j], hs[j])).collect();
        let uk = rings[0].knots.clone();
        let rows: Vec<Vec<Vector4>> = (0..rings[0].cps.len())
            .map(|i| {
                (0..3)
                    .map(|j| {
                        let c = rings[j].cps[i];
                        let w = mer[j][3];
                        Vector4::new(c[0] * w, c[1] * w, c[2] * w, c[3] * w)
                    })
                    .collect()
            })
            .collect();
        let surface = Surface::NurbsSurface(NurbsSurface::new(BSplineSurface::new(
            (KnotVec::from(uk), KnotVec::from(vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0])),
            rows,
        )));
        // Outward: away from the fillet's center circle when convex, towards it when concave.
        let mid_a = a0 + sweep / 2.0;
        let mid = f.at(
            radius + d_a * rho * (1.0 - FRAC_1_SQRT_2),
            h_wall * (1.0 - FRAC_1_SQRT_2),
            mid_a,
        );
        let c_mid = f.at(r_flat, h_wall, mid_a);
        let mut out = unit(sub(mid, c_mid));
        if !convex {
            out = out.map(|v| -v);
        }
        faces.push(oriented(vec![wire], surface, |_| out)?);
    }
    let new_shell: Shell = faces.into();
    ensure!(
        new_shell.shell_condition() == ShellCondition::Closed,
        "the rounded body does not close"
    );
    let mut shells = solid.boundaries().clone();
    shells[si] = new_shell;
    Solid::try_new(shells).map_err(|e| anyhow!("the rounded body is invalid: {e:?}"))
}

/// Raises (`height` > 0) or sinks (`height` < 0) the flat face bounded by the whole circle of
/// `radius` about `axis` through `center` into a spherical cap, built directly: the face is
/// replaced by sphere-zone faces on the circle's own edges (so the wall keeps them) and a tiny flat
/// at the top, where the patches would otherwise meet in a pole.
pub fn dome_circle(solid: &Solid, center: P3, axis: P3, radius: f64, height: f64) -> Result<Solid> {
    ensure!(height.abs() > 1e-6, "the dome height must not be zero");
    ensure!(
        height.abs() <= radius * (1.0 + 1e-9),
        "a dome higher than the face's radius would overhang it"
    );
    let axis = unit(axis);
    let tol = 1e-4 * radius.max(1.0);
    let on_circle = |p: P3| {
        let d = sub(p, center);
        let h = dot(d, axis);
        (norm(sub(d, axis.map(|v| v * h))) - radius).abs() < tol && h.abs() < tol
    };
    let on_edge = |e: &Edge| edge_points(e).into_iter().all(on_circle);
    let (si, fi) = solid
        .boundaries()
        .iter()
        .enumerate()
        .find_map(|(si, sh)| {
            sh.face_iter()
                .position(|f| {
                    matches!(f.surface(), Surface::Plane(_))
                        && f.boundaries().len() == 1
                        && f.boundaries()[0].edge_iter().all(&on_edge)
                })
                .map(|fi| (si, fi))
        })
        .ok_or_else(|| anyhow!("the face is not a flat disc bounded by the circle"))?;
    let shell = &solid.boundaries()[si];
    let flat = &shell[fi];
    let Surface::Plane(plane) = flat.oriented_surface() else {
        unreachable!()
    };
    let nv = plane.normal();
    let n = [nv.x, nv.y, nv.z];
    let t = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let x = unit(cross(cross(n, t), n));
    let f = Frame {
        o: center,
        n,
        x,
        y: cross(n, x),
    };
    // The sphere: centre at height c on the axis, radius big_r; meridian angle phi from the axis.
    let (r, h) = (radius, height);
    let sg = h.signum();
    let c = (h * h - r * r) / (2.0 * h);
    let big_r = (r * r + h * h) / (2.0 * h.abs());
    let phi_rim = (-c * sg / big_r).clamp(-1.0, 1.0).acos();
    let phi_top = 0.01 * phi_rim;
    let mer_at = |phi: f64| (big_r * phi.sin(), c + sg * big_r * phi.cos());
    let (r_top, h_top) = mer_at(phi_top);
    // The meridian from the rim up to the top circle: one rational quadratic arc.
    let half = (phi_rim - phi_top) / 2.0;
    let wm = half.cos();
    let pm = mer_at(phi_top + half);
    let mid = (pm.0 / wm, c + (pm.1 - c) / wm);
    let mer: [(f64, f64, f64); 3] = [(r, 0.0, 1.0), (mid.0, mid.1, wm), (r_top, h_top, 1.0)];

    let lp = &flat.boundaries()[0];
    let mut spans: HashMap<EdgeID, (Edge, f64, f64)> = HashMap::new();
    for e in lp.edge_iter() {
        let a = e.absolute_clone();
        let (_, _, a0) = f.polar(p3(a.front().point()));
        let (_, _, a1) = f.polar(p3(a.back().point()));
        let (_, _, am) = f.polar(edge_points(&a)[3]);
        let ccw = (a1 - a0).rem_euclid(TAU);
        let via = (am - a0).rem_euclid(TAU);
        let sweep = if via < ccw { ccw } else { ccw - TAU };
        spans.insert(a.id(), (a, a0, sweep));
    }
    ensure!(spans.len() >= 2, "the circle has too few edges");
    // Top vertices and the meridians up to them, one per rim vertex.
    let mut tops: HashMap<VertexID, (Vertex, Edge)> = HashMap::new();
    for (a, _, _) in spans.values() {
        for v in [a.front(), a.back()] {
            tops.entry(v.id()).or_insert_with(|| {
                let (_, _, ang) = f.polar(p3(v.point()));
                let vt = builder::vertex(pt(f.at(r_top, h_top, ang)));
                let hom = |(rr, hh, w): (f64, f64, f64)| {
                    let q = f.at(rr, hh, ang);
                    [q[0] * w, q[1] * w, q[2] * w, w]
                };
                let piece = Piece {
                    cps: mer.iter().map(|&m| hom(m)).collect(),
                    knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                };
                let m = Edge::new(v, &vt, piece.curve());
                (vt, m)
            });
        }
    }
    let mut top_edges: HashMap<EdgeID, Edge> = HashMap::new();
    for (id, (a, a0, sweep)) in &spans {
        let (v0, v1) = (&tops[&a.front().id()].0, &tops[&a.back().id()].0);
        top_edges.insert(*id, Edge::new(v0, v1, f.arc(r_top, h_top, *a0, *sweep).curve()));
    }
    let sphere_c = f.at(0.0, c, 0.0);
    let mut faces: Vec<Face> = shell
        .face_iter()
        .enumerate()
        .filter(|&(i, _)| i != fi)
        .map(|(_, x)| x.clone())
        .collect();
    let mut top_wire: Vec<Edge> = Vec::new();
    for e in lp.edge_iter() {
        let (_, a0, sweep) = &spans[&e.id()];
        let te = if e.orientation() {
            top_edges[&e.id()].clone()
        } else {
            top_edges[&e.id()].inverse()
        };
        top_wire.push(te.clone());
        let wire: Wire = vec![
            e.clone(),
            tops[&e.back().id()].1.clone(),
            te.inverse(),
            tops[&e.front().id()].1.inverse(),
        ]
        .into();
        let rings: Vec<Piece> = mer.iter().map(|&(rr, hh, _)| f.arc(rr, hh, *a0, *sweep)).collect();
        let rows: Vec<Vec<Vector4>> = (0..rings[0].cps.len())
            .map(|i| {
                (0..3)
                    .map(|j| {
                        let q = rings[j].cps[i];
                        let w = mer[j].2;
                        Vector4::new(q[0] * w, q[1] * w, q[2] * w, q[3] * w)
                    })
                    .collect()
            })
            .collect();
        let surface = Surface::NurbsSurface(NurbsSurface::new(BSplineSurface::new(
            (
                KnotVec::from(rings[0].knots.clone()),
                KnotVec::from(vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
            ),
            rows,
        )));
        faces.push(oriented(vec![wire], surface, |p| sub(p, sphere_c).map(|v| v * sg))?);
    }
    let o = f.at(0.0, h_top, 0.0);
    let top_plane = Surface::Plane(truck_modeling::Plane::new(
        pt(o),
        pt(std::array::from_fn(|i| o[i] + f.x[i])),
        pt(std::array::from_fn(|i| o[i] + f.y[i])),
    ));
    faces.push(oriented(vec![top_wire.into()], top_plane, |_| n.map(|v| v * sg))?);
    let new_shell: Shell = faces.into();
    ensure!(
        new_shell.shell_condition() == ShellCondition::Closed,
        "the domed body does not close"
    );
    let mut shells = solid.boundaries().clone();
    shells[si] = new_shell;
    Solid::try_new(shells).map_err(|e| anyhow!("the domed body is invalid: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::{subtract, volume};
    use crate::mesher::print_mesh;
    use crate::operations::{extrude_circle, extrude_polygon};

    /// Volume of the ring a fillet of `rho` takes from (convex) or adds to (concave) a corner at
    /// radius `r`, by Pappus: the corner area (1 − π/4)ρ² at its centroid's radius.
    fn ring(r: f64, rho: f64, outward: f64) -> f64 {
        let area = (1.0 - PI / 4.0) * rho * rho;
        // Centroid of the corner region from the corner: ρ(10 − 3π)/(12 − 3π).
        let c = rho * (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI);
        area * TAU * (r + outward * c)
    }

    #[test]
    fn a_rod_end_domes_up_and_down() {
        let rod = extrude_circle([0.0, 0.0], 5.0, 10.0).unwrap();
        let (r, h) = (5.0f64, 2.0f64);
        let big = (r * r + h * h) / (2.0 * h);
        let cap = PI * h * h * (3.0 * big - h) / 3.0;
        let base = PI * 250.0;
        for (hh, expected) in [
            (2.0, base + cap),
            (-2.0, base - cap),
            (5.0, base + 2.0 / 3.0 * PI * 125.0),
        ] {
            let s = dome_circle(&rod, [0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 5.0, hh).unwrap();
            let v = crate::mass::mass_properties(&[print_mesh(&s, 0.002)]).volume;
            assert!((v - expected).abs() / expected < 5e-3, "{hh}: {v} vs {expected}");
            assert!(!print_mesh(&s, 0.01).triangles.is_empty());
        }
        assert!(dome_circle(&rod, [0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 5.0, 6.0)
            .unwrap_err()
            .to_string()
            .contains("overhang"));
    }

    #[test]
    fn the_top_of_a_rod_is_rounded() {
        let rod = extrude_circle([0.0, 0.0], 5.0, 10.0).unwrap();
        let out = round_circle(&rod, [0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 5.0, 1.5).unwrap();
        let v = volume(&out).unwrap();
        let expected = volume(&rod).unwrap() - ring(5.0, 1.5, -1.0);
        assert!((v - expected).abs() / expected < 1e-3, "{v} vs {expected}");
        assert!(!print_mesh(&out, 0.01).triangles.is_empty());
        // The bottom works too, and a second fillet stacks on the first.
        let both = round_circle(&out, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 5.0, 1.0).unwrap();
        let v2 = volume(&both).unwrap();
        let e2 = expected - ring(5.0, 1.0, -1.0);
        assert!((v2 - e2).abs() / e2 < 1e-3, "{v2} vs {e2}");
    }

    #[test]
    fn a_hole_rim_is_rounded() {
        let plate = extrude_polygon(&[[-10.0, -10.0], [10.0, -10.0], [10.0, 10.0], [-10.0, 10.0]], 5.0).unwrap();
        let hole = extrude_circle([0.0, 0.0], 3.0, 5.0).unwrap();
        let drilled = subtract(&plate, &hole).unwrap().remove(0);
        let out = round_circle(&drilled, [0.0, 0.0, 5.0], [0.0, 0.0, 1.0], 3.0, 1.0).unwrap();
        let v = volume(&out).unwrap();
        let expected = volume(&drilled).unwrap() - ring(3.0, 1.0, 1.0);
        assert!((v - expected).abs() / expected < 1e-3, "{v} vs {expected}");
    }

    #[test]
    fn the_root_of_a_boss_gets_material() {
        let plate = extrude_polygon(&[[-10.0, -10.0], [10.0, -10.0], [10.0, 10.0], [-10.0, 10.0]], 2.0).unwrap();
        let boss = crate::boolean::transformed(
            &extrude_circle([0.0, 0.0], 3.0, 5.0).unwrap(),
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 2.0, 1.0],
            ],
        );
        let body = crate::boolean::union(&plate, &boss).unwrap().unwrap();
        let out = round_circle(&body, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0], 3.0, 1.0).unwrap();
        let v = volume(&out).unwrap();
        let expected = volume(&body).unwrap() + ring(3.0, 1.0, 1.0);
        assert!((v - expected).abs() / expected < 1e-3, "{v} vs {expected}");
    }

    #[test]
    fn too_large_a_fillet_is_refused() {
        let rod = extrude_circle([0.0, 0.0], 5.0, 2.0).unwrap();
        let err = round_circle(&rod, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0], 5.0, 3.0).unwrap_err();
        assert!(err.to_string().contains("too large"), "{err}");
    }
}
