//! B-Rep body types. truck's `Solid` already carries the full topology
//! (shells -> faces -> wires -> edges -> vertices) with NURBS geometry, so we
//! re-export it instead of wrapping it.

pub use truck_modeling::{Point3, Solid, Vector3};

/// A sketch plane: origin plus orthonormal in-plane axes; the normal is `x × y`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
}

impl Plane {
    /// XY, normal +Z (the build plate for 3D printing).
    pub const TOP: Plane = Plane {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
    };
    /// XZ, normal -Y (faces a viewer standing in front, at -Y).
    pub const FRONT: Plane = Plane {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 0.0, 1.0],
    };
    /// YZ, normal +X.
    pub const RIGHT: Plane = Plane {
        origin: [0.0; 3],
        x: [0.0, 1.0, 0.0],
        y: [0.0, 0.0, 1.0],
    };

    pub fn normal(&self) -> [f64; 3] {
        cross(self.x, self.y)
    }

    pub fn to_world(&self, p: [f64; 2]) -> [f64; 3] {
        std::array::from_fn(|i| self.origin[i] + self.x[i] * p[0] + self.y[i] * p[1])
    }

    /// Projects a world point into plane coordinates.
    pub fn to_local(&self, p: [f64; 3]) -> [f64; 2] {
        let d = sub(p, self.origin);
        [dot(d, self.x), dot(d, self.y)]
    }

    /// A plane through `origin` facing `normal`; the x axis follows world X (or Y when X is
    /// nearly normal to the plane) so sketches on faces get a predictable orientation.
    pub fn from_origin_normal(origin: [f64; 3], normal: [f64; 3]) -> Plane {
        let n = normalize(normal);
        let project = |a: [f64; 3]| sub(a, scale(n, dot(a, n)));
        let mut x = project([1.0, 0.0, 0.0]);
        if dot(x, x) < 0.01 {
            x = project([0.0, 1.0, 0.0]);
        }
        let x = normalize(x);
        Plane {
            origin,
            x,
            y: cross(n, x),
        }
    }

    /// Column-major 4x4 matrix taking plane coordinates (x, y, along-normal) to world.
    pub fn matrix(&self) -> [[f64; 4]; 4] {
        let n = self.normal();
        let o = self.origin;
        [
            [self.x[0], self.x[1], self.x[2], 0.0],
            [self.y[0], self.y[1], self.y[2], 0.0],
            [n[0], n[1], n[2], 0.0],
            [o[0], o[1], o[2], 1.0],
        ]
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|v| v * s)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: [f64; 3]) -> [f64; 3] {
    scale(a, 1.0 / dot(a, a).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-12)
    }

    #[test]
    fn standard_planes_face_the_right_way() {
        assert!(near(Plane::TOP.normal(), [0.0, 0.0, 1.0]));
        assert!(near(Plane::FRONT.normal(), [0.0, -1.0, 0.0]));
        assert!(near(Plane::RIGHT.normal(), [1.0, 0.0, 0.0]));
    }

    #[test]
    fn local_world_round_trip_and_face_planes() {
        let p = Plane::from_origin_normal([1.0, 2.0, 3.0], [0.0, 0.0, -2.0]);
        assert!(near(p.normal(), [0.0, 0.0, -1.0]));
        assert!(near(p.x, [1.0, 0.0, 0.0]));
        let w = p.to_world([4.0, -5.0]);
        let l = p.to_local(w);
        assert!((l[0] - 4.0).abs() < 1e-12 && (l[1] + 5.0).abs() < 1e-12);
        // A face whose normal is world X falls back to world Y for its x axis.
        let side = Plane::from_origin_normal([0.0; 3], [1.0, 0.0, 0.0]);
        assert!(near(side.x, [0.0, 1.0, 0.0]) && near(side.normal(), [1.0, 0.0, 0.0]));
    }
}
