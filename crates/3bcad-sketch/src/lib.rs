//! 2D parametric sketcher: points, lines, circles, arcs and splines driven by a sparse
//! Levenberg-Marquardt constraint solver with exact (auto-differentiated) Jacobians.

pub mod constraint;
pub mod entity;
pub mod geom;
pub mod inference;
pub mod profile;
pub mod scalar;
pub mod solver;
pub mod tools;

pub use constraint::Constraint;
pub use entity::{Entity, EntityId, Geometry, PointId};
pub use profile::{Region, Segment};
pub use solver::SolveReport;

use geom::P2;
use scalar::Dual;

/// A 2D sketch. Coordinates are in the sketch plane's local frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sketch {
    params: Vec<f64>,
    /// Offset of each point's x coordinate in `params` (y follows it).
    pub(crate) points: Vec<usize>,
    pub(crate) entities: Vec<Entity>,
    constraints: Vec<Constraint>,
    /// Where each dimension's label sits; parallel to `constraints`.
    labels: Vec<Option<P2>>,
}

/// How well the sketch is defined, like SolidWorks' status bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    UnderDefined,
    FullyDefined,
    /// Conflicting or redundant relations.
    OverDefined,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    pub status: Status,
    /// Remaining degrees of freedom.
    pub dof: usize,
    /// Per point: it can still move.
    pub free_points: Vec<bool>,
    /// Per entity: some of its geometry can still move.
    pub free_entities: Vec<bool>,
    /// Per entity: part of a conflicting or redundant group.
    pub over_entities: Vec<bool>,
    /// Per relation: part of a conflicting or redundant group.
    pub over_constraints: Vec<bool>,
}

impl Sketch {
    /// Fully-defined rectangle with one corner grounded at the origin.
    pub fn rectangle(width: f64, height: f64) -> Self {
        let mut s = Self::default();
        let p0 = s.add_point(0.0, 0.0);
        let p2 = s.add_point(width, height);
        s.add_rectangle(p0, p2).expect("non-degenerate rectangle");
        let p1 = PointId(2);
        s.constrain(Constraint::Fix(p0, [0.0, 0.0]));
        s.constrain(Constraint::Distance(p0, p1, width));
        s.constrain(Constraint::Distance(p1, p2, height));
        s
    }

    /// Fully-defined circle.
    pub fn circle(center: P2, radius: f64) -> Self {
        let mut s = Self::default();
        let c = s.add_point(center[0], center[1]);
        let e = s.add_circle(c, radius);
        s.constrain(Constraint::Fix(c, center));
        s.constrain(Constraint::Radius(e, radius));
        s
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> PointId {
        self.points.push(self.params.len());
        self.params.extend([x, y]);
        PointId(self.points.len() - 1)
    }

    pub fn add_line(&mut self, a: PointId, b: PointId) -> EntityId {
        self.push_entity(Geometry::Line { a, b })
    }

    pub fn add_circle(&mut self, center: PointId, radius: f64) -> EntityId {
        self.params.push(radius);
        let radius = self.params.len() - 1;
        self.push_entity(Geometry::Circle { center, radius })
    }

    /// Counter-clockwise arc; `end` is pulled onto the circle through `start` by the solver.
    pub fn add_arc(&mut self, center: PointId, start: PointId, end: PointId) -> EntityId {
        self.push_entity(Geometry::Arc { center, start, end })
    }

    pub fn add_spline(&mut self, points: Vec<PointId>) -> EntityId {
        assert!(points.len() >= 2, "a spline needs two points");
        self.push_entity(Geometry::Spline { points })
    }

    fn push_entity(&mut self, geom: Geometry) -> EntityId {
        self.entities.push(Entity::new(geom));
        EntityId(self.entities.len() - 1)
    }

    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    pub fn point_ids(&self) -> impl Iterator<Item = PointId> {
        (0..self.points.len()).map(PointId)
    }

    pub fn point(&self, id: PointId) -> P2 {
        let i = self.points[id.0];
        [self.params[i], self.params[i + 1]]
    }

    pub fn set_point(&mut self, id: PointId, p: P2) {
        let i = self.points[id.0];
        self.params[i] = p[0];
        self.params[i + 1] = p[1];
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn entity(&self, id: EntityId) -> &Entity {
        &self.entities[id.0]
    }

    pub fn entity_ids(&self) -> impl Iterator<Item = EntityId> {
        (0..self.entities.len()).map(EntityId)
    }

    pub(crate) fn entity_mut(&mut self, id: EntityId) -> &mut Entity {
        &mut self.entities[id.0]
    }

    pub fn set_construction(&mut self, id: EntityId, construction: bool) {
        self.entities[id.0].construction = construction;
    }

    /// Radius of a circle or arc.
    pub fn radius(&self, id: EntityId) -> f64 {
        match self.entities[id.0].geom {
            Geometry::Circle { radius, .. } => self.params[radius],
            Geometry::Arc { center, start, .. } => geom::dist(self.point(center), self.point(start)),
            _ => panic!("radius of a non-round entity"),
        }
    }

    /// Entities that use `p` as an endpoint, center or spline point.
    pub fn entities_at(&self, p: PointId) -> Vec<EntityId> {
        self.entity_ids()
            .filter(|&e| self.entity(e).points().contains(&p))
            .collect()
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// Whether `c` could be added: valid kinds and not already present.
    pub fn accepts(&self, c: &Constraint) -> bool {
        c.is_valid(self) && !self.constraints.contains(c)
    }

    /// Adds a relation or dimension and returns its index. Panics on invalid input; check with [`Self::accepts`].
    pub fn constrain(&mut self, c: Constraint) -> usize {
        assert!(c.is_valid(self), "invalid constraint {c:?}");
        self.constraints.push(c);
        self.labels.push(None);
        self.constraints.len() - 1
    }

    /// Changes a dimension; for pattern relations every instance of the pattern follows.
    pub fn set_dimension(&mut self, i: usize, value: f64) {
        match self.constraints[i].pattern_group() {
            Some(g) => self
                .constraints
                .iter_mut()
                .filter(|c| c.pattern_group() == Some(g))
                .for_each(|c| c.set_value(value)),
            None => self.constraints[i].set_value(value),
        }
    }

    pub fn label(&self, i: usize) -> Option<P2> {
        self.labels[i]
    }

    pub fn set_label(&mut self, i: usize, at: P2) {
        self.labels[i] = Some(at);
    }

    /// The current value of the quantity a dimension measures (whatever its target is).
    pub fn measure(&self, c: &Constraint) -> f64 {
        let mut zero = c.clone();
        zero.set_value(0.0);
        let local: Vec<f64> = zero.locals(self).iter().map(|&i| self.params[i]).collect();
        let mut out = Vec::new();
        zero.eval(self, &local, &mut out);
        out[0]
    }

    pub fn remove_constraint(&mut self, i: usize) {
        self.remove(&[], &[], &[i]);
    }

    /// Deletes points, entities and relations. Entities using a deleted point go too, as do points
    /// only used by deleted entities and relations that referenced anything deleted.
    pub fn remove(&mut self, points: &[PointId], entities: &[EntityId], constraints: &[usize]) {
        let (np, ne) = (self.points.len(), self.entities.len());
        let mut del_p = vec![false; np];
        points.iter().for_each(|p| del_p[p.0] = true);
        let mut del_e = vec![false; ne];
        entities.iter().for_each(|e| del_e[e.0] = true);
        for (i, e) in self.entities.iter().enumerate() {
            del_e[i] |= e.points().iter().any(|p| del_p[p.0]);
        }
        let mut kept_use = vec![false; np];
        let mut del_use = vec![false; np];
        for (i, e) in self.entities.iter().enumerate() {
            for p in e.points() {
                if del_e[i] {
                    del_use[p.0] = true;
                } else {
                    kept_use[p.0] = true;
                }
            }
        }
        (0..np).for_each(|p| del_p[p] |= del_use[p] && !kept_use[p]);

        let mut params = Vec::new();
        let mut point_map = vec![None; np];
        let mut new_points = Vec::new();
        for p in 0..np {
            if !del_p[p] {
                point_map[p] = Some(PointId(new_points.len()));
                new_points.push(params.len());
                let q = self.point(PointId(p));
                params.extend(q);
            }
        }
        let fp = |p: PointId| point_map[p.0];
        let mut entity_map = vec![None; ne];
        let mut new_entities = Vec::new();
        for (i, e) in self.entities.iter().enumerate() {
            if del_e[i] {
                continue;
            }
            let geom = match &e.geom {
                Geometry::Line { a, b } => Geometry::Line {
                    a: fp(*a).unwrap(),
                    b: fp(*b).unwrap(),
                },
                Geometry::Circle { center, radius } => {
                    params.push(self.params[*radius]);
                    Geometry::Circle {
                        center: fp(*center).unwrap(),
                        radius: params.len() - 1,
                    }
                }
                Geometry::Arc { center, start, end } => Geometry::Arc {
                    center: fp(*center).unwrap(),
                    start: fp(*start).unwrap(),
                    end: fp(*end).unwrap(),
                },
                Geometry::Spline { points } => Geometry::Spline {
                    points: points.iter().map(|p| fp(*p).unwrap()).collect(),
                },
            };
            entity_map[i] = Some(EntityId(new_entities.len()));
            new_entities.push(Entity {
                geom,
                construction: e.construction,
            });
        }
        let fe = |e: EntityId| entity_map[e.0];
        let mut new_constraints = Vec::new();
        let mut new_labels = Vec::new();
        for (i, c) in self.constraints.iter().enumerate() {
            if constraints.contains(&i) {
                continue;
            }
            if let Some(c) = c.remap(&fp, &fe) {
                new_constraints.push(c);
                new_labels.push(self.labels[i]);
            }
        }
        *self = Sketch {
            params,
            points: new_points,
            entities: new_entities,
            constraints: new_constraints,
            labels: new_labels,
        };
    }

    /// Moves the geometry to satisfy every relation, changing it as little as possible.
    pub fn solve(&mut self) -> SolveReport {
        let mut params = std::mem::take(&mut self.params);
        let report = {
            let sys = SketchSystem::new(self);
            solver::solve(&sys, &mut params, 200, 1e-9)
        };
        self.params = params;
        report
    }

    /// Drags points: they follow the cursor exactly when the relations allow it; otherwise the
    /// relations win and pull them back (fully-defined geometry does not move).
    pub fn drag(&mut self, moves: &[(PointId, P2)]) -> SolveReport {
        let n = self.constraints.len();
        for &(p, to) in moves {
            self.set_point(p, to);
            self.constraints.push(Constraint::Fix(p, to));
            self.labels.push(None);
        }
        let pinned = self.solve();
        self.constraints.truncate(n);
        self.labels.truncate(n);
        if pinned.converged {
            pinned
        } else {
            self.solve()
        }
    }

    /// Degrees of freedom and definition status at the current geometry (call after [`Self::solve`]).
    pub fn analyze(&self) -> Analysis {
        let sys = SketchSystem::new(self);
        let n = self.params.len();
        // Parameters no entity uses (e.g. the radius of a circle trimmed into an arc) do not count.
        let mut live = vec![false; n];
        for &i in &self.points {
            live[i] = true;
            live[i + 1] = true;
        }
        for e in &self.entities {
            if let Geometry::Circle { radius, .. } = e.geom {
                live[radius] = true;
            }
        }
        let mut free = live.clone();
        let mut over = vec![false; n];
        let mut over_terms = vec![false; sys.cons.len()];
        let mut dof = 0;
        let mut col_of = vec![usize::MAX; n];
        let mut in_comp = vec![false; n];
        for comp in solver::components(&sys, n) {
            for (i, &p) in comp.params.iter().enumerate() {
                col_of[p] = i;
                in_comp[p] = true;
            }
            let jac = solver::jacobian(&sys, &comp, &self.params, &col_of);
            let rank = solver::rank(&jac);
            let conflicting = jac.residuals.iter().map(|r| r * r).sum::<f64>().sqrt() > 1e-6;
            // Only user relations count: implicit arc relations are often implied (mirrored arcs).
            let user = solver::Jacobian {
                rows: (0..jac.rows.len())
                    .filter(|&i| jac.row_terms[i] < self.constraints.len())
                    .map(|i| jac.rows[i].clone())
                    .collect(),
                residuals: Vec::new(),
                row_terms: Vec::new(),
                cols: jac.cols,
            };
            let user_rank = solver::rank(&user);
            let redundant = user_rank.rank < user_rank.rows;
            dof += comp.params.len() - rank.rank;
            for (i, &p) in comp.params.iter().enumerate() {
                free[p] = rank.free[i];
                over[p] = conflicting || redundant;
            }
            if conflicting || redundant {
                comp.terms.iter().for_each(|&t| over_terms[t] = true);
            }
        }
        dof += (0..n).filter(|&p| live[p] && !in_comp[p]).count();
        let point_free = |p: PointId| free[self.points[p.0]] || free[self.points[p.0] + 1];
        let entity_params = |e: &Entity| -> Vec<usize> {
            let mut v: Vec<usize> = e
                .points()
                .iter()
                .flat_map(|p| [self.points[p.0], self.points[p.0] + 1])
                .collect();
            if let Geometry::Circle { radius, .. } = e.geom {
                v.push(radius);
            }
            v
        };
        let over_entities: Vec<bool> = self
            .entities
            .iter()
            .map(|e| entity_params(e).iter().any(|&p| over[p]))
            .collect();
        let status = if over_terms.iter().any(|&o| o) {
            Status::OverDefined
        } else if dof == 0 {
            Status::FullyDefined
        } else {
            Status::UnderDefined
        };
        Analysis {
            status,
            dof,
            free_points: self.point_ids().map(point_free).collect(),
            free_entities: self
                .entities
                .iter()
                .map(|e| entity_params(e).iter().any(|&p| free[p]))
                .collect(),
            over_constraints: over_terms[..self.constraints.len()].to_vec(),
            over_entities: if status == Status::OverDefined {
                over_entities
            } else {
                vec![false; self.entities.len()]
            },
        }
    }

    /// Relations whose removal alone would make an over-defined sketch solvable (SketchXpert-lite).
    pub fn diagnose(&self) -> Vec<usize> {
        let analysis = self.analyze();
        if analysis.status != Status::OverDefined {
            return Vec::new();
        }
        (0..self.constraints.len())
            .filter(|&i| analysis.over_constraints[i])
            .filter(|&i| {
                let mut s = self.clone();
                s.remove_constraint(i);
                s.solve().converged && s.analyze().status != Status::OverDefined
            })
            .collect()
    }
}

/// The sketch as a solver problem: user relations plus implicit arc relations.
struct SketchSystem<'a> {
    sk: &'a Sketch,
    cons: Vec<Constraint>,
    locals: Vec<Vec<usize>>,
}

impl<'a> SketchSystem<'a> {
    fn new(sk: &'a Sketch) -> Self {
        let implicit = sk
            .entity_ids()
            .filter(|&e| matches!(sk.entity(e).geom, Geometry::Arc { .. }))
            .map(Constraint::ArcConsistency);
        let cons: Vec<Constraint> = sk.constraints.iter().cloned().chain(implicit).collect();
        let locals = cons.iter().map(|c| c.locals(sk)).collect();
        Self { sk, cons, locals }
    }
}

impl solver::System for SketchSystem<'_> {
    fn terms(&self) -> usize {
        self.cons.len()
    }
    fn term_params(&self, t: usize) -> &[usize] {
        &self.locals[t]
    }
    fn eval(&self, t: usize, local: &[f64], out: &mut Vec<f64>) {
        self.cons[t].eval(self.sk, local, out);
    }
    fn eval_dual(&self, t: usize, local: &[Dual], out: &mut Vec<Dual>) {
        self.cons[t].eval(self.sk, local, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P2, b: P2) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    #[test]
    fn rectangle_follows_dimension_edits() {
        let mut s = Sketch::rectangle(40.0, 30.0);
        assert_eq!(s.analyze().status, Status::FullyDefined);
        s.set_dimension(5, 55.0);
        s.set_dimension(6, 12.5);
        assert!(s.solve().converged);
        let pts: Vec<P2> = s.point_ids().map(|p| s.point(p)).collect();
        assert!(close(pts[0], [0.0, 0.0]) && close(pts[2], [55.0, 0.0]));
        assert!(close(pts[1], [55.0, 12.5]) && close(pts[3], [0.0, 12.5]));
    }

    #[test]
    fn line_snaps_tangent_to_circle() {
        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let circle = s.add_circle(c, 2.0);
        let (a, b) = (s.add_point(-5.0, 3.0), s.add_point(5.0, 3.5));
        let line = s.add_line(a, b);
        for c in [
            Constraint::Fix(c, [0.0, 0.0]),
            Constraint::Radius(circle, 2.0),
            Constraint::Horizontal(line),
            Constraint::Tangent(line, circle),
        ] {
            s.constrain(c);
        }
        assert!(s.solve().converged);
        assert!((s.point(a)[1] - 2.0).abs() < 1e-6 && (s.point(b)[1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn angle_between_lines() {
        let mut s = Sketch::default();
        let o = s.add_point(0.0, 0.0);
        let (x, y) = (s.add_point(10.0, 0.0), s.add_point(5.0, 5.0));
        let (l1, l2) = (s.add_line(o, x), s.add_line(o, y));
        s.constrain(Constraint::Fix(o, [0.0, 0.0]));
        s.constrain(Constraint::Fix(x, [10.0, 0.0]));
        s.constrain(Constraint::Distance(o, y, 10.0));
        s.constrain(Constraint::Angle(l1, l2, 60f64.to_radians()));
        assert!(s.solve().converged);
        assert!(close(s.point(y), [5.0, 75f64.sqrt()]), "{:?}", s.point(y));
        assert_eq!(s.analyze().status, Status::FullyDefined);
    }

    #[test]
    fn contradictory_sketch_is_over_defined_and_diagnosed() {
        let mut s = Sketch::rectangle(10.0, 10.0);
        let fight = s.constrain(Constraint::Fix(PointId(0), [3.0, 0.0]));
        assert!(!s.solve().converged);
        let a = s.analyze();
        assert_eq!(a.status, Status::OverDefined);
        assert!(a.over_constraints[fight]);
        let fixes = s.diagnose();
        assert!(fixes.contains(&fight), "{fixes:?}");
        assert!(!fixes.contains(&0), "horizontal relation is not the culprit: {fixes:?}");
    }

    #[test]
    fn redundant_relation_is_over_defined() {
        let mut s = Sketch::rectangle(10.0, 5.0);
        // Parallel is already implied by the two horizontals.
        s.constrain(Constraint::Parallel(EntityId(0), EntityId(2)));
        assert!(s.solve().converged);
        assert_eq!(s.analyze().status, Status::OverDefined);
    }

    #[test]
    fn dof_counts_and_free_geometry() {
        let mut s = Sketch::default();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(4.0, 1.0);
        let l = s.add_line(a, b);
        assert_eq!(s.analyze().dof, 4);
        s.constrain(Constraint::Horizontal(l));
        s.solve();
        let an = s.analyze();
        assert_eq!((an.status, an.dof), (Status::UnderDefined, 3));
        s.constrain(Constraint::Fix(a, [0.0, 0.0]));
        s.constrain(Constraint::Distance(a, b, 5.0));
        s.solve();
        let an = s.analyze();
        assert_eq!((an.status, an.dof), (Status::FullyDefined, 0));
        assert!(an.free_points.iter().all(|f| !f) && an.free_entities.iter().all(|f| !f));
    }

    #[test]
    fn every_relation_has_exact_derivatives() {
        // Checks the dual-number Jacobian of every relation against central differences.
        let mut s = Sketch::default();
        let p: Vec<PointId> = [
            [0.0, 0.0],
            [3.0, 1.0],
            [1.0, 4.0],
            [5.0, 3.0],
            [2.0, 2.5],
            [6.0, -1.0],
            [-1.0, 2.0],
        ]
        .iter()
        .map(|q| s.add_point(q[0], q[1]))
        .collect();
        let l1 = s.add_line(p[0], p[1]);
        let l2 = s.add_line(p[2], p[3]);
        let circle = s.add_circle(p[4], 1.5);
        let arc = s.add_arc(p[5], p[6], p[1]);
        let all = [
            Constraint::Fix(p[0], [1.0, 2.0]),
            Constraint::Coincident(p[0], p[2]),
            Constraint::Horizontal(l1),
            Constraint::Vertical(l2),
            Constraint::HorizontalPoints(p[0], p[3]),
            Constraint::VerticalPoints(p[1], p[2]),
            Constraint::Parallel(l1, l2),
            Constraint::Perpendicular(l1, l2),
            Constraint::Collinear(l1, l2),
            Constraint::Equal(l1, l2),
            Constraint::Equal(circle, arc),
            Constraint::Midpoint(p[4], l1),
            Constraint::Concentric(circle, arc),
            Constraint::Symmetric(p[2], p[3], l1),
            Constraint::PointOnCurve(p[3], l1),
            Constraint::PointOnCurve(p[3], arc),
            Constraint::Tangent(l2, circle),
            Constraint::Tangent(l1, arc),
            Constraint::Distance(p[0], p[3], 2.0),
            Constraint::HorizontalDistance(p[0], p[3], 2.0),
            Constraint::VerticalDistance(p[0], p[3], 2.0),
            Constraint::PointLineDistance(p[4], l2, 1.0),
            Constraint::Angle(l1, l2, 0.3),
            Constraint::Radius(arc, 2.0),
            Constraint::Diameter(circle, 2.0),
            Constraint::LinearCopy {
                src: p[0],
                dst: p[3],
                step: 2,
                spacing: 1.5,
                angle: 0.4,
                group: 0,
            },
            Constraint::CircularCopy {
                src: p[0],
                dst: p[3],
                center: p[4],
                step: 1,
                angle: 0.7,
                group: 1,
            },
            Constraint::ArcConsistency(arc),
        ];
        for c in all {
            assert!(c.is_valid(&s), "{c:?}");
            let idx = c.locals(&s);
            let x: Vec<f64> = idx.iter().map(|&i| s.params[i]).collect();
            let duals: Vec<Dual> = x.iter().enumerate().map(|(i, &v)| Dual::var(v, i)).collect();
            let mut d = Vec::new();
            c.eval(&s, &duals, &mut d);
            for (j, _) in x.iter().enumerate() {
                let h = 1e-6;
                let (mut xp, mut xm) = (x.clone(), x.clone());
                xp[j] += h;
                xm[j] -= h;
                let (mut rp, mut rm) = (Vec::new(), Vec::new());
                c.eval(&s, &xp, &mut rp);
                c.eval(&s, &xm, &mut rm);
                for (k, r) in d.iter().enumerate() {
                    let fd = (rp[k] - rm[k]) / (2.0 * h);
                    assert!((r.d[j] - fd).abs() < 1e-5, "{} d/d{j}: {} vs {fd}", c.name(), r.d[j]);
                }
            }
        }
    }

    #[test]
    fn remove_remaps_ids_and_drops_dependents() {
        let mut s = Sketch::rectangle(10.0, 5.0);
        // Deleting the bottom line keeps its corner points (still used by the side lines).
        s.remove(&[], &[EntityId(0)], &[]);
        assert_eq!(s.entities().len(), 3);
        assert_eq!(s.point_count(), 4);
        assert!(s.constraints().iter().all(|c| c.is_valid(&s)));
        // Deleting a point deletes the lines through it and points left unused.
        let mut s = Sketch::rectangle(10.0, 5.0);
        s.remove(&[PointId(0)], &[], &[]);
        assert_eq!(s.entities().len(), 2);
        assert_eq!(s.point_count(), 3);
        assert!(!s.constraints().iter().any(|c| matches!(c, Constraint::Fix(..))));
    }

    #[test]
    fn drag_moves_free_geometry_only() {
        let mut s = Sketch::rectangle(10.0, 5.0);
        s.drag(&[(PointId(1), [30.0, 30.0])]);
        assert!(close(s.point(PointId(1)), [10.0, 5.0]), "fully defined corner stays");
        let mut s = Sketch::default();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(5.0, 0.0);
        let l = s.add_line(a, b);
        s.constrain(Constraint::Horizontal(l));
        s.constrain(Constraint::Fix(a, [0.0, 0.0]));
        s.drag(&[(b, [8.0, 3.0])]);
        assert!(
            close(s.point(b), [8.0, 0.0]),
            "slides along the horizontal: {:?}",
            s.point(b)
        );
    }

    #[test]
    fn pattern_spacing_edit_moves_all_instances() {
        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let e = s.add_circle(c, 1.0);
        s.constrain(Constraint::Fix(c, [0.0, 0.0]));
        s.linear_pattern(&[e], 3, 5.0, 0.0).unwrap();
        let i = s
            .constraints()
            .iter()
            .position(|c| matches!(c, Constraint::LinearCopy { .. }))
            .unwrap();
        s.set_dimension(i, 8.0);
        assert!(s.solve().converged);
        let centers: Vec<P2> = s
            .entities()
            .iter()
            .filter_map(|e| e.center())
            .map(|p| s.point(p))
            .collect();
        assert!(
            close(centers[1], [8.0, 0.0]) && close(centers[2], [16.0, 0.0]),
            "{centers:?}"
        );
    }

    #[test]
    fn large_sketch_solves_correctly() {
        // 250 independent constrained rectangles = 1000 lines.
        let mut s = Sketch::default();
        for i in 0..250 {
            let (x, y) = ((i % 25) as f64 * 20.0, (i / 25) as f64 * 20.0);
            let a = s.add_point(x, y);
            let b = s.add_point(x + 9.0, y + 7.0);
            s.add_rectangle(a, b).unwrap();
            s.constrain(Constraint::Fix(a, [x, y]));
            let corner = PointId(b.0 + 1);
            s.constrain(Constraint::Distance(a, corner, 10.0));
            s.constrain(Constraint::Distance(corner, b, 8.0));
        }
        assert!(s.solve().converged);
        assert_eq!(s.analyze().status, Status::FullyDefined);
    }

    /// Run with `cargo test --release -p threebcad-sketch -- --ignored`.
    #[test]
    #[ignore]
    fn large_sketch_solves_fast() {
        let mut s = Sketch::default();
        for i in 0..250 {
            let (x, y) = ((i % 25) as f64 * 20.0, (i / 25) as f64 * 20.0);
            let a = s.add_point(x, y);
            let b = s.add_point(x + 9.0, y + 7.0);
            s.add_rectangle(a, b).unwrap();
            s.constrain(Constraint::Fix(a, [x, y]));
            s.constrain(Constraint::Distance(a, PointId(b.0 + 1), 10.0));
        }
        let t = std::time::Instant::now();
        assert!(s.solve().converged);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        assert!(ms < 50.0, "1000-entity solve took {ms:.1} ms");
    }

    #[test]
    fn big_connected_component_uses_iterative_step() {
        // One long chain (> DENSE_MAX parameters) with every link dimensioned.
        let mut s = Sketch::default();
        let mut prev = s.add_point(0.0, 0.0);
        s.constrain(Constraint::Fix(prev, [0.0, 0.0]));
        for i in 1..200 {
            let p = s.add_point(i as f64 * 1.1, 0.3 * (i % 2) as f64);
            let l = s.add_line(prev, p);
            s.constrain(Constraint::Horizontal(l));
            s.constrain(Constraint::Distance(prev, p, 1.0));
            prev = p;
        }
        assert!(s.solve().converged);
        assert!(close(s.point(prev), [199.0, 0.0]), "{:?}", s.point(prev));
    }
}
