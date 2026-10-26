//! Inferencing while drawing: snapping to existing geometry and the relations that come with it.

use std::f64::consts::PI;

use crate::geom::{self, P2};
use crate::{Constraint, EntityId, Geometry, PointId, Sketch};

/// What the cursor snapped to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Snap {
    Free(P2),
    /// An existing point (endpoint, center, sketch point): the new geometry shares it.
    Point(PointId),
    /// The middle of a line: adds a Midpoint relation.
    Midpoint(EntityId),
    /// Anywhere on a line, circle or arc: adds a Coincident (point-on-curve) relation.
    OnCurve(EntityId, P2),
}

impl Snap {
    pub fn pos(&self, sk: &Sketch) -> P2 {
        match *self {
            Snap::Free(p) | Snap::OnCurve(_, p) => p,
            Snap::Point(id) => sk.point(id),
            Snap::Midpoint(e) => {
                let (a, b) = sk.entity(e).endpoints().expect("line");
                geom::lerp(sk.point(a), sk.point(b), 0.5)
            }
        }
    }

    /// The point to draw from/to: the snapped point itself, or a new one with its relation.
    pub fn place(&self, sk: &mut Sketch) -> PointId {
        let p = self.pos(sk);
        let new = |sk: &mut Sketch| sk.add_point(p[0], p[1]);
        match *self {
            Snap::Point(id) => id,
            Snap::Free(_) => new(sk),
            Snap::Midpoint(e) => {
                let q = new(sk);
                sk.constrain(Constraint::Midpoint(q, e));
                q
            }
            Snap::OnCurve(e, _) => {
                let q = new(sk);
                sk.constrain(Constraint::PointOnCurve(q, e));
                q
            }
        }
    }
}

/// What the cursor snaps to within `tol` (sketch units): points first, then line midpoints, then curves.
pub fn snap(sk: &Sketch, cursor: P2, tol: f64, ignore: &[PointId]) -> Snap {
    let nearest_point = sk
        .point_ids()
        .filter(|p| !ignore.contains(p))
        .map(|p| (geom::dist(sk.point(p), cursor), p))
        .filter(|(d, _)| *d <= tol)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, p)) = nearest_point {
        return Snap::Point(p);
    }
    // Curves that only connect ignored points (the one being drawn) are skipped too.
    let usable = |e: EntityId| !sk.entity(e).points().iter().all(|p| ignore.contains(p));
    for e in sk.entity_ids().filter(|&e| usable(e)) {
        if let Some((a, b)) = sk.entity(e).endpoints().filter(|_| sk.entity(e).is_line()) {
            if geom::dist(geom::lerp(sk.point(a), sk.point(b), 0.5), cursor) <= tol {
                return Snap::Midpoint(e);
            }
        }
    }
    let on_curve = sk
        .entity_ids()
        .filter(|&e| usable(e) && !matches!(sk.entity(e).geom, Geometry::Spline { .. }))
        .filter_map(|e| {
            let q = closest_on_entity(sk, e, cursor);
            let d = geom::dist(q, cursor);
            (d <= tol).then_some((d, e, q))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match on_curve {
        Some((_, e, q)) => Snap::OnCurve(e, q),
        None => Snap::Free(cursor),
    }
}

/// Closest point on a line segment, circle or arc.
pub fn closest_on_entity(sk: &Sketch, e: EntityId, p: P2) -> P2 {
    match sk.entity(e).geom {
        Geometry::Line { a, b } => geom::closest_on_segment(sk.point(a), sk.point(b), p).0,
        Geometry::Circle { center, .. } | Geometry::Arc { center, .. } => {
            let c = sk.point(center);
            let on_circle = geom::add(c, geom::scale(geom::normalize(geom::sub(p, c)), sk.radius(e)));
            match sk.entity(e).geom {
                Geometry::Arc { start, end, .. } if !geom::on_arc(c, sk.point(start), sk.point(end), on_circle) => {
                    let (s, t) = (sk.point(start), sk.point(end));
                    if geom::dist(s, p) < geom::dist(t, p) {
                        s
                    } else {
                        t
                    }
                }
                _ => on_circle,
            }
        }
        Geometry::Spline { .. } => {
            let poly = sk.polyline(e);
            poly.windows(2)
                .map(|w| geom::closest_on_segment(w[0], w[1], p).0)
                .min_by(|x, y| geom::dist(*x, p).total_cmp(&geom::dist(*y, p)))
                .unwrap_or(p)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Horizontal,
    Vertical,
}

/// Horizontal/vertical when the segment `from`-`to` is within `max_angle` radians of it.
pub fn align(from: P2, to: P2, max_angle: f64) -> Option<Align> {
    let d = geom::sub(to, from);
    if geom::len(d) < 1e-12 {
        return None;
    }
    let a = d[1].atan2(d[0]).abs();
    if a < max_angle || PI - a < max_angle {
        Some(Align::Horizontal)
    } else if (a - PI / 2.0).abs() < max_angle {
        Some(Align::Vertical)
    } else {
        None
    }
}

/// Existing points the cursor lines up with: (same y, same x) within `tol`, nearest first.
pub fn aligned_with(sk: &Sketch, cursor: P2, tol: f64, ignore: &[PointId]) -> (Option<PointId>, Option<PointId>) {
    let best = |axis: usize| {
        sk.point_ids()
            .filter(|p| !ignore.contains(p))
            .filter(|&p| (sk.point(p)[axis] - cursor[axis]).abs() <= tol)
            .min_by(|&a, &b| geom::dist(sk.point(a), cursor).total_cmp(&geom::dist(sk.point(b), cursor)))
    };
    (best(1), best(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_in_priority_order() {
        let mut s = Sketch::default();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(10.0, 0.0);
        let l = s.add_line(a, b);
        assert_eq!(snap(&s, [0.2, 0.1], 0.5, &[]), Snap::Point(a));
        assert_eq!(snap(&s, [0.2, 0.1], 0.5, &[a]), Snap::OnCurve(l, [0.2, 0.0]));
        assert_eq!(snap(&s, [5.1, 0.2], 0.5, &[]), Snap::Midpoint(l));
        assert_eq!(snap(&s, [3.0, 0.3], 0.5, &[]), Snap::OnCurve(l, [3.0, 0.0]));
        assert_eq!(snap(&s, [3.0, 3.0], 0.5, &[]), Snap::Free([3.0, 3.0]));
        // Placing a midpoint snap adds the relation and keeps it satisfied.
        let q = Snap::Midpoint(l).place(&mut s);
        s.constrain(Constraint::Fix(a, [0.0, 0.0]));
        s.drag(&[(b, [20.0, 0.0])]);
        assert!((s.point(q)[0] - 10.0).abs() < 1e-6);
    }

    #[test]
    fn alignment() {
        assert_eq!(
            align([0.0, 0.0], [10.0, 0.3], 3f64.to_radians()),
            Some(Align::Horizontal)
        );
        assert_eq!(
            align([0.0, 0.0], [-10.0, 0.3], 3f64.to_radians()),
            Some(Align::Horizontal)
        );
        assert_eq!(align([0.0, 0.0], [0.2, -8.0], 3f64.to_radians()), Some(Align::Vertical));
        assert_eq!(align([0.0, 0.0], [5.0, 5.0], 3f64.to_radians()), None);
        let mut s = Sketch::default();
        let p = s.add_point(4.0, 7.0);
        assert_eq!(aligned_with(&s, [9.0, 7.1], 0.2, &[]), (Some(p), None));
        assert_eq!(aligned_with(&s, [4.1, -3.0], 0.2, &[]), (None, Some(p)));
    }

    #[test]
    fn closest_on_arc_clamps_to_its_ends() {
        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let st = s.add_point(1.0, 0.0);
        let en = s.add_point(0.0, 1.0);
        let arc = s.add_arc(c, st, en);
        let q = closest_on_entity(&s, arc, [0.5, 0.5]);
        assert!((geom::len(q) - 1.0).abs() < 1e-12);
        assert_eq!(closest_on_entity(&s, arc, [1.0, -1.0]), [1.0, 0.0]);
    }
}
