//! Domes: a smooth cap standing on a flat face. A circle gets an exact spherical cap (a circular
//! face of a body is domed in place by `round::dome_circle` instead); any other
//! (star-shaped) outline is lofted through copies of itself shrunk towards its centre on an
//! elliptical profile, closing in a small flat top.

use std::f64::consts::FRAC_PI_2;

use anyhow::{ensure, Result};
use truck_modeling::Solid;

use crate::operations::{ProfileRegion, ProfileSegment};
use crate::skin::{self, Band, Caps, Ends};
use crate::solid::Plane;
use crate::tool::Tool;

/// The cap on `outline` (a closed loop in `plane`), `height` along the plane normal (negative:
/// below the plane, for sinking a dome into the material).
pub fn dome(outline: &[ProfileSegment], plane: &Plane, height: f64) -> Result<Solid> {
    ensure!(height.abs() > 1e-6, "the dome height must not be zero");
    if let [ProfileSegment::Circle { center, radius, .. }] = *outline {
        let (r, h) = (radius, height.abs());
        // Sphere through the rim and the top: centre on the axis at c.
        let c = (h * h - r * r) / (2.0 * h);
        let n = plane.normal();
        let up = if height > 0.0 { n } else { n.map(|v| -v) };
        let o = plane.to_world(center);
        let section = Plane {
            origin: o,
            x: plane.x,
            y: up,
        };
        let region = ProfileRegion {
            outer: vec![
                ProfileSegment::Line {
                    from: [0.0, 0.0],
                    to: [r, 0.0],
                },
                ProfileSegment::Arc {
                    center: [0.0, c],
                    from: [r, 0.0],
                    to: [0.0, h],
                    ccw: true,
                },
                ProfileSegment::Line {
                    from: [0.0, h],
                    to: [0.0, 0.0],
                },
            ],
            holes: Vec::new(),
        };
        let tool = Tool::Revolution {
            regions: vec![region],
            plane: section,
            axis: ([0.0, 0.0], [0.0, 1.0]),
            a0: 0.0,
            a1: std::f64::consts::TAU,
        };
        return Ok(tool.solids()?.remove(0));
    }
    let pts = crate::offset::sample_loop(outline, 32);
    ensure!(pts.len() >= 3, "the face outline is too small");
    let k = pts.len() as f64;
    let c = [
        pts.iter().map(|p| p[0]).sum::<f64>() / k,
        pts.iter().map(|p| p[1]).sum::<f64>() / k,
    ];
    let base: Vec<_> = skin::loop_pieces(outline).0;
    let m = plane.matrix();
    let levels = 10;
    let top = 0.94 * FRAC_PI_2;
    let mut sections = Vec::new();
    let mut params = Vec::new();
    for i in 0..=levels {
        let th = top * i as f64 / levels as f64;
        let (s, z) = (th.cos(), height * th.sin());
        // Shrink about the centre and lift, in the plane's own frame.
        let local: [[f64; 4]; 4] = [
            [s, 0.0, 0.0, 0.0],
            [0.0, s, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [c[0] * (1.0 - s), c[1] * (1.0 - s), z, 1.0],
        ];
        let full = crate::sweep::mul(&m, &local);
        sections.push(vec![base.iter().map(|p| p.transformed(&full)).collect()]);
        params.push(th);
    }
    let band = Band {
        sections,
        params,
        ends: Ends::default(),
    };
    skin::skin(&[band], Caps::Planar)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::boolean::volume;
    use crate::tool::polygon;

    #[test]
    fn a_circle_gets_a_spherical_cap() {
        let s = dome(
            &[ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: 5.0,
                ccw: true,
            }],
            &Plane::TOP,
            2.0,
        )
        .unwrap();
        // Cap volume πh²(3R − h)/3 with R = (r² + h²)/2h.
        let (r, h) = (5.0f64, 2.0f64);
        let big = (r * r + h * h) / (2.0 * h);
        let expected = PI * h * h * (3.0 * big - h) / 3.0;
        let v = volume(&s).unwrap();
        // The mesh sits inside the sphere: allow 1 %.
        assert!((v - expected).abs() / expected < 1e-2, "{v} vs {expected}");
        let (lo, hi) = crate::boolean::bbox(&s);
        assert!(lo[2].abs() < 1e-6 && (hi[2] - 2.0).abs() < 1e-3, "{lo:?} {hi:?}");
    }

    #[test]
    fn a_square_gets_a_rounded_cap_and_can_sink() {
        let sq = polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]).unwrap();
        let up = dome(&sq.outer, &Plane::TOP, 3.0).unwrap();
        let v = volume(&up).unwrap();
        // An elliptical profile: about π/4 · area · height · 2/3 … area · height.
        assert!(v > 100.0 * 3.0 * 0.5 && v < 100.0 * 3.0, "{v}");
        let (_, hi) = crate::boolean::bbox(&up);
        assert!((hi[2] - 3.0).abs() < 0.05, "{hi:?}");
        let down = dome(&sq.outer, &Plane::TOP, -3.0).unwrap();
        let (lo, _) = crate::boolean::bbox(&down);
        assert!((lo[2] + 3.0).abs() < 0.05, "{lo:?}");
        assert!((volume(&down).unwrap() - v).abs() < 1e-3 * v);
    }
}
