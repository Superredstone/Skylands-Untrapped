use clap::Parser;
use log::{info, warn};
use skylands_core::{
    get_character_voice_path, ActorKind, AudioEngine, CollisionMesh, FixedTimestep, GameState,
    GameStateMachine, InputMapper, PortalGameplay, World, WorldAudioCue,
};
use skylands_extractor::Extractor;
use skylands_formats::{decode_cmpr, AssetLoader};
use skylands_portal::protocol::{decode_portal_response, PortalEvent, PortalZone, REPORT_SIZE};
use skylands_portal::{MockPortal, SkylanderTag, UsbPortal};
use skylands_render::{
    mat4, Camera, CameraVolume, CameraVolumeMode, EnvironmentScene, EnvironmentSubmesh, FreeCam,
    GpuMesh, GpuTexture, InstanceRaw, LightUniform, Mesh, ModelInstance, RenderAppHandler,
    Renderer, UiBatch, WindowApp, WindowConfig,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::EventLoop;
use winit::keyboard::KeyCode;
use winit::window::Window;

#[derive(Parser, Debug)]
#[command(
    name = "skylands-untrapped",
    version = "0.1.0",
    about = "Native port runtime for Skylanders Trap Team"
)]
struct Args {
    #[arg(short, long, default_value = "rom.eu.wbfs")]
    rom: PathBuf,

    #[arg(short, long, default_value = "assets")]
    assets_dir: PathBuf,

    #[arg(
        long,
        help = "Force mock portal mode instead of physical USB Traptanium Portal"
    )]
    mock_portal: bool,

    #[arg(
        long,
        help = "Run in headless mode without window creation (for CLI / tests / CI)"
    )]
    headless: bool,

    #[arg(
        long,
        default_value = "120",
        help = "Maximum ticks to execute in headless mode"
    )]
    ticks: u64,
}

pub enum ActivePortal {
    Usb(UsbPortal),
    Mock(MockPortal),
}

impl ActivePortal {
    pub fn poll_events(&mut self) -> Vec<PortalEvent> {
        match self {
            ActivePortal::Usb(usb) => {
                let mut events = Vec::new();
                while let Ok(Some(pkt)) = usb.poll_packet() {
                    if let Some(ev) = decode_portal_response(&pkt) {
                        events.push(ev);
                    }
                }
                events
            }
            ActivePortal::Mock(mock) => mock.poll_events(),
        }
    }

    pub fn send_packet(&mut self, pkt: &[u8; REPORT_SIZE]) {
        match self {
            ActivePortal::Usb(usb) => {
                let _ = usb.send_packet(pkt);
            }
            ActivePortal::Mock(mock) => {
                mock.send_command(pkt);
            }
        }
    }
}

pub struct GameEngine {
    pub portal: ActivePortal,
    pub gameplay: PortalGameplay,
    pub world: World,
    pub state: GameStateMachine,
    pub input: InputMapper,
    pub audio: AudioEngine,
    pub timestep: FixedTimestep,
    pub last_frame: Instant,
    pub assets: Arc<AssetLoader>,
}

impl GameEngine {
    pub fn new(portal: ActivePortal, assets: Arc<AssetLoader>) -> Self {
        let mut state = GameStateMachine::new();
        state.set_state(GameState::Hub {
            academy_area: "Skylander Academy Courtyard".into(),
        });

        let mut world = World::new();
        world.triggers.push(skylands_core::ZoneTrigger {
            id: 1,
            position: [0.0, 0.0, -8.0],
            radius: 2.5,
            target_zone: "Academy Library".to_string(),
            target_spawn: [0.0, 0.0, 5.0],
            prompt: "Press A to Enter Academy".to_string(),
        });

        Self {
            portal,
            gameplay: PortalGameplay::new(),
            world,
            state,
            input: InputMapper::new(),
            audio: AudioEngine::new(),
            timestep: FixedTimestep::new(),
            last_frame: Instant::now(),
            assets,
        }
    }

    pub fn tick(&mut self) {
        self.input.poll_gamepads();

        // 1. Poll portal hardware / mock events
        let prev_state = self.state.current().clone();
        let events = self.portal.poll_events();
        for ev in events {
            let outgoing =
                self.gameplay
                    .handle_event(ev, &mut self.world, &mut self.state, &mut self.audio);
            for pkt in outgoing {
                self.portal.send_packet(&pkt);
            }
        }

        // If villain capture just triggered, attempt to load and stream real dialogue
        if let GameState::VillainCapture { villain_id, .. } = self.state.current() {
            let just_entered = match prev_state {
                GameState::VillainCapture {
                    villain_id: prev_id,
                    ..
                } => prev_id != *villain_id,
                _ => true,
            };
            if just_entered {
                if let Some(voice_path) = get_character_voice_path(*villain_id) {
                    if let Ok(clip) = self.assets.load_audio(voice_path) {
                        self.audio.portal.clear();
                        self.audio.play_villain_clip(&clip);
                    }
                }
            }
        }

        // 2. Stream villain audio packets to Traptanium speaker if active
        if self.audio.portal.is_streaming() {
            if let Some(pkt) = self.audio.portal.pop_packet() {
                self.portal.send_packet(&pkt);
            }
        }

        // 3. Update world actors with 60Hz input
        self.world.update(self.timestep.dt(), self.input.state());
        for cue in self.world.drain_audio_cues() {
            match cue {
                WorldAudioCue::PickupCollected => self.audio.on_pickup_collected(),
                WorldAudioCue::HeroAttack(kind) => self.audio.on_hero_attack(kind),
                WorldAudioCue::AttackHit => self.audio.on_attack_hit(),
                WorldAudioCue::PropDestroyed => self.audio.on_prop_destroyed(),
                WorldAudioCue::ZoneEntered(ref to_zone) => {
                    self.audio.system.play_sfx_tone(660.0, 0.25);
                    let from_zone = self.world.active_zone.clone();
                    self.state.set_state(GameState::ZoneTransition {
                        from_zone,
                        to_zone: to_zone.clone(),
                        timer: 0.0,
                        fade_duration: 1.0,
                    });
                }
            }
        }

        // 4. Update state machine
        self.state.tick(self.timestep.dt());
    }

    pub fn update(&mut self) -> u32 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame);
        self.last_frame = now;

        let mut ticks = 0;
        self.timestep.update(elapsed, |_count, _dt| {
            ticks += 1;
        });

        for _ in 0..ticks {
            self.tick();
        }

        ticks
    }

    pub fn load_environment_collision(&mut self, path: &str) {
        let candidates = [
            path.to_string(),
            format!("{}.igb", path),
            format!("{}.bld", path),
            "Level_Hub_2014_01.bld".to_string(),
            "level_hub_2014_01.bld".to_string(),
            "Arena_Mesh_300.bld".to_string(),
            "wii_courtyard.bld".to_string(),
            "levels/hub.igb".to_string(),
            "maps/courtyard.igb".to_string(),
        ];
        for cand in &candidates {
            if self.assets.has_file(cand) {
                if let Ok(models) = self.assets.load_level_models(cand) {
                    let mut combined_mesh = CollisionMesh::from_triangles(Vec::new());
                    let mut markers = Vec::new();
                    for model in &models {
                        let sub = CollisionMesh::from_igb(model);
                        combined_mesh = CollisionMesh::from_triangles(
                            combined_mesh
                                .triangles
                                .into_iter()
                                .chain(sub.triangles)
                                .collect(),
                        );
                        markers.extend(model.extract_level_markers());
                    }
                    if !markers.is_empty() {
                        self.world.load_level_entities(&markers);
                    }
                    if combined_mesh.triangles.is_empty() {
                        for comp in [
                            "Arena_Mesh_300.bld",
                            "challengeMesh_outdoor.bld",
                            "challengeMesh_Ruins.bld",
                        ] {
                            if self.assets.has_file(comp) {
                                if let Ok(comp_models) = self.assets.load_level_models(comp) {
                                    for m in &comp_models {
                                        let sub = CollisionMesh::from_igb(m);
                                        combined_mesh = CollisionMesh::from_triangles(
                                            combined_mesh
                                                .triangles
                                                .into_iter()
                                                .chain(sub.triangles)
                                                .collect(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if !combined_mesh.triangles.is_empty() {
                        self.world.environment.mesh = combined_mesh;
                        return;
                    }
                }
            }
        }
        self.world.environment.mesh = CollisionMesh::synthetic_courtyard();
        if self.world.triggers.is_empty() {
            self.world.triggers.push(skylands_core::ZoneTrigger {
                id: 1,
                position: [0.0, 0.0, -8.0],
                radius: 2.5,
                target_zone: "Academy Library".to_string(),
                target_spawn: [0.0, 0.0, 5.0],
                prompt: "Press A to Enter Academy".to_string(),
            });
        }
    }
}

pub struct AppRuntime {
    pub engine: GameEngine,
    pub assets: Arc<AssetLoader>,
    pub renderer: Option<Renderer>,
    pub camera: Camera,
    pub pedestal_mesh: Option<GpuMesh>,
    pub actor_mesh: Option<GpuMesh>,
    pub skybox_mesh: Option<GpuMesh>,
    pub prop_mesh: Option<GpuMesh>,
    pub coin_mesh: Option<GpuMesh>,
    pub gem_mesh: Option<GpuMesh>,
    pub actor_texture: Option<GpuTexture>,
    pub dynamic_textures: HashMap<String, GpuTexture>,
    pub dynamic_meshes: HashMap<String, GpuMesh>,
    pub dynamic_models: HashMap<String, ModelInstance>,
    pub environment: Option<EnvironmentScene>,
    pub last_active_hero_id: Option<u32>,
    pub anim_time: f32,
    pub ui_batch: UiBatch,
    pub free_cam: FreeCam,
    pub free_cam_keys: HashSet<KeyCode>,
}

impl AppRuntime {
    pub fn new(engine: GameEngine, assets: Arc<AssetLoader>) -> Self {
        let mut camera = Camera::new(1280.0 / 720.0);
        // Camera volumes:
        // 1. Hub Courtyard fixed overhead view
        camera.volumes.push(CameraVolume {
            name: "Courtyard Overhead".to_string(),
            bounds_min: [-15.0, -5.0, -15.0],
            bounds_max: [15.0, 15.0, 15.0],
            mode: CameraVolumeMode::FixedOverhead {
                height: 8.0,
                distance: 10.0,
            },
        });
        // 2. Portal gate fixed cinematic angle
        camera.volumes.push(CameraVolume {
            name: "Portal Cinematic".to_string(),
            bounds_min: [-3.0, -2.0, -12.0],
            bounds_max: [3.0, 10.0, -5.0],
            mode: CameraVolumeMode::FixedCinematic {
                eye: [0.0, 3.0, -4.0],
                target: [0.0, 1.0, -8.0],
            },
        });

        Self {
            engine,
            assets,
            renderer: None,
            camera,
            pedestal_mesh: None,
            actor_mesh: None,
            skybox_mesh: None,
            prop_mesh: None,
            coin_mesh: None,
            gem_mesh: None,
            actor_texture: None,
            dynamic_textures: HashMap::new(),
            dynamic_meshes: HashMap::new(),
            dynamic_models: HashMap::new(),
            environment: None,
            last_active_hero_id: None,
            anim_time: 0.0,
            ui_batch: UiBatch::new(),
            free_cam: FreeCam::default(),
            free_cam_keys: HashSet::new(),
        }
    }

    pub fn resolve_material_texture(&mut self, material_name: &str) -> Option<GpuTexture> {
        let key = material_name.replace('\\', "/").to_ascii_lowercase();
        if let Some(tex) = self.dynamic_textures.get(&key) {
            return Some(tex.clone());
        }

        let candidates = [
            format!("{}.png.igb.tex.igz", key),
            format!("{}.cmpr", key),
            format!("{}.png", key),
            format!("textures/{}.png", key),
            format!("textures/{}.cmpr", key),
            format!("textures/characters/{}.png", key),
            format!("textures/characters/{}.cmpr", key),
            key.clone(),
        ];

        for cand in &candidates {
            if self.assets.has_file(cand) {
                if let Some(tex) = self.load_cmpr_texture(cand, 64, 64) {
                    return Some(tex.clone());
                }
            }
        }
        None
    }

    pub fn load_model(&mut self, path: &str) -> Option<&ModelInstance> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !self.dynamic_models.contains_key(&key) {
            self.renderer.as_ref()?;
            let igb = self.assets.load_igb(path).ok()?;
            let mut submesh_defs = Vec::new();

            for geo in &igb.geometries {
                if let Some(mesh) = Mesh::from_geometry(geo) {
                    let tex = mesh
                        .material_name
                        .as_deref()
                        .and_then(|mat| self.resolve_material_texture(mat));
                    submesh_defs.push((mesh, tex));
                }
            }
            if submesh_defs.is_empty() {
                for igb_mesh in &igb.meshes {
                    let mesh = Mesh::from_igb(igb_mesh);
                    let tex = mesh
                        .material_name
                        .as_deref()
                        .and_then(|mat| self.resolve_material_texture(mat));
                    submesh_defs.push((mesh, tex));
                }
            }

            if !submesh_defs.is_empty() {
                let renderer = self.renderer.as_ref()?;
                let mut submeshes = Vec::with_capacity(submesh_defs.len());
                for (mesh, tex) in submesh_defs {
                    let gpu_mesh = GpuMesh::from_mesh(&renderer.device, &mesh);
                    submeshes.push((gpu_mesh, tex));
                }
                self.dynamic_models
                    .insert(key.clone(), ModelInstance::new(submeshes));
            }
        }
        self.dynamic_models.get(&key)
    }

    pub fn load_mesh(&mut self, path: &str) -> Option<&GpuMesh> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !self.dynamic_meshes.contains_key(&key) {
            let renderer = self.renderer.as_ref()?;
            if let Ok(igb) = self.assets.load_igb(path) {
                // First inspect parsed igGeometry nodes if present
                let mut resolved_mesh = None;
                for geo in &igb.geometries {
                    if let Some(m) = Mesh::from_geometry(geo) {
                        resolved_mesh = Some(m);
                        break;
                    }
                }
                // Fallback to top-level meshes
                let mesh = resolved_mesh.or_else(|| igb.meshes.first().map(Mesh::from_igb));
                if let Some(mesh) = mesh {
                    let gpu_mesh = GpuMesh::from_mesh(&renderer.device, &mesh);
                    self.dynamic_meshes.insert(key.clone(), gpu_mesh);
                }
            }
        }
        self.dynamic_meshes.get(&key)
    }

    pub fn load_geometry_nodes(&mut self, path: &str) -> Vec<String> {
        let mut loaded_keys = Vec::new();
        let renderer = match self.renderer.as_ref() {
            Some(r) => r,
            None => return loaded_keys,
        };

        if let Ok(igb) = self.assets.load_igb(path) {
            for (i, geo) in igb.geometries.iter().enumerate() {
                if let Some(mesh) = Mesh::from_geometry(geo) {
                    let subkey = format!("{}:{}", path.replace('\\', "/").to_ascii_lowercase(), i);
                    let gpu_mesh = GpuMesh::from_mesh(&renderer.device, &mesh);
                    self.dynamic_meshes.insert(subkey.clone(), gpu_mesh);
                    loaded_keys.push(subkey);
                }
            }
        }
        loaded_keys
    }

    pub fn load_cmpr_texture(
        &mut self,
        path: &str,
        width: u32,
        height: u32,
    ) -> Option<&GpuTexture> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !self.dynamic_textures.contains_key(&key) {
            let renderer = self.renderer.as_ref()?;
            if let Ok(rgba) = self.assets.load_cmpr_rgba(path, width, height) {
                if let Ok(tex) = GpuTexture::from_rgba(
                    &renderer.device,
                    &renderer.queue,
                    &rgba,
                    width,
                    height,
                    Some(path),
                ) {
                    self.dynamic_textures.insert(key.clone(), tex);
                }
            }
        }
        self.dynamic_textures.get(&key)
    }

    pub fn load_environment(&mut self, path: &str) -> Option<&EnvironmentScene> {
        self.renderer.as_ref()?;
        let candidates = [
            path.to_string(),
            format!("{}.igb", path),
            format!("{}.bld", path),
            "Level_Hub_2014_01.bld".to_string(),
            "level_hub_2014_01.bld".to_string(),
            "Arena_Mesh_300.bld".to_string(),
            "wii_courtyard.bld".to_string(),
            "levels/hub.igb".to_string(),
            "maps/courtyard.igb".to_string(),
        ];

        let mut resolved_models = Vec::new();
        let mut loaded_name = path.to_string();
        for cand in &candidates {
            if self.assets.has_file(cand) {
                if let Ok(models) = self.assets.load_level_models(cand) {
                    resolved_models = models;
                    loaded_name = cand.clone();
                    break;
                }
            }
        }

        if !resolved_models.is_empty() {
            let mut combined_collision = CollisionMesh::from_triangles(Vec::new());
            let mut submesh_defs = Vec::new();
            let mut markers = Vec::new();

            for igb in &resolved_models {
                let sub_col = CollisionMesh::from_igb(igb);
                combined_collision = CollisionMesh::from_triangles(
                    combined_collision
                        .triangles
                        .into_iter()
                        .chain(sub_col.triangles)
                        .collect(),
                );
                markers.extend(igb.extract_level_markers());

                let instances = igb.evaluate_scene_graph();
                for (geo_idx, geo) in igb.geometries.iter().enumerate() {
                    if let Some(mesh) = Mesh::from_geometry(geo) {
                        let tex = mesh
                            .material_name
                            .as_deref()
                            .and_then(|mat_name| self.resolve_material_texture(mat_name));

                        let matching_instances: Vec<InstanceRaw> = instances
                            .iter()
                            .filter(|inst| inst.geometry_index == geo_idx)
                            .map(|inst| InstanceRaw::from_flat_slice(&inst.world_transform))
                            .collect();

                        let inst_list = if matching_instances.is_empty() {
                            vec![InstanceRaw::default()]
                        } else {
                            matching_instances
                        };

                        submesh_defs.push((mesh, tex, inst_list));
                    }
                }

                if igb.geometries.is_empty() {
                    for igb_mesh in &igb.meshes {
                        let mesh = Mesh::from_igb(igb_mesh);
                        let tex = mesh
                            .material_name
                            .as_deref()
                            .and_then(|mat_name| self.resolve_material_texture(mat_name));
                        submesh_defs.push((mesh, tex, vec![InstanceRaw::default()]));
                    }
                }
            }

            if submesh_defs.is_empty() {
                for comp_cand in [
                    "Arena_Mesh_300.bld",
                    "challengeMesh_outdoor.bld",
                    "challengeMesh_Ruins.bld",
                ] {
                    if self.assets.has_file(comp_cand) {
                        if let Ok(comp_models) = self.assets.load_level_models(comp_cand) {
                            for comp_igb in &comp_models {
                                let sub_col = CollisionMesh::from_igb(comp_igb);
                                combined_collision = CollisionMesh::from_triangles(
                                    combined_collision
                                        .triangles
                                        .into_iter()
                                        .chain(sub_col.triangles)
                                        .collect(),
                                );
                                for geo in &comp_igb.geometries {
                                    if let Some(mesh) = Mesh::from_geometry(geo) {
                                        let tex =
                                            mesh.material_name.as_deref().and_then(|mat_name| {
                                                self.resolve_material_texture(mat_name)
                                            });
                                        submesh_defs.push((
                                            mesh,
                                            tex,
                                            vec![InstanceRaw::default()],
                                        ));
                                    }
                                }
                            }
                            if !submesh_defs.is_empty() {
                                break;
                            }
                        }
                    }
                }
            }

            if !markers.is_empty() {
                self.engine.world.load_level_entities(&markers);

                if let Some(renderer) = &self.renderer {
                    for m in &markers {
                        if let Some(ref igb_mesh) = m.mesh {
                            let key = m.name.to_ascii_lowercase();
                            if !self.dynamic_meshes.contains_key(&key) {
                                let render_mesh = Mesh::from_igb(igb_mesh);
                                let gpu_mesh = GpuMesh::from_mesh(&renderer.device, &render_mesh);
                                self.dynamic_meshes.insert(key.clone(), gpu_mesh);
                            }
                            for actor in &mut self.engine.world.actors {
                                if actor.name == m.name && actor.model_path.is_none() {
                                    actor.model_path = Some(key.clone());
                                }
                            }
                        }
                    }
                }

                if let Some(spawn) = markers.iter().find(|m| {
                    let l = m.name.to_ascii_lowercase();
                    l.starts_with("sp_hero")
                        || l.starts_with("spawn_start")
                        || l.starts_with("sp_start")
                }) {
                    self.camera.set_immediate(
                        [
                            spawn.position[0],
                            spawn.position[1] + 4.5,
                            spawn.position[2] + 7.5,
                        ],
                        spawn.position,
                    );
                }
            }

            if !combined_collision.triangles.is_empty() {
                self.engine.world.environment.mesh = combined_collision;
            } else {
                self.engine.world.environment.mesh = CollisionMesh::synthetic_courtyard();
            }

            let renderer = self.renderer.as_ref()?;
            let mut submeshes = Vec::with_capacity(submesh_defs.len());
            for (mesh, tex, inst_list) in submesh_defs {
                let gpu_mesh = GpuMesh::from_mesh(&renderer.device, &mesh);
                submeshes.push(EnvironmentSubmesh {
                    mesh: gpu_mesh,
                    texture: tex,
                    instances: inst_list,
                });
            }

            self.environment = Some(EnvironmentScene::new(loaded_name, submeshes));
        } else {
            self.engine.world.environment.mesh = CollisionMesh::synthetic_courtyard();
            // Default courtyard triggers if no level file loaded
            if self.engine.world.triggers.is_empty() {
                self.engine.world.triggers.push(skylands_core::ZoneTrigger {
                    id: 1,
                    position: [0.0, 0.0, -8.0],
                    radius: 2.5,
                    target_zone: "Academy Library".to_string(),
                    target_spawn: [0.0, 0.0, 5.0],
                    prompt: "Press A to Enter Academy".to_string(),
                });
            }
            let renderer = self.renderer.as_ref()?;
            self.environment = Some(EnvironmentScene::synthetic_courtyard(&renderer.device));
        }

        self.environment.as_ref()
    }

    pub fn build_hud_overlay(
        engine: &GameEngine,
        camera: &Camera,
        width: f32,
        height: f32,
        ui_batch: &mut UiBatch,
    ) {
        ui_batch.clear();
        let hud = engine.world.hud_state(&engine.state);

        // 1. Top-Left: Hero Status
        if let Some(name) = &hud.hero_name {
            // Element badge diamond
            let elem_color = if let Some(elem) = hud.hero_element {
                let (r, g, b) = elem.rgb();
                [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
            } else {
                [0.5, 0.5, 0.5, 1.0]
            };
            ui_batch.draw_diamond(36.0, 36.0, 16.0, [0.1, 0.1, 0.1, 0.85]);
            ui_batch.draw_diamond(36.0, 36.0, 12.0, elem_color);

            // Hero Name Label
            ui_batch.draw_text(1, name, 60.0, 20.0, 14.0, [1.0, 1.0, 1.0, 1.0]);

            // Health bar container (200x16 px)
            ui_batch.draw_rect([60.0, 38.0, 204.0, 18.0], [0.1, 0.1, 0.1, 0.9]);
            ui_batch.draw_rect([62.0, 40.0, 200.0, 14.0], [0.25, 0.25, 0.25, 1.0]);

            // Health fill
            let health_ratio = if hud.max_health > 0 {
                (hud.health as f32 / hud.max_health as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let fill_w = 200.0 * health_ratio;
            let fill_color = if health_ratio > 0.5 {
                [0.1, 0.85, 0.2, 1.0] // Green
            } else if health_ratio > 0.2 {
                [0.95, 0.75, 0.1, 1.0] // Yellow
            } else {
                [0.9, 0.15, 0.15, 1.0] // Red
            };
            if fill_w > 0.0 {
                ui_batch.draw_rect([62.0, 40.0, fill_w, 14.0], fill_color);
            }

            // Health text
            let hp_str = format!("{}/{}", hud.health, hud.max_health);
            ui_batch.draw_text(1, &hp_str, 66.0, 42.0, 10.0, [1.0, 1.0, 1.0, 1.0]);
        }

        // 2. Top-Right: Coin Counter
        let coin_x = (width - 160.0).max(280.0);
        // Background pill
        ui_batch.draw_rect([coin_x - 10.0, 18.0, 150.0, 32.0], [0.1, 0.1, 0.1, 0.85]);
        // Coin icon (gold diamond)
        ui_batch.draw_diamond(coin_x + 6.0, 34.0, 9.0, [1.0, 0.85, 0.1, 1.0]);
        // Coin tally text
        let coin_str = format!("{} COINS", hud.coins);
        ui_batch.draw_text(
            1,
            &coin_str,
            coin_x + 22.0,
            26.0,
            14.0,
            [1.0, 0.9, 0.2, 1.0],
        );

        // 3. Center / Bottom: Villain Capture Banner
        if let Some((vname, progress)) = hud.villain_capture {
            let banner_w = 420.0_f32.min(width - 40.0);
            let banner_h = 70.0;
            let banner_x = (width - banner_w) * 0.5;
            let banner_y = 90.0;

            // Purple / dark backdrop panel
            ui_batch.draw_rect(
                [banner_x, banner_y, banner_w, banner_h],
                [0.18, 0.05, 0.28, 0.92],
            );
            ui_batch.draw_rect(
                [
                    banner_x + 2.0,
                    banner_y + 2.0,
                    banner_w - 4.0,
                    banner_h - 4.0,
                ],
                [0.08, 0.02, 0.15, 0.95],
            );

            // Title: "TRAPPING VILLAIN: <NAME>"
            let banner_title = format!("TRAPPING VILLAIN: {}", vname.to_uppercase());
            ui_batch.draw_text(
                1,
                &banner_title,
                banner_x + 16.0,
                banner_y + 14.0,
                13.0,
                [0.9, 0.6, 1.0, 1.0],
            );

            // Animated progress bar
            let bar_x = banner_x + 16.0;
            let bar_y = banner_y + 38.0;
            let bar_w = banner_w - 32.0;
            let bar_h = 16.0;

            ui_batch.draw_rect([bar_x, bar_y, bar_w, bar_h], [0.2, 0.1, 0.3, 1.0]);

            let fill_w = bar_w * progress.clamp(0.0, 1.0);
            if fill_w > 0.0 {
                ui_batch.draw_rect(
                    [bar_x, bar_y, fill_w, bar_h],
                    [0.85, 0.2, 0.95, 1.0], // Magic/Trap magenta glow
                );
            }

            let pct_str = format!("{}%", (progress * 100.0) as u32);
            ui_batch.draw_text(
                1,
                &pct_str,
                bar_x + bar_w * 0.5 - 12.0,
                bar_y + 2.0,
                11.0,
                [1.0, 1.0, 1.0, 1.0],
            );
        }

        // 4. Floating Damage Popups
        for popup in &engine.world.damage_popups {
            if let Some([sx, sy]) = camera.project_to_screen(popup.world_pos, (width, height)) {
                let y_offset = popup.timer * 40.0;
                let alpha = (1.0 - (popup.timer / popup.max_lifetime)).clamp(0.0, 1.0);
                let color = [
                    popup.color[0],
                    popup.color[1],
                    popup.color[2],
                    popup.color[3] * alpha,
                ];
                ui_batch.draw_text(1, &popup.text, sx, sy - y_offset, 14.0, color);
            }
        }

        // 5. Interaction prompt banner at screen bottom when near a trigger
        if let Some(prompt) = &hud.prompt {
            let banner_w = 460.0_f32.min(width - 40.0);
            let banner_h = 44.0;
            let banner_x = (width - banner_w) * 0.5;
            let banner_y = height - 70.0;

            // Outer dark frame
            ui_batch.draw_rect(
                [banner_x, banner_y, banner_w, banner_h],
                [0.1, 0.12, 0.18, 0.9],
            );
            // Inner accent frame
            ui_batch.draw_rect(
                [
                    banner_x + 2.0,
                    banner_y + 2.0,
                    banner_w - 4.0,
                    banner_h - 4.0,
                ],
                [0.15, 0.22, 0.32, 0.95],
            );

            // Button badge icon (A)
            let badge_x = banner_x + 24.0;
            let badge_y = banner_y + 22.0;
            ui_batch.draw_diamond(badge_x, badge_y, 14.0, [0.1, 0.6, 0.9, 1.0]);
            ui_batch.draw_text(
                1,
                "A",
                badge_x - 4.0,
                badge_y - 7.0,
                14.0,
                [1.0, 1.0, 1.0, 1.0],
            );

            // Prompt text
            ui_batch.draw_text(
                1,
                prompt,
                banner_x + 48.0,
                banner_y + 14.0,
                14.0,
                [1.0, 0.95, 0.7, 1.0],
            );
        }

        // 6. Screen fade overlay (black quad with alpha fade_alpha) during GameState::ZoneTransition
        if let Some((_, _, timer, duration)) = hud.transition {
            if duration > 0.0 {
                let half_dur = duration * 0.5;
                let fade_alpha = if timer < half_dur {
                    (timer / half_dur).clamp(0.0, 1.0)
                } else {
                    (1.0 - (timer - half_dur) / half_dur).clamp(0.0, 1.0)
                };
                ui_batch.draw_rect([0.0, 0.0, width, height], [0.0, 0.0, 0.0, fade_alpha]);
            }
        }
    }

    pub fn render_hud(
        engine: &GameEngine,
        camera: &Camera,
        free_cam_enabled: bool,
        width: f32,
        height: f32,
        ui_batch: &mut UiBatch,
    ) {
        Self::build_hud_overlay(engine, camera, width, height, ui_batch);

        if free_cam_enabled {
            let banner_text = "[FREE CAM - F12 TO EXIT]";
            let char_size = 14.0;
            let banner_w = banner_text.len() as f32 * char_size + 32.0;
            let banner_h = 28.0;
            let banner_x = (width - banner_w) * 0.5;
            let banner_y = 12.0;

            ui_batch.draw_rect(
                [banner_x, banner_y, banner_w, banner_h],
                [0.1, 0.1, 0.1, 0.85],
            );
            ui_batch.draw_rect(
                [
                    banner_x + 2.0,
                    banner_y + 2.0,
                    banner_w - 4.0,
                    banner_h - 4.0,
                ],
                [0.8, 0.2, 0.2, 0.9],
            );
            ui_batch.draw_text(
                1,
                banner_text,
                banner_x + 16.0,
                banner_y + 7.0,
                char_size,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    }
}

impl RenderAppHandler for AppRuntime {
    fn on_init(&mut self, window: Arc<Window>) {
        info!("Initializing wgpu render pipeline on surface window...");
        match pollster::block_on(Renderer::new(window)) {
            Ok(renderer) => {
                let pedestal = Mesh::portal_pedestal(1.5, 0.2, 32);
                let hero_quad = Mesh::unit_quad();
                let skybox = Mesh::skybox_cube(80.0);
                let prop_cube = Mesh::prop_cube(0.8);
                let coin = Mesh::collectible_coin(0.35, 0.08, 16);
                let gem = Mesh::collectible_gem(0.3, 0.6);

                let gpu_pedestal = GpuMesh::from_mesh(&renderer.device, &pedestal);
                let gpu_hero = GpuMesh::from_mesh(&renderer.device, &hero_quad);
                let gpu_skybox = GpuMesh::from_mesh(&renderer.device, &skybox);
                let gpu_prop = GpuMesh::from_mesh(&renderer.device, &prop_cube);
                let gpu_coin = GpuMesh::from_mesh(&renderer.device, &coin);
                let gpu_gem = GpuMesh::from_mesh(&renderer.device, &gem);

                // Decode 8x8 CMPR demo texture (cyan / orange blocks)
                let mut cmpr_data = vec![0u8; 32];
                for (b, col) in [
                    (0, 0x07FFu16),
                    (1, 0xFD20u16),
                    (2, 0xFD20u16),
                    (3, 0x07FFu16),
                ] {
                    let base = b * 8;
                    cmpr_data[base..base + 2].copy_from_slice(&col.to_be_bytes());
                    cmpr_data[base + 2..base + 4].copy_from_slice(&0x0000u16.to_be_bytes());
                }
                if let Ok(rgba) = decode_cmpr(&cmpr_data, 8, 8) {
                    if let Ok(tex) = GpuTexture::from_rgba(
                        &renderer.device,
                        &renderer.queue,
                        &rgba,
                        8,
                        8,
                        Some("Actor CMPR Texture"),
                    ) {
                        self.actor_texture = Some(tex);
                    }
                }

                self.camera.aspect =
                    renderer.config.width as f32 / renderer.config.height.max(1) as f32;
                self.pedestal_mesh = Some(gpu_pedestal);
                self.actor_mesh = Some(gpu_hero);
                self.skybox_mesh = Some(gpu_skybox);
                self.prop_mesh = Some(gpu_prop);
                self.coin_mesh = Some(gpu_coin);
                self.gem_mesh = Some(gpu_gem);
                self.renderer = Some(renderer);

                self.load_environment("levels/hub.igb");

                // Dynamically resolve meshes from VFS if available
                if self.assets.has_file("models/hero.igb") {
                    let _ = self.load_mesh("models/hero.igb");
                }
                info!("Render pipeline successfully initialized.");
            }
            Err(e) => {
                warn!("Failed to initialize wgpu renderer: {}", e);
            }
        }
    }

    fn on_resize(&mut self, width: u32, height: u32) {
        if height > 0 {
            self.camera.aspect = width as f32 / height as f32;
        }
        if let Some(r) = &mut self.renderer {
            r.resize(width, height);
        }
    }

    fn on_key(&mut self, key_code: KeyCode, state: bool) {
        if key_code == KeyCode::F12 && state {
            self.free_cam.enabled = !self.free_cam.enabled;
            if self.free_cam.enabled {
                self.free_cam = FreeCam::from_camera(self.camera.eye, self.camera.target);
                self.free_cam.enabled = true;
                self.engine.input.state.reset();
                self.free_cam_keys.clear();
            } else {
                self.free_cam_keys.clear();
            }
            return;
        }

        if self.free_cam.enabled {
            match key_code {
                KeyCode::KeyW
                | KeyCode::KeyS
                | KeyCode::KeyA
                | KeyCode::KeyD
                | KeyCode::KeyQ
                | KeyCode::KeyE
                | KeyCode::Space
                | KeyCode::ShiftLeft
                | KeyCode::ShiftRight
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
                | KeyCode::ArrowLeft
                | KeyCode::ArrowRight => {
                    if state {
                        self.free_cam_keys.insert(key_code);
                    } else {
                        self.free_cam_keys.remove(&key_code);
                    }
                }
                _ => {}
            }
            return;
        }

        self.engine.input.handle_key_code(key_code, state);
    }

    fn on_mouse_motion(&mut self, delta_x: f32, delta_y: f32) {
        if self.free_cam.enabled {
            let sens = self.free_cam.sensitivity;
            self.free_cam.rotate(delta_x * sens, -delta_y * sens);
        }
    }

    fn on_update(&mut self) {
        self.engine.update();
    }

    fn on_render(&mut self) {
        let dt = 1.0 / 60.0;
        self.anim_time += dt;

        if self.engine.world.active_hero_id != self.last_active_hero_id {
            self.last_active_hero_id = self.engine.world.active_hero_id;
            let active_keys: std::collections::HashSet<String> = self
                .engine
                .world
                .actors
                .iter()
                .filter_map(|a| a.model_path.as_ref())
                .map(|p| p.replace('\\', "/").to_ascii_lowercase())
                .collect();
            self.dynamic_models
                .retain(|k, _| active_keys.contains(k) || k == "models/hero.igb");
            self.dynamic_meshes
                .retain(|k, _| active_keys.contains(k) || k == "models/hero.igb");
        }

        let preload_paths: Vec<(String, Option<String>)> = self
            .engine
            .world
            .actors
            .iter()
            .filter(|a| a.active)
            .filter_map(|a| {
                a.model_path
                    .as_ref()
                    .map(|p| (p.clone(), a.texture_path.clone()))
            })
            .collect();

        for (model_path, tex_path) in preload_paths {
            let _ = self.load_model(&model_path);
            let _ = self.load_mesh(&model_path);
            if let Some(tp) = tex_path {
                let _ = self.load_cmpr_texture(&tp, 64, 64);
            }
        }

        if let Some(r) = &mut self.renderer {
            if self.free_cam.enabled {
                let mut move_vec = [0.0f32; 3];
                if self.free_cam_keys.contains(&KeyCode::KeyW)
                    || self.free_cam_keys.contains(&KeyCode::ArrowUp)
                {
                    move_vec[2] += 1.0;
                }
                if self.free_cam_keys.contains(&KeyCode::KeyS)
                    || self.free_cam_keys.contains(&KeyCode::ArrowDown)
                {
                    move_vec[2] -= 1.0;
                }
                if self.free_cam_keys.contains(&KeyCode::KeyD)
                    || self.free_cam_keys.contains(&KeyCode::ArrowRight)
                {
                    move_vec[0] += 1.0;
                }
                if self.free_cam_keys.contains(&KeyCode::KeyA)
                    || self.free_cam_keys.contains(&KeyCode::ArrowLeft)
                {
                    move_vec[0] -= 1.0;
                }
                if self.free_cam_keys.contains(&KeyCode::KeyE)
                    || self.free_cam_keys.contains(&KeyCode::Space)
                {
                    move_vec[1] += 1.0;
                }
                if self.free_cam_keys.contains(&KeyCode::KeyQ)
                    || self.free_cam_keys.contains(&KeyCode::ShiftLeft)
                    || self.free_cam_keys.contains(&KeyCode::ShiftRight)
                {
                    move_vec[1] -= 1.0;
                }

                self.free_cam.translate(move_vec, dt);
                self.free_cam.apply_to_camera(&mut self.camera);
            } else {
                if let Some(hero_id) = self.engine.world.active_hero_id {
                    if let Some(hero) = self.engine.world.actors.iter().find(|a| a.id == hero_id) {
                        self.camera.update_target_for_hero(hero.position);
                    }
                } else {
                    self.camera.target_target = [0.0, 0.0, 0.0];
                    self.camera.target_eye = [0.0, 4.5, 7.5];
                }
                self.camera.update_interpolation(dt);
            }

            let view_proj = self.camera.build_view_projection_matrix();
            r.update_camera(view_proj);
            r.update_light(LightUniform::default());

            let clear_color = if self.engine.state.is_villain_capture() {
                // Trap capture background (dark vortex purple)
                [0.15, 0.05, 0.25, 1.0]
            } else {
                // Academy Hub background (sky blue)
                [0.2, 0.5, 0.8, 1.0]
            };

            let mut to_render = Vec::new();

            if let Some(env) = &self.environment {
                for submesh in &env.submeshes {
                    to_render.push((
                        &submesh.mesh,
                        submesh.texture.as_ref(),
                        submesh.instances.as_slice(),
                    ));
                }
            }

            let pedestal_instances = [InstanceRaw::default()];
            if let Some(pedestal) = &self.pedestal_mesh {
                to_render.push((pedestal, None, pedestal_instances.as_slice()));
            }

            let actor_instances: Vec<[InstanceRaw; 1]> = self
                .engine
                .world
                .actors
                .iter()
                .map(|a| {
                    let (pos, rot) = match a.kind {
                        ActorKind::Collectible { .. } => {
                            let bob_y = (self.anim_time * 4.0).sin() * 0.08;
                            let p = [a.position[0], a.position[1] + bob_y, a.position[2]];
                            let r = self.anim_time * 3.0;
                            (p, r)
                        }
                        _ => (a.position, a.rotation),
                    };
                    [InstanceRaw {
                        model: mat4::transform(pos, rot, [1.0, 1.0, 1.0]),
                    }]
                })
                .collect();

            for (idx, actor) in self.engine.world.actors.iter().enumerate() {
                if !actor.active {
                    continue;
                }
                let inst_slice = actor_instances[idx].as_slice();
                let model_path = actor.model_path.as_deref().unwrap_or("models/hero.igb");
                let model_key = model_path.replace('\\', "/").to_ascii_lowercase();

                if let Some(model) = self.dynamic_models.get(&model_key) {
                    for (gpu_mesh, submesh_tex) in &model.submeshes {
                        let tex = submesh_tex
                            .as_ref()
                            .or_else(|| {
                                actor.texture_path.as_ref().and_then(|tp| {
                                    self.dynamic_textures
                                        .get(&tp.replace('\\', "/").to_ascii_lowercase())
                                })
                            })
                            .or(self.actor_texture.as_ref());
                        to_render.push((gpu_mesh, tex, inst_slice));
                    }
                    continue;
                }

                // Fallback to mesh
                let dynamic_mesh = self.dynamic_meshes.get(&model_key);
                let fallback_mesh = match actor.kind {
                    ActorKind::Prop { .. } => self.prop_mesh.as_ref().or(self.actor_mesh.as_ref()),
                    ActorKind::Collectible { .. } => {
                        let lower = actor.name.to_ascii_lowercase();
                        if lower.contains("gem") {
                            self.gem_mesh.as_ref().or(self.coin_mesh.as_ref())
                        } else {
                            self.coin_mesh.as_ref().or(self.gem_mesh.as_ref())
                        }
                    }
                    _ => self.actor_mesh.as_ref(),
                };
                let actor_mesh = dynamic_mesh.or(fallback_mesh);
                let tex = actor
                    .texture_path
                    .as_ref()
                    .and_then(|tp| {
                        self.dynamic_textures
                            .get(&tp.replace('\\', "/").to_ascii_lowercase())
                    })
                    .or(self.actor_texture.as_ref());

                if let Some(m) = actor_mesh {
                    to_render.push((m, tex, inst_slice));
                }
            }

            let (screen_w, screen_h) = (r.config.width as f32, r.config.height as f32);
            let mut ui = std::mem::take(&mut self.ui_batch);
            Self::render_hud(
                &self.engine,
                &self.camera,
                self.free_cam.enabled,
                screen_w,
                screen_h,
                &mut ui,
            );

            if let Err(e) = r.render_scene_with_ui(
                self.skybox_mesh.as_ref(),
                &to_render,
                Some(&mut ui),
                clear_color,
            ) {
                warn!("Render pass error: {:?}", e);
            }
            self.ui_batch = ui;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("Starting skylands-untrapped runtime (Linux/Windows)");

    let args = Args::parse();

    // 1. Asset Extraction phase
    let extractor = Extractor::new(&args.rom, &args.assets_dir);
    if !extractor.is_extracted() {
        info!(
            "Assets not found in {:?}. Extracting from {:?}...",
            args.assets_dir, args.rom
        );
        extractor.extract_if_needed()?;
    } else {
        info!("Found existing extracted assets in {:?}", args.assets_dir);
    }

    // 2. Mount Alchemy v8.1.0 game archives into VFS AssetLoader
    let mut assets = AssetLoader::default();
    let files_dir = args.assets_dir.join("files");
    let mounted_count = match assets.mount_dir(&files_dir) {
        Ok(count) => count,
        Err(e) => {
            warn!("Scanning {:?} failed: {}", files_dir, e);
            0
        }
    };
    if mounted_count > 0 {
        info!(
            "Mounted {} archive(s) into VFS from {:?}",
            mounted_count, files_dir
        );
    } else {
        let permanent_dir = files_dir.join("permanent");
        for arc_name in ["global.arc", "global.bld"] {
            let arc_path = permanent_dir.join(arc_name);
            if arc_path.exists() {
                match assets.mount(&arc_path) {
                    Ok(_) => info!("Successfully mounted {} (Alchemy IGA v11)", arc_name),
                    Err(e) => warn!("Failed to mount {}: {}", arc_name, e),
                }
            }
        }
    }
    let assets = Arc::new(assets);

    // 3. Initialize Traptanium Portal (USB HID or Mock)
    let portal = if !args.mock_portal {
        info!("Probing for physical USB Traptanium Portal (VID: 0x1430, PID: 0x0150)...");
        match UsbPortal::open_first() {
            Ok(mut usb_portal) => {
                info!("USB Traptanium Portal connected and initialized.");
                usb_portal.set_led_color(PortalZone::All, 0, 255, 200).ok();
                usb_portal.set_trap_light(255, 80, 0).ok();
                ActivePortal::Usb(usb_portal)
            }
            Err(e) => {
                warn!(
                    "Physical USB Portal not connected ({}). Falling back to virtual MockPortal.",
                    e
                );
                create_mock_portal()
            }
        }
    } else {
        info!("Mock portal mode requested.");
        create_mock_portal()
    };

    let mut engine = GameEngine::new(portal, assets.clone());

    if args.headless {
        engine.load_environment_collision("Level_Hub_2014_01.bld");
        info!(
            "Loaded environment collision: {} triangles, {} zone triggers, {} actors",
            engine.world.environment.mesh.triangles.len(),
            engine.world.triggers.len(),
            engine.world.actors.len(),
        );
        info!("Running in headless mode for {} ticks...", args.ticks);
        let mut headless_engine = engine;
        for _ in 0..args.ticks {
            headless_engine.tick();
        }
        info!(
            "Headless run complete. Actors in world: {}, Current state: {:?}",
            headless_engine.world.actors.len(),
            headless_engine.state.current()
        );
        return Ok(());
    }

    // 4. Window & Render Loop
    info!("Starting window lifecycle & wgpu rendering...");
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            warn!(
                "Failed to initialize window event loop: {}. Run via `nix-shell --run \"cargo run\"` on NixOS to provide graphics libraries.",
                e
            );
            return Err(e.into());
        }
    };
    let runtime = AppRuntime::new(engine, assets);
    let app = WindowApp::new(WindowConfig::default(), runtime);
    app.run(event_loop)?;

    Ok(())
}

fn create_mock_portal() -> ActivePortal {
    let mut mock = MockPortal::new();
    // Simulate placing Kaos Trap into Trap Slot (slot 0)
    let mut block0 = [0u8; 16];
    block0[0..4].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    let mut block1 = [0u8; 16];
    block1[0..2].copy_from_slice(&3010u16.to_le_bytes()); // Kaos Trap
    block1[2..4].copy_from_slice(&1010u16.to_le_bytes()); // Kaos Villain ID
    let block8 = [0u8; 16];

    let tag = SkylanderTag::parse_blocks(&block0, &block1, &block8);
    info!(
        "Mock Portal: inserted tag in Trap Slot (Character ID: {}, Villain: {:?}, IsTrap: {})",
        tag.character_id, tag.villain_id, tag.is_trap
    );
    mock.insert_tag(0, vec![block0, block1, block8]);
    ActivePortal::Mock(mock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_engine_mock_ticks() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let mut engine = GameEngine::new(portal, assets);
        assert_eq!(
            engine.state.current(),
            &GameState::Hub {
                academy_area: "Skylander Academy Courtyard".into()
            }
        );

        // Run 5 ticks: mock tag should be picked up -> VillainCapture triggered
        for _ in 0..5 {
            engine.tick();
        }

        assert!(engine.state.is_villain_capture());
        assert!(engine.audio.portal.is_streaming());
    }

    #[test]
    fn test_app_runtime_vfs_loading() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let engine = GameEngine::new(portal, assets.clone());
        let mut runtime = AppRuntime::new(engine, assets);

        assert!(runtime.renderer.is_none());
        assert!(runtime.dynamic_meshes.is_empty());
        assert!(runtime.dynamic_textures.is_empty());
        // Load without active GPU renderer returns None gracefully
        assert!(runtime.load_mesh("models/hero.igb").is_none());
        assert!(runtime.load_model("models/hero.igb").is_none());
        assert!(runtime
            .load_cmpr_texture("textures/hero.png", 8, 8)
            .is_none());
    }

    #[test]
    fn test_mock_portal_character_resolution_and_actor_binding() {
        let mut mock = MockPortal::new();

        // 1. Kaos trap in slot 0
        let mut b0 = [0u8; 16];
        b0[0..4].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        let mut b1 = [0u8; 16];
        b1[0..2].copy_from_slice(&3010u16.to_le_bytes()); // Kaos Trap
        b1[2..4].copy_from_slice(&1010u16.to_le_bytes()); // Kaos Villain
        let b8 = [0u8; 16];
        mock.insert_tag(0, vec![b0, b1, b8]);

        // 2. Snap Shot hero in slot 1
        let mut h0 = [0u8; 16];
        h0[0..4].copy_from_slice(&[0x55, 0x66, 0x77, 0x88]);
        let mut h1 = [0u8; 16];
        h1[0..2].copy_from_slice(&400u16.to_le_bytes()); // Snap Shot
        let h8 = [0u8; 16];
        mock.insert_tag(1, vec![h0, h1, h8]);

        let assets = Arc::new(AssetLoader::default());
        let mut engine = GameEngine::new(ActivePortal::Mock(mock), assets);

        for _ in 0..10 {
            engine.tick();
        }

        assert_eq!(engine.world.actors.len(), 2);

        let kaos = engine
            .world
            .find_by_character_id(1010)
            .expect("Kaos spawned");
        assert!(kaos.is_villain);
        assert_eq!(
            kaos.model_path.as_deref(),
            Some("models/characters/kaos.igb")
        );
        assert_eq!(
            kaos.texture_path.as_deref(),
            Some("textures/characters/kaos.png")
        );

        let hero = engine
            .world
            .find_by_character_id(400)
            .expect("Hero spawned");
        assert!(!hero.is_villain);
        assert_eq!(
            hero.model_path.as_deref(),
            Some("models/characters/snapshot.igb")
        );
        assert_eq!(
            hero.texture_path.as_deref(),
            Some("textures/characters/snapshot.png")
        );

        // Despawn hero
        match &mut engine.portal {
            ActivePortal::Mock(m) => m.remove_tag(1),
            _ => unreachable!(),
        }

        for _ in 0..5 {
            engine.tick();
        }

        assert_eq!(engine.world.actors.len(), 1);
        assert!(engine.world.find_by_character_id(400).is_none());
        assert!(engine.world.find_by_character_id(1010).is_some());
    }

    #[test]
    fn test_load_environment_fallback_and_render_population() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let engine = GameEngine::new(portal, assets.clone());
        let mut runtime = AppRuntime::new(engine, assets);

        // Without renderer, load_environment returns None
        assert!(runtime.load_environment("levels/hub.igb").is_none());

        if let Ok(renderer) = Renderer::new_headless(640, 480) {
            runtime.renderer = Some(renderer);
            let env = runtime
                .load_environment("levels/hub.igb")
                .expect("Loads fallback synthetic courtyard");
            assert_eq!(env.name, "synthetic_courtyard");
            assert_eq!(env.submeshes.len(), 2);

            let mut to_render = Vec::new();
            if let Some(env) = &runtime.environment {
                for submesh in &env.submeshes {
                    to_render.push((
                        &submesh.mesh,
                        submesh.texture.as_ref(),
                        submesh.instances.as_slice(),
                    ));
                }
            }
            assert_eq!(to_render.len(), 2);
        }
    }

    #[test]
    fn test_load_real_environment_arena_mesh() {
        use std::path::Path;
        let p1 = Path::new("assets/files/level/Arena_Mesh_300.bld");
        let p2 = Path::new("../../assets/files/level/Arena_Mesh_300.bld");
        let path = if p1.exists() { p1 } else { p2 };
        if path.exists() {
            let mut loader = AssetLoader::default();
            loader.mount(path).expect("mount Arena_Mesh_300");
            let assets = Arc::new(loader);
            let portal = create_mock_portal();
            let engine = GameEngine::new(portal, assets.clone());
            let mut runtime = AppRuntime::new(engine, assets);

            if let Ok(renderer) = Renderer::new_headless(640, 480) {
                runtime.renderer = Some(renderer);
                let env = runtime
                    .load_environment("Arena_Mesh_300.bld")
                    .expect("Loads real Arena_Mesh_300 environment");
                assert_eq!(env.name, "Arena_Mesh_300.bld");
                assert!(!env.submeshes.is_empty());
                assert!(!runtime.engine.world.environment.mesh.triangles.is_empty());

                let mut to_render = Vec::new();
                if let Some(env) = &runtime.environment {
                    for submesh in &env.submeshes {
                        to_render.push((
                            &submesh.mesh,
                            submesh.texture.as_ref(),
                            submesh.instances.as_slice(),
                        ));
                    }
                }
                assert!(!to_render.is_empty());
            }
        }
    }

    #[test]
    fn test_headless_environment_collision_and_jumping() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let mut engine = GameEngine::new(portal, assets);

        assert_eq!(engine.world.environment.mesh.triangles.len(), 10);

        let id = engine
            .world
            .spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);

        // 1. Jump test
        engine.input.state.press(skylands_core::WiiButton::A);
        engine.tick();
        engine.input.state.release(skylands_core::WiiButton::A);

        let hero = engine.world.actors.iter().find(|a| a.id == id).unwrap();
        assert!(hero.position[1] > 0.0, "Hero should be airborne after jump");
        assert!(!hero.collider.grounded);

        // 2. Fall & land back on floor
        for _ in 0..60 {
            engine.tick();
        }
        let hero = engine.world.actors.iter().find(|a| a.id == id).unwrap();
        assert!(
            (hero.position[1] - 0.0).abs() < 1e-4,
            "Hero should land flush on courtyard floor"
        );
        assert!(hero.collider.grounded);

        // 3. Move north towards wall at z = -20
        engine.input.state.set_stick(0.0, 1.0); // -Z stick
        for _ in 0..120 {
            engine.tick();
        }
        let hero = engine.world.actors.iter().find(|a| a.id == id).unwrap();
        assert!(
            hero.position[2] >= -19.5,
            "Hero cannot penetrate north wall boundary: z={}",
            hero.position[2]
        );
    }

    #[test]
    fn test_headless_entity_counts_and_pickup_interactions() {
        use skylands_formats::{mat4_identity, LevelMarker, MarkerKind};

        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let mut engine = GameEngine::new(portal, assets);

        let markers = vec![
            LevelMarker {
                name: "sp_hero".to_string(),
                kind: MarkerKind::Spawn,
                position: [0.0, 0.0, 0.0],
                world_transform: mat4_identity(),
                node_index: 0,
                geometry_index: None,
                mesh: None,
            },
            LevelMarker {
                name: "prop_wooden_barrel".to_string(),
                kind: MarkerKind::Prop,
                position: [5.0, 0.0, 0.0],
                world_transform: mat4_identity(),
                node_index: 1,
                geometry_index: None,
                mesh: None,
            },
            LevelMarker {
                name: "pickup_coin_gold".to_string(),
                kind: MarkerKind::Collectible,
                position: [0.0, 0.0, -2.0],
                world_transform: mat4_identity(),
                node_index: 2,
                geometry_index: None,
                mesh: None,
            },
            LevelMarker {
                name: "pickup_gem_emerald".to_string(),
                kind: MarkerKind::Collectible,
                position: [0.0, 0.0, -10.0],
                world_transform: mat4_identity(),
                node_index: 3,
                geometry_index: None,
                mesh: None,
            },
        ];

        // 1. Populate world with level entities
        engine.world.load_level_entities(&markers);
        assert_eq!(engine.world.environment.spawn_point, [0.0, 0.0, 0.0]);
        assert_eq!(engine.world.actors.len(), 3);

        // 2. Spawn hero
        let hero_id = engine.world.spawn(
            400,
            0,
            "Snap Shot",
            engine.world.environment.spawn_point,
            false,
            None,
            None,
        );
        assert_eq!(engine.world.actors.len(), 4);
        assert_eq!(engine.world.active_hero_id, Some(hero_id));

        // 3. Move hero north towards coin at z = -2.0
        engine.input.state.set_stick(0.0, 1.0); // -Z movement, speed 5.0
        for _ in 0..30 {
            engine.tick();
        }

        // 4. Verify coin was collected and gem is still active
        let coin = engine
            .world
            .actors
            .iter()
            .find(|a| a.name == "pickup_coin_gold")
            .unwrap();
        assert_eq!(
            coin.kind,
            ActorKind::Collectible {
                value: 1,
                collected: true
            }
        );
        assert!(!coin.active);

        let gem = engine
            .world
            .actors
            .iter()
            .find(|a| a.name == "pickup_gem_emerald")
            .unwrap();
        assert_eq!(
            gem.kind,
            ActorKind::Collectible {
                value: 5,
                collected: false
            }
        );
        assert!(gem.active);

        let prop = engine
            .world
            .actors
            .iter()
            .find(|a| a.name == "prop_wooden_barrel")
            .unwrap();
        assert!(prop.active);
        assert_eq!(engine.world.coins, 1);

        // Verify HUD snapshot overlay batching
        let mut ui_batch = UiBatch::new();
        let camera = Camera::new(800.0 / 600.0);
        AppRuntime::build_hud_overlay(&engine, &camera, 800.0, 600.0, &mut ui_batch);
        assert!(!ui_batch.is_empty());
        assert!(!ui_batch.vertices.is_empty());
        assert!(!ui_batch.indices.is_empty());
    }

    #[test]
    fn test_headless_hud_overlay_render_pass() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let engine = GameEngine::new(portal, assets.clone());
        let mut runtime = AppRuntime::new(engine, assets);

        if let Ok(mut renderer) = Renderer::new_headless(800, 600) {
            let hero_id =
                runtime
                    .engine
                    .world
                    .spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);
            assert_eq!(runtime.engine.world.active_hero_id, Some(hero_id));
            runtime.engine.world.coins = 42;

            runtime.engine.state.set_state(GameState::VillainCapture {
                villain_id: 1010,
                progress: 0.5,
            });

            let (w, h) = (renderer.config.width as f32, renderer.config.height as f32);
            let mut ui = UiBatch::new();
            AppRuntime::build_hud_overlay(&runtime.engine, &runtime.camera, w, h, &mut ui);
            assert!(!ui.is_empty());

            let clear_color = [0.1, 0.1, 0.2, 1.0];
            let res = renderer.render_scene_with_ui(None, &[], Some(&mut ui), clear_color);
            assert!(res.is_ok());
        }
    }

    #[test]
    fn test_headless_hero_attack_and_damage_popup_hud() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let engine = GameEngine::new(portal, assets.clone());
        let mut runtime = AppRuntime::new(engine, assets);

        let hero_id =
            runtime
                .engine
                .world
                .spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);
        assert_eq!(runtime.engine.world.active_hero_id, Some(hero_id));

        // Spawn destructible barrel directly in front of hero
        let prop_id = runtime.engine.world.spawn_prop(
            "prop_destruct_barrel",
            [0.0, 0.0, -1.0],
            true,
            None,
            None,
        );
        if let Some(prop) = runtime
            .engine
            .world
            .actors
            .iter_mut()
            .find(|a| a.id == prop_id)
        {
            prop.health = 50;
        }

        // Simulate 10 ticks: tick 0 triggers primary attack (Key J -> WiiButton::B)
        runtime.engine.input.handle_key_code(KeyCode::KeyJ, true);
        for tick in 0..10 {
            if tick == 1 {
                runtime.engine.input.handle_key_code(KeyCode::KeyJ, false);
            }
            runtime.engine.tick();
        }

        // Barrel destroyed, damage popup created
        assert!(runtime
            .engine
            .world
            .actors
            .iter()
            .find(|a| a.id == prop_id)
            .is_none());
        assert!(!runtime.engine.world.damage_popups.is_empty());

        // Verify HUD pass projects damage popup and generates valid vertices
        let mut ui = UiBatch::new();
        let camera = Camera::new(800.0 / 600.0);
        AppRuntime::build_hud_overlay(&runtime.engine, &camera, 800.0, 600.0, &mut ui);
        assert!(!ui.vertices.is_empty());
        assert!(!ui.indices.is_empty());
    }

    #[test]
    fn test_headless_zone_transition_walk_into_trigger_and_step() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let engine = GameEngine::new(portal, assets.clone());
        let mut runtime = AppRuntime::new(engine, assets);

        let hero_id =
            runtime
                .engine
                .world
                .spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);
        assert_eq!(runtime.engine.world.active_hero_id, Some(hero_id));

        // Place a doorway trigger 3m ahead on -Z
        runtime.engine.world.triggers.clear();
        runtime
            .engine
            .world
            .triggers
            .push(skylands_core::ZoneTrigger {
                id: 1,
                position: [0.0, 0.0, -3.0],
                radius: 1.5,
                target_zone: "Academy Library".to_string(),
                target_spawn: [0.0, 0.0, 10.0],
                prompt: "Press A to Enter Library".to_string(),
            });

        // Walk hero forward (Key W -> positive stick Y -> negative Z velocity)
        runtime.engine.input.handle_key_code(KeyCode::KeyW, true);
        for _ in 0..20 {
            runtime.engine.tick();
        }
        runtime.engine.input.handle_key_code(KeyCode::KeyW, false);

        // Verify hero moved inside trigger and prompt is active
        assert_eq!(
            runtime.engine.world.active_prompt,
            Some("Press A to Enter Library".to_string())
        );

        // Trigger Button A (Space keycode -> WiiButton::A)
        runtime.engine.input.handle_key_code(KeyCode::Space, true);
        runtime.engine.tick();
        runtime.engine.input.handle_key_code(KeyCode::Space, false);

        // State machine entered ZoneTransition
        assert!(matches!(
            runtime.engine.state.current(),
            GameState::ZoneTransition { .. }
        ));

        // Step 30 ticks through fade transition
        for _ in 0..30 {
            runtime.engine.tick();
            if let Some(hero) = runtime.engine.world.actors.iter().find(|a| a.id == hero_id) {
                runtime.camera.update_target_for_hero(hero.position);
            }
            runtime.camera.update_interpolation(0.016);
        }

        // Verify active zone is Library, hero repositioned at target spawn (10.0 on Z)
        assert_eq!(runtime.engine.world.active_zone, "Academy Library");
        let hero = runtime
            .engine
            .world
            .actors
            .iter()
            .find(|a| a.id == hero_id)
            .unwrap();
        assert_eq!(hero.position, [0.0, 0.0, 10.0]);

        // Verify camera smoothly repositioned towards hero
        assert!(runtime.camera.target[2] > 0.0);
    }

    #[test]
    fn test_free_cam_toggle_and_hero_input_freeze() {
        let portal = create_mock_portal();
        let assets = Arc::new(AssetLoader::default());
        let engine = GameEngine::new(portal, assets.clone());
        let mut runtime = AppRuntime::new(engine, assets);

        let hero_id =
            runtime
                .engine
                .world
                .spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);
        assert_eq!(runtime.engine.world.active_hero_id, Some(hero_id));

        // Initial free_cam state is disabled
        assert!(!runtime.free_cam.enabled);

        // Press W normally: hero receives stick input
        runtime.on_key(KeyCode::KeyW, true);
        assert_eq!(runtime.engine.input.state.stick, [0.0, 1.0]);

        // Press F12 to enable free cam mode
        runtime.on_key(KeyCode::F12, true);
        assert!(runtime.free_cam.enabled);
        // Engine input state must be reset on enable
        assert_eq!(runtime.engine.input.state.stick, [0.0, 0.0]);
        assert_eq!(runtime.engine.input.state.buttons, 0);

        // While in free cam, WASD/Arrow/Space keys route to free_cam_keys and freeze hero input
        runtime.on_key(KeyCode::KeyW, true);
        runtime.on_key(KeyCode::Space, true);
        assert!(runtime.free_cam_keys.contains(&KeyCode::KeyW));
        assert!(runtime.free_cam_keys.contains(&KeyCode::Space));
        // Hero input remains zero / frozen
        assert_eq!(runtime.engine.input.state.stick, [0.0, 0.0]);
        assert_eq!(runtime.engine.input.state.buttons, 0);

        // Mouse motion rotates free_cam
        let initial_yaw = runtime.free_cam.yaw;
        let initial_pitch = runtime.free_cam.pitch;
        runtime.on_mouse_motion(100.0, 50.0);
        assert_ne!(runtime.free_cam.yaw, initial_yaw);
        assert_ne!(runtime.free_cam.pitch, initial_pitch);

        // Camera movement simulation in free cam mode
        let prev_cam_eye = runtime.free_cam.eye;
        let dt = 0.1;
        let mut move_vec = [0.0f32; 3];
        if runtime.free_cam_keys.contains(&KeyCode::KeyW) {
            move_vec[2] += 1.0;
        }
        runtime.free_cam.translate(move_vec, dt);
        runtime.free_cam.apply_to_camera(&mut runtime.camera);
        assert_ne!(runtime.free_cam.eye, prev_cam_eye);
        assert_eq!(runtime.camera.eye, runtime.free_cam.eye);

        // HUD banner rendering check
        let mut ui_batch = UiBatch::new();
        AppRuntime::render_hud(
            &runtime.engine,
            &runtime.camera,
            runtime.free_cam.enabled,
            800.0,
            600.0,
            &mut ui_batch,
        );
        assert!(!ui_batch.is_empty());

        // Press F12 again to disable free cam mode
        runtime.on_key(KeyCode::F12, true);
        assert!(!runtime.free_cam.enabled);
        assert!(runtime.free_cam_keys.is_empty());

        // Key presses now route back to hero input
        runtime.on_key(KeyCode::KeyW, true);
        assert_eq!(runtime.engine.input.state.stick, [0.0, 1.0]);
    }
}
