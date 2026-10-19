use crate::texture::GpuTexture;
use bytemuck::{Pod, Zeroable};
use skylands_formats::igb::{IgbGeometryNode, IgbMesh, IgbVertex};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct InstanceRaw {
    pub model: [[f32; 4]; 4],
}

impl Default for InstanceRaw {
    fn default() -> Self {
        Self {
            model: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

impl InstanceRaw {
    pub fn from_flat_slice(m: &[f32; 16]) -> Self {
        Self {
            model: [
                [m[0], m[1], m[2], m[3]],
                [m[4], m[5], m[6], m[7]],
                [m[8], m[9], m[10], m[11]],
                [m[12], m[13], m[14], m[15]],
            ],
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

impl From<&IgbVertex> for Vertex {
    fn from(v: &IgbVertex) -> Self {
        Self {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
            color: v.color,
        }
    }
}

impl From<IgbVertex> for Vertex {
    fn from(v: IgbVertex) -> Self {
        Self::from(&v)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub bone_indices: [u32; 4],
    pub bone_weights: [f32; 4],
}

impl SkinnedVertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x2,
        3 => Float32x4,
        4 => Uint32x4,
        5 => Float32x4
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SkinnedVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

impl From<&IgbVertex> for SkinnedVertex {
    fn from(v: &IgbVertex) -> Self {
        Self {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
            color: v.color,
            bone_indices: v.bone_indices,
            bone_weights: v.bone_weights,
        }
    }
}

impl From<IgbVertex> for SkinnedVertex {
    fn from(v: IgbVertex) -> Self {
        Self::from(&v)
    }
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material_name: Option<String>,
}

impl Mesh {
    pub fn new(name: impl Into<String>, vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        Self {
            name: name.into(),
            vertices,
            indices,
            material_name: None,
        }
    }

    pub fn with_material(mut self, material_name: impl Into<String>) -> Self {
        self.material_name = Some(material_name.into());
        self
    }

    pub fn from_igb(igb_mesh: &IgbMesh) -> Self {
        let vertices = igb_mesh.vertices.iter().map(Vertex::from).collect();
        Self {
            name: igb_mesh.name.clone(),
            vertices,
            indices: igb_mesh.indices.clone(),
            material_name: igb_mesh.material_name.clone(),
        }
    }

    pub fn from_geometry(geometry: &IgbGeometryNode) -> Option<Self> {
        geometry.mesh.as_ref().map(Self::from_igb)
    }

    pub fn unit_quad() -> Self {
        let vertices = vec![
            Vertex {
                position: [-0.5, 0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            Vertex {
                position: [-0.5, -0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        Self::new("unit_quad", vertices, indices)
    }

    pub fn portal_pedestal(radius: f32, height: f32, segments: u32) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        // Center top vertex
        vertices.push(Vertex {
            position: [0.0, height, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            color: [0.0, 0.8, 1.0, 1.0], // Cyan glow
        });

        // Top ring vertices
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            vertices.push(Vertex {
                position: [x, height, z],
                normal: [0.0, 1.0, 0.0],
                uv: [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
                color: [0.1, 0.5, 0.9, 1.0],
            });
        }

        // Top fan indices
        for i in 1..=segments {
            let next = if i == segments { 1 } else { i + 1 };
            indices.push(0);
            indices.push(i);
            indices.push(next);
        }

        // Base bottom vertex
        let bottom_center = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, -1.0, 0.0],
            uv: [0.5, 0.5],
            color: [0.2, 0.2, 0.2, 1.0],
        });

        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            vertices.push(Vertex {
                position: [x, 0.0, z],
                normal: [0.0, -1.0, 0.0],
                uv: [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
                color: [0.2, 0.2, 0.2, 1.0],
            });
        }

        for i in 1..=segments {
            let curr = bottom_center + i;
            let next = if i == segments {
                bottom_center + 1
            } else {
                bottom_center + i + 1
            };
            indices.push(bottom_center);
            indices.push(next);
            indices.push(curr);
        }

        Self::new("portal_pedestal", vertices, indices)
    }

    pub fn prop_cube(size: f32) -> Self {
        let half = size * 0.5;
        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        let color = [0.65, 0.45, 0.25, 1.0];

        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            (
                [0.0, 0.0, 1.0],
                [
                    [-half, -half, half],
                    [half, -half, half],
                    [half, half, half],
                    [-half, half, half],
                ],
            ),
            (
                [0.0, 0.0, -1.0],
                [
                    [half, -half, -half],
                    [-half, -half, -half],
                    [-half, half, -half],
                    [half, half, -half],
                ],
            ),
            (
                [-1.0, 0.0, 0.0],
                [
                    [-half, -half, -half],
                    [-half, -half, half],
                    [-half, half, half],
                    [-half, half, -half],
                ],
            ),
            (
                [1.0, 0.0, 0.0],
                [
                    [half, -half, half],
                    [half, -half, -half],
                    [half, half, -half],
                    [half, half, half],
                ],
            ),
            (
                [0.0, 1.0, 0.0],
                [
                    [-half, half, half],
                    [half, half, half],
                    [half, half, -half],
                    [-half, half, -half],
                ],
            ),
            (
                [0.0, -1.0, 0.0],
                [
                    [-half, -half, -half],
                    [half, -half, -half],
                    [half, -half, half],
                    [-half, -half, half],
                ],
            ),
        ];

        for (normal, quad) in faces {
            let start = vertices.len() as u32;
            let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
            for (p, uv) in quad.into_iter().zip(uvs) {
                vertices.push(Vertex {
                    position: p,
                    normal,
                    uv,
                    color,
                });
            }
            indices.push(start);
            indices.push(start + 1);
            indices.push(start + 2);
            indices.push(start);
            indices.push(start + 2);
            indices.push(start + 3);
        }

        Self::new("prop_cube", vertices, indices)
    }

    pub fn collectible_coin(radius: f32, thickness: f32, segments: u32) -> Self {
        let half_h = thickness * 0.5;
        let segs = segments.max(8);
        let gold_color = [1.0, 0.84, 0.0, 1.0];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let top_center = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, half_h, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            color: gold_color,
        });
        for i in 0..segs {
            let angle = (i as f32 / segs as f32) * std::f32::consts::TAU;
            vertices.push(Vertex {
                position: [angle.cos() * radius, half_h, angle.sin() * radius],
                normal: [0.0, 1.0, 0.0],
                uv: [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
                color: gold_color,
            });
        }
        for i in 1..=segs {
            let next = if i == segs { 1 } else { i + 1 };
            indices.push(top_center);
            indices.push(i);
            indices.push(next);
        }

        let bot_center = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, -half_h, 0.0],
            normal: [0.0, -1.0, 0.0],
            uv: [0.5, 0.5],
            color: gold_color,
        });
        for i in 0..segs {
            let angle = (i as f32 / segs as f32) * std::f32::consts::TAU;
            vertices.push(Vertex {
                position: [angle.cos() * radius, -half_h, angle.sin() * radius],
                normal: [0.0, -1.0, 0.0],
                uv: [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
                color: gold_color,
            });
        }
        for i in 1..=segs {
            let curr = bot_center + i;
            let next = if i == segs {
                bot_center + 1
            } else {
                bot_center + i + 1
            };
            indices.push(bot_center);
            indices.push(next);
            indices.push(curr);
        }

        for i in 0..segs {
            let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;
            let n0 = [a0.cos(), 0.0, a0.sin()];
            let n1 = [a1.cos(), 0.0, a1.sin()];

            let base = vertices.len() as u32;
            vertices.push(Vertex {
                position: [a0.cos() * radius, half_h, a0.sin() * radius],
                normal: n0,
                uv: [0.0, 0.0],
                color: gold_color,
            });
            vertices.push(Vertex {
                position: [a0.cos() * radius, -half_h, a0.sin() * radius],
                normal: n0,
                uv: [0.0, 1.0],
                color: gold_color,
            });
            vertices.push(Vertex {
                position: [a1.cos() * radius, -half_h, a1.sin() * radius],
                normal: n1,
                uv: [1.0, 1.0],
                color: gold_color,
            });
            vertices.push(Vertex {
                position: [a1.cos() * radius, half_h, a1.sin() * radius],
                normal: n1,
                uv: [1.0, 0.0],
                color: gold_color,
            });

            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 3);
        }

        Self::new("collectible_coin", vertices, indices)
    }

    pub fn collectible_gem(radius: f32, height: f32) -> Self {
        let half_h = height * 0.5;
        let segs = 6;
        let gem_color = [0.1, 0.9, 0.8, 1.0];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let top_idx = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, half_h, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            color: [0.3, 1.0, 0.9, 1.0],
        });

        let bot_idx = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, -half_h, 0.0],
            normal: [0.0, -1.0, 0.0],
            uv: [0.5, 0.5],
            color: [0.1, 0.7, 0.6, 1.0],
        });

        let ring_start = vertices.len() as u32;
        for i in 0..segs {
            let angle = (i as f32 / segs as f32) * std::f32::consts::TAU;
            vertices.push(Vertex {
                position: [angle.cos() * radius, 0.0, angle.sin() * radius],
                normal: [angle.cos(), 0.0, angle.sin()],
                uv: [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
                color: gem_color,
            });
        }

        for i in 0..segs {
            let curr = ring_start + i;
            let next = ring_start + ((i + 1) % segs);
            indices.push(top_idx);
            indices.push(curr);
            indices.push(next);
        }

        for i in 0..segs {
            let curr = ring_start + i;
            let next = ring_start + ((i + 1) % segs);
            indices.push(bot_idx);
            indices.push(next);
            indices.push(curr);
        }

        Self::new("collectible_gem", vertices, indices)
    }

    pub fn skybox_cube(size: f32) -> Self {
        let half = size * 0.5;
        // 8 cube vertices, inward facing normals, gradient sky color
        let p = [
            [-half, -half, -half], // 0
            [half, -half, -half],  // 1
            [half, half, -half],   // 2
            [-half, half, -half],  // 3
            [-half, -half, half],  // 4
            [half, -half, half],   // 5
            [half, half, half],    // 6
            [-half, half, half],   // 7
        ];

        let horizon_color = [0.45, 0.65, 0.85, 1.0];
        let zenith_color = [0.12, 0.28, 0.55, 1.0];

        let make_vert = |pos: [f32; 3]| -> Vertex {
            let t = ((pos[1] / half) + 1.0) * 0.5;
            let color = [
                horizon_color[0] * (1.0 - t) + zenith_color[0] * t,
                horizon_color[1] * (1.0 - t) + zenith_color[1] * t,
                horizon_color[2] * (1.0 - t) + zenith_color[2] * t,
                1.0,
            ];
            Vertex {
                position: pos,
                normal: [-pos[0] / half, -pos[1] / half, -pos[2] / half],
                uv: [0.0, 0.0],
                color,
            }
        };

        // Inward facing cube faces (quad = 2 triangles)
        let faces = [
            // Front (-Z): 0, 3, 2, 0, 2, 1
            (p[0], p[3], p[2], p[1]),
            // Back (+Z): 5, 6, 7, 5, 7, 4
            (p[5], p[6], p[7], p[4]),
            // Left (-X): 4, 7, 3, 4, 3, 0
            (p[4], p[7], p[3], p[0]),
            // Right (+X): 1, 2, 6, 1, 6, 5
            (p[1], p[2], p[6], p[5]),
            // Top (+Y): 3, 7, 6, 3, 6, 2
            (p[3], p[7], p[6], p[2]),
            // Bottom (-Y): 4, 0, 1, 4, 1, 5
            (p[4], p[0], p[1], p[5]),
        ];

        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);

        for (v0, v1, v2, v3) in faces {
            let start = vertices.len() as u32;
            vertices.push(make_vert(v0));
            vertices.push(make_vert(v1));
            vertices.push(make_vert(v2));
            vertices.push(make_vert(v3));

            indices.push(start);
            indices.push(start + 1);
            indices.push(start + 2);
            indices.push(start);
            indices.push(start + 2);
            indices.push(start + 3);
        }

        Self::new("skybox_cube", vertices, indices)
    }
}

pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_elements: u32,
    pub material_name: Option<String>,
}

impl GpuMesh {
    pub fn from_mesh(device: &wgpu::Device, mesh: &Mesh) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Vertex Buffer", mesh.name)),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Index Buffer", mesh.name)),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            num_elements: mesh.indices.len() as u32,
            material_name: mesh.material_name.clone(),
        }
    }
}

pub struct ModelInstance {
    pub submeshes: Vec<(GpuMesh, Option<GpuTexture>)>,
}

impl ModelInstance {
    pub fn new(submeshes: Vec<(GpuMesh, Option<GpuTexture>)>) -> Self {
        Self { submeshes }
    }

    pub fn is_empty(&self) -> bool {
        self.submeshes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.submeshes.len()
    }
}

pub struct EnvironmentSubmesh {
    pub mesh: GpuMesh,
    pub texture: Option<GpuTexture>,
    pub instances: Vec<InstanceRaw>,
}

pub struct EnvironmentScene {
    pub name: String,
    pub submeshes: Vec<EnvironmentSubmesh>,
}

impl EnvironmentScene {
    pub fn new(name: impl Into<String>, submeshes: Vec<EnvironmentSubmesh>) -> Self {
        Self {
            name: name.into(),
            submeshes,
        }
    }

    pub fn synthetic_courtyard_meshes() -> (Mesh, Mesh) {
        // Ground plane: 40x40 cobblestone courtyard quad at y = 0.0
        let ground = Mesh::new(
            "courtyard_ground",
            vec![
                Vertex {
                    position: [-20.0, 0.0, -20.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                    color: [0.45, 0.45, 0.45, 1.0],
                },
                Vertex {
                    position: [20.0, 0.0, -20.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [8.0, 0.0],
                    color: [0.5, 0.5, 0.5, 1.0],
                },
                Vertex {
                    position: [20.0, 0.0, 20.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [8.0, 8.0],
                    color: [0.4, 0.4, 0.4, 1.0],
                },
                Vertex {
                    position: [-20.0, 0.0, 20.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 8.0],
                    color: [0.42, 0.42, 0.42, 1.0],
                },
            ],
            vec![0, 1, 2, 0, 2, 3],
        )
        .with_material("mat_courtyard_stone");

        // Boundary walls: 4 surrounding stone wall segments
        let mut wall_verts = Vec::new();
        let mut wall_indices = Vec::new();
        let wall_height = 3.0;

        let wall_segments = [
            // North wall
            ([-20.0, 0.0, -20.0], [20.0, 0.0, -20.0], [0.0, 0.0, 1.0]),
            // South wall
            ([20.0, 0.0, 20.0], [-20.0, 0.0, 20.0], [0.0, 0.0, -1.0]),
            // East wall
            ([20.0, 0.0, -20.0], [20.0, 0.0, 20.0], [-1.0, 0.0, 0.0]),
            // West wall
            ([-20.0, 0.0, 20.0], [-20.0, 0.0, -20.0], [1.0, 0.0, 0.0]),
        ];

        for (p0, p1, normal) in wall_segments {
            let base_idx = wall_verts.len() as u32;
            wall_verts.push(Vertex {
                position: p0,
                normal,
                uv: [0.0, 0.0],
                color: [0.35, 0.35, 0.38, 1.0],
            });
            wall_verts.push(Vertex {
                position: p1,
                normal,
                uv: [4.0, 0.0],
                color: [0.35, 0.35, 0.38, 1.0],
            });
            wall_verts.push(Vertex {
                position: [p1[0], wall_height, p1[2]],
                normal,
                uv: [4.0, 1.0],
                color: [0.3, 0.3, 0.32, 1.0],
            });
            wall_verts.push(Vertex {
                position: [p0[0], wall_height, p0[2]],
                normal,
                uv: [0.0, 1.0],
                color: [0.3, 0.3, 0.32, 1.0],
            });

            wall_indices.extend_from_slice(&[
                base_idx,
                base_idx + 1,
                base_idx + 2,
                base_idx,
                base_idx + 2,
                base_idx + 3,
            ]);
        }

        let walls = Mesh::new("courtyard_walls", wall_verts, wall_indices)
            .with_material("mat_courtyard_walls");

        (ground, walls)
    }

    pub fn synthetic_courtyard(device: &wgpu::Device) -> Self {
        let (ground_mesh, walls_mesh) = Self::synthetic_courtyard_meshes();
        let gpu_ground = GpuMesh::from_mesh(device, &ground_mesh);
        let gpu_walls = GpuMesh::from_mesh(device, &walls_mesh);

        Self {
            name: "synthetic_courtyard".to_string(),
            submeshes: vec![
                EnvironmentSubmesh {
                    mesh: gpu_ground,
                    texture: None,
                    instances: vec![InstanceRaw::default()],
                },
                EnvironmentSubmesh {
                    mesh: gpu_walls,
                    texture: None,
                    instances: vec![InstanceRaw::default()],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_conversion_from_igb() {
        let igb_mesh = IgbMesh {
            name: "test_igb".to_string(),
            vertices: vec![IgbVertex {
                position: [1.0, 2.0, 3.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.5, 0.5],
                color: [1.0, 0.0, 0.0, 1.0],
                ..Default::default()
            }],
            indices: vec![0, 0, 0],
            material_name: Some("test_mat".to_string()),
        };

        let mesh = Mesh::from_igb(&igb_mesh);
        assert_eq!(mesh.name, "test_igb");
        assert_eq!(mesh.material_name, Some("test_mat".to_string()));
        assert_eq!(mesh.vertices.len(), 1);
        assert_eq!(mesh.vertices[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(mesh.indices, vec![0, 0, 0]);
    }

    #[test]
    fn test_mesh_conversion_from_geometry_node() {
        let igb_mesh = IgbMesh {
            name: "geo_mesh".to_string(),
            vertices: vec![IgbVertex::default()],
            indices: vec![0, 0, 0],
            material_name: Some("geo_mat".to_string()),
        };
        let geo = IgbGeometryNode {
            name: "geo_node".to_string(),
            vertex_buffer_index: Some(0),
            index_buffer_index: Some(0),
            attr_list_index: Some(0),
            material_index: Some(0),
            mesh: Some(igb_mesh),
        };

        let mesh = Mesh::from_geometry(&geo).expect("Mesh converted from geometry");
        assert_eq!(mesh.name, "geo_mesh");
        assert_eq!(mesh.material_name, Some("geo_mat".to_string()));
        assert_eq!(mesh.vertices.len(), 1);
    }

    #[test]
    fn test_portal_pedestal_mesh() {
        let pedestal = Mesh::portal_pedestal(1.0, 0.2, 16);
        assert!(!pedestal.vertices.is_empty());
        assert!(!pedestal.indices.is_empty());
    }

    #[test]
    fn test_synthetic_courtyard_mesh_generation() {
        let (ground, walls) = EnvironmentScene::synthetic_courtyard_meshes();
        assert_eq!(ground.vertices.len(), 4);
        assert_eq!(ground.indices.len(), 6);
        assert_eq!(
            ground.material_name,
            Some("mat_courtyard_stone".to_string())
        );

        assert_eq!(walls.vertices.len(), 16); // 4 walls * 4 verts
        assert_eq!(walls.indices.len(), 24); // 4 walls * 6 indices
        assert_eq!(walls.material_name, Some("mat_courtyard_walls".to_string()));

        let inst = InstanceRaw::from_flat_slice(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 6.0, 7.0, 1.0,
        ]);
        assert_eq!(inst.model[3], [5.0, 6.0, 7.0, 1.0]);
    }
}
