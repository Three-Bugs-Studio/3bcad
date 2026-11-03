//! Sweeps: a planar profile carried along a path (a chain of lines, arcs and splines in 3D, or a
//! helix) and skinned into a solid. Chain paths use rotation-minimizing frames, so the profile does
//! not spin around tangent-continuous paths; a helix uses the screw motion that maps the helix onto
//! itself, which keeps a thread profile in its plane through the axis.

use std::f64::consts::{PI, TAU};

use anyhow::{bail, ensure, Result};
use truck_modeling::Solid;

use crate::operations::{ProfileRegion, ProfileSegment};
use crate::skin::{self, Band, Caps, Ends, Piece};
use crate::solid::Plane;
use crate::topology::{cross, dot, norm, sub, unit, P3};

pub type Mat = [[f64; 4]; 4];

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P3, s: f64) -> P3 {
    a.map(|v| v * s)
}

/// A helix (or, with `pitch` 0, a flat spiral) around `axis` through `origin`.
#[derive(Clone, Debug, PartialEq)]
pub struct Helix {
    pub origin: P3,
    /// Unit axis direction; the helix rises along it.
    pub axis: P3,
    /// Unit direction from the axis to the start point.
    pub x: P3,
    /// Start radius.
    pub radius: f64,
    /// Rise per turn along the axis (0 for a spiral).
    pub pitch: f64,
    pub turns: f64,
    /// Radius change per turn: a taper (`pitch · tan(angle)`) or the spacing of a spiral.
    pub grow: f64,
    /// Counter-clockwise about `axis` (right-handed for a positive pitch).
    pub ccw: bool,
}

impl Helix {
    fn y(&self) -> P3 {
        cross(self.axis, self.x)
    }

    fn angle(&self, t: f64) -> f64 {
        let a = TAU * self.turns * t;
        if self.ccw {
            a
        } else {
            -a
        }
    }

    /// Point at `t` in 0..=1.
    pub fn point(&self, t: f64) -> P3 {
        let a = self.angle(t);
        let r = self.radius + self.grow * self.turns * t;
        let (s, c) = a.sin_cos();
        let (x, y) = (self.x, self.y());
        std::array::from_fn(|i| self.origin[i] + r * (c * x[i] + s * y[i]) + self.axis[i] * self.pitch * self.turns * t)
    }

    /// Unit tangent at `t`.
    pub fn tangent(&self, t: f64) -> P3 {
        let h = 1e-6;
        let (a, b) = (self.point((t - h).max(0.0)), self.point((t + h).min(1.0)));
        unit(sub(b, a))
    }

    pub fn length(&self) -> f64 {
        let n = (self.turns.abs() * 64.0).ceil().max(8.0) as usize;
        (0..n)
            .map(|k| {
                norm(sub(
                    self.point((k + 1) as f64 / n as f64),
                    self.point(k as f64 / n as f64),
                ))
            })
            .sum()
    }

    /// Points along the helix, `per_turn` per turn.
    pub fn polyline(&self, per_turn: usize) -> Vec<P3> {
        let n = (self.turns.abs() * per_turn as f64).ceil().max(2.0) as usize;
        (0..=n).map(|k| self.point(k as f64 / n as f64)).collect()
    }

    /// The rigid motion taking the start of the helix (and anything placed there) to `t`: turn about
    /// the axis, rise along it, and move out by the taper.
    pub fn motion(&self, t: f64) -> Mat {
        let a = self.angle(t);
        let rise = self.pitch * self.turns * t;
        let out = self.grow * self.turns * t;
        let r = rotation(self.origin, self.axis, a);
        // Move out along the start direction first, then turn and rise.
        let shift = scale(self.x, out);
        let lift = scale(self.axis, rise);
        let moved = |p: P3| add(skin::apply(&r, add(p, shift)), lift);
        affine_from(moved)
    }
}

/// Rotation by `angle` about the line through `p` along unit `d` (column-major).
pub fn rotation(p: P3, d: P3, angle: f64) -> Mat {
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

/// The affine map `f` as a matrix (from the images of the origin and the unit vectors).
fn affine_from(f: impl Fn(P3) -> P3) -> Mat {
    let o = f([0.0; 3]);
    let col = |e: P3| {
        let p = sub(f(e), o);
        [p[0], p[1], p[2], 0.0]
    };
    [
        col([1.0, 0.0, 0.0]),
        col([0.0, 1.0, 0.0]),
        col([0.0, 0.0, 1.0]),
        [o[0], o[1], o[2], 1.0],
    ]
}

/// A frame (x, y, z = tangent, origin) as a matrix taking frame coordinates to world.
fn frame(x: P3, y: P3, z: P3, o: P3) -> Mat {
    [
        [x[0], x[1], x[2], 0.0],
        [y[0], y[1], y[2], 0.0],
        [z[0], z[1], z[2], 0.0],
        [o[0], o[1], o[2], 1.0],
    ]
}

/// Inverse of a rigid frame matrix.
fn inverse_rigid(m: &Mat) -> Mat {
    let r: [P3; 3] = std::array::from_fn(|c| [m[c][0], m[c][1], m[c][2]]);
    let o = [m[3][0], m[3][1], m[3][2]];
    // Rows of the inverse are the columns of the rotation.
    let t = [-dot(r[0], o), -dot(r[1], o), -dot(r[2], o)];
    [
        [r[0][0], r[1][0], r[2][0], 0.0],
        [r[0][1], r[1][1], r[2][1], 0.0],
        [r[0][2], r[1][2], r[2][2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

pub fn mul(a: &Mat, b: &Mat) -> Mat {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}

/// One piece of a path in world space.
#[derive(Clone, Debug, PartialEq)]
pub enum PathSeg {
    Line(P3, P3),
    /// Arc about `center` with unit `normal` (right-handed), from `from` turning `sweep` radians.
    Arc {
        center: P3,
        normal: P3,
        from: P3,
        sweep: f64,
    },
    Bezier([P3; 4]),
}

impl PathSeg {
    pub fn point(&self, t: f64) -> P3 {
        match *self {
            PathSeg::Line(a, b) => add(a, scale(sub(b, a), t)),
            PathSeg::Arc {
                center,
                normal,
                from,
                sweep,
            } => {
                let r = rotation(center, unit(normal), sweep * t);
                skin::apply(&r, from)
            }
            PathSeg::Bezier(b) => {
                let u = 1.0 - t;
                let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                std::array::from_fn(|i| (0..4).map(|k| w[k] * b[k][i]).sum())
            }
        }
    }

    pub fn tangent(&self, t: f64) -> P3 {
        match *self {
            PathSeg::Line(a, b) => unit(sub(b, a)),
            PathSeg::Arc {
                center, normal, sweep, ..
            } => {
                let p = self.point(t);
                let t = cross(unit(normal), sub(p, center));
                unit(scale(t, sweep.signum()))
            }
            PathSeg::Bezier(_) => {
                let h = 1e-6;
                let (a, b) = (self.point((t - h).max(0.0)), self.point((t + h).min(1.0)));
                unit(sub(b, a))
            }
        }
    }

    pub fn length(&self) -> f64 {
        match *self {
            PathSeg::Line(a, b) => norm(sub(b, a)),
            PathSeg::Arc {
                center, from, sweep, ..
            } => norm(sub(from, center)) * sweep.abs(),
            PathSeg::Bezier(_) => (0..32)
                .map(|k| norm(sub(self.point((k + 1) as f64 / 32.0), self.point(k as f64 / 32.0))))
                .sum(),
        }
    }

    /// How many sections this piece needs.
    fn stations(&self, twist_here: f64) -> usize {
        let bend = match *self {
            PathSeg::Line(..) => 1,
            PathSeg::Arc { sweep, .. } => (sweep.abs() / (PI / 12.0)).ceil().max(2.0) as usize,
            PathSeg::Bezier(_) => 8,
        };
        bend.max((twist_here.abs() / (PI / 12.0)).ceil() as usize).max(1) + 1
    }

    pub fn start(&self) -> P3 {
        self.point(0.0)
    }

    /// The piece cut into stretches of at most a quarter turn, each skinned as its own faces.
    fn split(&self) -> Vec<PathSeg> {
        match *self {
            PathSeg::Arc {
                center,
                normal,
                from,
                sweep,
            } => {
                let n = ((sweep.abs() / (PI / 2.0) - 1e-9).ceil() as usize).max(1);
                (0..n)
                    .map(|k| PathSeg::Arc {
                        center,
                        normal,
                        from: skin::apply(&rotation(center, unit(normal), sweep * k as f64 / n as f64), from),
                        sweep: sweep / n as f64,
                    })
                    .collect()
            }
            ref other => vec![other.clone()],
        }
    }

    pub fn end(&self) -> P3 {
        self.point(1.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SweepPath {
    /// Pieces joined end to start; `closed` when the last ends where the first starts.
    Chain(Vec<PathSeg>),
    Helix(Helix),
}

impl SweepPath {
    pub fn start(&self) -> (P3, P3) {
        match self {
            SweepPath::Chain(segs) => (segs[0].start(), segs[0].tangent(0.0)),
            SweepPath::Helix(h) => (h.point(0.0), h.tangent(0.0)),
        }
    }

    pub fn length(&self) -> f64 {
        match self {
            SweepPath::Chain(segs) => segs.iter().map(PathSeg::length).sum(),
            SweepPath::Helix(h) => h.length(),
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            SweepPath::Chain(segs) => {
                segs.len() > 1 && norm(sub(segs[segs.len() - 1].end(), segs[0].start())) < 1e-6
                    || matches!(segs.as_slice(), [PathSeg::Arc { sweep, .. }] if (sweep.abs() - TAU).abs() < 1e-9)
            }
            SweepPath::Helix(_) => false,
        }
    }

    /// The path moved by a rigid transform.
    pub fn transformed(&self, m: &Mat) -> SweepPath {
        let p = |v: P3| skin::apply(m, v);
        let lin = |v: P3| sub(skin::apply(m, v), skin::apply(m, [0.0; 3]));
        let det = {
            let a = lin([1.0, 0.0, 0.0]);
            let b = lin([0.0, 1.0, 0.0]);
            let c = lin([0.0, 0.0, 1.0]);
            dot(a, cross(b, c))
        };
        match self {
            SweepPath::Chain(segs) => SweepPath::Chain(
                segs.iter()
                    .map(|s| match *s {
                        PathSeg::Line(a, b) => PathSeg::Line(p(a), p(b)),
                        PathSeg::Arc {
                            center,
                            normal,
                            from,
                            sweep,
                        } => PathSeg::Arc {
                            center: p(center),
                            normal: lin(normal),
                            from: p(from),
                            // A mirror turns the arc the other way about the mapped normal.
                            sweep: if det < 0.0 { -sweep } else { sweep },
                        },
                        PathSeg::Bezier(b) => PathSeg::Bezier(b.map(p)),
                    })
                    .collect(),
            ),
            SweepPath::Helix(h) => SweepPath::Helix(Helix {
                origin: p(h.origin),
                axis: lin(h.axis),
                x: lin(h.x),
                ccw: if det < 0.0 { !h.ccw } else { h.ccw },
                ..h.clone()
            }),
        }
    }

    /// Rigid motions taking the start of the path to each station, grouped into bands that each
    /// get their own faces; with the station parameters (arc length).
    pub fn stations(&self, twist: f64) -> Result<Vec<(Vec<Mat>, Vec<f64>)>> {
        match self {
            SweepPath::Helix(h) => {
                ensure!(h.turns > 1e-9, "the helix needs at least some turn");
                // A band per half turn, a section every 15 degrees.
                let bands = (h.turns * 2.0).ceil().max(1.0) as usize;
                let per = 12usize.max((twist.abs() / bands as f64 / (PI / 12.0)).ceil() as usize);
                let total = h.length();
                let (_, t0) = self.start();
                let p0 = h.point(0.0);
                Ok((0..bands)
                    .map(|b| {
                        let ts: Vec<f64> = (0..=per)
                            .map(|k| (b as f64 + k as f64 / per as f64) / bands as f64)
                            .collect();
                        let mats = ts
                            .iter()
                            .map(|&t| {
                                let tw = rotation(p0, t0, twist * t);
                                mul(&h.motion(t), &tw)
                            })
                            .collect();
                        (mats, ts.iter().map(|t| t * total).collect())
                    })
                    .collect())
            }
            SweepPath::Chain(segs) => {
                ensure!(!segs.is_empty(), "the path is empty");
                for w in segs.windows(2) {
                    ensure!(
                        norm(sub(w[0].end(), w[1].start())) < 1e-6,
                        "the path is not connected; its pieces must join end to end"
                    );
                    let (a, b) = (w[0].tangent(1.0), w[1].tangent(0.0));
                    if dot(a, b) < (2f64).to_radians().cos() {
                        bail!(
                            "the path has a sharp corner at ({:.2}, {:.2}, {:.2}); round it with a sketch fillet",
                            w[1].start()[0],
                            w[1].start()[1],
                            w[1].start()[2]
                        );
                    }
                }
                let total: f64 = segs.iter().map(PathSeg::length).sum();
                ensure!(total > 1e-9, "the path has no length");
                // Rotation-minimizing frames by double reflection (Wang et al. 2008), on a fine
                // walk along the path; stations pick frames off that walk.
                let (p0, t0) = (segs[0].start(), segs[0].tangent(0.0));
                let helper = if t0[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let mut r = unit(sub(helper, scale(t0, dot(helper, t0))));
                let (mut prev_p, mut prev_t) = (p0, t0);
                let f0 = frame(r, cross(t0, r), t0, p0);
                let inv0 = inverse_rigid(&f0);
                let mut s_acc = 0.0;
                let mut out = Vec::new();
                for whole in segs {
                    for seg in whole.split() {
                        let len = seg.length();
                        let n = seg.stations(twist * len / total);
                        let fine = 16;
                        let mut mats = Vec::new();
                        let mut params = Vec::new();
                        for k in 0..n {
                            let t_k = k as f64 / (n - 1) as f64;
                            if k > 0 {
                                let t_prev = (k - 1) as f64 / (n - 1) as f64;
                                for j in 1..=fine {
                                    let t = t_prev + (t_k - t_prev) * j as f64 / fine as f64;
                                    let (p, tan) = (seg.point(t), seg.tangent(t));
                                    let v1 = sub(p, prev_p);
                                    let c1 = dot(v1, v1);
                                    if c1 > 1e-24 {
                                        let rl = sub(r, scale(v1, 2.0 / c1 * dot(v1, r)));
                                        let tl = sub(prev_t, scale(v1, 2.0 / c1 * dot(v1, prev_t)));
                                        let v2 = sub(tan, tl);
                                        let c2 = dot(v2, v2);
                                        r = if c2 > 1e-24 {
                                            sub(rl, scale(v2, 2.0 / c2 * dot(v2, rl)))
                                        } else {
                                            rl
                                        };
                                    }
                                    r = unit(sub(r, scale(tan, dot(r, tan))));
                                    prev_p = p;
                                    prev_t = tan;
                                }
                            }
                            let s = s_acc + len * t_k;
                            let (p, tan) = (seg.point(t_k), seg.tangent(t_k));
                            let fk = frame(r, cross(tan, r), tan, p);
                            let tw = rotation([0.0; 3], [0.0, 0.0, 1.0], twist * s / total);
                            mats.push(mul(&mul(&fk, &tw), &inv0));
                            params.push(s);
                        }
                        s_acc += len;
                        out.push((mats, params));
                    }
                }
                if self.is_closed() {
                    // A closed path must end in the frame it started in (for a non-planar loop
                    // the frames come back turned): spread the difference as a twist.
                    let last = out.last().unwrap().0.last().unwrap();
                    let back = mul(last, &f0);
                    let rx: P3 = [back[0][0], back[0][1], back[0][2]];
                    let err = dot(cross(r_of(&f0), rx), t0).atan2(dot(r_of(&f0), rx));
                    if err.abs() > 1e-9 {
                        for (mats, params) in &mut out {
                            for (m, s) in mats.iter_mut().zip(params.iter()) {
                                let at = skin::apply(m, p0);
                                let tan = sub(skin::apply(m, add(p0, t0)), at);
                                *m = mul(&rotation(at, unit(tan), -err * s / total), m);
                            }
                        }
                    }
                }
                Ok(out)
            }
        }
    }
}

fn r_of(m: &Mat) -> P3 {
    [m[0][0], m[0][1], m[0][2]]
}

/// The profile regions of `plane` carried along `path`: one solid per region.
pub fn sweep(regions: &[ProfileRegion], plane: &Plane, path: &SweepPath, twist: f64) -> Result<Vec<Solid>> {
    ensure!(!regions.is_empty(), "nothing to sweep");
    let to_world = plane.matrix();
    let bands = path.stations(twist)?;
    let closed = path.is_closed();
    regions
        .iter()
        .map(|r| {
            let loops: Vec<Vec<Piece>> = std::iter::once(&r.outer)
                .chain(&r.holes)
                .map(|lp| {
                    skin::loop_pieces(lp)
                        .0
                        .iter()
                        .map(|p| p.transformed(&to_world))
                        .collect()
                })
                .collect();
            let skin_bands: Vec<Band> = bands
                .iter()
                .map(|(mats, params)| Band {
                    sections: mats
                        .iter()
                        .map(|m| {
                            loops
                                .iter()
                                .map(|lp| lp.iter().map(|p| p.transformed(m)).collect())
                                .collect()
                        })
                        .collect(),
                    params: params.clone(),
                    ends: Ends::default(),
                })
                .collect();
            skin::skin(&skin_bands, if closed { Caps::Closed } else { Caps::Planar })
        })
        .collect()
}

/// Which face of a swept tool a point lies on: start cap, end cap or the wall of profile segment
/// `Side(i)` (counted like `Tool::face_at`).
pub fn face_at(
    regions: &[ProfileRegion],
    plane: &Plane,
    path: &SweepPath,
    twist: f64,
    p: P3,
) -> Option<(bool, bool, [f64; 2])> {
    let bands = path.stations(twist).ok()?;
    let to_world = plane.matrix();
    let mut best: Option<(f64, usize, usize, [f64; 2], f64)> = None;
    let count = bands.len();
    for (bi, (mats, _)) in bands.iter().enumerate() {
        for (k, m) in mats.iter().enumerate() {
            // Station plane: the profile plane moved by m.
            let full = mul(m, &to_world);
            let inv = invert_affine(&full)?;
            let local = skin::apply(&inv, p);
            let w = local[2].abs();
            if best.as_ref().is_none_or(|b| w < b.0) {
                best = Some((w, bi, k, [local[0], local[1]], local[2]));
            }
        }
    }
    let (w, bi, k, local, _) = best?;
    let scale = regions
        .iter()
        .flat_map(|r| crate::offset::sample_loop(&r.outer, 16))
        .map(|q| q[0].abs().max(q[1].abs()))
        .fold(1.0, f64::max);
    let tol = 1e-5 * scale + 2.0 * crate::boolean::GROW;
    let closed = path.is_closed();
    let at_start = !closed && bi == 0 && k == 0 && w < tol;
    let at_end = !closed && bi + 1 == count && k + 1 == bands[bi].0.len() && w < tol;
    Some((at_start, at_end, local))
}

/// Inverse of a general affine matrix (column-major).
pub fn invert_affine(m: &Mat) -> Option<Mat> {
    let a = nalgebra::Matrix4::from_fn(|r, c| m[c][r]);
    let inv = a.try_inverse()?;
    Some(std::array::from_fn(|c| std::array::from_fn(|r| inv[(r, c)])))
}

/// The ISO metric thread space (the groove between two teeth) for pitch `p`, in a plane through the
/// axis: x is the distance from the axis, y runs along it. `major` is the major radius; external
/// grooves are cut inward from it, internal ones outward from the minor radius. The groove reaches
/// `margin` past the material so the cut never grazes the surface.
pub fn iso_groove(p: f64, major: f64, internal: bool, margin: f64) -> Vec<[f64; 2]> {
    let h = 3f64.sqrt() / 2.0 * p;
    let t30 = (PI / 6.0).tan();
    if !internal {
        // Root flat p/4 at the minor radius, crest flat p/8 at the major radius (basic profile).
        let r1 = major - 5.0 * h / 8.0;
        let w_root = p / 8.0;
        let w_out = 7.0 * p / 16.0 + margin * t30;
        let ro = major + margin;
        vec![[r1, -w_root], [ro, -w_out], [ro, w_out], [r1, w_root]]
    } else {
        // Internal: the nut's root at the major radius (flat p/8), its crest at the minor radius
        // (flat p/4); the groove runs out of the hole wall into the nut.
        let r1 = major - 5.0 * h / 8.0;
        let w_root = p / 16.0;
        let w_in = 3.0 * p / 8.0 + margin * t30;
        let ri = r1 - margin;
        vec![[ri, -w_in], [major, -w_root], [major, w_root], [ri, w_in]]
    }
}

/// Cross-section of an ISO metric thread in a plane across its axis, as a closed loop around the
/// origin (local x, y): the distance from the axis as the thread turns once. Between the minor
/// radius `r_min` and the major radius `r_max` the basic profile has a root flat of `p/4`, a crest
/// flat of `p/8` and 60° flanks; `low` and `high` are where the loop's flats really sit, so the
/// flanks can run on a little past the material and cut it cleanly. `phase` turns the loop.
///
/// Carried by the thread's own screw motion, this loop sweeps exactly the threaded surface, with
/// flat ends across the axis.
pub fn thread_section(
    p: f64,
    r_min: f64,
    r_max: f64,
    low: f64,
    high: f64,
    phase: f64,
    ccw: bool,
) -> Vec<ProfileSegment> {
    // The profile r(z) over one pitch, the root flat centered at z = 0; flanks have dz/dr = tan 30°.
    let t30 = (PI / 6.0).tan();
    let root = p / 8.0 - (r_min - low) * t30;
    let crest = p / 16.0 - (high - r_max) * t30;
    let rise = (high - low) * t30;
    // Breakpoints in z (one pitch from -root): root flat, flank up, crest flat, flank down.
    let zs = [
        -root,
        root,
        root + rise,
        root + rise + 2.0 * crest,
        root + 2.0 * rise + 2.0 * crest,
    ];
    debug_assert!((zs[4] - (p - root)).abs() < 1e-9);
    let r_at = |z: f64| -> f64 {
        let z = (z + root).rem_euclid(p) - root;
        if z <= zs[1] {
            low
        } else if z <= zs[2] {
            low + (z - zs[1]) / t30
        } else if z <= zs[3] {
            high
        } else {
            high - (z - zs[3]) / t30
        }
    };
    // At angle a the profile has risen by p·a/2π, so the section sees z = -p·a/2π.
    let dir = if ccw { 1.0 } else { -1.0 };
    let pt = |a: f64| -> [f64; 2] {
        let r = r_at(-p * a / TAU);
        let th = phase + dir * a;
        [r * th.cos(), r * th.sin()]
    };
    // Angles (0..2π) where the profile changes piece, walking z downwards from 0.
    let mut cuts: Vec<f64> = zs.iter().map(|z| (-z / p).rem_euclid(1.0) * TAU).collect();
    cuts.push(0.0);
    cuts.push(TAU);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let mut out = Vec::new();
    for w in cuts.windows(2) {
        let (a0, a1) = (w[0], w[1]);
        let mid = (a0 + a1) / 2.0;
        let (r0, rm, r1) = (r_at(-p * a0 / TAU), r_at(-p * mid / TAU), r_at(-p * a1 / TAU));
        let _ = (r0, r1);
        let flat = (r_at(-p * (a0 + 1e-6) / TAU) - r_at(-p * (a1 - 1e-6) / TAU)).abs() < 1e-9;
        if flat {
            out.push(ProfileSegment::Arc {
                center: [0.0, 0.0],
                from: pt(a0),
                to: pt(a1),
                ccw,
            });
            let _ = rm;
            continue;
        }
        // A flank is an Archimedean spiral: cubic Hermite pieces of at most 30°.
        let n = ((a1 - a0) / (PI / 6.0)).ceil().max(1.0) as usize;
        for k in 0..n {
            let (b0, b1) = (
                a0 + (a1 - a0) * k as f64 / n as f64,
                a0 + (a1 - a0) * (k + 1) as f64 / n as f64,
            );
            let h = 1e-7;
            let d = |a: f64| {
                let (q0, q1) = (pt(a - h), pt(a + h));
                [(q1[0] - q0[0]) / (2.0 * h), (q1[1] - q0[1]) / (2.0 * h)]
            };
            // Tangents taken just inside the piece, where the spiral is smooth.
            let (d0, d1) = (d(b0 + 2.0 * h), d(b1 - 2.0 * h));
            let s = (b1 - b0) / 3.0;
            let (p0, p3) = (pt(b0), pt(b1));
            out.push(ProfileSegment::Bezier([
                p0,
                [p0[0] + d0[0] * s, p0[1] + d0[1] * s],
                [p3[0] - d1[0] * s, p3[1] - d1[1] * s],
                p3,
            ]));
        }
    }
    // Close exactly: the last piece ends where the first starts.
    let first = crate::offset::seg_start(&out[0]);
    match out.last_mut().unwrap() {
        ProfileSegment::Arc { to, .. } => *to = first,
        ProfileSegment::Bezier(b) => b[3] = first,
        _ => {}
    }
    out
}

/// A profile region from a polygon (counter-clockwise).
pub fn polygon_region(pts: &[[f64; 2]]) -> ProfileRegion {
    let n = pts.len();
    let mut lp: Vec<ProfileSegment> = (0..n)
        .map(|i| ProfileSegment::Line {
            from: pts[i],
            to: pts[(i + 1) % n],
        })
        .collect();
    if crate::offset::signed_area(&lp) < 0.0 {
        lp = pts
            .iter()
            .rev()
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| ProfileSegment::Line { from: *w[0], to: *w[1] })
            .chain(std::iter::once(ProfileSegment::Line {
                from: pts[0],
                to: pts[n - 1],
            }))
            .collect();
    }
    ProfileRegion {
        outer: lp,
        holes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::{self, volume};
    use crate::mesher::print_mesh;

    #[test]
    fn a_circle_swept_along_a_line_and_an_arc_is_a_bent_pipe() {
        // Radius 1 profile on the Front plane at the origin, path up z then a quarter turn.
        let plane = Plane::TOP;
        let region = ProfileRegion {
            outer: vec![ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: 1.0,
                ccw: true,
            }],
            holes: Vec::new(),
        };
        let path = SweepPath::Chain(vec![
            PathSeg::Line([0.0; 3], [0.0, 0.0, 10.0]),
            PathSeg::Arc {
                center: [5.0, 0.0, 10.0],
                normal: [0.0, 1.0, 0.0],
                from: [0.0, 0.0, 10.0],
                sweep: PI / 2.0,
            },
        ]);
        let s = sweep(&[region], &plane, &path, 0.0).unwrap().remove(0);
        // Pappus: area x path length.
        let expected = PI * (10.0 + 5.0 * PI / 2.0);
        let v = volume(&s).unwrap();
        // A thin pipe meshes coarsely relative to its size.
        assert!((v - expected).abs() / expected < 2e-2, "{v} vs {expected}");
        // The far end sits at x = 5, z = 15.
        let (lo, hi) = boolean::bbox(&s);
        assert!(
            (hi[0] - 5.0).abs() < 1e-3 && (hi[2] - 16.0).abs() < 1e-3 && lo[2].abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
    }

    #[test]
    fn a_square_swept_round_a_circle_is_a_ring() {
        let plane = Plane::FRONT;
        let sq = polygon_region(&[[9.0, -1.0], [11.0, -1.0], [11.0, 1.0], [9.0, 1.0]]);
        let path = SweepPath::Chain(vec![PathSeg::Arc {
            center: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            from: [10.0, 0.0, 0.0],
            sweep: TAU,
        }]);
        let s = sweep(&[sq], &plane, &path, 0.0).unwrap().remove(0);
        let expected = 4.0 * TAU * 10.0;
        let v = volume(&s).unwrap();
        assert!((v - expected).abs() / expected < 5e-3, "{v} vs {expected}");
    }

    #[test]
    fn a_twisted_square_keeps_its_volume() {
        let sq = polygon_region(&[[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]]);
        let path = SweepPath::Chain(vec![PathSeg::Line([0.0; 3], [0.0, 0.0, 20.0])]);
        let s = sweep(&[sq], &Plane::TOP, &path, PI / 2.0).unwrap().remove(0);
        let v = volume(&s).unwrap();
        assert!((v - 320.0).abs() / 320.0 < 5e-3, "{v}");
        let m = print_mesh(&s, 0.01);
        assert!(!m.triangles.is_empty());
    }

    #[test]
    fn a_sharp_corner_in_the_path_is_refused() {
        let sq = polygon_region(&[[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]);
        let path = SweepPath::Chain(vec![
            PathSeg::Line([0.0; 3], [0.0, 0.0, 10.0]),
            PathSeg::Line([0.0, 0.0, 10.0], [10.0, 0.0, 10.0]),
        ]);
        let e = sweep(&[sq], &Plane::TOP, &path, 0.0).unwrap_err();
        assert!(e.to_string().contains("sharp corner"), "{e}");
    }

    #[test]
    fn a_helical_spring_has_the_right_volume() {
        // Circle of radius 0.5 swept along a helix of radius 5, pitch 3, 2 turns.
        let h = Helix {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            radius: 5.0,
            pitch: 3.0,
            turns: 2.0,
            grow: 0.0,
            ccw: true,
        };
        let plane = Plane {
            origin: [5.0, 0.0, 0.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 0.0, 1.0],
        };
        let c = ProfileRegion {
            outer: vec![ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: 0.5,
                ccw: true,
            }],
            holes: Vec::new(),
        };
        let s = sweep(&[c], &plane, &SweepPath::Helix(h), 0.0).unwrap().remove(0);
        // A profile through the axis turned by screw motion: Pappus with the circle's centroid path
        // turning around the axis (rise does not change the swept volume).
        let expected = PI * 0.25 * TAU * 5.0 * 2.0;
        let v = volume(&s).unwrap();
        assert!((v - expected).abs() / expected < 1e-2, "{v} vs {expected}");
        let (_, hi) = boolean::bbox(&s);
        assert!((hi[2] - 6.5).abs() < 1e-3, "{hi:?}");
    }
}
