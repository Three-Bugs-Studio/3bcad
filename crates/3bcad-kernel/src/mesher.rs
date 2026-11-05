//! B-Rep -> triangles. truck's tessellator is chord-tolerance driven, so curved
//! faces get more triangles where they bend most (round holes stay round).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use truck_meshalgo::prelude::*;
use truck_modeling::Solid;

use crate::solid::Plane;

/// Triangles that remember their B-Rep face, plus B-Rep edges and vertices, for display and picking.
#[derive(Clone, Debug, Default)]
pub struct RenderMesh {
    /// Flat triangle list (three entries per triangle).
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Face index (into `faces`) of each entry of `positions`.
    pub face_of: Vec<u32>,
    /// One entry per B-Rep face: its plane when the face is planar.
    pub faces: Vec<Option<Plane>>,
    /// One polyline per B-Rep edge.
    pub edges: Vec<Vec<[f32; 3]>>,
    pub vertices: Vec<[f32; 3]>,
}

/// Welded, indexed triangle mesh for printing (STL / 3MF).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PrintMesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[usize; 3]>,
}

fn f32x3(v: [f64; 3]) -> [f32; 3] {
    v.map(|c| c as f32)
}

/// `tol` is the max distance (model units) between mesh and true surface.
pub fn render_mesh(solid: &Solid, tol: f64) -> RenderMesh {
    let meshed = solid.robust_triangulation(tol);
    let mut out = RenderMesh::default();
    for face in meshed.face_iter() {
        let index = out.faces.len() as u32;
        let Some(mut poly) = face.surface() else {
            out.faces.push(None);
            continue;
        };
        if !face.orientation() {
            poly.invert();
        }
        let (pos, nor) = (poly.positions(), poly.normals());
        let mut normal: Option<Vector3> = None;
        let mut planar = true;
        for tri in poly.faces().triangle_iter() {
            let p = tri.map(|v| pos[v.pos]);
            let flat = (p[1] - p[0]).cross(p[2] - p[0]);
            if flat.magnitude2() > 0.0 {
                let flat = flat.normalize();
                match normal {
                    None => normal = Some(flat),
                    Some(n) => planar &= n.dot(flat) > 1.0 - 1e-9,
                }
            }
            let flat = if flat.magnitude2() > 0.0 {
                flat.normalize()
            } else {
                Vector3::unit_z()
            };
            for (v, pt) in tri.iter().zip(p) {
                out.positions.push(f32x3(pt.into()));
                out.normals.push(f32x3(v.nor.map_or(flat, |i| nor[i]).into()));
                out.face_of.push(index);
            }
        }
        let plane = match (planar, normal, pos.first()) {
            (true, Some(n), Some(&o)) => Some(Plane::from_origin_normal(o.into(), n.into())),
            _ => None,
        };
        out.faces.push(plane);
    }
    let mut seen = HashSet::new();
    for edge in meshed.edge_iter() {
        if seen.insert(edge.id()) {
            out.edges
                .push(edge.curve().0.iter().map(|&p| f32x3(p.into())).collect());
        }
    }
    let mut seen = HashSet::new();
    for v in meshed.vertex_iter() {
        if seen.insert(v.id()) {
            out.vertices.push(f32x3(v.point().into()));
        }
    }
    out
}

/// What `Solid::robust_triangulation` returns.
type Meshed = truck_topology::Solid<Point3, PolylineCurve<Point3>, Option<PolygonMesh>>;

/// The finest chord tolerance the angle tolerance may drive an export to (mm).
const MIN_TOL: f64 = 1e-4;

/// Watertight mesh: faces share their boundary polylines, and welding merges
/// the duplicated positions so every edge is used by exactly two triangles.
pub fn print_mesh(solid: &Solid, tol: f64) -> PrintMesh {
    welded(&solid.robust_triangulation(tol))
}

/// `print_mesh` within a chord tolerance (mm) and an angle tolerance (radians): curved faces are
/// meshed finer until no triangle spans more than `angle` of the surface's turn.
pub fn print_mesh_within(solid: &Solid, chord: f64, angle: f64) -> PrintMesh {
    let mut tol = chord;
    let mut meshed = solid.robust_triangulation(tol);
    for _ in 0..6 {
        let turn = max_turn(&meshed);
        if turn <= angle || tol <= MIN_TOL {
            break;
        }
        // A facet's turn grows with the square root of its chord deviation.
        tol = (tol * (angle / turn).powi(2) * 0.8).max(MIN_TOL);
        meshed = solid.robust_triangulation(tol);
    }
    welded(&meshed)
}

/// The widest angle (radians) between the surface normals at the corners of one triangle.
fn max_turn(meshed: &Meshed) -> f64 {
    meshed
        .face_iter()
        .filter_map(|f| f.surface())
        .flat_map(|poly| {
            let nor = poly.normals().to_vec();
            poly.faces()
                .triangle_iter()
                .filter_map(|t| {
                    let n = [t[0].nor?, t[1].nor?, t[2].nor?].map(|i| nor[i].normalize());
                    Some(
                        (0..3)
                            .map(|k| n[k].dot(n[(k + 1) % 3]).clamp(-1.0, 1.0).acos())
                            .fold(0.0, f64::max),
                    )
                })
                .collect::<Vec<_>>()
        })
        .fold(0.0, f64::max)
}

fn welded(meshed: &Meshed) -> PrintMesh {
    let poly = meshed.to_polygon();
    let (vertices, remap) = weld(poly.positions(), TOLERANCE);
    PrintMesh {
        vertices,
        triangles: poly
            .faces()
            .triangle_iter()
            .map(|t| t.map(|v| remap[v.pos]))
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0])
            .collect(),
    }
}

/// Merges positions closer than `eps`: the kept positions and each input's index among them.
/// A hash grid of `eps` cells, so it stays linear (truck's own welding is quadratic, seconds on
/// a thread's mesh).
fn weld(positions: &[Point3], eps: f64) -> (Vec<[f64; 3]>, Vec<usize>) {
    use std::collections::HashMap;
    let cell = |p: &Point3| {
        [
            (p.x / eps).floor() as i64,
            (p.y / eps).floor() as i64,
            (p.z / eps).floor() as i64,
        ]
    };
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    let mut kept: Vec<[f64; 3]> = Vec::new();
    let mut remap = Vec::with_capacity(positions.len());
    for p in positions {
        let c = cell(p);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(ids) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &i in ids {
                            let q = kept[i];
                            if (q[0] - p.x).powi(2) + (q[1] - p.y).powi(2) + (q[2] - p.z).powi(2) <= eps * eps {
                                found = Some(i);
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        let i = found.unwrap_or_else(|| {
            kept.push([p.x, p.y, p.z]);
            grid.entry(c).or_default().push(kept.len() - 1);
            kept.len() - 1
        });
        remap.push(i);
    }
    (kept, remap)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::f64::consts::PI;

    use super::*;
    use crate::operations::{extrude, extrude_circle, extrude_polygon, ProfileRegion, ProfileSegment};

    fn volume(m: &PrintMesh) -> f64 {
        m.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| m.vertices[i]);
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }

    /// Closed and consistently oriented: each directed edge appears exactly once
    /// and its reverse exactly once.
    fn assert_watertight(m: &PrintMesh) {
        let mut edges = HashMap::new();
        for t in &m.triangles {
            for i in 0..3 {
                *edges.entry((t[i], t[(i + 1) % 3])).or_insert(0) += 1;
            }
        }
        for (&(a, b), &n) in &edges {
            assert_eq!(n, 1, "edge {a}-{b} used {n} times");
            assert_eq!(edges.get(&(b, a)), Some(&1), "edge {a}-{b} has no twin");
        }
    }

    #[test]
    fn box_is_watertight_with_positive_volume() {
        // Clockwise profile and negative depth both exercise the orientation fix.
        for (pts, depth) in [
            (vec![[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]], 30.0),
            (vec![[0.0, 0.0], [0.0, 20.0], [10.0, 20.0], [10.0, 0.0]], 30.0),
            (vec![[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]], -30.0),
        ] {
            let m = print_mesh(&extrude_polygon(&pts, depth).unwrap(), 0.01);
            assert_watertight(&m);
            assert!((volume(&m) - 6000.0).abs() < 1e-6, "volume {}", volume(&m));
        }
    }

    #[test]
    fn cylinder_is_watertight_and_round() {
        for depth in [10.0, -10.0] {
            let m = print_mesh(&extrude_circle([5.0, 5.0], 4.0, depth).unwrap(), 0.01);
            assert_watertight(&m);
            let exact = PI * 16.0 * 10.0;
            assert!((volume(&m) - exact).abs() / exact < 0.01, "volume {}", volume(&m));
        }
    }

    fn plate_with_hole() -> ProfileRegion {
        let sq = [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];
        ProfileRegion {
            outer: (0..4)
                .map(|i| ProfileSegment::Line {
                    from: sq[i],
                    to: sq[(i + 1) % 4],
                })
                .collect(),
            holes: vec![vec![ProfileSegment::Circle {
                center: [10.0, 10.0],
                radius: 6.0,
                ccw: false,
            }]],
        }
    }

    #[test]
    fn region_with_hole_on_every_plane() {
        let expected = (400.0 - PI * 36.0) * 5.0;
        for plane in [
            Plane::TOP,
            Plane::FRONT,
            Plane::RIGHT,
            Plane::from_origin_normal([1.0, 2.0, 3.0], [1.0, 1.0, 1.0]),
        ] {
            for depth in [5.0, -5.0] {
                let solids = extrude(&[plate_with_hole()], depth, &plane).unwrap();
                let m = print_mesh(&solids[0], 0.01);
                assert_watertight(&m);
                assert!(
                    (volume(&m) - expected).abs() / expected < 0.01,
                    "{plane:?} {depth}: {}",
                    volume(&m)
                );
            }
        }
        // On the Front plane the solid grows towards -Y (the plane normal).
        let solid = &extrude(&[plate_with_hole()], 5.0, &Plane::FRONT).unwrap()[0];
        let m = print_mesh(solid, 0.1);
        assert!(m.vertices.iter().all(|v| v[1] <= 1e-9 && v[1] >= -5.0 - 1e-9));
    }

    #[test]
    fn arcs_and_beziers_extrude() {
        // A D shape: a line closed by a half-circle arc, and a lens of two Bézier spans.
        let d = ProfileRegion {
            outer: vec![
                ProfileSegment::Line {
                    from: [0.0, 2.0],
                    to: [0.0, -2.0],
                },
                ProfileSegment::Arc {
                    center: [0.0, 0.0],
                    from: [0.0, -2.0],
                    to: [0.0, 2.0],
                    ccw: true,
                },
            ],
            holes: vec![],
        };
        let m = print_mesh(&extrude(&[d], 1.0, &Plane::TOP).unwrap()[0], 0.001);
        assert_watertight(&m);
        assert!((volume(&m) - PI * 2.0).abs() < 0.01, "{}", volume(&m));
        let lens = ProfileRegion {
            outer: vec![
                ProfileSegment::Bezier([[0.0, 0.0], [1.0, -1.0], [3.0, -1.0], [4.0, 0.0]]),
                ProfileSegment::Bezier([[4.0, 0.0], [3.0, 1.0], [1.0, 1.0], [0.0, 0.0]]),
            ],
            holes: vec![],
        };
        let m = print_mesh(&extrude(&[lens], 2.0, &Plane::TOP).unwrap()[0], 0.001);
        assert_watertight(&m);
        assert!(volume(&m) > 0.0);
    }

    #[test]
    fn an_angle_tolerance_rounds_out_a_coarse_cylinder() {
        let cyl = extrude_circle([0.0, 0.0], 4.0, 10.0).unwrap();
        let exact = PI * 16.0 * 10.0;
        let coarse = print_mesh_within(&cyl, 0.5, PI);
        let fine = print_mesh_within(&cyl, 0.5, 5f64.to_radians());
        assert_watertight(&fine);
        assert!(fine.triangles.len() > coarse.triangles.len());
        // 5° facets: a 72-gon, 0.13 % short of the circle.
        assert!(
            (volume(&fine) - exact).abs() / exact < 0.003,
            "volume {}",
            volume(&fine)
        );
        assert!((volume(&coarse) - exact).abs() > (volume(&fine) - exact).abs());
    }

    #[test]
    fn render_mesh_keeps_faces_edges_and_planes() {
        let solid = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 5.0).unwrap();
        let m = render_mesh(&solid, 0.05);
        assert_eq!(m.faces.len(), 6);
        assert!(m.faces.iter().all(Option::is_some), "every box face is planar");
        assert_eq!(m.edges.len(), 12);
        assert_eq!(m.vertices.len(), 8);
        assert_eq!(m.face_of.len(), m.positions.len());
        let top = m.faces.iter().flatten().find(|p| p.normal()[2] > 0.5).unwrap();
        assert!((top.origin[2] - 5.0).abs() < 1e-9);
        // Triangles wind counter-clockwise seen from outside: the print checks rely on it.
        for t in m.positions.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|p| p.map(f64::from));
            let n = crate::topology::cross(crate::topology::sub(b, a), crate::topology::sub(c, a));
            let out = crate::topology::sub(a, [5.0, 5.0, 2.5]);
            assert!(crate::topology::dot(n, out) > 0.0);
        }

        let cyl = render_mesh(&extrude_circle([0.0, 0.0], 4.0, 10.0).unwrap(), 0.05);
        assert_eq!(
            cyl.faces.iter().filter(|f| f.is_some()).count(),
            2,
            "only the caps are planar"
        );
        assert!(cyl.faces.iter().any(Option::is_none));
    }

    #[test]
    fn rejects_degenerate_input() {
        assert!(extrude_polygon(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], 5.0).is_err());
        assert!(extrude_circle([0.0, 0.0], 3.0, 0.0).is_err());
    }
}
