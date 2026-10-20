use crate::mesh_conversion::{combine_or_normalize_model_meshes, Mesh as ProceduralMesh};
use crate::texture_conversion::create_colored_material;
use bevy::prelude::*;
use bevy::utils::HashMap;
use skylands_core::{
    get_character_info, resolve_character_model_archive, resolve_model_archive, ActorIdentity,
    Collectible, Element, Hero, Projectile, Prop, Villain,
};
use skylands_formats::AssetLoader;
use std::sync::{Arc, Mutex};

pub fn element_to_color(element: Element) -> Color {
    let (r, g, b) = element.rgb();
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

#[derive(Resource, Clone, Default)]
pub struct ModelVisualRegistry {
    pub cache: HashMap<String, (Handle<Mesh>, Handle<StandardMaterial>)>,
}

impl ModelVisualRegistry {
    pub fn get_or_load_model(
        &mut self,
        identifier: &str,
        target_height: Option<f32>,
        base_color: Option<Color>,
        asset_server: Option<&CharacterAssetServer>,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
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
        let bevy_mesh = combine_or_normalize_model_meshes(&model.meshes, target_height);
        let mesh_handle = meshes.add(bevy_mesh);

        let base_color_texture = None;
        let arc_name = resolved_path
            .strip_suffix(".bld")
            .unwrap_or(&resolved_path)
            .to_string()
            + ".arc";
        if loader.has_file(&arc_name) {
            if let Ok(rgba) = loader.load_cmpr_rgba(&arc_name, 128, 128) {
                let img = crate::texture_conversion::rgba8_to_bevy_image(128, 128, rgba);
                // ponytail: texture asset addition deferred; standard material with elemental color
                let _ = img;
            }
        }
        if let Ok((w, h, rgba)) = loader.load_first_image_rgba(&resolved_path) {
            let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
            let _ = img;
        }

        let color = base_color.unwrap_or(Color::srgb(0.7, 0.7, 0.7));
        let material_handle = materials.add(StandardMaterial {
            base_color: color,
            base_color_texture,
            perceptual_roughness: 0.8,
            reflectance: 0.2,
            ..default()
        });

        let pair = (mesh_handle, material_handle);
        self.cache.insert(key, pair.clone());
        Some(pair)
    }
}

#[derive(Resource, Clone, Default)]
pub struct CharacterVisualRegistry {
    pub cache: HashMap<u16, (Handle<Mesh>, Handle<StandardMaterial>)>,
}

impl CharacterVisualRegistry {
    pub fn get_or_load(
        &mut self,
        character_id: u16,
        model_path: Option<&str>,
        asset_server: Option<&CharacterAssetServer>,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Option<(Handle<Mesh>, Handle<StandardMaterial>)> {
        if let Some(pair) = self.cache.get(&character_id) {
            return Some(pair.clone());
        }

        let asset_server = asset_server?;
        let loader = asset_server.loader.lock().ok()?;

        let bld_name = model_path
            .map(|s| s.to_string())
            .or_else(|| resolve_character_model_archive(character_id))?;

        let model = loader.load_igb(&bld_name).ok()?;
        let normalized_bevy_mesh = combine_or_normalize_model_meshes(&model.meshes, Some(1.8));
        let mesh_handle = meshes.add(normalized_bevy_mesh);

        let mat_color = if character_id == 16 {
            Color::srgb(0.42, 0.18, 0.60)
        } else {
            let (_, elem) = get_character_info(character_id);
            element_to_color(elem)
        };

        let base_color_texture = None;
        if let Ok((w, h, rgba)) = loader.load_first_image_rgba(&bld_name) {
            let img = crate::texture_conversion::rgba8_to_bevy_image(w, h, rgba);
            let _ = img;
        }

        let material_handle = materials.add(StandardMaterial {
            base_color: mat_color,
            base_color_texture,
            perceptual_roughness: 0.8,
            reflectance: 0.2,
            ..default()
        });

        let pair = (mesh_handle, material_handle);
        self.cache.insert(character_id, pair.clone());
        Some(pair)
    }
}

#[derive(Resource, Clone, Default)]
pub struct CharacterAssetServer {
    pub loader: Arc<Mutex<AssetLoader>>,
}

impl CharacterAssetServer {
    pub fn new(loader: AssetLoader) -> Self {
        Self {
            loader: Arc::new(Mutex::new(loader)),
        }
    }

    pub fn from_arc(loader: Arc<Mutex<AssetLoader>>) -> Self {
        Self { loader }
    }
}

impl From<AssetLoader> for CharacterAssetServer {
    fn from(loader: AssetLoader) -> Self {
        Self::new(loader)
    }
}

impl From<Arc<Mutex<AssetLoader>>> for CharacterAssetServer {
    fn from(loader: Arc<Mutex<AssetLoader>>) -> Self {
        Self { loader }
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

pub fn setup_environment_and_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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

    // Spawn courtyard ground and pedestal in world
    commands.spawn((
        Mesh3d(courtyard_mesh.clone()),
        MeshMaterial3d(courtyard_material.clone()),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

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
                )
            });

            if let Some((mesh, mat)) = custom_visual {
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

        // When archive missing, registry returns None gracefully
        let res = registry.get_or_load(
            16,
            Some("016_Spyro.bld"),
            Some(&server),
            &mut meshes,
            &mut materials,
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
            reg.cache
                .insert(16, (custom_mesh.clone(), custom_mat.clone()));
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
            reg.cache
                .insert(401, (foodfight_mesh.clone(), foodfight_mat.clone()));
        }

        {
            let mut ident = app.world_mut().get_mut::<ActorIdentity>(entity).unwrap();
            ident.character_id = 401;
            ident.name = "Food Fight".to_string();
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

            let res = registry.get_or_load(
                16,
                Some("016_Spyro.bld"),
                Some(&server),
                &mut meshes,
                &mut materials,
            );
            assert!(
                res.is_some(),
                "Spyro mesh and material must load from mounted BLD"
            );
            let (mesh_handle, mat_handle) = res.unwrap();
            let loaded_mesh = meshes.get(&mesh_handle).expect("mesh in assets");
            assert!(loaded_mesh.count_vertices() >= 3);
            let loaded_mat = materials.get(&mat_handle).expect("material in assets");
            assert_eq!(loaded_mat.base_color, Color::srgb(0.42, 0.18, 0.60));

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

            // 1. Hat loading
            let hat = registry.get_or_load_model(
                "outfit/Hat_114.bld",
                Some(0.5),
                None,
                Some(&server),
                &mut meshes,
                &mut materials,
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
            );
            assert!(
                villain.is_some(),
                "1001_ChompyMage.bld should be loaded by ModelVisualRegistry"
            );
        }
    }
}
