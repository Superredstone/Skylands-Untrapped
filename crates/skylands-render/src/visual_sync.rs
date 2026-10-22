use crate::mesh_conversion::{
    combine_or_normalize_model_meshes, igb_model_to_bevy_mesh, Mesh as ProceduralMesh,
};
use crate::texture_conversion::create_colored_material;
use bevy::prelude::*;
use bevy::utils::HashMap;
pub use skylands_core::GameAssetServer as CharacterAssetServer;
use skylands_core::{
    get_character_info, resolve_character_model_archive, resolve_model_archive, ActiveLevel,
    ActorIdentity, Collectible, Element, Hero, Projectile, Prop, Villain,
};
#[cfg(test)]
use skylands_formats::AssetLoader;

pub fn element_to_color(element: Element) -> Color {
    let (r, g, b) = element.rgb();
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
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
        let bevy_mesh = igb_model_to_bevy_mesh(&model, target_height);
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

#[derive(Resource, Clone, Default)]
pub struct CharacterVisualRegistry {
    pub cache: HashMap<String, (Handle<Mesh>, Handle<StandardMaterial>)>,
}

impl CharacterVisualRegistry {
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
        let normalized_bevy_mesh = igb_model_to_bevy_mesh(&model, Some(1.8));
        let mesh_handle = meshes.add(normalized_bevy_mesh);

        let mat_color = if character_id == 16 {
            Color::srgb(0.42, 0.18, 0.60)
        } else {
            let (_, elem) = get_character_info(character_id);
            element_to_color(elem)
        };

        let mut base_color_texture = None;
        if let Ok((w, h, rgba)) = loader.load_first_image_rgba(&bld_name) {
            let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
            base_color_texture = Some(images.add(img));
        } else if let Ok((w, h, rgba)) = loader.load_companion_texture_rgba(&bld_name, None) {
            let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
            base_color_texture = Some(images.add(img));
        }

        let color = if base_color_texture.is_some() {
            Color::WHITE
        } else {
            mat_color
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
        self.cache.insert(cache_key, pair.clone());
        Some(pair)
    }
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

#[allow(clippy::too_many_arguments)]
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
                let resolved = if loader.has_file(&level_name) {
                    Some(level_name.clone())
                } else {
                    resolve_model_archive("level", &level_name)
                };

                if let Some(path) = resolved {
                    if let Ok(model) = loader.load_igb(&path) {
                        if !model.meshes.is_empty() {
                            let is_level = path.to_ascii_lowercase().contains("level");
                            let instances = model.evaluate_scene_graph();

                            let mut material_groups: HashMap<
                                String,
                                Vec<&skylands_formats::igb::SceneInstance>,
                            > = HashMap::new();
                            if !instances.is_empty() && !model.geometries.is_empty() {
                                for inst in &instances {
                                    if let Some(geo) = model.geometries.get(inst.geometry_index) {
                                        // ponytail: per-geometry material names never match a
                                        // texture file, so levels merge into one draw call; split by
                                        // real texture once tfbRuntimeMaterial pairing exists.
                                        let mat_name = if is_level {
                                            "level".to_string()
                                        } else {
                                            geo.mesh
                                                .as_ref()
                                                .and_then(|m| m.material_name.clone())
                                                .unwrap_or_else(|| format!("mat_{}", geo.name))
                                        };
                                        material_groups.entry(mat_name).or_default().push(inst);
                                    }
                                }
                            }

                            if material_groups.is_empty() {
                                let mut base_color_texture = None;
                                if let Ok((w, h, rgba)) =
                                    loader.load_companion_texture_rgba(&path, None)
                                {
                                    let img =
                                        crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
                                    base_color_texture = Some(images.add(img));
                                } else if !is_level {
                                    if let Ok((w, h, rgba)) = loader.load_first_image_rgba(&path) {
                                        let img = crate::texture_conversion::rgba8_to_bevy_image(
                                            w, h, rgba,
                                        );
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

                                let combined_mesh =
                                    combine_or_normalize_model_meshes(&model.meshes, None);
                                let mesh_handle = meshes.add(combined_mesh);

                                commands.spawn((
                                    Mesh3d(mesh_handle),
                                    MeshMaterial3d(level_material),
                                    Transform::from_xyz(0.0, 0.0, 0.0),
                                ));

                                loaded_real_level = true;
                            } else {
                                for (mat_name, group_instances) in material_groups {
                                    let mut total_verts = 0;
                                    let mut total_idxs = 0;
                                    for inst in &group_instances {
                                        if let Some(geo) = model.geometries.get(inst.geometry_index)
                                        {
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
                                        if let Some(geo) = model.geometries.get(inst.geometry_index)
                                        {
                                            if let Some(ref m) = geo.mesh {
                                                if m.vertices.is_empty() || m.indices.is_empty() {
                                                    continue;
                                                }
                                                for v in &m.vertices {
                                                    positions.push(
                                                        skylands_formats::igb::transform_point(
                                                            &inst.world_transform,
                                                            v.position,
                                                        ),
                                                    );
                                                    normals.push(
                                                        skylands_formats::igb::transform_dir(
                                                            &inst.world_transform,
                                                            v.normal,
                                                        ),
                                                    );
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
                                    bevy_mesh
                                        .insert_indices(bevy::render::mesh::Indices::U32(indices));

                                    let mut base_color_texture = None;
                                    if let Ok((w, h, rgba)) =
                                        loader.load_companion_texture_rgba(&path, Some(&mat_name))
                                    {
                                        let img = crate::texture_conversion::rgba8_to_bevy_image(
                                            w, h, rgba,
                                        );
                                        base_color_texture = Some(images.add(img));
                                    }

                                    let mat_color = if base_color_texture.is_some() {
                                        Color::WHITE
                                    } else {
                                        Color::srgb(0.72, 0.70, 0.65)
                                    };

                                    let submesh_mat = materials.add(StandardMaterial {
                                        base_color: mat_color,
                                        base_color_texture,
                                        perceptual_roughness: 0.85,
                                        reflectance: 0.20,
                                        cull_mode: None,
                                        double_sided: true,
                                        ..default()
                                    });

                                    let mesh_handle = meshes.add(bevy_mesh);
                                    commands.spawn((
                                        Mesh3d(mesh_handle),
                                        MeshMaterial3d(submesh_mat),
                                        Transform::from_xyz(0.0, 0.0, 0.0),
                                    ));

                                    loaded_real_level = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !loaded_real_level {
        // Spawn courtyard ground fallback
        commands.spawn((
            Mesh3d(courtyard_mesh.clone()),
            MeshMaterial3d(courtyard_material.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
    }

    // Spawn pedestal in world
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
    mut viewer_settings: Option<ResMut<crate::ViewerSettings>>,
    actor_query: Query<
        (
            Entity,
            &ActorIdentity,
            Option<&Hero>,
            Option<&Villain>,
            Option<&Prop>,
        ),
        Or<(Without<Mesh3d>, Changed<ActorIdentity>)>,
    >,
    collectible_query: Query<(Entity, &Collectible), Without<Mesh3d>>,
    projectile_query: Query<Entity, (With<Projectile>, Without<Mesh3d>)>,
) {
    let Some(assets) = assets else {
        return;
    };

    for (entity, ident, maybe_hero, maybe_villain, maybe_prop) in &actor_query {
        if maybe_hero.is_some() {
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

            if let Some((mesh, mat)) = custom_visual {
                if let Some(ref mut vs) = viewer_settings {
                    if vs.viewer_mode {
                        if let Some(m) = meshes.get(&mesh) {
                            vs.vert_count = m.count_vertices();
                            vs.tri_count = m
                                .indices()
                                .map(|idx| idx.len() / 3)
                                .unwrap_or(vs.vert_count / 3);
                        }
                    }
                }
                commands
                    .entity(entity)
                    .insert((Mesh3d(mesh), MeshMaterial3d(mat)));
            } else {
                commands.entity(entity).insert((
                    Mesh3d(assets.hero_mesh.clone()),
                    MeshMaterial3d(assets.hero_material.clone()),
                ));
            }
        } else if maybe_villain.is_some() {
            let villain_visual = model_registry.as_mut().and_then(|reg| {
                let resolved_archive;
                let model_name = if let Some(path) = ident.model_path.as_deref() {
                    path
                } else if ident.character_id > 0 {
                    resolved_archive = resolve_character_model_archive(ident.character_id)?;
                    &resolved_archive
                } else {
                    return None;
                };
                let (_, elem) = get_character_info(ident.character_id);
                let col = Some(element_to_color(elem));
                reg.get_or_load_model(
                    model_name,
                    Some(2.0),
                    col,
                    asset_server.as_deref(),
                    &mut meshes,
                    &mut materials,
                    &mut images,
                )
            });

            if let Some((mesh, mat)) = villain_visual {
                commands
                    .entity(entity)
                    .insert((Mesh3d(mesh), MeshMaterial3d(mat)));
            } else {
                commands.entity(entity).insert((
                    Mesh3d(assets.villain_mesh.clone()),
                    MeshMaterial3d(assets.villain_material.clone()),
                ));
            }
        } else if maybe_prop.is_some() {
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

    for entity in &projectile_query {
        commands.entity(entity).insert((
            Mesh3d(assets.projectile_mesh.clone()),
            MeshMaterial3d(assets.projectile_material.clone()),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        app.update();

        assert!(app.world().get_resource::<RenderMeshAssets>().is_some());
        let mesh_count = app.world_mut().query::<&Mesh3d>().iter(app.world()).count();
        assert!(mesh_count >= 2, "Must spawn level geometry and pedestal");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
