//! B-Rep -> triangles. truck's tessellator is chord-tolerance driven, so curved
//! faces get more triangles where they bend most (round holes stay round).

use truck_meshalgo::prelude::*;
use truck_modeling::Solid;

/// Flat triangle list with per-vertex normals plus B-Rep edges as line segments.
#[derive(Clone, Debug, Default)]
pub struct RenderMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Pairs of points (line list).
    pub edges: Vec<[f32; 3]>,
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
    let poly = meshed.to_polygon();
    let (pos, nor) = (poly.positions(), poly.normals());
    let mut out = RenderMesh::default();
    for tri in poly.faces().triangle_iter() {
        let p = tri.map(|v| pos[v.pos]);
        let flat = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
        for (v, pt) in tri.iter().zip(p) {
            out.positions.push(f32x3(pt.into()));
            out.normals.push(f32x3(v.nor.map_or(flat, |i| nor[i]).into()));
        }
    }
    for edge in meshed.edge_iter() {
        for w in edge.curve().0.windows(2) {
            out.edges.extend([f32x3(w[0].into()), f32x3(w[1].into())]);
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

    use super::*;
    use crate::operations::{extrude_circle, extrude_polygon};

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
            let exact = std::f64::consts::PI * 16.0 * 10.0;
            assert!((volume(&m) - exact).abs() / exact < 0.01, "volume {}", volume(&m));
        }
    }

    #[test]
    fn render_mesh_has_triangles_and_edges() {
        let m = render_mesh(&extrude_circle([0.0, 0.0], 4.0, 10.0).unwrap(), 0.05);
        assert!(m.positions.len() >= 3 && m.positions.len().is_multiple_of(3));
        assert_eq!(m.positions.len(), m.normals.len());
        assert!(!m.edges.is_empty() && m.edges.len().is_multiple_of(2));
    }

    #[test]
    fn rejects_degenerate_input() {
        assert!(extrude_polygon(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], 5.0).is_err());
        assert!(extrude_circle([0.0, 0.0], 3.0, 0.0).is_err());
    }
}
