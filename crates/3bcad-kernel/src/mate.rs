//! Assembly mates, and the solver that moves rigid components until their mates hold.
//!
//! Every free component has six unknowns (a translation and a rotation vector) and every mate a
//! few residuals; Levenberg–Marquardt drives them to zero like the sketch solver does in 2D.

use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};

use crate::print::{rotate, Rot, IDENTITY};
use crate::topology::{cross, dot, norm, sub, P3};

/// How far off (mm) a mate may be and still hold.
pub const TOL: f64 = 1e-6;
const MAX_ITERATIONS: usize = 60;
const DRAG_ITERATIONS: usize = 6;
/// A drag pulls this weakly next to the mates, so the mates always win.
const DRAG_WEIGHT: f64 = 1e-3;
/// Finite-difference step for the Jacobian (mm, and radians).
const STEP: f64 = 1e-6;

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P3, s: f64) -> P3 {
    a.map(|v| v * s)
}

fn mul(a: &Rot, b: &Rot) -> Rot {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn transpose(a: &Rot) -> Rot {
    std::array::from_fn(|i| std::array::from_fn(|j| a[j][i]))
}

/// The rotation by `w` radians about the direction of `w` (Rodrigues).
fn exp(w: P3) -> Rot {
    let th = norm(w);
    if th < 1e-15 {
        return IDENTITY;
    }
    let k = scale(w, 1.0 / th);
    let (s, c) = th.sin_cos();
    let kx = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]];
    let kx2 = mul(&kx, &kx);
    std::array::from_fn(|i| std::array::from_fn(|j| f64::from(u8::from(i == j)) + s * kx[i][j] + (1.0 - c) * kx2[i][j]))
}

/// Rows made orthonormal again (Gram–Schmidt), so rounding never skews a part.
fn orthonormal(r: Rot) -> Rot {
    let x = crate::topology::unit(r[0]);
    let y = crate::topology::unit(sub(r[1], scale(x, dot(x, r[1]))));
    [x, y, cross(x, y)]
}

/// Where a component sits: a point `x` of its part is at `r·x + t`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub r: Rot,
    pub t: P3,
}

impl Default for Pose {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Pose {
    pub const IDENTITY: Pose = Pose {
        r: IDENTITY,
        t: [0.0; 3],
    };

    pub fn at(t: P3) -> Self {
        Pose { r: IDENTITY, t }
    }

    pub fn apply(&self, p: P3) -> P3 {
        add(rotate(&self.r, p), self.t)
    }

    /// A direction turned with the component.
    pub fn turn(&self, v: P3) -> P3 {
        rotate(&self.r, v)
    }

    /// This pose after `inner`: a point goes through `inner` first.
    pub fn compose(&self, inner: &Pose) -> Pose {
        Pose {
            r: mul(&self.r, &inner.r),
            t: self.apply(inner.t),
        }
    }

    pub fn inverse(&self) -> Pose {
        let r = transpose(&self.r);
        Pose {
            r,
            t: scale(rotate(&r, self.t), -1.0),
        }
    }

    /// Turned by the rotation vector `w` (radians about its direction) around the point `about`.
    pub fn rotated(&self, w: P3, about: P3) -> Pose {
        let q = exp(w);
        Pose {
            r: orthonormal(mul(&q, &self.r)),
            t: add(rotate(&q, sub(self.t, about)), about),
        }
    }

    pub fn moved(&self, v: P3) -> Pose {
        Pose {
            r: self.r,
            t: add(self.t, v),
        }
    }
}

/// A face a mate holds on to, in its part's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Geom {
    /// A point on a planar face and its outward normal.
    Plane { p: P3, n: P3 },
    /// A point on a cylinder's axis, the axis direction and the radius.
    Cylinder { p: P3, d: P3, r: f64 },
}

impl Geom {
    pub fn moved(&self, pose: &Pose) -> Geom {
        match *self {
            Geom::Plane { p, n } => Geom::Plane {
                p: pose.apply(p),
                n: pose.turn(n),
            },
            Geom::Cylinder { p, d, r } => Geom::Cylinder {
                p: pose.apply(p),
                d: pose.turn(d),
                r,
            },
        }
    }

    fn point(&self) -> P3 {
        match *self {
            Geom::Plane { p, .. } | Geom::Cylinder { p, .. } => p,
        }
    }

    /// The normal of a plane, the axis of a cylinder.
    fn dir(&self) -> P3 {
        match *self {
            Geom::Plane { n, .. } => n,
            Geom::Cylinder { d, .. } => d,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MateKind {
    Coincident,
    Concentric,
    Distance,
    Angle,
    Parallel,
    Perpendicular,
    Tangent,
    Lock,
}

impl MateKind {
    pub const ALL: [MateKind; 8] = [
        MateKind::Coincident,
        MateKind::Concentric,
        MateKind::Distance,
        MateKind::Angle,
        MateKind::Parallel,
        MateKind::Perpendicular,
        MateKind::Tangent,
        MateKind::Lock,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MateKind::Coincident => "Coincident",
            MateKind::Concentric => "Concentric",
            MateKind::Distance => "Distance",
            MateKind::Angle => "Angle",
            MateKind::Parallel => "Parallel",
            MateKind::Perpendicular => "Perpendicular",
            MateKind::Tangent => "Tangent",
            MateKind::Lock => "Lock",
        }
    }

    /// Distance (mm) and Angle (degrees) carry a value.
    pub fn has_value(self) -> bool {
        matches!(self, MateKind::Distance | MateKind::Angle)
    }

    /// Whether this mate can join faces of these shapes.
    pub fn fits(self, a: &Geom, b: &Geom) -> bool {
        let cyl = |g: &Geom| matches!(g, Geom::Cylinder { .. });
        match self {
            MateKind::Concentric => cyl(a) && cyl(b),
            MateKind::Tangent => cyl(a) || cyl(b),
            _ => true,
        }
    }
}

/// A mate between faces of two components, ready to solve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mate {
    pub kind: MateKind,
    /// The components (indices into the poses) and their faces, in part coordinates.
    pub a: usize,
    pub ga: Geom,
    pub b: usize,
    pub gb: Geom,
    /// Distance (mm) or angle (degrees).
    pub value: f64,
    /// The other alignment: faces facing the same way instead of each other, the other side of a
    /// plane, the inside of a cylinder.
    pub flip: bool,
    /// For Lock: where `b` sits relative to `a`.
    pub lock: Pose,
}

impl Mate {
    /// What is off, pushed onto `out`; directions are scaled by `len` so they weigh like lengths.
    pub fn residuals(&self, pa: &Pose, pb: &Pose, len: f64, out: &mut Vec<f64>) {
        use Geom::{Cylinder, Plane};
        use MateKind::*;
        let s = if self.flip { -1.0 } else { 1.0 };
        let push = |out: &mut Vec<f64>, v: P3, k: f64| out.extend(v.map(|x| x * k));
        if self.kind == Lock {
            let want = pa.compose(&self.lock);
            for i in 0..3 {
                push(out, sub(want.r[i], pb.r[i]), len);
            }
            push(out, sub(want.t, pb.t), 1.0);
            return;
        }
        // With a plane and a cylinder, the plane comes first.
        let (ga, gb) = match (self.ga.moved(pa), self.gb.moved(pb)) {
            (c @ Cylinder { .. }, p @ Plane { .. }) => (p, c),
            g => g,
        };
        let (ua, ub) = (ga.dir(), gb.dir());
        let off = sub(gb.point(), ga.point());
        match (self.kind, ga, gb) {
            (Coincident | Distance, Plane { .. }, Plane { .. }) => {
                push(out, add(ua, scale(ub, s)), len);
                out.push(dot(off, ua) - if self.kind == Distance { self.value } else { 0.0 });
            }
            (Coincident | Distance | Tangent, Plane { .. }, Cylinder { r, .. }) => {
                out.push(dot(ua, ub) * len);
                let v = match self.kind {
                    Coincident => 0.0,
                    Distance => self.value,
                    _ => r,
                };
                out.push(dot(off, ua) - s * v);
            }
            (Coincident | Concentric, Cylinder { .. }, Cylinder { .. }) => {
                push(out, sub(ua, scale(ub, s)), len);
                push(out, cross(off, ua), 1.0);
            }
            (Distance | Tangent, Cylinder { r: ra, .. }, Cylinder { r: rb, .. }) => {
                push(out, cross(ua, ub), len);
                let v = match self.kind {
                    Distance => self.value,
                    _ if self.flip => (ra - rb).abs(),
                    _ => ra + rb,
                };
                out.push(norm(cross(off, ua)) - v);
            }
            (Angle, ..) => out.push((dot(ua, ub) - s * self.value.to_radians().cos()) * len),
            (Parallel, Plane { .. }, Cylinder { .. }) | (Perpendicular, Plane { .. }, Plane { .. }) => {
                out.push(dot(ua, ub) * len)
            }
            (Perpendicular, Cylinder { .. }, Cylinder { .. }) => out.push(dot(ua, ub) * len),
            (Parallel | Perpendicular, ..) => push(out, cross(ua, ub), len),
            // Concentric planes, tangent planes: refused when the mate is made.
            _ => {}
        }
    }

    /// The largest error of the mate where the components are now.
    pub fn error(&self, poses: &[Pose], len: f64) -> f64 {
        let mut r = Vec::new();
        self.residuals(&poses[self.a], &poses[self.b], len, &mut r);
        r.iter().fold(0.0, |m, v| m.max(v.abs()))
    }
}

/// The length directions are weighed with: the size of the region the mates work in.
pub fn length_scale(poses: &[Pose], mates: &[Mate]) -> f64 {
    mates
        .iter()
        .flat_map(|m| [m.ga.moved(&poses[m.a]).point(), m.gb.moved(&poses[m.b]).point()])
        .map(norm)
        .fold(10.0, f64::max)
        .min(1e4)
}

/// Pulls a component towards where the mouse wants it while it is dragged.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub component: usize,
    pub target: Pose,
}

/// Moves the free components until every mate holds, pulling a dragged one towards its target as
/// far as its mates let it. Returns the mates that cannot hold together with the ones before them
/// (they are over-defined and left out), in order.
pub fn solve(poses: &mut [Pose], fixed: &[bool], mates: &[Mate], drag: Option<Drag>) -> Vec<usize> {
    let len = length_scale(poses, mates);
    let mut failed = Vec::new();
    for (comps, own) in clusters(fixed, mates, drag.map(|d| d.component)) {
        if let Some(d) = drag.filter(|d| comps.contains(&d.component)) {
            lm(poses, &comps, mates, &own, Some(d), len);
        }
        let before: Vec<Pose> = comps.iter().map(|&c| poses[c]).collect();
        if lm(poses, &comps, mates, &own, None, len) {
            continue;
        }
        // Over-defined: add the mates one at a time, leaving out each one that breaks the others.
        let restore = |poses: &mut [Pose], saved: &[Pose]| {
            for (&c, p) in comps.iter().zip(saved) {
                poses[c] = *p;
            }
        };
        restore(poses, &before);
        let mut active = Vec::new();
        for &m in &own {
            let saved: Vec<Pose> = comps.iter().map(|&c| poses[c]).collect();
            active.push(m);
            if !lm(poses, &comps, mates, &active, None, len) {
                active.pop();
                failed.push(m);
                restore(poses, &saved);
            }
        }
    }
    // Nothing can move to fix a mate between fixed components (or within one).
    for (i, m) in mates.iter().enumerate() {
        let stuck = m.a == m.b || (fixed[m.a] && fixed[m.b]);
        if stuck && m.error(poses, len) > TOL {
            failed.push(i);
        }
    }
    failed.sort_unstable();
    failed
}

/// Free components joined by mates, each group with the mates that touch it. Groups solve on
/// their own; components with no mates are left alone unless dragged.
fn clusters(fixed: &[bool], mates: &[Mate], dragged: Option<usize>) -> Vec<(Vec<usize>, Vec<usize>)> {
    let mut parent: Vec<usize> = (0..fixed.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for m in mates.iter().filter(|m| !fixed[m.a] && !fixed[m.b]) {
        let (a, b) = (root(&mut parent, m.a), root(&mut parent, m.b));
        parent[a] = b;
    }
    let mut groups: Vec<(usize, Vec<usize>, Vec<usize>)> = Vec::new();
    let group_of = |parent: &mut [usize], c: usize, groups: &mut Vec<(usize, Vec<usize>, Vec<usize>)>| {
        let r = root(parent, c);
        match groups.iter().position(|g| g.0 == r) {
            Some(k) => k,
            None => {
                groups.push((r, Vec::new(), Vec::new()));
                groups.len() - 1
            }
        }
    };
    for (i, m) in mates.iter().enumerate() {
        if m.a == m.b {
            continue;
        }
        let Some(free) = [m.a, m.b].into_iter().find(|&c| !fixed[c]) else {
            continue;
        };
        let k = group_of(&mut parent, free, &mut groups);
        groups[k].2.push(i);
    }
    if let Some(d) = dragged.filter(|&d| !fixed[d]) {
        group_of(&mut parent, d, &mut groups);
    }
    for c in (0..fixed.len()).filter(|&c| !fixed[c]) {
        let r = root(&mut parent, c);
        if let Some(g) = groups.iter_mut().find(|g| g.0 == r) {
            g.1.push(c);
        }
    }
    groups.into_iter().map(|(_, comps, own)| (comps, own)).collect()
}

/// Levenberg–Marquardt over the poses of `comps` for the mates `own` (and a drag); true when the
/// mates hold afterwards.
// shortcut: dense normal equations per group of mated parts; a single chain of several hundred
// mated parts gets slow, switch to a sparse Cholesky if assemblies like that come up.
fn lm(poses: &mut [Pose], comps: &[usize], mates: &[Mate], own: &[usize], drag: Option<Drag>, len: f64) -> bool {
    let mut col = vec![usize::MAX; poses.len()];
    for (k, &c) in comps.iter().enumerate() {
        col[c] = k * 6;
    }
    let n = comps.len() * 6;
    // Each term: the free components it moves, and its residuals for given poses.
    let term = |poses: &[Pose], t: usize, out: &mut Vec<f64>| match own.get(t) {
        Some(&m) => mates[m].residuals(&poses[mates[m].a], &poses[mates[m].b], len, out),
        None => {
            let d = drag.expect("the last term is the drag");
            let p = &poses[d.component];
            for i in 0..3 {
                out.extend(sub(p.r[i], d.target.r[i]).map(|v| v * len * DRAG_WEIGHT));
            }
            out.extend(sub(p.t, d.target.t).map(|v| v * DRAG_WEIGHT));
        }
    };
    let ends = |t: usize| -> Vec<usize> {
        match own.get(t) {
            Some(&m) => {
                let mut e: Vec<usize> = [mates[m].a, mates[m].b]
                    .into_iter()
                    .filter(|&c| col[c] != usize::MAX)
                    .collect();
                e.dedup();
                e
            }
            None => vec![drag.expect("drag").component],
        }
    };
    let terms = own.len() + usize::from(drag.is_some());
    let all = |poses: &[Pose]| -> Vec<f64> {
        let mut r = Vec::new();
        for t in 0..terms {
            term(poses, t, &mut r);
        }
        r
    };
    let holds = |r: &[f64]| {
        r[..r.len() - if drag.is_some() { 12 } else { 0 }]
            .iter()
            .all(|v| v.abs() < TOL)
    };
    let nudge = |p: &Pose, k: usize, h: f64| {
        let mut e = [0.0; 3];
        e[k % 3] = h;
        if k < 3 {
            p.moved(e)
        } else {
            p.rotated(e, p.t)
        }
    };
    let mut r = all(poses);
    let sq = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
    let mut lambda = 1e-3;
    // A drag is solved again every frame, so a few steps per frame are enough.
    for _ in 0..if drag.is_some() {
        DRAG_ITERATIONS
    } else {
        MAX_ITERATIONS
    } {
        if drag.is_none() && holds(&r) {
            return true;
        }
        let mut a = DMatrix::<f64>::zeros(n, n);
        let mut g = DVector::<f64>::zeros(n);
        let mut trial = poses.to_vec();
        let mut base = Vec::new();
        for t in 0..terms {
            base.clear();
            term(poses, t, &mut base);
            // Columns of this term's Jacobian, by central differences.
            let mut cols: Vec<(usize, Vec<f64>)> = Vec::new();
            for c in ends(t) {
                for k in 0..6 {
                    let (mut plus, mut minus) = (Vec::new(), Vec::new());
                    trial[c] = nudge(&poses[c], k, STEP);
                    term(&trial, t, &mut plus);
                    trial[c] = nudge(&poses[c], k, -STEP);
                    term(&trial, t, &mut minus);
                    trial[c] = poses[c];
                    let d: Vec<f64> = plus.iter().zip(&minus).map(|(p, m)| (p - m) / (2.0 * STEP)).collect();
                    cols.push((col[c] + k, d));
                }
            }
            for (i, ci) in &cols {
                g[*i] += ci.iter().zip(&base).map(|(j, r)| j * r).sum::<f64>();
                for (j, cj) in &cols {
                    a[(*i, *j)] += ci.iter().zip(cj).map(|(x, y)| x * y).sum::<f64>();
                }
            }
        }
        let improved = loop {
            let mut m = a.clone();
            for i in 0..n {
                m[(i, i)] += lambda * a[(i, i)] + 1e-12;
            }
            let Some(chol) = m.cholesky() else {
                lambda *= 10.0;
                if lambda > 1e10 {
                    break None;
                }
                continue;
            };
            let delta = chol.solve(&(-&g));
            let mut next = poses.to_vec();
            for &c in comps {
                let k = col[c];
                let v = [delta[k], delta[k + 1], delta[k + 2]];
                let w = [delta[k + 3], delta[k + 4], delta[k + 5]];
                next[c] = poses[c].rotated(w, poses[c].t).moved(v);
            }
            let rn = all(&next);
            let (now, then) = (sq(&r), sq(&rn));
            if then < now {
                lambda = (lambda / 3.0).max(1e-12);
                break Some((next, rn, now - then <= 1e-12 * now));
            }
            lambda *= 4.0;
            if lambda > 1e6 {
                break None;
            }
        };
        let Some((next, rn, stalled)) = improved else { break };
        for &c in comps {
            poses[c] = next[c];
        }
        r = rn;
        if stalled {
            break;
        }
    }
    holds(&r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: P3 = [0.0, 0.0, 1.0];

    fn plane(p: P3, n: P3) -> Geom {
        Geom::Plane { p, n }
    }

    fn cyl(p: P3, d: P3, r: f64) -> Geom {
        Geom::Cylinder { p, d, r }
    }

    fn mate(kind: MateKind, a: usize, ga: Geom, b: usize, gb: Geom, value: f64) -> Mate {
        Mate {
            kind,
            a,
            ga,
            b,
            gb,
            value,
            flip: false,
            lock: Pose::IDENTITY,
        }
    }

    /// A tipped and shifted pose, so every mate has work to do.
    fn off() -> Pose {
        Pose::at([3.0, -7.0, 11.0]).rotated([0.3, -0.2, 0.5], [3.0, -7.0, 11.0])
    }

    fn holds(poses: &[Pose], mates: &[Mate]) {
        let len = length_scale(poses, mates);
        for m in mates {
            assert!(
                m.error(poses, len) < TOL,
                "{:?} is off by {}",
                m.kind,
                m.error(poses, len)
            );
        }
    }

    /// A 20 mm base block at the origin (fixed) and a part to place on it.
    fn place(mates: &[Mate]) -> Vec<Pose> {
        let mut poses = vec![Pose::IDENTITY, off()];
        assert_eq!(solve(&mut poses, &[true, false], mates, None), Vec::<usize>::new());
        assert_eq!(poses[0], Pose::IDENTITY, "a fixed part never moves");
        holds(&poses, mates);
        poses
    }

    #[test]
    fn coincident_faces_meet_face_to_face() {
        // The part's bottom (z = 0, normal −Z) on the base's top (z = 20).
        let m = mate(
            MateKind::Coincident,
            0,
            plane([0.0, 0.0, 20.0], Z),
            1,
            plane([0.0; 3], [0.0, 0.0, -1.0]),
            0.0,
        );
        let p = place(&[m]);
        assert!((p[1].apply([5.0, 5.0, 0.0])[2] - 20.0).abs() < 1e-6);
        assert!(norm(sub(p[1].turn(Z), Z)) < 1e-6, "it stands upright");
        // Flipped, the faces look the same way: the part hangs upside down through the top.
        let flipped = place(&[Mate { flip: true, ..m }]);
        assert!(norm(sub(flipped[1].turn(Z), [0.0, 0.0, -1.0])) < 1e-6);
    }

    #[test]
    fn a_concentric_pin_lines_up_with_its_hole() {
        let hole = cyl([10.0, 10.0, 0.0], Z, 3.0);
        let pin = cyl([0.0; 3], [1.0, 0.0, 0.0], 2.9);
        let p = place(&[mate(MateKind::Concentric, 0, hole, 1, pin, 0.0)]);
        let Geom::Cylinder { p: at, d, .. } = pin.moved(&p[1]) else {
            unreachable!()
        };
        assert!(norm(cross(d, Z)) < 1e-6);
        assert!((at[0] - 10.0).abs() < 1e-6 && (at[1] - 10.0).abs() < 1e-6);
    }

    #[test]
    fn distance_and_angle_mates_keep_their_value() {
        let top = plane([0.0, 0.0, 20.0], Z);
        let bottom = plane([0.0; 3], [0.0, 0.0, -1.0]);
        let p = place(&[mate(MateKind::Distance, 0, top, 1, bottom, 5.0)]);
        assert!((p[1].apply([0.0; 3])[2] - 25.0).abs() < 1e-6, "5 mm above the top");
        // The part's side turned 30° from the base's side.
        let side = plane([20.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let p = place(&[mate(MateKind::Angle, 0, side, 1, side, 30.0)]);
        let n = p[1].turn([1.0, 0.0, 0.0]);
        assert!((n[0] - 30f64.to_radians().cos()).abs() < 1e-6);
    }

    #[test]
    fn parallel_perpendicular_and_tangent_mates() {
        let side = plane([20.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let p = place(&[mate(MateKind::Parallel, 0, side, 1, side, 0.0)]);
        assert!(norm(cross(p[1].turn([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0])) < 1e-6);
        let p = place(&[mate(MateKind::Perpendicular, 0, side, 1, side, 0.0)]);
        assert!(dot(p[1].turn([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]).abs() < 1e-6);
        // A 4 mm roller lying on the top: its axis 4 mm above it, level.
        let roller = cyl([0.0; 3], [0.0, 1.0, 0.0], 4.0);
        let p = place(&[mate(MateKind::Tangent, 0, plane([0.0, 0.0, 20.0], Z), 1, roller, 0.0)]);
        assert!((p[1].apply([0.0; 3])[2] - 24.0).abs() < 1e-6);
        assert!(dot(p[1].turn([0.0, 1.0, 0.0]), Z).abs() < 1e-6);
        // Two cylinders side by side: axes r1 + r2 apart.
        let shaft = cyl([0.0; 3], Z, 5.0);
        let p = place(&[mate(MateKind::Tangent, 0, shaft, 1, roller, 0.0)]);
        let Geom::Cylinder { p: at, .. } = roller.moved(&p[1]) else {
            unreachable!()
        };
        assert!((at[0].hypot(at[1]) - 9.0).abs() < 1e-6);
    }

    #[test]
    fn a_locked_part_follows_the_part_it_is_locked_to() {
        let lock = Pose::at([0.0, 0.0, 20.0]);
        let m = Mate {
            lock,
            ..mate(MateKind::Lock, 0, plane([0.0; 3], Z), 1, plane([0.0; 3], Z), 0.0)
        };
        let mut poses = vec![off(), Pose::IDENTITY];
        assert!(solve(&mut poses, &[true, false], &[m], None).is_empty());
        assert!(norm(sub(poses[1].t, poses[0].apply([0.0, 0.0, 20.0]))) < 1e-6);
        assert!(norm(sub(poses[1].turn(Z), poses[0].turn(Z))) < 1e-6);
    }

    #[test]
    fn a_mate_that_contradicts_the_others_is_reported_and_left_out() {
        let top = plane([0.0, 0.0, 20.0], Z);
        let bottom = plane([0.0; 3], [0.0, 0.0, -1.0]);
        let mates = [
            mate(MateKind::Distance, 0, top, 1, bottom, 5.0),
            mate(MateKind::Distance, 0, top, 1, bottom, 9.0),
        ];
        let mut poses = vec![Pose::IDENTITY, off()];
        assert_eq!(solve(&mut poses, &[true, false], &mates, None), vec![1]);
        holds(&poses, &mates[..1]);
        // Two fixed parts cannot be moved to fix a mate between them.
        let mut poses = vec![Pose::IDENTITY, off()];
        assert_eq!(solve(&mut poses, &[true, true], &mates[..1], None), vec![0]);
    }

    #[test]
    fn a_dragged_part_moves_only_where_its_mates_let_it() {
        // A collar on a fixed vertical shaft: it slides and turns on it, nothing else.
        let shaft = cyl([0.0; 3], Z, 5.0);
        let mates = [mate(MateKind::Concentric, 0, shaft, 1, shaft, 0.0)];
        let mut poses = vec![Pose::IDENTITY, Pose::IDENTITY];
        let target = Pose::at([30.0, -10.0, 12.0]);
        let drag = Drag { component: 1, target };
        assert!(solve(&mut poses, &[true, false], &mates, Some(drag)).is_empty());
        holds(&poses, &mates);
        assert!(
            (poses[1].t[2] - 12.0).abs() < 1e-3,
            "it slid up the shaft: {:?}",
            poses[1].t
        );
        assert!(poses[1].t[0].hypot(poses[1].t[1]) < 1e-6, "but stayed on it");
        // A part with no mates goes where it is dragged.
        let mut poses = vec![Pose::IDENTITY, Pose::IDENTITY];
        assert!(solve(&mut poses, &[true, false], &[], Some(drag)).is_empty());
        assert!(norm(sub(poses[1].t, target.t)) < 1e-6);
    }

    #[test]
    fn a_hundred_part_chain_solves_at_interactive_speed() {
        // Each link stacks on the one before: concentric pins, flat faces 10 mm apart.
        let n = 100;
        let axis = cyl([0.0; 3], Z, 2.0);
        let mut mates = Vec::new();
        for i in 1..n {
            mates.push(mate(MateKind::Concentric, i - 1, axis, i, axis, 0.0));
            mates.push(mate(
                MateKind::Coincident,
                i - 1,
                plane([0.0, 0.0, 10.0], Z),
                i,
                plane([0.0; 3], [0.0, 0.0, -1.0]),
                0.0,
            ));
        }
        let mut poses: Vec<Pose> = (0..n)
            .map(|i| {
                let k = i as f64;
                Pose::at([0.3 * (k * 1.7).sin(), 0.3 * (k * 2.3).cos(), 10.0 * k + 0.5])
                    .rotated([0.02 * (k * 0.9).sin(), 0.03, 0.01], [0.0; 3])
            })
            .collect();
        poses[0] = Pose::IDENTITY;
        let mut fixed = vec![false; n];
        fixed[0] = true;
        let started = std::time::Instant::now();
        assert!(solve(&mut poses, &fixed, &mates, None).is_empty());
        let took = started.elapsed();
        holds(&poses, &mates);
        assert!((poses[n - 1].t[2] - 990.0).abs() < 1e-5);
        // Dragging the last link (a warm start) is what has to feel instant.
        let started = std::time::Instant::now();
        let drag = Drag {
            component: n - 1,
            target: poses[n - 1].moved([5.0, 0.0, 0.0]),
        };
        assert!(solve(&mut poses, &fixed, &mates, Some(drag)).is_empty());
        let dragged = started.elapsed();
        holds(&poses, &mates);
        eprintln!("100-part chain: solved in {took:?}, one drag frame in {dragged:?}");
        assert!(
            took.as_secs_f64() < 2.0 && dragged.as_secs_f64() < 1.0,
            "{took:?} / {dragged:?}"
        );
    }

    #[test]
    fn poses_compose_and_invert() {
        let p = off();
        let q = Pose::at([1.0, 2.0, 3.0]).rotated([0.0, 0.0, 1.0], [0.0; 3]);
        let x = [4.0, -5.0, 6.0];
        assert!(norm(sub(p.compose(&q).apply(x), p.apply(q.apply(x)))) < 1e-12);
        assert!(norm(sub(p.inverse().apply(p.apply(x)), x)) < 1e-12);
    }
}
