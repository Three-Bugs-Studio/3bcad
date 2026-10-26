use std::f64::consts::{PI, TAU};

use crate::scalar::Scalar;
use crate::{EntityId, Geometry, PointId, Sketch};

/// Sketch relations and driving dimensions. Each one contributes residuals that are zero when
/// it is satisfied; residuals are written once over [`Scalar`] so the solver gets exact derivatives.
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    /// Grounds a point.
    Fix(PointId, [f64; 2]),
    Coincident(PointId, PointId),
    Horizontal(EntityId),
    Vertical(EntityId),
    /// Two points share a y coordinate.
    HorizontalPoints(PointId, PointId),
    /// Two points share an x coordinate.
    VerticalPoints(PointId, PointId),
    Parallel(EntityId, EntityId),
    Perpendicular(EntityId, EntityId),
    Collinear(EntityId, EntityId),
    /// Equal length (two lines) or equal radius (two circles/arcs).
    Equal(EntityId, EntityId),
    Midpoint(PointId, EntityId),
    Concentric(EntityId, EntityId),
    /// The two points mirror each other about the line.
    Symmetric(PointId, PointId, EntityId),
    /// Point lies on a line (extended), circle or arc.
    PointOnCurve(PointId, EntityId),
    /// A line tangent to a circle or arc.
    Tangent(EntityId, EntityId),
    Distance(PointId, PointId, f64),
    HorizontalDistance(PointId, PointId, f64),
    VerticalDistance(PointId, PointId, f64),
    PointLineDistance(PointId, EntityId, f64),
    /// Signed angle from line `a` to line `b`, radians.
    Angle(EntityId, EntityId, f64),
    Radius(EntityId, f64),
    Diameter(EntityId, f64),
    /// Linear sketch pattern: `dst = src + step * spacing * (cos angle, sin angle)`.
    /// Every relation of one pattern shares `group`, so editing the spacing moves all instances.
    LinearCopy {
        src: PointId,
        dst: PointId,
        step: u32,
        spacing: f64,
        angle: f64,
        group: u32,
    },
    /// Circular sketch pattern: `dst` is `src` rotated by `step * angle` about `center`.
    CircularCopy {
        src: PointId,
        dst: PointId,
        center: PointId,
        step: u32,
        angle: f64,
        group: u32,
    },
    /// Keeps an arc's end on its circle. Added automatically, never shown.
    ArcConsistency(EntityId),
}

use Constraint::*;

impl Constraint {
    pub fn name(&self) -> &'static str {
        match self {
            Fix(..) => "Fix",
            Coincident(..) => "Coincident",
            Horizontal(..) | HorizontalPoints(..) => "Horizontal",
            Vertical(..) | VerticalPoints(..) => "Vertical",
            Parallel(..) => "Parallel",
            Perpendicular(..) => "Perpendicular",
            Collinear(..) => "Collinear",
            Equal(..) => "Equal",
            Midpoint(..) => "Midpoint",
            Concentric(..) => "Concentric",
            Symmetric(..) => "Symmetric",
            PointOnCurve(..) => "Coincident (on curve)",
            Tangent(..) => "Tangent",
            Distance(..) => "Distance",
            HorizontalDistance(..) => "Horizontal distance",
            VerticalDistance(..) => "Vertical distance",
            PointLineDistance(..) => "Distance to line",
            Angle(..) => "Angle",
            Radius(..) => "Radius",
            Diameter(..) => "Diameter",
            LinearCopy { .. } => "Linear pattern",
            CircularCopy { .. } => "Circular pattern",
            ArcConsistency(..) => "Arc",
        }
    }

    /// Short glyph drawn next to the geometry, like SolidWorks' relation icons.
    pub fn glyph(&self) -> &'static str {
        match self {
            Fix(..) => "⚓",
            Coincident(..) | PointOnCurve(..) => "◉",
            Horizontal(..) | HorizontalPoints(..) => "—",
            Vertical(..) | VerticalPoints(..) => "|",
            Parallel(..) => "∥",
            Perpendicular(..) => "⊥",
            Collinear(..) => "≡",
            Equal(..) => "=",
            Midpoint(..) => "M",
            Concentric(..) => "◎",
            Symmetric(..) => "⇋",
            Tangent(..) => "T",
            _ => "",
        }
    }

    /// The driving value of a dimension (pattern spacing / angle included).
    pub fn dimension(&self) -> Option<f64> {
        match *self {
            Distance(.., d)
            | HorizontalDistance(.., d)
            | VerticalDistance(.., d)
            | PointLineDistance(.., d)
            | Angle(.., d)
            | Radius(_, d)
            | Diameter(_, d) => Some(d),
            LinearCopy { spacing, .. } => Some(spacing),
            CircularCopy { angle, .. } => Some(angle),
            _ => None,
        }
    }

    pub fn is_angular(&self) -> bool {
        matches!(self, Angle(..) | CircularCopy { .. })
    }

    /// Hidden from relation lists.
    pub fn is_internal(&self) -> bool {
        matches!(self, ArcConsistency(..))
    }

    pub(crate) fn pattern_group(&self) -> Option<u32> {
        match *self {
            LinearCopy { group, .. } | CircularCopy { group, .. } => Some(group),
            _ => None,
        }
    }

    pub(crate) fn set_value(&mut self, value: f64) {
        match self {
            Distance(.., d)
            | HorizontalDistance(.., d)
            | VerticalDistance(.., d)
            | PointLineDistance(.., d)
            | Angle(.., d)
            | Radius(_, d)
            | Diameter(_, d) => *d = value,
            LinearCopy { spacing, .. } => *spacing = value,
            CircularCopy { angle, .. } => *angle = value,
            _ => {}
        }
    }

    /// Points referenced directly.
    pub fn points(&self) -> Vec<PointId> {
        match *self {
            Fix(p, _) | Midpoint(p, _) | PointOnCurve(p, _) | PointLineDistance(p, ..) => vec![p],
            Coincident(p, q)
            | HorizontalPoints(p, q)
            | VerticalPoints(p, q)
            | Distance(p, q, _)
            | HorizontalDistance(p, q, _)
            | VerticalDistance(p, q, _)
            | Symmetric(p, q, _)
            | LinearCopy { src: p, dst: q, .. } => vec![p, q],
            CircularCopy { src, dst, center, .. } => vec![src, dst, center],
            _ => vec![],
        }
    }

    /// Entities referenced directly.
    pub fn entities(&self) -> Vec<EntityId> {
        match *self {
            Horizontal(e) | Vertical(e) | Radius(e, _) | Diameter(e, _) | ArcConsistency(e) => vec![e],
            Midpoint(_, e) | Symmetric(_, _, e) | PointOnCurve(_, e) | PointLineDistance(_, e, _) => vec![e],
            Parallel(a, b)
            | Perpendicular(a, b)
            | Collinear(a, b)
            | Equal(a, b)
            | Concentric(a, b)
            | Tangent(a, b)
            | Angle(a, b, _) => vec![a, b],
            _ => vec![],
        }
    }

    /// Rewrites ids after points/entities were removed; `None` when a referenced item is gone.
    pub(crate) fn remap(
        &self,
        fp: &impl Fn(PointId) -> Option<PointId>,
        fe: &impl Fn(EntityId) -> Option<EntityId>,
    ) -> Option<Self> {
        Some(match *self {
            Fix(p, at) => Fix(fp(p)?, at),
            Coincident(p, q) => Coincident(fp(p)?, fp(q)?),
            Horizontal(e) => Horizontal(fe(e)?),
            Vertical(e) => Vertical(fe(e)?),
            HorizontalPoints(p, q) => HorizontalPoints(fp(p)?, fp(q)?),
            VerticalPoints(p, q) => VerticalPoints(fp(p)?, fp(q)?),
            Parallel(a, b) => Parallel(fe(a)?, fe(b)?),
            Perpendicular(a, b) => Perpendicular(fe(a)?, fe(b)?),
            Collinear(a, b) => Collinear(fe(a)?, fe(b)?),
            Equal(a, b) => Equal(fe(a)?, fe(b)?),
            Midpoint(p, e) => Midpoint(fp(p)?, fe(e)?),
            Concentric(a, b) => Concentric(fe(a)?, fe(b)?),
            Symmetric(p, q, e) => Symmetric(fp(p)?, fp(q)?, fe(e)?),
            PointOnCurve(p, e) => PointOnCurve(fp(p)?, fe(e)?),
            Tangent(a, b) => Tangent(fe(a)?, fe(b)?),
            Distance(p, q, d) => Distance(fp(p)?, fp(q)?, d),
            HorizontalDistance(p, q, d) => HorizontalDistance(fp(p)?, fp(q)?, d),
            VerticalDistance(p, q, d) => VerticalDistance(fp(p)?, fp(q)?, d),
            PointLineDistance(p, e, d) => PointLineDistance(fp(p)?, fe(e)?, d),
            Angle(a, b, d) => Angle(fe(a)?, fe(b)?, d),
            Radius(e, d) => Radius(fe(e)?, d),
            Diameter(e, d) => Diameter(fe(e)?, d),
            LinearCopy {
                src,
                dst,
                step,
                spacing,
                angle,
                group,
            } => LinearCopy {
                src: fp(src)?,
                dst: fp(dst)?,
                step,
                spacing,
                angle,
                group,
            },
            CircularCopy {
                src,
                dst,
                center,
                step,
                angle,
                group,
            } => CircularCopy {
                src: fp(src)?,
                dst: fp(dst)?,
                center: fp(center)?,
                step,
                angle,
                group,
            },
            ArcConsistency(e) => ArcConsistency(fe(e)?),
        })
    }

    /// Whether the referenced items exist and have the right kinds.
    pub fn is_valid(&self, sk: &Sketch) -> bool {
        let pts = self.points();
        let ents = self.entities();
        if pts.iter().any(|p| p.0 >= sk.points.len()) || ents.iter().any(|e| e.0 >= sk.entities.len()) {
            return false;
        }
        let line = |e: EntityId| sk.entities[e.0].is_line();
        let round = |e: EntityId| sk.entities[e.0].is_round();
        let distinct_pts = pts.len() < 2 || pts[0] != pts[1];
        let distinct_ents = ents.len() < 2 || ents[0] != ents[1];
        distinct_pts
            && distinct_ents
            && match *self {
                Horizontal(e) | Vertical(e) | Midpoint(_, e) | Symmetric(_, _, e) | PointLineDistance(_, e, _) => {
                    line(e)
                }
                Parallel(a, b) | Perpendicular(a, b) | Collinear(a, b) | Angle(a, b, _) => line(a) && line(b),
                Equal(a, b) => (line(a) && line(b)) || (round(a) && round(b)),
                Concentric(a, b) => round(a) && round(b),
                PointOnCurve(_, e) => line(e) || round(e),
                Tangent(a, b) => line(a) && round(b),
                Radius(e, _) | Diameter(e, _) => round(e),
                ArcConsistency(e) => matches!(sk.entities[e.0].geom, Geometry::Arc { .. }),
                CircularCopy { src, dst, center, .. } => src != center && dst != center,
                _ => true,
            }
            && !matches!(*self, Midpoint(p, e) if sk.entities[e.0].points().contains(&p))
    }

    /// Indices into the sketch parameter vector this constraint reads, in the order [`Self::eval`] expects.
    pub(crate) fn locals(&self, sk: &Sketch) -> Vec<usize> {
        let mut v = Vec::with_capacity(8);
        let pt = |v: &mut Vec<usize>, p: PointId| {
            let i = sk.points[p.0];
            v.extend([i, i + 1]);
        };
        let ent = |v: &mut Vec<usize>, e: EntityId| match sk.entities[e.0].geom {
            Geometry::Line { a, b } => {
                pt(v, a);
                pt(v, b);
            }
            Geometry::Circle { center, radius } => {
                pt(v, center);
                v.push(radius);
            }
            Geometry::Arc { center, start, .. } => {
                pt(v, center);
                pt(v, start);
            }
            Geometry::Spline { .. } => unreachable!("splines take no relations"),
        };
        match *self {
            Concentric(a, b) => {
                pt(&mut v, sk.entities[a.0].center().expect("round"));
                pt(&mut v, sk.entities[b.0].center().expect("round"));
            }
            ArcConsistency(e) => {
                let Geometry::Arc { center, start, end } = sk.entities[e.0].geom else {
                    unreachable!()
                };
                [center, start, end].into_iter().for_each(|p| pt(&mut v, p));
            }
            _ => {
                self.points().into_iter().for_each(|p| pt(&mut v, p));
                self.entities().into_iter().for_each(|e| ent(&mut v, e));
            }
        }
        v
    }

    /// Residuals from the local parameter values listed by [`Self::locals`].
    pub(crate) fn eval<T: Scalar>(&self, sk: &Sketch, v: &[T], out: &mut Vec<T>) {
        let mut c = Cursor { v, i: 0 };
        let is_arc = |e: EntityId| matches!(sk.entities[e.0].geom, Geometry::Arc { .. });
        let k = T::cst;
        match *self {
            Fix(_, at) => {
                let p = c.pt();
                out.extend([p[0] - k(at[0]), p[1] - k(at[1])]);
            }
            Coincident(..) => {
                let (p, q) = (c.pt(), c.pt());
                out.extend([p[0] - q[0], p[1] - q[1]]);
            }
            Horizontal(_) => {
                let (a, b) = c.line();
                out.push(a[1] - b[1]);
            }
            Vertical(_) => {
                let (a, b) = c.line();
                out.push(a[0] - b[0]);
            }
            HorizontalPoints(..) => {
                let (p, q) = (c.pt(), c.pt());
                out.push(p[1] - q[1]);
            }
            VerticalPoints(..) => {
                let (p, q) = (c.pt(), c.pt());
                out.push(p[0] - q[0]);
            }
            Parallel(..) | Perpendicular(..) | Collinear(..) | Angle(..) => {
                let (a1, b1) = c.line();
                let (a2, b2) = c.line();
                let (u, w) = (sub(b1, a1), sub(b2, a2));
                let lens = len(u) * len(w);
                match *self {
                    Parallel(..) => out.push(cross(u, w) / lens),
                    Perpendicular(..) => out.push(dot(u, w) / lens),
                    Collinear(..) => out.extend([cross(u, w) / lens, cross(u, sub(a2, a1)) / len(u)]),
                    Angle(.., target) => {
                        let actual = cross(u, w).atan2(dot(u, w));
                        // Wrap into [-PI, PI) so 359° vs 1° is a small error, not a huge one.
                        let turns = ((actual.value() - target + PI) / TAU).floor();
                        out.push(actual - k(target + turns * TAU));
                    }
                    _ => unreachable!(),
                }
            }
            Equal(a, b) => {
                if sk.entities[a.0].is_line() {
                    let (a1, b1) = c.line();
                    let (a2, b2) = c.line();
                    out.push(len(sub(b1, a1)) - len(sub(b2, a2)));
                } else {
                    let (_, r1) = c.round(is_arc(a));
                    let (_, r2) = c.round(is_arc(b));
                    out.push(r1 - r2);
                }
            }
            Midpoint(..) => {
                let p = c.pt();
                let (a, b) = c.line();
                out.extend([p[0] - (a[0] + b[0]) * k(0.5), p[1] - (a[1] + b[1]) * k(0.5)]);
            }
            Concentric(..) => {
                let (p, q) = (c.pt(), c.pt());
                out.extend([p[0] - q[0], p[1] - q[1]]);
            }
            Symmetric(..) => {
                let (p, q) = (c.pt(), c.pt());
                let (a, b) = c.line();
                let u = sub(b, a);
                let m = [(p[0] + q[0]) * k(0.5), (p[1] + q[1]) * k(0.5)];
                out.extend([cross(u, sub(m, a)) / len(u), dot(sub(q, p), u) / len(u)]);
            }
            PointOnCurve(_, e) => {
                let p = c.pt();
                if sk.entities[e.0].is_line() {
                    let (a, b) = c.line();
                    let u = sub(b, a);
                    out.push(cross(u, sub(p, a)) / len(u));
                } else {
                    let (center, r) = c.round(is_arc(e));
                    out.push(len(sub(p, center)) - r);
                }
            }
            Tangent(l, e) => {
                let (a, b) = c.line();
                let (center, r) = c.round(is_arc(e));
                let u = sub(b, a);
                // Sharing an endpoint, tangency is "radius ⟂ line" there; the distance form would be
                // degenerate (the line can only touch the circle from outside).
                match sk.shared_endpoint(l, e) {
                    Some(first) => out.push(dot(u, sub(if first { a } else { b }, center)) / len(u)),
                    None => out.push((cross(u, sub(center, a)) / len(u)).abs() - r),
                }
            }
            Distance(.., d) => {
                let (p, q) = (c.pt(), c.pt());
                out.push(len(sub(q, p)) - k(d));
            }
            HorizontalDistance(.., d) => {
                let (p, q) = (c.pt(), c.pt());
                out.push((q[0] - p[0]).abs() - k(d));
            }
            VerticalDistance(.., d) => {
                let (p, q) = (c.pt(), c.pt());
                out.push((q[1] - p[1]).abs() - k(d));
            }
            PointLineDistance(.., d) => {
                let p = c.pt();
                let (a, b) = c.line();
                let u = sub(b, a);
                out.push((cross(u, sub(p, a)) / len(u)).abs() - k(d));
            }
            Radius(e, d) => {
                let (_, r) = c.round(is_arc(e));
                out.push(r - k(d));
            }
            Diameter(e, d) => {
                let (_, r) = c.round(is_arc(e));
                out.push(r * k(2.0) - k(d));
            }
            LinearCopy {
                step, spacing, angle, ..
            } => {
                let (src, dst) = (c.pt(), c.pt());
                let off = step as f64 * spacing;
                out.extend([
                    dst[0] - src[0] - k(off * angle.cos()),
                    dst[1] - src[1] - k(off * angle.sin()),
                ]);
            }
            CircularCopy { step, angle, .. } => {
                let (src, dst, center) = (c.pt(), c.pt(), c.pt());
                let (s, co) = (step as f64 * angle).sin_cos();
                let rel = sub(src, center);
                out.extend([
                    dst[0] - center[0] - (rel[0] * k(co) - rel[1] * k(s)),
                    dst[1] - center[1] - (rel[0] * k(s) + rel[1] * k(co)),
                ]);
            }
            ArcConsistency(_) => {
                let (center, start, end) = (c.pt(), c.pt(), c.pt());
                out.push(len(sub(start, center)) - len(sub(end, center)));
            }
        }
    }
}

/// Walks the local parameter values in the order [`Constraint::locals`] pushed them.
struct Cursor<'a, T> {
    v: &'a [T],
    i: usize,
}

impl Sketch {
    /// If line `l` and round entity `e` share a point, whether it is the line's first point.
    fn shared_endpoint(&self, l: EntityId, e: EntityId) -> Option<bool> {
        let (a, b) = self.entities[l.0].endpoints()?;
        let (s, t) = self.entities[e.0].endpoints()?;
        if a == s || a == t {
            Some(true)
        } else if b == s || b == t {
            Some(false)
        } else {
            None
        }
    }
}

impl<T: Scalar> Cursor<'_, T> {
    fn pt(&mut self) -> [T; 2] {
        self.i += 2;
        [self.v[self.i - 2], self.v[self.i - 1]]
    }

    fn line(&mut self) -> ([T; 2], [T; 2]) {
        (self.pt(), self.pt())
    }

    /// Center and radius of a circle (radius parameter) or an arc (distance to its start).
    fn round(&mut self, is_arc: bool) -> ([T; 2], T) {
        let c = self.pt();
        if is_arc {
            let s = self.pt();
            (c, len(sub(s, c)))
        } else {
            self.i += 1;
            (c, self.v[self.i - 1])
        }
    }
}

fn sub<T: Scalar>(a: [T; 2], b: [T; 2]) -> [T; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot<T: Scalar>(a: [T; 2], b: [T; 2]) -> T {
    a[0] * b[0] + a[1] * b[1]
}

fn cross<T: Scalar>(a: [T; 2], b: [T; 2]) -> T {
    a[0] * b[1] - a[1] * b[0]
}

fn len<T: Scalar>(a: [T; 2]) -> T {
    a[0].hypot(a[1])
}
