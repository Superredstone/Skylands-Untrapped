use bytemuck::{Pod, Zeroable};
use skylands_formats::igb::{IgbMesh, IgbVertex};
use wgpu::util::DeviceExt;

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

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn new(name: impl Into<String>, vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        Self {
            name: name.into(),
            vertices,
            indices,
        }
    }

    pub fn from_igb(igb_mesh: &IgbMesh) -> Self {
        let vertices = igb_mesh.vertices.iter().map(Vertex::from).collect();
        Self {
            name: igb_mesh.name.clone(),
            vertices,
            indices: igb_mesh.indices.clone(),
        }
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
}

pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_elements: u32,
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
            }],
            indices: vec![0, 0, 0],
        };

        let mesh = Mesh::from_igb(&igb_mesh);
        assert_eq!(mesh.name, "test_igb");
        assert_eq!(mesh.vertices.len(), 1);
        assert_eq!(mesh.vertices[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(mesh.indices, vec![0, 0, 0]);
    }

    #[test]
    fn test_portal_pedestal_mesh() {
        let pedestal = Mesh::portal_pedestal(1.0, 0.2, 16);
        assert!(!pedestal.vertices.is_empty());
        assert!(!pedestal.indices.is_empty());
    }
}
