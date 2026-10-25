use crate::mesh_conversion::{
    combine_or_normalize_model_meshes, igb_model_to_bevy_mesh, Mesh as ProceduralMesh,
};
use crate::texture_conversion::create_colored_material;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::render::view::RenderLayers;
use bevy::utils::HashMap;
pub use skylands_core::GameAssetServer as CharacterAssetServer;
use skylands_core::{
    get_character_info, resolve_character_model_archive, resolve_model_archive, ActiveLevel,
    ActorAnimation, ActorIdentity, ActorScript, Collectible, Element, Enemy, Hero, LevelDrawnModel,
    Projectile, Prop, Villain,
};
#[cfg(test)]
use skylands_formats::AssetLoader;

pub fn element_to_color(element: Element) -> Color {
    let (r, g, b) = element.rgb();
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

fn ensure_unskinned_mesh(mesh_handle: &Handle<Mesh>, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    if let Some(m) = meshes.get(mesh_handle) {
        if m.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some() {
            let mut unskinned = m.clone();
            unskinned.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
            unskinned.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
            return meshes.add(unskinned);
        }
    }
    mesh_handle.clone()
}

#[derive(Resource, Clone, Default)]
pub struct ModelVisualRegistry {
    pub cache: HashMap<String, (Handle<Mesh>, Handle<StandardMaterial>)>,
}

impl ModelVisualRegistry {
    #[allow(clippy::too_many_arguments)]
    pub fn get_or_load_model(
        &mut self,
        identifier: &str,
        target_height: Option<f32>,
        base_color: Option<Color>,
        asset_server: Option<&CharacterAssetServer>,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> Option<(Handle<Mesh>, Handle<StandardMaterial>)> {
        let key = identifier.to_ascii_lowercase();
        if let Some(pair) = self.cache.get(&key) {
            return Some(pair.clone());
        }

        let asset_server = asset_server?;
        let loader = asset_server.loader.lock().ok()?;

        let resolved_path = if loader.has_file(identifier) {
            Some(identifier.to_string())
        } else if let Some((cat, name)) = identifier.split_once('/') {
            resolve_model_archive(cat, name)
        } else {
            resolve_model_archive("character", identifier)
                .or_else(|| resolve_model_archive("outfit", identifier))
                .or_else(|| resolve_model_archive("item", identifier))
                .or_else(|| resolve_model_archive("misc", identifier))
                .or_else(|| resolve_model_archive("level", identifier))
        }?;

        let model = loader.load_igb(&resolved_path).ok()?;
        let mut bevy_mesh = igb_model_to_bevy_mesh(&model, target_height);
        bevy_mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
        bevy_mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
        let mesh_handle = meshes.add(bevy_mesh);

        let mut base_color_texture = None;
        if let Ok((w, h, rgba)) = loader.load_first_image_rgba(&resolved_path) {
            let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
            base_color_texture = Some(images.add(img));
        } else if let Ok((w, h, rgba)) = loader.load_companion_texture_rgba(&resolved_path, None) {
            let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
            base_color_texture = Some(images.add(img));
        }

        let color = if base_color_texture.is_some() {
            Color::WHITE
        } else {
            base_color.unwrap_or(Color::srgb(0.7, 0.7, 0.7))
        };

        let material_handle = materials.add(StandardMaterial {
            base_color: color,
            base_color_texture,
            perceptual_roughness: 0.8,
            reflectance: 0.2,
            cull_mode: None,
            double_sided: true,
            ..default()
        });

        let pair = (mesh_handle, material_handle);
        self.cache.insert(key, pair.clone());
        Some(pair)
    }
}

#[derive(Component)]
pub struct CharacterSubmeshEntity;

/// Actor whose visual has been built. Skinned characters keep their meshes on
/// `CharacterSubmeshEntity` children, so `Mesh3d` on the actor can't mark this.
#[derive(Component)]
pub struct ActorVisual;

pub type SubmeshVisual = (Handle<Mesh>, Handle<StandardMaterial>);

/// Rigid model (weapon, backpack) parented to a skeleton joint.
#[derive(Clone)]
pub struct AttachmentVisual {
    /// Bone index in the primary skeleton.
    pub bone: usize,
    /// Placement in bone space.
    pub transform: Transform,
    pub parts: Vec<SubmeshVisual>,
}

/// Projectile drawn with its character's model (children hold the meshes).
#[derive(Component)]
pub struct ProjectileVisual;

/// Spawned under a joint entity; despawned with the joints.
#[derive(Component)]
pub struct CharacterAttachmentEntity;

#[derive(Resource, Clone, Default)]
pub struct CharacterVisualRegistry {
    pub cache: HashMap<String, SubmeshVisual>,
    pub submeshes: HashMap<String, Vec<SubmeshVisual>>,
    pub attachments: HashMap<String, Vec<AttachmentVisual>>,
    /// The character's projectile model (`IgbModel.projectile`).
    pub projectiles: HashMap<String, Vec<SubmeshVisual>>,
    pub models: HashMap<String, std::sync::Arc<skylands_formats::igb::IgbModel>>,
}

impl CharacterVisualRegistry {
    pub fn get_model(
        &self,
        character_id: u16,
        model_path: Option<&str>,
    ) -> Option<std::sync::Arc<skylands_formats::igb::IgbModel>> {
        let bld_name = model_path
            .map(|s| s.to_string())
            .or_else(|| resolve_character_model_archive(character_id))?;
        let cache_key = format!("{}:{}", character_id, bld_name.to_ascii_lowercase());
        self.models.get(&cache_key).cloned()
    }

    pub fn get_submeshes(
        &self,
        character_id: u16,
        model_path: Option<&str>,
    ) -> Option<Vec<SubmeshVisual>> {
        let bld_name = model_path
            .map(|s| s.to_string())
            .or_else(|| resolve_character_model_archive(character_id))?;
        let cache_key = format!("{}:{}", character_id, bld_name.to_ascii_lowercase());
        self.submeshes.get(&cache_key).cloned()
    }

    pub fn get_attachments(
        &self,
        character_id: u16,
        model_path: Option<&str>,
    ) -> Vec<AttachmentVisual> {
        let Some(bld_name) = model_path
            .map(|s| s.to_string())
            .or_else(|| resolve_character_model_archive(character_id))
        else {
            return Vec::new();
        };
        let cache_key = format!("{}:{}", character_id, bld_name.to_ascii_lowercase());
        self.attachments
            .get(&cache_key)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_projectile(
        &self,
        character_id: u16,
        model_path: Option<&str>,
    ) -> Option<Vec<SubmeshVisual>> {
        let bld_name = model_path
            .map(|s| s.to_string())
            .or_else(|| resolve_character_model_archive(character_id))?;
        let cache_key = format!("{}:{}", character_id, bld_name.to_ascii_lowercase());
        self.projectiles.get(&cache_key).cloned()
    }

    pub fn get_or_load(
        &mut self,
        character_id: u16,
        model_path: Option<&str>,
        asset_server: Option<&CharacterAssetServer>,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> Option<(Handle<Mesh>, Handle<StandardMaterial>)> {
        let bld_name = model_path
            .map(|s| s.to_string())
            .or_else(|| resolve_character_model_archive(character_id))?;
        let cache_key = format!("{}:{}", character_id, bld_name.to_ascii_lowercase());

        if let Some(pair) = self.cache.get(&cache_key) {
            return Some(pair.clone());
        }

        let asset_server = asset_server?;
        let loader = asset_server.loader.lock().ok()?;

        let model = loader.load_igb(&bld_name).ok()?;

        let mat_color = if character_id == 16 {
            Color::srgb(0.42, 0.18, 0.60)
        } else {
            let (_, elem) = get_character_info(character_id);
            element_to_color(elem)
        };

        // Extract submeshes in model space (preserving joint / IBP skinning alignment)
        let submesh_defs = crate::mesh_conversion::igb_character_submeshes(&model);
        let mut loaded_submeshes = Vec::new();

        for sub in &submesh_defs {
            let rgba = sub
                .image_index
                .and_then(|i| loader.load_image_rgba(&bld_name, i).ok())
                .or_else(|| loader.load_first_image_rgba(&bld_name).ok())
                .or_else(|| loader.load_companion_texture_rgba(&bld_name, None).ok());
            let mut alpha_mode = AlphaMode::Opaque;
            let sub_texture = rgba.map(|(w, h, rgba)| {
                alpha_mode = crate::texture_conversion::alpha_mode_for_rgba(&rgba);
                images.add(crate::texture_conversion::rgba8_to_model_texture(
                    w,
                    h,
                    rgba,
                    sub.texture_wrap,
                ))
            });

            let sub_color = if sub_texture.is_some() {
                Color::WHITE
            } else if let Some(c) = sub.base_color {
                Color::srgb(c[0], c[1], c[2])
            } else {
                mat_color
            };

            let sub_mat = materials.add(StandardMaterial {
                base_color: sub_color,
                base_color_texture: sub_texture,
                alpha_mode,
                perceptual_roughness: 0.8,
                reflectance: 0.2,
                cull_mode: None,
                double_sided: true,
                ..default()
            });

            let sub_mesh_handle = meshes.add(sub.mesh.clone());
            loaded_submeshes.push((sub_mesh_handle, sub_mat));
        }

        let first_pair = loaded_submeshes.first().cloned().unwrap_or_else(|| {
            let fallback_mesh = meshes.add(igb_model_to_bevy_mesh(&model, None));
            let fallback_mat = materials.add(StandardMaterial {
                base_color: mat_color,
                perceptual_roughness: 0.8,
                reflectance: 0.2,
                cull_mode: None,
                double_sided: true,
                ..default()
            });
            (fallback_mesh, fallback_mat)
        });

        // Attachments are unskinned: they follow their joint entity rigidly.
        let skeleton = model.primary_skeleton();
        let base_name = |n: &str| n.rsplit(':').next().unwrap_or(n).to_ascii_lowercase();
        let mut loaded_attachments = Vec::new();
        for att in &model.attachments {
            let Some(bone) = skeleton.and_then(|s| {
                s.bones
                    .iter()
                    .position(|b| base_name(&b.name) == base_name(&att.bone))
            }) else {
                continue;
            };
            let parts = rigid_parts(
                &model,
                &att.meshes,
                &loader,
                &bld_name,
                meshes,
                materials,
                images,
            );
            loaded_attachments.push(AttachmentVisual {
                bone,
                transform: Transform::from_matrix(Mat4::from_cols_array(&att.transform)),
                parts,
            });
        }

        if let Some(p) = &model.projectile {
            let parts = rigid_parts(
                &model, &p.meshes, &loader, &bld_name, meshes, materials, images,
            );
            self.projectiles.insert(cache_key.clone(), parts);
        }
        self.attachments
            .insert(cache_key.clone(), loaded_attachments);
        self.submeshes.insert(cache_key.clone(), loaded_submeshes);
        self.cache.insert(cache_key.clone(), first_pair.clone());
        self.models.insert(cache_key, model);
        Some(first_pair)
    }
}

/// Unskinned meshes of a rigid model (weapon, projectile) with their own textures.
fn rigid_parts(
    model: &skylands_formats::igb::IgbModel,
    mesh_indices: &[usize],
    loader: &skylands_formats::AssetLoader,
    bld_name: &str,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Vec<SubmeshVisual> {
    mesh_indices
        .iter()
        .filter_map(|&i| model.meshes.get(i))
        .map(|m| {
            let mut mesh = crate::mesh_conversion::igb_mesh_to_bevy(m);
            mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
            mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
            let mut alpha_mode = AlphaMode::Opaque;
            let texture = m
                .image_index
                .and_then(|i| loader.load_image_rgba(bld_name, i).ok())
                .map(|(w, h, rgba)| {
                    alpha_mode = crate::texture_conversion::alpha_mode_for_rgba(&rgba);
                    images.add(crate::texture_conversion::rgba8_to_model_texture(
                        w,
                        h,
                        rgba,
                        m.texture_wrap,
                    ))
                });
            let color = match (&texture, m.base_color) {
                (Some(_), _) => Color::WHITE,
                (None, Some(c)) => Color::srgb(c[0], c[1], c[2]),
                (None, None) => Color::srgb(0.6, 0.6, 0.6),
            };
            let material = materials.add(StandardMaterial {
                base_color: color,
                base_color_texture: texture,
                alpha_mode,
                perceptual_roughness: 0.8,
                reflectance: 0.2,
                cull_mode: None,
                double_sided: true,
                ..default()
            });
            (meshes.add(mesh), material)
        })
        .collect()
}

#[derive(Resource, Clone)]
pub struct RenderMeshAssets {
    pub courtyard_mesh: Handle<Mesh>,
    pub courtyard_material: Handle<StandardMaterial>,
    pub pedestal_mesh: Handle<Mesh>,
    pub pedestal_material: Handle<StandardMaterial>,
    pub hero_mesh: Handle<Mesh>,
    pub hero_material: Handle<StandardMaterial>,
    pub villain_mesh: Handle<Mesh>,
    pub villain_material: Handle<StandardMaterial>,
    pub prop_mesh: Handle<Mesh>,
    pub prop_material: Handle<StandardMaterial>,
    pub coin_mesh: Handle<Mesh>,
    pub coin_material: Handle<StandardMaterial>,
    pub gem_mesh: Handle<Mesh>,
    pub gem_material: Handle<StandardMaterial>,
    pub projectile_mesh: Handle<Mesh>,
    pub projectile_material: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelRenderMesh;

/// Model of a script-animated level actor (`IgbModel::node_owner`), in the actor's
/// space. Follows the core entity whose `ActorScript` has the same marker node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedActorMesh {
    pub marker_node: usize,
    /// The actor has been seen; its later absence means it was removed.
    pub seen: bool,
}

/// Part of the level's sky layer, seen only by `SkyCamera`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelSkyMesh;

/// Level geometry sharing one diffuse texture, wrap mode and tint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LevelMaterialKey {
    image: Option<usize>,
    wrap: [skylands_formats::igb::TextureWrap; 2],
    /// Tint bits, only used when there is no texture.
    color: Option<[u32; 4]>,
}

impl LevelMaterialKey {
    fn of(mesh: &skylands_formats::igb::IgbMesh) -> Self {
        Self {
            image: mesh.image_index,
            wrap: mesh.texture_wrap,
            color: mesh
                .image_index
                .is_none()
                .then_some(mesh.base_color)
                .flatten()
                .map(|c| c.map(f32::to_bits)),
        }
    }
}

pub fn spawn_level_render_mesh(
    commands: &mut Commands,
    loader: &skylands_formats::AssetLoader,
    level_name: &str,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> bool {
    let resolved = if loader.has_file(level_name) {
        Some(level_name.to_string())
    } else {
        resolve_model_archive("level", level_name)
    };

    let Some(path) = resolved else {
        return false;
    };

    let Ok(model) = loader.load_igb(&path) else {
        return false;
    };

    if model.meshes.is_empty() {
        return false;
    }

    let is_level = path.to_ascii_lowercase().contains("level");
    let instances = model.evaluate_scene_graph();

    // One draw per diffuse material (`Drawable._material`), so each part keeps its texture.
    // Sky layer parts are drawn apart, around the camera (`SkyCamera`), and models of
    // script-animated actors apart per actor (`ScriptedActorMesh`).
    let mut material_groups: HashMap<
        (bool, Option<usize>, LevelMaterialKey),
        Vec<&skylands_formats::igb::SceneInstance>,
    > = HashMap::new();
    if !instances.is_empty() && !model.geometries.is_empty() {
        let tagged = instances
            .iter()
            .map(|i| (false, i))
            .chain(model.sky_instances.iter().map(|i| (true, i)));
        for (sky, inst) in tagged {
            if let Some(m) = model
                .geometries
                .get(inst.geometry_index)
                .and_then(|g| g.mesh.as_ref())
            {
                let owner = (!sky)
                    .then(|| model.node_owner.get(&inst.node_index).copied())
                    .flatten();
                material_groups
                    .entry((sky, owner, LevelMaterialKey::of(m)))
                    .or_default()
                    .push(inst);
            }
        }
    }
    let level_textures = loader.load_images_rgba(
        &path,
        material_groups.keys().filter_map(|(_, _, k)| k.image),
    );
    // Parts sharing an image (sky and world, or several tints) share one `Image`.
    let mut texture_handles = HashMap::new();
    let mut groups: Vec<_> = material_groups.into_iter().collect();
    groups.sort_by_key(|((sky, owner, k), _)| (*sky, *owner, k.image));

    let mut spawned_any = false;

    if groups.is_empty() {
        let mut base_color_texture = None;
        if let Ok((w, h, rgba)) = loader.load_companion_texture_rgba(&path, None) {
            let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
            base_color_texture = Some(images.add(img));
        } else if !is_level {
            if let Ok((w, h, rgba)) = loader.load_first_image_rgba(&path) {
                let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
                base_color_texture = Some(images.add(img));
            }
        }

        let mat_color = if base_color_texture.is_some() {
            Color::WHITE
        } else {
            Color::srgb(0.72, 0.70, 0.65)
        };

        let level_material = materials.add(StandardMaterial {
            base_color: mat_color,
            base_color_texture,
            perceptual_roughness: 0.85,
            reflectance: 0.20,
            cull_mode: None,
            double_sided: true,
            ..default()
        });

        let mut combined_mesh = combine_or_normalize_model_meshes(&model.meshes, None);
        combined_mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
        combined_mesh.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
        let mesh_handle = meshes.add(combined_mesh);

        commands.spawn((
            LevelRenderMesh,
            Mesh3d(mesh_handle),
            MeshMaterial3d(level_material),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        spawned_any = true;
    } else {
        for ((sky, owner, key), group_instances) in groups {
            // Owned parts are built in their actor's space.
            let owner_world = owner.and_then(|o| model.nodes.get(o)).map(|n| n.transform);
            let to_local = owner_world
                .map(|w| skylands_formats::igb::mat4_affine_inverse(&w))
                .unwrap_or_else(skylands_formats::igb::mat4_identity);
            let mut total_verts = 0;
            let mut total_idxs = 0;
            for inst in &group_instances {
                if let Some(geo) = model.geometries.get(inst.geometry_index) {
                    if let Some(ref m) = geo.mesh {
                        total_verts += m.vertices.len();
                        total_idxs += m.indices.len();
                    }
                }
            }

            if total_verts == 0 || total_idxs == 0 {
                continue;
            }

            let mut positions = Vec::with_capacity(total_verts);
            let mut normals = Vec::with_capacity(total_verts);
            let mut uvs = Vec::with_capacity(total_verts);
            let mut colors = Vec::with_capacity(total_verts);
            let mut indices = Vec::with_capacity(total_idxs);
            let mut vert_offset = 0u32;

            for inst in &group_instances {
                if let Some(geo) = model.geometries.get(inst.geometry_index) {
                    if let Some(ref m) = geo.mesh {
                        if m.vertices.is_empty() || m.indices.is_empty() {
                            continue;
                        }
                        let xf = skylands_formats::igb::mat4_mul(&to_local, &inst.world_transform);
                        for v in &m.vertices {
                            positions.push(skylands_formats::igb::transform_point(&xf, v.position));
                            normals.push(skylands_formats::igb::transform_dir(&xf, v.normal));
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

            if indices.is_empty() {
                continue;
            }

            let mut bevy_mesh = Mesh::new(
                bevy::render::mesh::PrimitiveTopology::TriangleList,
                bevy::render::render_asset::RenderAssetUsages::default(),
            );
            bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            bevy_mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

            let texture = key.image.and_then(|i| {
                let (w, h, rgba) = level_textures.get(&i)?;
                let entry = texture_handles.entry((i, key.wrap)).or_insert_with(|| {
                    let alpha = crate::texture_conversion::alpha_mode_for_rgba(rgba);
                    let image = crate::texture_conversion::rgba8_to_model_texture(
                        *w,
                        *h,
                        rgba.clone(),
                        key.wrap,
                    );
                    (images.add(image), alpha)
                });
                Some(entry.clone())
            });
            let alpha_mode = texture.as_ref().map_or(AlphaMode::Opaque, |t| t.1);
            let base_color_texture = texture.map(|t| t.0);

            let mat_color = match (&base_color_texture, key.color) {
                (Some(_), _) => Color::WHITE,
                (None, Some(c)) => Color::srgb(
                    f32::from_bits(c[0]),
                    f32::from_bits(c[1]),
                    f32::from_bits(c[2]),
                ),
                (None, None) => Color::srgb(0.72, 0.70, 0.65),
            };

            let submesh_mat = materials.add(StandardMaterial {
                base_color: mat_color,
                base_color_texture,
                alpha_mode,
                perceptual_roughness: 0.85,
                reflectance: 0.20,
                cull_mode: None,
                double_sided: true,
                unlit: sky,
                ..default()
            });

            let mesh_handle = meshes.add(bevy_mesh);
            let transform = owner_world.map_or(Transform::IDENTITY, |w| {
                Transform::from_matrix(Mat4::from_cols_array(&w))
            });
            let mut part = commands.spawn((
                LevelRenderMesh,
                Mesh3d(mesh_handle),
                MeshMaterial3d(submesh_mat),
                transform,
            ));
            if let Some(marker_node) = owner {
                part.insert(ScriptedActorMesh {
                    marker_node,
                    seen: false,
                });
            }
            if sky {
                part.insert((
                    LevelSkyMesh,
                    RenderLayers::layer(crate::camera::SKY_LAYER),
                    NotShadowCaster,
                ));
            }

            spawned_any = true;
        }
    }

    spawned_any
}

/// Moves script-animated actor models with their core entity; hides them once the
/// entity is gone (`OpRemove my`).
pub fn scripted_actor_mesh_follow_system(
    actors: Query<(&ActorScript, &Transform), Without<ScriptedActorMesh>>,
    mut parts: Query<(&mut ScriptedActorMesh, &mut Transform, &mut Visibility)>,
) {
    let by_node: HashMap<usize, Transform> =
        actors.iter().map(|(s, t)| (s.marker_node, *t)).collect();
    for (mut part, mut transform, mut visibility) in &mut parts {
        match by_node.get(&part.marker_node) {
            Some(t) => {
                part.seen = true;
                *transform = *t;
            }
            None if part.seen => *visibility = Visibility::Hidden,
            None => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn level_render_mesh_sync_system(
    mut commands: Commands,
    active_level: Option<Res<ActiveLevel>>,
    asset_server: Option<Res<CharacterAssetServer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    render_assets: Option<Res<RenderMeshAssets>>,
    existing_level_meshes: Query<Entity, With<LevelRenderMesh>>,
    mut last_level: Local<Option<String>>,
) {
    let Some(ref active) = active_level else {
        return;
    };
    if last_level.is_none() {
        *last_level = Some(active.name.clone());
        return;
    }
    if last_level.as_deref() == Some(&active.name) {
        return;
    }
    *last_level = Some(active.name.clone());

    for ent in &existing_level_meshes {
        commands.entity(ent).despawn_recursive();
    }

    let mut loaded = false;
    if let Some(ref server) = asset_server {
        if let Ok(loader) = server.loader.lock() {
            loaded = spawn_level_render_mesh(
                &mut commands,
                &loader,
                &active.name,
                &mut meshes,
                &mut materials,
                &mut images,
            );
        }
    }

    if !loaded {
        let (courtyard_mesh, courtyard_mat) = if let Some(ref ra) = render_assets {
            (ra.courtyard_mesh.clone(), ra.courtyard_material.clone())
        } else {
            (
                meshes.add(ProceduralMesh::synthetic_courtyard().to_bevy()),
                create_colored_material(Color::srgb(0.5, 0.5, 0.48), &mut materials),
            )
        };
        commands.spawn((
            LevelRenderMesh,
            Mesh3d(courtyard_mesh),
            MeshMaterial3d(courtyard_mat),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
    }
}

pub fn setup_environment_and_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Option<Res<CharacterAssetServer>>,
    active_level: Option<Res<ActiveLevel>>,
    viewer_settings: Option<Res<crate::ViewerSettings>>,
) {
    let courtyard_mesh = meshes.add(ProceduralMesh::synthetic_courtyard().to_bevy());
    let courtyard_material = create_colored_material(Color::srgb(0.5, 0.5, 0.48), &mut materials);

    let pedestal_mesh = meshes.add(ProceduralMesh::portal_pedestal(0.0, 1.8, 0.25, 24).to_bevy());
    let pedestal_material = create_colored_material(Color::srgb(0.3, 0.7, 0.9), &mut materials);

    let hero_mesh = meshes.add(ProceduralMesh::prop_cube(0.5).to_bevy());
    let hero_material = create_colored_material(Color::srgb(0.2, 0.6, 1.0), &mut materials);

    let villain_mesh = meshes.add(ProceduralMesh::prop_cube(0.6).to_bevy());
    let villain_material = create_colored_material(Color::srgb(0.8, 0.2, 0.2), &mut materials);

    let prop_mesh = meshes.add(ProceduralMesh::prop_cube(0.4).to_bevy());
    let prop_material = create_colored_material(Color::srgb(0.6, 0.4, 0.2), &mut materials);

    let coin_mesh = meshes.add(ProceduralMesh::collectible_coin(0.25, 0.05, 12).to_bevy());
    let coin_material = create_colored_material(Color::srgb(1.0, 0.84, 0.0), &mut materials);

    let gem_mesh = meshes.add(ProceduralMesh::collectible_gem(0.2, 0.4).to_bevy());
    let gem_material = create_colored_material(Color::srgb(0.1, 0.8, 1.0), &mut materials);

    let projectile_mesh = meshes.add(ProceduralMesh::collectible_coin(0.2, 0.1, 8).to_bevy());
    let projectile_material = create_colored_material(Color::srgb(1.0, 0.6, 0.1), &mut materials);

    let is_viewer = viewer_settings
        .as_ref()
        .map(|v| v.viewer_mode)
        .unwrap_or(false);
    let level_name = active_level
        .as_ref()
        .map(|l| l.name.clone())
        .unwrap_or_else(|| "Level_Hub_2014_01.bld".to_string());

    let mut loaded_real_level = false;

    if !is_viewer {
        if let Some(ref server) = asset_server {
            if let Ok(loader) = server.loader.lock() {
                loaded_real_level = spawn_level_render_mesh(
                    &mut commands,
                    &loader,
                    &level_name,
                    &mut meshes,
                    &mut materials,
                    &mut images,
                );
            }
        }
    }

    if !loaded_real_level {
        commands.spawn((
            LevelRenderMesh,
            Mesh3d(courtyard_mesh.clone()),
            MeshMaterial3d(courtyard_material.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
    }

    commands.spawn((
        Mesh3d(pedestal_mesh.clone()),
        MeshMaterial3d(pedestal_material.clone()),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    commands.insert_resource(RenderMeshAssets {
        courtyard_mesh,
        courtyard_material,
        pedestal_mesh,
        pedestal_material,
        hero_mesh,
        hero_material,
        villain_mesh,
        villain_material,
        prop_mesh,
        prop_material,
        coin_mesh,
        coin_material,
        gem_mesh,
        gem_material,
        projectile_mesh,
        projectile_material,
    });
}

#[derive(Component)]
pub struct CharacterAnimator {
    pub skeleton: skylands_formats::Skeleton,
    pub animations: Vec<skylands_formats::AnimationClip>,
    /// Logical state last requested (`animation_state_name`, viewer or Magic Moment clip).
    pub current_clip: String,
    /// Seconds into `playing`.
    pub elapsed: f32,
    pub joints: Vec<Entity>,
    /// The character's `tfbAnimationState` transitions (empty: plain clip switches).
    pub graph: skylands_formats::AnimationGraph,
    /// Clip actually playing; differs from the request inside a transition (`idletowalk`).
    pub playing: String,
    /// `tfbAnimationState` id of `playing`, when the graph knows it.
    pub state: Option<i32>,
    pub blend: Option<ClipBlend>,
}

impl CharacterAnimator {
    pub fn new(
        skeleton: skylands_formats::Skeleton,
        animations: Vec<skylands_formats::AnimationClip>,
        graph: skylands_formats::AnimationGraph,
        joints: Vec<Entity>,
    ) -> Self {
        let mut animator = Self {
            skeleton,
            animations,
            current_clip: String::new(),
            elapsed: 0.0,
            joints,
            graph,
            playing: String::new(),
            state: None,
            blend: None,
        };
        animator.advance("idle", 0.0);
        animator.blend = None;
        animator
    }

    fn clip(&self, name: &str) -> Option<&skylands_formats::AnimationClip> {
        self.animations.iter().find(|c| c.name == name)
    }

    /// Switches to `clip`, cross-fading from the current pose over `blend` seconds.
    fn start(&mut self, clip: String, state: Option<i32>, blend: f32) {
        self.blend = (blend > 0.0 && !self.playing.is_empty()).then(|| ClipBlend {
            from: std::mem::take(&mut self.playing),
            from_time: self.sample_time(),
            duration: blend,
            elapsed: 0.0,
        });
        self.playing = clip;
        self.state = state;
        self.elapsed = 0.0;
    }

    /// Steps playback by `dt` towards the logical state `requested`. A new request goes
    /// through the graph's transition for it (destination state and blend time); a clip
    /// that ends with an automatic transition moves on to its destination.
    pub fn advance(&mut self, requested: &str, dt: f32) {
        if let Some(blend) = &mut self.blend {
            blend.elapsed += dt;
            if blend.elapsed >= blend.duration {
                self.blend = None;
            }
        }
        if self.current_clip != requested {
            self.current_clip = requested.to_string();
            let Some(target) = resolve_clip(&self.animations, requested).map(|c| c.name.clone())
            else {
                self.playing.clear();
                self.state = None;
                self.elapsed = 0.0;
                return;
            };
            let request = self.graph.state_for_clip(&target);
            let transition = request
                .and_then(|r| self.graph.transition(self.state, r))
                .and_then(|t| {
                    let clip = self.graph.clip_for_state(t.to)?;
                    self.clip(clip)
                        .map(|c| (c.name.clone(), Some(t.to), t.blend))
                });
            let (clip, state, blend) = transition.unwrap_or((target, request, 0.0));
            self.start(clip, state, blend);
            return;
        }
        self.elapsed += dt;
        let ended = self
            .clip(&self.playing)
            .is_some_and(|c| self.elapsed >= c.duration);
        let next = self
            .state
            .filter(|_| ended)
            .and_then(|s| self.graph.auto_transition(s))
            .and_then(|t| {
                let clip = self.clip(self.graph.clip_for_state(t.to)?)?;
                Some((clip.name.clone(), t.to, t.blend))
            });
        if let Some((clip, state, blend)) = next {
            self.start(clip, Some(state), blend);
        }
    }

    pub fn advance_state(&mut self, state_id: i32, dt: f32) {
        if let Some(blend) = &mut self.blend {
            blend.elapsed += dt;
            if blend.elapsed >= blend.duration {
                self.blend = None;
            }
        }
        if self.state != Some(state_id) {
            let clip = self
                .graph
                .clip_for_state(state_id)
                .and_then(|name| self.clip(name))
                .map(|c| c.name.clone());
            if let Some(clip) = clip {
                let transition = self.graph.transition(self.state, state_id).and_then(|t| {
                    let c = self.clip(self.graph.clip_for_state(t.to)?)?;
                    Some((c.name.clone(), Some(t.to), t.blend))
                });
                let (target_clip, target_state, blend) =
                    transition.unwrap_or((clip, Some(state_id), 0.0));
                self.current_clip = target_clip.clone();
                self.start(target_clip, target_state, blend);
                return;
            }
        }
        self.elapsed += dt;
        let ended = self
            .clip(&self.playing)
            .is_some_and(|c| self.elapsed >= c.duration);
        let next = self
            .state
            .filter(|_| ended)
            .and_then(|s| self.graph.auto_transition(s))
            .and_then(|t| {
                let clip = self.clip(self.graph.clip_for_state(t.to)?)?;
                Some((clip.name.clone(), t.to, t.blend))
            });
        if let Some((clip, state, blend)) = next {
            self.start(clip, Some(state), blend);
        }
    }

    /// Attacks, jump take-offs, the portal entrance and states that auto-advance play once
    /// and hold their last frame; everything else loops.
    fn holds_last_frame(&self) -> bool {
        is_one_shot(&self.current_clip)
            || self
                .state
                .is_some_and(|s| self.graph.auto_transition(s).is_some())
    }

    fn sample_time(&self) -> f32 {
        match self.clip(&self.playing) {
            Some(c) if self.holds_last_frame() => self.elapsed.min(c.duration * 0.999),
            _ => self.elapsed,
        }
    }

    /// Joint transforms (model space) for the current frame, cross-faded while blending.
    pub fn pose(&self) -> Vec<Transform> {
        let sample = |clip: &str, t: f32| -> Vec<Transform> {
            self.clip(clip)
                .map(|c| c.sample_global_transforms(t, &self.skeleton))
                .unwrap_or_default()
                .iter()
                .map(|m| Transform::from_matrix(Mat4::from_cols_array(m)))
                .collect()
        };
        let mut pose = sample(&self.playing, self.sample_time());
        if let Some(blend) = &self.blend {
            let from = sample(&blend.from, blend.from_time);
            let w = (blend.elapsed / blend.duration).clamp(0.0, 1.0);
            for (to, from) in pose.iter_mut().zip(from) {
                *to = Transform {
                    translation: from.translation.lerp(to.translation, w),
                    rotation: from.rotation.slerp(to.rotation, w),
                    scale: from.scale.lerp(to.scale, w),
                };
            }
        }
        pose
    }
}

/// Cross-fade from the pose `from` held at `from_time` (`AnimationTransition.blend`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClipBlend {
    pub from: String,
    pub from_time: f32,
    pub duration: f32,
    pub elapsed: f32,
}

/// Real clip for a logical state from `skylands_core::animation_state_name`, falling back
/// through the names character files use (`CrockWarrior_CombatIdle`, `Kaos_JumpStanding`).
pub fn resolve_clip<'a>(
    clips: &'a [skylands_formats::AnimationClip],
    state: &str,
) -> Option<&'a skylands_formats::AnimationClip> {
    let fallbacks: &[&str] = match state {
        "idle" => &["combatidle"],
        "walk" => &["run"],
        "run" => &["walk"],
        "jump" => &["jumpstanding", "flyup", "falling"],
        "fall" => &["falling", "flightfalling"],
        "spawn" => &["spawn02", "spawn01"],
        "toyalive" => &["toy", "spawn02", "spawn"],
        "attack_sweep" => &["spinattack", "smashattack", "attack"],
        s if s.starts_with("attack") => &["attack"],
        _ => &[],
    };
    let named = |n: &str| clips.iter().find(|a| a.name.eq_ignore_ascii_case(n));
    named(state)
        .or_else(|| fallbacks.iter().find_map(|n| named(n)))
        .or_else(|| {
            state
                .starts_with("attack")
                .then(|| clips.iter().find(|a| a.name.contains("attack")))
                .flatten()
        })
        .or_else(|| named("idle"))
        .or_else(|| clips.first())
}

/// Attacks, jump take-offs and the portal entrance play once and hold their last frame;
/// everything else loops.
fn is_one_shot(state: &str) -> bool {
    state.starts_with("attack") || matches!(state, "jump" | "spawn" | "toyalive")
}

pub fn character_animation_system(
    time: Res<Time>,
    viewer_settings: Option<Res<crate::ViewerSettings>>,
    mut anim_query: Query<(
        &mut CharacterAnimator,
        Option<&ActorAnimation>,
        Option<&crate::magic_moment::MagicMomentPose>,
    )>,
    mut transform_query: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let viewer_clip = viewer_settings
        .as_ref()
        .filter(|v| v.viewer_mode)
        .map(|v| v.clip_name.clone());

    for (mut anim, actor_anim, mm_pose) in &mut anim_query {
        match mm_pose {
            // The Magic Moment drives its Skylander's clip and time (`ToyAlive` held at
            // rate 0), with no transitions.
            Some(pose) => {
                let clip = resolve_clip(&anim.animations, &pose.clip).map(|c| c.name.clone());
                anim.current_clip = pose.clip.clone();
                anim.playing = clip.unwrap_or_default();
                anim.state = None;
                anim.blend = None;
                anim.elapsed = pose.time;
            }
            None => {
                if viewer_clip.is_none() {
                    if let Some(state_id) = actor_anim.and_then(|a| a.state) {
                        anim.advance_state(state_id, dt);
                    } else {
                        let requested = actor_anim
                            .and_then(|a| a.current_clip.as_deref())
                            .unwrap_or("idle")
                            .to_string();
                        anim.advance(&requested, dt);
                    }
                } else {
                    let requested = viewer_clip
                        .as_deref()
                        .or_else(|| actor_anim.and_then(|a| a.current_clip.as_deref()))
                        .unwrap_or("idle")
                        .to_string();
                    anim.advance(&requested, dt);
                }
            }
        }

        for (&joint_entity, pose) in anim.joints.iter().zip(anim.pose()) {
            if let Ok(mut tf) = transform_query.get_mut(joint_entity) {
                *tf = pose;
            }
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn actor_visual_sync_system(
    mut commands: Commands,
    assets: Option<Res<RenderMeshAssets>>,
    asset_server: Option<Res<CharacterAssetServer>>,
    mut visual_registry: Option<ResMut<CharacterVisualRegistry>>,
    mut model_registry: Option<ResMut<ModelVisualRegistry>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut ibp_assets: Option<ResMut<Assets<SkinnedMeshInverseBindposes>>>,
    mut viewer_settings: Option<ResMut<crate::ViewerSettings>>,
    anim_query: Query<&CharacterAnimator>,
    submesh_query: Query<(Entity, &Parent), With<CharacterSubmeshEntity>>,
    actor_query: Query<
        (
            Entity,
            &ActorIdentity,
            Option<&Hero>,
            Option<&Villain>,
            Option<&Prop>,
            Has<Enemy>,
            Has<LevelDrawnModel>,
            Has<crate::magic_moment::MagicMomentModel>,
        ),
        Or<(Without<ActorVisual>, Changed<ActorIdentity>)>,
    >,
    collectible_query: Query<(Entity, &Collectible), Without<Mesh3d>>,
    projectile_query: Query<(Entity, &Projectile), (Without<Mesh3d>, Without<ProjectileVisual>)>,
    owner_query: Query<&ActorIdentity>,
) {
    let Some(assets) = assets else {
        return;
    };

    for (entity, ident, maybe_hero, maybe_villain, maybe_prop, is_enemy, level_drawn, is_intro) in
        &actor_query
    {
        commands.entity(entity).insert(ActorVisual);
        // The portal intro copy is drawn like the hero it shows.
        let is_hero = maybe_hero.is_some() || is_intro;
        let is_villain = maybe_villain.is_some() || is_enemy;

        if is_hero || is_villain {
            let custom_visual = visual_registry.as_mut().and_then(|reg| {
                reg.get_or_load(
                    ident.character_id,
                    ident.model_path.as_deref(),
                    asset_server.as_deref(),
                    &mut meshes,
                    &mut materials,
                    &mut images,
                )
            });
            let model_data = visual_registry
                .as_ref()
                .and_then(|reg| reg.get_model(ident.character_id, ident.model_path.as_deref()));

            if let Some((mesh, mat)) = custom_visual {
                if let Ok(old_anim) = anim_query.get(entity) {
                    for &j in &old_anim.joints {
                        commands.entity(j).despawn_recursive();
                    }
                    commands.entity(entity).remove::<SkinnedMesh>();
                    commands.entity(entity).remove::<CharacterAnimator>();
                }
                for (child, parent) in &submesh_query {
                    if parent.get() == entity {
                        commands.entity(child).despawn_recursive();
                    }
                }

                let submeshes = visual_registry
                    .as_ref()
                    .and_then(|reg| {
                        reg.get_submeshes(ident.character_id, ident.model_path.as_deref())
                    })
                    .unwrap_or_default();

                if is_hero {
                    if let Some(ref mut vs) = viewer_settings {
                        if vs.viewer_mode {
                            if !submeshes.is_empty() {
                                let mut total_v = 0;
                                let mut total_t = 0;
                                for (sm, _) in &submeshes {
                                    if let Some(m) = meshes.get(sm) {
                                        let v = m.count_vertices();
                                        total_v += v;
                                        total_t +=
                                            m.indices().map(|idx| idx.len() / 3).unwrap_or(v / 3);
                                    }
                                }
                                vs.vert_count = total_v;
                                vs.tri_count = total_t;
                            } else if let Some(m) = meshes.get(&mesh) {
                                vs.vert_count = m.count_vertices();
                                vs.tri_count = m
                                    .indices()
                                    .map(|idx| idx.len() / 3)
                                    .unwrap_or(vs.vert_count / 3);
                            }
                            if let Some(ref md) = model_data {
                                vs.bone_count =
                                    md.primary_skeleton().map(|s| s.bones.len()).unwrap_or(0);
                            }
                        }
                    }
                }

                let mut skinned_applied = false;
                if let (Some(ref md), Some(ref mut ibp_res)) = (&model_data, &mut ibp_assets) {
                    if let Some(skeleton) = md.primary_skeleton() {
                        if !skeleton.bones.is_empty() {
                            let ibp_mats: Vec<Mat4> = skeleton
                                .bones
                                .iter()
                                .map(|b| Mat4::from_cols_array(&b.inverse_bind_pose))
                                .collect();
                            let ibp_handle =
                                ibp_res.add(SkinnedMeshInverseBindposes::from(ibp_mats));
                            let joints: Vec<Entity> = skeleton
                                .bones
                                .iter()
                                .map(|b| {
                                    let bind_world = skylands_formats::igb::mat4_affine_inverse(
                                        &b.inverse_bind_pose,
                                    );
                                    commands
                                        .spawn((
                                            Transform::from_matrix(Mat4::from_cols_array(
                                                &bind_world,
                                            )),
                                            Visibility::default(),
                                        ))
                                        .id()
                                })
                                .collect();

                            if submeshes.len() > 1 {
                                for (sub_m, sub_mat) in &submeshes {
                                    let child = commands
                                        .spawn((
                                            Mesh3d(sub_m.clone()),
                                            MeshMaterial3d(sub_mat.clone()),
                                            SkinnedMesh {
                                                inverse_bindposes: ibp_handle.clone(),
                                                joints: joints.clone(),
                                            },
                                            CharacterSubmeshEntity,
                                            Visibility::default(),
                                        ))
                                        .id();
                                    commands.entity(entity).add_child(child);
                                }
                                commands.entity(entity).insert((
                                    CharacterAnimator::new(
                                        skeleton.clone(),
                                        md.primary_animations(),
                                        md.animation_graph(),
                                        joints.clone(),
                                    ),
                                    Visibility::default(),
                                ));
                            } else {
                                commands.entity(entity).insert((
                                    Mesh3d(mesh.clone()),
                                    MeshMaterial3d(mat.clone()),
                                    SkinnedMesh {
                                        inverse_bindposes: ibp_handle,
                                        joints: joints.clone(),
                                    },
                                    CharacterAnimator::new(
                                        skeleton.clone(),
                                        md.primary_animations(),
                                        md.animation_graph(),
                                        joints.clone(),
                                    ),
                                ));
                            }
                            commands.entity(entity).add_children(&joints);
                            let attachments = visual_registry
                                .as_ref()
                                .map(|reg| {
                                    reg.get_attachments(
                                        ident.character_id,
                                        ident.model_path.as_deref(),
                                    )
                                })
                                .unwrap_or_default();
                            for att in attachments {
                                let Some(&joint) = joints.get(att.bone) else {
                                    continue;
                                };
                                for (part_mesh, part_mat) in att.parts {
                                    let child = commands
                                        .spawn((
                                            Mesh3d(part_mesh),
                                            MeshMaterial3d(part_mat),
                                            att.transform,
                                            CharacterAttachmentEntity,
                                            Visibility::default(),
                                        ))
                                        .id();
                                    commands.entity(joint).add_child(child);
                                }
                            }
                            skinned_applied = true;
                        }
                    }
                }

                if !skinned_applied {
                    let safe_mesh = ensure_unskinned_mesh(&mesh, &mut meshes);
                    commands
                        .entity(entity)
                        .insert((Mesh3d(safe_mesh), MeshMaterial3d(mat)));
                }
            } else if is_hero {
                commands.entity(entity).insert((
                    Mesh3d(assets.hero_mesh.clone()),
                    MeshMaterial3d(assets.hero_material.clone()),
                ));
            } else {
                commands.entity(entity).insert((
                    Mesh3d(assets.villain_mesh.clone()),
                    MeshMaterial3d(assets.villain_material.clone()),
                ));
            }
        } else if maybe_prop.is_some() && !level_drawn {
            let prop_visual = model_registry.as_mut().and_then(|reg| {
                let path = ident.model_path.as_deref()?;
                reg.get_or_load_model(
                    path,
                    Some(1.0),
                    Some(Color::srgb(0.6, 0.4, 0.2)),
                    asset_server.as_deref(),
                    &mut meshes,
                    &mut materials,
                    &mut images,
                )
            });

            if let Some((mesh, mat)) = prop_visual {
                commands
                    .entity(entity)
                    .insert((Mesh3d(mesh), MeshMaterial3d(mat)));
            } else {
                commands.entity(entity).insert((
                    Mesh3d(assets.prop_mesh.clone()),
                    MeshMaterial3d(assets.prop_material.clone()),
                ));
            }
        }
    }

    for (entity, coll) in &collectible_query {
        let (mesh, mat) = if coll.value > 1 {
            (assets.gem_mesh.clone(), assets.gem_material.clone())
        } else {
            (assets.coin_mesh.clone(), assets.coin_material.clone())
        };
        commands
            .entity(entity)
            .insert((Mesh3d(mesh), MeshMaterial3d(mat)));
    }

    for (entity, projectile) in &projectile_query {
        // The firing character's projectile actor model (harpoon, fireball, boomerang).
        let parts = projectile
            .owner
            .and_then(|o| owner_query.get(o).ok())
            .zip(visual_registry.as_ref())
            .and_then(|(ident, reg)| {
                reg.get_projectile(ident.character_id, ident.model_path.as_deref())
            })
            .filter(|parts| !parts.is_empty());
        let Some(parts) = parts else {
            commands.entity(entity).insert((
                Mesh3d(assets.projectile_mesh.clone()),
                MeshMaterial3d(assets.projectile_material.clone()),
            ));
            continue;
        };
        commands
            .entity(entity)
            .insert((ProjectileVisual, Visibility::default()))
            .with_children(|p| {
                for (mesh, mat) in parts {
                    p.spawn((Mesh3d(mesh), MeshMaterial3d(mat)));
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_material_key_splits_by_texture() {
        use skylands_formats::igb::{IgbMesh, TextureWrap};
        let mesh = |image: Option<usize>, color: Option<[f32; 4]>| IgbMesh {
            image_index: image,
            base_color: color,
            ..Default::default()
        };
        let grass = LevelMaterialKey::of(&mesh(Some(3), Some([0.5; 4])));
        assert_eq!(grass, LevelMaterialKey::of(&mesh(Some(3), None)));
        assert_ne!(grass, LevelMaterialKey::of(&mesh(Some(4), None)));
        let clamped = IgbMesh {
            texture_wrap: [TextureWrap::Clamp; 2],
            uv_scroll: None,
            ..mesh(Some(3), None)
        };
        assert_ne!(grass, LevelMaterialKey::of(&clamped));
        // Untextured parts keep their tint apart.
        assert_ne!(
            LevelMaterialKey::of(&mesh(None, Some([1.0, 0.0, 0.0, 1.0]))),
            LevelMaterialKey::of(&mesh(None, Some([0.0, 1.0, 0.0, 1.0]))),
        );
    }

    #[test]
    fn test_character_asset_server_resource() {
        let mut app = App::new();
        let server = CharacterAssetServer::default();
        app.insert_resource(server);
        assert!(app.world().get_resource::<CharacterAssetServer>().is_some());
    }

    #[test]
    fn test_character_visual_registry_resource() {
        let mut app = App::new();
        let registry = CharacterVisualRegistry::default();
        app.insert_resource(registry);
        assert!(app
            .world()
            .get_resource::<CharacterVisualRegistry>()
            .is_some());
    }

    #[test]
    fn test_spyro_custom_visual_loading_fallback() {
        let server = CharacterAssetServer::default();
        let mut registry = CharacterVisualRegistry::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();

        // When archive missing, registry returns None gracefully
        let res = registry.get_or_load(
            16,
            Some("016_Spyro.bld"),
            Some(&server),
            &mut meshes,
            &mut materials,
            &mut images,
        );
        assert!(res.is_none());
    }

    #[test]
    fn test_actor_visual_sync_system_spyro_assignment() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<CharacterAssetServer>();
        app.init_resource::<CharacterVisualRegistry>();
        app.init_resource::<ModelVisualRegistry>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let hero_mesh = meshes.add(ProceduralMesh::prop_cube(0.5).to_bevy());
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let hero_mat = materials.add(StandardMaterial::default());

        let render_assets = RenderMeshAssets {
            courtyard_mesh: hero_mesh.clone(),
            courtyard_material: hero_mat.clone(),
            pedestal_mesh: hero_mesh.clone(),
            pedestal_material: hero_mat.clone(),
            hero_mesh: hero_mesh.clone(),
            hero_material: hero_mat.clone(),
            villain_mesh: hero_mesh.clone(),
            villain_material: hero_mat.clone(),
            prop_mesh: hero_mesh.clone(),
            prop_material: hero_mat.clone(),
            coin_mesh: hero_mesh.clone(),
            coin_material: hero_mat.clone(),
            gem_mesh: hero_mesh.clone(),
            gem_material: hero_mat.clone(),
            projectile_mesh: hero_mesh.clone(),
            projectile_material: hero_mat.clone(),
        };
        app.insert_resource(render_assets);

        let custom_mesh = {
            let mut m = app.world_mut().resource_mut::<Assets<Mesh>>();
            m.add(ProceduralMesh::prop_cube(1.8).to_bevy())
        };
        let custom_mat = {
            let mut mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
            mats.add(StandardMaterial {
                base_color: Color::srgb(0.42, 0.18, 0.60),
                ..default()
            })
        };
        {
            let mut reg = app.world_mut().resource_mut::<CharacterVisualRegistry>();
            reg.cache.insert(
                "16:016_spyro.bld".to_string(),
                (custom_mesh.clone(), custom_mat.clone()),
            );
        }

        let entity = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 16,
                    character_id: 16,
                    name: "Spyro".to_string(),
                    is_villain: false,
                    model_path: Some("016_Spyro.bld".to_string()),
                    texture_path: Some("016_Spyro.arc".to_string()),
                },
                Hero,
            ))
            .id();

        app.add_systems(Update, actor_visual_sync_system);
        app.update();

        let assigned_mesh = app.world().get::<Mesh3d>(entity).unwrap();
        assert_eq!(assigned_mesh.0, custom_mesh);
        let assigned_mat = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .unwrap();
        assert_eq!(assigned_mat.0, custom_mat);

        // Test dynamic switch of character
        let foodfight_mesh = {
            let mut m = app.world_mut().resource_mut::<Assets<Mesh>>();
            m.add(ProceduralMesh::prop_cube(1.0).to_bevy())
        };
        let foodfight_mat = {
            let mut mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
            mats.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.8, 0.2),
                ..default()
            })
        };
        {
            let mut reg = app.world_mut().resource_mut::<CharacterVisualRegistry>();
            reg.cache.insert(
                "401:476_artichoke.bld".to_string(),
                (foodfight_mesh.clone(), foodfight_mat.clone()),
            );
        }

        {
            let mut ident = app.world_mut().get_mut::<ActorIdentity>(entity).unwrap();
            ident.character_id = 401;
            ident.name = "Food Fight".to_string();
            ident.model_path = Some("476_Artichoke.bld".to_string());
        }

        app.update();

        let updated_mesh = app.world().get::<Mesh3d>(entity).unwrap();
        assert_eq!(updated_mesh.0, foodfight_mesh);
        let updated_mat = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .unwrap();
        assert_eq!(updated_mat.0, foodfight_mat);
    }

    #[test]
    fn test_character_animation_and_skinning_pipeline() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<Assets<SkinnedMeshInverseBindposes>>();
        app.init_resource::<CharacterAssetServer>();
        app.init_resource::<CharacterVisualRegistry>();
        app.init_resource::<ModelVisualRegistry>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let hero_mesh = meshes.add(ProceduralMesh::prop_cube(0.5).to_bevy());
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let hero_mat = materials.add(StandardMaterial::default());

        let render_assets = RenderMeshAssets {
            courtyard_mesh: hero_mesh.clone(),
            courtyard_material: hero_mat.clone(),
            pedestal_mesh: hero_mesh.clone(),
            pedestal_material: hero_mat.clone(),
            hero_mesh: hero_mesh.clone(),
            hero_material: hero_mat.clone(),
            villain_mesh: hero_mesh.clone(),
            villain_material: hero_mat.clone(),
            prop_mesh: hero_mesh.clone(),
            prop_material: hero_mat.clone(),
            coin_mesh: hero_mesh.clone(),
            coin_material: hero_mat.clone(),
            gem_mesh: hero_mesh.clone(),
            gem_material: hero_mat.clone(),
            projectile_mesh: hero_mesh.clone(),
            projectile_material: hero_mat.clone(),
        };
        app.insert_resource(render_assets);

        let bone_0 = skylands_formats::Bone {
            name: "root".to_string(),
            parent: None,
            inverse_bind_pose: skylands_formats::mat4_identity(),
            local_transform: skylands_formats::mat4_identity(),
        };
        let mut bone_1_local = skylands_formats::mat4_identity();
        bone_1_local[13] = 1.0;
        let mut bone_1_ibp = skylands_formats::mat4_identity();
        bone_1_ibp[13] = -1.0;
        let bone_1 = skylands_formats::Bone {
            name: "head".to_string(),
            parent: Some(0),
            inverse_bind_pose: bone_1_ibp,
            local_transform: bone_1_local,
        };
        let skeleton = skylands_formats::Skeleton::new(vec![bone_0, bone_1], 0);

        let idle_clip = skylands_formats::AnimationClip::new(
            "idle",
            1.0,
            vec![
                skylands_formats::BoneTrack {
                    bone_index: 0,
                    positions: vec![
                        skylands_formats::Keyframe::new(0.0, [0.0, 0.0, 0.0]),
                        skylands_formats::Keyframe::new(0.5, [0.0, 0.5, 0.0]),
                        skylands_formats::Keyframe::new(1.0, [0.0, 0.0, 0.0]),
                    ],
                    rotations: vec![skylands_formats::Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0])],
                    scales: vec![skylands_formats::Keyframe::new(0.0, [1.0, 1.0, 1.0])],
                },
                skylands_formats::BoneTrack {
                    bone_index: 1,
                    positions: vec![skylands_formats::Keyframe::new(0.0, [0.0, 1.0, 0.0])],
                    rotations: vec![skylands_formats::Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0])],
                    scales: vec![skylands_formats::Keyframe::new(0.0, [1.0, 1.0, 1.0])],
                },
            ],
        );

        let mock_model = std::sync::Arc::new(skylands_formats::IgbModel {
            skeletons: vec![skeleton],
            animations: vec![idle_clip],
            ..default()
        });

        let key = format!("16:{}", "016_spyro.bld");
        {
            let mut reg = app.world_mut().resource_mut::<CharacterVisualRegistry>();
            reg.cache
                .insert(key.clone(), (hero_mesh.clone(), hero_mat.clone()));
            reg.models.insert(key, mock_model);
        }

        let entity = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 16,
                    character_id: 16,
                    name: "Spyro".to_string(),
                    is_villain: false,
                    model_path: Some("016_Spyro.bld".to_string()),
                    texture_path: None,
                },
                Hero,
            ))
            .id();

        app.add_systems(
            Update,
            (actor_visual_sync_system, character_animation_system),
        );
        app.update();

        let root_joint = {
            let skinned = app.world().get::<SkinnedMesh>(entity);
            assert!(skinned.is_some(), "Entity must have SkinnedMesh component");
            let skinned = skinned.unwrap();
            assert_eq!(skinned.joints.len(), 2, "2 joints spawned for 2 bones");

            let animator = app.world().get::<CharacterAnimator>(entity);
            assert!(
                animator.is_some(),
                "Entity must have CharacterAnimator component"
            );
            let animator = animator.unwrap();
            assert_eq!(animator.current_clip, "idle");
            assert_eq!(animator.joints.len(), 2);
            skinned.joints[0]
        };

        {
            let mut anim = app
                .world_mut()
                .get_mut::<CharacterAnimator>(entity)
                .unwrap();
            anim.elapsed = 0.5;
        }
        app.update();

        let root_tf = app.world().get::<Transform>(root_joint).unwrap();
        assert!(
            (root_tf.translation.y - 0.5).abs() < 1e-2,
            "Root joint animated to Y=0.5, got {}",
            root_tf.translation.y
        );
    }

    /// Spyro hero with a one-bone rig and two submeshes (body + wings, so the actor itself
    /// has no `Mesh3d`), plus `attachments` on that bone.
    fn skinned_character_app(attachments: Vec<AttachmentVisual>) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<Assets<SkinnedMeshInverseBindposes>>();
        app.init_resource::<CharacterVisualRegistry>();

        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(ProceduralMesh::prop_cube(0.5).to_bevy());
        let mat = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        app.insert_resource(RenderMeshAssets {
            courtyard_mesh: mesh.clone(),
            courtyard_material: mat.clone(),
            pedestal_mesh: mesh.clone(),
            pedestal_material: mat.clone(),
            hero_mesh: mesh.clone(),
            hero_material: mat.clone(),
            villain_mesh: mesh.clone(),
            villain_material: mat.clone(),
            prop_mesh: mesh.clone(),
            prop_material: mat.clone(),
            coin_mesh: mesh.clone(),
            coin_material: mat.clone(),
            gem_mesh: mesh.clone(),
            gem_material: mat.clone(),
            projectile_mesh: mesh.clone(),
            projectile_material: mat.clone(),
        });

        let bone = skylands_formats::Bone {
            name: "root".to_string(),
            parent: None,
            inverse_bind_pose: skylands_formats::mat4_identity(),
            local_transform: skylands_formats::mat4_identity(),
        };
        let model = std::sync::Arc::new(skylands_formats::IgbModel {
            skeletons: vec![skylands_formats::Skeleton::new(vec![bone], 0)],
            animations: vec![skylands_formats::AnimationClip::new("idle", 1.0, vec![])],
            ..default()
        });
        let key = "16:016_spyro.bld".to_string();
        {
            let mut reg = app.world_mut().resource_mut::<CharacterVisualRegistry>();
            reg.cache.insert(key.clone(), (mesh.clone(), mat.clone()));
            reg.submeshes.insert(
                key.clone(),
                vec![(mesh.clone(), mat.clone()), (mesh.clone(), mat.clone())],
            );
            reg.attachments.insert(key.clone(), attachments);
            reg.models.insert(key, model);
        }

        let entity = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 16,
                    character_id: 16,
                    name: "Spyro".to_string(),
                    is_villain: false,
                    model_path: Some("016_Spyro.bld".to_string()),
                    texture_path: None,
                },
                Hero,
            ))
            .id();
        app.add_systems(Update, actor_visual_sync_system);
        app.update();
        (app, entity)
    }

    #[test]
    fn test_multi_submesh_character_is_built_once() {
        let (mut app, entity) = skinned_character_app(Vec::new());
        let joints = app
            .world()
            .get::<CharacterAnimator>(entity)
            .unwrap()
            .joints
            .clone();
        let children = app.world().get::<Children>(entity).unwrap().len();

        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().get::<CharacterAnimator>(entity).unwrap().joints,
            joints,
            "visual must not be rebuilt every frame (animation restarts)"
        );
        assert_eq!(app.world().get::<Children>(entity).unwrap().len(), children);
    }

    #[test]
    fn test_attachment_follows_its_joint() {
        let mesh = Handle::<Mesh>::default();
        let mat = Handle::<StandardMaterial>::default();
        let (app, entity) = skinned_character_app(vec![AttachmentVisual {
            bone: 0,
            transform: Transform::from_xyz(0.0, 0.0, 0.9144),
            parts: vec![(mesh, mat)],
        }]);
        let joint = app.world().get::<CharacterAnimator>(entity).unwrap().joints[0];
        let children = app
            .world()
            .get::<Children>(joint)
            .expect("joint has the weapon");
        assert_eq!(children.len(), 1);
        let weapon = children[0];
        assert!(app
            .world()
            .get::<CharacterAttachmentEntity>(weapon)
            .is_some());
        assert!(app.world().get::<Mesh3d>(weapon).is_some());
        assert_eq!(
            app.world().get::<Transform>(weapon).unwrap().translation,
            Vec3::new(0.0, 0.0, 0.9144)
        );
    }

    #[test]
    fn test_resolve_clip_fallbacks() {
        let clip = |n: &str| skylands_formats::AnimationClip::new(n, 1.0, vec![]);
        let clips = vec![
            clip("combatidle"),
            clip("run"),
            clip("jumpstanding"),
            clip("falling"),
            clip("smashattack"),
            clip("spawn02"),
            clip("toy"),
        ];
        let name = |s: &str| resolve_clip(&clips, s).map(|c| c.name.as_str());
        assert_eq!(name("idle"), Some("combatidle"));
        assert_eq!(name("walk"), Some("run"));
        assert_eq!(name("jump"), Some("jumpstanding"));
        assert_eq!(name("fall"), Some("falling"));
        assert_eq!(name("attack_sweep"), Some("smashattack"));
        assert_eq!(name("attack_combo_2"), Some("smashattack"));
        assert_eq!(name("spawn"), Some("spawn02"));
        assert_eq!(name("toyalive"), Some("toy"));
        assert_eq!(name("unknown"), Some("combatidle"));
        assert!(resolve_clip(&[], "idle").is_none());
    }

    #[test]
    fn test_animator_follows_gameplay_state_outside_viewer() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        // Always inserted by the render plugin; its clip must not override gameplay.
        app.insert_resource(crate::ViewerSettings::default());
        app.add_systems(Update, character_animation_system);

        let entity = app
            .world_mut()
            .spawn((
                CharacterAnimator {
                    elapsed: 0.7,
                    ..CharacterAnimator::new(
                        skylands_formats::Skeleton::new(vec![], 0),
                        vec![
                            skylands_formats::AnimationClip::new("idle", 1.0, vec![]),
                            skylands_formats::AnimationClip::new("walk", 1.0, vec![]),
                        ],
                        default(),
                        vec![],
                    )
                },
                ActorAnimation {
                    current_clip: Some("walk".to_string()),
                    ..default()
                },
            ))
            .id();
        app.update();

        let animator = app.world().get::<CharacterAnimator>(entity).unwrap();
        assert_eq!(animator.current_clip, "walk");
        assert_eq!(animator.elapsed, 0.0, "clip change restarts playback");

        app.world_mut()
            .resource_mut::<crate::ViewerSettings>()
            .viewer_mode = true;
        app.update();
        let animator = app.world().get::<CharacterAnimator>(entity).unwrap();
        assert_eq!(
            animator.current_clip, "idle",
            "viewer clip wins in viewer mode"
        );
    }

    #[test]
    fn test_animator_follows_state_transitions_and_blends() {
        use skylands_formats::{
            AnimationClip, AnimationGraph, AnimationTransition, Bone, BoneTrack, Keyframe,
            Skeleton, AUTO_TRANSITION,
        };
        // One bone, each clip holding it at a different height.
        let clip = |name: &str, duration: f32, y: f32| {
            AnimationClip::new(
                name,
                duration,
                vec![BoneTrack {
                    bone_index: 0,
                    positions: vec![Keyframe::new(0.0, [0.0, y, 0.0])],
                    ..default()
                }],
            )
        };
        let transition = |name: &str, request, from: Vec<i32>, to, blend| AnimationTransition {
            name: name.to_string(),
            request,
            from,
            to,
            blend,
        };
        // Spyro's walk-from-idle path (`016_Spyro.bld`).
        let graph = AnimationGraph {
            states: vec![
                (0, "idle".into()),
                (1, "walk".into()),
                (99, "idletowalk".into()),
            ],
            transitions: vec![
                transition("", 1, vec![], 1, 0.064),
                transition("idle to idletowalk", 1, vec![0], 99, 0.2),
                transition("idletowalk to walk", AUTO_TRANSITION, vec![99], 1, 0.1),
            ],
        };
        let mut anim = CharacterAnimator::new(
            Skeleton::new(vec![Bone::default()], 0),
            vec![
                clip("idle", 1.0, 0.0),
                clip("walk", 1.0, 2.0),
                clip("idletowalk", 0.5, 1.0),
            ],
            graph,
            vec![],
        );
        assert_eq!((anim.playing.as_str(), anim.state), ("idle", Some(0)));

        anim.advance("walk", 1.0 / 60.0);
        assert_eq!(
            (anim.playing.as_str(), anim.state),
            ("idletowalk", Some(99))
        );
        anim.advance("walk", 0.1);
        let y = anim.pose()[0].translation.y;
        assert!((y - 0.5).abs() < 1e-4, "halfway idle -> idletowalk: {y}");

        anim.advance("walk", 0.4);
        assert_eq!(anim.playing, "walk", "idletowalk auto-advances at its end");
        anim.advance("walk", 0.1);
        assert_eq!(anim.blend, None);
        assert!((anim.pose()[0].translation.y - 2.0).abs() < 1e-4);

        // No specific transition from walk to idle here: plain switch, no blend.
        anim.advance("idle", 1.0 / 60.0);
        assert_eq!(
            (anim.playing.as_str(), anim.blend.is_none()),
            ("idle", true)
        );

        // Advance by state ID: state 1 has intermediate state 99 (idletowalk) from state 0 (idle)
        anim.advance_state(1, 1.0 / 60.0);
        assert_eq!(anim.playing, "idletowalk");
        assert_eq!(anim.state, Some(99));
        anim.advance_state(1, 0.5);
        assert_eq!(anim.playing, "walk");
        assert_eq!(anim.state, Some(1));
    }

    #[test]
    fn test_spyro_mount_and_load_visual_from_bld() {
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
            let server = CharacterAssetServer::default();
            {
                let mut loader = server.loader.lock().unwrap();
                let dir = p.parent().unwrap();
                loader.mount_dir(dir).expect("mount character dir");
            }

            let mut registry = CharacterVisualRegistry::default();
            let mut meshes = Assets::<Mesh>::default();
            let mut materials = Assets::<StandardMaterial>::default();
            let mut images = Assets::<Image>::default();

            let res = registry.get_or_load(
                16,
                Some("016_Spyro.bld"),
                Some(&server),
                &mut meshes,
                &mut materials,
                &mut images,
            );
            assert!(
                res.is_some(),
                "Spyro mesh and material must load from mounted BLD"
            );
            let (mesh_handle, mat_handle) = res.unwrap();
            let loaded_mesh = meshes.get(&mesh_handle).expect("mesh in assets");
            assert!(loaded_mesh.count_vertices() >= 3);
            assert!(
                loaded_mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some(),
                "Skinned character mesh must have JOINT_INDEX attribute"
            );
            assert!(
                loaded_mesh
                    .attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)
                    .is_some(),
                "Skinned character mesh must have JOINT_WEIGHT attribute"
            );
            let model_data = registry
                .get_model(16, Some("016_Spyro.bld"))
                .expect("model data in registry");
            assert!(!model_data.skeletons.is_empty(), "Skeleton present");
            assert!(!model_data.animations.is_empty(), "Animations present");
            assert!(model_data.animations.iter().any(|a| a.name == "idle"));
            let loaded_mat = materials.get(&mat_handle).expect("material in assets");
            assert!(
                loaded_mat.base_color_texture.is_some(),
                "Base color texture must be bound when texture is available"
            );
            assert_eq!(loaded_mat.base_color, Color::WHITE);
            assert_eq!(
                loaded_mat.cull_mode, None,
                "Character materials must disable backface culling"
            );
            assert!(
                loaded_mat.double_sided,
                "Character materials must be double-sided"
            );

            // Verify igImage2 extraction
            let loader = server.loader.lock().unwrap();
            let tex_res = loader.load_first_image_rgba("016_Spyro.bld");
            assert!(tex_res.is_ok(), "First image rgba extracted from Spyro BLD");
            let (w, h, rgba) = tex_res.unwrap();
            assert!(w > 0 && h > 0);
            assert_eq!(rgba.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn test_spyro_tail_wing_alpha_and_tiled_body() {
        let dir = ["assets/files/character", "../../assets/files/character"]
            .iter()
            .map(std::path::Path::new)
            .find(|d| d.join("016_Spyro.bld").exists());
        let Some(dir) = dir else { return };
        let server = CharacterAssetServer::default();
        server
            .loader
            .lock()
            .unwrap()
            .mount_dir(dir)
            .expect("mount character dir");

        let mut registry = CharacterVisualRegistry::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();
        registry
            .get_or_load(
                16,
                Some("016_Spyro.bld"),
                Some(&server),
                &mut meshes,
                &mut materials,
                &mut images,
            )
            .expect("load spyro");
        let subs = registry
            .get_submeshes(16, Some("016_Spyro.bld"))
            .expect("spyro submeshes");
        // Shiny, eyes, body, wings and the tail from the 9-bone tail rig.
        assert_eq!(subs.len(), 5);

        let mats: Vec<&StandardMaterial> = subs
            .iter()
            .map(|(_, m)| materials.get(m).expect("material"))
            .collect();
        let masked = mats
            .iter()
            .filter(|m| m.alpha_mode == AlphaMode::Mask(0.5))
            .count();
        assert_eq!(masked, 1, "only the wing texture has cut-outs");
        for mat in &mats {
            let tex = mat.base_color_texture.as_ref().expect("textured");
            let image = images.get(tex).expect("image");
            let bevy::image::ImageSampler::Descriptor(desc) = &image.sampler else {
                panic!("character textures need an explicit sampler");
            };
            assert!(matches!(
                desc.address_mode_u,
                bevy::image::ImageAddressMode::Repeat
            ));
            assert!(matches!(
                desc.address_mode_v,
                bevy::image::ImageAddressMode::Repeat
            ));
        }
    }

    #[test]
    fn test_universal_model_registry_loads_hats_and_items() {
        let root1 = std::path::Path::new("assets/files");
        let root2 = std::path::Path::new("../../assets/files");
        let root = if root1.exists() {
            Some(root1)
        } else if root2.exists() {
            Some(root2)
        } else {
            None
        };

        if let Some(r) = root {
            let server = CharacterAssetServer::default();
            {
                let mut loader = server.loader.lock().unwrap();
                let _ = loader.mount_dir(r);
            }

            let mut registry = ModelVisualRegistry::default();
            let mut meshes = Assets::<Mesh>::default();
            let mut materials = Assets::<StandardMaterial>::default();
            let mut images = Assets::<Image>::default();

            // 1. Hat loading
            let hat = registry.get_or_load_model(
                "outfit/Hat_114.bld",
                Some(0.5),
                None,
                Some(&server),
                &mut meshes,
                &mut materials,
                &mut images,
            );
            assert!(
                hat.is_some(),
                "Hat_114.bld should be loaded by ModelVisualRegistry"
            );

            // 2. Item loading
            let item = registry.get_or_load_model(
                "item/Item_HelpingHand.bld",
                Some(0.8),
                None,
                Some(&server),
                &mut meshes,
                &mut materials,
                &mut images,
            );
            assert!(
                item.is_some(),
                "Item_HelpingHand.bld should be loaded by ModelVisualRegistry"
            );

            // 3. Villain loading
            let villain = registry.get_or_load_model(
                "character/1001_ChompyMage.bld",
                Some(2.0),
                None,
                Some(&server),
                &mut meshes,
                &mut materials,
                &mut images,
            );
            assert!(
                villain.is_some(),
                "1001_ChompyMage.bld should be loaded by ModelVisualRegistry"
            );
        }
    }

    fn create_test_archive(
        path: &std::path::Path,
        files: &[(&str, &[u8])],
    ) -> Result<(), std::io::Error> {
        let mut buf = Vec::new();
        let num_files = files.len() as u32;

        buf.extend_from_slice(b"IGA\x1a");
        buf.extend_from_slice(&11u32.to_le_bytes()); // version
        buf.extend_from_slice(&100u32.to_le_bytes()); // toc_size
        buf.extend_from_slice(&num_files.to_le_bytes()); // num_files
        buf.extend_from_slice(&2048u32.to_le_bytes()); // sector_size
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        let mut nt_payload = Vec::new();
        let mut nt_offsets = Vec::new();
        let mut cur_offset = num_files * 4;

        for (name, _) in files {
            nt_offsets.push(cur_offset);
            let name_bytes = name.as_bytes();
            cur_offset += name_bytes.len() as u32 + 1;
        }

        for off in &nt_offsets {
            nt_payload.extend_from_slice(&off.to_le_bytes());
        }
        for (name, _) in files {
            nt_payload.extend_from_slice(name.as_bytes());
            nt_payload.push(0);
        }

        let mut total_file_data = 0usize;
        for (_, data) in files {
            total_file_data += data.len() + 64;
        }
        let nt_offset = (2048 + total_file_data as u64).div_ceil(2048) * 2048;
        let nt_size = nt_payload.len() as u64;
        buf.extend_from_slice(&nt_offset.to_le_bytes());
        buf.extend_from_slice(&nt_size.to_le_bytes());

        let mut hashed_files: Vec<(u32, &str, &[u8])> = files
            .iter()
            .map(|(name, data)| (skylands_formats::hash_path(name), *name, *data))
            .collect();
        hashed_files.sort_by_key(|(h, _, _)| *h);

        for (h, _, _) in &hashed_files {
            buf.extend_from_slice(&h.to_le_bytes());
        }

        let mut cur_file_offset = 2048u32;
        for (_, _, data) in &hashed_files {
            buf.extend_from_slice(&cur_file_offset.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
            cur_file_offset += 64 + data.len() as u32;
        }

        buf.resize(2048, 0);

        let mut cur_write_offset = 2048;
        for (_, _, data) in &hashed_files {
            buf.resize(cur_write_offset, 0);
            buf.extend_from_slice(data);
            cur_write_offset += 64 + data.len();
        }

        buf.resize(nt_offset as usize, 0);
        buf.extend_from_slice(&nt_payload);

        std::fs::write(path, &buf)?;
        Ok(())
    }

    #[test]
    fn test_model_visual_registry_binds_decoded_texture() {
        let tmp_dir = std::env::temp_dir().join("skylands_texture_bind_test");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let bld_path = tmp_dir.join("test_model.bld");
        let arc_path = tmp_dir.join("test_model.arc");

        // Simple IGB model with 1 vertex
        let mut igb_bytes = Vec::new();
        igb_bytes.extend_from_slice(b"IGBF");
        igb_bytes.extend_from_slice(&8u32.to_le_bytes());
        igb_bytes.extend_from_slice(&1u32.to_le_bytes());
        igb_bytes.extend_from_slice(&0u32.to_le_bytes());
        igb_bytes.extend_from_slice(&1u32.to_le_bytes());
        igb_bytes.extend_from_slice(&0u32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());

        create_test_archive(&bld_path, &[("level.bld", &igb_bytes)]).expect("write test bld");

        // 8x8 tile = 32 bytes CMPR (red color)
        let mut cmpr_data = vec![0u8; 32 * 16 * 16]; // sufficient for 128x128
        for chunk in cmpr_data.as_chunks_mut::<8>().0 {
            chunk[0..2].copy_from_slice(&0xF800u16.to_be_bytes());
            chunk[2..4].copy_from_slice(&0x0000u16.to_be_bytes());
        }
        create_test_archive(&arc_path, &[("textures/model.png.igb.tex.igz", &cmpr_data)])
            .expect("write test arc");

        let mut loader = AssetLoader::default();
        loader.mount_dir(&tmp_dir).expect("mount dir");
        let server = CharacterAssetServer::new(loader);

        let mut registry = ModelVisualRegistry::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();

        let res = registry.get_or_load_model(
            "test_model.bld",
            Some(1.0),
            Some(Color::srgb(0.5, 0.5, 0.5)),
            Some(&server),
            &mut meshes,
            &mut materials,
            &mut images,
        );

        assert!(res.is_some(), "Model must load");
        let (_mesh_handle, mat_handle) = res.unwrap();
        let mat = materials.get(&mat_handle).expect("material exists");
        assert!(
            mat.base_color_texture.is_some(),
            "Decoded texture must be bound to base_color_texture"
        );
        assert_eq!(mat.base_color, Color::WHITE);
        assert_eq!(mat.cull_mode, None);
        assert!(mat.double_sided);

        let tex_handle = mat.base_color_texture.as_ref().unwrap();
        let image = images
            .get(tex_handle)
            .expect("image exists in Assets<Image>");
        assert_eq!(image.texture_descriptor.size.width, 128);
        assert_eq!(image.texture_descriptor.size.height, 128);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_setup_environment_and_assets_level_loading() {
        let tmp_dir = std::env::temp_dir().join("skylands_level_env_test");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let bld_path = tmp_dir.join("Level_Hub_2014_01.bld");
        let arc_path = tmp_dir.join("Level_Hub_2014_01.arc");

        let mut igb_bytes = Vec::new();
        igb_bytes.extend_from_slice(b"IGBF");
        igb_bytes.extend_from_slice(&8u32.to_le_bytes());
        igb_bytes.extend_from_slice(&1u32.to_le_bytes());
        igb_bytes.extend_from_slice(&0u32.to_le_bytes());
        igb_bytes.extend_from_slice(&1u32.to_le_bytes());
        igb_bytes.extend_from_slice(&0u32.to_le_bytes());
        for _ in 0..12 {
            igb_bytes.extend_from_slice(&1.0f32.to_le_bytes());
        }

        create_test_archive(&bld_path, &[("level.bld", &igb_bytes)]).expect("write level bld");

        let mut cmpr_data = vec![0u8; 32 * 16 * 16];
        for chunk in cmpr_data.as_chunks_mut::<8>().0 {
            chunk[0..2].copy_from_slice(&0x07E0u16.to_be_bytes());
            chunk[2..4].copy_from_slice(&0x0000u16.to_be_bytes());
        }
        create_test_archive(&arc_path, &[("textures/hub.png.igb.tex.igz", &cmpr_data)])
            .expect("write level arc");

        let mut loader = AssetLoader::default();
        loader.mount_dir(&tmp_dir).expect("mount dir");
        let server = CharacterAssetServer::new(loader);

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.insert_resource(server);
        app.insert_resource(ActiveLevel::new("Level_Hub_2014_01.bld"));
        app.add_systems(Startup, setup_environment_and_assets);
        app.add_systems(Update, level_render_mesh_sync_system);

        app.update();

        assert!(app.world().get_resource::<RenderMeshAssets>().is_some());
        let mesh_count = app.world_mut().query::<&Mesh3d>().iter(app.world()).count();
        assert!(mesh_count >= 2, "Must spawn level geometry and pedestal");

        let level_render_count = app
            .world_mut()
            .query_filtered::<Entity, With<LevelRenderMesh>>()
            .iter(app.world())
            .count();
        assert_eq!(
            level_render_count, 1,
            "Level mesh must have LevelRenderMesh"
        );

        // Change active level to a new level
        app.world_mut().resource_mut::<ActiveLevel>().name = "Level_313_Tutorial.bld".to_string();
        app.update();

        // Level render mesh system ran and maintained level mesh
        let level_render_after = app
            .world_mut()
            .query_filtered::<Entity, With<LevelRenderMesh>>()
            .iter(app.world())
            .count();
        assert_eq!(
            level_render_after, 1,
            "Level render mesh updated on level swap"
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_villain_visual_sync_and_mesh_sanitization() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<Assets<SkinnedMeshInverseBindposes>>();
        app.init_resource::<CharacterAssetServer>();
        app.init_resource::<CharacterVisualRegistry>();
        app.init_resource::<ModelVisualRegistry>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let fallback_mesh = meshes.add(ProceduralMesh::prop_cube(0.5).to_bevy());
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let fallback_mat = materials.add(StandardMaterial::default());

        let render_assets = RenderMeshAssets {
            courtyard_mesh: fallback_mesh.clone(),
            courtyard_material: fallback_mat.clone(),
            pedestal_mesh: fallback_mesh.clone(),
            pedestal_material: fallback_mat.clone(),
            hero_mesh: fallback_mesh.clone(),
            hero_material: fallback_mat.clone(),
            villain_mesh: fallback_mesh.clone(),
            villain_material: fallback_mat.clone(),
            prop_mesh: fallback_mesh.clone(),
            prop_material: fallback_mat.clone(),
            coin_mesh: fallback_mesh.clone(),
            coin_material: fallback_mat.clone(),
            gem_mesh: fallback_mesh.clone(),
            gem_material: fallback_mat.clone(),
            projectile_mesh: fallback_mesh.clone(),
            projectile_material: fallback_mat.clone(),
        };
        app.insert_resource(render_assets);

        // 1. Verify ensure_unskinned_mesh strips joint attributes
        let mut skinned_bevy_mesh = ProceduralMesh::prop_cube(1.0).to_bevy();
        skinned_bevy_mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            bevy::render::mesh::VertexAttributeValues::Uint16x4(vec![
                [0, 0, 0, 0];
                skinned_bevy_mesh
                    .count_vertices()
            ]),
        );
        skinned_bevy_mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_WEIGHT,
            vec![[1.0, 0.0, 0.0, 0.0]; skinned_bevy_mesh.count_vertices()],
        );
        let skinned_handle = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(skinned_bevy_mesh);
        assert!(app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&skinned_handle)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
            .is_some());

        let safe_handle = {
            let mut m = app.world_mut().resource_mut::<Assets<Mesh>>();
            ensure_unskinned_mesh(&skinned_handle, &mut m)
        };
        assert_ne!(safe_handle, skinned_handle);
        let sanitized = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&safe_handle)
            .unwrap();
        assert!(sanitized.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_none());
        assert!(sanitized.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_none());

        // 2. Spawn villain with mock skinned model
        let bone = skylands_formats::Bone {
            name: "root".to_string(),
            parent: None,
            inverse_bind_pose: skylands_formats::mat4_identity(),
            local_transform: skylands_formats::mat4_identity(),
        };
        let skel = skylands_formats::Skeleton::new(vec![bone], 0);
        let mock_model = std::sync::Arc::new(skylands_formats::IgbModel {
            skeletons: vec![skel],
            animations: vec![skylands_formats::AnimationClip::new("idle", 1.0, vec![])],
            ..default()
        });

        let villain_mesh = {
            let mut m = app.world_mut().resource_mut::<Assets<Mesh>>();
            m.add(ProceduralMesh::prop_cube(1.0).to_bevy())
        };
        let villain_mat = {
            let mut mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
            mats.add(StandardMaterial::default())
        };

        let key = "1010:1010_kaos.bld".to_string();
        {
            let mut reg = app.world_mut().resource_mut::<CharacterVisualRegistry>();
            reg.cache
                .insert(key.clone(), (villain_mesh.clone(), villain_mat.clone()));
            reg.models.insert(key, mock_model);
        }

        let villain_entity = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 2010,
                    character_id: 1010,
                    name: "Kaos".to_string(),
                    is_villain: true,
                    model_path: Some("1010_Kaos.bld".to_string()),
                    texture_path: None,
                },
                skylands_core::Villain,
            ))
            .id();

        app.add_systems(Update, actor_visual_sync_system);
        app.update();

        let skinned = app.world().get::<SkinnedMesh>(villain_entity);
        assert!(
            skinned.is_some(),
            "Villain entity must receive SkinnedMesh when skeleton is present"
        );
        let animator = app.world().get::<CharacterAnimator>(villain_entity);
        assert!(
            animator.is_some(),
            "Villain entity must receive CharacterAnimator"
        );
    }

    #[test]
    fn test_character_multi_submesh_rendering_and_bind_pose_identity() {
        let targets = [
            "assets/files/character/016_Spyro.bld",
            "../../assets/files/character/016_Spyro.bld",
        ];
        let p = match targets
            .iter()
            .map(std::path::Path::new)
            .find(|p| p.exists())
        {
            Some(p) => p,
            None => return,
        };

        let server = CharacterAssetServer::default();
        {
            let mut loader = server.loader.lock().unwrap();
            let dir = p.parent().unwrap();
            loader.mount_dir(dir).expect("mount character dir");
        }

        let mut registry = CharacterVisualRegistry::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();

        let _ = registry.get_or_load(
            16,
            Some("016_Spyro.bld"),
            Some(&server),
            &mut meshes,
            &mut materials,
            &mut images,
        );

        let submeshes = registry
            .get_submeshes(16, Some("016_Spyro.bld"))
            .expect("submeshes loaded");
        assert_eq!(
            submeshes.len(),
            5,
            "Spyro has 5 submeshes (body, head, details, wings, tail)"
        );

        let model = registry.get_model(16, Some("016_Spyro.bld")).unwrap();
        let skel = model.primary_skeleton().expect("primary skeleton");
        assert_eq!(skel.bones.len(), 50);

        // Verify mathematical identity: joint world * IBP == Identity at bind pose
        for (i, b) in skel.bones.iter().enumerate() {
            let bind_world = skylands_formats::igb::mat4_affine_inverse(&b.inverse_bind_pose);
            let id = skylands_formats::igb::mat4_mul(&bind_world, &b.inverse_bind_pose);
            for (k, &val) in id.iter().enumerate() {
                let expected = if k % 5 == 0 { 1.0 } else { 0.0 };
                assert!(
                    (val - expected).abs() < 1e-4,
                    "bone {i} ({}) bind_world * IBP must be Identity, got {id:?}",
                    b.name
                );
            }
        }
    }

    #[test]
    fn test_character_animation_system_routes_actor_animation_state() {
        use skylands_formats::igb::{AnimationClip, AnimationGraph, Bone, Skeleton};
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);

        let entity = app
            .world_mut()
            .spawn((
                CharacterAnimator {
                    playing: "idle".to_string(),
                    current_clip: "idle".to_string(),
                    state: Some(0),
                    elapsed: 0.0,
                    skeleton: Skeleton::new(vec![Bone::default()], 0),
                    animations: vec![
                        AnimationClip::new("idle", 1.0, vec![]),
                        AnimationClip::new("talk", 1.0, vec![]),
                    ],
                    graph: AnimationGraph {
                        states: vec![(0, "idle".into()), (9, "talk".into())],
                        transitions: vec![],
                    },
                    blend: None,
                    joints: vec![],
                },
                ActorAnimation {
                    current_clip: Some("idle".to_string()),
                    time: 0.0,
                    speed: 1.0,
                    state: Some(9),
                },
            ))
            .id();

        app.add_systems(Update, character_animation_system);
        app.update();

        let anim = app.world().get::<CharacterAnimator>(entity).unwrap();
        assert_eq!(
            anim.playing, "talk",
            "character_animation_system must advance to state 9 (talk)"
        );
        assert_eq!(anim.state, Some(9));
    }
}
