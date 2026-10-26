use egui_wgpu::wgpu;

const SHADER: &str = r#"
struct Globals { view_proj: mat4x4<f32>, eye: vec4<f32> };
@group(0) @binding(0) var<uniform> globals: Globals;
// Per pick id: 0 = normal, 1 = hovered, 2 = selected.
@group(0) @binding(1) var<storage, read> states: array<u32>;

const HOVER = vec3<f32>(1.0, 0.55, 0.1);
const SELECT = vec3<f32>(0.1, 0.5, 1.0);

fn state_of(id: u32) -> u32 {
    if (id < arrayLength(&states)) { return states[id]; }
    return 0u;
}

fn tint(c: vec3<f32>, s: u32, amount: f32) -> vec3<f32> {
    if (s == 1u) { return mix(c, HOVER, amount); }
    if (s == 2u) { return mix(c, SELECT, amount); }
    return c;
}

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) id: u32,
};

@vertex
fn vs_mesh(@location(0) pos: vec3<f32>, @location(1) nor: vec3<f32>, @location(2) id: u32) -> MeshOut {
    var out: MeshOut;
    out.clip = globals.view_proj * vec4<f32>(pos, 1.0);
    out.world = pos;
    out.normal = nor;
    out.id = id;
    return out;
}

@fragment
fn fs_mesh(frag: MeshOut) -> @location(0) vec4<f32> {
    let v = normalize(globals.eye.xyz - frag.world);
    var n = normalize(frag.normal);
    if (dot(n, v) < 0.0) { n = -n; }
    // Headlight slightly above-right of the eye, plus a sky/ground hemisphere fill.
    let l = normalize(v + vec3<f32>(0.25, 0.25, 0.6));
    let diffuse = max(dot(n, l), 0.0);
    let hemi = 0.5 + 0.5 * n.z;
    let spec = pow(max(dot(n, normalize(l + v)), 0.0), 64.0);
    let base = vec3<f32>(0.70, 0.74, 0.80);
    let c = base * (0.22 + 0.18 * hemi + 0.6 * diffuse) + vec3<f32>(0.25 * spec);
    return vec4<f32>(tint(c, state_of(frag.id), 0.45), 1.0);
}

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) @interpolate(flat) id: u32,
};

@vertex
fn vs_line(@location(0) pos: vec3<f32>, @location(1) color: vec3<f32>, @location(2) id: u32) -> LineOut {
    var out: LineOut;
    out.clip = globals.view_proj * vec4<f32>(pos, 1.0);
    out.color = color;
    out.id = id;
    return out;
}

@fragment
fn fs_line(frag: LineOut) -> @location(0) vec4<f32> {
    return vec4<f32>(tint(frag.color, state_of(frag.id), 1.0), 1.0);
}

@fragment
fn fs_plane(frag: LineOut) -> @location(0) vec4<f32> {
    let s = state_of(frag.id);
    let alpha = select(0.10, 0.28, s != 0u);
    return vec4<f32>(tint(frag.color, s, 0.8), alpha);
}
"#;

/// Shaded faces, lines (B-Rep edges, grid, axes) and translucent reference planes.
pub struct Pipelines {
    pub mesh: wgpu::RenderPipeline,
    pub lines: wgpu::RenderPipeline,
    pub planes: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl Pipelines {
    /// Formats and sample count must match the egui render pass the viewport paints into.
    pub fn new(
        device: &wgpu::Device,
        color: wgpu::TextureFormat,
        depth: Option<wgpu::TextureFormat>,
        msaa: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3bcad viewport"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3bcad globals"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3bcad viewport"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        // Every vertex type is (vec3, vec3, u32 pick id): (position, normal|color, id).
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32];
        let make = |vs: &str, fs: &str, topology, bias, blend: Option<wgpu::BlendState>, depth_write| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 28,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: depth.map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: depth_write,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: msaa,
                    ..Default::default()
                },
                multiview: None,
                cache: None,
            })
        };
        // Faces are pushed back slightly so edges drawn on them win the depth test.
        let face_bias = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 2.0,
            clamp: 0.0,
        };
        let tri = wgpu::PrimitiveTopology::TriangleList;
        Self {
            mesh: make("vs_mesh", "fs_mesh", tri, face_bias, None, true),
            lines: make(
                "vs_line",
                "fs_line",
                wgpu::PrimitiveTopology::LineList,
                Default::default(),
                None,
                true,
            ),
            planes: make(
                "vs_line",
                "fs_plane",
                tri,
                Default::default(),
                Some(wgpu::BlendState::ALPHA_BLENDING),
                false,
            ),
            bind_group_layout,
        }
    }
}
