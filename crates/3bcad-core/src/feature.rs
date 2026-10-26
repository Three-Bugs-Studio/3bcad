use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

use crate::FeatureId;

#[derive(Clone, Debug, PartialEq)]
pub struct Feature {
    pub name: String,
    pub kind: FeatureKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FeatureKind {
    /// A sketch on a reference plane or on a planar face.
    // shortcut: a sketch on a face stores the face's plane when it was created; follow the face once faces have persistent ids (#42).
    Sketch {
        sketch: Sketch,
        plane: Plane,
    },
    ExtrudeBoss {
        sketch: FeatureId,
        depth: f64,
    },
}
