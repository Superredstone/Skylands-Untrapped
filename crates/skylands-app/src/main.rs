use clap::Parser;
use log::{info, warn};
use skylands_extractor::Extractor;
use skylands_formats::ArchiveMount;
use skylands_portal::{MockPortal, PortalZone, SkylanderTag, UsbPortal};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "skylands-untrapped", version = "0.1.0", about = "Native port runtime for Skylanders Trap Team")]
struct Args {
    #[arg(short, long, default_value = "rom.eu.wbfs")]
    rom: PathBuf,

    #[arg(short, long, default_value = "assets")]
    assets_dir: PathBuf,

    #[arg(long, help = "Force mock portal mode instead of physical USB Traptanium Portal")]
    mock_portal: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("Starting skylands-untrapped runtime (Linux/Windows)");

    let args = Args::parse();

    // 1. Asset Extraction phase (only extracts if not present)
    let extractor = Extractor::new(&args.rom, &args.assets_dir);
    if !extractor.is_extracted() {
        info!("Assets not found in {:?}. Extracting from {:?}...", args.assets_dir, args.rom);
        extractor.extract_if_needed()?;
    } else {
        info!("Found existing extracted assets in {:?}", args.assets_dir);
    }

    // 2. Mount Alchemy v8.1.0 game archives
    let mut vfs = ArchiveMount::new();
    let global_arc = args.assets_dir.join("files").join("permanent").join("global.arc");
    if global_arc.exists() {
        match vfs.mount(&global_arc) {
            Ok(_) => info!("Successfully mounted global.arc (Alchemy IGA v11)"),
            Err(e) => warn!("Failed to mount global.arc: {}", e),
        }
    }

    // 3. Initialize Traptanium Portal (USB HID or Mock)
    if !args.mock_portal {
        info!("Probing for physical USB Traptanium Portal (VID: 0x1430, PID: 0x0150)...");
        match UsbPortal::open_first() {
            Ok(mut usb_portal) => {
                info!("USB Traptanium Portal connected and initialized.");
                usb_portal.set_led_color(PortalZone::All, 0, 255, 200).ok();
                usb_portal.set_trap_light(255, 80, 0).ok();
            }
            Err(e) => {
                warn!("Physical USB Portal not connected ({}). Falling back to virtual MockPortal.", e);
                init_mock_portal();
            }
        }
    } else {
        info!("Mock portal mode requested.");
        init_mock_portal();
    }

    info!("Engine initialized successfully. Ready for gameplay loop.");
    Ok(())
}

fn init_mock_portal() {
    let mut mock = MockPortal::new();
    // Simulate placing a Kaos Trap into the Trap Slot (slot 0)
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
}
