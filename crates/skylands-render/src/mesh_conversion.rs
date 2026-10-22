use bevy::render::mesh::{Indices, Mesh as BevyMesh, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use skylands_formats::igb::{transform_point, IgbMesh, IgbModel};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
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

    pub fn to_bevy(&self) -> BevyMesh {
        let mut bevy_mesh = BevyMesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );

        let positions: Vec<[f32; 3]> = self.vertices.iter().map(|v| v.position).collect();
        let normals: Vec<[f32; 3]> = self.vertices.iter().map(|v| v.normal).collect();
        let uvs: Vec<[f32; 2]> = self.vertices.iter().map(|v| v.uv).collect();
        let colors: Vec<[f32; 4]> = self.vertices.iter().map(|v| v.color).collect();

        bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_POSITION, positions);
        bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_NORMAL, normals);
        bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_UV_0, uvs);
        bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_COLOR, colors);
        bevy_mesh.insert_indices(Indices::U32(self.indices.clone()));

        bevy_mesh
    }

    pub fn prop_cube(half_extent: f32) -> Self {
        let h = half_extent;
        let p = [
            [-h, -h, h],
            [h, -h, h],
            [h, h, h],
            [-h, h, h],
            [-h, -h, -h],
            [h, -h, -h],
            [h, h, -h],
            [-h, h, -h],
        ];

        let faces = [
            (
                p[0],
                p[1],
                p[2],
                p[3],
                [0.0, 0.0, 1.0],
                [0.72, 0.52, 0.35, 1.0],
            ),
            (
                p[5],
                p[4],
                p[7],
                p[6],
                [0.0, 0.0, -1.0],
                [0.72, 0.52, 0.35, 1.0],
            ),
            (
                p[4],
                p[0],
                p[3],
                p[7],
                [-1.0, 0.0, 0.0],
                [0.65, 0.46, 0.30, 1.0],
            ),
            (
                p[1],
                p[5],
                p[6],
                p[2],
                [1.0, 0.0, 0.0],
                [0.65, 0.46, 0.30, 1.0],
            ),
            (
                p[3],
                p[2],
                p[6],
                p[7],
                [0.0, 1.0, 0.0],
                [0.80, 0.58, 0.40, 1.0],
            ),
            (
                p[4],
                p[5],
                p[1],
                p[0],
                [0.0, -1.0, 0.0],
                [0.55, 0.38, 0.24, 1.0],
            ),
        ];

        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);

        for (v0, v1, v2, v3, norm, col) in faces {
            let start = vertices.len() as u32;
            vertices.push(Vertex {
                position: v0,
                normal: norm,
                uv: [0.0, 1.0],
                color: col,
            });
            vertices.push(Vertex {
                position: v1,
                normal: norm,
                uv: [1.0, 1.0],
                color: col,
            });
            vertices.push(Vertex {
                position: v2,
                normal: norm,
                uv: [1.0, 0.0],
                color: col,
            });
            vertices.push(Vertex {
                position: v3,
                normal: norm,
                uv: [0.0, 0.0],
                color: col,
            });

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
        let gem_color = [0.1, 0.75, 1.0, 1.0];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let top = [0.0, half_h, 0.0];
        let bot = [0.0, -half_h, 0.0];
        let mid = [
            [radius, 0.0, 0.0],
            [0.0, 0.0, radius],
            [-radius, 0.0, 0.0],
            [0.0, 0.0, -radius],
        ];

        for i in 0..4 {
            let next = (i + 1) % 4;
            let p0 = mid[i];
            let p1 = mid[next];

            let v_ab = [p1[0] - top[0], p1[1] - top[1], p1[2] - top[2]];
            let v_ac = [p0[0] - top[0], p0[1] - top[1], p0[2] - top[2]];
            let nx = v_ab[1] * v_ac[2] - v_ab[2] * v_ac[1];
            let ny = v_ab[2] * v_ac[0] - v_ab[0] * v_ac[2];
            let nz = v_ab[0] * v_ac[1] - v_ab[1] * v_ac[0];
            let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-5);
            let norm = [nx / len, ny / len, nz / len];

            let base = vertices.len() as u32;
            vertices.push(Vertex {
                position: top,
                normal: norm,
                uv: [0.5, 0.0],
                color: gem_color,
            });
            vertices.push(Vertex {
                position: p0,
                normal: norm,
                uv: [0.0, 1.0],
                color: gem_color,
            });
            vertices.push(Vertex {
                position: p1,
                normal: norm,
                uv: [1.0, 1.0],
                color: gem_color,
            });

            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
        }

        for i in 0..4 {
            let next = (i + 1) % 4;
            let p0 = mid[i];
            let p1 = mid[next];

            let v_ab = [p0[0] - bot[0], p0[1] - bot[1], p0[2] - bot[2]];
            let v_ac = [p1[0] - bot[0], p1[1] - bot[1], p1[2] - bot[2]];
            let nx = v_ab[1] * v_ac[2] - v_ab[2] * v_ac[1];
            let ny = v_ab[2] * v_ac[0] - v_ab[0] * v_ac[2];
            let nz = v_ab[0] * v_ac[1] - v_ab[1] * v_ac[0];
            let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-5);
            let norm = [nx / len, ny / len, nz / len];

            let base = vertices.len() as u32;
            vertices.push(Vertex {
                position: bot,
                normal: norm,
                uv: [0.5, 1.0],
                color: gem_color,
            });
            vertices.push(Vertex {
                position: p1,
                normal: norm,
                uv: [1.0, 0.0],
                color: gem_color,
            });
            vertices.push(Vertex {
                position: p0,
                normal: norm,
                uv: [0.0, 0.0],
                color: gem_color,
            });

            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
        }

        Self::new("collectible_gem", vertices, indices)
    }

    pub fn portal_pedestal(
        inner_radius: f32,
        outer_radius: f32,
        height: f32,
        segments: u32,
    ) -> Self {
        let segs = segments.max(16);
        let stone_color = [0.35, 0.38, 0.45, 1.0];
        let rune_color = [0.2, 0.8, 1.0, 1.0];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for i in 0..segs {
            let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;

            let col = if i % 2 == 0 { rune_color } else { stone_color };

            let p0 = [a0.cos() * inner_radius, height, a0.sin() * inner_radius];
            let p1 = [a0.cos() * outer_radius, height, a0.sin() * outer_radius];
            let p2 = [a1.cos() * outer_radius, height, a1.sin() * outer_radius];
            let p3 = [a1.cos() * inner_radius, height, a1.sin() * inner_radius];

            let base = vertices.len() as u32;
            vertices.push(Vertex {
                position: p0,
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
                color: col,
            });
            vertices.push(Vertex {
                position: p1,
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 0.0],
                color: col,
            });
            vertices.push(Vertex {
                position: p2,
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 1.0],
                color: col,
            });
            vertices.push(Vertex {
                position: p3,
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 1.0],
                color: col,
            });

            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 3);
        }

        Self::new("portal_pedestal", vertices, indices)
    }

    pub fn synthetic_courtyard() -> Self {
        let floor_color = [0.45, 0.48, 0.42, 1.0];
        let step_color = [0.65, 0.62, 0.55, 1.0];
        let wall_color = [0.55, 0.50, 0.45, 1.0];

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let add_quad = |verts: &mut Vec<Vertex>,
                        inds: &mut Vec<u32>,
                        p0: [f32; 3],
                        p1: [f32; 3],
                        p2: [f32; 3],
                        p3: [f32; 3],
                        norm: [f32; 3],
                        col: [f32; 4]| {
            let base = verts.len() as u32;
            verts.push(Vertex {
                position: p0,
                normal: norm,
                uv: [0.0, 0.0],
                color: col,
            });
            verts.push(Vertex {
                position: p1,
                normal: norm,
                uv: [1.0, 0.0],
                color: col,
            });
            verts.push(Vertex {
                position: p2,
                normal: norm,
                uv: [1.0, 1.0],
                color: col,
            });
            verts.push(Vertex {
                position: p3,
                normal: norm,
                uv: [0.0, 1.0],
                color: col,
            });

            inds.push(base);
            inds.push(base + 1);
            inds.push(base + 2);
            inds.push(base);
            inds.push(base + 2);
            inds.push(base + 3);
        };

        // Main floor: 40x40 plane at y = 0
        add_quad(
            &mut vertices,
            &mut indices,
            [-20.0, 0.0, -20.0],
            [20.0, 0.0, -20.0],
            [20.0, 0.0, 20.0],
            [-20.0, 0.0, 20.0],
            [0.0, 1.0, 0.0],
            floor_color,
        );

        // Raised pavilion: 12x12 at y = 0.5
        add_quad(
            &mut vertices,
            &mut indices,
            [-6.0, 0.5, -6.0],
            [6.0, 0.5, -6.0],
            [6.0, 0.5, 6.0],
            [-6.0, 0.5, 6.0],
            [0.0, 1.0, 0.0],
            step_color,
        );

        // North wall
        add_quad(
            &mut vertices,
            &mut indices,
            [-20.0, 0.0, -20.0],
            [20.0, 0.0, -20.0],
            [20.0, 4.0, -20.0],
            [-20.0, 4.0, -20.0],
            [0.0, 0.0, 1.0],
            wall_color,
        );
        // South wall
        add_quad(
            &mut vertices,
            &mut indices,
            [20.0, 0.0, 20.0],
            [-20.0, 0.0, 20.0],
            [-20.0, 4.0, 20.0],
            [20.0, 4.0, 20.0],
            [0.0, 0.0, -1.0],
            wall_color,
        );
        // West wall
        add_quad(
            &mut vertices,
            &mut indices,
            [-20.0, 0.0, 20.0],
            [-20.0, 0.0, -20.0],
            [-20.0, 4.0, -20.0],
            [-20.0, 4.0, 20.0],
            [1.0, 0.0, 0.0],
            wall_color,
        );
        // East wall
        add_quad(
            &mut vertices,
            &mut indices,
            [20.0, 0.0, -20.0],
            [20.0, 0.0, 20.0],
            [20.0, 4.0, 20.0],
            [20.0, 4.0, -20.0],
            [-1.0, 0.0, 0.0],
            wall_color,
        );

        Self::new("synthetic_courtyard", vertices, indices)
    }

    pub fn skybox_cube(extent: f32) -> Self {
        let h = extent;
        let p = [
            [-h, -h, -h],
            [h, -h, -h],
            [h, h, -h],
            [-h, h, -h],
            [-h, -h, h],
            [h, -h, h],
            [h, h, h],
            [-h, h, h],
        ];

        let make_vert = |pos: [f32; 3]| Vertex {
            position: pos,
            normal: [0.0, 0.0, 0.0],
            uv: [0.0, 0.0],
            color: [0.4, 0.6, 0.9, 1.0],
        };

        let faces = [
            (p[0], p[1], p[2], p[3]),
            (p[5], p[4], p[7], p[6]),
            (p[4], p[0], p[3], p[7]),
            (p[1], p[5], p[6], p[2]),
            (p[3], p[2], p[6], p[7]),
            (p[4], p[5], p[1], p[0]),
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

pub fn igb_mesh_to_bevy(igb_mesh: &IgbMesh) -> BevyMesh {
    let mut bevy_mesh = BevyMesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    let positions: Vec<[f32; 3]> = igb_mesh.vertices.iter().map(|v| v.position).collect();
    let normals: Vec<[f32; 3]> = igb_mesh.vertices.iter().map(|v| v.normal).collect();
    let uvs: Vec<[f32; 2]> = igb_mesh.vertices.iter().map(|v| v.uv).collect();
    let colors: Vec<[f32; 4]> = igb_mesh.vertices.iter().map(|v| v.color).collect();

    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_POSITION, positions);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_NORMAL, normals);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_UV_0, uvs);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_COLOR, colors);
    bevy_mesh.insert_indices(Indices::U32(igb_mesh.indices.clone()));

    bevy_mesh
}

fn is_auxiliary_character_submesh(vertices: &[skylands_formats::igb::IgbVertex]) -> bool {
    if vertices.is_empty() {
        return true;
    }

    let mut min = vertices[0].position;
    let mut max = vertices[0].position;
    for v in vertices {
        min[0] = min[0].min(v.position[0]);
        min[1] = min[1].min(v.position[1]);
        min[2] = min[2].min(v.position[2]);

        max[0] = max[0].max(v.position[0]);
        max[1] = max[1].max(v.position[1]);
        max[2] = max[2].max(v.position[2]);
    }

    let dx = max[0] - min[0];
    let dy = max[1] - min[1];
    let dz = max[2] - min[2];
    let max_dim = dx.max(dy).max(dz);

    // Filter oversized outlier submeshes (e.g. 600-unit background dome / VFX card)
    if max_dim > 15.0 {
        return true;
    }

    // Filter horizontal ground decals, discs, rings, pedestals, and shadow plates
    // (e.g. Kaos ground rings dx/dz ~ 8.4-11.6, Snap Shot ground disc dx/dz ~ 14.68, dy <= 1.2)
    let horizontal_span = dx.max(dz);
    if horizontal_span > 2.5 && dy < 1.25 && (dy < 0.25 || horizontal_span / dy.max(0.01) > 3.0) {
        return true;
    }

    // Filter 2D billboard quads/cards: trivial vertex count (<= 16) with planar span > 1.5
    if vertices.len() <= 16 && (dy < 0.05 || dx < 0.05 || dz < 0.05) && max_dim > 1.5 {
        return true;
    }

    // Filter flat 2D billboard cards where one axis thickness is near zero while other axes > 1.8
    let min_dim = dx.min(dy).min(dz);
    let mid_dim = if (dx <= dy && dx >= dz) || (dx <= dz && dx >= dy) {
        dx
    } else if (dy <= dx && dy >= dz) || (dy <= dz && dy >= dx) {
        dy
    } else {
        dz
    };
    if min_dim < 0.08 && mid_dim > 1.8 {
        return true;
    }

    false
}

pub fn combine_or_normalize_model_meshes(
    meshes: &[IgbMesh],
    target_height: Option<f32>,
) -> BevyMesh {
    let mut non_empty: Vec<&IgbMesh> = meshes.iter().filter(|m| !m.vertices.is_empty()).collect();

    // If normalizing to character target_height, filter out billboards, ground rings, oversized meshes
    if target_height.is_some() {
        let filtered: Vec<&IgbMesh> = non_empty
            .iter()
            .copied()
            .filter(|m| !is_auxiliary_character_submesh(&m.vertices))
            .collect();
        if !filtered.is_empty() {
            non_empty = filtered;
        }
    }

    if non_empty.is_empty() {
        return Mesh::prop_cube(0.5).to_bevy();
    }

    let mut total_vert_count = 0;
    let mut total_idx_count = 0;
    for m in &non_empty {
        total_vert_count += m.vertices.len();
        total_idx_count += m.indices.len();
    }

    let mut combined_vertices = Vec::with_capacity(total_vert_count);
    let mut combined_indices = Vec::with_capacity(total_idx_count);

    let mut vert_offset = 0u32;
    for m in &non_empty {
        combined_vertices.extend(m.vertices.iter().cloned());
        if m.indices.is_empty() {
            for idx in 0..m.vertices.len() as u32 {
                combined_indices.push(vert_offset + idx);
            }
        } else {
            for &idx in &m.indices {
                combined_indices.push(vert_offset + idx);
            }
        }
        vert_offset += m.vertices.len() as u32;
    }

    let mut min = combined_vertices[0].position;
    let mut max = combined_vertices[0].position;

    for v in &combined_vertices {
        min[0] = min[0].min(v.position[0]);
        min[1] = min[1].min(v.position[1]);
        min[2] = min[2].min(v.position[2]);

        max[0] = max[0].max(v.position[0]);
        max[1] = max[1].max(v.position[1]);
        max[2] = max[2].max(v.position[2]);
    }

    let scale = if let Some(th) = target_height {
        let orig_height = max[1] - min[1];
        if orig_height > 1e-4 {
            th / orig_height
        } else {
            1.0
        }
    } else {
        1.0
    };

    let center_x = (min[0] + max[0]) * 0.5;
    let center_z = (min[2] + max[2]) * 0.5;
    let min_y = min[1];

    let mut bevy_mesh = BevyMesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    let positions: Vec<[f32; 3]> = combined_vertices
        .iter()
        .map(|v| {
            if target_height.is_some() {
                [
                    (v.position[0] - center_x) * scale,
                    (v.position[1] - min_y) * scale,
                    (v.position[2] - center_z) * scale,
                ]
            } else {
                v.position
            }
        })
        .collect();

    let normals: Vec<[f32; 3]> = combined_vertices.iter().map(|v| v.normal).collect();
    let uvs: Vec<[f32; 2]> = combined_vertices.iter().map(|v| v.uv).collect();
    let colors: Vec<[f32; 4]> = combined_vertices.iter().map(|v| v.color).collect();

    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_POSITION, positions);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_NORMAL, normals);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_UV_0, uvs);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_COLOR, colors);
    bevy_mesh.insert_indices(Indices::U32(combined_indices));

    bevy_mesh
}

pub fn combine_scene_instances_to_bevy(
    model: &IgbModel,
    instances: &[skylands_formats::igb::SceneInstance],
) -> BevyMesh {
    let mut total_vert_count = 0;
    let mut total_idx_count = 0;

    for inst in instances {
        if let Some(geo) = model.geometries.get(inst.geometry_index) {
            if let Some(ref m) = geo.mesh {
                total_vert_count += m.vertices.len();
                total_idx_count += m.indices.len();
            }
        }
    }

    if total_vert_count == 0 {
        return Mesh::prop_cube(0.5).to_bevy();
    }

    let mut positions = Vec::with_capacity(total_vert_count);
    let mut normals = Vec::with_capacity(total_vert_count);
    let mut uvs = Vec::with_capacity(total_vert_count);
    let mut colors = Vec::with_capacity(total_vert_count);
    let mut indices = Vec::with_capacity(total_idx_count);

    let mut vert_offset = 0u32;
    for inst in instances {
        if let Some(geo) = model.geometries.get(inst.geometry_index) {
            if let Some(ref m) = geo.mesh {
                if m.vertices.is_empty() {
                    continue;
                }
                for v in &m.vertices {
                    positions.push(transform_point(&inst.world_transform, v.position));
                    normals.push(v.normal);
                    uvs.push(v.uv);
                    colors.push(v.color);
                }
                for &idx in &m.indices {
                    indices.push(vert_offset + idx);
                }
                vert_offset += m.vertices.len() as u32;
            }
        }
    }

    let mut bevy_mesh = BevyMesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_POSITION, positions);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_NORMAL, normals);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_UV_0, uvs);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_COLOR, colors);
    bevy_mesh.insert_indices(Indices::U32(indices));
    bevy_mesh
}

pub fn igb_model_to_bevy_mesh(model: &IgbModel, target_height: Option<f32>) -> BevyMesh {
    let instances = model.evaluate_scene_graph();

    let valid_instances: Vec<&skylands_formats::igb::SceneInstance> = if target_height.is_some() {
        let mut filtered = Vec::new();
        for inst in &instances {
            if let Some(geo) = model.geometries.get(inst.geometry_index) {
                if let Some(ref m) = geo.mesh {
                    if !is_auxiliary_character_submesh(&m.vertices) {
                        filtered.push(inst);
                    }
                }
            }
        }
        if filtered.is_empty() {
            instances.iter().collect()
        } else {
            filtered
        }
    } else {
        instances.iter().collect()
    };

    let mut total_vert_count = 0;
    let mut total_idx_count = 0;

    if !valid_instances.is_empty() && !model.geometries.is_empty() {
        for inst in &valid_instances {
            if let Some(geo) = model.geometries.get(inst.geometry_index) {
                if let Some(ref m) = geo.mesh {
                    total_vert_count += m.vertices.len();
                    total_idx_count += m.indices.len();
                }
            }
        }
    }

    if total_vert_count == 0 {
        return combine_or_normalize_model_meshes(&model.meshes, target_height);
    }

    let mut combined_vertices = Vec::with_capacity(total_vert_count);
    let mut combined_indices = Vec::with_capacity(total_idx_count);
    let mut vert_offset = 0u32;

    for inst in &valid_instances {
        if let Some(geo) = model.geometries.get(inst.geometry_index) {
            if let Some(ref m) = geo.mesh {
                if m.vertices.is_empty() {
                    continue;
                }
                for v in &m.vertices {
                    let mut transformed = *v;
                    transformed.position = transform_point(&inst.world_transform, v.position);
                    combined_vertices.push(transformed);
                }
                if m.indices.is_empty() {
                    for idx in 0..m.vertices.len() as u32 {
                        combined_indices.push(vert_offset + idx);
                    }
                } else {
                    for &idx in &m.indices {
                        combined_indices.push(vert_offset + idx);
                    }
                }
                vert_offset += m.vertices.len() as u32;
            }
        }
    }

    let mut min = combined_vertices[0].position;
    let mut max = combined_vertices[0].position;

    for v in &combined_vertices {
        min[0] = min[0].min(v.position[0]);
        min[1] = min[1].min(v.position[1]);
        min[2] = min[2].min(v.position[2]);

        max[0] = max[0].max(v.position[0]);
        max[1] = max[1].max(v.position[1]);
        max[2] = max[2].max(v.position[2]);
    }

    let scale = if let Some(th) = target_height {
        let orig_height = max[1] - min[1];
        if orig_height > 1e-4 {
            th / orig_height
        } else {
            1.0
        }
    } else {
        1.0
    };

    let center_x = (min[0] + max[0]) * 0.5;
    let center_z = (min[2] + max[2]) * 0.5;
    let min_y = min[1];

    let mut bevy_mesh = BevyMesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    let positions: Vec<[f32; 3]> = combined_vertices
        .iter()
        .map(|v| {
            if target_height.is_some() {
                [
                    (v.position[0] - center_x) * scale,
                    (v.position[1] - min_y) * scale,
                    (v.position[2] - center_z) * scale,
                ]
            } else {
                v.position
            }
        })
        .collect();

    let normals: Vec<[f32; 3]> = combined_vertices.iter().map(|v| v.normal).collect();
    let uvs: Vec<[f32; 2]> = combined_vertices.iter().map(|v| v.uv).collect();
    let colors: Vec<[f32; 4]> = combined_vertices.iter().map(|v| v.color).collect();

    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_POSITION, positions);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_NORMAL, normals);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_UV_0, uvs);
    bevy_mesh.insert_attribute(BevyMesh::ATTRIBUTE_COLOR, colors);
    bevy_mesh.insert_indices(Indices::U32(combined_indices));

    bevy_mesh
}

pub fn normalize_model_mesh(mesh: &IgbMesh, target_height: f32) -> BevyMesh {
    combine_or_normalize_model_meshes(std::slice::from_ref(mesh), Some(target_height))
}

pub fn igb_model_to_bevy_meshes(model: &IgbModel) -> Vec<BevyMesh> {
    let mut out = Vec::new();
    for m in &model.meshes {
        if !m.vertices.is_empty() && !m.indices.is_empty() {
            out.push(igb_mesh_to_bevy(m));
        }
    }
    if out.is_empty() {
        // Fallback to procedural cube if empty
        out.push(Mesh::prop_cube(0.5).to_bevy());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use skylands_formats::igb::IgbVertex;

    #[test]
    fn test_mesh_to_bevy() {
        let prop = Mesh::prop_cube(1.0);
        let bevy_mesh = prop.to_bevy();
        assert!(bevy_mesh.attribute(BevyMesh::ATTRIBUTE_POSITION).is_some());
        assert!(bevy_mesh.attribute(BevyMesh::ATTRIBUTE_NORMAL).is_some());
        assert!(bevy_mesh.attribute(BevyMesh::ATTRIBUTE_UV_0).is_some());
        assert!(bevy_mesh.attribute(BevyMesh::ATTRIBUTE_COLOR).is_some());
        assert!(bevy_mesh.indices().is_some());
    }

    #[test]
    fn test_igb_mesh_to_bevy() {
        let igb = IgbMesh {
            name: "test".to_string(),
            vertices: vec![
                IgbVertex {
                    position: [0.0, 1.0, 2.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.5, 0.5],
                    color: [1.0, 0.5, 0.25, 1.0],
                    ..Default::default()
                },
                IgbVertex {
                    position: [1.0, 0.0, 2.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [1.0, 0.5],
                    color: [1.0, 0.5, 0.25, 1.0],
                    ..Default::default()
                },
                IgbVertex {
                    position: [0.0, 0.0, 1.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.5],
                    color: [1.0, 0.5, 0.25, 1.0],
                    ..Default::default()
                },
            ],
            indices: vec![0, 1, 2],
            material_name: None,
        };

        let bevy_mesh = igb_mesh_to_bevy(&igb);
        assert_eq!(bevy_mesh.count_vertices(), 3);
        assert_eq!(bevy_mesh.indices().unwrap().len(), 3);
    }

    #[test]
    fn test_normalize_model_mesh() {
        let igb = IgbMesh {
            name: "tall_model".to_string(),
            vertices: vec![
                IgbVertex {
                    position: [10.0, 5.0, 10.0],
                    ..Default::default()
                },
                IgbVertex {
                    position: [20.0, 50.0, 20.0],
                    ..Default::default()
                },
            ],
            indices: vec![0, 1],
            material_name: None,
        };

        let normalized = normalize_model_mesh(&igb, 1.8);
        let pos = normalized.attribute(BevyMesh::ATTRIBUTE_POSITION).unwrap();
        if let bevy::render::mesh::VertexAttributeValues::Float32x3(pts) = pos {
            assert_eq!(pts[0][1], 0.0);
            assert!((pts[1][1] - 1.8).abs() < 1e-4);
            assert!((pts[0][0] + pts[1][0]).abs() < 1e-4);
            assert!((pts[0][2] + pts[1][2]).abs() < 1e-4);
        } else {
            panic!("expected Float32x3");
        }
    }

    #[test]
    fn test_procedural_primitives() {
        let coin = Mesh::collectible_coin(0.25, 0.05, 16).to_bevy();
        assert!(coin.count_vertices() > 0);

        let gem = Mesh::collectible_gem(0.2, 0.4).to_bevy();
        assert!(gem.count_vertices() > 0);

        let pedestal = Mesh::portal_pedestal(0.0, 1.8, 0.25, 24).to_bevy();
        assert!(pedestal.count_vertices() > 0);

        let courtyard = Mesh::synthetic_courtyard().to_bevy();
        assert!(courtyard.count_vertices() > 0);

        let skybox = Mesh::skybox_cube(50.0).to_bevy();
        assert!(skybox.count_vertices() > 0);
    }

    #[test]
    fn test_spyro_bld_to_normalized_bevy_mesh() {
        let p1 = std::path::Path::new("assets/files/character/016_Spyro.bld");
        let p2 = std::path::Path::new("../../assets/files/character/016_Spyro.bld");
        let path = if p1.exists() {
            Some(p1.to_path_buf())
        } else if p2.exists() {
            Some(p2.to_path_buf())
        } else {
            None
        };

        if let Some(p) = path {
            let bytes = std::fs::read(p).unwrap();
            let model = IgbModel::parse(std::io::Cursor::new(&bytes)).expect("parse spyro igb");
            let mesh = model
                .meshes
                .iter()
                .find(|m| !m.vertices.is_empty())
                .expect("mesh found");

            let target_height = 1.8f32;
            let bevy_mesh = normalize_model_mesh(mesh, target_height);
            assert_eq!(
                bevy_mesh.primitive_topology(),
                PrimitiveTopology::TriangleList
            );

            let pos = bevy_mesh.attribute(BevyMesh::ATTRIBUTE_POSITION).unwrap();
            if let bevy::render::mesh::VertexAttributeValues::Float32x3(pts) = pos {
                assert_eq!(pts.len(), mesh.vertices.len());
                let min_y = pts.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
                let max_y = pts.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    (min_y - 0.0).abs() < 1e-4,
                    "Base of Spyro should be grounded at y=0"
                );
                assert!(
                    (max_y - target_height).abs() < 1e-3,
                    "Top of Spyro should match target_height"
                );
            } else {
                panic!("Expected Float32x3 positions");
            }

            assert!(bevy_mesh.attribute(BevyMesh::ATTRIBUTE_NORMAL).is_some());
            assert!(bevy_mesh.attribute(BevyMesh::ATTRIBUTE_UV_0).is_some());
            assert!(bevy_mesh.indices().is_some());
        }
    }

    #[test]
    fn test_multi_submesh_combine_and_normalize() {
        let sub1 = IgbMesh {
            name: "body".to_string(),
            vertices: vec![
                IgbVertex {
                    position: [-1.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    ..Default::default()
                },
                IgbVertex {
                    position: [1.0, 2.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [1.0, 1.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    ..Default::default()
                },
            ],
            indices: vec![0, 1],
            material_name: None,
        };
        let sub2 = IgbMesh {
            name: "hat".to_string(),
            vertices: vec![
                IgbVertex {
                    position: [0.0, 2.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.5, 0.5],
                    color: [1.0, 0.0, 0.0, 1.0],
                    ..Default::default()
                },
                IgbVertex {
                    position: [0.0, 4.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.5, 1.0],
                    color: [1.0, 0.0, 0.0, 1.0],
                    ..Default::default()
                },
            ],
            indices: vec![0, 1],
            material_name: None,
        };

        let combined = combine_or_normalize_model_meshes(&[sub1, sub2], Some(2.0));
        assert_eq!(combined.count_vertices(), 4);
        let indices = combined.indices().unwrap();
        assert_eq!(indices.len(), 4);
        let idx_vec: Vec<u32> = match indices {
            Indices::U32(v) => v.clone(),
            _ => panic!("expected u32 indices"),
        };
        assert_eq!(idx_vec, vec![0, 1, 2, 3]);

        let pos = combined.attribute(BevyMesh::ATTRIBUTE_POSITION).unwrap();
        if let bevy::render::mesh::VertexAttributeValues::Float32x3(pts) = pos {
            let min_y = pts.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
            let max_y = pts.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
            assert!((min_y - 0.0).abs() < 1e-4);
            assert!((max_y - 2.0).abs() < 1e-4);
        } else {
            panic!("expected Float32x3");
        }
    }

    #[test]
    fn test_character_model_submesh_filtering() {
        for char_file in [
            "assets/files/character/016_Spyro.bld",
            "assets/files/character/462_Water_Crocky.bld",
            "assets/files/character/476_Artichoke.bld",
            "assets/files/character/459_Barbarian.bld",
            "assets/files/character/1010_Kaos.bld",
        ] {
            let p1 = std::path::Path::new(char_file);
            let p2 = std::path::Path::new("../../").join(char_file);
            let path = if p1.exists() {
                Some(p1.to_path_buf())
            } else if p2.exists() {
                Some(p2)
            } else {
                None
            };

            let Some(p) = path else { continue };
            let bytes = std::fs::read(p).unwrap();
            let model = IgbModel::parse(std::io::Cursor::new(&bytes)).expect("parse character bld");

            let target_height = 1.8f32;
            let bevy_mesh = igb_model_to_bevy_mesh(&model, Some(target_height));
            assert_eq!(
                bevy_mesh.primitive_topology(),
                PrimitiveTopology::TriangleList
            );
            assert!(bevy_mesh.count_vertices() >= 3);

            let pos = bevy_mesh.attribute(BevyMesh::ATTRIBUTE_POSITION).unwrap();
            if let bevy::render::mesh::VertexAttributeValues::Float32x3(pts) = pos {
                let min_y = pts.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
                let max_y = pts.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    (min_y - 0.0).abs() < 1e-4,
                    "{char_file} base should be grounded at y=0"
                );
                assert!(
                    (max_y - target_height).abs() < 1e-3,
                    "{char_file} top height should match target_height"
                );
            } else {
                panic!("Expected Float32x3 positions");
            }
        }
    }
}
