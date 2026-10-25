//! Parametric feature history (DAG + rollback), part documents, undo/redo.

pub mod document;
pub mod feature;
pub mod history;

pub use document::PartDocument;
pub use feature::{Feature, FeatureKind};
pub use history::{FeatureId, History, Rebuild};
