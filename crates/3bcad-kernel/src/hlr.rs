//! Hidden-line removal for drawings: the edges and silhouettes of bodies projected onto a view
//! plane, split into what the viewer sees and what the bodies hide. Works on the triangles and edge
//! polylines of `Topology`, so it handles every surface truck can mesh.

use std::collections::HashMap;

use crate::solid::Plane;
use crate::topology::{cross, dot, sub, EdgeCurve, TopoFace, Topology, P3};

pub type P2 = [f64; 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Edge {
        body: usize,
        edge: usize,
    },
    /// Where a curved face turns away from the viewer.
    Silhouette,
    /// The outline of a face the section plane cuts.
    Cut,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub source: Source,
    pub visible: bool,
    pub points: Vec<P2>,
}

#[derive(Clone, Debug, Default)]
pub struct Projection {
    pub lines: Vec<Line>,
    /// The cut faces' outlines as segments, for hatching (empty without a cut).
    pub cut: Vec<[P2; 2]>,
}

/// Where `p` lands in a view: (right, up, towards the viewer).
pub fn to_view(view: &Plane, p: P3) -> P3 {
    [dot(p, view.x), dot(p, view.y), dot(p, view.normal())]
}

/// Projects the bodies onto `view` (x right on paper, y up, the normal towards the viewer).
/// With `cut`, only material at or behind the plane at that depth along the normal is kept, and
/// the faces the plane cuts are outlined.
pub fn project(bodies: &[&Topology], view: &Plane, cut: Option<f64>) -> Projection {
    let v = |p: P3| to_view(view, p);
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for t in bodies {
        for f in &t.faces {
            for p in f.tris.iter().flatten() {
                let q = v(*p);
                for i in 0..3 {
                    lo[i] = lo[i].min(q[i]);
                    hi[i] = hi[i].max(q[i]);
                }
            }
        }
    }
    if lo[0] > hi[0] {
        return Projection::default();
    }
    let diag = dot(sub(hi, lo), sub(hi, lo)).sqrt().max(1e-9);
    // shortcut: exact edges and chordal triangles disagree by the mesh tolerance, which grows at
    // grazing angles, so an edge less than this far behind a face still counts as seen. A per-face
    // tolerance from the mesher would be tighter.
    let eps = (diag * 2e-3).max(0.03);
    let depth = cut.unwrap_or(f64::INFINITY);

    let mut tris = Vec::new();
    // Which face each triangle belongs to, numbered across all bodies.
    let mut face_of = Vec::new();
    let mut cut_segments: Vec<[P3; 2]> = Vec::new();
    // Silhouette segments, with the cylinder they lie on.
    let mut silhouettes: Vec<[P3; 2]> = Vec::new();
    let mut silhouette_face = Vec::new();
    let mut id = 0u32;
    for t in bodies {
        for f in &t.faces {
            let vt: Vec<[P3; 3]> = f.tris.iter().map(|t| t.map(v)).collect();
            for t in &vt {
                clip_triangle(t, depth, &mut tris, &mut cut_segments);
            }
            face_of.resize(tris.len(), id);
            if f.plane.is_none() {
                for [a, b] in face_silhouettes(f, view) {
                    if let Some(s) = clip_segment(a, b, depth) {
                        silhouettes.push(s);
                        // A cylinder never hides its own outline, but the outline traced on its
                        // flat facets lies just inside them: skip them when judging it.
                        silhouette_face.push(f.cylinder.map(|_| id));
                    }
                }
            }
            id += 1;
        }
    }
    let occluder = Occluder::new(tris, face_of, &cut_segments, lo, hi, eps, depth);
    let step = diag / 300.0;
    let mut out = Projection::default();

    for (bi, t) in bodies.iter().enumerate() {
        for (ei, e) in t.edges.iter().enumerate() {
            if !drawn(e, view) {
                continue;
            }
            let pts: Vec<P3> = e.points.iter().map(|&p| v(p)).collect();
            for run in clip_polyline(&pts, depth) {
                occluder.split(&run, step, Source::Edge { body: bi, edge: ei }, &[], &mut out.lines);
            }
        }
    }
    for (chain, used) in chains(silhouettes) {
        let mut skip: Vec<u32> = used.iter().filter_map(|&s| silhouette_face[s]).collect();
        skip.dedup();
        occluder.split(&chain, step, Source::Silhouette, &skip, &mut out.lines);
    }
    out.cut = cut_segments.iter().map(|s| s.map(|p| [p[0], p[1]])).collect();
    for (chain, _) in chains(cut_segments) {
        out.lines.push(Line {
            source: Source::Cut,
            visible: true,
            points: chain.iter().map(|p| [p[0], p[1]]).collect(),
        });
    }
    out
}

/// Whether an edge shows as a line: sharp edges and open edges always; a smooth edge (a seam of a
/// cylinder, where a fillet meets a face) only where it is the outline, its faces turning away.
fn drawn(e: &crate::topology::TopoEdge, view: &Plane) -> bool {
    let [a, b] = match &e.sides[..] {
        [a, b] => [a, b],
        _ => return true,
    };
    if a.face != b.face && dot(a.normal, b.normal) < 0.9998 {
        return true;
    }
    let n = view.normal();
    let (da, db) = (dot(a.normal, n), dot(b.normal, n));
    da.abs() < 1e-3 || db.abs() < 1e-3 || (da > 0.0) != (db > 0.0)
}

/// Where a curved face turns from facing the viewer to facing away: the zero line of
/// (surface normal · view direction) over its triangles. Normals are exact on cylinders and
/// averaged from the triangles elsewhere (triangle normals alone flicker on a cylinder seen end-on).
fn face_silhouettes(face: &TopoFace, view: &Plane) -> Vec<[P3; 2]> {
    let key = |p: P3| p.map(f64::to_bits);
    let out = view.normal();
    let mut sum: HashMap<[u64; 3], P3> = HashMap::new();
    if face.cylinder.is_none() {
        for t in &face.tris {
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            for p in t {
                let s = sum.entry(key(*p)).or_default();
                *s = [0, 1, 2].map(|i| s[i] + n[i]);
            }
        }
    }
    let facing = |p: P3| -> f64 {
        let n = match face.cylinder {
            Some((c, d, _)) => {
                let r = sub(p, c);
                sub(r, d.map(|x| x * dot(r, d)))
            }
            None => sum[&key(p)],
        };
        dot(n, out) / dot(n, n).sqrt().max(1e-300)
    };
    let mut segs = Vec::new();
    for t in &face.tris {
        let f = t.map(facing);
        // Seen edge-on all over (a cylinder along the view): its rims are the outline.
        if f.iter().all(|x| x.abs() < 1e-6) {
            continue;
        }
        let mut cross_at = Vec::new();
        for i in 0..3 {
            let (mut a, mut b, mut fa, mut fb) = (t[i], t[(i + 1) % 3], f[i], f[(i + 1) % 3]);
            if (fa > 0.0) == (fb > 0.0) {
                continue;
            }
            // The same point from both triangles that share the edge.
            if key(a) > key(b) {
                (a, b, fa, fb) = (b, a, fb, fa);
            }
            cross_at.push(to_view(view, lerp(a, b, fa / (fa - fb))));
        }
        if let [a, b] = cross_at[..] {
            segs.push([a, b]);
        }
    }
    segs
}

fn lerp(a: P3, b: P3, t: f64) -> P3 {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Keeps the part of a triangle at or behind `depth`; a triangle the plane crosses also gives a
/// segment of the cut outline.
fn clip_triangle(t: &[P3; 3], depth: f64, kept: &mut Vec<[P3; 3]>, cut: &mut Vec<[P3; 2]>) {
    let s = t.map(|p| p[2] - depth);
    if s.iter().all(|&x| x <= 0.0) {
        kept.push(*t);
        return;
    }
    let mut poly = Vec::new();
    let mut crossing = Vec::new();
    for i in 0..3 {
        let (a, b, sa, sb) = (t[i], t[(i + 1) % 3], s[i], s[(i + 1) % 3]);
        if sa <= 0.0 {
            poly.push(a);
        }
        if (sa > 0.0) != (sb > 0.0) {
            let p = lerp(a, b, sa / (sa - sb));
            poly.push(p);
            crossing.push(p);
        }
    }
    for i in 1..poly.len().saturating_sub(1) {
        kept.push([poly[0], poly[i], poly[i + 1]]);
    }
    if let [a, b] = crossing[..] {
        cut.push([a, b]);
    }
}

fn clip_segment(a: P3, b: P3, depth: f64) -> Option<[P3; 2]> {
    let (sa, sb) = (a[2] - depth, b[2] - depth);
    match (sa <= 0.0, sb <= 0.0) {
        (true, true) => Some([a, b]),
        (false, false) => None,
        (true, false) => Some([a, lerp(a, b, sa / (sa - sb))]),
        (false, true) => Some([lerp(a, b, sa / (sa - sb)), b]),
    }
}

/// The runs of a polyline at or behind `depth`.
fn clip_polyline(pts: &[P3], depth: f64) -> Vec<Vec<P3>> {
    let mut runs: Vec<Vec<P3>> = Vec::new();
    let mut open = false;
    for w in pts.windows(2) {
        match clip_segment(w[0], w[1], depth) {
            Some([a, b]) => {
                if !open || a != w[0] {
                    runs.push(vec![a]);
                }
                runs.last_mut().unwrap().push(b);
                open = b == w[1];
            }
            None => open = false,
        }
    }
    runs
}

/// Joins segments that share end points into polylines, each with the segments it took.
fn chains(segments: Vec<[P3; 2]>) -> Vec<(Vec<P3>, Vec<usize>)> {
    let key = |p: P3| p.map(f64::to_bits);
    let mut at: HashMap<[u64; 3], Vec<usize>> = HashMap::new();
    for (i, s) in segments.iter().enumerate() {
        for p in s {
            at.entry(key(*p)).or_default().push(i);
        }
    }
    let mut used = vec![false; segments.len()];
    let mut out = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut chain = segments[start].to_vec();
        let mut taken = vec![start];
        // Grow forwards from the last point, then backwards from the first.
        for back in [false, true] {
            loop {
                let end = if back { chain[0] } else { *chain.last().unwrap() };
                let next = at[&key(end)].iter().copied().find(|&j| !used[j]);
                let Some(j) = next else { break };
                used[j] = true;
                taken.push(j);
                let s = segments[j];
                let p = if key(s[0]) == key(end) { s[1] } else { s[0] };
                if back {
                    chain.insert(0, p);
                } else {
                    chain.push(p);
                }
            }
        }
        out.push((chain, taken));
    }
    out
}

/// Triangles in view coordinates, binned on a grid in the view plane, and the section's cap.
struct Occluder {
    tris: Vec<[P3; 3]>,
    face_of: Vec<u32>,
    lo: P2,
    cell: f64,
    n: usize,
    grid: Vec<Vec<u32>>,
    /// Cap outline segments by grid row, for the even-odd inside test.
    cap: Vec<Vec<[P2; 2]>>,
    eps: f64,
    depth: f64,
}

impl Occluder {
    fn new(tris: Vec<[P3; 3]>, face_of: Vec<u32>, cap: &[[P3; 2]], lo: P3, hi: P3, eps: f64, depth: f64) -> Self {
        let n = ((tris.len() as f64).sqrt() as usize).clamp(1, 256);
        let cell = ((hi[0] - lo[0]).max(hi[1] - lo[1]) / n as f64).max(1e-9);
        let mut o = Self {
            tris,
            face_of,
            lo: [lo[0], lo[1]],
            cell,
            n,
            grid: vec![Vec::new(); n * n],
            cap: vec![Vec::new(); n],
            eps,
            depth,
        };
        for (i, t) in o.tris.iter().enumerate() {
            let (x0, x1) = o.span(t.iter().map(|p| p[0]), 0);
            let (y0, y1) = o.span(t.iter().map(|p| p[1]), 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    o.grid[y * n + x].push(i as u32);
                }
            }
        }
        for s in cap {
            let (y0, y1) = o.span(s.iter().map(|p| p[1]), 1);
            for row in &mut o.cap[y0..=y1] {
                row.push(s.map(|p| [p[0], p[1]]));
            }
        }
        o
    }

    fn index(&self, v: f64, axis: usize) -> usize {
        (((v - self.lo[axis]) / self.cell).floor().max(0.0) as usize).min(self.n - 1)
    }

    fn span(&self, vals: impl Iterator<Item = f64> + Clone, axis: usize) -> (usize, usize) {
        let lo = vals.clone().fold(f64::INFINITY, f64::min);
        let hi = vals.fold(f64::NEG_INFINITY, f64::max);
        (self.index(lo, axis), self.index(hi, axis))
    }

    /// Whether something lies in front of `p`, leaving out the faces `skip`.
    fn hidden(&self, p: P3, skip: &[u32]) -> bool {
        let cell = &self.grid[self.index(p[1], 1) * self.n + self.index(p[0], 0)];
        let covered = cell.iter().any(|&i| {
            if skip.contains(&self.face_of[i as usize]) {
                return false;
            }
            let [a, b, c] = self.tris[i as usize];
            let det = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if det.abs() < 1e-12 {
                return false;
            }
            let u = ((p[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (p[1] - a[1])) / det;
            let w = ((b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1])) / det;
            let tol = -1e-9;
            u >= tol && w >= tol && u + w <= 1.0 - tol && a[2] + u * (b[2] - a[2]) + w * (c[2] - a[2]) > p[2] + self.eps
        });
        covered || (p[2] < self.depth - self.eps && self.in_cap(p))
    }

    /// Even-odd test against the cap outline, with a ray towards +x.
    fn in_cap(&self, p: P3) -> bool {
        let row = &self.cap[self.index(p[1], 1)];
        let crossings = row
            .iter()
            .filter(|[a, b]| (a[1] > p[1]) != (b[1] > p[1]))
            .filter(|[a, b]| a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) > p[0])
            .count();
        crossings % 2 == 1
    }

    /// Splits a polyline into seen and hidden runs, judged every `step` along it.
    fn split(&self, pts: &[P3], step: f64, source: Source, skip: &[u32], out: &mut Vec<Line>) {
        let mut runs: Vec<Line> = Vec::new();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            let n = ((len / step).ceil() as usize).max(1);
            for k in 0..n {
                let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
                let visible = !self.hidden(lerp(a, b, 0.5 * (t0 + t1)), skip);
                let (p0, p1) = (lerp(a, b, t0), lerp(a, b, t1));
                match runs.last_mut() {
                    Some(l) if l.visible == visible => l.points.push([p1[0], p1[1]]),
                    _ => runs.push(Line {
                        source,
                        visible,
                        points: vec![[p0[0], p0[1]], [p1[0], p1[1]]],
                    }),
                }
            }
        }
        let long = |l: &Line| {
            l.points
                .windows(2)
                .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                .sum::<f64>()
                > 1e-6
        };
        out.extend(runs.into_iter().filter(long));
    }
}

/// A circular edge seen straight on: its centre and radius in view coordinates.
pub fn circle_in_view(curve: &EdgeCurve, view: &Plane) -> Option<(P2, f64)> {
    match *curve {
        EdgeCurve::Circle { center, axis, radius } if dot(axis, view.normal()).abs() > 0.9999 => {
            let c = to_view(view, center);
            Some(([c[0], c[1]], radius))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::{subtract, transformed};
    use crate::operations::{extrude_circle, extrude_polygon};
    use crate::topology::analyze;

    /// A 30 mm cube on the XY plane with a Ø10 hole straight down its middle.
    fn holed_cube() -> Topology {
        let cube = extrude_polygon(&[[0.0, 0.0], [30.0, 0.0], [30.0, 30.0], [0.0, 30.0]], 30.0).unwrap();
        let hole = extrude_circle([15.0, 15.0], 5.0, 40.0).unwrap();
        let down = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, -5.0, 1.0],
        ];
        let s = subtract(&cube, &transformed(&hole, down)).unwrap().remove(0);
        analyze(&s, 0.02)
    }

    fn length(lines: &[&Line]) -> f64 {
        lines
            .iter()
            .flat_map(|l| l.points.windows(2))
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum()
    }

    #[test]
    fn the_front_view_shows_the_outline_and_hides_the_hole() {
        let t = holed_cube();
        let p = project(&[&t], &Plane::FRONT, None);
        let seen: Vec<&Line> = p.lines.iter().filter(|l| l.visible).collect();
        let hidden: Vec<&Line> = p.lines.iter().filter(|l| !l.visible).collect();
        // The square outline: 4 sides of 30 (the front face's edges; the back ones lie under them).
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for q in seen.iter().flat_map(|l| &l.points) {
            for i in 0..2 {
                lo[i] = lo[i].min(q[i]);
                hi[i] = hi[i].max(q[i]);
            }
        }
        assert!(
            (lo[0] - 0.0).abs() < 1e-6 && (hi[0] - 30.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
        assert!(
            (lo[1] - 0.0).abs() < 1e-6 && (hi[1] - 30.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
        // The hole's two sides are hidden lines from the bottom to the top at x = 10 and x = 20.
        for x in [10.0, 20.0] {
            let ys: Vec<f64> = hidden
                .iter()
                .filter(|l| l.points.iter().all(|q| (q[0] - x).abs() < 0.05))
                .flat_map(|l| l.points.iter().map(|q| q[1]))
                .collect();
            let (y0, y1) = (
                ys.iter().copied().fold(f64::INFINITY, f64::min),
                ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            );
            assert!(
                y0.abs() < 1e-6 && (y1 - 30.0).abs() < 1e-6,
                "hidden at x = {x}: {y0}..{y1}"
            );
        }
        // Nothing else is hidden, apart from back edges lying under the outline.
        let outline = |q: &P2| q.iter().any(|v| v.abs() < 1e-6 || (v - 30.0).abs() < 1e-6);
        assert!(hidden
            .iter()
            .flat_map(|l| &l.points)
            .all(|q| (q[0] - 10.0).abs() < 0.05 || (q[0] - 20.0).abs() < 0.05 || outline(q)));
        assert!(p.cut.is_empty());
    }

    #[test]
    fn the_top_view_sees_the_hole_as_circles() {
        let t = holed_cube();
        let circles: Vec<(P2, f64)> = t
            .edges
            .iter()
            .filter_map(|e| circle_in_view(&e.curve, &Plane::TOP))
            .collect();
        assert!(!circles.is_empty());
        assert!(circles
            .iter()
            .all(|(c, r)| (c[0] - 15.0).abs() < 1e-4 && (c[1] - 15.0).abs() < 1e-4 && (r - 5.0).abs() < 1e-4));
        let p = project(&[&t], &Plane::TOP, None);
        let on_rim = |q: &P2| ((q[0] - 15.0).hypot(q[1] - 15.0) - 5.0).abs() < 0.05;
        let on_square = |q: &P2| q.iter().any(|v| v.abs() < 1e-6 || (v - 30.0).abs() < 1e-6);
        // Seen: the square outline and the hole's top rim, all the way round.
        let seen: Vec<&Line> = p.lines.iter().filter(|l| l.visible).collect();
        assert!(seen.iter().flat_map(|l| &l.points).all(|q| on_rim(q) || on_square(q)));
        let rim: f64 = length(
            &seen
                .iter()
                .copied()
                .filter(|l| l.points.iter().all(on_rim))
                .collect::<Vec<_>>(),
        );
        assert!(rim > 10.0 * std::f64::consts::PI - 0.5, "{rim}");
        let square = length(
            &seen
                .iter()
                .copied()
                .filter(|l| l.points.iter().all(on_square))
                .collect::<Vec<_>>(),
        );
        assert!(square > 120.0 - 1e-6, "{square}");
        // Hidden lines only where the bottom edges lie under the top ones.
        assert!(p
            .lines
            .iter()
            .filter(|l| !l.visible)
            .flat_map(|l| &l.points)
            .all(|q| on_rim(q) || on_square(q)));
    }

    #[test]
    fn a_cylinder_seen_at_an_angle_shows_both_sides() {
        let t = analyze(&extrude_circle([0.0, 0.0], 5.0, 20.0).unwrap(), 0.02);
        // Looking down at 35° from the front-right: each side line is the 20 mm height foreshortened.
        let s = (1.0f64 / 3.0).sqrt();
        let n = [s, -s, s];
        let up = crate::topology::unit([-1.0, 1.0, 2.0]);
        let view = Plane {
            origin: [0.0; 3],
            x: cross(up, n),
            y: up,
        };
        let p = project(&[&t], &view, None);
        let sides: Vec<&Line> = p
            .lines
            .iter()
            .filter(|l| l.source == Source::Silhouette && l.visible)
            .collect();
        let want = 2.0 * 20.0 * dot([0.0, 0.0, 1.0], up);
        assert!(
            (length(&sides) - want).abs() < 0.05 * want,
            "{} of {want}",
            length(&sides)
        );
        // They run at the cylinder's radius either side of its axis.
        assert!(sides
            .iter()
            .flat_map(|l| &l.points)
            .all(|q| (q[0].abs() - 5.0).abs() < 0.05));
    }

    #[test]
    fn a_section_through_the_hole_cuts_two_slabs() {
        let t = holed_cube();
        // Front view looks along +Y, so the normal is -Y and depth -15 is the plane y = 15.
        let p = project(&[&t], &Plane::FRONT, Some(-15.0));
        let cut: f64 = p.cut.iter().map(|[a, b]| (b[0] - a[0]).hypot(b[1] - a[1])).sum();
        // Two 10 x 30 rectangles either side of the hole.
        assert!((cut - 160.0).abs() < 0.5, "{cut}");
        let lines: Vec<&Line> = p.lines.iter().collect();
        assert!(p.lines.iter().any(|l| l.source == Source::Cut));
        // Behind the plane the hole's back half is seen; nothing is hidden behind the cut faces
        // except what lies in them.
        assert!(length(&lines) > 160.0);
        assert!(p
            .lines
            .iter()
            .filter(|l| l.visible)
            .all(|l| l.points.iter().all(|q| q[0] > -1e-6 && q[0] < 30.0 + 1e-6)));
    }
}
