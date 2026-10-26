use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::{self, util::DeviceExt};
use glam::{Mat4, Vec3};

use crate::pipeline::Pipelines;

/// Id of geometry that cannot be picked or highlighted (grid, axes).
pub const NO_ID: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub nor: [f32; 3],
    pub id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub id: u32,
}

const GRID_MINOR: [f32; 3] = [0.70, 0.72, 0.76];
const GRID_MAJOR: [f32; 3] = [0.56, 0.59, 0.64];

/// CPU-side scene. Ids index the highlight state buffer and come back from picking.
#[derive(Clone, Debug, Default)]
pub struct SceneGeometry {
    pub triangles: Vec<MeshVertex>,
    pub lines: Vec<LineVertex>,
    /// Translucent triangles (reference planes).
    pub planes: Vec<LineVertex>,
    /// Pickable points (B-Rep vertices), drawn as overlay markers by the app.
    pub points: Vec<([f32; 3], u32)>,
}

impl SceneGeometry {
    pub fn line(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3], id: u32) {
        self.lines
            .extend([LineVertex { pos: a, color, id }, LineVertex { pos: b, color, id }]);
    }

    /// Ground grid on the XY plane (major line every 10 steps) and the X/Y/Z axes.
    pub fn add_grid(&mut self, half_extent: f32, step: f32) {
        let n = (half_extent / step) as i32;
        let h = half_extent;
        for i in (-n..=n).filter(|&i| i != 0) {
            let t = i as f32 * step;
            let c = if i % 10 == 0 { GRID_MAJOR } else { GRID_MINOR };
            self.line([t, -h, 0.0], [t, h, 0.0], c, NO_ID);
            self.line([-h, t, 0.0], [h, t, 0.0], c, NO_ID);
        }
        self.line([-h, 0.0, 0.0], [h, 0.0, 0.0], [0.85, 0.2, 0.2], NO_ID);
        self.line([0.0, -h, 0.0], [0.0, h, 0.0], [0.2, 0.7, 0.25], NO_ID);
        self.line([0.0, 0.0, 0.0], [0.0, 0.0, h * 0.5], [0.2, 0.35, 0.9], NO_ID);
    }

    /// A translucent quad with an outline, for a reference plane.
    pub fn add_plane(&mut self, corners: [[f32; 3]; 4], color: [f32; 3], id: u32) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.planes.push(LineVertex {
                pos: corners[i],
                color,
                id,
            });
        }
        for i in 0..4 {
            self.line(corners[i], corners[(i + 1) % 4], color, id);
        }
    }

    /// Bounds of the shaded geometry (grid and planes excluded).
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
    states: wgpu::Buffer,
    states_len: usize,
    bind_group: wgpu::BindGroup,
    triangles: Option<(wgpu::Buffer, u32)>,
    lines: Option<(wgpu::Buffer, u32)>,
    planes: Option<(wgpu::Buffer, u32)>,
    version: u64,
    states_version: u64,
}

fn states_buffer(device: &wgpu::Device, len: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("3bcad pick states"),
        size: (len.max(1) * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bind_group(
    device: &wgpu::Device,
    pipelines: &Pipelines,
    globals: &wgpu::Buffer,
    states: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3bcad globals"),
        layout: &pipelines.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: states.as_entire_binding(),
            },
        ],
    })
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
    let states = states_buffer(device, 1);
    let bind_group = bind_group(device, &pipelines, &globals, &states);
    render_state
        .renderer
        .write()
        .callback_resources
        .insert(ViewportResources {
            pipelines,
            globals,
            states,
            states_len: 1,
            bind_group,
            triangles: None,
            lines: None,
            planes: None,
            version: u64::MAX,
            states_version: u64::MAX,
        });
}

/// One frame of the 3D viewport. Geometry is re-uploaded only when `version` changes and the
/// highlight states only when `states_version` changes.
pub struct ViewportCallback {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub geometry: Arc<SceneGeometry>,
    pub version: u64,
    /// Per pick id: 0 = normal, 1 = hovered, 2 = selected.
    pub states: Arc<Vec<u32>>,
    pub states_version: u64,
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
            res.planes = upload(device, &self.geometry.planes);
            res.version = self.version;
        }
        if res.states_version != self.states_version {
            if self.states.len() > res.states_len {
                res.states_len = self.states.len().next_power_of_two();
                res.states = states_buffer(device, res.states_len);
                res.bind_group = bind_group(device, &res.pipelines, &res.globals, &res.states);
            }
            let mut data = self.states.to_vec();
            data.resize(res.states_len.max(1), 0);
            queue.write_buffer(&res.states, 0, bytemuck::cast_slice(&data));
            res.states_version = self.states_version;
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
        // Planes last: they are translucent and do not write depth.
        for (pipeline, buffer) in [
            (&res.pipelines.mesh, &res.triangles),
            (&res.pipelines.lines, &res.lines),
            (&res.pipelines.planes, &res.planes),
        ] {
            if let Some((buffer, count)) = buffer {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }
    }
}
