use threebcad_sketch::Sketch;

use crate::FeatureId;

#[derive(Clone, Debug, PartialEq)]
pub struct Feature {
    pub name: String,
    pub kind: FeatureKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FeatureKind {
    // shortcut: sketches live on the XY plane only; add a plane/face reference when sketch-on-face lands.
    Sketch(Sketch),
    ExtrudeBoss { sketch: FeatureId, depth: f64 },
}
