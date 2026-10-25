//! Sketch geometry. Coordinates live in the owning sketch's flat parameter
//! vector so the solver can move every degree of freedom at once.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Line {
        a: PointId,
        b: PointId,
    },
    /// `radius` is a parameter index, so the solver can drive it.
    Circle {
        center: PointId,
        radius: usize,
    },
}
