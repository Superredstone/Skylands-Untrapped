// ponytail: 8x8 font atlas bitmap + quad batching; upgrade to msdf/ttf when complex typography needed
use crate::camera::mat4;
use crate::texture::GpuTexture;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub const UI_SHADER_SRC: &str = r#"
struct UiUniform {
    proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> ui_proj: UiUniform;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct UiVertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct UiVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_ui(model: UiVertexInput) -> UiVertexOutput {
    var out: UiVertexOutput;
    out.clip_position = ui_proj.proj * vec4<f32>(model.position, 0.0, 1.0);
    out.uv = model.uv;
    out.color = model.color;
    return out;
}

@fragment
fn fs_ui(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(t_diffuse, s_diffuse, in.uv);
    return in.color * tex;
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl UiVertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct UiUniform {
    pub proj: [[f32; 4]; 4],
}

pub struct UiDrawCall {
    pub vertex_offset: u32,
    pub index_offset: u32,
    pub index_count: u32,
    pub texture_id: usize,
}

#[derive(Default)]
pub struct UiBatch {
    pub vertices: Vec<UiVertex>,
    pub indices: Vec<u32>,
    pub draw_calls: Vec<UiDrawCall>,
    current_texture_id: usize,
}

impl UiBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.draw_calls.clear();
        self.current_texture_id = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn set_texture(&mut self, texture_id: usize) {
        if self.current_texture_id != texture_id && !self.indices.is_empty() {
            let last_index = self.indices.len() as u32;
            let last_call_start = self
                .draw_calls
                .last()
                .map(|c| c.index_offset + c.index_count)
                .unwrap_or(0);
            let count = last_index - last_call_start;
            if count > 0 {
                self.draw_calls.push(UiDrawCall {
                    vertex_offset: 0,
                    index_offset: last_call_start,
                    index_count: count,
                    texture_id: self.current_texture_id,
                });
            }
        }
        self.current_texture_id = texture_id;
    }

    pub fn draw_rect(&mut self, rect: [f32; 4], color: [f32; 4]) {
        self.set_texture(0);
        let uv = [0.0, 0.0, 1.0, 1.0];
        self.push_quad(rect, uv, color);
    }

    pub fn draw_textured_rect(
        &mut self,
        rect: [f32; 4],
        uv: [f32; 4],
        color: [f32; 4],
        texture_id: usize,
    ) {
        self.set_texture(texture_id);
        self.push_quad(rect, uv, color);
    }

    fn push_quad(&mut self, rect: [f32; 4], uv: [f32; 4], color: [f32; 4]) {
        let x = rect[0];
        let y = rect[1];
        let w = rect[2];
        let h = rect[3];

        let u0 = uv[0];
        let v0 = uv[1];
        let u1 = uv[2];
        let v1 = uv[3];

        let start_idx = self.vertices.len() as u32;

        self.vertices.push(UiVertex {
            position: [x, y],
            uv: [u0, v0],
            color,
        });
        self.vertices.push(UiVertex {
            position: [x + w, y],
            uv: [u1, v0],
            color,
        });
        self.vertices.push(UiVertex {
            position: [x + w, y + h],
            uv: [u1, v1],
            color,
        });
        self.vertices.push(UiVertex {
            position: [x, y + h],
            uv: [u0, v1],
            color,
        });

        self.indices.push(start_idx);
        self.indices.push(start_idx + 1);
        self.indices.push(start_idx + 2);
        self.indices.push(start_idx);
        self.indices.push(start_idx + 2);
        self.indices.push(start_idx + 3);
    }

    pub fn draw_diamond(&mut self, center_x: f32, center_y: f32, radius: f32, color: [f32; 4]) {
        self.set_texture(0);
        let start_idx = self.vertices.len() as u32;
        let uv = [0.5, 0.5];

        self.vertices.push(UiVertex {
            position: [center_x, center_y - radius],
            uv,
            color,
        });
        self.vertices.push(UiVertex {
            position: [center_x + radius, center_y],
            uv,
            color,
        });
        self.vertices.push(UiVertex {
            position: [center_x, center_y + radius],
            uv,
            color,
        });
        self.vertices.push(UiVertex {
            position: [center_x - radius, center_y],
            uv,
            color,
        });

        self.indices.push(start_idx);
        self.indices.push(start_idx + 1);
        self.indices.push(start_idx + 2);
        self.indices.push(start_idx);
        self.indices.push(start_idx + 2);
        self.indices.push(start_idx + 3);
    }

    pub fn draw_text(
        &mut self,
        font_texture_id: usize,
        text: &str,
        mut x: f32,
        y: f32,
        char_size: f32,
        color: [f32; 4],
    ) {
        self.set_texture(font_texture_id);
        for b in text.bytes() {
            let ch = if (32..=126).contains(&b) { b } else { b'?' };
            let col = (ch % 16) as f32;
            let row = ((ch / 16) - 2) as f32; // starts at ascii 32 (row 0..5)
            let u0 = col / 16.0;
            let v0 = row / 6.0;
            let u1 = (col + 1.0) / 16.0;
            let v1 = (row + 1.0) / 6.0;

            self.push_quad([x, y, char_size, char_size], [u0, v0, u1, v1], color);
            x += char_size;
        }
    }

    pub fn finish_draw_calls(&mut self) {
        let total = self.indices.len() as u32;
        let last_start = self
            .draw_calls
            .last()
            .map(|c| c.index_offset + c.index_count)
            .unwrap_or(0);
        let rem = total - last_start;
        if rem > 0 {
            self.draw_calls.push(UiDrawCall {
                vertex_offset: 0,
                index_offset: last_start,
                index_count: rem,
                texture_id: self.current_texture_id,
            });
        }
    }
}

pub struct FontAtlas {
    pub texture: GpuTexture,
}

impl FontAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        // 16 columns x 6 rows of 8x8 glyphs: 128x48 RGBA
        const ATLAS_W: u32 = 128;
        const ATLAS_H: u32 = 48;
        let mut pixels = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];

        // 8x8 basic font glyphs for ASCII 32..127
        for ch in 32u8..=126u8 {
            let col = (ch % 16) as u32;
            let row = ((ch / 16) - 2) as u32;
            let bitmap = get_8x8_glyph(ch);
            for (gy, row_bits) in bitmap.iter().enumerate() {
                for gx in 0..8 {
                    if (row_bits & (1 << (7 - gx))) != 0 {
                        let px = col * 8 + gx;
                        let py = row * 8 + gy as u32;
                        let idx = ((py * ATLAS_W + px) * 4) as usize;
                        pixels[idx] = 255;
                        pixels[idx + 1] = 255;
                        pixels[idx + 2] = 255;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }

        let texture =
            GpuTexture::from_rgba(device, queue, &pixels, ATLAS_W, ATLAS_H, Some("Font Atlas"))
                .expect("Failed to build font atlas texture");
        Self { texture }
    }
}

pub struct UiPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub font_atlas: FontAtlas,
    pub white_texture: GpuTexture,
}

impl UiPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UI Shader Module"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER_SRC.into()),
        });

        let ortho = mat4::orthographic(0.0, width as f32, height as f32, 0.0, -1.0, 1.0);
        let uniform = UiUniform { proj: ortho };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UI Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("UI Uniform Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("UI Uniform Bind Group"),
        });

        let texture_layout = GpuTexture::bind_group_layout(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("UI Pipeline Layout"),
            bind_group_layouts: &[&uniform_layout, &texture_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UI Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_ui"),
                buffers: &[UiVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ui"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::Renderer::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let font_atlas = FontAtlas::new(device, queue);
        let white_pixels = [255u8, 255, 255, 255];
        let white_texture =
            GpuTexture::from_rgba(device, queue, &white_pixels, 1, 1, Some("UI White Tex"))
                .expect("Failed to build white texture");

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            font_atlas,
            white_texture,
        }
    }

    pub fn resize(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let ortho = mat4::orthographic(
            0.0,
            width.max(1) as f32,
            height.max(1) as f32,
            0.0,
            -1.0,
            1.0,
        );
        let uniform = UiUniform { proj: ortho };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }
}

// 8x8 font glyph definitions
fn get_8x8_glyph(ch: u8) -> [u8; 8] {
    match ch {
        b'0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        b'1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        b'2' => [0x3C, 0x66, 0x06, 0x1C, 0x30, 0x60, 0x7E, 0x00],
        b'3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        b'4' => [0x0C, 0x1C, 0x34, 0x64, 0x7E, 0x04, 0x04, 0x00],
        b'5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        b'6' => [0x1C, 0x30, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        b'7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        b'8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        b'9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        b'A' | b'a' => [0x18, 0x3C, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        b'B' | b'b' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        b'C' | b'c' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        b'D' | b'd' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        b'E' | b'e' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
        b'F' | b'f' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
        b'G' | b'g' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3A, 0x00],
        b'H' | b'h' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        b'I' | b'i' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        b'J' | b'j' => [0x0E, 0x06, 0x06, 0x06, 0x06, 0x66, 0x3C, 0x00],
        b'K' | b'k' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        b'L' | b'l' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        b'M' | b'm' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        b'N' | b'n' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        b'O' | b'o' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        b'P' | b'p' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        b'Q' | b'q' => [0x3C, 0x66, 0x66, 0x66, 0x6A, 0x6C, 0x36, 0x00],
        b'R' | b'r' => [0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00],
        b'S' | b's' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        b'T' | b't' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        b'U' | b'u' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        b'V' | b'v' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        b'W' | b'w' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        b'X' | b'x' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
        b'Y' | b'y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        b'Z' | b'z' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
        b' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        b':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        b'/' => [0x06, 0x0C, 0x18, 0x30, 0x60, 0x40, 0x00, 0x00],
        b'-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        b'%' => [0x62, 0x64, 0x08, 0x10, 0x26, 0x46, 0x00, 0x00],
        b'<' => [0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x00],
        b'>' => [0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x00],
        b'(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        b')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        _ => [0x3C, 0x66, 0x06, 0x1C, 0x18, 0x00, 0x18, 0x00], // '?'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_batch_quad_and_rect() {
        let mut batch = UiBatch::new();
        batch.draw_rect([10.0, 20.0, 100.0, 50.0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(batch.vertices.len(), 4);
        assert_eq!(batch.indices.len(), 6);
        assert_eq!(batch.vertices[0].position, [10.0, 20.0]);
        assert_eq!(batch.vertices[2].position, [110.0, 70.0]);

        batch.finish_draw_calls();
        assert_eq!(batch.draw_calls.len(), 1);
        assert_eq!(batch.draw_calls[0].index_count, 6);
    }

    #[test]
    fn test_ui_orthographic_projection() {
        let ortho = mat4::orthographic(0.0, 800.0, 600.0, 0.0, -1.0, 1.0);
        // Map (0, 0) -> (-1, 1)
        let x_ndc = ortho[0][0] * 0.0 + ortho[3][0];
        let y_ndc = ortho[1][1] * 0.0 + ortho[3][1];
        assert!((x_ndc - (-1.0)).abs() < 1e-5);
        assert!((y_ndc - 1.0).abs() < 1e-5);

        // Map (800, 600) -> (1, -1)
        let x_ndc2 = ortho[0][0] * 800.0 + ortho[3][0];
        let y_ndc2 = ortho[1][1] * 600.0 + ortho[3][1];
        assert!((x_ndc2 - 1.0).abs() < 1e-5);
        assert!((y_ndc2 - (-1.0)).abs() < 1e-5);
    }
}
