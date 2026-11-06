//! Printability checks on triangle meshes: faces that need support, walls too thin to print, and
//! the rotation that lays a part flat on one of its faces.

use crate::topology::{cross, dot, norm, sub, unit, P3};

/// A triangle, counter-clockwise seen from outside the part.
pub type Tri = [P3; 3];

/// A row-major 3x3 rotation.
pub type Rot = [[f64; 3]; 3];

pub const IDENTITY: Rot = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// How close to the lowest point (mm) a triangle must be to count as lying on the bed: well inside
/// a first layer, and past the micron a boolean may leave between bodies' bottoms.
const BED_TOL: f64 = 0.05;
/// Triangles per BVH leaf.
const LEAF: usize = 4;

fn normal(t: &Tri) -> P3 {
    unit(cross(sub(t[1], t[0]), sub(t[2], t[0])))
}

fn centroid(t: &Tri) -> P3 {
    std::array::from_fn(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.0)
}

pub fn area(t: &Tri) -> f64 {
    0.5 * norm(cross(sub(t[1], t[0]), sub(t[2], t[0])))
}

/// Triangles that need support when printed with `up` as the build direction: those facing down
/// more than `max_angle` (radians, measured from vertical). Faces resting on the bed do not.
pub fn overhangs(tris: &[Tri], up: P3, max_angle: f64) -> Vec<bool> {
    let up = unit(up);
    let bed = tris.iter().flatten().map(|&p| dot(p, up)).fold(f64::INFINITY, f64::min);
    // A face tilted `a` from vertical points down by sin(a).
    let limit = max_angle.sin();
    tris.iter()
        .map(|t| -dot(normal(t), up) > limit && t.iter().any(|&p| dot(p, up) - bed > BED_TOL))
        .collect()
}

/// Wall thickness behind each triangle: the distance from its centroid straight into the part to
/// the far side, or `None` when that is `max` or more.
pub fn wall_thickness(tris: &[Tri], max: f64) -> Vec<Option<f64>> {
    let normals: Vec<P3> = tris.iter().map(normal).collect();
    let bvh = Bvh::new(tris);
    (0..tris.len())
        .map(|i| {
            let into = normals[i].map(|v| -v);
            if into == [0.0; 3] {
                return None;
            }
            bvh.far_side(tris, &normals, centroid(&tris[i]), into, max, i)
        })
        .collect()
}

/// The rotation that turns `down` to point straight down (−Z), so the face whose outward normal it
/// is rests on the bed.
pub fn lay_flat(down: P3) -> Rot {
    let n = unit(down);
    let z = [0.0, 0.0, -1.0];
    let c = dot(n, z);
    let v = cross(n, z);
    let s2 = dot(v, v);
    if s2 < 1e-24 {
        // Already down (or no direction at all), or straight up: turn it over about X.
        return if c > -0.5 {
            IDENTITY
        } else {
            [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]]
        };
    }
    // Rodrigues: I + [v]x + [v]x² (1 − c) / s².
    let k = (1.0 - c) / s2;
    let vx = [[0.0, -v[2], v[1]], [v[2], 0.0, -v[0]], [-v[1], v[0], 0.0]];
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            let vx2: f64 = (0..3).map(|m| vx[i][m] * vx[m][j]).sum();
            f64::from(u8::from(i == j)) + vx[i][j] + k * vx2
        })
    })
}

pub fn rotate(r: &Rot, p: P3) -> P3 {
    [dot(r[0], p), dot(r[1], p), dot(r[2], p)]
}

/// Möller–Trumbore: distance along the ray to the triangle.
fn ray_triangle(o: P3, d: P3, [a, b, c]: &Tri) -> Option<f64> {
    let (e1, e2) = (sub(*b, *a), sub(*c, *a));
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-15 {
        return None;
    }
    let s = sub(o, *a);
    let u = dot(s, p) / det;
    let q = cross(s, e1);
    let v = dot(d, q) / det;
    let t = dot(e2, q) / det;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0).then_some(t)
}

fn ray_box(o: P3, d: P3, lo: P3, hi: P3, tmax: f64) -> bool {
    let (mut t0, mut t1) = (0.0_f64, tmax);
    for i in 0..3 {
        if d[i].abs() < 1e-300 {
            if o[i] < lo[i] || o[i] > hi[i] {
                return false;
            }
            continue;
        }
        let (a, b) = ((lo[i] - o[i]) / d[i], (hi[i] - o[i]) / d[i]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// A bounding volume tree over triangles, for ray queries.
pub(crate) struct Bvh {
    nodes: Vec<Node>,
    /// Triangle indices, grouped by leaf.
    order: Vec<usize>,
}

struct Node {
    lo: P3,
    hi: P3,
    /// A leaf holds `order[first..first + count]`; an inner node (count 0) has its left child next
    /// and its right child at `first`.
    first: usize,
    count: usize,
}

impl Bvh {
    pub(crate) fn new(tris: &[Tri]) -> Self {
        let mut bvh = Bvh {
            nodes: Vec::new(),
            order: (0..tris.len()).collect(),
        };
        if !tris.is_empty() {
            bvh.build(tris, 0, tris.len());
        }
        bvh
    }

    fn build(&mut self, tris: &[Tri], start: usize, end: usize) -> usize {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in self.order[start..end].iter().flat_map(|&i| tris[i]) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k] - 1e-9);
                hi[k] = hi[k].max(p[k] + 1e-9);
            }
        }
        let me = self.nodes.len();
        self.nodes.push(Node {
            lo,
            hi,
            first: start,
            count: end - start,
        });
        if end - start <= LEAF {
            return me;
        }
        let axis = (0..3)
            .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
            .unwrap_or(0);
        let mid = (start + end) / 2;
        self.order[start..end].select_nth_unstable_by(mid - start, |&a, &b| {
            centroid(&tris[a])[axis].total_cmp(&centroid(&tris[b])[axis])
        });
        self.build(tris, start, mid);
        let right = self.build(tris, mid, end);
        self.nodes[me] = Node {
            first: right,
            count: 0,
            ..self.nodes[me]
        };
        me
    }

    /// The nearest triangle (other than `skip`) the ray leaves the part through, closer than `max`.
    fn far_side(&self, tris: &[Tri], normals: &[P3], o: P3, d: P3, max: f64, skip: usize) -> Option<f64> {
        let mut best = max;
        let mut stack = if self.nodes.is_empty() { vec![] } else { vec![0] };
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k];
            if !ray_box(o, d, n.lo, n.hi, best) {
                continue;
            }
            if n.count == 0 {
                stack.extend([k + 1, n.first]);
                continue;
            }
            for &i in &self.order[n.first..n.first + n.count] {
                // A face turned towards the ray is where it would enter the part, not leave it.
                if i == skip || dot(normals[i], d) <= 0.0 {
                    continue;
                }
                if let Some(t) = ray_triangle(o, d, &tris[i]).filter(|&t| t > 1e-9 && t < best) {
                    best = t;
                }
            }
        }
        (best < max).then_some(best)
    }

    /// How many triangles the ray from `o` along `d` passes through, or `None` when it grazes an
    /// edge or runs along a face (a crossing that cannot be counted reliably: try another ray).
    pub(crate) fn crossings(&self, tris: &[Tri], o: P3, d: P3) -> Option<usize> {
        let mut count = 0;
        let mut stack = if self.nodes.is_empty() { vec![] } else { vec![0] };
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k];
            if !ray_box(o, d, n.lo, n.hi, f64::INFINITY) {
                continue;
            }
            if n.count == 0 {
                stack.extend([k + 1, n.first]);
                continue;
            }
            for &i in &self.order[n.first..n.first + n.count] {
                let [a, b, c] = tris[i];
                let (e1, e2) = (sub(b, a), sub(c, a));
                let p = cross(d, e2);
                let det = dot(e1, p);
                let scale = norm(e1) * norm(e2);
                if det.abs() <= 1e-12 * scale {
                    // In the triangle's plane: harmless unless it actually runs across it.
                    if dot(cross(e1, e2), sub(o, a)).abs() <= 1e-12 * scale {
                        return None;
                    }
                    continue;
                }
                let s = sub(o, a);
                let u = dot(s, p) / det;
                let q = cross(s, e1);
                let v = dot(d, q) / det;
                let t = dot(e2, q) / det;
                if t <= 0.0 || u < -1e-9 || v < -1e-9 || u + v > 1.0 + 1e-9 {
                    continue;
                }
                if u < 1e-9 || v < 1e-9 || u + v > 1.0 - 1e-9 {
                    return None;
                }
                count += 1;
            }
        }
        Some(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesher::{print_mesh, PrintMesh};
    use crate::operations::extrude_polygon;

    fn tris(m: &PrintMesh) -> Vec<Tri> {
        m.triangles.iter().map(|t| t.map(|i| m.vertices[i])).collect()
    }

    fn volume(t: &[Tri]) -> f64 {
        t.iter().map(|[a, b, c]| dot(*a, cross(*b, *c)) / 6.0).sum()
    }

    /// A T standing on its stem when printed along +Y: stem x 4..6, bar x 0..10 at y 10..12, 5 deep.
    fn tee() -> Vec<Tri> {
        let pts = [
            [4.0, 0.0],
            [6.0, 0.0],
            [6.0, 10.0],
            [10.0, 10.0],
            [10.0, 12.0],
            [0.0, 12.0],
            [0.0, 10.0],
            [4.0, 10.0],
        ];
        tris(&print_mesh(&extrude_polygon(&pts, 5.0).unwrap(), 0.01))
    }

    #[test]
    fn the_arms_of_a_tee_overhang_but_its_foot_does_not() {
        let t = tee();
        let flagged = |up: P3, max: f64| -> f64 {
            t.iter()
                .zip(overhangs(&t, up, max))
                .filter(|(_, o)| *o)
                .map(|(t, _)| area(t))
                .sum()
        };
        // The undersides of the two 4 x 5 arms; the 2 x 5 foot rests on the bed.
        assert!((flagged([0.0, 1.0, 0.0], 45f64.to_radians()) - 40.0).abs() < 1e-6);
        // A printer that bridges anything needs no support; standing on its back, nothing hangs.
        assert_eq!(flagged([0.0, 1.0, 0.0], 90f64.to_radians()), 0.0);
        assert_eq!(flagged([0.0, 0.0, 1.0], 45f64.to_radians()), 0.0);
    }

    #[test]
    fn thin_walls_are_measured_and_thick_ones_are_not() {
        // A 10 x 10 block with a 20 mm long, 0.6 mm thick fin along +X, 10 tall.
        let pts = [
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 0.6],
            [10.0, 0.6],
            [10.0, 10.0],
            [0.0, 10.0],
        ];
        let t = tris(&print_mesh(&extrude_polygon(&pts, 10.0).unwrap(), 0.01));
        let thick = wall_thickness(&t, 2.0);
        let fin_face = |t: &Tri| t.iter().all(|p| (p[1] - 0.6).abs() < 1e-9);
        let fin: Vec<_> = t.iter().zip(&thick).filter(|(t, _)| fin_face(t)).collect();
        assert!(!fin.is_empty());
        for (_, w) in fin {
            assert!(w.is_some_and(|w| (w - 0.6).abs() < 1e-9), "{w:?}");
        }
        // The top is 10 mm above the bottom: not thin.
        for (t, w) in t.iter().zip(&thick) {
            if t.iter().all(|p| (p[2] - 10.0).abs() < 1e-9) {
                assert_eq!(*w, None);
            }
        }
        assert!(wall_thickness(&[], 1.0).is_empty());
    }

    #[test]
    fn laying_a_box_on_its_side_puts_that_face_on_the_bed() {
        let t = tris(&print_mesh(
            &extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]], 30.0).unwrap(),
            0.01,
        ));
        for down in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 1.0, 1.0],
        ] {
            let r = lay_flat(down);
            assert!(norm(sub(rotate(&r, unit(down)), [0.0, 0.0, -1.0])) < 1e-12, "{down:?}");
            let turned: Vec<Tri> = t.iter().map(|t| t.map(|p| rotate(&r, p))).collect();
            assert!((volume(&turned) - 6000.0).abs() < 1e-6, "a rotation keeps the volume");
            // Only the face on the bed (or, on a corner, faces 35° from vertical) look down.
            let hangs = overhangs(&turned, [0.0, 0.0, 1.0], 45f64.to_radians());
            assert!(hangs.iter().all(|h| !h), "{down:?}");
        }
        assert_eq!(lay_flat([0.0; 3]), IDENTITY);
    }
}
