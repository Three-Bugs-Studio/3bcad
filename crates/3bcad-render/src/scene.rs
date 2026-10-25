use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::{self, util::DeviceExt};
use glam::{Mat4, Vec3};

use crate::pipeline::Pipelines;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub nor: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

const GRID_MINOR: [f32; 3] = [0.70, 0.72, 0.76];
const GRID_MAJOR: [f32; 3] = [0.56, 0.59, 0.64];

/// CPU-side scene: shaded triangles plus coloured lines.
#[derive(Clone, Debug, Default)]
pub struct SceneGeometry {
    pub triangles: Vec<MeshVertex>,
    pub lines: Vec<LineVertex>,
}

impl SceneGeometry {
    pub fn line(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3]) {
        self.lines
            .extend([LineVertex { pos: a, color }, LineVertex { pos: b, color }]);
    }

    /// Ground grid on the XY plane (major line every 10 steps) and the X/Y/Z axes.
    pub fn add_grid(&mut self, half_extent: f32, step: f32) {
        let n = (half_extent / step) as i32;
        let h = half_extent;
        for i in (-n..=n).filter(|&i| i != 0) {
            let t = i as f32 * step;
            let c = if i % 10 == 0 { GRID_MAJOR } else { GRID_MINOR };
            self.line([t, -h, 0.0], [t, h, 0.0], c);
            self.line([-h, t, 0.0], [h, t, 0.0], c);
        }
        self.line([-h, 0.0, 0.0], [h, 0.0, 0.0], [0.85, 0.2, 0.2]);
        self.line([0.0, -h, 0.0], [0.0, h, 0.0], [0.2, 0.7, 0.25]);
        self.line([0.0, 0.0, 0.0], [0.0, 0.0, h * 0.5], [0.2, 0.35, 0.9]);
    }

    /// Bounds of the shaded geometry (grid excluded).
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut it = self.triangles.iter().map(|v| Vec3::from(v.pos));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
}

/// GPU state, stored in egui-wgpu's callback resources.
struct ViewportResources {
    pipelines: Pipelines,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    triangles: Option<(wgpu::Buffer, u32)>,
    lines: Option<(wgpu::Buffer, u32)>,
    version: u64,
}

/// Call once at startup. `depth_format` / `msaa` must match the eframe options.
pub fn install(render_state: &egui_wgpu::RenderState, depth_format: Option<wgpu::TextureFormat>, msaa: u32) {
    let device = &render_state.device;
    let pipelines = Pipelines::new(device, render_state.target_format, depth_format, msaa);
    let globals = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("3bcad globals"),
        size: std::mem::size_of::<Globals>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3bcad globals"),
        layout: &pipelines.bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: globals.as_entire_binding(),
        }],
    });
    render_state
        .renderer
        .write()
        .callback_resources
        .insert(ViewportResources {
            pipelines,
            globals,
            bind_group,
            triangles: None,
            lines: None,
            version: u64::MAX,
        });
}

/// One frame of the 3D viewport; geometry is re-uploaded only when `version` changes.
pub struct ViewportCallback {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub geometry: Arc<SceneGeometry>,
    pub version: u64,
}

fn upload<T: Pod>(device: &wgpu::Device, data: &[T]) -> Option<(wgpu::Buffer, u32)> {
    (!data.is_empty()).then(|| {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("3bcad vertices"),
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        (buffer, data.len() as u32)
    })
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<ViewportResources>() else {
            return Vec::new();
        };
        let globals = Globals {
            view_proj: self.view_proj.to_cols_array_2d(),
            eye: self.eye.extend(1.0).to_array(),
        };
        queue.write_buffer(&res.globals, 0, bytemuck::bytes_of(&globals));
        if res.version != self.version {
            res.triangles = upload(device, &self.geometry.triangles);
            res.lines = upload(device, &self.geometry.lines);
            res.version = self.version;
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(res) = resources.get::<ViewportResources>() else {
            return;
        };
        pass.set_bind_group(0, &res.bind_group, &[]);
        for (pipeline, buffer) in [
            (&res.pipelines.mesh, &res.triangles),
            (&res.pipelines.lines, &res.lines),
        ] {
            if let Some((buffer, count)) = buffer {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }
    }
}
