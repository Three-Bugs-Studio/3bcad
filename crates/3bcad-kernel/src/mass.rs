//! Mass properties from a fine, watertight tessellation (divergence theorem over signed tetrahedra).

use nalgebra::{Matrix3, SymmetricEigen};

use crate::mesher::PrintMesh;

#[derive(Clone, Debug, PartialEq)]
pub struct MassProperties {
    /// mm³
    pub volume: f64,
    /// mm²
    pub area: f64,
    /// mm
    pub center: [f64; 3],
    /// Inertia tensor about the center for unit density (mm⁵); multiply by density for mass units.
    pub inertia: [[f64; 3]; 3],
    /// Principal moments (ascending) and their axes, unit density.
    pub principal: [f64; 3],
    pub axes: [[f64; 3]; 3],
}

/// Combined properties of several closed meshes.
pub fn mass_properties(meshes: &[PrintMesh]) -> MassProperties {
    let (mut vol, mut area) = (0.0, 0.0);
    let mut first = [0.0; 3];
    // Second moments ∫ x_i x_j dV about the origin.
    let mut second = [[0.0; 3]; 3];
    for m in meshes {
        for t in &m.triangles {
            let [a, b, c] = t.map(|i| m.vertices[i]);
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
            // Tetrahedron (0, a, b, c).
            let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]);
            let v = det / 6.0;
            vol += v;
            for i in 0..3 {
                first[i] += v * (a[i] + b[i] + c[i]) / 4.0;
                for j in 0..3 {
                    // ∫ x_i x_j over a tetrahedron with one vertex at the origin.
                    let s = a[i] * a[j] + b[i] * b[j] + c[i] * c[j];
                    let sum_i = a[i] + b[i] + c[i];
                    let sum_j = a[j] + b[j] + c[j];
                    second[i][j] += v * (s + sum_i * sum_j) / 20.0;
                }
            }
        }
    }
    let center = if vol.abs() > 1e-300 {
        first.map(|f| f / vol)
    } else {
        [0.0; 3]
    };
    // Move the second moments to the center, then I = tr(S) 1 - S.
    let mut s = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            s[i][j] = second[i][j] - vol * center[i] * center[j];
        }
    }
    let tr = s[0][0] + s[1][1] + s[2][2];
    let inertia: [[f64; 3]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|j| if i == j { tr - s[i][j] } else { -s[i][j] }));
    let eig = SymmetricEigen::new(Matrix3::from_fn(|i, j| inertia[i][j]));
    let mut order = [0, 1, 2];
    order.sort_by(|&x, &y| eig.eigenvalues[x].total_cmp(&eig.eigenvalues[y]));
    MassProperties {
        volume: vol,
        area,
        center,
        inertia,
        principal: order.map(|k| eig.eigenvalues[k]),
        axes: order.map(|k| [0, 1, 2].map(|r| eig.eigenvectors[(r, k)])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesher::print_mesh;
    use crate::operations::extrude_polygon;

    #[test]
    fn box_mass_properties_match_formulas() {
        // 10 x 20 x 30 box from the origin.
        let s = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]], 30.0).unwrap();
        let p = mass_properties(&[print_mesh(&s, 0.01)]);
        assert!((p.volume - 6000.0).abs() < 1e-6);
        assert!((p.area - 2.0 * (200.0 + 300.0 + 600.0)).abs() < 1e-6);
        assert!((0..3).all(|i| (p.center[i] - [5.0, 10.0, 15.0][i]).abs() < 1e-9));
        let m = 6000.0;
        let ixx = m * (20.0f64.powi(2) + 30.0f64.powi(2)) / 12.0;
        let iyy = m * (10.0f64.powi(2) + 30.0f64.powi(2)) / 12.0;
        let izz = m * (10.0f64.powi(2) + 20.0f64.powi(2)) / 12.0;
        assert!((p.inertia[0][0] - ixx).abs() < 1e-3 * ixx, "{:?}", p.inertia);
        assert!((p.inertia[1][1] - iyy).abs() < 1e-3 * iyy);
        assert!((p.inertia[2][2] - izz).abs() < 1e-3 * izz);
        assert!(p.inertia[0][1].abs() < 1e-3);
        assert!((p.principal[0] - izz).abs() < 1e-3 * izz && (p.principal[2] - ixx).abs() < 1e-3 * ixx);
    }
}
