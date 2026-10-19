use clap::Parser;
use log::{info, warn};
use skylands_core::{
    AudioEngine, FixedTimestep, GameState, GameStateMachine, InputMapper, PortalGameplay, World,
};
use skylands_extractor::Extractor;
use skylands_formats::{decode_cmpr, ArchiveMount};
use skylands_portal::protocol::{decode_portal_response, PortalEvent, PortalZone, REPORT_SIZE};
use skylands_portal::{MockPortal, SkylanderTag, UsbPortal};
use skylands_render::{
    mat4, Camera, GpuMesh, GpuTexture, InstanceRaw, Mesh, RenderAppHandler, Renderer, WindowApp,
    WindowConfig,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::EventLoop;
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
}

impl GameEngine {
    pub fn new(portal: ActivePortal) -> Self {
        let mut state = GameStateMachine::new();
        state.set_state(GameState::Hub {
            academy_area: "Skylander Academy Courtyard".into(),
        });

        Self {
            portal,
            gameplay: PortalGameplay::new(),
            world: World::new(),
            state,
            input: InputMapper::new(),
            audio: AudioEngine::new(),
            timestep: FixedTimestep::new(),
            last_frame: Instant::now(),
        }
    }

    pub fn tick(&mut self) {
        self.input.poll_gamepads();

        // 1. Poll portal hardware / mock events
        let events = self.portal.poll_events();
        for ev in events {
            let outgoing =
                self.gameplay
                    .handle_event(ev, &mut self.world, &mut self.state, &mut self.audio);
            for pkt in outgoing {
                self.portal.send_packet(&pkt);
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
}

pub struct AppRuntime {
    pub engine: GameEngine,
    pub renderer: Option<Renderer>,
    pub camera: Camera,
    pub pedestal_mesh: Option<GpuMesh>,
    pub actor_mesh: Option<GpuMesh>,
    pub actor_texture: Option<GpuTexture>,
}

impl AppRuntime {
    pub fn new(engine: GameEngine) -> Self {
        Self {
            engine,
            renderer: None,
            camera: Camera::new(1280.0 / 720.0),
            pedestal_mesh: None,
            actor_mesh: None,
            actor_texture: None,
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

                let gpu_pedestal = GpuMesh::from_mesh(&renderer.device, &pedestal);
                let gpu_hero = GpuMesh::from_mesh(&renderer.device, &hero_quad);

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
                self.renderer = Some(renderer);
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

    fn on_key(&mut self, key_code: u32, state: bool) {
        self.engine.input.handle_key_code(key_code, state);
    }

    fn on_update(&mut self) {
        self.engine.update();
    }

    fn on_render(&mut self) {
        if let Some(r) = &mut self.renderer {
            if let Some(hero_id) = self.engine.world.active_hero_id {
                if let Some(hero) = self.engine.world.actors.iter().find(|a| a.id == hero_id) {
                    self.camera.target = hero.position;
                    self.camera.eye = [
                        hero.position[0],
                        hero.position[1] + 4.5,
                        hero.position[2] + 7.5,
                    ];
                }
            } else {
                self.camera.target = [0.0, 0.0, 0.0];
                self.camera.eye = [0.0, 4.5, 7.5];
            }

            let view_proj = self.camera.build_view_projection_matrix();
            r.update_camera(view_proj);

            let clear_color = if self.engine.state.is_villain_capture() {
                // Trap capture background (dark vortex purple)
                [0.15, 0.05, 0.25, 1.0]
            } else {
                // Academy Hub background (sky blue)
                [0.2, 0.5, 0.8, 1.0]
            };

            let mut to_render = Vec::new();
            let pedestal_instances = [InstanceRaw::default()];
            if let Some(pedestal) = &self.pedestal_mesh {
                to_render.push((pedestal, None, pedestal_instances.as_slice()));
            }

            let actor_instances: Vec<InstanceRaw> = self
                .engine
                .world
                .actors
                .iter()
                .map(|a| InstanceRaw {
                    model: mat4::transform(a.position, a.rotation, [1.0, 1.0, 1.0]),
                })
                .collect();

            if !actor_instances.is_empty() {
                if let Some(actor) = &self.actor_mesh {
                    to_render.push((actor, self.actor_texture.as_ref(), &actor_instances));
                }
            }

            if let Err(e) = r.render_instanced(&to_render, clear_color) {
                warn!("Render pass error: {:?}", e);
            }
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

    // 2. Mount Alchemy v8.1.0 game archives
    let mut vfs = ArchiveMount::new();
    let permanent_dir = args.assets_dir.join("files").join("permanent");
    for arc_name in ["global.arc", "global.bld"] {
        let arc_path = permanent_dir.join(arc_name);
        if arc_path.exists() {
            match vfs.mount(&arc_path) {
                Ok(_) => info!("Successfully mounted {} (Alchemy IGA v11)", arc_name),
                Err(e) => warn!("Failed to mount {}: {}", arc_name, e),
            }
        }
    }

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

    let engine = GameEngine::new(portal);

    if args.headless {
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
    let event_loop = EventLoop::new()?;
    let runtime = AppRuntime::new(engine);
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
        let mut engine = GameEngine::new(portal);
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
}
