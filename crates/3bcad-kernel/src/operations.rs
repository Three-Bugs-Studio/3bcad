use std::f64::consts::{PI, TAU};

use anyhow::{anyhow, bail, ensure, Result};
use truck_modeling::*;

use crate::solid::Plane;

/// One piece of a planar profile loop, in plane coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum ProfileSegment {
    Line {
        from: [f64; 2],
        to: [f64; 2],
    },
    /// Circular arc around `center`, counter-clockwise from `from` to `to` when `ccw`.
    Arc {
        center: [f64; 2],
        from: [f64; 2],
        to: [f64; 2],
        ccw: bool,
    },
    /// A full circle; only valid as a loop by itself.
    Circle {
        center: [f64; 2],
        radius: f64,
        ccw: bool,
    },
    Bezier([[f64; 2]; 4]),
}

/// A face to extrude: counter-clockwise outer loop and clockwise holes.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileRegion {
    pub outer: Vec<ProfileSegment>,
    pub holes: Vec<Vec<ProfileSegment>>,
}

/// Extrudes each region along the plane normal by `depth` (may be negative): one solid per region.
pub fn extrude(regions: &[ProfileRegion], depth: f64, plane: &Plane) -> Result<Vec<Solid>> {
    ensure!(depth.abs() > TOLERANCE, "extrude depth must be non-zero");
    ensure!(!regions.is_empty(), "nothing to extrude");
    let m = plane.matrix();
    let to_world = Matrix4::new(
        m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1], m[2][2], m[2][3],
        m[3][0], m[3][1], m[3][2], m[3][3],
    );
    regions
        .iter()
        .map(|r| {
            let mut wires = vec![wire(&r.outer)?];
            for h in &r.holes {
                wires.push(wire(h)?);
            }
            // The profile face must face the sweep direction, otherwise the solid comes out inside-out.
            if depth < 0.0 {
                wires.iter_mut().for_each(|w| {
                    w.invert();
                });
            }
            let face = builder::try_attach_plane(&wires).map_err(|e| anyhow!("cannot build profile face: {e:?}"))?;
            let solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, depth));
            Ok(builder::transformed(&solid, to_world))
        })
        .collect()
}

fn pt([x, y]: [f64; 2]) -> Point3 {
    Point3::new(x, y, 0.0)
}

fn wire(lp: &[ProfileSegment]) -> Result<Wire> {
    if let [ProfileSegment::Circle { center, radius, ccw }] = *lp {
        ensure!(radius > TOLERANCE, "circle radius must be positive");
        let v = builder::vertex(pt(center) + Vector3::new(radius, 0.0, 0.0));
        let axis = if ccw { Vector3::unit_z() } else { -Vector3::unit_z() };
        return Ok(builder::rsweep(&v, pt(center), axis, Rad(TAU)));
    }
    ensure!(!lp.is_empty(), "empty profile loop");
    let start = |s: &ProfileSegment| match *s {
        ProfileSegment::Line { from, .. } | ProfileSegment::Arc { from, .. } => Ok(from),
        ProfileSegment::Bezier(b) => Ok(b[0]),
        ProfileSegment::Circle { .. } => Err(anyhow!("a circle must be a loop by itself")),
    };
    let vertices: Vec<Vertex> = lp
        .iter()
        .map(|s| start(s).map(|p| builder::vertex(pt(p))))
        .collect::<Result<_>>()?;
    let n = lp.len();
    lp.iter()
        .enumerate()
        .map(|(i, s)| {
            let (v0, v1) = (&vertices[i], &vertices[(i + 1) % n]);
            Ok(match *s {
                ProfileSegment::Line { .. } => builder::line(v0, v1),
                ProfileSegment::Arc { center, from, to, ccw } => {
                    let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
                    let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
                    let mut sweep = (a1 - a0).rem_euclid(TAU);
                    if sweep < 1e-12 {
                        sweep = TAU;
                    }
                    if !ccw {
                        sweep -= TAU;
                    }
                    let r = (from[0] - center[0]).hypot(from[1] - center[1]);
                    let mid = a0 + sweep / 2.0;
                    if sweep.abs() >= PI * 2.0 - 1e-9 {
                        bail!("an arc cannot close on itself; use a circle");
                    }
                    builder::circle_arc(v0, v1, pt([center[0] + r * mid.cos(), center[1] + r * mid.sin()]))
                }
                ProfileSegment::Bezier(b) => builder::bezier(v0, v1, vec![pt(b[1]), pt(b[2])]),
                ProfileSegment::Circle { .. } => unreachable!(),
            })
        })
        .collect()
}

/// Extrudes a closed polygon on the Top (XY) plane.
pub fn extrude_polygon(points: &[[f64; 2]], depth: f64) -> Result<Solid> {
    ensure!(points.len() >= 3, "profile needs at least 3 points");
    let area2: f64 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    ensure!(area2.abs() > TOLERANCE, "profile has zero area");
    let mut pts = points.to_vec();
    if area2 < 0.0 {
        pts.reverse();
    }
    let outer = (0..pts.len())
        .map(|i| ProfileSegment::Line {
            from: pts[i],
            to: pts[(i + 1) % pts.len()],
        })
        .collect();
    Ok(extrude(
        &[ProfileRegion {
            outer,
            holes: Vec::new(),
        }],
        depth,
        &Plane::TOP,
    )?
    .remove(0))
}

/// Extrudes a circle on the Top (XY) plane.
pub fn extrude_circle(center: [f64; 2], radius: f64, depth: f64) -> Result<Solid> {
    let outer = vec![ProfileSegment::Circle {
        center,
        radius,
        ccw: true,
    }];
    Ok(extrude(
        &[ProfileRegion {
            outer,
            holes: Vec::new(),
        }],
        depth,
        &Plane::TOP,
    )?
    .remove(0))
}
