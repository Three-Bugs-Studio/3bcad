//! Small 2D geometry helpers shared by the tools, profiles and inferencing.

use std::f64::consts::TAU;

pub type P2 = [f64; 2];

pub fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn scale(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

pub fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}

pub fn dist(a: P2, b: P2) -> f64 {
    len(sub(a, b))
}

pub fn normalize(a: P2) -> P2 {
    let l = len(a);
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        [1.0, 0.0]
    }
}

pub fn lerp(a: P2, b: P2, t: f64) -> P2 {
    add(a, scale(sub(b, a), t))
}

/// `a` rotated +90°.
pub fn perp(a: P2) -> P2 {
    [-a[1], a[0]]
}

pub fn rotate(a: P2, angle: f64) -> P2 {
    let (s, c) = angle.sin_cos();
    [a[0] * c - a[1] * s, a[0] * s + a[1] * c]
}

pub fn angle_of(a: P2) -> f64 {
    a[1].atan2(a[0])
}

/// Counter-clockwise angle from `from` to `to`, in `(0, TAU]`.
pub fn ccw_sweep(from: f64, to: f64) -> f64 {
    let s = (to - from).rem_euclid(TAU);
    if s <= 1e-12 {
        TAU
    } else {
        s
    }
}

/// Whether `p` lies (angularly) on the counter-clockwise arc from `start` to `end` around `center`.
pub fn on_arc(center: P2, start: P2, end: P2, p: P2) -> bool {
    let a0 = angle_of(sub(start, center));
    let sweep = ccw_sweep(a0, angle_of(sub(end, center)));
    (angle_of(sub(p, center)) - a0).rem_euclid(TAU) <= sweep + 1e-9
}

/// Circle through three points; `None` if they are collinear.
pub fn circle_through(a: P2, b: P2, c: P2) -> Option<(P2, f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-12 * (1.0 + dot(a, a) + dot(b, b) + dot(c, c)) {
        return None;
    }
    let (a2, b2, c2) = (dot(a, a), dot(b, b), dot(c, c));
    let center = [
        (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d,
        (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d,
    ];
    Some((center, dist(center, a)))
}

/// Intersection of the infinite lines a1-b1 and a2-b2 as parameters `(t, u)` along each.
pub fn line_line(a1: P2, b1: P2, a2: P2, b2: P2) -> Option<(f64, f64)> {
    let (r, s) = (sub(b1, a1), sub(b2, a2));
    let denom = cross(r, s);
    if denom.abs() < 1e-12 * len(r) * len(s) {
        return None;
    }
    let q = sub(a2, a1);
    Some((cross(q, s) / denom, cross(q, r) / denom))
}

/// Parameters `t` along the infinite line a-b where it meets the circle.
pub fn line_circle(a: P2, b: P2, c: P2, r: f64) -> Vec<f64> {
    let d = sub(b, a);
    let f = sub(a, c);
    let (qa, qb, qc) = (dot(d, d), 2.0 * dot(f, d), dot(f, f) - r * r);
    let disc = qb * qb - 4.0 * qa * qc;
    if qa == 0.0 || disc < 0.0 {
        return Vec::new();
    }
    let s = disc.sqrt();
    let mut ts = vec![(-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa)];
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
    ts
}

pub fn circle_circle(c1: P2, r1: f64, c2: P2, r2: f64) -> Vec<P2> {
    let d = dist(c1, c2);
    if d < 1e-12 || d > r1 + r2 + 1e-12 || d < (r1 - r2).abs() - 1e-12 {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let u = scale(sub(c2, c1), 1.0 / d);
    let m = add(c1, scale(u, a));
    if h < 1e-12 {
        return vec![m];
    }
    vec![add(m, scale(perp(u), h)), sub(m, scale(perp(u), h))]
}

/// Closest point on segment a-b to `p`, with its parameter in `[0, 1]`.
pub fn closest_on_segment(a: P2, b: P2, p: P2) -> (P2, f64) {
    let d = sub(b, a);
    let l2 = dot(d, d);
    let t = if l2 > 0.0 {
        (dot(sub(p, a), d) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (lerp(a, b, t), t)
}

pub fn dist_to_polyline(poly: &[P2], p: P2) -> f64 {
    poly.windows(2)
        .map(|w| dist(closest_on_segment(w[0], w[1], p).0, p))
        .fold(f64::INFINITY, f64::min)
}

/// Cubic Bézier spans of a Catmull-Rom spline through `points` (closed when `closed`).
pub fn catmull_rom(points: &[P2], closed: bool) -> Vec<[P2; 4]> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let at = |i: isize| -> P2 {
        if closed {
            points[i.rem_euclid(n as isize) as usize]
        } else {
            points[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let tangent = |i: isize| scale(sub(at(i + 1), at(i - 1)), 0.5);
    let spans = if closed { n } else { n - 1 };
    (0..spans as isize)
        .map(|i| {
            let (p0, p1) = (at(i), at(i + 1));
            [
                p0,
                add(p0, scale(tangent(i), 1.0 / 3.0)),
                sub(p1, scale(tangent(i + 1), 1.0 / 3.0)),
                p1,
            ]
        })
        .collect()
}

pub fn bezier_point(b: &[P2; 4], t: f64) -> P2 {
    let s = 1.0 - t;
    let (w0, w1, w2, w3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
    [
        w0 * b[0][0] + w1 * b[1][0] + w2 * b[2][0] + w3 * b[3][0],
        w0 * b[0][1] + w1 * b[1][1] + w2 * b[2][1] + w3 * b[3][1],
    ]
}

/// `n + 1` points along a counter-clockwise arc starting at angle `a0`.
pub fn arc_points(center: P2, r: f64, a0: f64, sweep: f64, n: usize) -> Vec<P2> {
    (0..=n)
        .map(|i| {
            let a = a0 + sweep * i as f64 / n as f64;
            add(center, [r * a.cos(), r * a.sin()])
        })
        .collect()
}

/// Signed area: positive for counter-clockwise polygons.
pub fn polygon_area(poly: &[P2]) -> f64 {
    poly.iter()
        .zip(poly.iter().cycle().skip(1))
        .map(|(a, b)| cross(*a, *b))
        .sum::<f64>()
        / 2.0
}

pub fn point_in_polygon(p: P2, poly: &[P2]) -> bool {
    let mut inside = false;
    for (a, b) in poly.iter().zip(poly.iter().cycle().skip(1)) {
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}

/// Whether segments a-b and c-d cross at a point interior to both.
pub fn segments_cross(a: P2, b: P2, c: P2, d: P2) -> bool {
    matches!(line_line(a, b, c, d), Some((t, u)) if t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: P2, b: P2) -> bool {
        dist(a, b) < 1e-9
    }

    #[test]
    fn circle_through_three_points() {
        let (c, r) = circle_through([1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]).unwrap();
        assert!(near(c, [0.0, 0.0]) && (r - 1.0).abs() < 1e-12);
        assert!(circle_through([0.0, 0.0], [1.0, 1.0], [2.0, 2.0]).is_none());
    }

    #[test]
    fn intersections() {
        let (t, u) = line_line([0.0, 0.0], [2.0, 0.0], [1.0, -1.0], [1.0, 1.0]).unwrap();
        assert!((t - 0.5).abs() < 1e-12 && (u - 0.5).abs() < 1e-12);
        assert!(line_line([0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]).is_none());
        let ts = line_circle([-2.0, 0.0], [2.0, 0.0], [0.0, 0.0], 1.0);
        assert_eq!(ts.len(), 2);
        assert!((ts[0] - 0.25).abs() < 1e-12 && (ts[1] - 0.75).abs() < 1e-12);
        let ps = circle_circle([0.0, 0.0], 1.0, [1.0, 0.0], 1.0);
        assert_eq!(ps.len(), 2);
        assert!(ps.iter().all(|p| (dist(*p, [0.0, 0.0]) - 1.0).abs() < 1e-12));
    }

    #[test]
    fn arcs_and_polygons() {
        assert!(on_arc([0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.7, 0.7]));
        assert!(!on_arc([0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [-0.7, -0.7]));
        let square = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        assert!((polygon_area(&square) - 4.0).abs() < 1e-12);
        assert!(point_in_polygon([1.0, 1.0], &square) && !point_in_polygon([3.0, 1.0], &square));
        assert!(segments_cross([0.0, 0.0], [2.0, 2.0], [0.0, 2.0], [2.0, 0.0]));
        assert!(!segments_cross([0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [1.0, 1.0]));
    }

    #[test]
    fn catmull_rom_passes_through_points() {
        let pts = [[0.0, 0.0], [1.0, 2.0], [3.0, 1.0], [4.0, 3.0]];
        let spans = catmull_rom(&pts, false);
        assert_eq!(spans.len(), 3);
        for (i, s) in spans.iter().enumerate() {
            assert!(near(bezier_point(s, 0.0), pts[i]) && near(bezier_point(s, 1.0), pts[i + 1]));
        }
        assert_eq!(catmull_rom(&pts, true).len(), 4);
    }
}
