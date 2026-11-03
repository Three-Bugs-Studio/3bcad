//! Lofts: a solid skinned through closed profiles on different planes, optionally bent to follow
//! guide curves and leaving the end profiles square (tangency).
//!
//! Profiles with the same pieces (segment for segment, same kinds) loft exactly; otherwise every
//! profile is refitted as the same number of cubic pieces, split by arc length. Each profile is
//! turned to start where the previous one starts and walked the same way round.

use anyhow::{bail, ensure, Result};
use truck_modeling::Solid;

use crate::operations::ProfileRegion;
use crate::skin::{self, Band, Caps, Ends, Piece};
use crate::solid::Plane;
use crate::topology::{cross, dot, norm, sub, unit, P3};

/// One loft profile: a region with a single closed loop, on its sketch plane.
#[derive(Clone, Debug)]
pub struct Section {
    pub region: ProfileRegion,
    pub plane: Plane,
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P3, s: f64) -> P3 {
    a.map(|v| v * s)
}

fn point(c: &[f64; 4]) -> P3 {
    [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
}

fn hom(p: P3, w: f64) -> [f64; 4] {
    [p[0] * w, p[1] * w, p[2] * w, w]
}

/// The piece walked backwards.
fn reversed(p: &Piece) -> Piece {
    let (k0, k1) = (p.knots[0], *p.knots.last().unwrap());
    Piece {
        cps: p.cps.iter().rev().copied().collect(),
        knots: p.knots.iter().rev().map(|k| k0 + k1 - k).collect(),
    }
}

fn samples(lp: &[Piece], per: usize) -> Vec<P3> {
    lp.iter()
        .flat_map(|p| {
            let (k0, k1) = (p.knots[0], *p.knots.last().unwrap());
            (0..per).map(move |i| p.eval(k0 + (k1 - k0) * i as f64 / per as f64))
        })
        .collect()
}

fn centroid(lp: &[Piece]) -> P3 {
    let s = samples(lp, 16);
    scale(s.iter().fold([0.0; 3], |a, p| add(a, *p)), 1.0 / s.len() as f64)
}

/// Newell normal of a loop (its area vector, unnormalized).
fn area_normal(lp: &[Piece]) -> P3 {
    let s = samples(lp, 16);
    let n = s.len();
    let mut a = [0.0; 3];
    for i in 0..n {
        let (p, q) = (s[i], s[(i + 1) % n]);
        a = add(a, cross(p, q));
    }
    scale(a, 0.5)
}

/// The loop split into `m` cubic pieces of equal length.
fn resampled(lp: &[Piece], m: usize) -> Vec<Piece> {
    let dense = samples(lp, 64);
    let n = dense.len();
    let mut cum = vec![0.0];
    for i in 0..n {
        let d = norm(sub(dense[(i + 1) % n], dense[i]));
        cum.push(cum[i] + d);
    }
    let total = cum[n];
    let at = |s: f64| -> P3 {
        let s = s.rem_euclid(total);
        let i = cum.partition_point(|&c| c <= s).saturating_sub(1).min(n - 1);
        let f = (s - cum[i]) / (cum[i + 1] - cum[i]).max(1e-300);
        add(scale(dense[i], 1.0 - f), scale(dense[(i + 1) % n], f))
    };
    (0..m)
        .map(|k| {
            let (s0, s1) = (total * k as f64 / m as f64, total * (k + 1) as f64 / m as f64);
            skin::fit_cubic(&|t| at(s0 + (s1 - s0) * t), 6)
        })
        .collect()
}

fn same_structure(a: &[Piece], b: &[Piece]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p.cps.len() == q.cps.len() && p.knots == q.knots)
}

/// Where a polyline crosses a plane, nearest to `near`.
fn cross_plane(guide: &[P3], o: P3, n: P3, near: P3) -> Option<P3> {
    let mut best: Option<(f64, P3)> = None;
    for w in guide.windows(2) {
        let (da, db) = (dot(sub(w[0], o), n), dot(sub(w[1], o), n));
        if da * db > 0.0 || (da - db).abs() < 1e-15 {
            continue;
        }
        let t = da / (da - db);
        let p = add(scale(w[0], 1.0 - t), scale(w[1], t));
        let d = norm(sub(p, near));
        if best.is_none_or(|b| d < b.0) {
            best = Some((d, p));
        }
    }
    best.map(|b| b.1)
}

/// Moves a section in its plane so its guide points land on the guides: a linear map about the
/// centroid for one or two guides, a least-squares affine map for more.
fn fit_to_guides(sec: &mut [Piece], c: P3, n: P3, from: &[P3], to: &[P3]) {
    let u = unit(if n[0].abs() < 0.9 {
        cross(n, [1.0, 0.0, 0.0])
    } else {
        cross(n, [0.0, 1.0, 0.0])
    });
    let v = cross(n, u);
    let loc = |p: P3| {
        let d = sub(p, c);
        [dot(d, u), dot(d, v)]
    };
    let ps: Vec<[f64; 2]> = from.iter().map(|&p| loc(p)).collect();
    let gs: Vec<[f64; 2]> = to.iter().map(|&p| loc(p)).collect();
    // m = [[a, b], [c, d]] and translation t.
    let (m, t) = match ps.len() {
        1 => {
            let (p, g) = (ps[0], gs[0]);
            let l2 = p[0] * p[0] + p[1] * p[1];
            if l2 < 1e-18 {
                ([[1.0, 0.0], [0.0, 1.0]], [g[0] - p[0], g[1] - p[1]])
            } else {
                let d = [g[0] - p[0], g[1] - p[1]];
                (
                    [
                        [1.0 + d[0] * p[0] / l2, d[0] * p[1] / l2],
                        [d[1] * p[0] / l2, 1.0 + d[1] * p[1] / l2],
                    ],
                    [0.0, 0.0],
                )
            }
        }
        2 => {
            let det = ps[0][0] * ps[1][1] - ps[1][0] * ps[0][1];
            if det.abs() < 1e-12 {
                return fit_to_guides(sec, c, n, &from[..1], &to[..1]);
            }
            // m · [p0 p1] = [g0 g1].
            let inv = [[ps[1][1] / det, -ps[1][0] / det], [-ps[0][1] / det, ps[0][0] / det]];
            let mm: [[f64; 2]; 2] =
                std::array::from_fn(|r| std::array::from_fn(|k| gs[0][r] * inv[0][k] + gs[1][r] * inv[1][k]));
            (mm, [0.0, 0.0])
        }
        _ => {
            // Least squares on [x y 1] for each output coordinate.
            let mut ata = nalgebra::Matrix3::<f64>::zeros();
            let mut atb = [nalgebra::Vector3::<f64>::zeros(); 2];
            for (p, g) in ps.iter().zip(&gs) {
                let r = nalgebra::Vector3::new(p[0], p[1], 1.0);
                ata += r * r.transpose();
                for k in 0..2 {
                    atb[k] += r * g[k];
                }
            }
            let Some(inv) = ata.try_inverse() else {
                return fit_to_guides(sec, c, n, &from[..2], &to[..2]);
            };
            let (x, y) = (inv * atb[0], inv * atb[1]);
            ([[x[0], x[1]], [y[0], y[1]]], [x[2], y[2]])
        }
    };
    for pc in sec.iter_mut() {
        for cp in pc.cps.iter_mut() {
            let p = point(cp);
            let d = sub(p, c);
            let (x, y, z) = (dot(d, u), dot(d, v), dot(d, n));
            let (nx, ny) = (m[0][0] * x + m[0][1] * y + t[0], m[1][0] * x + m[1][1] * y + t[1]);
            let q = add(c, add(add(scale(u, nx), scale(v, ny)), scale(n, z)));
            *cp = hom(q, cp[3]);
        }
    }
}

/// Lofts through `sections` in order. `guides` are polylines from the first profile to the last;
/// `start_weight` / `end_weight` (0 = free) make the loft leave the end profiles square to their
/// planes, the weight scaling how far the tangency reaches.
pub fn loft(sections: &[Section], guides: &[Vec<P3>], start_weight: f64, end_weight: f64) -> Result<Solid> {
    ensure!(sections.len() >= 2, "a loft needs two profiles or more");
    let mut loops: Vec<Vec<Piece>> = Vec::new();
    for s in sections {
        ensure!(
            s.region.holes.is_empty(),
            "loft profiles must be single loops without holes"
        );
        let m = s.plane.matrix();
        loops.push(
            skin::loop_pieces(&s.region.outer)
                .0
                .iter()
                .map(|p| p.transformed(&m))
                .collect(),
        );
    }
    let centers: Vec<P3> = loops.iter().map(|l| centroid(l)).collect();
    for k in 1..centers.len() {
        ensure!(
            norm(sub(centers[k], centers[k - 1])) > 1e-6,
            "two profiles of the loft are in the same place"
        );
    }
    // Every loop the same way round, along the loft.
    for k in 0..loops.len() {
        let along = if k + 1 < loops.len() {
            sub(centers[k + 1], centers[k])
        } else {
            sub(centers[k], centers[k - 1])
        };
        if dot(area_normal(&loops[k]), along) < 0.0 {
            loops[k] = loops[k].iter().rev().map(reversed).collect();
        }
    }
    // Same structure everywhere.
    let exact = loops.windows(2).all(|w| same_structure(&w[0], &w[1]));
    if !exact {
        let counts: Vec<usize> = loops.iter().map(Vec::len).collect();
        if counts.iter().all(|&c| c == counts[0]) {
            loops = loops
                .iter()
                .map(|l| l.iter().map(|p| skin::cubic(p, 4)).collect())
                .collect();
        } else {
            let m = counts.iter().copied().max().unwrap().max(4);
            loops = loops.iter().map(|l| resampled(l, m)).collect();
        }
    }
    // Start each loop where the previous one starts.
    for k in 1..loops.len() {
        let n = loops[k].len();
        let (ck, cp) = (centers[k], centers[k - 1]);
        let cost = |s: usize| -> f64 {
            (0..n)
                .map(|i| {
                    let a = sub(loops[k][(i + s) % n].start(), ck);
                    let b = sub(loops[k - 1][i].start(), cp);
                    norm(sub(a, b)).powi(2)
                })
                .sum()
        };
        let best = (0..n).min_by(|&a, &b| cost(a).total_cmp(&cost(b))).unwrap();
        // Only shift between pieces of the same kind.
        if best != 0 && (0..n).all(|i| loops[k][(i + best) % n].knots == loops[k][i].knots) {
            loops[k].rotate_left(best);
        }
    }
    let normals: Vec<P3> = sections.iter().map(|s| s.plane.normal()).collect();
    let mut secs: Vec<Vec<Piece>> = loops.clone();
    let mut planes: Vec<(P3, P3)> = centers.iter().copied().zip(normals.iter().copied()).collect();
    if !guides.is_empty() {
        // Which piece start each guide runs through on the first profile.
        let first = &loops[0];
        let picks: Vec<usize> = guides
            .iter()
            .map(|g| {
                (0..first.len())
                    .min_by(|&a, &b| norm(sub(first[a].start(), g[0])).total_cmp(&norm(sub(first[b].start(), g[0]))))
                    .unwrap()
            })
            .collect();
        let mut out: Vec<Vec<Piece>> = Vec::new();
        let mut out_planes = Vec::new();
        for k in 0..loops.len() {
            out.push(loops[k].clone());
            out_planes.push(planes[k]);
            if k + 1 == loops.len() {
                break;
            }
            for f in [0.25, 0.5, 0.75] {
                let mut sec: Vec<Piece> = loops[k]
                    .iter()
                    .zip(&loops[k + 1])
                    .map(|(a, b)| Piece {
                        cps: a
                            .cps
                            .iter()
                            .zip(&b.cps)
                            .map(|(x, y)| std::array::from_fn(|d| x[d] * (1.0 - f) + y[d] * f))
                            .collect(),
                        knots: a.knots.clone(),
                    })
                    .collect();
                let c = add(scale(centers[k], 1.0 - f), scale(centers[k + 1], f));
                let mut n = add(scale(normals[k], 1.0 - f), scale(normals[k + 1], f));
                if norm(n) < 1e-9 {
                    n = normals[k];
                }
                let n = unit(n);
                let from: Vec<P3> = picks.iter().map(|&i| sec[i].start()).collect();
                let mut to = Vec::new();
                for (g, &p) in guides.iter().zip(&from) {
                    match cross_plane(g, c, n, p) {
                        Some(q) => to.push(q),
                        None => bail!("a guide curve does not run from the first profile to the last"),
                    }
                }
                let cc = centroid(&sec);
                fit_to_guides(&mut sec, cc, n, &from, &to);
                out.push(sec);
                out_planes.push((c, n));
            }
        }
        secs = out;
        planes = out_planes;
    }
    let mut params = vec![0.0];
    for k in 1..secs.len() {
        let d = norm(sub(centroid(&secs[k]), centroid(&secs[k - 1])));
        params.push(params[k - 1] + d.max(1e-6));
    }
    let along = sub(centroid(&secs[secs.len() - 1]), centroid(&secs[0]));
    let ders = |sec: &[Piece], n: P3, w: f64| -> Vec<[f64; 3]> {
        let n = if dot(n, along) < 0.0 { scale(n, -1.0) } else { n };
        sec.iter()
            .flat_map(|p| p.cps.iter().map(move |_| scale(n, w)))
            .collect()
    };
    let ends = Ends {
        start: (start_weight > 0.0).then(|| ders(&secs[0], planes[0].1, start_weight)),
        end: (end_weight > 0.0).then(|| ders(&secs[secs.len() - 1], planes[planes.len() - 1].1, end_weight)),
    };
    let band = Band {
        sections: secs.into_iter().map(|s| vec![s]).collect(),
        params,
        ends,
    };
    skin::skin(&[band], Caps::Planar)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::boolean::volume;
    use crate::mesher::print_mesh;
    use crate::tool::{disc, polygon};

    fn at(z: f64) -> Plane {
        Plane {
            origin: [0.0, 0.0, z],
            ..Plane::TOP
        }
    }

    #[test]
    fn a_loft_between_equal_squares_is_a_box() {
        let sq = polygon(&[[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]).unwrap();
        let s = loft(
            &[
                Section {
                    region: sq.clone(),
                    plane: at(0.0),
                },
                Section {
                    region: sq,
                    plane: at(10.0),
                },
            ],
            &[],
            0.0,
            0.0,
        )
        .unwrap();
        let v = volume(&s).unwrap();
        assert!((v - 1000.0).abs() < 1e-3, "{v}");
    }

    #[test]
    fn a_loft_between_circles_is_a_frustum() {
        let s = loft(
            &[
                Section {
                    region: disc([0.0, 0.0], 5.0),
                    plane: at(0.0),
                },
                Section {
                    region: disc([0.0, 0.0], 3.0),
                    plane: at(10.0),
                },
            ],
            &[],
            0.0,
            0.0,
        )
        .unwrap();
        let v = volume(&s).unwrap();
        let expected = PI * 10.0 / 3.0 * (25.0 + 15.0 + 9.0);
        assert!((v - expected).abs() / expected < 2e-3, "{v} vs {expected}");
    }

    #[test]
    fn a_square_lofts_into_a_circle_and_through_three_profiles() {
        let sq = polygon(&[[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]).unwrap();
        let s = loft(
            &[
                Section {
                    region: sq.clone(),
                    plane: at(0.0),
                },
                Section {
                    region: disc([0.0, 0.0], 4.0),
                    plane: at(10.0),
                },
                Section {
                    region: sq,
                    plane: at(20.0),
                },
            ],
            &[],
            0.0,
            0.0,
        )
        .unwrap();
        let v = volume(&s).unwrap();
        // Between a 10x10 square (100) and a circle of r 4 (50.3) the area stays in that range.
        assert!(v > 20.0 * 50.0 && v < 20.0 * 100.0, "{v}");
        assert!(!print_mesh(&s, 0.01).triangles.is_empty());
    }

    #[test]
    fn tangency_bulges_the_loft() {
        let tilted = Plane {
            origin: [0.0, 0.0, 10.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, (0.3f64).cos(), (0.3f64).sin()],
        };
        let mk = |w: f64| {
            loft(
                &[
                    Section {
                        region: disc([0.0, 0.0], 5.0),
                        plane: at(0.0),
                    },
                    Section {
                        region: disc([0.0, 0.0], 5.0),
                        plane: tilted,
                    },
                ],
                &[],
                w,
                w,
            )
            .map(|s| volume(&s).unwrap())
        };
        let (free, square) = (mk(0.0).unwrap(), mk(1.0).unwrap());
        assert!(free > 0.0 && square > 0.0);
        assert!((free - square).abs() > 1e-3, "{free} {square}");
    }

    #[test]
    fn a_guide_curve_pulls_the_middle_out() {
        let sq = polygon(&[[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]).unwrap();
        // A guide from corner (5, 5) bowing out to (8, 8) at mid-height.
        let guide: Vec<P3> = (0..=20)
            .map(|i| {
                let t = i as f64 / 20.0;
                let bow = 3.0 * (PI * t).sin();
                [5.0 + bow, 5.0 + bow, 10.0 * t]
            })
            .collect();
        let plain = loft(
            &[
                Section {
                    region: sq.clone(),
                    plane: at(0.0),
                },
                Section {
                    region: sq.clone(),
                    plane: at(10.0),
                },
            ],
            &[],
            0.0,
            0.0,
        )
        .unwrap();
        let guided = loft(
            &[
                Section {
                    region: sq.clone(),
                    plane: at(0.0),
                },
                Section {
                    region: sq,
                    plane: at(10.0),
                },
            ],
            &[guide],
            0.0,
            0.0,
        )
        .unwrap();
        let (_, hi) = crate::boolean::bbox(&guided);
        assert!(hi[0] > 6.5, "{hi:?}");
        assert!(volume(&guided).unwrap() > volume(&plain).unwrap() + 50.0);
    }
}
