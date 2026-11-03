//! Modelled threads. Cutting a helical groove with a boolean is unreliable in truck (the groove
//! running out through an end face breaks the face splitting at random), so threads are stitched in:
//!
//! 1. a coaxial cylinder cut clears the threaded length (a robust cylinder/plane boolean);
//! 2. the thread itself is swept by its own screw motion from its cross-section
//!    (`sweep::thread_section`): the whole threaded core for an external thread, the ring between
//!    the thread and a *glue* circle outside the major diameter for an internal one;
//! 3. where the thread stops inside the material, its end loop becomes a hole in the cut's flat end
//!    face (external), or replaces the loop on the glue circle (internal, whose glue cylinder faces
//!    are dropped), so the swept faces join the body edge to edge.

use anyhow::{anyhow, ensure, Result};
use truck_modeling::*;

use crate::operations::{extrude, ProfileRegion, ProfileSegment};
use crate::solid::Plane;
use crate::sweep::{self, Helix, SweepPath};
use crate::topology::{cross, dot, norm, sub, unit, P3};

/// Where and what thread to model on a body.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadSpec {
    /// A point on the thread axis; `a` and `b` are measured from it along `axis`.
    pub origin: P3,
    /// Unit axis.
    pub axis: P3,
    /// Unit direction across the axis where the thread starts.
    pub x: P3,
    pub pitch: f64,
    /// Basic major radius (half the nominal diameter).
    pub major: f64,
    /// Radius of the threaded cylinder face: the rod for an external thread, the hole otherwise.
    pub wall: f64,
    pub internal: bool,
    /// Right-handed.
    pub right: bool,
    /// The threaded length, `a < b`, along the axis from `origin`.
    pub a: f64,
    pub b: f64,
    /// The thread runs out through the end face at `a` (`b`) rather than stopping in the material.
    pub free_a: bool,
    pub free_b: bool,
}

impl ThreadSpec {
    fn y(&self) -> P3 {
        cross(self.axis, self.x)
    }

    fn at(&self, s: f64) -> P3 {
        std::array::from_fn(|i| self.origin[i] + self.axis[i] * s)
    }

    /// Basic minor radius (ISO 68-1: major − 5H/8).
    pub fn minor(&self) -> f64 {
        self.major - 5.0 * 3f64.sqrt() / 2.0 * self.pitch / 8.0
    }

    /// The glue radius (internal threads) and where the flats of the thread loop sit: just inside
    /// the rod, or just outside the hole, so no face of the thread grazes the cylinder.
    fn radii(&self) -> (f64, f64, f64) {
        let delta = (0.01 * self.pitch).min(0.01);
        let r1 = self.minor();
        if self.internal {
            let glue = self.major + 0.2 * self.pitch;
            (glue, self.wall.max(r1) + delta, self.major)
        } else {
            (0.9 * r1, r1, self.wall.min(self.major) - delta)
        }
    }
}

/// Distance from the axis and position along it.
fn polar(spec: &ThreadSpec, p: P3) -> (f64, f64) {
    let d = sub(p, spec.origin);
    let s = dot(d, spec.axis);
    let r = norm(sub(d, spec.axis.map(|v| v * s)));
    (r, s)
}

fn reverse(lp: &[ProfileSegment]) -> Vec<ProfileSegment> {
    lp.iter()
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

fn ccw_loop(lp: Vec<ProfileSegment>, ccw: bool) -> Vec<ProfileSegment> {
    if (crate::offset::signed_area(&lp) > 0.0) == ccw {
        lp
    } else {
        reverse(&lp)
    }
}

fn circle(r: f64, ccw: bool) -> Vec<ProfileSegment> {
    vec![ProfileSegment::Circle {
        center: [0.0, 0.0],
        radius: r,
        ccw,
    }]
}

/// Points along an edge, for classifying it.
fn edge_points(e: &Edge) -> Vec<P3> {
    let c = e.curve();
    let (t0, t1) = c.range_tuple();
    (0..=4)
        .map(|k| {
            let p = c.subs(t0 + (t1 - t0) * k as f64 / 4.0);
            [p.x, p.y, p.z]
        })
        .collect()
}

fn on_circle(spec: &ThreadSpec, pts: &[P3], r: f64, s: Option<f64>, tol: f64) -> bool {
    pts.iter().all(|&p| {
        let (pr, ps) = polar(spec, p);
        (pr - r).abs() < tol && s.is_none_or(|s| (ps - s).abs() < tol)
    })
}

fn wire_points(w: &Wire) -> Vec<P3> {
    w.edge_iter().flat_map(edge_points).collect()
}

fn face_points(f: &Face) -> Vec<P3> {
    f.boundaries().iter().flat_map(wire_points).collect()
}

fn is_plane(f: &Face) -> bool {
    matches!(f.surface(), Surface::Plane(_))
}

/// The thread solid swept by the screw motion over `a..b`: the threaded core, or for an internal
/// thread the ring between the glue circle and the thread.
fn sleeve(spec: &ThreadSpec) -> Result<Solid> {
    let (glue, low, high) = spec.radii();
    let p = spec.pitch;
    let (r_min, r_max) = (spec.minor(), spec.major);
    let thread = sweep::thread_section(p, r_min, r_max, low, high, 0.0, spec.right);
    let region = if spec.internal {
        ProfileRegion {
            outer: circle(glue, true),
            holes: vec![ccw_loop(thread, false)],
        }
    } else {
        ProfileRegion {
            outer: ccw_loop(thread, true),
            holes: Vec::new(),
        }
    };
    let origin = spec.at(spec.a);
    let plane = Plane {
        origin,
        x: spec.x,
        y: spec.y(),
    };
    let helix = Helix {
        origin,
        axis: spec.axis,
        x: spec.x,
        radius: 0.0,
        pitch: p,
        turns: (spec.b - spec.a) / p,
        grow: 0.0,
        ccw: spec.right,
    };
    sweep::sweep(&[region], &plane, &SweepPath::Helix(helix), 0.0)?
        .pop()
        .ok_or_else(|| anyhow!("the thread did not sweep"))
}

/// The cut that clears room for the sleeve, reaching past free ends.
fn clearance(spec: &ThreadSpec) -> Result<Solid> {
    let (glue, _, _) = spec.radii();
    let over = spec.pitch + 0.1 * spec.wall;
    let a = if spec.free_a { spec.a - over } else { spec.a };
    let b = if spec.free_b { spec.b + over } else { spec.b };
    let region = if spec.internal {
        ProfileRegion {
            outer: circle(glue, true),
            holes: Vec::new(),
        }
    } else {
        ProfileRegion {
            outer: circle(spec.wall.max(spec.major) + 0.2 * spec.pitch + 0.05 * spec.wall, true),
            holes: Vec::new(),
        }
    };
    // Seams away from the body's own (which usually start on x), so no seam edges coincide.
    let (sn, cs) = 0.7f64.sin_cos();
    let (x, y) = (spec.x, spec.y());
    let x: P3 = std::array::from_fn(|i| cs * x[i] + sn * y[i]);
    let plane = Plane {
        origin: spec.at(a),
        x,
        y: cross(spec.axis, x),
    };
    extrude(&[region], b - a, &plane)?
        .pop()
        .ok_or_else(|| anyhow!("the thread clearance did not build"))
}

/// Models the thread on `body` (one closed shell); the result is watertight.
pub fn thread(body: &Solid, spec: &ThreadSpec) -> Result<Solid> {
    ensure!(spec.pitch > 1e-6, "the thread pitch must be positive");
    ensure!(
        spec.b - spec.a > spec.pitch * 0.25,
        "the thread is shorter than a quarter pitch"
    );
    ensure!(
        norm(spec.axis) > 0.5 && dot(spec.axis, spec.x).abs() < 1e-6,
        "bad thread frame"
    );
    let (glue, low, high) = spec.radii();
    ensure!(high - low > 1e-3, "the cylinder is too small for this thread");
    if !spec.internal {
        ensure!(
            spec.wall > spec.minor() + 1e-3,
            "the rod is thinner than the thread's minor diameter"
        );
    } else {
        ensure!(
            spec.wall < spec.major - 1e-3,
            "the hole is wider than the thread's major diameter"
        );
    }
    ensure!(body.boundaries().len() == 1, "threads need a body without voids");

    let sleeve = sleeve(spec)?;
    let cleared = crate::boolean::subtract(body, &clearance(spec)?)?;
    if cleared.is_empty() && !spec.internal && spec.free_a && spec.free_b {
        // A bare rod threaded end to end: the sleeve, with its own caps, is the whole part.
        return Ok(sleeve);
    }
    ensure!(!cleared.is_empty(), "the thread clearance removes the whole body");
    // Both parts are outward-facing, so the stitched shell is too.
    stitch(spec, &cleared, &sleeve, glue)
}

/// Joins the cleared body (in one or more pieces) and the thread sleeve into one shell.
fn stitch(spec: &ThreadSpec, cleared: &[Solid], sleeve: &Solid, glue: f64) -> Result<Solid> {
    let tol = 1e-4 * spec.major.max(1.0) + 2.0 * crate::boolean::GROW;
    let free = [spec.free_a, spec.free_b];
    let ends = [spec.a, spec.b];
    let end_of = |s: f64| (0..2).find(|&e| (s - ends[e]).abs() < tol);
    let mut faces: Vec<Face> = Vec::new();
    // Thread-shaped loops of the sleeve's caps, by end.
    let mut loops: [Option<Wire>; 2] = [None, None];
    for f in sleeve.boundaries()[0].face_iter() {
        let pts = face_points(f);
        if is_plane(f) {
            let (_, s) = polar(spec, pts[0]);
            let end = end_of(s).ok_or_else(|| anyhow!("the thread sleeve ends in the wrong place"))?;
            if free[end] && !spec.internal {
                faces.push(f.clone());
                continue;
            }
            let w = f
                .boundaries()
                .into_iter()
                .find(|w| !on_circle(spec, &wire_points(w), glue, None, tol))
                .ok_or_else(|| anyhow!("the thread end has no thread loop"))?;
            loops[end] = Some(w);
        } else if !(spec.internal && on_circle(spec, &pts, glue, None, tol)) {
            faces.push(f.clone());
        }
    }
    let mut used = [false; 2];
    for piece in cleared {
        for sh in piece.boundaries() {
            for f in sh.face_iter() {
                let pts = face_points(f);
                // truck leaves faces that lie wholly inside the cut (an old, smaller hole) in place.
                let inside = |pts: &[P3]| spec.internal && pts.iter().all(|&p| polar(spec, p).0 < glue - tol);
                if !is_plane(f) {
                    if !(spec.internal && on_circle(spec, &pts, glue, None, tol)) && !inside(&pts) {
                        faces.push(f.clone());
                    }
                    continue;
                }
                let mut wires = f.boundaries();
                let n = wires.len();
                wires.retain(|w| !inside(&wire_points(w)));
                let mut swapped = wires.len() != n;
                if spec.internal {
                    // Swap the loop on the glue circle for the thread loop.
                    for (end, &s) in ends.iter().enumerate() {
                        if let Some(i) = wires
                            .iter()
                            .position(|w| on_circle(spec, &wire_points(w), glue, Some(s), tol))
                        {
                            ensure!(!used[end], "the thread end meets the body twice");
                            wires[i] = loops[end]
                                .clone()
                                .ok_or_else(|| anyhow!("the thread sleeve has no end"))?;
                            used[end] = true;
                            swapped = true;
                        }
                    }
                } else if let Some(end) = pts
                    .iter()
                    .map(|&p| polar(spec, p))
                    .try_fold(None, |acc: Option<usize>, (_, s)| {
                        let e = end_of(s)?;
                        (acc.is_none() || acc == Some(e)).then_some(Some(e))
                    })
                    .flatten()
                {
                    // A cap of the clearance cut: the thread loop becomes a hole in it.
                    if !free[end] {
                        ensure!(!used[end], "the thread end meets the body twice");
                        ensure!(wires.len() == 1, "external threads on hollow rods are not supported");
                        wires.push(
                            loops[end]
                                .clone()
                                .ok_or_else(|| anyhow!("the thread sleeve has no end"))?,
                        );
                        used[end] = true;
                        swapped = true;
                    }
                }
                if swapped {
                    faces.push(crate::topology::face_like(f, wires).map_err(|e| anyhow!("thread end face: {e:?}"))?);
                } else {
                    faces.push(f.clone());
                }
            }
        }
    }
    for end in 0..2 {
        ensure!(
            used[end] || (free[end] && !spec.internal),
            "the thread end does not meet a flat face of the body"
        );
    }
    let shell: Shell = faces.into();
    ensure!(
        shell.shell_condition() == ShellCondition::Closed,
        "the threaded body does not close"
    );
    Solid::try_new(vec![shell]).map_err(|e| anyhow!("the threaded body is invalid: {e:?}"))
}

/// The thread spec's frame from an axis: any unit direction across it.
pub fn across(axis: P3) -> P3 {
    let a = unit(axis);
    let t = if a[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    unit(cross(cross(a, t), a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::volume;
    use crate::mesher::print_mesh;
    use crate::operations::extrude_circle;

    fn spec(internal: bool, wall: f64, a: f64, b: f64, free_a: bool, free_b: bool) -> ThreadSpec {
        ThreadSpec {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            pitch: 1.0,
            major: 3.0,
            wall,
            internal,
            right: true,
            a,
            b,
            free_a,
            free_b,
        }
    }

    /// The thread's cross-section area (one full turn of the loop) times the length is what is
    /// left of the threaded part.
    fn loop_area(s: &ThreadSpec) -> f64 {
        let (_, low, high) = s.radii();
        let lp = sweep::thread_section(s.pitch, s.minor(), s.major, low, high, 0.0, true);
        crate::offset::signed_area(&lp).abs()
    }

    #[test]
    fn an_external_thread_runs_out_of_the_free_end() {
        let rod = extrude_circle([0.0, 0.0], 3.0, 10.0).unwrap();
        let s = spec(false, 3.0, 4.0, 10.0, false, true);
        let t = std::time::Instant::now();
        let out = thread(&rod, &s).unwrap();
        eprintln!("external thread took {:?}", t.elapsed());
        let v = volume(&out).unwrap();
        let expected = std::f64::consts::PI * 9.0 * 4.0 + loop_area(&s) * 6.0;
        assert!((v - expected).abs() / expected < 5e-3, "{v} vs {expected}");
        assert!(!print_mesh(&out, 0.01).triangles.is_empty());
    }

    #[test]
    fn a_bare_rod_threads_end_to_end() {
        let rod = extrude_circle([0.0, 0.0], 3.0, 10.0).unwrap();
        let s = spec(false, 3.0, 0.0, 10.0, true, true);
        let out = thread(&rod, &s).unwrap();
        let v = volume(&out).unwrap();
        let expected = loop_area(&s) * 10.0;
        // All of it is helical surface: the mesh sits 0.6 % inside.
        assert!((v - expected).abs() / expected < 1e-2, "{v} vs {expected}");
    }

    #[test]
    fn an_external_thread_can_stop_in_the_middle() {
        let rod = extrude_circle([0.0, 0.0], 3.0, 10.0).unwrap();
        let s = spec(false, 3.0, 2.0, 8.0, false, false);
        let out = thread(&rod, &s).unwrap();
        let v = volume(&out).unwrap();
        let expected = std::f64::consts::PI * 9.0 * 4.0 + loop_area(&s) * 6.0;
        assert!((v - expected).abs() / expected < 5e-3, "{v} vs {expected}");
    }

    #[test]
    fn an_internal_thread_goes_through_a_plate() {
        let plate =
            crate::operations::extrude_polygon(&[[-8.0, -8.0], [8.0, -8.0], [8.0, 8.0], [-8.0, 8.0]], 5.0).unwrap();
        let hole = extrude_circle([0.0, 0.0], 2.5, 5.0).unwrap();
        let mut hole = hole;
        hole = crate::boolean::transformed(&hole, crate::sweep::rotation([0.0; 3], [0.0, 0.0, 1.0], 0.0));
        let drilled = crate::boolean::subtract(&plate, &hole).unwrap().remove(0);
        let s = spec(true, 2.5, 0.0, 5.0, true, true);
        let t = std::time::Instant::now();
        let out = thread(&drilled, &s).unwrap();
        eprintln!("internal thread took {:?}", t.elapsed());
        let v = volume(&out).unwrap();
        let expected = 256.0 * 5.0 - loop_area(&s) * 5.0;
        assert!((v - expected).abs() / expected < 5e-3, "{v} vs {expected}");
    }
}
