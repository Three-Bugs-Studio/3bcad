//! Offsetting profile loops and open chains in their plane: drafts, thin features, shells, ribs.
//! Distances are signed: positive moves a segment to the left of its direction of travel, which is
//! into the material for a counter-clockwise outer loop and for a clockwise hole.

use anyhow::{bail, ensure, Result};

use crate::operations::{ProfileRegion, ProfileSegment};

type P2 = [f64; 2];

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}

fn left(d: P2) -> P2 {
    [-d[1], d[0]]
}

fn unit(a: P2) -> P2 {
    scale(a, 1.0 / len(a))
}

/// Start point of a segment (circles start at angle 0).
pub fn seg_start(s: &ProfileSegment) -> P2 {
    match *s {
        ProfileSegment::Line { from, .. } | ProfileSegment::Arc { from, .. } => from,
        ProfileSegment::Circle { center, radius, .. } => [center[0] + radius, center[1]],
        ProfileSegment::Bezier(b) => b[0],
    }
}

pub fn seg_end(s: &ProfileSegment) -> P2 {
    match *s {
        ProfileSegment::Line { to, .. } | ProfileSegment::Arc { to, .. } => to,
        ProfileSegment::Circle { .. } => seg_start(s),
        ProfileSegment::Bezier(b) => b[3],
    }
}

/// The infinite curve a segment lies on, after moving it by `d` to its left.
#[derive(Clone, Copy, Debug)]
enum Carrier {
    Line { p: P2, dir: P2 },
    Circle { c: P2, r: f64 },
}

fn carrier(s: &ProfileSegment, d: f64) -> Result<Carrier> {
    Ok(match *s {
        ProfileSegment::Line { from, to } => {
            ensure!(len(sub(to, from)) > 1e-12, "zero-length line in the profile");
            let dir = unit(sub(to, from));
            Carrier::Line {
                p: add(from, scale(left(dir), d)),
                dir,
            }
        }
        ProfileSegment::Arc { center, from, ccw, .. } => {
            // Left of a counter-clockwise arc is its center.
            let r = len(sub(from, center)) + if ccw { -d } else { d };
            ensure!(r > 1e-9, "offset collapses an arc of the profile");
            Carrier::Circle { c: center, r }
        }
        ProfileSegment::Circle { center, radius, ccw } => {
            let r = radius + if ccw { -d } else { d };
            ensure!(r > 1e-9, "offset collapses a circle of the profile");
            Carrier::Circle { c: center, r }
        }
        ProfileSegment::Bezier(_) => bail!("splines cannot be offset yet; use lines and arcs"),
    })
}

/// Unit tangent of a segment at its start (`at_end = false`) or end.
fn tangent(s: &ProfileSegment, at_end: bool) -> P2 {
    match *s {
        ProfileSegment::Line { from, to } => unit(sub(to, from)),
        ProfileSegment::Arc { center, from, to, ccw } => {
            let p = if at_end { to } else { from };
            let t = left(unit(sub(p, center)));
            if ccw {
                t
            } else {
                scale(t, -1.0)
            }
        }
        ProfileSegment::Circle { ccw, .. } => {
            if ccw {
                [0.0, 1.0]
            } else {
                [0.0, -1.0]
            }
        }
        ProfileSegment::Bezier(b) => {
            let d = if at_end { sub(b[3], b[2]) } else { sub(b[1], b[0]) };
            unit(d)
        }
    }
}

/// Where two offset carriers meet, nearest to `near`; `None` when they are parallel or miss.
fn meet(a: Carrier, b: Carrier, near: P2) -> Option<P2> {
    let pick = |pts: &[P2]| {
        pts.iter()
            .copied()
            .min_by(|p, q| len(sub(*p, near)).total_cmp(&len(sub(*q, near))))
    };
    match (a, b) {
        (Carrier::Line { p: p1, dir: d1 }, Carrier::Line { p: p2, dir: d2 }) => {
            let den = cross(d1, d2);
            if den.abs() < 1e-9 {
                return None;
            }
            let t = cross(sub(p2, p1), d2) / den;
            Some(add(p1, scale(d1, t)))
        }
        (Carrier::Line { p, dir }, Carrier::Circle { c, r }) | (Carrier::Circle { c, r }, Carrier::Line { p, dir }) => {
            let foot = add(p, scale(dir, dot(sub(c, p), dir)));
            let h2 = r * r - len(sub(foot, c)).powi(2);
            if h2 < -1e-12 {
                return None;
            }
            let h = h2.max(0.0).sqrt();
            pick(&[add(foot, scale(dir, h)), sub(foot, scale(dir, h))])
        }
        (Carrier::Circle { c: c1, r: r1 }, Carrier::Circle { c: c2, r: r2 }) => {
            let d = len(sub(c2, c1));
            if d < 1e-12 {
                return None;
            }
            let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let h2 = r1 * r1 - a * a;
            if h2 < -1e-12 {
                return None;
            }
            let u = unit(sub(c2, c1));
            let m = add(c1, scale(u, a));
            let h = h2.max(0.0).sqrt();
            pick(&[add(m, scale(left(u), h)), sub(m, scale(left(u), h))])
        }
    }
}

/// The segment rebuilt between new end points on its offset carrier.
fn rebuild(s: &ProfileSegment, from: P2, to: P2, d: f64) -> ProfileSegment {
    match *s {
        ProfileSegment::Line { .. } => ProfileSegment::Line { from, to },
        ProfileSegment::Arc { center, ccw, .. } => ProfileSegment::Arc { center, from, to, ccw },
        ProfileSegment::Circle { center, radius, ccw } => ProfileSegment::Circle {
            center,
            radius: radius + if ccw { -d } else { d },
            ccw,
        },
        ProfileSegment::Bezier(_) => unreachable!("rejected by carrier()"),
    }
}

/// Offsets a closed loop, each segment by its own distance (`dist.len() == lp.len()`).
pub fn offset_loop(lp: &[ProfileSegment], dist: &[f64]) -> Result<Vec<ProfileSegment>> {
    ensure!(lp.len() == dist.len(), "one offset distance per segment");
    if let [s @ ProfileSegment::Circle { .. }] = lp {
        carrier(s, dist[0])?;
        return Ok(vec![rebuild(s, [0.0; 2], [0.0; 2], dist[0])]);
    }
    let n = lp.len();
    let carriers: Vec<Carrier> = lp
        .iter()
        .zip(dist)
        .map(|(s, &d)| carrier(s, d))
        .collect::<Result<_>>()?;
    // corner[i] is where segment i-1 ends and segment i starts.
    let corners: Vec<P2> = (0..n)
        .map(|i| {
            let prev = (i + n - 1) % n;
            let v = seg_start(&lp[i]);
            meet(carriers[prev], carriers[i], v).unwrap_or_else(|| {
                // Tangent join: average the two offset points.
                let a = add(v, scale(left(tangent(&lp[prev], true)), dist[prev]));
                let b = add(v, scale(left(tangent(&lp[i], false)), dist[i]));
                scale(add(a, b), 0.5)
            })
        })
        .collect();
    let out: Vec<ProfileSegment> = (0..n)
        .map(|i| rebuild(&lp[i], corners[i], corners[(i + 1) % n], dist[i]))
        .collect();
    let (before, after) = (signed_area(lp), signed_area(&out));
    ensure!(
        before * after > 0.0 && after.abs() > 1e-9,
        "the offset is larger than the profile allows"
    );
    // A line that flips direction means its neighbours overtook it.
    for (a, b) in lp.iter().zip(&out) {
        if let (ProfileSegment::Line { from, to }, ProfileSegment::Line { from: f2, to: t2 }) = (a, b) {
            ensure!(
                dot(sub(*to, *from), sub(*t2, *f2)) > 0.0,
                "the offset is larger than the profile allows"
            );
        }
    }
    Ok(out)
}

/// Same distance for every segment of every loop of every region.
pub fn offset_regions(regions: &[ProfileRegion], d: f64) -> Result<Vec<ProfileRegion>> {
    regions
        .iter()
        .map(|r| {
            Ok(ProfileRegion {
                outer: offset_loop(&r.outer, &vec![d; r.outer.len()])?,
                holes: r
                    .holes
                    .iter()
                    .map(|h| offset_loop(h, &vec![d; h.len()]))
                    .collect::<Result<_>>()?,
            })
        })
        .collect()
}

/// Shoelace area of a loop (arcs and circles sampled), positive when counter-clockwise.
pub fn signed_area(lp: &[ProfileSegment]) -> f64 {
    let pts = sample_loop(lp, 64);
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(a, b)| cross(*a, *b))
        .sum::<f64>()
        / 2.0
}

/// Points along a loop, `per_turn` points per full turn of an arc.
pub fn sample_loop(lp: &[ProfileSegment], per_turn: usize) -> Vec<P2> {
    let mut out = Vec::new();
    for s in lp {
        match *s {
            ProfileSegment::Line { from, .. } => out.push(from),
            ProfileSegment::Arc { center, from, to, ccw } => {
                let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
                let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
                let mut sweep = (a1 - a0).rem_euclid(std::f64::consts::TAU);
                if !ccw {
                    sweep -= std::f64::consts::TAU;
                }
                let r = len(sub(from, center));
                let k = ((sweep.abs() / std::f64::consts::TAU * per_turn as f64).ceil() as usize).max(2);
                for j in 0..k {
                    let a = a0 + sweep * j as f64 / k as f64;
                    out.push([center[0] + r * a.cos(), center[1] + r * a.sin()]);
                }
            }
            ProfileSegment::Circle { center, radius, ccw } => {
                for j in 0..per_turn {
                    let a = std::f64::consts::TAU * j as f64 / per_turn as f64 * if ccw { 1.0 } else { -1.0 };
                    out.push([center[0] + radius * a.cos(), center[1] + radius * a.sin()]);
                }
            }
            ProfileSegment::Bezier(b) => {
                for j in 0..8 {
                    let t = j as f64 / 8.0;
                    let u = 1.0 - t;
                    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    out.push([
                        (0..4).map(|i| w[i] * b[i][0]).sum(),
                        (0..4).map(|i| w[i] * b[i][1]).sum(),
                    ]);
                }
            }
        }
    }
    out
}

fn reversed(s: &ProfileSegment) -> ProfileSegment {
    match *s {
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
    }
}

/// Offsets an open chain (segments joined end to start) by `d` to its left.
fn offset_chain(chain: &[ProfileSegment], d: f64) -> Result<Vec<ProfileSegment>> {
    let n = chain.len();
    let carriers: Vec<Carrier> = chain.iter().map(|s| carrier(s, d)).collect::<Result<_>>()?;
    let mut pts = vec![add(seg_start(&chain[0]), scale(left(tangent(&chain[0], false)), d))];
    for i in 1..n {
        let v = seg_start(&chain[i]);
        pts.push(
            meet(carriers[i - 1], carriers[i], v).unwrap_or_else(|| add(v, scale(left(tangent(&chain[i], false)), d))),
        );
    }
    pts.push(add(
        seg_end(&chain[n - 1]),
        scale(left(tangent(&chain[n - 1], true)), d),
    ));
    Ok((0..n).map(|i| rebuild(&chain[i], pts[i], pts[i + 1], d)).collect())
}

/// Thickens an open chain into a closed counter-clockwise region: `left` to one side, `right` to
/// the other (either may be zero, not both).
pub fn thicken_chain(chain: &[ProfileSegment], left_t: f64, right_t: f64) -> Result<ProfileRegion> {
    ensure!(!chain.is_empty(), "nothing to thicken");
    ensure!(left_t + right_t > 1e-9, "thickness must be positive");
    if chain.iter().any(|s| matches!(s, ProfileSegment::Circle { .. })) {
        bail!("a circle is closed; use a closed thin feature");
    }
    let a = offset_chain(chain, left_t)?;
    let b = offset_chain(chain, -right_t)?;
    // Right side forward, cap, left side backward, cap: counter-clockwise.
    let mut lp = b.clone();
    lp.push(ProfileSegment::Line {
        from: seg_end(b.last().unwrap()),
        to: seg_end(a.last().unwrap()),
    });
    lp.extend(a.iter().rev().map(reversed));
    lp.push(ProfileSegment::Line {
        from: seg_start(&a[0]),
        to: seg_start(&b[0]),
    });
    lp.retain(|s| !matches!(s, ProfileSegment::Line { from, to } if len(sub(*to, *from)) < 1e-12));
    if signed_area(&lp) < 0.0 {
        lp = lp.iter().rev().map(reversed).collect();
    }
    Ok(ProfileRegion {
        outer: lp,
        holes: Vec::new(),
    })
}

/// Thin wall along a closed region's loops: `outside` beyond each loop and `inside` into the material.
pub fn thin_region(r: &ProfileRegion, inside: f64, outside: f64) -> Result<Vec<ProfileRegion>> {
    ensure!(inside + outside > 1e-9, "thickness must be positive");
    let mut out = Vec::new();
    for lp in std::iter::once(&r.outer).chain(&r.holes) {
        let n = lp.len();
        let outer = offset_loop(lp, &vec![-outside; n])?;
        let inner = offset_loop(lp, &vec![inside; n])?;
        // The wall of a hole runs clockwise; flip both loops so the wall's outer loop is counter-clockwise.
        let (outer, inner) = if signed_area(lp) > 0.0 {
            (outer, inner)
        } else {
            (inner, outer)
        };
        let flip = |l: &[ProfileSegment]| l.iter().rev().map(reversed).collect::<Vec<_>>();
        let outer = if signed_area(&outer) < 0.0 { flip(&outer) } else { outer };
        let inner = if signed_area(&inner) > 0.0 { flip(&inner) } else { inner };
        out.push(ProfileRegion {
            outer,
            holes: vec![inner],
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Vec<ProfileSegment> {
        let p = [[0.0, 0.0], [s, 0.0], [s, s], [0.0, s]];
        (0..4)
            .map(|i| ProfileSegment::Line {
                from: p[i],
                to: p[(i + 1) % 4],
            })
            .collect()
    }

    #[test]
    fn square_shrinks_and_grows() {
        let inner = offset_loop(&square(10.0), &[1.0; 4]).unwrap();
        assert!((signed_area(&inner) - 64.0).abs() < 1e-9);
        assert!((seg_start(&inner[0])[0] - 1.0).abs() < 1e-12);
        let outer = offset_loop(&square(10.0), &[-1.0; 4]).unwrap();
        assert!((signed_area(&outer) - 144.0).abs() < 1e-9);
        assert!(offset_loop(&square(10.0), &[6.0; 4]).is_err(), "collapse is an error");
        // Per-segment distances: only the bottom edge moves.
        let one = offset_loop(&square(10.0), &[2.0, 0.0, 0.0, 0.0]).unwrap();
        assert!((signed_area(&one) - 80.0).abs() < 1e-9);
    }

    #[test]
    fn rounded_corner_and_circle_offset() {
        // Square with a ccw fillet arc at the top-right corner.
        let lp = vec![
            ProfileSegment::Line {
                from: [0.0, 0.0],
                to: [10.0, 0.0],
            },
            ProfileSegment::Line {
                from: [10.0, 0.0],
                to: [10.0, 8.0],
            },
            ProfileSegment::Arc {
                center: [8.0, 8.0],
                from: [10.0, 8.0],
                to: [8.0, 10.0],
                ccw: true,
            },
            ProfileSegment::Line {
                from: [8.0, 10.0],
                to: [0.0, 10.0],
            },
            ProfileSegment::Line {
                from: [0.0, 10.0],
                to: [0.0, 0.0],
            },
        ];
        let o = offset_loop(&lp, &[1.0; 5]).unwrap();
        let ProfileSegment::Arc { from, to, .. } = o[2] else {
            panic!()
        };
        assert!((len(sub(from, [8.0, 8.0])) - 1.0).abs() < 1e-9 && (len(sub(to, [8.0, 8.0])) - 1.0).abs() < 1e-9);
        let c = offset_loop(
            &[ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: 5.0,
                ccw: false,
            }],
            &[1.0],
        )
        .unwrap();
        assert!(matches!(c[0], ProfileSegment::Circle { radius, .. } if (radius - 6.0).abs() < 1e-12));
    }

    #[test]
    fn open_chain_thickens_into_a_closed_region() {
        let chain = vec![
            ProfileSegment::Line {
                from: [0.0, 0.0],
                to: [10.0, 0.0],
            },
            ProfileSegment::Line {
                from: [10.0, 0.0],
                to: [10.0, 10.0],
            },
        ];
        let r = thicken_chain(&chain, 1.0, 1.0).unwrap();
        // An L of width 2: 12*2 + 2*10 (the vertical leg minus the shared corner square) ... exact: 11*2 + 9*2 + 2*2.
        let a = signed_area(&r.outer);
        assert!((a - 40.0).abs() < 1e-9, "{a}");
        let wall = thin_region(
            &ProfileRegion {
                outer: square(10.0),
                holes: vec![],
            },
            1.0,
            0.0,
        )
        .unwrap();
        assert!((signed_area(&wall[0].outer) + signed_area(&wall[0].holes[0]) - 36.0).abs() < 1e-9);
    }
}
