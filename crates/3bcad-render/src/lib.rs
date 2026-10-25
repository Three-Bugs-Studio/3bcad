//! wgpu 3D viewport that paints inside egui via a paint callback.

pub mod camera;
pub mod pipeline;
pub mod scene;

pub use camera::OrbitCamera;
pub use scene::{install, LineVertex, MeshVertex, SceneGeometry, ViewportCallback};
