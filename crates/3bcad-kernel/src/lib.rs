//! B-Rep modelling bridge over the pure-Rust `truck` kernel.

pub mod boolean;
pub mod dome;
pub mod edge_ops;
pub mod loft;
pub mod mass;
pub mod mesher;
pub mod offset;
pub mod operations;
pub mod round;
pub mod skin;
pub mod solid;
pub mod sweep;
pub mod thread;
pub mod tool;
pub mod topology;
pub mod wrap;

pub use solid::{Plane, Solid};
pub use tool::{Draft, Tool, ToolFace};
