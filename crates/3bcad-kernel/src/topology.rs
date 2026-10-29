//! What a body is made of, in the same order as `mesher::render_mesh` lists it: faces with their
//! triangles and planes, edges with their adjacent faces and exact shape, vertices. Used for
//! naming faces across rebuilds, for edge features (fillet, chamfer) and for measuring.

use std::collections::HashMap;

use truck_meshalgo::prelude::*;
use truck_modeling::*;

use crate::solid::Plane;

pub type P3 = [f64; 3];

#[derive(Clone, Debug)]
pub struct TopoFace {
    pub plane: Option<Plane>,
    /// Axis point, axis direction and radius when the face is a cylinder (a revolved line).
    pub cylinder: Option<(P3, P3, f64)>,
    pub tris: Vec<[P3; 3]>,
    pub area: f64,
    /// A point inside the face (centroid of its largest triangle).
    pub sample: P3,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EdgeCurve {
    Line { a: P3, b: P3 },
    Circle { center: P3, axis: P3, radius: f64 },
    Other,
}

#[derive(Clone, Debug)]
pub struct EdgeSide {
    pub face: usize,
    /// Outward normal of the face at the edge's midpoint.
    pub normal: P3,
    /// Direction from the edge into the face, perpendicular to the edge.
    pub inward: P3,
}

#[derive(Clone, Debug)]
pub struct TopoEdge {
    pub curve: EdgeCurve,
    pub points: Vec<P3>,
    pub length: f64,
    pub mid: P3,
    /// Unit tangent at `mid`, as the first face's boundary runs.
    pub tangent: P3,
    pub sides: Vec<EdgeSide>,
    pub vertices: [usize; 2],
}

#[derive(Clone, Debug, Default)]
pub struct Topology {
    pub faces: Vec<TopoFace>,
    pub edges: Vec<TopoEdge>,
    pub vertices: Vec<P3>,
    /// Faces touching each vertex.
    pub vertex_faces: Vec<Vec<usize>>,
}

fn v3(p: Point3) -> P3 {
    [p.x, p.y, p.z]
}

pub fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}

pub fn unit(a: P3) -> P3 {
    let l = norm(a).max(1e-300);
    a.map(|v| v / l)
}

/// Distance from `p` to triangle `t`.
pub fn point_triangle(p: P3, [a, b, c]: [P3; 3]) -> f64 {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let n = cross(ab, ac);
    let nn = dot(n, n);
    if nn < 1e-300 {
        return norm(ap);
    }
    // Inside the prism over the triangle: plane distance; otherwise the nearest edge.
    let w = dot(cross(ab, ap), n) / nn;
    let v = dot(cross(ap, ac), n) / nn;
    if v >= 0.0 && w >= 0.0 && v + w <= 1.0 {
        return dot(ap, n).abs() / nn.sqrt();
    }
    let seg = |a: P3, b: P3| {
        let ab = sub(b, a);
        let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-300)).clamp(0.0, 1.0);
        norm(sub(p, [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t]))
    };
    seg(a, b).min(seg(b, c)).min(seg(c, a))
}

impl TopoFace {
    pub fn distance(&self, p: P3) -> f64 {
        self.tris
            .iter()
            .map(|&t| point_triangle(p, t))
            .fold(f64::INFINITY, f64::min)
    }

    /// Area-weighted center.
    pub fn center(&self) -> P3 {
        let mut c = [0.0; 3];
        let mut total = 0.0;
        for t in &self.tris {
            let a = 0.5 * norm(cross(sub(t[1], t[0]), sub(t[2], t[0])));
            for (i, ci) in c.iter_mut().enumerate() {
                *ci += a * (t[0][i] + t[1][i] + t[2][i]) / 3.0;
            }
            total += a;
        }
        c.map(|v| v / total.max(1e-300))
    }
}

/// Fits a line or circle through samples of an exact curve.
fn classify(points: &[P3]) -> EdgeCurve {
    let (a, b) = (points[0], *points.last().unwrap());
    let scale = points.iter().map(|p| norm(sub(*p, a))).fold(0.0, f64::max).max(1e-9);
    let tol = 1e-7 * scale.max(1.0);
    if norm(sub(b, a)) > tol {
        let d = unit(sub(b, a));
        if points.iter().all(|p| norm(cross(sub(*p, a), d)) < tol) {
            return EdgeCurve::Line { a, b };
        }
    }
    // Circle through three spread samples.
    let n = points.len();
    let (p, q, r) = (points[0], points[n / 3], points[2 * n / 3]);
    let (u, v) = (sub(q, p), sub(r, p));
    let w = cross(u, v);
    let ww = dot(w, w);
    if ww < 1e-18 {
        return EdgeCurve::Other;
    }
    let num = cross(
        [
            dot(u, u) * v[0] - dot(v, v) * u[0],
            dot(u, u) * v[1] - dot(v, v) * u[1],
            dot(u, u) * v[2] - dot(v, v) * u[2],
        ],
        w,
    );
    let center = [
        p[0] + num[0] / (2.0 * ww),
        p[1] + num[1] / (2.0 * ww),
        p[2] + num[2] / (2.0 * ww),
    ];
    let radius = norm(sub(p, center));
    let axis = unit(w);
    let fits = points
        .iter()
        .all(|x| (norm(sub(*x, center)) - radius).abs() < tol && dot(sub(*x, center), axis).abs() < tol);
    if fits {
        EdgeCurve::Circle { center, axis, radius }
    } else {
        EdgeCurve::Other
    }
}

/// Outward normal of `face` near `p`: exact from the surface, else from the nearest triangle.
fn face_normal(face: &Face, tf: &TopoFace, p: P3) -> P3 {
    let surface = face.oriented_surface();
    let exact = surface
        .search_nearest_parameter(Point3::new(p[0], p[1], p[2]), SPHint2D::None, 100)
        .map(|(u, v)| surface.normal(u, v))
        .filter(|n| n.magnitude2() > 0.5);
    if let Some(n) = exact {
        return [n.x, n.y, n.z];
    }
    tf.tris
        .iter()
        .min_by(|a, b| point_triangle(p, **a).total_cmp(&point_triangle(p, **b)))
        .map_or([0.0, 0.0, 1.0], |t| unit(cross(sub(t[1], t[0]), sub(t[2], t[0]))))
}

/// Cylinder axis and radius from a face's surface: truck builds cylinders by revolving a line.
fn cylinder_of(face: &Face, tf: &TopoFace) -> Option<(P3, P3, f64)> {
    if tf.plane.is_some() {
        return None;
    }
    let pts: Vec<P3> = tf.tris.iter().flatten().copied().collect();
    if pts.len() < 6 {
        return None;
    }
    // Axis direction from the normals: all normals are perpendicular to it. Exact normals are
    // costly (a surface search each), so compare a few spread-out samples, not every vertex.
    let n0 = face_normal(face, tf, tf.tris[0][0]);
    let step = (tf.tris.len() / 16).max(1);
    let (far, n1) = tf
        .tris
        .iter()
        .step_by(step)
        .map(|t| (t[0], face_normal(face, tf, t[0])))
        .max_by(|a, b| norm(cross(a.1, n0)).total_cmp(&norm(cross(b.1, n0))))?;
    let axis = cross(n0, n1);
    if norm(axis) < 1e-6 {
        return None;
    }
    let axis = unit(axis);
    // Lines along the two normals meet on the axis (projected onto a plane across the axis).
    let flat = |p: P3| sub(p, axis.map(|a| a * dot(p, axis)));
    let (p0, p1) = (flat(tf.tris[0][0]), flat(far));
    let (d0, d1) = (flat(n0), flat(n1));
    let w = cross(d0, d1);
    let ww = dot(w, w);
    if ww < 1e-12 {
        return None;
    }
    let t = dot(cross(sub(p1, p0), d1), w) / ww;
    let c = [p0[0] + d0[0] * t, p0[1] + d0[1] * t, p0[2] + d0[2] * t];
    let radius = norm(sub(p0, c));
    let ok = pts
        .iter()
        .all(|p| (norm(sub(flat(*p), c)) - radius).abs() < 1e-2 * radius.max(1.0));
    ok.then_some((c, axis, radius))
}

/// Faces, edges and vertices of a solid; `tol` is the tessellation tolerance for the faces.
pub fn analyze(solid: &Solid, tol: f64) -> Topology {
    let meshed = solid.triangulation(tol);
    let mut out = Topology::default();
    let faces: Vec<Face> = solid.face_iter().cloned().collect();
    for mface in meshed.face_iter() {
        let mut tris = Vec::new();
        if let Some(mut poly) = mface.surface() {
            if !mface.orientation() {
                poly.invert();
            }
            let pos = poly.positions();
            for t in poly.faces().triangle_iter() {
                tris.push(t.map(|v| v3(pos[v.pos])));
            }
        }
        let areas: Vec<f64> = tris
            .iter()
            .map(|t| 0.5 * norm(cross(sub(t[1], t[0]), sub(t[2], t[0]))))
            .collect();
        let (best, _) = areas
            .iter()
            .enumerate()
            .fold((0, 0.0), |acc, (i, &a)| if a > acc.1 { (i, a) } else { acc });
        let sample = tris.get(best).map_or([0.0; 3], |t| {
            (0..3)
                .map(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.0)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        });
        let n = tris.get(best).map(|t| unit(cross(sub(t[1], t[0]), sub(t[2], t[0]))));
        let planar = n.is_some_and(|n| {
            tris.iter()
                .zip(&areas)
                .all(|(t, &a)| a < 1e-12 || dot(unit(cross(sub(t[1], t[0]), sub(t[2], t[0]))), n) > 1.0 - 1e-9)
        });
        out.faces.push(TopoFace {
            plane: (planar && n.is_some()).then(|| Plane::from_origin_normal(sample, n.unwrap())),
            cylinder: None,
            tris,
            area: areas.iter().sum(),
            sample,
        });
    }
    for (i, f) in faces.iter().enumerate() {
        if let Some(tf) = out.faces.get(i) {
            let c = cylinder_of(f, tf);
            out.faces[i].cylinder = c;
        }
    }

    let mut vertex_index = HashMap::new();
    for v in solid.vertex_iter() {
        vertex_index.entry(v.id()).or_insert_with(|| {
            out.vertices.push(v3(v.point()));
            out.vertices.len() - 1
        });
    }
    out.vertex_faces = vec![Vec::new(); out.vertices.len()];

    // Edge id -> (face index, oriented edge as that face's boundary runs).
    let mut uses: HashMap<EdgeID, Vec<(usize, Edge)>> = HashMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for w in f.boundaries() {
            for e in w.edge_iter() {
                uses.entry(e.id()).or_default().push((fi, e.clone()));
            }
        }
    }
    let mut seen = HashMap::new();
    for e in solid.edge_iter() {
        if seen.insert(e.id(), ()).is_some() {
            continue;
        }
        let sides_src = uses.get(&e.id()).cloned().unwrap_or_default();
        let oriented = sides_src.first().map_or(e.clone(), |(_, oe)| oe.clone());
        let curve = oriented.oriented_curve();
        let (t0, t1) = curve.range_tuple();
        let points: Vec<P3> = (0..=32)
            .map(|k| v3(curve.subs(t0 + (t1 - t0) * k as f64 / 32.0)))
            .collect();
        let length = points.windows(2).map(|w| norm(sub(w[1], w[0]))).sum();
        let tm = 0.5 * (t0 + t1);
        let mid = v3(curve.subs(tm));
        let tangent = {
            let d = curve.der(tm);
            unit([d.x, d.y, d.z])
        };
        let sides = sides_src
            .iter()
            .map(|(fi, oe)| {
                let normal = unit(face_normal(&faces[*fi], &out.faces[*fi], mid));
                // This face's boundary runs along `oe`: the face lies to its left (n × d).
                let dir = {
                    let c = oe.oriented_curve();
                    let (a, b) = c.range_tuple();
                    let d = c.der(0.5 * (a + b));
                    unit([d.x, d.y, d.z])
                };
                EdgeSide {
                    face: *fi,
                    normal,
                    inward: unit(cross(normal, dir)),
                }
            })
            .collect::<Vec<_>>();
        let vertices = [
            vertex_index[&oriented.front().id()],
            vertex_index[&oriented.back().id()],
        ];
        for s in &sides {
            for v in vertices {
                if !out.vertex_faces[v].contains(&s.face) {
                    out.vertex_faces[v].push(s.face);
                }
            }
        }
        out.edges.push(TopoEdge {
            curve: classify(&points),
            points,
            length,
            mid,
            tangent,
            sides,
            vertices,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesher::render_mesh;
    use crate::operations::{extrude_circle, extrude_polygon};

    #[test]
    fn box_edges_are_lines_with_two_faces_and_inward_directions() {
        let s = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 10.0).unwrap();
        let t = analyze(&s, 0.05);
        assert_eq!((t.faces.len(), t.edges.len(), t.vertices.len()), (6, 12, 8));
        let m = render_mesh(&s, 0.05);
        assert_eq!(m.edges.len(), t.edges.len(), "same order as the render mesh");
        for (a, b) in m.edges.iter().zip(&t.edges) {
            let p = a[a.len() / 2];
            assert!(b.points.iter().any(|q| norm(sub(*q, p.map(f64::from))) < 1.0));
        }
        for e in &t.edges {
            assert!(matches!(e.curve, EdgeCurve::Line { .. }));
            assert_eq!(e.sides.len(), 2);
            for s in &e.sides {
                // Stepping inward from the edge midpoint stays on the face.
                let p = [0, 1, 2].map(|i| e.mid[i] + s.inward[i] * 0.5);
                assert!(t.faces[s.face].distance(p) < 1e-6, "{e:?}");
                // Convex edge: the other face's normal points away from this face.
                let other = e.sides.iter().find(|o| o.face != s.face).unwrap();
                assert!(dot(s.inward, other.normal) < -0.5);
            }
        }
        assert!(t.vertex_faces.iter().all(|f| f.len() == 3));
    }

    #[test]
    fn cylinder_has_circular_edges_and_a_cylindrical_face() {
        let s = extrude_circle([1.0, 2.0], 4.0, 10.0).unwrap();
        let t = analyze(&s, 0.01);
        let circles: Vec<_> = t
            .edges
            .iter()
            .filter_map(|e| match e.curve {
                EdgeCurve::Circle { center, radius, .. } => Some((center, radius)),
                _ => None,
            })
            .collect();
        assert!(!circles.is_empty());
        assert!(circles
            .iter()
            .all(|(c, r)| (r - 4.0).abs() < 1e-6 && (c[0] - 1.0).abs() < 1e-6));
        let cyl = t
            .faces
            .iter()
            .find_map(|f| f.cylinder)
            .expect("side face is a cylinder");
        assert!((cyl.2 - 4.0).abs() < 1e-3 && cyl.1[2].abs() > 0.999, "{cyl:?}");
    }
}
