//! B-Rep -> triangles. truck's tessellator is chord-tolerance driven, so curved
//! faces get more triangles where they bend most (round holes stay round).

use std::collections::HashSet;

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
#[derive(Clone, Debug, Default)]
pub struct PrintMesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[usize; 3]>,
}

fn f32x3(v: [f64; 3]) -> [f32; 3] {
    v.map(|c| c as f32)
}

/// `tol` is the max distance (model units) between mesh and true surface.
pub fn render_mesh(solid: &Solid, tol: f64) -> RenderMesh {
    let meshed = solid.triangulation(tol);
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

/// Watertight mesh: faces share their boundary polylines, and welding merges
/// the duplicated positions so every edge is used by exactly two triangles.
pub fn print_mesh(solid: &Solid, tol: f64) -> PrintMesh {
    let mut poly = solid.triangulation(tol).to_polygon();
    poly.put_together_same_attrs(TOLERANCE);
    PrintMesh {
        vertices: poly.positions().iter().map(|&p| p.into()).collect(),
        triangles: poly.faces().triangle_iter().map(|t| t.map(|v| v.pos)).collect(),
    }
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
