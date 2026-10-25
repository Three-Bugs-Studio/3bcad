use std::f64::consts::PI;

use anyhow::{anyhow, ensure, Result};
use truck_modeling::*;

/// Extrudes a closed polygon on the XY plane along Z by `depth` (may be negative).
pub fn extrude_polygon(points: &[[f64; 2]], depth: f64) -> Result<Solid> {
    ensure!(points.len() >= 3, "profile needs at least 3 points");
    ensure!(depth.abs() > TOLERANCE, "extrude depth must be non-zero");
    let area2: f64 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    ensure!(area2.abs() > TOLERANCE, "profile has zero area");

    let mut pts = points.to_vec();
    // The profile face must face the sweep direction, otherwise the solid comes out inside-out.
    if (area2 > 0.0) != (depth > 0.0) {
        pts.reverse();
    }
    let v: Vec<Vertex> = pts
        .iter()
        .map(|p| builder::vertex(Point3::new(p[0], p[1], 0.0)))
        .collect();
    let wire: Wire = (0..v.len())
        .map(|i| builder::line(&v[i], &v[(i + 1) % v.len()]))
        .collect();
    sweep(wire, depth)
}

/// Extrudes a circle on the XY plane along Z by `depth` (may be negative).
pub fn extrude_circle(center: [f64; 2], radius: f64, depth: f64) -> Result<Solid> {
    ensure!(radius > TOLERANCE, "circle radius must be positive");
    ensure!(depth.abs() > TOLERANCE, "extrude depth must be non-zero");
    let c = Point3::new(center[0], center[1], 0.0);
    let v = builder::vertex(c + Vector3::new(radius, 0.0, 0.0));
    let wire: Wire = builder::rsweep(&v, c, Vector3::unit_z() * depth.signum(), Rad(2.0 * PI));
    sweep(wire, depth)
}

fn sweep(wire: Wire, depth: f64) -> Result<Solid> {
    let face = builder::try_attach_plane(&[wire]).map_err(|e| anyhow!("cannot build profile face: {e:?}"))?;
    Ok(builder::tsweep(&face, Vector3::new(0.0, 0.0, depth)))
}
