//! B-Rep modelling bridge over the pure-Rust `truck` kernel.

pub mod boolean;
pub mod edge_ops;
pub mod mass;
pub mod mesher;
pub mod offset;
pub mod operations;
pub mod solid;
pub mod tool;
pub mod topology;

pub use solid::{Plane, Solid};
pub use tool::{Draft, Tool, ToolFace};
