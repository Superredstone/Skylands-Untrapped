use crate::mesh::{InstanceRaw, SkinnedVertex, Vertex};
use bytemuck::{Pod, Zeroable};

pub const SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct LightUniform {
    direction: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    intensity: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<uniform> light: LightUniform;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct InstanceInput {
    @location(4) model_0: vec4<f32>,
    @location(5) model_1: vec4<f32>,
    @location(6) model_2: vec4<f32>,
    @location(7) model_3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );
    var out: VertexOutput;
    out.clip_position = camera.view_proj * model_matrix * vec4<f32>(model.position, 1.0);
    let normal_matrix = mat3x3<f32>(
        model_matrix[0].xyz,
        model_matrix[1].xyz,
        model_matrix[2].xyz,
    );
    out.normal = normal_matrix * model.normal;
    out.uv = model.uv;
    out.color = model.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    let base_color = in.color * tex_color;

    let normal = normalize(in.normal);
    let ambient = light.ambient_color * light.ambient_intensity;
    let diff = max(dot(normal, -normalize(light.direction)), 0.0) * light.color * light.intensity;
    let final_rgb = (ambient + diff) * base_color.rgb;

    return vec4<f32>(final_rgb, base_color.a);
}
"#;

pub const SKYBOX_SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct SkyboxVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct SkyboxVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_skybox(input: SkyboxVertexInput) -> SkyboxVertexOutput {
    var out: SkyboxVertexOutput;
    var vp = camera.view_proj;
    vp[3] = vec4<f32>(0.0, 0.0, 0.0, vp[3].w);

    let clip = vp * vec4<f32>(input.position, 1.0);
    out.clip_position = vec4<f32>(clip.x, clip.y, clip.w, clip.w);
    out.color = input.color;
    out.uv = input.uv;
    return out;
}

@fragment
fn fs_skybox(in: SkyboxVertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

pub const SKINNING_SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct BoneUniform {
    matrices: array<mat4x4<f32>, 64>,
};

@group(1) @binding(0)
var<uniform> bones: BoneUniform;

struct SkinnedVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) bone_indices: vec4<u32>,
    @location(5) bone_weights: vec4<f32>,
};

struct InstanceInput {
    @location(6) model_matrix_0: vec4<f32>,
    @location(7) model_matrix_1: vec4<f32>,
    @location(8) model_matrix_2: vec4<f32>,
    @location(9) model_matrix_3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_skinned(model: SkinnedVertexInput, instance: InstanceInput) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    let skin_matrix =
        model.bone_weights[0] * bones.matrices[model.bone_indices[0]] +
        model.bone_weights[1] * bones.matrices[model.bone_indices[1]] +
        model.bone_weights[2] * bones.matrices[model.bone_indices[2]] +
        model.bone_weights[3] * bones.matrices[model.bone_indices[3]];

    let world_pos = model_matrix * skin_matrix * vec4<f32>(model.position, 1.0);
    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_pos;
    out.uv = model.uv;
    out.color = model.color;
    out.world_normal = (model_matrix * skin_matrix * vec4<f32>(model.normal, 0.0)).xyz;
    return out;
}

@group(2) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(2) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_skinned(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    return in.color * tex_color;
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BoneUniform {
    pub matrices: [[[f32; 4]; 4]; 64],
}

impl Default for BoneUniform {
    fn default() -> Self {
        Self {
            matrices: [[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]; 64],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct LightUniform {
    pub direction: [f32; 3],
    pub _pad0: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            direction: [-0.5, -0.8, -0.3],
            _pad0: 0.0,
            color: [1.0, 0.98, 0.9],
            intensity: 1.0,
            ambient_color: [0.3, 0.35, 0.45],
            ambient_intensity: 0.6,
        }
    }
}

pub fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Skylands Mesh Shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
    });

    let depth_stencil = depth_format.map(|df| wgpu::DepthStencilState {
        format: df,
        depth_write_enabled: true,
        depth_compare: wgpu::CompareFunction::Less,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Skylands Render Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc(), InstanceRaw::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

pub fn create_skybox_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Skylands Skybox Shader"),
        source: wgpu::ShaderSource::Wgsl(SKYBOX_SHADER_SRC.into()),
    });

    let depth_stencil = depth_format.map(|df| wgpu::DepthStencilState {
        format: df,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::LessEqual,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Skylands Skybox Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_skybox"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_skybox"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

pub fn create_skinned_render_pipeline(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
    bone_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Skinned Render Pipeline Layout"),
        bind_group_layouts: &[camera_layout, bone_layout, texture_layout],
        push_constant_ranges: &[],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Skinned Mesh Shader"),
        source: wgpu::ShaderSource::Wgsl(SKINNING_SHADER_SRC.into()),
    });

    let depth_stencil = depth_format.map(|df| wgpu::DepthStencilState {
        format: df,
        depth_write_enabled: true,
        depth_compare: wgpu::CompareFunction::Less,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Skinned Render Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_skinned"),
            buffers: &[SkinnedVertex::desc(), InstanceRaw::desc_skinned()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_skinned"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_light_uniform_alignment_and_defaults() {
        assert_eq!(std::mem::size_of::<LightUniform>(), 48);
        assert_eq!(std::mem::align_of::<LightUniform>(), 4);
        let light = LightUniform::default();
        assert_eq!(light.intensity, 1.0);
        assert_eq!(light.ambient_intensity, 0.6);
    }

    #[test]
    fn test_bone_uniform_size_and_alignment() {
        assert_eq!(std::mem::size_of::<BoneUniform>(), 4096);
        assert_eq!(std::mem::align_of::<BoneUniform>(), 4);
        let bones = BoneUniform::default();
        assert_eq!(bones.matrices[0][0][0], 1.0);
        assert_eq!(bones.matrices[63][3][3], 1.0);
    }

    #[test]
    fn test_skinned_vertex_stride_and_shader_compilation() {
        use crate::mesh::SkinnedVertex;
        assert_eq!(std::mem::size_of::<SkinnedVertex>(), 80);
        let desc = SkinnedVertex::desc();
        assert_eq!(desc.array_stride, 80);
        assert_eq!(desc.attributes.len(), 6);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            })) {
                Some(a) => a,
                None => return,
            };

        let (device, _) = match pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default(), None),
        ) {
            Ok(pair) => pair,
            Err(_) => return,
        };

        let _shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Skinned Vertex Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SKINNING_SHADER_SRC)),
        });
    }
}
