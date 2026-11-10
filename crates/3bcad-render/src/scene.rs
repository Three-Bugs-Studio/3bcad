use std::collections::HashMap;
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
    /// An analysis colour over the body colour (sRGB), alpha saying how much of it: 0 for none.
    pub tint: [u8; 4],
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
/// Silhouettes are drawn like model edges.
pub const EDGE_COLOR: [f32; 3] = [0.08, 0.08, 0.1];

/// How the model is drawn (the View toolbar's display style).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayStyle {
    #[default]
    ShadedWithEdges,
    Shaded,
    HiddenLinesRemoved,
    HiddenLinesVisible,
    Wireframe,
}

impl DisplayStyle {
    pub const ALL: [Self; 5] = [
        Self::ShadedWithEdges,
        Self::Shaded,
        Self::HiddenLinesRemoved,
        Self::HiddenLinesVisible,
        Self::Wireframe,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ShadedWithEdges => "Shaded With Edges",
            Self::Shaded => "Shaded",
            Self::HiddenLinesRemoved => "Hidden Lines Removed",
            Self::HiddenLinesVisible => "Hidden Lines Visible",
            Self::Wireframe => "Wireframe",
        }
    }

    /// Settings-file name.
    pub fn id(self) -> &'static str {
        match self {
            Self::ShadedWithEdges => "shaded_edges",
            Self::Shaded => "shaded",
            Self::HiddenLinesRemoved => "hidden_removed",
            Self::HiddenLinesVisible => "hidden_visible",
            Self::Wireframe => "wireframe",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id.trim())
    }

    /// The next style (the cycle shortcut).
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    /// Curved faces have no edges along their sides: these styles draw their outlines.
    pub fn silhouettes(self) -> bool {
        matches!(
            self,
            Self::HiddenLinesRemoved | Self::HiddenLinesVisible | Self::Wireframe
        )
    }
}

/// CPU-side scene. Ids index the highlight state buffer and come back from picking.
#[derive(Clone, Debug, Default)]
pub struct SceneGeometry {
    pub triangles: Vec<MeshVertex>,
    pub lines: Vec<LineVertex>,
    /// Model (B-Rep) edges: hidden in the Shaded style, cut by the section view.
    pub edges: Vec<LineVertex>,
    /// Translucent triangles (reference planes).
    pub planes: Vec<LineVertex>,
    /// Pickable points (B-Rep vertices), drawn as overlay markers by the app.
    pub points: Vec<([f32; 3], u32)>,
    /// A coarse copy of `triangles`, drawn while the view moves (large models only).
    pub lod: Vec<MeshVertex>,
    /// Picking's bounding boxes, built on the first pick.
    pub chunk_boxes: std::sync::OnceLock<Vec<[Vec3; 2]>>,
}

impl SceneGeometry {
    pub fn line(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3], id: u32) {
        self.lines
            .extend([LineVertex { pos: a, color, id }, LineVertex { pos: b, color, id }]);
    }

    /// A model edge segment.
    pub fn edge(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3], id: u32) {
        self.edges
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

    /// Fills `lod` with a coarse copy of the triangles when there are more than `over` of them.
    pub fn build_lod(&mut self, over: usize) {
        self.lod = if self.triangles.len() / 3 > over {
            decimate(&self.triangles, 160)
        } else {
            Vec::new()
        };
    }
}

/// Vertex clustering: every vertex snaps to its cell of a `cells`-per-side grid over the bounds,
/// and triangles that shrink to a line or a point disappear. Pick ids and colours are kept.
pub fn decimate(tris: &[MeshVertex], cells: u32) -> Vec<MeshVertex> {
    let Some((lo, hi)) = SceneGeometry {
        triangles: tris.to_vec(),
        ..Default::default()
    }
    .bounds() else {
        return Vec::new();
    };
    let size = (hi - lo).max_element().max(1e-6) / cells as f32;
    let cell = |p: [f32; 3]| ((Vec3::from(p) - lo) / size).floor().as_ivec3();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in tris.chunks_exact(3) {
        let c = [cell(t[0].pos), cell(t[1].pos), cell(t[2].pos)];
        if c[0] == c[1] || c[1] == c[2] || c[0] == c[2] {
            continue;
        }
        let mut key = c.map(|c| (c.x, c.y, c.z));
        key.sort();
        if !seen.insert((key, t[0].id)) {
            continue;
        }
        out.extend(t.iter().zip(c).map(|(v, c)| MeshVertex {
            pos: (lo + (c.as_vec3() + 0.5) * size).to_array(),
            ..*v
        }));
    }
    out
}

/// The scene's triangle edges with the two faces' normals, for outlines of curved faces.
#[derive(Debug, Default)]
pub struct Adjacency {
    edges: Vec<([f32; 3], [f32; 3], Vec3, Vec3)>,
}

impl Adjacency {
    pub fn new(tris: &[MeshVertex]) -> Self {
        let key = |p: [f32; 3]| p.map(f32::to_bits);
        let mut first: HashMap<([u32; 3], [u32; 3]), Vec3> = HashMap::new();
        let mut edges = Vec::new();
        for t in tris.chunks_exact(3) {
            let [a, b, c] = [t[0].pos, t[1].pos, t[2].pos].map(Vec3::from);
            let n = (b - a).cross(c - a);
            if n.length_squared() == 0.0 {
                continue;
            }
            let n = n.normalize();
            for (p, q) in [(t[0].pos, t[1].pos), (t[1].pos, t[2].pos), (t[2].pos, t[0].pos)] {
                let k = if key(p) < key(q) {
                    (key(p), key(q))
                } else {
                    (key(q), key(p))
                };
                match first.remove(&k) {
                    Some(m) => edges.push((p, q, m, n)),
                    None => {
                        first.insert(k, n);
                    }
                }
            }
        }
        Self { edges }
    }

    /// Edges where the surface turns away from the eye (or, in an orthographic view, from
    /// `toward`, the direction the camera looks): the outline a curved face shows.
    pub fn silhouette(&self, eye: Vec3, toward: Option<Vec3>) -> Vec<LineVertex> {
        let mut out = Vec::new();
        for &(a, b, n1, n2) in &self.edges {
            let v = toward.unwrap_or_else(|| Vec3::from(a) - eye);
            if n1.dot(v) * n2.dot(v) < 0.0 {
                out.extend([a, b].map(|pos| LineVertex {
                    pos,
                    color: EDGE_COLOR,
                    id: NO_ID,
                }));
            }
        }
        out
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    body: [f32; 4],
    style: [f32; 4],
    clip: [f32; 4],
}

type Buffer = Option<(wgpu::Buffer, u32)>;

/// GPU state, stored in egui-wgpu's callback resources.
struct ViewportResources {
    pipelines: Pipelines,
    globals: wgpu::Buffer,
    states: wgpu::Buffer,
    states_len: usize,
    bind_group: wgpu::BindGroup,
    triangles: Buffer,
    lod: Buffer,
    lines: Buffer,
    edges: Buffer,
    planes: Buffer,
    silhouettes: Buffer,
    version: u64,
    states_version: u64,
    silhouettes_version: u64,
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
            lod: None,
            lines: None,
            edges: None,
            planes: None,
            silhouettes: None,
            version: u64::MAX,
            states_version: u64::MAX,
            silhouettes_version: u64::MAX,
        });
}

/// One frame of the 3D viewport. Geometry is re-uploaded only when `version` changes, the
/// highlight states only when `states_version` changes, and silhouettes with theirs.
pub struct ViewportCallback {
    pub view_proj: Mat4,
    pub eye: Vec3,
    /// Colour of the solid bodies (0..1 per channel).
    pub body_color: [f32; 3],
    pub geometry: Arc<SceneGeometry>,
    pub version: u64,
    /// Per pick id: 0 = normal, 1 = hovered, 2 = selected.
    pub states: Arc<Vec<u32>>,
    pub states_version: u64,
    pub style: DisplayStyle,
    /// Section plane: unit normal and offset; the side where `normal · p > offset` is cut away.
    pub clip: Option<[f32; 4]>,
    pub silhouettes: Arc<Vec<LineVertex>>,
    pub silhouettes_version: u64,
    /// Draw the coarse copy (the view is moving).
    pub lod: bool,
}

fn upload<T: Pod>(device: &wgpu::Device, data: &[T]) -> Buffer {
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
        let flat = matches!(
            self.style,
            DisplayStyle::HiddenLinesRemoved | DisplayStyle::HiddenLinesVisible
        );
        let globals = Globals {
            view_proj: self.view_proj.to_cols_array_2d(),
            eye: self.eye.extend(1.0).to_array(),
            body: [self.body_color[0], self.body_color[1], self.body_color[2], 1.0],
            style: [if flat { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            clip: self.clip.unwrap_or([0.0; 4]),
        };
        queue.write_buffer(&res.globals, 0, bytemuck::bytes_of(&globals));
        if res.version != self.version {
            res.triangles = upload(device, &self.geometry.triangles);
            res.lod = upload(device, &self.geometry.lod);
            res.lines = upload(device, &self.geometry.lines);
            res.edges = upload(device, &self.geometry.edges);
            res.planes = upload(device, &self.geometry.planes);
            res.version = self.version;
        }
        if res.silhouettes_version != self.silhouettes_version {
            res.silhouettes = upload(device, &self.silhouettes);
            res.silhouettes_version = self.silhouettes_version;
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
        let p = &res.pipelines;
        let style = self.style;
        let faces = match (&res.lod, self.lod) {
            (Some(_), true) => &res.lod,
            _ => &res.triangles,
        };
        // Planes last: they are translucent and do not write depth.
        let mut draws: Vec<(&wgpu::RenderPipeline, &Buffer)> = Vec::new();
        if style != DisplayStyle::Wireframe {
            draws.push((&p.mesh, faces));
        }
        draws.push((&p.lines, &res.lines));
        if style != DisplayStyle::Shaded && !(self.lod && res.lod.is_some()) {
            draws.push((&p.edges, &res.edges));
        }
        if style.silhouettes() && !self.lod {
            draws.push((&p.edges, &res.silhouettes));
        }
        if style == DisplayStyle::HiddenLinesVisible && !self.lod {
            draws.push((&p.hidden, &res.edges));
            draws.push((&p.hidden, &res.silhouettes));
        }
        draws.push((&p.planes, &res.planes));
        for (pipeline, buffer) in draws {
            if let Some((buffer, count)) = buffer {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed cylinder (radius 5, height 20, axis Z) as a triangle soup.
    fn cylinder() -> Vec<MeshVertex> {
        let n = 64;
        let mut out = Vec::new();
        let at = |i: usize, z: f32| {
            // The seam welds: the last column is the first one, to the bit.
            let a = (i % n) as f32 / n as f32 * std::f32::consts::TAU;
            [5.0 * a.cos(), 5.0 * a.sin(), z]
        };
        let mut tri = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
            out.extend([a, b, c].map(|pos| MeshVertex {
                pos,
                nor: [0.0; 3],
                id: 0,
                tint: [0; 4],
            }))
        };
        for i in 0..n {
            let (a, b, c, d) = (at(i, 0.0), at(i + 1, 0.0), at(i + 1, 20.0), at(i, 20.0));
            tri(a, b, c);
            tri(a, c, d);
            tri([0.0, 0.0, 0.0], b, a);
            tri([0.0, 0.0, 20.0], d, c);
        }
        out
    }

    #[test]
    fn a_cylinder_seen_from_the_side_shows_its_two_outlines() {
        let adj = Adjacency::new(&cylinder());
        // Orthographic, looking along +Y: the outline is the two side lines at x = ±5, 20 long,
        // plus the rim edges seen edge-on (where the caps meet the side).
        let lines = adj.silhouette(Vec3::ZERO, Some(Vec3::Y));
        let vertical: f32 = lines
            .chunks_exact(2)
            .filter(|l| (l[0].pos[2] - l[1].pos[2]).abs() > 1.0)
            .map(|l| {
                assert!((l[0].pos[0].abs() - 5.0).abs() < 0.05, "{:?}", l[0].pos);
                Vec3::from(l[0].pos).distance(Vec3::from(l[1].pos))
            })
            .sum();
        assert!((vertical - 40.0).abs() < 1e-3, "{vertical}");
    }

    #[test]
    fn decimation_keeps_the_shape_with_fewer_triangles() {
        let tris = cylinder();
        let coarse = decimate(&tris, 4);
        assert!(!coarse.is_empty() && coarse.len() < tris.len());
        let full = SceneGeometry {
            triangles: tris,
            ..Default::default()
        }
        .bounds()
        .unwrap();
        let lod = SceneGeometry {
            triangles: coarse,
            ..Default::default()
        }
        .bounds()
        .unwrap();
        // Vertices move to their cell's middle: at most half a cell (20 / 4 / 2 = 2.5).
        assert!(full.0.distance(lod.0) < 2.6 * 3f32.sqrt() && full.1.distance(lod.1) < 2.6 * 3f32.sqrt());
    }

    #[test]
    fn display_styles_cycle_and_round_trip() {
        let mut s = DisplayStyle::default();
        for _ in 0..DisplayStyle::ALL.len() {
            assert_eq!(DisplayStyle::from_id(s.id()), Some(s));
            s = s.next();
        }
        assert_eq!(s, DisplayStyle::ShadedWithEdges);
    }
}
