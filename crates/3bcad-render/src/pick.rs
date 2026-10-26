//! CPU picking against the scene: rays for faces and planes, screen-space distance for edges and
//! vertices, with occlusion so hidden edges cannot be clicked through a face.

use glam::{Vec2, Vec3};

use crate::scene::{SceneGeometry, NO_ID};
use crate::OrbitCamera;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Vertex,
    Edge,
    Face,
    Plane,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub id: u32,
    pub kind: HitKind,
    /// World point under the cursor.
    pub point: Vec3,
}

/// Ray-triangle intersection (Möller–Trumbore): distance along the ray.
fn ray_triangle(o: Vec3, d: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    let t = e2.dot(q) * inv;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
}

fn nearest_triangle<'a>(o: Vec3, d: Vec3, tris: impl Iterator<Item = ([Vec3; 3], u32)> + 'a) -> Option<(f32, u32)> {
    tris.filter_map(|(t, id)| ray_triangle(o, d, t).map(|dist| (dist, id)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// What is under `cursor` (pixels from the viewport's top-left), SolidWorks order:
/// vertices, then edges, then faces, then reference planes.
pub fn pick(scene: &SceneGeometry, cam: &OrbitCamera, viewport: Vec2, cursor: Vec2, tol_px: f32) -> Option<Hit> {
    let (o, d) = cam.ray(cursor, viewport);
    let face = nearest_triangle(
        o,
        d,
        scene
            .triangles
            .chunks_exact(3)
            .map(|t| ([t[0].pos.into(), t[1].pos.into(), t[2].pos.into()], t[0].id)),
    );
    let visible = |p: Vec3| face.is_none_or(|(t, _)| (p - o).dot(d) <= t * 1.002 + 1e-4);

    let vertex = scene
        .points
        .iter()
        .filter(|(_, id)| *id != NO_ID)
        .filter_map(|&(p, id)| {
            let p = Vec3::from(p);
            let px = cam.project(p, viewport)?;
            let dist = px.distance(cursor);
            (dist <= tol_px && visible(p)).then_some((dist, id, p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, id, point)) = vertex {
        return Some(Hit {
            id,
            kind: HitKind::Vertex,
            point,
        });
    }

    let edge = scene
        .lines
        .chunks_exact(2)
        .filter(|l| l[0].id != NO_ID)
        .filter_map(|l| {
            let (a, b) = (Vec3::from(l[0].pos), Vec3::from(l[1].pos));
            let (pa, pb) = (cam.project(a, viewport)?, cam.project(b, viewport)?);
            let ab = pb - pa;
            let t = if ab.length_squared() > 0.0 {
                ((cursor - pa).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let dist = (pa + ab * t).distance(cursor);
            let p = a.lerp(b, t);
            (dist <= tol_px && visible(p)).then_some((dist, l[0].id, p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, id, point)) = edge {
        return Some(Hit {
            id,
            kind: HitKind::Edge,
            point,
        });
    }

    if let Some((t, id)) = face {
        return Some(Hit {
            id,
            kind: HitKind::Face,
            point: o + d * t,
        });
    }

    let plane = nearest_triangle(
        o,
        d,
        scene
            .planes
            .chunks_exact(3)
            .map(|t| ([t[0].pos.into(), t[1].pos.into(), t[2].pos.into()], t[0].id)),
    );
    plane.map(|(t, id)| Hit {
        id,
        kind: HitKind::Plane,
        point: o + d * t,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::MeshVertex;

    /// A 10x10 face at z=0 (id 0) with a boundary edge (id 1), a corner vertex (id 2),
    /// an edge hidden below the face (id 3) and a plane far below (id 4).
    fn scene() -> SceneGeometry {
        let mut s = SceneGeometry::default();
        let quad = [[-5.0, -5.0, 0.0], [5.0, -5.0, 0.0], [5.0, 5.0, 0.0], [-5.0, 5.0, 0.0]];
        for i in [0, 1, 2, 0, 2, 3] {
            s.triangles.push(MeshVertex {
                pos: quad[i],
                nor: [0.0, 0.0, 1.0],
                id: 0,
            });
        }
        s.line(quad[1], quad[2], [0.0; 3], 1);
        s.points.push((quad[2], 2));
        s.line([-2.0, 0.0, -3.0], [2.0, 0.0, -3.0], [0.0; 3], 3);
        s.add_plane(
            [
                [-50.0, -50.0, -20.0],
                [50.0, -50.0, -20.0],
                [50.0, 50.0, -20.0],
                [-50.0, 50.0, -20.0],
            ],
            [0.0; 3],
            4,
        );
        s
    }

    fn top_camera() -> OrbitCamera {
        let mut cam = OrbitCamera::default();
        cam.distance = 40.0;
        cam.normal_to(Vec3::Z, Vec3::X);
        while cam.animate(0.1) {}
        cam
    }

    #[test]
    fn picks_in_priority_order_and_respects_occlusion() {
        let (s, cam) = (scene(), top_camera());
        let vp = Vec2::new(800.0, 600.0);
        let at = |p: [f32; 3]| cam.project(p.into(), vp).unwrap();
        let hit = |px: Vec2| pick(&s, &cam, vp, px, 6.0).map(|h| (h.id, h.kind));
        assert_eq!(
            hit(at([0.0, 0.0, 0.0]) + Vec2::new(30.0, 0.0)),
            Some((0, HitKind::Face))
        );
        assert_eq!(hit(at([5.0, 0.0, 0.0]) + Vec2::new(2.0, 0.0)), Some((1, HitKind::Edge)));
        assert_eq!(
            hit(at([5.0, 5.0, 0.0]) + Vec2::new(1.0, 1.0)),
            Some((2, HitKind::Vertex))
        );
        // The hidden edge sits right under the face center: the face wins.
        assert_eq!(hit(at([0.0, 0.0, 0.0])), Some((0, HitKind::Face)));
        // Off the model, the reference plane is picked.
        assert_eq!(hit(at([20.0, 20.0, -20.0])), Some((4, HitKind::Plane)));
    }
}
