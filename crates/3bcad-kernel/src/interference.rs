//! Interference between closed triangle meshes: the volume two parts share. Faces that only touch
//! (a part resting on another, a pin in a hole of its own size) do not interfere.
//!
//! The shared volume is bounded by the pieces of each surface that lie inside the other part, so
//! it is the divergence-theorem sum over those pieces. truck's booleans cannot be used: they fail
//! on exactly the coincident faces mated parts have.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::print::{Bvh, Tri};
use crate::topology::{cross, dot, norm, sub, unit, P3};

/// How far (mm) a surface must reach into another part to interfere; closer, the two touch. It
/// is wider than the meshing tolerance, so a pin's facets never seem to cut into its own hole.
pub const TOUCH: f64 = 0.01;
/// Shared volume (mm³) below which two parts are taken to touch.
pub const MIN_VOLUME: f64 = 1e-3;
/// Triangles that cross the other part's surface are split down to this size (mm).
const FINE: f64 = 0.01;
const MAX_DEPTH: u32 = 40;
/// Ray directions for inside tests, none along an axis, tried in turn when one grazes an edge.
const RAYS: [P3; 3] = [
    [0.577_215_66, 0.612_345_1, 0.540_302_3],
    [-0.381_966, 0.692_307_7, 0.595_241_1],
    [0.267_949_2, -0.414_213_6, 0.869_839_3],
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interference {
    pub volume: f64,
    /// Triangles of the first and of the second mesh with a piece inside the other part.
    pub inside_a: Vec<usize>,
    pub inside_b: Vec<usize>,
}

struct Solid<'a> {
    tris: &'a [Tri],
    bvh: Bvh,
}

impl Solid<'_> {
    fn contains(&self, p: P3) -> bool {
        RAYS.iter()
            .find_map(|&d| self.bvh.crossings(self.tris, p, unit(d)))
            .is_some_and(|n| n % 2 == 1)
    }
}

fn bounds(tris: &[Tri]) -> (P3, P3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in tris.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn mid(a: P3, b: P3) -> P3 {
    std::array::from_fn(|i| (a[i] + b[i]) / 2.0)
}

/// The volume parts `a` and `b` (closed, outward-facing meshes in the same coordinates) share,
/// or `None` when they only touch or are apart.
// shortcut: pieces of a triangle are classified by a sample point, so the volume is within a few
// percent where the parts' surfaces cross; clip triangles exactly if a precise figure is needed.
pub fn interference(a: &[Tri], b: &[Tri]) -> Option<Interference> {
    let ((alo, ahi), (blo, bhi)) = (bounds(a), bounds(b));
    let lo: P3 = std::array::from_fn(|k| alo[k].max(blo[k]) - TOUCH);
    let hi: P3 = std::array::from_fn(|k| ahi[k].min(bhi[k]) + TOUCH);
    if (0..3).any(|k| lo[k] >= hi[k]) {
        return None;
    }
    let (sa, sb) = (
        Solid {
            tris: a,
            bvh: Bvh::new(a),
        },
        Solid {
            tris: b,
            bvh: Bvh::new(b),
        },
    );
    let o = mid(lo, hi);
    // Long triangles are cut to a quarter of the overlap first, so a thin part piercing one between
    // its corners is still seen.
    let coarse = (0..3)
        .map(|k| hi[k] - lo[k])
        .fold(f64::INFINITY, f64::min)
        .max(4.0 * FINE)
        / 4.0;
    let mut out = Interference::default();
    let mut volume = 0.0;
    for (own, other, first) in [(a, &sb, true), (b, &sa, false)] {
        for (i, t) in own.iter().enumerate() {
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            if norm(n) == 0.0 {
                continue;
            }
            let walk = Walk {
                other,
                n: unit(n),
                first,
                lo,
                hi,
                coarse,
                o,
                seen: RefCell::new(HashMap::new()),
            };
            let v = walk.part(*t, 0);
            if v != 0.0 {
                volume += v;
                if first { &mut out.inside_a } else { &mut out.inside_b }.push(i);
            }
        }
    }
    out.volume = volume.abs();
    (out.volume > MIN_VOLUME).then_some(out)
}

struct Walk<'a, 'b> {
    other: &'a Solid<'b>,
    /// The triangle's outward normal.
    n: P3,
    /// Faces of the two parts lying on each other and facing the same way bound the shared
    /// volume once: they count for the first part only.
    first: bool,
    lo: P3,
    hi: P3,
    coarse: f64,
    /// Volumes are taken from here, the middle of the overlap.
    o: P3,
    /// Corners are shared by the pieces around them: each is tested once.
    seen: RefCell<HashMap<[u64; 3], bool>>,
}

impl Walk<'_, '_> {
    /// Whether the surface at `p` bounds the shared volume.
    fn counts(&self, p: P3) -> bool {
        let key = p.map(f64::to_bits);
        if let Some(&c) = self.seen.borrow().get(&key) {
            return c;
        }
        let at = |s: f64| std::array::from_fn(|k| p[k] + s * TOUCH * self.n[k]);
        // Just inside this part, and inside the other one; just outside this part is inside the
        // other too unless the faces lie on each other.
        let c = self.other.contains(at(-1.0)) && (self.first || self.other.contains(at(1.0)));
        self.seen.borrow_mut().insert(key, c);
        c
    }

    /// The signed volume `t` (or its pieces inside the other part) contributes.
    fn part(&self, t: Tri, depth: u32) -> f64 {
        let (tlo, thi) = bounds(&[t]);
        if (0..3).any(|k| thi[k] < self.lo[k] || tlo[k] > self.hi[k]) {
            return 0.0;
        }
        let (k, size) = (0..3)
            .map(|k| (k, norm(sub(t[k], t[(k + 1) % 3]))))
            .fold((0, 0.0), |best, e| if e.1 > best.1 { e } else { best });
        let c: P3 = std::array::from_fn(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0);
        let mut here = None;
        let split = depth < MAX_DEPTH
            && (size > self.coarse
                || size > FINE && {
                    let h = self.counts(c);
                    here = Some(h);
                    t.iter().any(|&p| self.counts(p) != h)
                });
        if split {
            // Halved across its longest edge, a sliver is cut along its length instead of into
            // ever more slivers.
            let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            let m = mid(a, b);
            return self.part([a, m, c], depth + 1) + self.part([m, b, c], depth + 1);
        }
        if !here.unwrap_or_else(|| self.counts(c)) {
            return 0.0;
        }
        let [a, b, c] = t.map(|p| sub(p, self.o));
        dot(a, cross(b, c)) / 6.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesher::print_mesh;
    use crate::operations::extrude_polygon;

    /// A box from `lo` to `hi`, meshed.
    fn block(lo: P3, hi: P3) -> Vec<Tri> {
        let s = extrude_polygon(
            &[
                [0.0, 0.0],
                [hi[0] - lo[0], 0.0],
                [hi[0] - lo[0], hi[1] - lo[1]],
                [0.0, hi[1] - lo[1]],
            ],
            hi[2] - lo[2],
        )
        .unwrap();
        let m = print_mesh(&s, 0.01);
        m.triangles
            .iter()
            .map(|t| t.map(|i| std::array::from_fn(|k| m.vertices[i][k] + lo[k])))
            .collect()
    }

    #[test]
    fn overlapping_blocks_share_their_overlap() {
        let a = block([0.0; 3], [10.0, 10.0, 10.0]);
        // 4 mm into the first, flush with it on four sides.
        let b = block([6.0, 0.0, 0.0], [16.0, 10.0, 10.0]);
        let i = interference(&a, &b).expect("they interfere");
        assert!((i.volume - 400.0).abs() < 0.5, "{}", i.volume);
        assert!(!i.inside_a.is_empty() && !i.inside_b.is_empty());
        // Corner to corner: a 2 x 3 x 4 overlap.
        let c = block([8.0, 7.0, 6.0], [20.0, 20.0, 20.0]);
        let v = interference(&a, &c).unwrap().volume;
        assert!((v - 24.0).abs() < 0.05, "{v}");
        assert_eq!(interference(&c, &a).map(|i| (i.volume - v).abs() < 1e-9), Some(true));
    }

    #[test]
    fn touching_or_separate_blocks_do_not_interfere() {
        let a = block([0.0; 3], [10.0, 10.0, 10.0]);
        assert_eq!(
            interference(&a, &block([10.0, 0.0, 0.0], [20.0, 10.0, 10.0])),
            None,
            "side by side"
        );
        assert_eq!(
            interference(&a, &block([2.0, 2.0, 10.0], [8.0, 8.0, 15.0])),
            None,
            "stacked"
        );
        assert_eq!(
            interference(&a, &block([30.0, 0.0, 0.0], [40.0, 10.0, 10.0])),
            None,
            "apart"
        );
    }

    #[test]
    fn a_pin_through_a_plate_shares_the_plate_s_thickness() {
        // A 64-sided pin, 40 long, through the middle of a 2 mm plate: its long side facets cross
        // the plate with every corner outside it.
        let ring: Vec<[f64; 2]> = (0..64)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / 64.0;
                [3.0 * a.cos(), 3.0 * a.sin()]
            })
            .collect();
        let area = 0.5 * 64.0 * 9.0 * (std::f64::consts::TAU / 64.0).sin();
        let m = print_mesh(&extrude_polygon(&ring, 40.0).unwrap(), 0.01);
        let pin: Vec<Tri> = m
            .triangles
            .iter()
            .map(|t| t.map(|i| std::array::from_fn(|k| m.vertices[i][k] + [10.0, 10.0, -20.0][k])))
            .collect();
        let plate = block([0.0, 0.0, -1.0], [20.0, 20.0, 1.0]);
        let v = interference(&pin, &plate).unwrap().volume;
        assert!((v - 2.0 * area).abs() / (2.0 * area) < 0.01, "{v} vs {}", 2.0 * area);
    }
}
