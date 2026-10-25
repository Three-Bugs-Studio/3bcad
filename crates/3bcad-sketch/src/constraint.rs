use std::f64::consts::PI;

use crate::{Entity, EntityId, PointId, Sketch};

/// Geometric and dimensional constraints. Each one contributes residuals that
/// are zero when it is satisfied.
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    /// Grounds a point (sketch origin, etc).
    Fix(PointId, [f64; 2]),
    Coincident(PointId, PointId),
    Horizontal(EntityId),
    Vertical(EntityId),
    Distance(PointId, PointId, f64),
    /// Signed angle from line `a` to line `b`, radians.
    Angle(EntityId, EntityId, f64),
    /// Line tangent to circle.
    Tangent(EntityId, EntityId),
    Radius(EntityId, f64),
}

impl Constraint {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Fix(..) => "Fix",
            Self::Coincident(..) => "Coincident",
            Self::Horizontal(..) => "Horizontal",
            Self::Vertical(..) => "Vertical",
            Self::Distance(..) => "Distance",
            Self::Angle(..) => "Angle",
            Self::Tangent(..) => "Tangent",
            Self::Radius(..) => "Radius",
        }
    }

    /// The driving value of a dimensional constraint.
    pub fn dimension(&self) -> Option<f64> {
        match *self {
            Self::Distance(_, _, d) | Self::Angle(_, _, d) | Self::Radius(_, d) => Some(d),
            _ => None,
        }
    }

    pub fn set_dimension(&mut self, value: f64) {
        if let Self::Distance(_, _, d) | Self::Angle(_, _, d) | Self::Radius(_, d) = self {
            *d = value;
        }
    }

    pub(crate) fn residuals(&self, sk: &Sketch, p: &[f64], out: &mut Vec<f64>) {
        let pt = |id| sk.pt(p, id);
        match *self {
            Self::Fix(a, at) => {
                let a = pt(a);
                out.extend([a[0] - at[0], a[1] - at[1]]);
            }
            Self::Coincident(a, b) => {
                let (a, b) = (pt(a), pt(b));
                out.extend([a[0] - b[0], a[1] - b[1]]);
            }
            Self::Horizontal(l) => {
                let (a, b) = sk.line(l);
                out.push(pt(a)[1] - pt(b)[1]);
            }
            Self::Vertical(l) => {
                let (a, b) = sk.line(l);
                out.push(pt(a)[0] - pt(b)[0]);
            }
            Self::Distance(a, b, d) => {
                let (a, b) = (pt(a), pt(b));
                out.push((b[0] - a[0]).hypot(b[1] - a[1]) - d);
            }
            Self::Angle(l1, l2, angle) => {
                let dir = |l| {
                    let (a, b) = sk.line(l);
                    let (a, b) = (pt(a), pt(b));
                    [b[0] - a[0], b[1] - a[1]]
                };
                let (u, v) = (dir(l1), dir(l2));
                let actual = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
                // Wrap into (-PI, PI] so 359 deg vs 1 deg is a small error, not a huge one.
                out.push((actual - angle + PI).rem_euclid(2.0 * PI) - PI);
            }
            Self::Tangent(l, c) => {
                let (a, b) = sk.line(l);
                let (a, b) = (pt(a), pt(b));
                let (center, r) = sk.circle_entity(c);
                let center = pt(center);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let dist = (dx * (center[1] - a[1]) - dy * (center[0] - a[0])).abs() / dx.hypot(dy);
                out.push(dist - p[r]);
            }
            Self::Radius(c, r) => out.push(p[sk.circle_entity(c).1] - r),
        }
    }
}

impl Sketch {
    fn line(&self, e: EntityId) -> (PointId, PointId) {
        match self.entities[e.0] {
            Entity::Line { a, b } => (a, b),
            ref other => panic!("constraint expects a line, got {other:?}"),
        }
    }

    fn circle_entity(&self, e: EntityId) -> (PointId, usize) {
        match self.entities[e.0] {
            Entity::Circle { center, radius } => (center, radius),
            ref other => panic!("constraint expects a circle, got {other:?}"),
        }
    }
}
