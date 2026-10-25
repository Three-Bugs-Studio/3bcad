//! 2D parametric sketcher: points, lines and circles driven by a
//! Levenberg-Marquardt constraint solver.

pub mod constraint;
pub mod entity;
pub mod solver;

pub use constraint::Constraint;
pub use entity::{Entity, EntityId, PointId};
pub use solver::SolveReport;

/// A sketch on the XY plane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sketch {
    params: Vec<f64>,
    /// Offset of each point's x coordinate in `params` (y follows it).
    points: Vec<usize>,
    pub entities: Vec<Entity>,
    pub constraints: Vec<Constraint>,
}

/// A closed region that features can extrude.
#[derive(Clone, Debug, PartialEq)]
pub enum Profile {
    Polygon(Vec<[f64; 2]>),
    Circle { center: [f64; 2], radius: f64 },
}

impl Sketch {
    /// Fully-defined rectangle with one corner grounded at the origin.
    pub fn rectangle(width: f64, height: f64) -> Self {
        let mut s = Self::default();
        let p = [
            s.add_point(0.0, 0.0),
            s.add_point(width, 0.0),
            s.add_point(width, height),
            s.add_point(0.0, height),
        ];
        let l: Vec<_> = (0..4).map(|i| s.add_line(p[i], p[(i + 1) % 4])).collect();
        s.constraints.extend([
            Constraint::Fix(p[0], [0.0, 0.0]),
            Constraint::Horizontal(l[0]),
            Constraint::Vertical(l[1]),
            Constraint::Horizontal(l[2]),
            Constraint::Vertical(l[3]),
            Constraint::Distance(p[0], p[1], width),
            Constraint::Distance(p[1], p[2], height),
        ]);
        s
    }

    /// Fully-defined circle.
    pub fn circle(center: [f64; 2], radius: f64) -> Self {
        let mut s = Self::default();
        let c = s.add_point(center[0], center[1]);
        let e = s.add_circle(c, radius);
        s.constraints
            .extend([Constraint::Fix(c, center), Constraint::Radius(e, radius)]);
        s
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> PointId {
        self.points.push(self.params.len());
        self.params.extend([x, y]);
        PointId(self.points.len() - 1)
    }

    pub fn add_line(&mut self, a: PointId, b: PointId) -> EntityId {
        self.entities.push(Entity::Line { a, b });
        EntityId(self.entities.len() - 1)
    }

    pub fn add_circle(&mut self, center: PointId, radius: f64) -> EntityId {
        self.params.push(radius);
        self.entities.push(Entity::Circle {
            center,
            radius: self.params.len() - 1,
        });
        EntityId(self.entities.len() - 1)
    }

    pub fn point(&self, id: PointId) -> [f64; 2] {
        self.pt(&self.params, id)
    }

    fn pt(&self, params: &[f64], id: PointId) -> [f64; 2] {
        let i = self.points[id.0];
        [params[i], params[i + 1]]
    }

    /// Moves the geometry to satisfy every constraint.
    pub fn solve(&mut self) -> SolveReport {
        let mut params = std::mem::take(&mut self.params);
        let report = solver::levenberg_marquardt(
            &mut params,
            |p| {
                let mut out = Vec::new();
                for c in &self.constraints {
                    c.residuals(self, p, &mut out);
                }
                out
            },
            200,
            1e-9,
        );
        self.params = params;
        report
    }

    /// The region to extrude.
    // shortcut: one circle, or one loop of lines listed head-to-tail; add loop detection for multi-loop / unordered profiles.
    pub fn profile(&self) -> Option<Profile> {
        if let [Entity::Circle { center, radius }] = self.entities.as_slice() {
            return Some(Profile::Circle {
                center: self.point(*center),
                radius: self.params[*radius],
            });
        }
        let pts: Vec<_> = self
            .entities
            .iter()
            .map(|e| match e {
                Entity::Line { a, .. } => Some(self.point(*a)),
                _ => None,
            })
            .collect::<Option<_>>()?;
        let closed = self
            .entities
            .iter()
            .zip(self.entities.iter().cycle().skip(1))
            .all(|pair| matches!(pair, (Entity::Line { b, .. }, Entity::Line { a, .. }) if a == b));
        (pts.len() >= 3 && closed).then_some(Profile::Polygon(pts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    #[test]
    fn rectangle_follows_dimension_edits() {
        let mut s = Sketch::rectangle(40.0, 30.0);
        s.constraints[5].set_dimension(55.0);
        s.constraints[6].set_dimension(12.5);
        assert!(s.solve().converged);
        let Some(Profile::Polygon(p)) = s.profile() else {
            panic!("no profile")
        };
        assert!(close(p[0], [0.0, 0.0]) && close(p[1], [55.0, 0.0]));
        assert!(close(p[2], [55.0, 12.5]) && close(p[3], [0.0, 12.5]));
    }

    #[test]
    fn line_snaps_tangent_to_circle() {
        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let circle = s.add_circle(c, 2.0);
        let (a, b) = (s.add_point(-5.0, 3.0), s.add_point(5.0, 3.5));
        let line = s.add_line(a, b);
        s.constraints.extend([
            Constraint::Fix(c, [0.0, 0.0]),
            Constraint::Radius(circle, 2.0),
            Constraint::Horizontal(line),
            Constraint::Tangent(line, circle),
        ]);
        assert!(s.solve().converged);
        assert!((s.point(a)[1] - 2.0).abs() < 1e-6 && (s.point(b)[1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn angle_between_lines() {
        let mut s = Sketch::default();
        let o = s.add_point(0.0, 0.0);
        let (x, y) = (s.add_point(10.0, 0.0), s.add_point(5.0, 5.0));
        let (l1, l2) = (s.add_line(o, x), s.add_line(o, y));
        s.constraints.extend([
            Constraint::Fix(o, [0.0, 0.0]),
            Constraint::Fix(x, [10.0, 0.0]),
            Constraint::Distance(o, y, 10.0),
            Constraint::Angle(l1, l2, 60f64.to_radians()),
        ]);
        assert!(s.solve().converged);
        let p = s.point(y);
        assert!(close(p, [5.0, 75f64.sqrt()]), "{p:?}");
    }

    #[test]
    fn contradictory_sketch_does_not_converge() {
        let mut s = Sketch::rectangle(10.0, 10.0);
        let p = PointId(0);
        s.constraints.push(Constraint::Fix(p, [3.0, 0.0])); // fights the origin fix
        assert!(!s.solve().converged);
    }
}
