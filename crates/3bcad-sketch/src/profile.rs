//! Closed regions for features: loops found from the entity graph in any order, nested loops
//! become holes (and islands inside holes become regions of their own).

use std::f64::consts::TAU;

use crate::geom::{self, P2};
use crate::{EntityId, Geometry, Sketch};

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Line {
        from: P2,
        to: P2,
    },
    /// Part of the circle around `center`, counter-clockwise from `from` to `to` when `ccw`.
    Arc {
        center: P2,
        from: P2,
        to: P2,
        ccw: bool,
    },
    /// A full circle, always a loop by itself.
    Circle {
        center: P2,
        radius: f64,
        ccw: bool,
    },
    /// Cubic Bézier (control points in order).
    Bezier([P2; 4]),
}

impl Segment {
    pub fn start(&self) -> P2 {
        match *self {
            Segment::Line { from, .. } | Segment::Arc { from, .. } => from,
            Segment::Circle { center, radius, .. } => [center[0] + radius, center[1]],
            Segment::Bezier(b) => b[0],
        }
    }

    pub fn end(&self) -> P2 {
        match *self {
            Segment::Line { to, .. } | Segment::Arc { to, .. } => to,
            Segment::Circle { .. } => self.start(),
            Segment::Bezier(b) => b[3],
        }
    }

    pub fn reversed(&self) -> Segment {
        match *self {
            Segment::Line { from, to } => Segment::Line { from: to, to: from },
            Segment::Arc { center, from, to, ccw } => Segment::Arc {
                center,
                from: to,
                to: from,
                ccw: !ccw,
            },
            Segment::Circle { center, radius, ccw } => Segment::Circle {
                center,
                radius,
                ccw: !ccw,
            },
            Segment::Bezier([a, b, c, d]) => Segment::Bezier([d, c, b, a]),
        }
    }

    /// Points along the segment, `start` included and `end` excluded; `per_turn` sets arc density.
    pub fn sample(&self, per_turn: usize, out: &mut Vec<P2>) {
        match *self {
            Segment::Line { from, .. } => out.push(from),
            Segment::Arc { center, from, to, ccw } => {
                let (a0, a1) = (
                    geom::angle_of(geom::sub(from, center)),
                    geom::angle_of(geom::sub(to, center)),
                );
                let sweep = if ccw {
                    geom::ccw_sweep(a0, a1)
                } else {
                    -geom::ccw_sweep(a1, a0)
                };
                let n = ((sweep.abs() / TAU * per_turn as f64).ceil() as usize).max(2);
                let pts = geom::arc_points(center, geom::dist(from, center), a0, sweep, n);
                out.extend(&pts[..n]);
            }
            Segment::Circle { center, radius, ccw } => {
                let pts = geom::arc_points(center, radius, 0.0, if ccw { TAU } else { -TAU }, per_turn);
                out.extend(&pts[..per_turn]);
            }
            Segment::Bezier(b) => out.extend((0..16).map(|i| geom::bezier_point(&b, i as f64 / 16.0))),
        }
    }
}

/// A face to extrude: counter-clockwise outer loop, clockwise holes.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub outer: Vec<Segment>,
    pub holes: Vec<Vec<Segment>>,
}

/// Polygon approximation of a loop (for containment, area and drawing).
pub fn loop_polygon(lp: &[Segment], per_turn: usize) -> Vec<P2> {
    let mut out = Vec::new();
    lp.iter().for_each(|s| s.sample(per_turn, &mut out));
    out
}

fn reverse_loop(lp: &[Segment]) -> Vec<Segment> {
    lp.iter().rev().map(Segment::reversed).collect()
}

impl Sketch {
    /// The curve of one entity as segments (splines become Bézier spans).
    pub fn segments(&self, e: EntityId) -> Vec<Segment> {
        match &self.entity(e).geom {
            Geometry::Line { a, b } => vec![Segment::Line {
                from: self.point(*a),
                to: self.point(*b),
            }],
            Geometry::Circle { center, .. } => vec![Segment::Circle {
                center: self.point(*center),
                radius: self.radius(e),
                ccw: true,
            }],
            Geometry::Arc { center, start, end } => vec![Segment::Arc {
                center: self.point(*center),
                from: self.point(*start),
                to: self.point(*end),
                ccw: true,
            }],
            Geometry::Spline { points } => {
                let closed = points.len() > 2 && points[0] == *points.last().unwrap();
                let pts: Vec<P2> = points[..points.len() - closed as usize]
                    .iter()
                    .map(|p| self.point(*p))
                    .collect();
                geom::catmull_rom(&pts, closed)
                    .into_iter()
                    .map(Segment::Bezier)
                    .collect()
            }
        }
    }

    /// Display polyline of an entity, end included.
    pub fn polyline(&self, e: EntityId) -> Vec<P2> {
        let segs = self.segments(e);
        let mut out = loop_polygon(&segs, 96);
        out.push(segs.last().map_or([0.0; 2], Segment::end));
        out
    }

    /// Closed regions made by the non-construction geometry.
    pub fn regions(&self) -> Result<Vec<Region>, String> {
        let tol = 1e-6 * (1.0 + self.params_scale());
        let mut loops: Vec<Vec<Segment>> = Vec::new();
        let mut curves: Vec<Vec<Segment>> = Vec::new();
        for e in self.entity_ids().filter(|&e| !self.entity(e).construction) {
            let segs = self.segments(e);
            let (Some(first), Some(last)) = (segs.first(), segs.last()) else {
                continue;
            };
            if matches!(first, Segment::Circle { .. }) || geom::dist(first.start(), last.end()) < tol {
                loops.push(segs);
            } else {
                curves.push(segs);
            }
        }

        // Join open curves at shared end positions; every junction must join exactly two curves.
        let mut nodes: Vec<P2> = Vec::new();
        let mut node_of = |p: P2| -> usize {
            match nodes.iter().position(|q| geom::dist(*q, p) < tol) {
                Some(i) => i,
                None => {
                    nodes.push(p);
                    nodes.len() - 1
                }
            }
        };
        let ends: Vec<(usize, usize)> = curves
            .iter()
            .map(|c| (node_of(c[0].start()), node_of(c.last().unwrap().end())))
            .collect();
        let mut degree = vec![0; nodes.len()];
        for &(a, b) in &ends {
            degree[a] += 1;
            degree[b] += 1;
        }
        if let Some(n) = degree.iter().position(|&d| d != 2) {
            let p = nodes[n];
            let what = if degree[n] < 2 {
                "Open contour"
            } else {
                "Branching contour"
            };
            return Err(format!("{what} at ({:.2}, {:.2})", p[0], p[1]));
        }
        let mut used = vec![false; curves.len()];
        for start in 0..curves.len() {
            if used[start] {
                continue;
            }
            let mut lp: Vec<Segment> = Vec::new();
            let (first_node, mut node) = ends[start];
            used[start] = true;
            lp.extend(curves[start].iter().cloned());
            while node != first_node {
                let next = (0..curves.len())
                    .find(|&j| !used[j] && (ends[j].0 == node || ends[j].1 == node))
                    .expect("degree 2 guarantees a next curve");
                used[next] = true;
                if ends[next].0 == node {
                    lp.extend(curves[next].iter().cloned());
                    node = ends[next].1;
                } else {
                    lp.extend(reverse_loop(&curves[next]));
                    node = ends[next].0;
                }
            }
            loops.push(lp);
        }
        if loops.is_empty() {
            return Err("Sketch has no closed profile".into());
        }

        let polys: Vec<Vec<P2>> = loops.iter().map(|l| loop_polygon(l, 64)).collect();
        for i in 0..polys.len() {
            for j in i + 1..polys.len() {
                if polylines_cross(&polys[i], &polys[j]) {
                    return Err("Contours intersect each other".into());
                }
            }
        }
        let contains =
            |outer: usize, inner: usize| polys[inner].iter().all(|p| geom::point_in_polygon(*p, &polys[outer]));
        let depth: Vec<usize> = (0..loops.len())
            .map(|i| (0..loops.len()).filter(|&j| j != i && contains(j, i)).count())
            .collect();
        let ccw = |i: usize| -> Vec<Segment> {
            if geom::polygon_area(&polys[i]) >= 0.0 {
                loops[i].clone()
            } else {
                reverse_loop(&loops[i])
            }
        };
        let mut regions = Vec::new();
        for i in (0..loops.len()).filter(|&i| depth[i].is_multiple_of(2)) {
            let holes = (0..loops.len())
                .filter(|&j| depth[j] == depth[i] + 1 && contains(i, j))
                .map(|j| reverse_loop(&ccw(j)))
                .collect();
            regions.push(Region { outer: ccw(i), holes });
        }
        Ok(regions)
    }

    fn params_scale(&self) -> f64 {
        self.point_ids()
            .map(|p| self.point(p))
            .fold(0.0, |m: f64, q| m.max(q[0].abs()).max(q[1].abs()))
    }
}

fn polylines_cross(a: &[P2], b: &[P2]) -> bool {
    let seg = |p: &[P2], i: usize| (p[i], p[(i + 1) % p.len()]);
    (0..a.len()).any(|i| {
        let (a0, a1) = seg(a, i);
        (0..b.len()).any(|j| {
            let (b0, b1) = seg(b, j);
            geom::segments_cross(a0, a1, b0, b1)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Constraint, PointId};

    fn area(r: &Region) -> f64 {
        geom::polygon_area(&loop_polygon(&r.outer, 256))
            + r.holes
                .iter()
                .map(|h| geom::polygon_area(&loop_polygon(h, 256)))
                .sum::<f64>()
    }

    #[test]
    fn unordered_lines_form_a_loop() {
        let mut s = Sketch::default();
        let p: Vec<PointId> = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
            .iter()
            .map(|q| s.add_point(q[0], q[1]))
            .collect();
        // Out of order and with mixed directions.
        s.add_line(p[2], p[1]);
        s.add_line(p[0], p[1]);
        s.add_line(p[3], p[0]);
        s.add_line(p[2], p[3]);
        let r = s.regions().unwrap();
        assert_eq!(r.len(), 1);
        assert!((area(&r[0]) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn hole_and_island() {
        let mut s = Sketch::rectangle(20.0, 20.0);
        let c = s.add_point(10.0, 10.0);
        s.add_circle(c, 6.0); // hole
        s.add_circle(c, 2.0); // island inside the hole
        let r = s.regions().unwrap();
        assert_eq!(r.len(), 2);
        let (outer, island) = if r[0].holes.is_empty() {
            (&r[1], &r[0])
        } else {
            (&r[0], &r[1])
        };
        assert_eq!(outer.holes.len(), 1);
        let pi = std::f64::consts::PI;
        assert!((area(outer) - (400.0 - pi * 36.0)).abs() < 0.05, "{}", area(outer));
        assert!((area(island) - pi * 4.0).abs() < 0.01);
    }

    #[test]
    fn construction_lines_are_ignored_and_errors_are_clear() {
        let mut s = Sketch::rectangle(10.0, 10.0);
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(10.0, 10.0);
        let diag = s.add_line(a, b);
        s.set_construction(diag, true);
        assert_eq!(s.regions().unwrap().len(), 1);
        s.set_construction(diag, false);
        assert!(s.regions().unwrap_err().contains("contour"));

        let mut open = Sketch::default();
        let a = open.add_point(0.0, 0.0);
        let b = open.add_point(1.0, 0.0);
        open.add_line(a, b);
        assert!(open.regions().unwrap_err().starts_with("Open contour"));
        assert!(Sketch::default().regions().unwrap_err().contains("no closed profile"));
    }

    #[test]
    fn arcs_and_lines_make_a_slot_region() {
        let mut s = Sketch::default();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(10.0, 0.0);
        s.add_slot(a, b, [5.0, 2.0]).unwrap();
        let r = s.regions().unwrap();
        assert_eq!(r.len(), 1);
        let expected = 10.0 * 4.0 + std::f64::consts::PI * 4.0;
        assert!((area(&r[0]) - expected).abs() < 0.01, "{}", area(&r[0]));
        s.constrain(Constraint::Fix(a, [0.0, 0.0]));
        assert!(s.solve().converged);
    }

    #[test]
    fn closed_spline_is_a_loop() {
        let mut s = Sketch::default();
        let p: Vec<PointId> = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]
            .iter()
            .map(|q| s.add_point(q[0], q[1]))
            .collect();
        s.add_spline(vec![p[0], p[1], p[2], p[3], p[0]]);
        let r = s.regions().unwrap();
        assert_eq!(r.len(), 1);
        assert!(area(&r[0]) > 12.0, "{}", area(&r[0]));
    }

    #[test]
    fn crossing_contours_are_rejected() {
        let mut s = Sketch::default();
        let c1 = s.add_point(0.0, 0.0);
        let c2 = s.add_point(3.0, 0.0);
        s.add_circle(c1, 2.0);
        s.add_circle(c2, 2.0);
        assert!(s.regions().unwrap_err().contains("intersect"));
    }
}
