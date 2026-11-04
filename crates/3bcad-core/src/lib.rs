//! Parametric feature history (DAG + rollback), part documents, undo/redo.

pub mod document;
pub mod feature;
pub mod history;
pub mod holes;
pub mod material;
pub mod naming;
pub mod rebuild;
pub mod units;

pub use document::PartDocument;
pub use feature::*;
pub use history::{FeatureId, History};
pub use material::Material;
pub use naming::{EdgeRef, FaceRef, FaceTag, VertexRef};
pub use rebuild::{Body, Rebuild};
pub use units::{UnitSystem, Units};
