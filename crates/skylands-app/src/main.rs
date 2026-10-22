use clap::Parser;
use log::{info, warn};
use std::path::PathBuf;
use std::sync::Mutex;

use bevy::prelude::*;
use skylands_core::{
    resolve_character_id_by_name, ActiveLevel, ActivePortal, ActorIdentity, GameState,
    PortalGameplayState, SkylandsCorePlugin,
};
use skylands_extractor::Extractor;
use skylands_formats::AssetLoader;
use skylands_portal::protocol::PortalZone;
use skylands_portal::{MockPortal, UsbPortal};
use skylands_render::{CharacterAssetServer, SkylandsRenderPlugin, ViewerSettings};

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

    #[arg(
        long,
        help = "Spawn specified character directly by name or ID (e.g. 'spyro' or '16')"
    )]
    character: Option<String>,

    #[arg(
        long,
        help = "Specify level archive to load (default: 'Level_Hub_2014_01.bld')"
    )]
    level: Option<String>,

    #[arg(
        long,
        help = "Run in showcase pedestal mode with auto-turntable rotation, orbit camera, and animation cycling"
    )]
    viewer: bool,
}

pub fn create_mock_portal(char_id: Option<u16>, include_trap: bool) -> ActivePortal {
    let mut mock = MockPortal::new();

    if include_trap {
        // 1. Trap slot (slot 0): Kaos Trap (3010) with Kaos villain (1010)
        let mut b0 = [0u8; 16];
        b0[0..4].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        let mut b1 = [0u8; 16];
        b1[0..2].copy_from_slice(&3010u16.to_le_bytes());
        b1[2..4].copy_from_slice(&1010u16.to_le_bytes());
        let b8 = [0u8; 16];
        mock.insert_tag(0, vec![b0, b1, b8]);
    }

    // 2. Ring slot (slot 1): Skylander hero (default Spyro 16)
    let hero_id = char_id.unwrap_or(16);
    let mut h0 = [0u8; 16];
    h0[0..4].copy_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
    let mut h1 = [0u8; 16];
    h1[0..2].copy_from_slice(&hero_id.to_le_bytes());
    let h8 = [0u8; 16];
    mock.insert_tag(1, vec![h0, h1, h8]);

    if include_trap {
        info!(
            "Mock Portal initialized with Kaos Trap (0) and Hero ID {} (1)",
            hero_id
        );
    } else {
        info!("Mock Portal initialized with Hero ID {} (1)", hero_id);
    }
    ActivePortal::Mock(mock)
}

fn init_portal(mock_portal: bool, char_id: Option<u16>, viewer_mode: bool) -> ActivePortal {
    let include_trap = !viewer_mode;
    if !mock_portal {
        info!("Probing for physical USB Traptanium Portal (VID: 0x1430, PID: 0x0150)...");
        match UsbPortal::open_first() {
            Ok(mut usb_portal) => {
                info!("USB Traptanium Portal connected and initialized.");
                usb_portal.set_led_color(PortalZone::All, 0, 255, 200).ok();
                usb_portal.set_trap_light(255, 80, 0).ok();
                ActivePortal::Usb(Mutex::new(usb_portal))
            }
            Err(e) => {
                warn!(
                    "Physical USB Portal not connected ({}). Falling back to virtual MockPortal.",
                    e
                );
                create_mock_portal(char_id, include_trap)
            }
        }
    } else {
        info!("Mock portal mode requested.");
        create_mock_portal(char_id, include_trap)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("Starting skylands-untrapped runtime (Linux/Windows, Bevy 0.15)");

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

    // 2. Mount Alchemy v8.1.0 game archives into VFS AssetLoader if available
    let mut assets = AssetLoader::default();
    let files_dir = args.assets_dir.join("files");
    if let Ok(mounted_count) = assets.mount_dir(&files_dir) {
        if mounted_count > 0 {
            info!(
                "Mounted {} archive(s) into VFS from {:?}",
                mounted_count, files_dir
            );
        }
    }
    let asset_server = CharacterAssetServer::new(assets);

    // 3. Resolve character parameter (defaulting to Spyro 16)
    let resolved_char_id = args
        .character
        .as_ref()
        .and_then(|c| {
            let id = c
                .parse::<u16>()
                .ok()
                .or_else(|| resolve_character_id_by_name(c));
            if id.is_none() {
                warn!("Unknown character '{}', falling back to Spyro (16)", c);
            }
            id
        })
        .unwrap_or(16);

    let active_level = args
        .level
        .as_ref()
        .map(|lvl| ActiveLevel::new(lvl.as_str()))
        .unwrap_or_default();

    // 4. Initialize portal
    let portal = init_portal(args.mock_portal, Some(resolved_char_id), args.viewer);

    // 5. Headless mode (CLI simulation / test runner)
    if args.headless {
        info!("Running in headless mode for {} ticks...", args.ticks);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SkylandsCorePlugin);
        app.insert_resource(asset_server.clone());
        app.insert_resource(active_level.clone());
        app.insert_resource(portal);

        let mut gameplay = app.world_mut().resource_mut::<PortalGameplayState>();
        gameplay.active_hero_character_id = Some(resolved_char_id);

        // Run startup schedule
        app.update();

        // Step fixed update ticks deterministically
        for _ in 0..args.ticks {
            app.world_mut().run_schedule(FixedUpdate);
        }

        let actor_count = app
            .world_mut()
            .query::<&ActorIdentity>()
            .iter(app.world())
            .count();
        let current_state = app.world().resource::<State<GameState>>();
        info!(
            "Headless run complete. Actors in world: {}, Current state: {:?}",
            actor_count,
            current_state.get()
        );
        return Ok(());
    }

    // 6. Windowed GUI mode
    info!("Starting Bevy 0.15 app lifecycle...");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Skylands Untrapped - Native Port".into(),
            resolution: (1280.0_f32, 720.0_f32).into(),
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(SkylandsCorePlugin);
    app.add_plugins(SkylandsRenderPlugin {
        viewer_mode: args.viewer,
    });
    app.insert_resource(asset_server);
    app.insert_resource(active_level);
    app.insert_resource(portal);

    let mut gameplay = app.world_mut().resource_mut::<PortalGameplayState>();
    gameplay.active_hero_character_id = Some(resolved_char_id);
    if args.viewer {
        let mut viewer_settings = app.world_mut().resource_mut::<ViewerSettings>();
        viewer_settings.current_char_id = resolved_char_id;
        if let Some(pos) = skylands_render::SHOWCASE_CHARACTERS
            .iter()
            .position(|&x| x == resolved_char_id)
        {
            viewer_settings.char_index = pos;
        }
    }

    app.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use skylands_core::{Hero, Villain};

    #[test]
    fn test_mock_portal_creation() {
        let portal_default = create_mock_portal(None, true);
        match portal_default {
            ActivePortal::Mock(mock) => {
                assert!(mock.loaded_tags.contains_key(&0));
                assert!(mock.loaded_tags.contains_key(&1));
                let b1 = &mock.loaded_tags[&1][1];
                let cid = u16::from_le_bytes([b1[0], b1[1]]);
                assert_eq!(cid, 16);
            }
            _ => panic!("Expected Mock portal"),
        }

        let portal = create_mock_portal(Some(401), true);
        match portal {
            ActivePortal::Mock(mock) => {
                assert!(mock.loaded_tags.contains_key(&0));
                assert!(mock.loaded_tags.contains_key(&1));
                let b1 = &mock.loaded_tags[&1][1];
                let cid = u16::from_le_bytes([b1[0], b1[1]]);
                assert_eq!(cid, 401);
            }
            _ => panic!("Expected Mock portal"),
        }

        let portal_viewer = create_mock_portal(Some(16), false);
        match portal_viewer {
            ActivePortal::Mock(mock) => {
                assert!(!mock.loaded_tags.contains_key(&0));
                assert!(mock.loaded_tags.contains_key(&1));
            }
            _ => panic!("Expected Mock portal"),
        }
    }

    #[test]
    fn test_headless_app_tick_simulation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SkylandsCorePlugin);
        app.insert_resource(CharacterAssetServer::default());
        app.insert_resource(create_mock_portal(Some(400), true));

        app.update();
        for _ in 0..60 {
            app.world_mut().run_schedule(FixedUpdate);
        }

        let hero_count = app
            .world_mut()
            .query_filtered::<Entity, With<Hero>>()
            .iter(app.world())
            .count();
        let villain_count = app
            .world_mut()
            .query_filtered::<Entity, With<Villain>>()
            .iter(app.world())
            .count();

        assert_eq!(hero_count, 1, "Expected 1 hero spawned from mock portal");
        assert_eq!(
            villain_count, 1,
            "Expected 1 villain spawned from Kaos trap tag"
        );

        let current_state = app.world().resource::<State<GameState>>();
        assert_eq!(
            current_state.get(),
            &GameState::VillainCapture,
            "Placing Kaos trap tag triggers VillainCapture state"
        );
    }
}
