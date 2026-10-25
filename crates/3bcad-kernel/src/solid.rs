//! B-Rep body types. truck's `Solid` already carries the full topology
//! (shells -> faces -> wires -> edges -> vertices) with NURBS geometry, so we
//! re-export it instead of wrapping it.

pub use truck_modeling::{Point3, Solid, Vector3};
