use skylands_formats::igb::IgbModel;
use skylands_formats::CollisionPrimitive;
use std::collections::HashMap;

// ponytail: basic uniform XZ grid cell size. Upgrade to BVH if static mesh topology warrants hierarchy.
const GRID_CELL_SIZE: f32 = 4.0;

#[inline]
pub fn vec3_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn vec3_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub fn vec3_scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
pub fn vec3_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn vec3_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub fn vec3_len_sq(a: [f32; 3]) -> f32 {
    vec3_dot(a, a)
}

#[inline]
pub fn vec3_len(a: [f32; 3]) -> f32 {
    vec3_len_sq(a).sqrt()
}

#[inline]
pub fn vec3_normalize(a: [f32; 3]) -> [f32; 3] {
    let len = vec3_len(a);
    if len > 1e-6 {
        vec3_scale(a, 1.0 / len)
    } else {
        [0.0, 0.0, 0.0]
    }
}

pub fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    let w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
    let inv_w = if w.abs() > 1e-6 { 1.0 / w } else { 1.0 };
    [
        (m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12]) * inv_w,
        (m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13]) * inv_w,
        (m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14]) * inv_w,
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn empty() -> Self {
        Self {
            min: [f32::INFINITY, f32::INFINITY, f32::INFINITY],
            max: [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY],
        }
    }

    pub fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }

    pub fn extend(&mut self, p: [f32; 3]) {
        self.min[0] = self.min[0].min(p[0]);
        self.min[1] = self.min[1].min(p[1]);
        self.min[2] = self.min[2].min(p[2]);

        self.max[0] = self.max[0].max(p[0]);
        self.max[1] = self.max[1].max(p[1]);
        self.max[2] = self.max[2].max(p[2]);
    }

    pub fn contains_2d(&self, x: f32, z: f32) -> bool {
        x >= self.min[0] && x <= self.max[0] && z >= self.min[2] && z <= self.max[2]
    }

    pub fn intersects_sphere(&self, center: [f32; 3], radius: f32) -> bool {
        let mut d_min_sq = 0.0f32;
        for (i, &c) in center.iter().enumerate() {
            if c < self.min[i] {
                let d = self.min[i] - c;
                d_min_sq += d * d;
            } else if c > self.max[i] {
                let d = c - self.max[i];
                d_min_sq += d * d;
            }
        }
        d_min_sq <= radius * radius
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub v0: [f32; 3],
    pub v1: [f32; 3],
    pub v2: [f32; 3],
    pub normal: [f32; 3],
    pub aabb: Aabb,
}

impl Triangle {
    pub fn is_valid(&self) -> bool {
        self.v0[0].is_finite()
            && self.v0[1].is_finite()
            && self.v0[2].is_finite()
            && self.v1[0].is_finite()
            && self.v1[1].is_finite()
            && self.v1[2].is_finite()
            && self.v2[0].is_finite()
            && self.v2[1].is_finite()
            && self.v2[2].is_finite()
            && (self.normal[0] != 0.0 || self.normal[1] != 0.0 || self.normal[2] != 0.0)
            && self.normal[0].is_finite()
            && self.normal[1].is_finite()
            && self.normal[2].is_finite()
            && self.v0[0].abs() < 10000.0
            && self.v0[1].abs() < 10000.0
            && self.v0[2].abs() < 10000.0
            && self.v1[0].abs() < 10000.0
            && self.v1[1].abs() < 10000.0
            && self.v1[2].abs() < 10000.0
            && self.v2[0].abs() < 10000.0
            && self.v2[1].abs() < 10000.0
            && self.v2[2].abs() < 10000.0
    }

    pub fn new(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> Self {
        let edge1 = vec3_sub(v1, v0);
        let edge2 = vec3_sub(v2, v0);
        let normal = vec3_normalize(vec3_cross(edge1, edge2));

        let mut aabb = Aabb::empty();
        aabb.extend(v0);
        aabb.extend(v1);
        aabb.extend(v2);

        Self {
            v0,
            v1,
            v2,
            normal,
            aabb,
        }
    }

    pub fn raycast_vertical(
        &self,
        x: f32,
        z: f32,
        y_start: f32,
        max_dist: f32,
    ) -> Option<(f32, [f32; 3])> {
        // Only upward-sloping or horizontal surfaces support standing
        if self.normal[1] < 0.1 {
            return None;
        }

        // Expanded 2D AABB check
        if x < self.aabb.min[0] - 1e-4
            || x > self.aabb.max[0] + 1e-4
            || z < self.aabb.min[2] - 1e-4
            || z > self.aabb.max[2] + 1e-4
        {
            return None;
        }

        let e0x = self.v0[0] - self.v2[0];
        let e0z = self.v0[2] - self.v2[2];
        let e1x = self.v1[0] - self.v2[0];
        let e1z = self.v1[2] - self.v2[2];

        let det = e0x * e1z - e0z * e1x;
        if det.abs() < 1e-7 {
            return None;
        }

        let dx = x - self.v2[0];
        let dz = z - self.v2[2];

        let w0 = (dx * e1z - dz * e1x) / det;
        let w1 = (e0x * dz - e0z * dx) / det;
        let w2 = 1.0 - w0 - w1;

        const EPS: f32 = -1e-3;
        if w0 < EPS || w1 < EPS || w2 < EPS {
            return None;
        }

        // Plane intersection for y
        let y = self.v0[1]
            - (self.normal[0] * (x - self.v0[0]) + self.normal[2] * (z - self.v0[2]))
                / self.normal[1];

        if y <= y_start + 0.1 && (y_start - y) <= max_dist {
            Some((y, self.normal))
        } else {
            None
        }
    }

    pub fn closest_point(&self, p: [f32; 3]) -> [f32; 3] {
        let ab = vec3_sub(self.v1, self.v0);
        let ac = vec3_sub(self.v2, self.v0);
        let ap = vec3_sub(p, self.v0);

        let d1 = vec3_dot(ab, ap);
        let d2 = vec3_dot(ac, ap);
        if d1 <= 0.0 && d2 <= 0.0 {
            return self.v0;
        }

        let bp = vec3_sub(p, self.v1);
        let d3 = vec3_dot(ab, bp);
        let d4 = vec3_dot(ac, bp);
        if d3 >= 0.0 && d4 <= d3 {
            return self.v1;
        }

        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            let v = d1 / (d1 - d3);
            return vec3_add(self.v0, vec3_scale(ab, v));
        }

        let cp = vec3_sub(p, self.v2);
        let d5 = vec3_dot(ab, cp);
        let d6 = vec3_dot(ac, cp);
        if d6 >= 0.0 && d5 <= d6 {
            return self.v2;
        }

        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            let w = d2 / (d2 - d6);
            return vec3_add(self.v0, vec3_scale(ac, w));
        }

        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
            return vec3_add(self.v1, vec3_scale(vec3_sub(self.v2, self.v1), w));
        }

        let denom = 1.0 / (va + vb + vc);
        let v = vb * denom;
        let w = vc * denom;
        vec3_add(self.v0, vec3_add(vec3_scale(ab, v), vec3_scale(ac, w)))
    }

    pub fn resolve_sphere(&self, center: [f32; 3], radius: f32) -> Option<[f32; 3]> {
        if !self.aabb.intersects_sphere(center, radius) {
            return None;
        }

        let closest = self.closest_point(center);
        let diff = vec3_sub(center, closest);
        let dist_sq = vec3_len_sq(diff);

        if dist_sq < radius * radius {
            let dist = dist_sq.sqrt();
            let (dir, pen) = if dist > 1e-6 {
                (vec3_scale(diff, 1.0 / dist), radius - dist)
            } else {
                (self.normal, radius)
            };
            Some(vec3_scale(dir, pen))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollisionMesh {
    pub triangles: Vec<Triangle>,
    pub bounds: Aabb,
    pub primitives: Vec<CollisionPrimitive>,
    spatial_grid: HashMap<(i32, i32), Vec<usize>>,
}

impl CollisionMesh {
    pub fn empty() -> Self {
        Self {
            triangles: Vec::new(),
            bounds: Aabb::empty(),
            primitives: Vec::new(),
            spatial_grid: HashMap::new(),
        }
    }

    pub fn from_triangles(triangles: Vec<Triangle>) -> Self {
        let mut bounds = Aabb::empty();
        let mut spatial_grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();

        for (idx, t) in triangles.iter().enumerate() {
            bounds.extend(t.aabb.min);
            bounds.extend(t.aabb.max);

            let min_gx = (t.aabb.min[0] / GRID_CELL_SIZE).floor() as i32;
            let max_gx = (t.aabb.max[0] / GRID_CELL_SIZE).floor() as i32;
            let min_gz = (t.aabb.min[2] / GRID_CELL_SIZE).floor() as i32;
            let max_gz = (t.aabb.max[2] / GRID_CELL_SIZE).floor() as i32;

            for gx in min_gx..=max_gx {
                for gz in min_gz..=max_gz {
                    spatial_grid.entry((gx, gz)).or_default().push(idx);
                }
            }
        }
        Self {
            triangles,
            bounds,
            primitives: Vec::new(),
            spatial_grid,
        }
    }

    pub fn from_vertices_and_indices(
        vertices: &[[f32; 3]],
        indices: &[u32],
        transform: Option<&[f32; 16]>,
    ) -> Self {
        let mut triangles = Vec::with_capacity(indices.len() / 3);
        for chunk in indices.as_chunks::<3>().0 {
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            if i0 < vertices.len() && i1 < vertices.len() && i2 < vertices.len() {
                let (v0, v1, v2) = if let Some(m) = transform {
                    (
                        transform_point(m, vertices[i0]),
                        transform_point(m, vertices[i1]),
                        transform_point(m, vertices[i2]),
                    )
                } else {
                    (vertices[i0], vertices[i1], vertices[i2])
                };
                let tri = Triangle::new(v0, v1, v2);
                if tri.is_valid() {
                    triangles.push(tri);
                }
            }
        }
        Self::from_triangles(triangles)
    }

    pub fn from_igb(igb: &IgbModel) -> Self {
        Self::from_igb_filtered(igb, true)
    }

    pub fn from_igb_filtered(igb: &IgbModel, filter_prefixes: bool) -> Self {
        let instances = igb.evaluate_scene_graph();
        let mut triangles = Vec::new();

        // 1. Incorporate dedicated analytical collision primitives (boxes, props, obstacles)
        for prim in &igb.collision_primitives {
            if prim.is_solid || prim.is_actor_surface {
                for tri in prim.to_world_triangles() {
                    let t = Triangle::new(tri[0], tri[1], tri[2]);
                    if t.is_valid() {
                        triangles.push(t);
                    }
                }
            }
        }

        // 2. Filter geometry instances
        let has_col_or_trig = if filter_prefixes {
            let in_geos = igb.geometries.iter().any(|g| {
                let lower = g.name.to_ascii_lowercase();
                lower.starts_with("col_") || lower.starts_with("trig_")
            });
            let in_meshes = igb.meshes.iter().any(|m| {
                let lower = m.name.to_ascii_lowercase();
                lower.starts_with("col_") || lower.starts_with("trig_")
            });
            in_geos || in_meshes
        } else {
            false
        };

        let should_include = |name: &str| -> bool {
            if !filter_prefixes {
                return true;
            }
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("vis_") {
                return false;
            }
            if has_col_or_trig {
                lower.starts_with("col_") || lower.starts_with("trig_")
            } else {
                true
            }
        };

        if !igb.geometries.is_empty() {
            for inst in instances {
                if let Some(geo) = igb.geometries.get(inst.geometry_index) {
                    if !should_include(&geo.name) {
                        continue;
                    }
                    if let Some(mesh) = &geo.mesh {
                        // When no explicit col_/trig_ markers exist, discard 2D vertical billboard cards (foliage/decals)
                        if !has_col_or_trig && filter_prefixes && mesh.vertices.len() <= 4 {
                            let mut min_p = [f32::MAX; 3];
                            let mut max_p = [f32::MIN; 3];
                            for v in &mesh.vertices {
                                for c in 0..3 {
                                    min_p[c] = min_p[c].min(v.position[c]);
                                    max_p[c] = max_p[c].max(v.position[c]);
                                }
                            }
                            let dx = max_p[0] - min_p[0];
                            let dy = max_p[1] - min_p[1];
                            let dz = max_p[2] - min_p[2];
                            // Vertical billboard quad (standing upright with negligible thickness)
                            if (dx < 0.05 || dz < 0.05) && dy > 1.0 {
                                continue;
                            }
                        }

                        let verts: Vec<[f32; 3]> =
                            mesh.vertices.iter().map(|v| v.position).collect();
                        let sub = Self::from_vertices_and_indices(
                            &verts,
                            &mesh.indices,
                            Some(&inst.world_transform),
                        );
                        triangles.extend(sub.triangles);
                    }
                }
            }
        } else {
            for mesh in &igb.meshes {
                if !should_include(&mesh.name) {
                    continue;
                }
                let verts: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
                let sub = Self::from_vertices_and_indices(&verts, &mesh.indices, None);
                triangles.extend(sub.triangles);
            }
        }

        let mut res = Self::from_triangles(triangles);
        res.primitives = igb.collision_primitives.clone();
        res
    }

    pub fn synthetic_courtyard() -> Self {
        // Ground plane: 40x40 courtyard quad at y = 0.0 (upward normal +Y)
        let g0 = [-20.0, 0.0, -20.0];
        let g1 = [20.0, 0.0, -20.0];
        let g2 = [20.0, 0.0, 20.0];
        let g3 = [-20.0, 0.0, 20.0];

        let mut triangles = vec![Triangle::new(g0, g2, g1), Triangle::new(g0, g3, g2)];

        // 4 surrounding boundary walls (height 3.0, normals point inward)
        let wall_height = 3.0;
        let wall_segments = [
            // North wall (faces +Z)
            ([-20.0, 0.0, -20.0], [20.0, 0.0, -20.0]),
            // South wall (faces -Z)
            ([20.0, 0.0, 20.0], [-20.0, 0.0, 20.0]),
            // East wall (faces -X)
            ([20.0, 0.0, -20.0], [20.0, 0.0, 20.0]),
            // West wall (faces +X)
            ([-20.0, 0.0, 20.0], [-20.0, 0.0, -20.0]),
        ];

        for (p0, p1) in wall_segments {
            let p0_top = [p0[0], wall_height, p0[2]];
            let p1_top = [p1[0], wall_height, p1[2]];

            triangles.push(Triangle::new(p0, p1, p1_top));
            triangles.push(Triangle::new(p0, p1_top, p0_top));
        }

        Self::from_triangles(triangles)
    }

    pub fn raycast_down(&self, origin: [f32; 3], max_dist: f32) -> Option<(f32, [f32; 3])> {
        let (x, z) = (origin[0], origin[2]);
        let gx = (x / GRID_CELL_SIZE).floor() as i32;
        let gz = (z / GRID_CELL_SIZE).floor() as i32;

        let mut best_hit = None;
        let mut highest_y = f32::NEG_INFINITY;

        if let Some(indices) = self.spatial_grid.get(&(gx, gz)) {
            for &idx in indices {
                let tri = &self.triangles[idx];
                if let Some((y, normal)) = tri.raycast_vertical(x, z, origin[1], max_dist) {
                    if y > highest_y {
                        highest_y = y;
                        best_hit = Some((y, normal));
                    }
                }
            }
        }

        best_hit
    }

    pub fn resolve_sphere(&self, center: [f32; 3], radius: f32) -> Option<[f32; 3]> {
        if !self.bounds.intersects_sphere(center, radius) {
            return None;
        }

        let mut total_push = [0.0f32; 3];
        let mut collided = false;

        let min_gx = ((center[0] - radius) / GRID_CELL_SIZE).floor() as i32;
        let max_gx = ((center[0] + radius) / GRID_CELL_SIZE).floor() as i32;
        let min_gz = ((center[2] - radius) / GRID_CELL_SIZE).floor() as i32;
        let max_gz = ((center[2] + radius) / GRID_CELL_SIZE).floor() as i32;

        let mut tested = std::collections::HashSet::new();

        for gx in min_gx..=max_gx {
            for gz in min_gz..=max_gz {
                if let Some(indices) = self.spatial_grid.get(&(gx, gz)) {
                    for &idx in indices {
                        if tested.insert(idx) {
                            let tri = &self.triangles[idx];
                            // Wall & obstacle collision (steep surfaces: |normal.y| < 0.7)
                            if tri.normal[1].abs() < 0.7 {
                                if let Some(push) = tri.resolve_sphere(center, radius) {
                                    total_push[0] += push[0];
                                    total_push[2] += push[2];
                                    collided = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        if collided {
            Some(total_push)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterCollider {
    pub radius: f32,
    pub height: f32,
    pub step_height: f32,
    pub grounded: bool,
}

impl Default for CharacterCollider {
    fn default() -> Self {
        Self {
            radius: 0.5,
            height: 1.8,
            step_height: 0.35,
            grounded: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Environment {
    pub mesh: CollisionMesh,
    pub gravity: f32,
    pub kill_plane_y: f32,
    pub spawn_point: [f32; 3],
    pub jump_impulse: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            mesh: CollisionMesh::synthetic_courtyard(),
            gravity: -24.0,
            kill_plane_y: -15.0,
            spawn_point: [0.0, 0.0, 0.0],
            jump_impulse: 9.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangle_vertical_raycast() {
        let tri = Triangle::new([-10.0, 0.0, -10.0], [0.0, 0.0, 10.0], [10.0, 0.0, -10.0]);

        let hit = tri.raycast_vertical(0.0, 0.0, 5.0, 10.0);
        assert!(hit.is_some());
        let (y, norm) = hit.unwrap();
        assert!((y - 0.0).abs() < 1e-4);
        assert!((norm[1] - 1.0).abs() < 1e-4);

        // Outside triangle
        let outside = tri.raycast_vertical(20.0, 20.0, 5.0, 10.0);
        assert!(outside.is_none());
    }

    #[test]
    fn test_synthetic_courtyard_raycast_and_walls() {
        let env = CollisionMesh::synthetic_courtyard();
        assert_eq!(env.triangles.len(), 10);

        // Ground raycast in center
        let hit = env.raycast_down([0.0, 2.0, 0.0], 5.0);
        assert!(hit.is_some());
        let (y, _) = hit.unwrap();
        assert!((y - 0.0).abs() < 1e-4);

        // Sphere pushout against north wall at z = -20
        let push = env.resolve_sphere([0.0, 1.0, -19.8], 0.5);
        assert!(push.is_some());
        let p = push.unwrap();
        assert!(p[2] > 0.0, "Push must be southward (+Z)");

        // No collision in open space
        let none = env.resolve_sphere([0.0, 1.0, 0.0], 0.5);
        assert!(none.is_none());
    }

    #[test]
    fn test_collision_mesh_prefix_filtering() {
        use skylands_formats::igb::{IgbGeometryNode, IgbMesh, IgbVertex};

        let mut igb = IgbModel {
            header: skylands_formats::igb::IgbHeader {
                magic: *skylands_formats::igb::IGB_MAGIC,
                version: 8,
                object_count: 3,
                string_table_offset: 0,
                endian: skylands_formats::igb::IgbEndian::Little,
            },
            directory: Vec::new(),
            nodes: Vec::new(),
            geometries: Vec::new(),
            attr_lists: Vec::new(),
            vertex_buffers: Vec::new(),
            index_buffers: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            images: Vec::new(),
            meshes: Vec::new(),
            skeletons: Vec::new(),
            animations: Vec::new(),
            collision_primitives: Vec::new(),
        };

        let make_mesh = |name: &str| -> IgbMesh {
            IgbMesh {
                name: name.to_string(),
                vertices: vec![
                    IgbVertex {
                        position: [0.0, 0.0, 0.0],
                        ..Default::default()
                    },
                    IgbVertex {
                        position: [1.0, 0.0, 0.0],
                        ..Default::default()
                    },
                    IgbVertex {
                        position: [0.0, 0.0, 1.0],
                        ..Default::default()
                    },
                ],
                indices: vec![0, 1, 2],
                material_name: None,
                skeleton: None,
                image_index: None,
                base_color: None,
            }
        };

        igb.geometries.push(IgbGeometryNode {
            name: "col_floor".to_string(),
            vertex_buffer_index: None,
            index_buffer_index: None,
            attr_list_index: None,
            material_index: None,
            mesh: Some(make_mesh("col_floor")),
        });
        igb.geometries.push(IgbGeometryNode {
            name: "vis_pillar".to_string(),
            vertex_buffer_index: None,
            index_buffer_index: None,
            attr_list_index: None,
            material_index: None,
            mesh: Some(make_mesh("vis_pillar")),
        });
        igb.geometries.push(IgbGeometryNode {
            name: "trig_exit".to_string(),
            vertex_buffer_index: None,
            index_buffer_index: None,
            attr_list_index: None,
            material_index: None,
            mesh: Some(make_mesh("trig_exit")),
        });

        // Filtered: col_floor and trig_exit included (2 triangles), vis_pillar excluded
        let filtered = CollisionMesh::from_igb(&igb);
        assert_eq!(filtered.triangles.len(), 2);

        // Unfiltered: all 3 included
        let unfiltered = CollisionMesh::from_igb_filtered(&igb, false);
        assert_eq!(unfiltered.triangles.len(), 3);
    }

    #[test]
    fn test_collision_mesh_dedicated_primitives() {
        use skylands_formats::igb::mat4_identity;
        use skylands_formats::{CollisionPrimitive, CollisionShapeType};

        let mut igb = IgbModel::default();
        let mut xform = mat4_identity();
        xform[12] = 10.0;
        xform[13] = 0.0;
        xform[14] = 10.0;

        igb.collision_primitives.push(CollisionPrimitive {
            name: "Prop_Crate".to_string(),
            shape_type: CollisionShapeType::Box,
            local_position: [0.0, 1.0, 0.0],
            local_rotation_deg: [0.0, 0.0, 0.0],
            extents: [2.0, 2.0, 2.0],
            world_transform: {
                let mut m = xform;
                m[13] = 1.0;
                m
            },
            is_solid: true,
            interacts_with_world: true,
            is_actor_surface: true,
        });

        let col = CollisionMesh::from_igb(&igb);
        assert_eq!(col.primitives.len(), 1);
        assert_eq!(col.triangles.len(), 12);

        // Ground raycast right above crate top (center at [10, 1, 10], half-height 1 -> top y = 2.0)
        let hit = col.raycast_down([10.0, 5.0, 10.0], 10.0);
        assert!(hit.is_some());
        let (y, _) = hit.unwrap();
        assert!(
            (y - 2.0).abs() < 1e-4,
            "Top surface should hit at y=2.0, got {y}"
        );

        // Sphere pushout against the obstacle's north wall (+Z face at z = 11.0, crate center z=10, half-length 1)
        let push = col.resolve_sphere([10.0, 1.0, 11.2], 0.5);
        assert!(
            push.is_some(),
            "Must push out sphere penetrating solid crate"
        );
        let p = push.unwrap();
        assert!(p[2] > 0.0, "Push must be outward (+Z), got {:?}", p);
    }
}
