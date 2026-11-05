//! Moving flat and cylindrical faces into a part (Fit clearances), built directly into the B-Rep:
//! a boolean band along a face would run its walls along the part's own faces, which truck cannot
//! intersect (and coaxial cylinders it gets wrong).

use std::collections::HashMap;

use anyhow::{anyhow, bail, ensure, Result};
use truck_modeling::*;

use crate::topology::{cross, dot, norm, sub, unit, Topology, P3};

/// How a face moves: flat faces slide along their normal, cylinders change radius.
#[derive(Clone, Copy)]
enum Kind {
    Plane {
        n: P3,
    },
    /// Axis point and direction, and +1 when the outward normal points away from the axis (a pin),
    /// −1 when it points at it (a hole).
    Cylinder {
        o: P3,
        a: P3,
        side: f64,
    },
}

impl Kind {
    fn of(topo: &Topology, f: usize) -> Option<Kind> {
        let face = &topo.faces[f];
        if let Some(p) = face.plane {
            return Some(Kind::Plane { n: p.normal() });
        }
        let (o, a, _) = face.cylinder?;
        let a = unit(a);
        // Which way the face looks: its biggest triangle's winding against the radius there.
        let t = face.tris.iter().max_by(|s, t| tri_area(s).total_cmp(&tri_area(t)))?;
        let c = std::array::from_fn(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.0);
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        Some(Kind::Cylinder {
            o,
            a,
            side: dot(n, radial(o, a, c)).signum(),
        })
    }

    /// Outward unit normal at `p`.
    fn normal(&self, p: P3) -> P3 {
        match *self {
            Kind::Plane { n } => n,
            Kind::Cylinder { o, a, side } => unit(radial(o, a, p)).map(|v| v * side),
        }
    }

    /// The map that moves the face `d` against its outward normal.
    fn map(&self, d: f64, r: f64) -> Result<Matrix4> {
        Ok(match *self {
            Kind::Plane { n } => Matrix4::from_translation(Vector3::new(-d * n[0], -d * n[1], -d * n[2])),
            Kind::Cylinder { o, a, side } => {
                let k = (r - side * d) / r;
                ensure!(
                    k > 1e-6,
                    "a {d:.2} mm clearance is more than the {r:.2} mm radius of a selected cylinder"
                );
                // p' = o + a a·(p − o) + k (p − o − a a·(p − o)).
                let lin: [[f64; 3]; 3] = std::array::from_fn(|i| {
                    std::array::from_fn(|j| a[i] * a[j] * (1.0 - k) + if i == j { k } else { 0.0 })
                });
                let t: P3 = std::array::from_fn(|i| o[i] - (0..3).map(|j| lin[i][j] * o[j]).sum::<f64>());
                Matrix4::from_cols(
                    Vector4::new(lin[0][0], lin[1][0], lin[2][0], 0.0),
                    Vector4::new(lin[0][1], lin[1][1], lin[2][1], 0.0),
                    Vector4::new(lin[0][2], lin[1][2], lin[2][2], 0.0),
                    Vector4::new(t[0], t[1], t[2], 1.0),
                )
            }
        })
    }
}

fn tri_area(t: &[P3; 3]) -> f64 {
    norm(cross(sub(t[1], t[0]), sub(t[2], t[0])))
}

/// The part of `p − o` square to the axis `a`.
fn radial(o: P3, a: P3, p: P3) -> P3 {
    let d = sub(p, o);
    let h = dot(d, a);
    std::array::from_fn(|i| d[i] - a[i] * h)
}

fn p3(p: Point3) -> P3 {
    [p.x, p.y, p.z]
}

fn samples(c: &Curve) -> Vec<P3> {
    let (t0, t1) = c.range_tuple();
    (0..=8).map(|k| p3(c.subs(t0 + (t1 - t0) * k as f64 / 8.0))).collect()
}

fn straight(c: &Curve) -> bool {
    let s = samples(c);
    let (a, b) = (s[0], s[8]);
    let len = norm(sub(b, a));
    len > 1e-9 && s.iter().all(|&p| norm(cross(sub(p, a), sub(b, a))) / len < 1e-7)
}

/// The smallest move `v` with `n·v = off` for every constraint, or `None` when they disagree.
fn solve(cons: &[(P3, f64)]) -> Option<P3> {
    // Keep independent normals (at most three); a repeated one must ask for the same offset.
    let mut kept: Vec<(P3, f64)> = Vec::new();
    for &(n, off) in cons {
        let mut g = vec![n];
        g.extend(kept.iter().map(|k| k.0));
        if kept.len() < 3 && gram_det(&g) > 1e-10 {
            kept.push((n, off));
        }
    }
    // v = Σ λ_k n_k with the Gram system G λ = off.
    let m = kept.len();
    let mut a = vec![vec![0.0; m + 1]; m];
    for i in 0..m {
        for j in 0..m {
            a[i][j] = dot(kept[i].0, kept[j].0);
        }
        a[i][m] = kept[i].1;
    }
    for c in 0..m {
        let piv = (c..m).max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))?;
        a.swap(c, piv);
        if a[c][c].abs() < 1e-12 {
            return None;
        }
        let pivot = a[c].clone();
        for (r, row) in a.iter_mut().enumerate() {
            if r != c {
                let f = row[c] / pivot[c];
                row.iter_mut().zip(&pivot).skip(c).for_each(|(x, p)| *x -= f * p);
            }
        }
    }
    let v = (0..m).fold([0.0; 3], |v, i| {
        let l = a[i][m] / a[i][i];
        std::array::from_fn(|k| v[k] + l * kept[i].0[k])
    });
    cons.iter().all(|&(n, off)| (dot(n, v) - off).abs() < 1e-7).then_some(v)
}

fn gram_det(v: &[P3]) -> f64 {
    let g = |i: usize, j: usize| dot(v[i], v[j]);
    match v.len() {
        1 => g(0, 0),
        2 => g(0, 0) * g(1, 1) - g(0, 1) * g(1, 0),
        _ => {
            let c = cross(v[0], v[1]);
            dot(c, v[2]).powi(2)
        }
    }
}

/// Moves faces `faces` (indices into `topo.faces`, which lists `solid`'s faces in order) `d` into
/// the part: flat faces slide along their normal, cylinders shrink (pins) or grow (holes). The
/// faces they meet stay where they are and must be flat or cylindrical; the edges and corners
/// between them are found again.
// shortcut: a moved cylinder meeting an unmoved flat face at a slant (a D-shaped hole's flat) is
// refused; solve those corners on the curve if it comes up.
pub fn move_faces(solid: &Solid, topo: &Topology, faces: &[usize], d: f64) -> Result<Solid> {
    ensure!(
        solid.boundaries().len() == 1,
        "bodies with an internal void cannot be fitted yet"
    );
    let shell = &solid.boundaries()[0];
    let all: Vec<&Face> = shell.face_iter().collect();
    ensure!(
        all.len() == topo.faces.len(),
        "the body's faces do not match its topology"
    );
    let kinds: Vec<Option<Kind>> = (0..all.len()).map(|f| Kind::of(topo, f)).collect();
    let mut maps: HashMap<usize, Matrix4> = HashMap::new();
    for &f in faces {
        let kind = kinds
            .get(f)
            .copied()
            .flatten()
            .ok_or_else(|| anyhow!("Fit works on flat and cylindrical faces"))?;
        let r = topo.faces[f].cylinder.map_or(1.0, |c| c.2);
        maps.insert(f, kind.map(d, r)?);
    }

    let mut faces_at: HashMap<VertexID, Vec<usize>> = HashMap::new();
    let mut faces_of: HashMap<EdgeID, Vec<usize>> = HashMap::new();
    for (i, f) in all.iter().enumerate() {
        for e in f.edge_iter() {
            faces_of.entry(e.id()).or_default().push(i);
            for v in [e.front(), e.back()] {
                let list = faces_at.entry(v.id()).or_default();
                if !list.contains(&i) {
                    list.push(i);
                }
            }
        }
    }

    // Corners: the smallest move that keeps each corner on all its faces, moved or not.
    let mut new_v: HashMap<VertexID, Vertex> = HashMap::new();
    for v in shell.vertex_iter() {
        let around = &faces_at[&v.id()];
        if new_v.contains_key(&v.id()) || !around.iter().any(|f| maps.contains_key(f)) {
            continue;
        }
        let p = p3(v.point());
        let mut cons = Vec::new();
        for &f in around {
            let kind =
                kinds[f].ok_or_else(|| anyhow!("Fit can only move faces whose neighbours are flat or cylindrical"))?;
            cons.push((kind.normal(p), if maps.contains_key(&f) { -d } else { 0.0 }));
        }
        let m = solve(&cons).ok_or_else(|| anyhow!("these faces cannot move together: a corner would leave a face"))?;
        new_v.insert(
            v.id(),
            builder::vertex(Point3::new(p[0] + m[0], p[1] + m[1], p[2] + m[2])),
        );
    }

    // Edges: straight ones join their new corners, curved ones move with their moved faces.
    let mut new_e: HashMap<EdgeID, Edge> = HashMap::new();
    for e in shell.edge_iter() {
        let around = &faces_of[&e.id()];
        let moved: Vec<&Matrix4> = around.iter().filter_map(|f| maps.get(f)).collect();
        let a = e.absolute_clone();
        let (front, back) = (a.front(), a.back());
        if new_e.contains_key(&e.id())
            || (moved.is_empty() && !new_v.contains_key(&front.id()) && !new_v.contains_key(&back.id()))
        {
            continue;
        }
        let v0 = new_v.get(&front.id()).unwrap_or(front).clone();
        let v1 = new_v.get(&back.id()).unwrap_or(back).clone();
        let curve = a.curve();
        let curve = if straight(&curve) {
            Curve::Line(Line(v0.point(), v1.point()))
        } else {
            ensure!(
                !moved.is_empty(),
                "these faces cannot move together: a curved edge would have to bend"
            );
            let c = moved.iter().fold(curve, |c, m| c.transformed(**m));
            let s = samples(&c);
            ensure!(
                norm(sub(s[0], p3(v0.point()))) < 1e-6 && norm(sub(s[8], p3(v1.point()))) < 1e-6,
                "these faces cannot move together: an edge would come apart"
            );
            c
        };
        new_e.insert(e.id(), Edge::new(&v0, &v1, curve));
    }

    let swap = |e: &Edge| match new_e.get(&e.id()) {
        Some(n) if e.orientation() => n.clone(),
        Some(n) => n.inverse(),
        None => e.clone(),
    };
    let mut out: Vec<Face> = Vec::new();
    for (i, f) in all.iter().enumerate() {
        let map = maps.get(&i);
        if map.is_none() && !f.edge_iter().any(|e| new_e.contains_key(&e.id())) {
            out.push((*f).clone());
            continue;
        }
        let wires: Vec<Wire> = f
            .boundaries()
            .iter()
            .map(|w| w.edge_iter().map(&swap).collect())
            .collect();
        let surface = match map {
            Some(m) => f.surface().transformed(*m),
            None => f.surface(),
        };
        out.push(crate::topology::face_on(f, wires, surface).map_err(|e| anyhow!("moved face: {e}"))?);
    }
    let new_shell: Shell = out.into();
    if new_shell.shell_condition() != ShellCondition::Closed {
        bail!("the fitted body does not close");
    }
    Solid::try_new(vec![new_shell]).map_err(|e| anyhow!("the fitted body is invalid: {e:?}"))
}
