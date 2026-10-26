//! Sketch geometry. Coordinates live in the owning sketch's flat parameter
//! vector so the solver can move every degree of freedom at once.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Line {
        a: PointId,
        b: PointId,
    },
    /// `radius` is a parameter index, so the solver can drive it.
    Circle {
        center: PointId,
        radius: usize,
    },
    /// Counter-clockwise from `start` to `end`. The radius is `|start - center|`;
    /// an implicit relation keeps `end` on the same circle.
    Arc {
        center: PointId,
        start: PointId,
        end: PointId,
    },
    /// Smooth curve through `points` (Catmull-Rom, drawn and modelled as cubic Béziers).
    Spline {
        points: Vec<PointId>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub geom: Geometry,
    /// Construction geometry constrains the sketch but is never part of a profile.
    pub construction: bool,
}

impl Entity {
    pub fn new(geom: Geometry) -> Self {
        Self {
            geom,
            construction: false,
        }
    }

    /// Start and end of an open curve (`None` for circles).
    pub fn endpoints(&self) -> Option<(PointId, PointId)> {
        match &self.geom {
            Geometry::Line { a, b } => Some((*a, *b)),
            Geometry::Arc { start, end, .. } => Some((*start, *end)),
            Geometry::Spline { points } => Some((points[0], *points.last()?)),
            Geometry::Circle { .. } => None,
        }
    }

    /// Every point the entity is defined by (endpoints, centers, spline points).
    pub fn points(&self) -> Vec<PointId> {
        match &self.geom {
            Geometry::Line { a, b } => vec![*a, *b],
            Geometry::Circle { center, .. } => vec![*center],
            Geometry::Arc { center, start, end } => vec![*center, *start, *end],
            Geometry::Spline { points } => points.clone(),
        }
    }

    pub fn is_line(&self) -> bool {
        matches!(self.geom, Geometry::Line { .. })
    }

    /// Circles and arcs: anything with a center and a radius.
    pub fn is_round(&self) -> bool {
        matches!(self.geom, Geometry::Circle { .. } | Geometry::Arc { .. })
    }

    pub fn center(&self) -> Option<PointId> {
        match self.geom {
            Geometry::Circle { center, .. } | Geometry::Arc { center, .. } => Some(center),
            _ => None,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self.geom {
            Geometry::Line { .. } if self.construction => "Centerline",
            Geometry::Line { .. } => "Line",
            Geometry::Circle { .. } => "Circle",
            Geometry::Arc { .. } => "Arc",
            Geometry::Spline { .. } => "Spline",
        }
    }
}
