use egui_wgpu::wgpu;

const SHADER: &str = r#"
struct Globals { view_proj: mat4x4<f32>, eye: vec4<f32> };
@group(0) @binding(0) var<uniform> globals: Globals;

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vs_mesh(@location(0) pos: vec3<f32>, @location(1) nor: vec3<f32>) -> MeshOut {
    var out: MeshOut;
    out.clip = globals.view_proj * vec4<f32>(pos, 1.0);
    out.world = pos;
    out.normal = nor;
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
    return vec4<f32>(base * (0.22 + 0.18 * hemi + 0.6 * diffuse) + vec3<f32>(0.25 * spec), 1.0);
}

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_line(@location(0) pos: vec3<f32>, @location(1) color: vec3<f32>) -> LineOut {
    var out: LineOut;
    out.clip = globals.view_proj * vec4<f32>(pos, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_line(frag: LineOut) -> @location(0) vec4<f32> {
    return vec4<f32>(frag.color, 1.0);
}
"#;

/// Shaded-faces pipeline and line pipeline (B-Rep edges, grid, axes).
pub struct Pipelines {
    pub mesh: wgpu::RenderPipeline,
    pub lines: wgpu::RenderPipeline,
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
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3bcad globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3bcad viewport"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        // Both vertex types are two vec3s: (position, normal) or (position, color).
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let make = |vs: &str, fs: &str, topology, bias| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 24,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(color.into())],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: depth.map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: true,
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
        Self {
            mesh: make("vs_mesh", "fs_mesh", wgpu::PrimitiveTopology::TriangleList, face_bias),
            lines: make(
                "vs_line",
                "fs_line",
                wgpu::PrimitiveTopology::LineList,
                Default::default(),
            ),
            bind_group_layout,
        }
    }
}
