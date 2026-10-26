//! Sketch tools: each creates geometry plus the relations SolidWorks would add, so the result
//! keeps its shape when dimensions change.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use crate::geom::{self, P2};
use crate::profile::Segment;
use crate::{Constraint, EntityId, Geometry, PointId, Sketch};

use Constraint::*;

type Result<T> = std::result::Result<T, String>;

const EPS: f64 = 1e-9;

/// Something picked in a sketch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Point(PointId),
    Entity(EntityId),
    /// A relation or dimension (index into the constraints).
    Constraint(usize),
}

impl Sketch {
    fn new_point(&mut self, p: P2) -> PointId {
        self.add_point(p[0], p[1])
    }

    fn close_polygon(&mut self, pts: &[PointId]) -> Vec<EntityId> {
        (0..pts.len())
            .map(|i| self.add_line(pts[i], pts[(i + 1) % pts.len()]))
            .collect()
    }

    fn try_constrain(&mut self, c: Constraint) -> Option<usize> {
        self.accepts(&c).then(|| self.constrain(c))
    }

    /// Corner rectangle from two opposite corners, with horizontal/vertical sides.
    pub fn add_rectangle(&mut self, p0: PointId, p2: PointId) -> Option<[EntityId; 4]> {
        let (a, c) = (self.point(p0), self.point(p2));
        if (a[0] - c[0]).abs() < EPS || (a[1] - c[1]).abs() < EPS {
            return None;
        }
        let p1 = self.new_point([c[0], a[1]]);
        let p3 = self.new_point([a[0], c[1]]);
        let l = self.close_polygon(&[p0, p1, p2, p3]);
        self.constrain(Horizontal(l[0]));
        self.constrain(Vertical(l[1]));
        self.constrain(Horizontal(l[2]));
        self.constrain(Vertical(l[3]));
        Some([l[0], l[1], l[2], l[3]])
    }

    /// Center rectangle: construction diagonals with the center at their midpoint.
    pub fn add_center_rectangle(&mut self, center: PointId, corner: PointId) -> Option<[EntityId; 4]> {
        let (c, k) = (self.point(center), self.point(corner));
        let p0 = self.new_point(geom::sub(geom::scale(c, 2.0), k));
        let Some(lines) = self.add_rectangle(p0, corner) else {
            self.remove(&[p0], &[], &[]);
            return None;
        };
        let n = self.point_count();
        let (p1, p3) = (PointId(n - 2), PointId(n - 1));
        let d0 = self.add_line(p0, corner);
        let d1 = self.add_line(p1, p3);
        self.set_construction(d0, true);
        self.set_construction(d1, true);
        self.constrain(Midpoint(center, d0));
        Some(lines)
    }

    /// Rectangle at any angle: `a`-`b` is one side, `c` sets the other side's length.
    pub fn add_rectangle_3pt(&mut self, a: PointId, b: PointId, c: P2) -> Option<[EntityId; 4]> {
        let (pa, pb) = (self.point(a), self.point(b));
        if geom::dist(pa, pb) < EPS {
            return None;
        }
        let n = geom::perp(geom::normalize(geom::sub(pb, pa)));
        let h = geom::dot(geom::sub(c, pb), n);
        if h.abs() < EPS {
            return None;
        }
        let p2 = self.new_point(geom::add(pb, geom::scale(n, h)));
        let p3 = self.new_point(geom::add(pa, geom::scale(n, h)));
        let l = self.close_polygon(&[a, b, p2, p3]);
        self.constrain(Perpendicular(l[0], l[1]));
        self.constrain(Parallel(l[0], l[2]));
        self.constrain(Parallel(l[1], l[3]));
        Some([l[0], l[1], l[2], l[3]])
    }

    /// Circle through three points (Perimeter Circle).
    pub fn add_circle_3pt(&mut self, a: P2, b: P2, c: P2) -> Option<EntityId> {
        let (center, r) = geom::circle_through(a, b, c)?;
        let center = self.new_point(center);
        Some(self.add_circle(center, r))
    }

    /// Centerpoint Arc: radius from `start`, ends at the ray towards `toward`.
    pub fn add_center_arc(&mut self, center: PointId, start: PointId, toward: P2, ccw: bool) -> Option<EntityId> {
        let c = self.point(center);
        let r = geom::dist(self.point(start), c);
        if r < EPS || geom::dist(toward, c) < EPS {
            return None;
        }
        let end = self.new_point(geom::add(c, geom::scale(geom::normalize(geom::sub(toward, c)), r)));
        Some(if ccw {
            self.add_arc(center, start, end)
        } else {
            self.add_arc(center, end, start)
        })
    }

    /// 3 Point Arc from `a` to `b` passing through `through`.
    pub fn add_arc_3pt(&mut self, a: PointId, b: PointId, through: P2) -> Option<EntityId> {
        let (pa, pb) = (self.point(a), self.point(b));
        let (c, _) = geom::circle_through(pa, pb, through)?;
        let ccw = geom::on_arc(c, pa, pb, through);
        let center = self.new_point(c);
        Some(if ccw {
            self.add_arc(center, a, b)
        } else {
            self.add_arc(center, b, a)
        })
    }

    /// Direction a curve leaves its endpoint `at` (towards the outside of the curve).
    fn tangent_out(&self, e: EntityId, at: PointId) -> Option<P2> {
        let p = |id| self.point(id);
        match self.entity(e).geom {
            Geometry::Line { a, b } if b == at => Some(geom::normalize(geom::sub(p(b), p(a)))),
            Geometry::Line { a, b } if a == at => Some(geom::normalize(geom::sub(p(a), p(b)))),
            Geometry::Arc { center, end, .. } if end == at => {
                Some(geom::perp(geom::normalize(geom::sub(p(end), p(center)))))
            }
            Geometry::Arc { center, start, .. } if start == at => Some(geom::scale(
                geom::perp(geom::normalize(geom::sub(p(start), p(center)))),
                -1.0,
            )),
            _ => None,
        }
    }

    /// Tangent Arc continuing smoothly from the line or arc ending at `from`.
    pub fn add_tangent_arc(&mut self, from: PointId, end: PointId) -> Option<EntityId> {
        let (src, dir) = self
            .entities_at(from)
            .into_iter()
            .find_map(|e| self.tangent_out(e, from).map(|d| (e, d)))?;
        let (s, t) = (self.point(from), self.point(end));
        let n = geom::perp(dir);
        let st = geom::sub(s, t);
        let k = geom::dot(n, st);
        if k.abs() < EPS * (1.0 + geom::len(st)) {
            return None;
        }
        let along = -geom::dot(st, st) / (2.0 * k);
        let center = self.new_point(geom::add(s, geom::scale(n, along)));
        let arc = if along > 0.0 {
            self.add_arc(center, from, end)
        } else {
            self.add_arc(center, end, from)
        };
        if self.entity(src).is_line() {
            self.constrain(Tangent(src, arc));
        }
        Some(arc)
    }

    /// Straight slot between arc centers `a` and `b`; `width_at` sets the half-width.
    pub fn add_slot(&mut self, a: PointId, b: PointId, width_at: P2) -> Option<[EntityId; 4]> {
        let (pa, pb) = (self.point(a), self.point(b));
        if geom::dist(pa, pb) < EPS {
            return None;
        }
        let n = geom::perp(geom::normalize(geom::sub(pb, pa)));
        let h = geom::dot(geom::sub(width_at, pa), n).abs();
        if h < EPS {
            return None;
        }
        let axis = self.add_line(a, b);
        self.set_construction(axis, true);
        let off = |p: P2, s: f64| geom::add(p, geom::scale(n, s * h));
        let (a_l, a_r) = (self.new_point(off(pa, 1.0)), self.new_point(off(pa, -1.0)));
        let (b_l, b_r) = (self.new_point(off(pb, 1.0)), self.new_point(off(pb, -1.0)));
        let right = self.add_line(a_r, b_r);
        let arc_b = self.add_arc(b, b_r, b_l);
        let left = self.add_line(b_l, a_l);
        let arc_a = self.add_arc(a, a_l, a_r);
        for c in [
            Tangent(right, arc_b),
            Tangent(right, arc_a),
            Tangent(left, arc_a),
            Tangent(left, arc_b),
            Equal(arc_a, arc_b),
        ] {
            self.constrain(c);
        }
        Some([right, arc_b, left, arc_a])
    }

    /// Centerpoint Straight Slot: `center` is the slot middle, `end` one arc center.
    pub fn add_center_slot(&mut self, center: PointId, end: PointId, width_at: P2) -> Option<[EntityId; 4]> {
        let c = self.point(center);
        let start = self.new_point(geom::sub(geom::scale(c, 2.0), self.point(end)));
        let axis = EntityId(self.entities.len());
        let Some(slot) = self.add_slot(start, end, width_at) else {
            self.remove(&[start], &[], &[]);
            return None;
        };
        self.constrain(Midpoint(center, axis));
        Some(slot)
    }

    /// Regular polygon around `center`. Inscribed: `at` is a vertex; circumscribed: an edge midpoint.
    /// Vertices sit on a construction circumcircle with equal sides; a circumscribed polygon also gets
    /// a construction inscribed circle tangent to its first side. (All sides tangent plus all sides
    /// equal would be redundant for an even side count, by Pitot's theorem.)
    pub fn add_polygon(&mut self, center: PointId, at: P2, sides: usize, inscribed: bool) -> Option<Vec<EntityId>> {
        let c = self.point(center);
        let r = geom::dist(at, c);
        if sides < 3 || r < EPS {
            return None;
        }
        let n = sides as f64;
        let a0 = geom::angle_of(geom::sub(at, c));
        let (rv, start) = if inscribed {
            (r, a0)
        } else {
            (r / (PI / n).cos(), a0 + PI / n)
        };
        let outer = self.add_circle(center, rv);
        self.set_construction(outer, true);
        let pts: Vec<PointId> = (0..sides)
            .map(|k| {
                let a = start + TAU * k as f64 / n;
                self.new_point(geom::add(c, [rv * a.cos(), rv * a.sin()]))
            })
            .collect();
        let lines = self.close_polygon(&pts);
        for &l in &lines[1..] {
            self.constrain(Equal(lines[0], l));
        }
        for &p in &pts {
            self.constrain(PointOnCurve(p, outer));
        }
        if !inscribed {
            let inner = self.add_circle(center, r);
            self.set_construction(inner, true);
            self.constrain(Tangent(lines[0], inner));
        }
        Some(lines)
    }

    fn replace_point(&mut self, e: EntityId, old: PointId, new: PointId) {
        let swap = |p: &mut PointId| {
            if *p == old {
                *p = new;
            }
        };
        match &mut self.entity_mut(e).geom {
            Geometry::Line { a, b } => {
                swap(a);
                swap(b);
            }
            Geometry::Arc { start, end, .. } => {
                swap(start);
                swap(end);
            }
            Geometry::Spline { points } => points.iter_mut().for_each(swap),
            Geometry::Circle { .. } => {}
        }
    }

    /// The two lines meeting at `corner`, each with its far end.
    fn corner_lines(&self, corner: PointId) -> Result<[(EntityId, PointId); 2]> {
        let lines: Vec<(EntityId, PointId)> = self
            .entities_at(corner)
            .into_iter()
            .filter_map(|e| match self.entity(e).geom {
                Geometry::Line { a, b } if !self.entity(e).construction => Some((e, if a == corner { b } else { a })),
                _ => None,
            })
            .collect();
        match lines[..] {
            [l1, l2] => Ok([l1, l2]),
            _ => Err("Pick a corner where exactly two lines meet".into()),
        }
    }

    /// Sketch Fillet: rounds the corner with a tangent arc. The corner stays as a "virtual sharp"
    /// point on both lines, so dimensions that used it keep working.
    pub fn fillet(&mut self, corner: PointId, r: f64) -> Result<EntityId> {
        if r <= 0.0 {
            return Err("Fillet radius must be positive".into());
        }
        let [(l1, o1), (l2, o2)] = self.corner_lines(corner)?;
        let p = self.point(corner);
        let (u1, u2) = (
            geom::normalize(geom::sub(self.point(o1), p)),
            geom::normalize(geom::sub(self.point(o2), p)),
        );
        let phi = geom::dot(u1, u2).clamp(-1.0, 1.0).acos();
        if !(1e-6..=PI - 1e-6).contains(&phi) {
            return Err("The lines are parallel".into());
        }
        let t = r / (phi / 2.0).tan();
        if t >= geom::dist(self.point(o1), p) || t >= geom::dist(self.point(o2), p) {
            return Err("Radius is too large for these lines".into());
        }
        let (q1, q2) = (geom::add(p, geom::scale(u1, t)), geom::add(p, geom::scale(u2, t)));
        let c = geom::add(
            p,
            geom::scale(geom::normalize(geom::add(u1, u2)), r / (phi / 2.0).sin()),
        );
        let (t1, t2, center) = (self.new_point(q1), self.new_point(q2), self.new_point(c));
        self.replace_point(l1, corner, t1);
        self.replace_point(l2, corner, t2);
        let arc = if geom::cross(geom::sub(q1, c), geom::sub(q2, c)) > 0.0 {
            self.add_arc(center, t1, t2)
        } else {
            self.add_arc(center, t2, t1)
        };
        for c in [
            Tangent(l1, arc),
            Tangent(l2, arc),
            PointOnCurve(corner, l1),
            PointOnCurve(corner, l2),
        ] {
            self.constrain(c);
        }
        let dim = self.constrain(Radius(arc, r));
        self.set_label(dim, geom::lerp(p, c, 0.5));
        Ok(arc)
    }

    /// Sketch Chamfer (distance-distance) with a virtual sharp at the old corner.
    pub fn chamfer(&mut self, corner: PointId, d1: f64, d2: f64) -> Result<EntityId> {
        if d1 <= 0.0 || d2 <= 0.0 {
            return Err("Chamfer distances must be positive".into());
        }
        let [(l1, o1), (l2, o2)] = self.corner_lines(corner)?;
        let p = self.point(corner);
        let (e1, e2) = (self.point(o1), self.point(o2));
        if d1 >= geom::dist(e1, p) || d2 >= geom::dist(e2, p) {
            return Err("Chamfer is too large for these lines".into());
        }
        let t1 = self.new_point(geom::add(p, geom::scale(geom::normalize(geom::sub(e1, p)), d1)));
        let t2 = self.new_point(geom::add(p, geom::scale(geom::normalize(geom::sub(e2, p)), d2)));
        self.replace_point(l1, corner, t1);
        self.replace_point(l2, corner, t2);
        let line = self.add_line(t1, t2);
        for c in [
            PointOnCurve(corner, l1),
            PointOnCurve(corner, l2),
            Distance(corner, t1, d1),
            Distance(corner, t2, d2),
        ] {
            self.constrain(c);
        }
        Ok(line)
    }

    /// Sketch Chamfer (angle-distance): `d` along the first line, `angle` between it and the chamfer.
    pub fn chamfer_angle(&mut self, corner: PointId, d: f64, angle: f64) -> Result<EntityId> {
        let [(l1, o1), (_, o2)] = self.corner_lines(corner)?;
        let p = self.point(corner);
        let (u1, u2) = (
            geom::normalize(geom::sub(self.point(o1), p)),
            geom::normalize(geom::sub(self.point(o2), p)),
        );
        let phi = geom::dot(u1, u2).clamp(-1.0, 1.0).acos();
        let beta = PI - phi - angle;
        if angle <= 0.0 || beta <= 0.0 {
            return Err("That chamfer angle does not fit this corner".into());
        }
        let line = self.chamfer(corner, d, d * angle.sin() / beta.sin())?;
        self.remove_constraint(self.constraints().len() - 1);
        let a = self.measure(&Angle(l1, line, 0.0));
        self.constrain(Angle(l1, line, a));
        Ok(line)
    }

    /// Where other entities cross `e`, as (parameter along `e`, position, other entity).
    /// The parameter is `t` in `[0, 1]` for lines, the angle from the start for arcs and the
    /// absolute angle for circles.
    pub fn intersections(&self, e: EntityId) -> Vec<(f64, P2, EntityId)> {
        let mine = self.segments(e);
        let mut out = Vec::new();
        for o in self.entity_ids().filter(|&o| o != e) {
            for theirs in self.segments(o) {
                for m in &mine {
                    for p in segment_hits(m, &theirs) {
                        out.push((self.param_on(e, p), p, o));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.dedup_by(|a, b| geom::dist(a.1, b.1) < 1e-9);
        out
    }

    fn param_on(&self, e: EntityId, p: P2) -> f64 {
        match self.entity(e).geom {
            Geometry::Line { a, b } => {
                let (pa, pb) = (self.point(a), self.point(b));
                let d = geom::sub(pb, pa);
                geom::dot(geom::sub(p, pa), d) / geom::dot(d, d)
            }
            Geometry::Arc { center, start, .. } => {
                let c = self.point(center);
                (geom::angle_of(geom::sub(p, c)) - geom::angle_of(geom::sub(self.point(start), c))).rem_euclid(TAU)
            }
            Geometry::Circle { center, .. } => geom::angle_of(geom::sub(p, self.point(center))).rem_euclid(TAU),
            Geometry::Spline { .. } => 0.0,
        }
    }

    /// Power trim: removes the piece of `e` under `at` between its neighbouring intersections.
    pub fn trim(&mut self, e: EntityId, at: P2) -> Result<()> {
        let hits = self.intersections(e);
        let geom_e = self.entity(e).geom.clone();
        let mut orphans = Vec::new();
        match geom_e {
            Geometry::Line { a, b } => {
                let t = self.param_on(e, at).clamp(0.0, 1.0);
                let inner = |h: &&(f64, P2, EntityId)| h.0 > 1e-9 && h.0 < 1.0 - 1e-9;
                let lo = hits.iter().rev().filter(inner).find(|h| h.0 < t).copied();
                let hi = hits.iter().filter(inner).find(|h| h.0 > t).copied();
                match (lo, hi) {
                    (None, None) => {
                        self.remove(&[], &[e], &[]);
                        return Ok(());
                    }
                    (None, Some(h)) => {
                        let np = self.new_point(h.1);
                        self.replace_point(e, a, np);
                        self.try_constrain(PointOnCurve(np, h.2));
                        orphans.push(a);
                    }
                    (Some(l), None) => {
                        let np = self.new_point(l.1);
                        self.replace_point(e, b, np);
                        self.try_constrain(PointOnCurve(np, l.2));
                        orphans.push(b);
                    }
                    (Some(l), Some(h)) => {
                        let (p1, p2) = (self.new_point(l.1), self.new_point(h.1));
                        self.replace_point(e, b, p1);
                        let rest = self.add_line(p2, b);
                        let construction = self.entity(e).construction;
                        self.set_construction(rest, construction);
                        self.constrain(Collinear(e, rest));
                        self.try_constrain(PointOnCurve(p1, l.2));
                        self.try_constrain(PointOnCurve(p2, h.2));
                    }
                }
            }
            Geometry::Circle { center, .. } => {
                if hits.len() < 2 {
                    self.remove(&[], &[e], &[]);
                    return Ok(());
                }
                let c = self.point(center);
                let a_at = geom::angle_of(geom::sub(at, c));
                let off = |h: &(f64, P2, EntityId)| (h.0 - a_at).rem_euclid(TAU);
                let next = hits.iter().min_by(|x, y| off(x).total_cmp(&off(y))).copied().unwrap();
                let prev = hits.iter().max_by(|x, y| off(x).total_cmp(&off(y))).copied().unwrap();
                let (start, end) = (self.new_point(next.1), self.new_point(prev.1));
                self.entity_mut(e).geom = Geometry::Arc { center, start, end };
                self.try_constrain(PointOnCurve(start, next.2));
                self.try_constrain(PointOnCurve(end, prev.2));
            }
            Geometry::Arc { center, start, end } => {
                let sweep = self.param_on(e, self.point(end));
                let t = self.param_on(e, at).min(sweep);
                let inner = |h: &&(f64, P2, EntityId)| h.0 > 1e-9 && h.0 < sweep - 1e-9;
                let lo = hits.iter().rev().filter(inner).find(|h| h.0 < t).copied();
                let hi = hits.iter().filter(inner).find(|h| h.0 > t).copied();
                match (lo, hi) {
                    (None, None) => {
                        self.remove(&[], &[e], &[]);
                        return Ok(());
                    }
                    (None, Some(h)) => {
                        let np = self.new_point(h.1);
                        self.replace_point(e, start, np);
                        self.try_constrain(PointOnCurve(np, h.2));
                        orphans.push(start);
                    }
                    (Some(l), None) => {
                        let np = self.new_point(l.1);
                        self.replace_point(e, end, np);
                        self.try_constrain(PointOnCurve(np, l.2));
                        orphans.push(end);
                    }
                    (Some(l), Some(h)) => {
                        let (p1, p2) = (self.new_point(l.1), self.new_point(h.1));
                        self.replace_point(e, end, p1);
                        let rest = self.add_arc(center, p2, end);
                        self.constrain(Equal(e, rest));
                        self.try_constrain(PointOnCurve(p1, l.2));
                        self.try_constrain(PointOnCurve(p2, h.2));
                    }
                }
            }
            Geometry::Spline { .. } => return Err("Trimming splines is not supported yet".into()),
        }
        let unused: Vec<PointId> = orphans
            .into_iter()
            .filter(|&p| self.entities_at(p).is_empty())
            .collect();
        if !unused.is_empty() {
            self.remove(&unused, &[], &[]);
        }
        Ok(())
    }

    /// Extends the free end of a line nearest `at` to the next entity in its direction.
    pub fn extend(&mut self, e: EntityId, at: P2) -> Result<()> {
        let Geometry::Line { a, b } = self.entity(e).geom else {
            return Err("Only lines can be extended".into());
        };
        let (pa, pb) = (self.point(a), self.point(b));
        let (end, forward) = if geom::dist(at, pb) <= geom::dist(at, pa) {
            (b, true)
        } else {
            (a, false)
        };
        if self.entities_at(end).len() > 1 {
            return Err("That end is connected; extend a free end".into());
        }
        let ray = Segment::Line { from: pa, to: pb };
        let mut best: Option<(f64, P2, EntityId)> = None;
        for o in self.entity_ids().filter(|&o| o != e) {
            for s in self.segments(o) {
                for (t, p) in line_hits(&ray, &s, true) {
                    let ahead = if forward { t > 1.0 + 1e-9 } else { t < -1e-9 };
                    let nearer = best.is_none_or(|(bt, ..)| if forward { t < bt } else { t > bt });
                    if ahead && nearer {
                        best = Some((t, p, o));
                    }
                }
            }
        }
        let (_, p, o) = best.ok_or("Nothing to extend to")?;
        self.set_point(end, p);
        self.try_constrain(PointOnCurve(end, o));
        Ok(())
    }

    /// Offset Entities. Positive `d` goes to the left of lines and arcs (inside a counter-clockwise
    /// loop) and outside circles. Chained entities stay joined at their corners.
    pub fn offset(&mut self, entities: &[EntityId], d: f64) -> Result<Vec<EntityId>> {
        if d == 0.0 || entities.is_empty() {
            return Err("Pick entities and a non-zero distance".into());
        }
        // Offset end positions per entity: (start, end, new radius for round entities).
        let mut ends: Vec<(P2, P2, Option<f64>)> = Vec::new();
        for &e in entities {
            let p = |id| self.point(id);
            ends.push(match self.entity(e).geom {
                Geometry::Line { a, b } => {
                    let n = geom::scale(geom::perp(geom::normalize(geom::sub(p(b), p(a)))), d);
                    (geom::add(p(a), n), geom::add(p(b), n), None)
                }
                Geometry::Arc { center, start, end } => {
                    let (c, r) = (p(center), self.radius(e));
                    let r2 = r - d;
                    if r2 <= EPS {
                        return Err("Offset is larger than the arc radius".into());
                    }
                    let at = |q: P2| geom::add(c, geom::scale(geom::sub(q, c), r2 / r));
                    (at(p(start)), at(p(end)), Some(r2))
                }
                Geometry::Circle { .. } => {
                    let r2 = self.radius(e) + d;
                    if r2 <= EPS {
                        return Err("Offset is larger than the circle radius".into());
                    }
                    ([0.0; 2], [0.0; 2], Some(r2))
                }
                Geometry::Spline { .. } => return Err("Offsetting splines is not supported yet".into()),
            });
        }
        // Corners shared by two picked entities become one new point.
        let mut shared: HashMap<PointId, Vec<(usize, bool)>> = HashMap::new();
        for (i, &e) in entities.iter().enumerate() {
            if let Some((s, t)) = self.entity(e).endpoints() {
                shared.entry(s).or_default().push((i, true));
                shared.entry(t).or_default().push((i, false));
            }
        }
        let mut new_end: HashMap<(usize, bool), PointId> = HashMap::new();
        for (_, uses) in shared.iter().filter(|(_, u)| u.len() == 2) {
            let ((i, si), (j, sj)) = (uses[0], uses[1]);
            let pos = |(k, start): (usize, bool)| if start { ends[k].0 } else { ends[k].1 };
            let both_lines = self.entity(entities[i]).is_line() && self.entity(entities[j]).is_line();
            let at = match both_lines {
                true => geom::line_line(ends[i].0, ends[i].1, ends[j].0, ends[j].1)
                    .map(|(t, _)| geom::lerp(ends[i].0, ends[i].1, t))
                    .unwrap_or(pos((i, si))),
                false => geom::lerp(pos((i, si)), pos((j, sj)), 0.5),
            };
            let q = self.new_point(at);
            new_end.insert((i, si), q);
            new_end.insert((j, sj), q);
        }
        let mut out = Vec::new();
        for (i, &e) in entities.iter().enumerate() {
            let mut point = |s: &mut Sketch, start: bool| {
                *new_end
                    .entry((i, start))
                    .or_insert_with(|| s.new_point(if start { ends[i].0 } else { ends[i].1 }))
            };
            let geom_e = self.entity(e).geom.clone();
            let created = match geom_e {
                Geometry::Line { .. } => {
                    let (a, b) = (point(self, true), point(self, false));
                    let l = self.add_line(a, b);
                    self.constrain(Parallel(e, l));
                    self.constrain(PointLineDistance(a, e, d.abs()));
                    l
                }
                Geometry::Arc { center, .. } => {
                    let (s, t) = (point(self, true), point(self, false));
                    self.add_arc(center, s, t)
                }
                Geometry::Circle { center, .. } => self.add_circle(center, ends[i].2.unwrap()),
                Geometry::Spline { .. } => unreachable!(),
            };
            out.push(created);
        }
        Ok(out)
    }

    fn next_group(&self) -> u32 {
        self.constraints()
            .iter()
            .filter_map(Constraint::pattern_group)
            .max()
            .map_or(0, |g| g + 1)
    }

    /// Copies entities with their points moved by `map`; points where `keep` holds are shared.
    /// Returns the copies, (source, copy) point pairs and (source, copy) circle pairs.
    #[allow(clippy::type_complexity)]
    fn copy_group(
        &mut self,
        entities: &[EntityId],
        map: &dyn Fn(P2) -> P2,
        flip: bool,
        keep: &dyn Fn(PointId) -> bool,
    ) -> (Vec<EntityId>, Vec<(PointId, PointId)>, Vec<(EntityId, EntityId)>) {
        let mut pmap: HashMap<PointId, PointId> = HashMap::new();
        let mut pairs = Vec::new();
        let mut circles = Vec::new();
        let mut copies = Vec::new();
        for &e in entities {
            let src = self.entity(e).clone();
            let mut get = |s: &mut Sketch, p: PointId| -> PointId {
                if keep(p) {
                    return p;
                }
                *pmap.entry(p).or_insert_with(|| {
                    let q = s.new_point(map(s.point(p)));
                    pairs.push((p, q));
                    q
                })
            };
            let copy = match src.geom {
                Geometry::Line { a, b } => {
                    let (a, b) = (get(self, a), get(self, b));
                    self.add_line(a, b)
                }
                Geometry::Circle { center, .. } => {
                    let c = get(self, center);
                    let r = self.radius(e);
                    let copy = self.add_circle(c, r);
                    circles.push((e, copy));
                    copy
                }
                Geometry::Arc { center, start, end } => {
                    let (c, s, t) = (get(self, center), get(self, start), get(self, end));
                    if flip {
                        self.add_arc(c, t, s)
                    } else {
                        self.add_arc(c, s, t)
                    }
                }
                Geometry::Spline { points } => {
                    let pts = points.iter().map(|&p| get(self, p)).collect();
                    self.add_spline(pts)
                }
            };
            self.set_construction(copy, src.construction);
            copies.push(copy);
        }
        (copies, pairs, circles)
    }

    /// Mirror Entities about a line, with Symmetric relations (points on the axis are shared).
    pub fn mirror(&mut self, entities: &[EntityId], axis: EntityId) -> Result<Vec<EntityId>> {
        let Geometry::Line { a, b } = self.entity(axis).geom else {
            return Err("Mirror about a line or centerline".into());
        };
        let (pa, pb) = (self.point(a), self.point(b));
        let reflect = move |p: P2| {
            let (foot, _) = foot_on_line(pa, pb, p);
            geom::sub(geom::scale(foot, 2.0), p)
        };
        let on_axis: Vec<PointId> = self
            .point_ids()
            .filter(|&p| {
                let q = self.point(p);
                geom::dist(foot_on_line(pa, pb, q).0, q) < 1e-9
            })
            .collect();
        let picked: Vec<EntityId> = entities.iter().copied().filter(|&e| e != axis).collect();
        if picked.is_empty() {
            return Err("Pick entities to mirror".into());
        }
        let (copies, pairs, circles) = self.copy_group(&picked, &reflect, true, &|p| on_axis.contains(&p));
        for (s, d) in pairs {
            self.constrain(Symmetric(s, d, axis));
        }
        for (s, d) in circles {
            self.constrain(Equal(s, d));
        }
        Ok(copies)
    }

    /// Linear Sketch Pattern: `count` instances (the original included) `spacing` apart along `angle`.
    pub fn linear_pattern(
        &mut self,
        entities: &[EntityId],
        count: usize,
        spacing: f64,
        angle: f64,
    ) -> Result<Vec<EntityId>> {
        if count < 2 || entities.is_empty() {
            return Err("Pick entities and at least 2 instances".into());
        }
        let group = self.next_group();
        let dir = [angle.cos(), angle.sin()];
        let mut out = Vec::new();
        for k in 1..count {
            let shift = geom::scale(dir, spacing * k as f64);
            let (copies, pairs, circles) = self.copy_group(entities, &|p| geom::add(p, shift), false, &|_| false);
            for (src, dst) in pairs {
                self.constrain(LinearCopy {
                    src,
                    dst,
                    step: k as u32,
                    spacing,
                    angle,
                    group,
                });
            }
            for (s, d) in circles {
                self.constrain(Equal(s, d));
            }
            out.extend(copies);
        }
        Ok(out)
    }

    /// Circular Sketch Pattern around `center` over `total` radians (a full turn spaces instances evenly).
    pub fn circular_pattern(
        &mut self,
        entities: &[EntityId],
        center: PointId,
        count: usize,
        total: f64,
    ) -> Result<Vec<EntityId>> {
        if count < 2 || entities.is_empty() {
            return Err("Pick entities and at least 2 instances".into());
        }
        let full = (total.abs() - TAU).abs() < 1e-9;
        let step = if full {
            total / count as f64
        } else {
            total / (count - 1) as f64
        };
        let c = self.point(center);
        let group = self.next_group();
        let mut out = Vec::new();
        for k in 1..count {
            let turn = step * k as f64;
            let rot = |p: P2| geom::add(c, geom::rotate(geom::sub(p, c), turn));
            let (copies, pairs, circles) = self.copy_group(entities, &rot, false, &|p| p == center);
            for (src, dst) in pairs {
                self.constrain(CircularCopy {
                    src,
                    dst,
                    center,
                    step: k as u32,
                    angle: step,
                    group,
                });
            }
            for (s, d) in circles {
                self.constrain(Equal(s, d));
            }
            out.extend(copies);
        }
        Ok(out)
    }

    /// The dimension Smart Dimension creates for the picked items, at its current value.
    /// `place` (where the label is dropped) picks horizontal/vertical/aligned for point pairs.
    pub fn smart_dimension(&self, items: &[Item], place: P2) -> Option<Constraint> {
        let line = |e: EntityId| self.entity(e).is_line();
        let round = |e: EntityId| self.entity(e).is_round();
        let c = match *items {
            [Item::Entity(e)] => match self.entity(e).geom {
                Geometry::Line { a, b } => self.point_pair_dimension(a, b, place),
                Geometry::Circle { .. } => Diameter(e, 0.0),
                Geometry::Arc { .. } => Radius(e, 0.0),
                Geometry::Spline { .. } => return None,
            },
            [Item::Point(p), Item::Point(q)] => self.point_pair_dimension(p, q, place),
            [Item::Point(p), Item::Entity(e)] | [Item::Entity(e), Item::Point(p)] if line(e) => {
                PointLineDistance(p, e, 0.0)
            }
            [Item::Point(p), Item::Entity(e)] | [Item::Entity(e), Item::Point(p)] if round(e) => {
                Distance(p, self.entity(e).center()?, 0.0)
            }
            [Item::Entity(e1), Item::Entity(e2)] if line(e1) && line(e2) => {
                let seg = |e: EntityId| {
                    self.entity(e)
                        .endpoints()
                        .map(|(a, b)| (self.point(a), self.point(b)))
                        .unwrap()
                };
                let ((a1, b1), (a2, b2)) = (seg(e1), seg(e2));
                let (u, v) = (geom::sub(b1, a1), geom::sub(b2, a2));
                if geom::cross(u, v).abs() < 1e-9 * geom::len(u) * geom::len(v) {
                    PointLineDistance(self.entity(e2).endpoints()?.0, e1, 0.0)
                } else {
                    Angle(e1, e2, 0.0)
                }
            }
            [Item::Entity(e1), Item::Entity(e2)] if round(e1) && round(e2) => {
                Distance(self.entity(e1).center()?, self.entity(e2).center()?, 0.0)
            }
            _ => return None,
        };
        let mut c = c;
        c.set_value(self.measure(&c));
        c.is_valid(self).then_some(c)
    }

    /// Relations that make sense for the picked items (Add Relations), minus ones already present.
    pub fn relation_candidates(&self, items: &[Item]) -> Vec<Constraint> {
        let points: Vec<PointId> = items
            .iter()
            .filter_map(|i| match *i {
                Item::Point(p) => Some(p),
                _ => None,
            })
            .collect();
        let entities: Vec<EntityId> = items
            .iter()
            .filter_map(|i| match *i {
                Item::Entity(e) => Some(e),
                _ => None,
            })
            .collect();
        let lines: Vec<EntityId> = entities.iter().copied().filter(|&e| self.entity(e).is_line()).collect();
        let rounds: Vec<EntityId> = entities
            .iter()
            .copied()
            .filter(|&e| self.entity(e).is_round())
            .collect();
        let only = |n_points: usize, n_lines: usize, n_rounds: usize| {
            points.len() == n_points
                && lines.len() == n_lines
                && rounds.len() == n_rounds
                && entities.len() == n_lines + n_rounds
        };
        let mut out = Vec::new();
        if only(1, 0, 0) {
            out.push(Fix(points[0], self.point(points[0])));
        } else if only(2, 0, 0) {
            let (p, q) = (points[0], points[1]);
            out.extend([Coincident(p, q), HorizontalPoints(p, q), VerticalPoints(p, q)]);
        } else if only(0, 1, 0) {
            out.extend([Horizontal(lines[0]), Vertical(lines[0])]);
        } else if only(0, 2, 0) {
            let (a, b) = (lines[0], lines[1]);
            out.extend([Parallel(a, b), Perpendicular(a, b), Collinear(a, b), Equal(a, b)]);
        } else if only(0, 1, 1) {
            out.push(Tangent(lines[0], rounds[0]));
        } else if only(0, 0, 2) {
            out.extend([Equal(rounds[0], rounds[1]), Concentric(rounds[0], rounds[1])]);
        } else if only(1, 1, 0) {
            out.extend([Midpoint(points[0], lines[0]), PointOnCurve(points[0], lines[0])]);
        } else if only(1, 0, 1) {
            out.push(PointOnCurve(points[0], rounds[0]));
        } else if only(2, 1, 0) {
            out.push(Symmetric(points[0], points[1], lines[0]));
        }
        out.retain(|c| self.accepts(c));
        out
    }

    fn point_pair_dimension(&self, p: PointId, q: PointId, place: P2) -> Constraint {
        let (a, b) = (self.point(p), self.point(q));
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        let within_x = place[0] >= x0 && place[0] <= x1;
        let within_y = place[1] >= y0 && place[1] <= y1;
        if within_x && !within_y && x1 - x0 > EPS {
            HorizontalDistance(p, q, 0.0)
        } else if within_y && !within_x && y1 - y0 > EPS {
            VerticalDistance(p, q, 0.0)
        } else {
            Distance(p, q, 0.0)
        }
    }
}

fn foot_on_line(a: P2, b: P2, p: P2) -> (P2, f64) {
    let d = geom::sub(b, a);
    let t = geom::dot(geom::sub(p, a), d) / geom::dot(d, d);
    (geom::lerp(a, b, t), t)
}

/// Hits of the line segment `line` with `other` as (t along `line`, point). With `infinite`,
/// `t` may fall outside `[0, 1]` (for extend).
fn line_hits(line: &Segment, other: &Segment, infinite: bool) -> Vec<(f64, P2)> {
    let Segment::Line { from: a, to: b } = *line else {
        return Vec::new();
    };
    let in_t = |t: f64| infinite || (-1e-9..=1.0 + 1e-9).contains(&t);
    match *other {
        Segment::Line { from: c, to: d } => geom::line_line(a, b, c, d)
            .filter(|&(t, u)| in_t(t) && (-1e-9..=1.0 + 1e-9).contains(&u))
            .map(|(t, _)| vec![(t, geom::lerp(a, b, t))])
            .unwrap_or_default(),
        Segment::Circle { center, radius, .. } => geom::line_circle(a, b, center, radius)
            .into_iter()
            .filter(|&t| in_t(t))
            .map(|t| (t, geom::lerp(a, b, t)))
            .collect(),
        Segment::Arc { center, from, to, ccw } => {
            let (s, e) = if ccw { (from, to) } else { (to, from) };
            geom::line_circle(a, b, center, geom::dist(from, center))
                .into_iter()
                .filter(|&t| in_t(t))
                .map(|t| (t, geom::lerp(a, b, t)))
                .filter(|&(_, p)| geom::on_arc(center, s, e, p))
                .collect()
        }
        Segment::Bezier(_) => bezier_lines(other)
            .iter()
            .flat_map(|l| line_hits(line, l, infinite))
            .collect(),
    }
}

fn bezier_lines(b: &Segment) -> Vec<Segment> {
    let mut pts = Vec::new();
    b.sample(0, &mut pts);
    pts.push(b.end());
    pts.windows(2).map(|w| Segment::Line { from: w[0], to: w[1] }).collect()
}

/// Intersection points of two segments.
fn segment_hits(s1: &Segment, s2: &Segment) -> Vec<P2> {
    match (s1, s2) {
        (Segment::Line { .. }, _) => line_hits(s1, s2, false).into_iter().map(|h| h.1).collect(),
        (_, Segment::Line { .. }) => line_hits(s2, s1, false).into_iter().map(|h| h.1).collect(),
        (Segment::Bezier(_), _) => bezier_lines(s1).iter().flat_map(|l| segment_hits(l, s2)).collect(),
        (_, Segment::Bezier(_)) => bezier_lines(s2).iter().flat_map(|l| segment_hits(s1, l)).collect(),
        _ => {
            let circle = |s: &Segment| match *s {
                Segment::Circle { center, radius, .. } => (center, radius),
                Segment::Arc { center, from, .. } => (center, geom::dist(from, center)),
                _ => unreachable!(),
            };
            let on = |s: &Segment, p: P2| match *s {
                Segment::Arc { center, from, to, ccw } => {
                    let (a, b) = if ccw { (from, to) } else { (to, from) };
                    geom::on_arc(center, a, b, p)
                }
                _ => true,
            };
            let ((c1, r1), (c2, r2)) = (circle(s1), circle(s2));
            geom::circle_circle(c1, r1, c2, r2)
                .into_iter()
                .filter(|&p| on(s1, p) && on(s2, p))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Region, Status};

    fn close(a: P2, b: P2) -> bool {
        geom::dist(a, b) < 1e-6
    }

    fn area(r: &[Region]) -> f64 {
        r.iter()
            .map(|r| {
                geom::polygon_area(&crate::profile::loop_polygon(&r.outer, 4096))
                    + r.holes
                        .iter()
                        .map(|h| geom::polygon_area(&crate::profile::loop_polygon(h, 4096)))
                        .sum::<f64>()
            })
            .sum()
    }

    #[test]
    fn rectangle_tools_keep_their_shape() {
        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let k = s.add_point(4.0, 3.0);
        s.add_center_rectangle(c, k).unwrap();
        s.constrain(Fix(c, [0.0, 0.0]));
        s.drag(&[(k, [6.0, 2.0])]);
        assert!(s.solve().converged);
        assert!(
            (area(&s.regions().unwrap()) - 48.0).abs() < 1e-6,
            "{}",
            area(&s.regions().unwrap())
        );

        let mut s = Sketch::default();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(3.0, 4.0);
        s.add_rectangle_3pt(a, b, [-4.0, 5.0]).unwrap();
        assert!(s.solve().converged);
        let an = s.analyze();
        assert_eq!((an.status, an.dof), (Status::UnderDefined, 5));
        assert!(s.add_rectangle_3pt(a, b, [1.5, 2.0]).is_none(), "collinear third point");
    }

    #[test]
    fn arcs_three_ways() {
        let mut s = Sketch::default();
        let a = s.add_point(1.0, 0.0);
        let b = s.add_point(-1.0, 0.0);
        let arc = s.add_arc_3pt(a, b, [0.0, -1.0]).unwrap();
        // Through the bottom means clockwise from a to b, stored counter-clockwise from b to a.
        assert!(matches!(s.entity(arc).geom, Geometry::Arc { start, end, .. } if start == b && end == a));
        assert!((s.radius(arc) - 1.0).abs() < 1e-12);

        let mut s = Sketch::default();
        let p0 = s.add_point(0.0, 0.0);
        let p1 = s.add_point(5.0, 0.0);
        let line = s.add_line(p0, p1);
        let end = s.add_point(5.0, 4.0);
        let arc = s.add_tangent_arc(p1, end).unwrap();
        assert!(s.constraints().contains(&Tangent(line, arc)));
        let Geometry::Arc { center, .. } = s.entity(arc).geom else {
            panic!()
        };
        assert!(close(s.point(center), [5.0, 2.0]));
        assert!(s.solve().converged);

        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let st = s.add_point(2.0, 0.0);
        let arc = s.add_center_arc(c, st, [0.0, 5.0], true).unwrap();
        let Geometry::Arc { end, .. } = s.entity(arc).geom else {
            panic!()
        };
        assert!(close(s.point(end), [0.0, 2.0]));
    }

    #[test]
    fn polygon_is_regular_and_under_defined_by_four() {
        for inscribed in [true, false] {
            let mut s = Sketch::default();
            let c = s.add_point(0.0, 0.0);
            let lines = s.add_polygon(c, [5.0, 0.0], 6, inscribed).unwrap();
            assert_eq!(lines.len(), 6);
            assert!(s.solve().converged);
            let an = s.analyze();
            assert_eq!((an.status, an.dof), (Status::UnderDefined, 4), "inscribed={inscribed}");
            assert_eq!(s.regions().unwrap().len(), 1);
            let side = |e: EntityId| {
                let (a, b) = s.entity(e).endpoints().unwrap();
                geom::dist(s.point(a), s.point(b))
            };
            let expected = if inscribed { 5.0 } else { 10.0 / 3f64.sqrt() };
            assert!(lines.iter().all(|&l| (side(l) - expected).abs() < 1e-9));
        }
    }

    #[test]
    fn fillet_keeps_rectangle_fully_defined() {
        let mut s = Sketch::rectangle(10.0, 6.0);
        let arc = s.fillet(PointId(1), 2.0).unwrap();
        assert!(s.solve().converged);
        assert_eq!(s.analyze().status, Status::FullyDefined);
        let pi = std::f64::consts::PI;
        let expected = 60.0 - (4.0 - pi);
        assert!(
            (area(&s.regions().unwrap()) - expected).abs() < 1e-3,
            "{}",
            area(&s.regions().unwrap())
        );
        // The radius dimension drives it; the virtual sharp keeps the height dimension alive.
        let r = s.constraints().iter().position(|c| *c == Radius(arc, 2.0)).unwrap();
        s.set_dimension(r, 3.0);
        assert!(s.solve().converged);
        assert!((s.radius(arc) - 3.0).abs() < 1e-6);
        assert!(close(s.point(PointId(1)), [10.0, 6.0]));
        assert!(s.fillet(PointId(0), 50.0).is_err());
    }

    #[test]
    fn chamfers() {
        let mut s = Sketch::rectangle(10.0, 6.0);
        s.chamfer(PointId(1), 2.0, 3.0).unwrap();
        assert!(s.solve().converged);
        assert_eq!(s.analyze().status, Status::FullyDefined);
        assert!((area(&s.regions().unwrap()) - 57.0).abs() < 1e-6);

        let mut s = Sketch::rectangle(10.0, 6.0);
        s.chamfer_angle(PointId(1), 2.0, 45f64.to_radians()).unwrap();
        assert!(s.solve().converged);
        assert!((area(&s.regions().unwrap()) - 58.0).abs() < 1e-6);
    }

    #[test]
    fn trim_line_between_crossings_and_circle_into_arc() {
        let mut s = Sketch::default();
        let p: Vec<PointId> = [
            [0.0, 0.0],
            [10.0, 0.0],
            [3.0, -2.0],
            [3.0, 2.0],
            [6.0, -2.0],
            [6.0, 2.0],
        ]
        .iter()
        .map(|q| s.add_point(q[0], q[1]))
        .collect();
        let h = s.add_line(p[0], p[1]);
        s.add_line(p[2], p[3]);
        s.add_line(p[4], p[5]);
        s.trim(h, [4.5, 0.0]).unwrap();
        assert_eq!(s.entities().len(), 4, "line split in two");
        assert!(s.constraints().iter().any(|c| matches!(c, Collinear(..))));
        assert!(s.solve().converged);

        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let circle = s.add_circle(c, 2.0);
        let a = s.add_point(-3.0, 0.0);
        let b = s.add_point(3.0, 0.0);
        s.add_line(a, b);
        s.trim(circle, [0.0, -2.0]).unwrap();
        let Geometry::Arc { start, end, .. } = s.entity(circle).geom else {
            panic!("circle became an arc")
        };
        assert!(close(s.point(start), [-2.0, 0.0]) || close(s.point(start), [2.0, 0.0]));
        assert!(
            geom::on_arc([0.0, 0.0], s.point(start), s.point(end), [0.0, 2.0]),
            "the top half stays"
        );
        assert!(s.solve().converged);
    }

    #[test]
    fn trim_end_and_extend() {
        let mut s = Sketch::default();
        let p: Vec<PointId> = [[0.0, 0.0], [10.0, 0.0], [6.0, -2.0], [6.0, 2.0]]
            .iter()
            .map(|q| s.add_point(q[0], q[1]))
            .collect();
        let h = s.add_line(p[0], p[1]);
        s.add_line(p[2], p[3]);
        s.trim(h, [9.0, 0.0]).unwrap();
        let (_, b) = s.entity(h).endpoints().unwrap();
        assert!(close(s.point(b), [6.0, 0.0]));
        assert_eq!(s.point_count(), 4, "the old end point is gone");

        let mut s = Sketch::default();
        let p: Vec<PointId> = [[0.0, 0.0], [4.0, 0.0], [6.0, -2.0], [6.0, 2.0]]
            .iter()
            .map(|q| s.add_point(q[0], q[1]))
            .collect();
        let h = s.add_line(p[0], p[1]);
        s.add_line(p[2], p[3]);
        s.extend(h, [3.9, 0.0]).unwrap();
        assert!(close(s.point(p[1]), [6.0, 0.0]));
        assert!(s.solve().converged);
    }

    #[test]
    fn offset_chain_stays_joined_and_parallel() {
        let mut s = Sketch::default();
        let p: Vec<PointId> = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]]
            .iter()
            .map(|q| s.add_point(q[0], q[1]))
            .collect();
        let l1 = s.add_line(p[0], p[1]);
        let l2 = s.add_line(p[1], p[2]);
        let out = s.offset(&[l1, l2], 1.0).unwrap();
        let (_, end1) = s.entity(out[0]).endpoints().unwrap();
        let (start2, _) = s.entity(out[1]).endpoints().unwrap();
        assert_eq!(end1, start2, "corner shared");
        assert!(close(s.point(end1), [9.0, 1.0]));
        assert!(s.solve().converged);
        let c = s.add_point(0.0, 0.0);
        let circle = s.add_circle(c, 2.0);
        let rings = s.offset(&[circle], 1.0).unwrap();
        assert!((s.radius(rings[0]) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn mirror_and_patterns() {
        let mut s = Sketch::default();
        let ax0 = s.add_point(0.0, -5.0);
        let ax1 = s.add_point(0.0, 5.0);
        let axis = s.add_line(ax0, ax1);
        s.set_construction(axis, true);
        s.constrain(Fix(ax0, [0.0, -5.0]));
        s.constrain(Fix(ax1, [0.0, 5.0]));
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(3.0, 1.0);
        let l = s.add_line(a, b);
        let c = s.add_point(4.0, 3.0);
        let circle = s.add_circle(c, 1.0);
        let copies = s.mirror(&[l, circle], axis).unwrap();
        assert_eq!(copies.len(), 2);
        let (ma, mb) = s.entity(copies[0]).endpoints().unwrap();
        assert_eq!(ma, a, "point on the axis is shared");
        assert!(close(s.point(mb), [-3.0, 1.0]));
        s.drag(&[(b, [2.0, 2.0])]);
        assert!(close(s.point(mb), [-2.0, 2.0]), "mirror follows: {:?}", s.point(mb));
        assert_ne!(s.analyze().status, Status::OverDefined);

        let mut s = Sketch::default();
        let c = s.add_point(5.0, 0.0);
        let e = s.add_circle(c, 1.0);
        let o = s.add_point(0.0, 0.0);
        let copies = s.circular_pattern(&[e], o, 4, std::f64::consts::TAU).unwrap();
        assert_eq!(copies.len(), 3);
        assert!(close(s.point(s.entity(copies[0]).center().unwrap()), [0.0, 5.0]));
        assert!(s.solve().converged);
        assert_ne!(s.analyze().status, Status::OverDefined);
    }

    #[test]
    fn relation_candidates_match_the_selection() {
        let s = Sketch::rectangle(10.0, 6.0);
        let names = |items: &[Item]| {
            s.relation_candidates(items)
                .iter()
                .map(Constraint::name)
                .collect::<Vec<_>>()
        };
        // The bottom line is already horizontal, so only Vertical is offered.
        assert_eq!(names(&[Item::Entity(EntityId(0))]), ["Vertical"]);
        assert_eq!(
            names(&[Item::Entity(EntityId(0)), Item::Entity(EntityId(1))]),
            ["Parallel", "Perpendicular", "Collinear", "Equal"]
        );
        assert_eq!(
            names(&[Item::Point(PointId(0)), Item::Entity(EntityId(1))]),
            ["Midpoint", "Coincident (on curve)"]
        );
        assert!(names(&[Item::Point(PointId(0))]).is_empty(), "already fixed");
        assert_eq!(names(&[Item::Point(PointId(1))]), ["Fix"]);
    }

    #[test]
    fn smart_dimension_picks_the_right_kind() {
        let mut s = Sketch::rectangle(10.0, 6.0);
        let c = s.add_point(20.0, 20.0);
        let circle = s.add_circle(c, 2.5);
        assert_eq!(
            s.smart_dimension(&[Item::Entity(circle)], [0.0; 2]),
            Some(Diameter(circle, 5.0))
        );
        let (p0, p2) = (PointId(0), PointId(1));
        assert!(
            matches!(s.smart_dimension(&[Item::Point(p0), Item::Point(p2)], [5.0, 9.0]), Some(HorizontalDistance(.., d)) if (d - 10.0).abs() < 1e-9)
        );
        assert!(
            matches!(s.smart_dimension(&[Item::Point(p0), Item::Point(p2)], [-3.0, 3.0]), Some(VerticalDistance(.., d)) if (d - 6.0).abs() < 1e-9)
        );
        assert!(matches!(
            s.smart_dimension(&[Item::Point(p0), Item::Point(p2)], [20.0, 20.0]),
            Some(Distance(..))
        ));
        // Two parallel lines measure their gap; two perpendicular ones their angle.
        assert!(
            matches!(s.smart_dimension(&[Item::Entity(EntityId(0)), Item::Entity(EntityId(2))], [0.0; 2]), Some(PointLineDistance(.., d)) if (d - 6.0).abs() < 1e-9)
        );
        assert!(matches!(
            s.smart_dimension(&[Item::Entity(EntityId(0)), Item::Entity(EntityId(1))], [0.0; 2]),
            Some(Angle(..))
        ));
    }
}
