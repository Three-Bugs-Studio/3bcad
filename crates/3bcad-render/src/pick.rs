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

fn nearest_triangle<'a>(
    o: Vec3,
    d: Vec3,
    tris: impl Iterator<Item = ([Vec3; 3], u32)> + 'a,
    kept: impl Fn(Vec3) -> bool,
) -> Option<(f32, u32)> {
    tris.filter_map(|(t, id)| ray_triangle(o, d, t).map(|dist| (dist, id)))
        .filter(|&(t, _)| kept(o + d * t))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// Triangles per bounding box: a ray skips most of a large mesh without testing its triangles.
const CHUNK: usize = 128;

/// Bounding boxes of each `CHUNK` triangles.
fn chunk_boxes(scene: &SceneGeometry) -> &[[Vec3; 2]] {
    scene.chunk_boxes.get_or_init(|| {
        scene
            .triangles
            .chunks(3 * CHUNK)
            .map(|c| {
                c.iter()
                    .fold([Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)], |[lo, hi], v| {
                        let p = Vec3::from(v.pos);
                        [lo.min(p), hi.max(p)]
                    })
            })
            .collect()
    })
}

/// Slab test: does the ray `o + t d` (t >= 0) meet the box?
fn ray_box(o: Vec3, d: Vec3, [lo, hi]: [Vec3; 2]) -> bool {
    let inv = Vec3::ONE / d.map(|c| if c.abs() < 1e-12 { 1e-12 } else { c });
    let (t1, t2) = ((lo - o) * inv, (hi - o) * inv);
    t1.min(t2).max_element().max(0.0) <= t1.max(t2).min_element()
}

/// What is under `cursor` (pixels from the viewport's top-left), in pick priority order:
/// vertices, then edges, then faces, then reference planes.
pub fn pick(scene: &SceneGeometry, cam: &OrbitCamera, viewport: Vec2, cursor: Vec2, tol_px: f32) -> Option<Hit> {
    pick_clipped(scene, cam, viewport, cursor, tol_px, None)
}

/// `pick`, ignoring the model where a section view (`normal · p > offset`) cuts it away.
pub fn pick_clipped(
    scene: &SceneGeometry,
    cam: &OrbitCamera,
    viewport: Vec2,
    cursor: Vec2,
    tol_px: f32,
    clip: Option<[f32; 4]>,
) -> Option<Hit> {
    let kept = |p: Vec3| clip.is_none_or(|[x, y, z, w]| Vec3::new(x, y, z).dot(p) <= w + 1e-4);
    let (o, d) = cam.ray(cursor, viewport);
    let boxes = chunk_boxes(scene);
    let face = nearest_triangle(
        o,
        d,
        scene
            .triangles
            .chunks(3 * CHUNK)
            .zip(boxes)
            .filter(|(_, b)| ray_box(o, d, **b))
            .flat_map(|(c, _)| c.chunks_exact(3))
            .map(|t| ([t[0].pos.into(), t[1].pos.into(), t[2].pos.into()], t[0].id)),
        kept,
    );
    let visible = |p: Vec3| kept(p) && face.is_none_or(|(t, _)| (p - o).dot(d) <= t * 1.002 + 1e-4);

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
        .chain(scene.edges.chunks_exact(2))
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
        |_| true,
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
                tint: [0; 4],
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

    #[test]
    fn a_section_view_picks_what_lies_behind_the_cut() {
        // Two parallel faces, at z = 0 (id 0) and z = -10 (id 5), seen from above.
        let mut s = scene();
        let quad = [
            [-5.0, -5.0, -10.0],
            [5.0, -5.0, -10.0],
            [5.0, 5.0, -10.0],
            [-5.0, 5.0, -10.0],
        ];
        for i in [0, 1, 2, 0, 2, 3] {
            s.triangles.push(MeshVertex {
                pos: quad[i],
                nor: [0.0, 0.0, 1.0],
                id: 5,
                tint: [0; 4],
            });
        }
        let cam = top_camera();
        let vp = Vec2::new(800.0, 600.0);
        let px = cam.project(Vec3::new(1.0, 1.0, 0.0), vp).unwrap();
        let hit = |clip| pick_clipped(&s, &cam, vp, px, 6.0, clip).map(|h| (h.id, h.kind));
        assert_eq!(hit(None), Some((0, HitKind::Face)));
        // Cut away everything above z = -5: the lower face is under the cursor now.
        let below = hit(Some([0.0, 0.0, 1.0, -5.0]));
        assert_eq!(below, Some((5, HitKind::Face)));
        let p = pick_clipped(&s, &cam, vp, px, 6.0, Some([0.0, 0.0, 1.0, -5.0]))
            .unwrap()
            .point;
        assert!((p.z + 10.0).abs() < 1e-3, "{p}");
    }

    #[test]
    fn large_meshes_are_picked_through_their_boxes() {
        // 100k small triangles in a row: the hit must be the one under the cursor.
        let mut s = SceneGeometry::default();
        for i in 0..100_000 {
            let x = i as f32 * 0.01;
            for p in [[x, 0.0, 0.0], [x + 0.01, 0.0, 0.0], [x, 0.01, 0.0]] {
                s.triangles.push(MeshVertex {
                    pos: p,
                    nor: [0.0, 0.0, 1.0],
                    id: i,
                    tint: [0; 4],
                });
            }
        }
        let mut cam = top_camera();
        cam.target = Vec3::new(500.0, 0.0, 0.0);
        cam.distance = 0.2;
        let vp = Vec2::new(800.0, 600.0);
        let px = cam.project(Vec3::new(500.003, 0.003, 0.0), vp).unwrap();
        let t = std::time::Instant::now();
        let hit = pick(&s, &cam, vp, px, 0.0).unwrap();
        assert_eq!(hit.id, 50_000);
        // The boxes are built on the first pick; the next ones only test nearby triangles.
        let first = t.elapsed();
        let t = std::time::Instant::now();
        assert_eq!(pick(&s, &cam, vp, px, 0.0).unwrap().id, 50_000);
        assert!(t.elapsed() <= first, "{:?} then {:?}", first, t.elapsed());
    }
}
