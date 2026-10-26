//! wgpu 3D viewport that paints inside egui via a paint callback, plus CPU picking.

pub mod camera;
pub mod pick;
pub mod pipeline;
pub mod scene;

pub use camera::OrbitCamera;
pub use pick::{pick, Hit, HitKind};
pub use scene::{install, LineVertex, MeshVertex, SceneGeometry, ViewportCallback, NO_ID};
