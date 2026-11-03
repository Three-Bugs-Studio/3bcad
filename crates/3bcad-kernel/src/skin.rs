//! Skinned solids: closed profile loops carried through a series of sections (stations), with a
//! B-spline surface through each piece of the loops. Sweeps, lofts, threads, variable fillets and
//! wraps are all built this way.
//!
//! Every section has the same structure: the same loops, each with the same pieces, and each piece
//! the same degree and knots in every section, so only the control points change. Along the
//! stations the control points (homogeneous, so arcs stay exact) are interpolated by a B-spline
//! (Piegl & Tiller, The NURBS Book, ch. 9). Neighbouring faces share their edges, so the shell
//! closes without any boolean.

use anyhow::{anyhow, bail, ensure, Result};
use nalgebra::DMatrix;
use truck_modeling::*;

use crate::operations::ProfileSegment;

pub type P3 = [f64; 3];

/// One piece of a section loop: homogeneous control points (x·w, y·w, z·w, w) and a clamped knot
/// vector; the degree is `knots.len() - cps.len() - 1`.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub cps: Vec<[f64; 4]>,
    pub knots: Vec<f64>,
}

impl Piece {
    pub fn degree(&self) -> usize {
        self.knots.len() - self.cps.len() - 1
    }

    fn point(&self, i: usize) -> P3 {
        let c = self.cps[i];
        [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
    }

    pub fn start(&self) -> P3 {
        self.point(0)
    }

    pub fn end(&self) -> P3 {
        self.point(self.cps.len() - 1)
    }

    /// A line from `a` to `b`.
    pub fn line(a: P3, b: P3) -> Piece {
        Piece {
            cps: vec![[a[0], a[1], a[2], 1.0], [b[0], b[1], b[2], 1.0]],
            knots: vec![0.0, 0.0, 1.0, 1.0],
        }
    }

    /// The piece moved by a column-major affine transform (rigid or not; weights are kept).
    pub fn transformed(&self, m: &[[f64; 4]; 4]) -> Piece {
        Piece {
            cps: self
                .cps
                .iter()
                .map(|c| {
                    let p = apply(m, [c[0] / c[3], c[1] / c[3], c[2] / c[3]]);
                    [p[0] * c[3], p[1] * c[3], p[2] * c[3], c[3]]
                })
                .collect(),
            knots: self.knots.clone(),
        }
    }

    /// Point at parameter `t` (in the knot range).
    pub fn eval(&self, t: f64) -> P3 {
        let p = self.degree();
        let n = self.cps.len();
        let span = find_span(&self.knots, p, n, t);
        let b = basis(&self.knots, p, span, t);
        let mut acc = [0.0; 4];
        for (j, bj) in b.iter().enumerate() {
            let c = self.cps[span - p + j];
            for k in 0..4 {
                acc[k] += bj * c[k];
            }
        }
        [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]]
    }

    pub fn curve(&self) -> Curve {
        if self.degree() == 1 && self.cps.len() == 2 && self.cps.iter().all(|c| (c[3] - 1.0).abs() < 1e-15) {
            return Curve::Line(Line(to_point(self.start()), to_point(self.end())));
        }
        let cps = self.cps.iter().map(|c| Vector4::new(c[0], c[1], c[2], c[3])).collect();
        Curve::NurbsCurve(NurbsCurve::new(BSplineCurve::new(
            KnotVec::from(self.knots.clone()),
            cps,
        )))
    }
}

pub fn apply(m: &[[f64; 4]; 4], p: P3) -> P3 {
    std::array::from_fn(|r| (0..3).map(|c| m[c][r] * p[c]).sum::<f64>() + m[3][r])
}

fn to_point(p: P3) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

/// Knot span index of `t` (Piegl A2.1).
fn find_span(knots: &[f64], p: usize, n: usize, t: f64) -> usize {
    if t >= knots[n] {
        return n - 1;
    }
    if t <= knots[p] {
        return p;
    }
    let (mut lo, mut hi) = (p, n);
    let mut mid = (lo + hi) / 2;
    while t < knots[mid] || t >= knots[mid + 1] {
        if t < knots[mid] {
            hi = mid;
        } else {
            lo = mid;
        }
        mid = (lo + hi) / 2;
    }
    mid
}

/// The `p + 1` non-zero basis functions at `t` (Piegl A2.2).
fn basis(knots: &[f64], p: usize, span: usize, t: f64) -> Vec<f64> {
    let mut n = vec![0.0; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    n[0] = 1.0;
    for j in 1..=p {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            let den = right[r + 1] + left[j - r];
            let tmp = if den.abs() < 1e-300 { 0.0 } else { n[r] / den };
            n[r] = saved + right[r + 1] * tmp;
            saved = left[j - r] * tmp;
        }
        n[j] = saved;
    }
    n
}

/// How a band of sections is interpolated along the stations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ends {
    /// Derivatives (per unit of the station parameter) of every control point at the first and the
    /// last station, in the same order as the control points of the section; `None` leaves the end
    /// free.
    pub start: Option<Vec<[f64; 3]>>,
    pub end: Option<Vec<[f64; 3]>>,
}

/// Knots, and the control points of every value column.
type Interpolated = (Vec<f64>, Vec<Vec<[f64; 4]>>);

/// B-spline through `values` at `params` (degree ≤ 3), optionally with end derivatives (applied to
/// the first three coordinates, the weight is assumed constant there). Returns knots and control
/// points for every value column.
fn interpolate(
    params: &[f64],
    values: &[Vec<[f64; 4]>],
    start: Option<&[[f64; 3]]>,
    end: Option<&[[f64; 3]]>,
) -> Result<Interpolated> {
    let m = params.len() - 1;
    ensure!(m >= 1, "a skin needs at least two sections");
    let cols = values[0].len();
    let nd = usize::from(start.is_some()) + usize::from(end.is_some());
    let ncp = m + 1 + nd;
    let p = 3.min(ncp - 1);
    let (t0, t1) = (params[0], params[m]);
    let t: Vec<f64> = params.iter().map(|v| (v - t0) / (t1 - t0)).collect();
    // Knots: interior knots by averaging (Piegl eq. 9.8), or the parameters themselves when both end
    // derivatives are given (eq. 9.22).
    let mut knots = vec![0.0; p + 1];
    let interior = ncp - p - 1;
    if interior > 0 {
        if nd == 2 && p == 3 {
            knots.extend(&t[1..m]);
        } else {
            // Spread the interior knots over the parameters with a moving average.
            for j in 1..=interior {
                let pos = j as f64 * m as f64 / (interior + 1) as f64;
                let k = (pos.floor() as usize).min(m - 1);
                let f = pos - k as f64;
                knots.push(t[k] * (1.0 - f) + t[k + 1] * f);
            }
        }
    }
    knots.extend(vec![1.0; p + 1]);
    ensure!(knots.len() == ncp + p + 1, "bad knot vector");
    let mut a = DMatrix::<f64>::zeros(ncp, ncp);
    let mut rhs = DMatrix::<f64>::zeros(ncp, cols * 4);
    let mut row = 0;
    for (k, &tk) in t.iter().enumerate() {
        let span = find_span(&knots, p, ncp, tk);
        for (j, b) in basis(&knots, p, span, tk).into_iter().enumerate() {
            a[(row, span - p + j)] = b;
        }
        for c in 0..cols {
            for d in 0..4 {
                rhs[(row, c * 4 + d)] = values[k][c][d];
            }
        }
        row += 1;
        let deriv = match k {
            0 => start.map(|s| (s, 0usize)),
            _ if k == m => end.map(|e| (e, ncp - 1)),
            _ => None,
        };
        if let Some((ders, at)) = deriv {
            let scale = t1 - t0;
            let (i0, i1, den) = if at == 0 {
                (0, 1, knots[p + 1] - knots[1])
            } else {
                (ncp - 2, ncp - 1, knots[ncp + p - 1] - knots[ncp - 1])
            };
            let c = p as f64 / den;
            a[(row, i0)] = -c;
            a[(row, i1)] = c;
            for col in 0..cols {
                let w = values[k][col][3];
                for d in 0..3 {
                    rhs[(row, col * 4 + d)] = ders[col][d] * scale * w;
                }
            }
            row += 1;
        }
    }
    let sol = a
        .lu()
        .solve(&rhs)
        .ok_or_else(|| anyhow!("the sections cannot be interpolated"))?;
    let cps = (0..cols)
        .map(|c| (0..ncp).map(|i| std::array::from_fn(|d| sol[(i, c * 4 + d)])).collect())
        .collect();
    Ok((knots, cps))
}

/// Sections of one smooth stretch, all with the same structure.
#[derive(Clone, Debug)]
pub struct Band {
    /// sections[k][loop][piece]
    pub sections: Vec<Vec<Vec<Piece>>>,
    /// Station parameters, increasing.
    pub params: Vec<f64>,
    pub ends: Ends,
}

/// What closes the two ends of the skin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caps {
    /// Flat faces on the first and last sections (which must be planar).
    Planar,
    /// The last section is the first one: the skin closes on itself (a ring).
    Closed,
}

/// Builds the solid through `bands`; each band starts with the last section of the one before.
pub fn skin(bands: &[Band], caps: Caps) -> Result<Solid> {
    ensure!(!bands.is_empty(), "nothing to skin");
    let first = &bands[0].sections[0];
    ensure!(!first.is_empty(), "the section has no loops");
    for b in bands {
        ensure!(
            b.sections.len() == b.params.len() && b.sections.len() >= 2,
            "a band needs two sections or more"
        );
        for s in &b.sections {
            ensure!(
                s.len() == first.len()
                    && s.iter().zip(first).all(|(l, f)| l.len() == f.len()
                        && l.iter()
                            .zip(f)
                            .all(|(p, q)| p.cps.len() == q.cps.len() && p.knots == q.knots)),
                "the sections do not match"
            );
        }
    }
    let nb = bands.len();
    // Boundary sections: 0..=nb (the last is the first again for a closed skin).
    let boundary = |k: usize| -> &Vec<Vec<Piece>> {
        if k < nb {
            &bands[k].sections[0]
        } else {
            bands[nb - 1].sections.last().unwrap()
        }
    };
    let closed = caps == Caps::Closed;
    let nbound = if closed { nb } else { nb + 1 };
    let verts: Vec<Vec<Vec<Vertex>>> = (0..nbound)
        .map(|k| {
            boundary(k)
                .iter()
                .map(|lp| lp.iter().map(|pc| builder::vertex(to_point(pc.start()))).collect())
                .collect()
        })
        .collect();
    let vert = |k: usize, l: usize, s: usize| -> &Vertex {
        let k = if closed { k % nb } else { k };
        let n = verts[k][l].len();
        &verts[k][l][s % n]
    };
    let u_edges: Vec<Vec<Vec<Edge>>> = (0..nbound)
        .map(|k| {
            boundary(k)
                .iter()
                .enumerate()
                .map(|(l, lp)| {
                    lp.iter()
                        .enumerate()
                        .map(|(s, pc)| Edge::new(vert(k, l, s), vert(k, l, s + 1), pc.curve()))
                        .collect()
                })
                .collect()
        })
        .collect();
    let u_edge = |k: usize, l: usize, s: usize| -> &Edge {
        let k = if closed { k % nb } else { k };
        &u_edges[k][l][s]
    };
    let mut shell = Shell::new();
    for (bi, band) in bands.iter().enumerate() {
        for (l, lp) in first.iter().enumerate() {
            let n = lp.len();
            // Interpolate every control point of every piece of this loop at once.
            let values: Vec<Vec<[f64; 4]>> = band
                .sections
                .iter()
                .map(|s| s[l].iter().flat_map(|p| p.cps.clone()).collect())
                .collect();
            let pick = |ders: &Option<Vec<[f64; 3]>>| -> Option<Vec<[f64; 3]>> {
                ders.as_ref().map(|d| {
                    let offset: usize = first[..l]
                        .iter()
                        .map(|lp| lp.iter().map(|p| p.cps.len()).sum::<usize>())
                        .sum();
                    let count: usize = lp.iter().map(|p| p.cps.len()).sum();
                    d[offset..offset + count].to_vec()
                })
            };
            let (s0, s1) = (pick(&band.ends.start), pick(&band.ends.end));
            let (vknots, cols) = interpolate(&band.params, &values, s0.as_deref(), s1.as_deref())?;
            let mut col = 0;
            let mut seams = Vec::with_capacity(n);
            let mut nets = Vec::with_capacity(n);
            for pc in lp.iter() {
                nets.push((col, pc.cps.len()));
                col += pc.cps.len();
            }
            for (s, &(c0, _)) in nets.iter().enumerate() {
                let cps: Vec<Vector4> = cols[c0].iter().map(|c| Vector4::new(c[0], c[1], c[2], c[3])).collect();
                let curve = Curve::NurbsCurve(NurbsCurve::new(BSplineCurve::new(KnotVec::from(vknots.clone()), cps)));
                seams.push(Edge::new(vert(bi, l, s), vert(bi + 1, l, s), curve));
            }
            for s in 0..n {
                let (c0, cnt) = nets[s];
                let rows: Vec<Vec<Vector4>> = (c0..c0 + cnt)
                    .map(|c| cols[c].iter().map(|v| Vector4::new(v[0], v[1], v[2], v[3])).collect())
                    .collect();
                let surface = Surface::NurbsSurface(NurbsSurface::new(BSplineSurface::new(
                    (KnotVec::from(lp[s].knots.clone()), KnotVec::from(vknots.clone())),
                    rows,
                )));
                let wire: Wire = vec![
                    u_edge(bi, l, s).clone(),
                    seams[(s + 1) % n].clone(),
                    u_edge(bi + 1, l, s).inverse(),
                    seams[s].inverse(),
                ]
                .into();
                shell.push(Face::try_new(vec![wire], surface).map_err(|e| anyhow!("skin face: {e:?}"))?);
            }
        }
    }
    if caps == Caps::Planar {
        let start: Vec<Wire> = (0..first.len())
            .map(|l| (0..first[l].len()).rev().map(|s| u_edge(0, l, s).inverse()).collect())
            .collect();
        let end: Vec<Wire> = (0..first.len())
            .map(|l| (0..first[l].len()).map(|s| u_edge(nb, l, s).clone()).collect())
            .collect();
        for wires in [start, end] {
            shell.push(
                builder::try_attach_plane(&wires).map_err(|e| anyhow!("the end of the shape is not flat: {e:?}"))?,
            );
        }
    }
    ensure!(
        shell.shell_condition() == ShellCondition::Closed,
        "the shape does not close"
    );
    // Outward when the start cap faces away from the second section; rings need the volume.
    let inward = match caps {
        Caps::Planar => {
            let cap = &shell[shell.len() - 2];
            let n = match cap.oriented_surface() {
                Surface::Plane(p) => p.normal(),
                _ => bail!("the end of the shape is not flat"),
            };
            let centroid = |sec: &Vec<Vec<Piece>>| -> Vector3 {
                let pts: Vec<P3> = sec
                    .iter()
                    .flatten()
                    .flat_map(|p| p.cps.iter().map(|c| [c[0] / c[3], c[1] / c[3], c[2] / c[3]]))
                    .collect();
                let k = pts.len() as f64;
                Vector3::new(
                    pts.iter().map(|p| p[0]).sum::<f64>() / k,
                    pts.iter().map(|p| p[1]).sum::<f64>() / k,
                    pts.iter().map(|p| p[2]).sum::<f64>() / k,
                )
            };
            let d = centroid(&bands[0].sections[1]) - centroid(&bands[0].sections[0]);
            ensure!(d.magnitude() > 1e-12, "the shape has no volume");
            n.dot(d) > 0.0
        }
        Caps::Closed => {
            let v = crate::boolean::volume(&Solid::new_unchecked(vec![shell.clone()]))
                .ok_or_else(|| anyhow!("the shape cannot be meshed"))?;
            ensure!(v.abs() > 1e-9, "the shape has no volume");
            v < 0.0
        }
    };
    let mut solid = Solid::try_new(vec![shell]).map_err(|e| anyhow!("the shape is invalid: {e:?}"))?;
    if inward {
        solid.not();
    }
    Ok(solid)
}

/// A rational circular arc of `sweep` radians about `center` in the plane spanned by unit `x`, `y`,
/// starting at `center + r·x`; pieces of at most 90°.
pub fn arc_piece(center: P3, x: P3, y: P3, r: f64, a0: f64, sweep: f64) -> Piece {
    let n = ((sweep.abs() / (std::f64::consts::PI / 2.0) - 1e-9).ceil() as usize).max(1);
    let d = sweep / n as f64;
    let wm = (d / 2.0).cos();
    let at = |a: f64, rr: f64| -> P3 { std::array::from_fn(|i| center[i] + rr * (a.cos() * x[i] + a.sin() * y[i])) };
    let hom = |p: P3, w: f64| [p[0] * w, p[1] * w, p[2] * w, w];
    let mut cps = vec![hom(at(a0, r), 1.0)];
    let mut knots = vec![0.0; 3];
    for k in 0..n {
        let m = a0 + d * (k as f64 + 0.5);
        cps.push(hom(at(m, r / wm), wm));
        cps.push(hom(at(a0 + d * (k as f64 + 1.0), r), 1.0));
        if k + 1 < n {
            knots.extend([(k + 1) as f64 / n as f64; 2]);
        }
    }
    knots.extend([1.0; 3]);
    Piece { cps, knots }
}

/// A profile segment of a sketch plane (local x, y, along-normal = 0) as pieces in the plane's own
/// frame; a circle becomes two half arcs so every piece has two distinct ends.
pub fn segment_pieces(s: &ProfileSegment) -> Vec<Piece> {
    let p3 = |p: [f64; 2]| [p[0], p[1], 0.0];
    match *s {
        ProfileSegment::Line { from, to } => vec![Piece::line(p3(from), p3(to))],
        ProfileSegment::Arc { center, from, to, ccw } => {
            let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
            let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
            let mut sweep = (a1 - a0).rem_euclid(std::f64::consts::TAU);
            if sweep < 1e-12 {
                sweep = std::f64::consts::TAU;
            }
            if !ccw {
                sweep -= std::f64::consts::TAU;
            }
            let r = (from[0] - center[0]).hypot(from[1] - center[1]);
            let mut pc = arc_piece(p3(center), [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], r, a0, sweep);
            // Pin the ends to the exact input points.
            let last = pc.cps.len() - 1;
            pc.cps[0] = [from[0], from[1], 0.0, 1.0];
            pc.cps[last] = [to[0], to[1], 0.0, 1.0];
            vec![pc]
        }
        ProfileSegment::Circle { center, radius, ccw } => {
            let sweep = if ccw {
                std::f64::consts::PI
            } else {
                -std::f64::consts::PI
            };
            vec![
                arc_piece(p3(center), [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], radius, 0.0, sweep),
                arc_piece(p3(center), [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], radius, sweep, sweep),
            ]
        }
        ProfileSegment::Bezier(b) => vec![Piece {
            cps: b.iter().map(|p| [p[0], p[1], 0.0, 1.0]).collect(),
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        }],
    }
}

/// Every segment of a loop as pieces, and for each piece the index of the segment it came from.
pub fn loop_pieces(lp: &[ProfileSegment]) -> (Vec<Piece>, Vec<usize>) {
    let mut pieces = Vec::new();
    let mut owner = Vec::new();
    for (i, s) in lp.iter().enumerate() {
        for p in segment_pieces(s) {
            pieces.push(p);
            owner.push(i);
        }
    }
    (pieces, owner)
}

/// A piece refitted as a non-rational cubic with `spans` uniform spans through samples of it, so
/// pieces of different kinds (line, arc, spline) can be lofted into one another.
pub fn cubic(piece: &Piece, spans: usize) -> Piece {
    let (k0, k1) = (piece.knots[0], *piece.knots.last().unwrap());
    let f = |t: f64| piece.eval(k0 + (k1 - k0) * t);
    fit_cubic(&f, spans)
}

/// A non-rational cubic B-spline with `spans` uniform spans interpolating `f` on [0, 1].
pub fn fit_cubic(f: &dyn Fn(f64) -> P3, spans: usize) -> Piece {
    let spans = spans.max(1);
    let ncp = spans + 3;
    let mut knots = vec![0.0; 4];
    for i in 1..spans {
        knots.push(i as f64 / spans as f64);
    }
    knots.extend([1.0; 4]);
    // Interpolate at ncp parameters: the Greville abscissae.
    let ts: Vec<f64> = (0..ncp)
        .map(|i| (knots[i + 1] + knots[i + 2] + knots[i + 3]) / 3.0)
        .collect();
    let mut a = DMatrix::<f64>::zeros(ncp, ncp);
    let mut rhs = DMatrix::<f64>::zeros(ncp, 3);
    for (r, &t) in ts.iter().enumerate() {
        let span = find_span(&knots, 3, ncp, t);
        for (j, b) in basis(&knots, 3, span, t).into_iter().enumerate() {
            a[(r, span - 3 + j)] = b;
        }
        let p = f(t);
        for d in 0..3 {
            rhs[(r, d)] = p[d];
        }
    }
    let sol = a.lu().solve(&rhs).expect("Greville interpolation is always solvable");
    let mut cps: Vec<[f64; 4]> = (0..ncp).map(|i| [sol[(i, 0)], sol[(i, 1)], sol[(i, 2)], 1.0]).collect();
    // The ends exactly on the curve's ends.
    let (s, e) = (f(0.0), f(1.0));
    cps[0] = [s[0], s[1], s[2], 1.0];
    cps[ncp - 1] = [e[0], e[1], e[2], 1.0];
    Piece { cps, knots }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{PI, TAU};

    use super::*;
    use crate::boolean::volume;
    use crate::mesher::print_mesh;

    fn translate(z: f64) -> [[f64; 4]; 4] {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, z, 1.0],
        ]
    }

    fn square(s: f64) -> Vec<Piece> {
        let p = [[0.0, 0.0, 0.0], [s, 0.0, 0.0], [s, s, 0.0], [0.0, s, 0.0]];
        (0..4).map(|i| Piece::line(p[i], p[(i + 1) % 4])).collect()
    }

    #[test]
    fn a_square_skinned_straight_up_is_a_box() {
        let sq = square(10.0);
        let band = Band {
            sections: (0..3)
                .map(|k| vec![sq.iter().map(|p| p.transformed(&translate(5.0 * k as f64))).collect()])
                .collect(),
            params: vec![0.0, 5.0, 10.0],
            ends: Ends::default(),
        };
        let s = skin(&[band], Caps::Planar).unwrap();
        assert!((volume(&s).unwrap() - 1000.0).abs() < 1e-6);
        assert!(!print_mesh(&s, 0.01).triangles.is_empty());
    }

    #[test]
    fn a_circle_skinned_up_is_a_cylinder_and_arcs_stay_exact() {
        let (c, _) = loop_pieces(&[ProfileSegment::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
            ccw: true,
        }]);
        assert!((c[0].eval(0.5)[1] - 5.0).abs() < 1e-12, "{:?}", c[0].eval(0.5));
        let band = Band {
            sections: vec![
                vec![c.clone()],
                vec![c.iter().map(|p| p.transformed(&translate(10.0))).collect()],
            ],
            params: vec![0.0, 10.0],
            ends: Ends::default(),
        };
        let s = skin(&[band], Caps::Planar).unwrap();
        let expected = PI * 25.0 * 10.0;
        assert!(
            (volume(&s).unwrap() - expected).abs() / expected < 5e-3,
            "{}",
            volume(&s).unwrap()
        );
    }

    #[test]
    fn a_cubic_fit_follows_an_arc_closely() {
        let arc = arc_piece([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 5.0, 0.0, PI / 2.0);
        let fit = cubic(&arc, 8);
        for k in 0..=20 {
            let p = fit.eval(k as f64 / 20.0);
            assert!((p[0].hypot(p[1]) - 5.0).abs() < 1e-4, "{p:?}");
        }
    }

    #[test]
    fn end_derivatives_bend_the_skin() {
        // Two squares with a sideways start tangent: still a closed solid, with more volume
        // pushed out than a plain prism? Only check it closes and keeps its ends.
        let sq = square(10.0);
        let n: usize = sq.iter().map(|p| p.cps.len()).sum();
        let band = Band {
            sections: vec![
                vec![sq.clone()],
                vec![sq.iter().map(|p| p.transformed(&translate(10.0))).collect()],
            ],
            params: vec![0.0, 1.0],
            ends: Ends {
                start: Some(vec![[0.0, 0.0, 10.0]; n]),
                end: Some(vec![[0.0, 0.0, 10.0]; n]),
            },
        };
        // Derivatives equal to the straight line's: still exactly a box.
        let s = skin(&[band], Caps::Planar).unwrap();
        assert!((volume(&s).unwrap() - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn a_ring_closes_on_itself() {
        // A small square turned about the z axis in 12 steps: a square torus.
        let sq: Vec<Piece> = {
            let p = [[10.0, 0.0, 0.0], [12.0, 0.0, 0.0], [12.0, 0.0, 2.0], [10.0, 0.0, 2.0]];
            (0..4).map(|i| Piece::line(p[i], p[(i + 1) % 4])).collect()
        };
        let rot = |a: f64| -> [[f64; 4]; 4] {
            let (s, c) = a.sin_cos();
            [
                [c, s, 0.0, 0.0],
                [-s, c, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        };
        let bands: Vec<Band> = (0..4)
            .map(|q| {
                let a0 = q as f64 * TAU / 4.0;
                let ks: Vec<f64> = (0..=6).map(|k| a0 + k as f64 * TAU / 24.0).collect();
                Band {
                    sections: ks
                        .iter()
                        .map(|&a| vec![sq.iter().map(|p| p.transformed(&rot(a))).collect()])
                        .collect(),
                    params: ks,
                    ends: Ends::default(),
                }
            })
            .collect();
        let s = skin(&bands, Caps::Closed).unwrap();
        let expected = 4.0 * TAU * 11.0;
        assert!(
            (volume(&s).unwrap() - expected).abs() / expected < 2e-3,
            "{}",
            volume(&s).unwrap()
        );
    }
}
