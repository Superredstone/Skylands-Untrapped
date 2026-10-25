use crate::audio::PortalSpeakerSink;
use crate::collision::CollisionMesh;
use crate::events::PortalEvent;
use bevy::prelude::*;
use skylands_formats::{
    hero_max_hp, hero_movement, AssetLoader, HeroMovement, MinionTuning, ScriptSet,
};
use skylands_portal::protocol::{
    decode_portal_response, encode_color_packet, encode_trap_light_packet, PortalZone, REPORT_SIZE,
};
use skylands_portal::{MockPortal, SkylanderTag, UsbPortal};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

/// `global.bld` scripts and the boot tuning `Init_Setup.bld` writes into them.
type MinionScripts = Option<Arc<(ScriptSet, MinionTuning)>>;

#[derive(Resource, Clone, Default)]
pub struct GameAssetServer {
    pub loader: Arc<Mutex<AssetLoader>>,
    minion_scripts: Arc<OnceLock<MinionScripts>>,
}

impl GameAssetServer {
    pub fn new(loader: AssetLoader) -> Self {
        Self::from_arc(Arc::new(Mutex::new(loader)))
    }

    pub fn from_arc(loader: Arc<Mutex<AssetLoader>>) -> Self {
        Self {
            loader,
            minion_scripts: Default::default(),
        }
    }

    /// Level-1 max HP from the character file's `<id>_HP_local` (`hero_max_hp`), parsed
    /// once per server. `None` when the scripts are missing.
    pub fn hero_max_hp(&self, model_path: &str) -> Option<u32> {
        let (character, scripts) = self.character_scripts(model_path)?;
        hero_max_hp(&character, &scripts.0, &scripts.1)
            .filter(|&hp| hp > 0.0)
            .map(|hp| hp.round() as u32)
    }

    /// Movement the character's script passes to "Minion - setup pre descriptions"
    /// (`hero_movement`, ft/s and deg/s).
    pub fn hero_movement(&self, model_path: &str) -> Option<HeroMovement> {
        let (character, scripts) = self.character_scripts(model_path)?;
        hero_movement(&character, &scripts.0, &scripts.1)
    }

    fn character_scripts(
        &self,
        model_path: &str,
    ) -> Option<(ScriptSet, Arc<(ScriptSet, MinionTuning)>)> {
        let parse = |p: &str| {
            let bytes = self.loader.lock().ok()?.load_raw(p).ok()?;
            ScriptSet::parse(&bytes)
        };
        let scripts = self
            .minion_scripts
            .get_or_init(|| {
                let global = parse("global.bld")?;
                let tuning = MinionTuning::from_scripts(&parse("Init_Setup.bld")?, &global).ok()?;
                Some(Arc::new((global, tuning)))
            })
            .clone()?;
        Some((parse(model_path)?, scripts))
    }
}

impl From<AssetLoader> for GameAssetServer {
    fn from(loader: AssetLoader) -> Self {
        Self::new(loader)
    }
}

impl From<Arc<Mutex<AssetLoader>>> for GameAssetServer {
    fn from(loader: Arc<Mutex<AssetLoader>>) -> Self {
        Self::from_arc(loader)
    }
}

#[derive(Resource, Debug, Clone)]
pub struct ActiveLevel {
    pub name: String,
}

impl Default for ActiveLevel {
    fn default() -> Self {
        Self {
            name: "Level_Hub_2014_01.bld".to_string(),
        }
    }
}

impl ActiveLevel {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiiButton {
    DpadUp = 1 << 0,
    DpadDown = 1 << 1,
    DpadLeft = 1 << 2,
    DpadRight = 1 << 3,
    A = 1 << 4,
    B = 1 << 5,
    C = 1 << 6,
    Z = 1 << 7,
    One = 1 << 8,
    Two = 1 << 9,
    Plus = 1 << 10,
    Minus = 1 << 11,
    Home = 1 << 12,
}

#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct WiiInputState {
    pub stick: [f32; 2],
    pub buttons: u32,
}

impl WiiInputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_down(&self, btn: WiiButton) -> bool {
        (self.buttons & (btn as u32)) != 0
    }

    pub fn press(&mut self, btn: WiiButton) {
        self.buttons |= btn as u32;
    }

    pub fn release(&mut self, btn: WiiButton) {
        self.buttons &= !(btn as u32);
    }

    pub fn set_button(&mut self, btn: WiiButton, pressed: bool) {
        if pressed {
            self.press(btn);
        } else {
            self.release(btn);
        }
    }

    pub fn set_stick(&mut self, x: f32, y: f32) {
        self.stick[0] = x.clamp(-1.0, 1.0);
        self.stick[1] = y.clamp(-1.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.stick = [0.0, 0.0];
        self.buttons = 0;
    }
}

#[derive(Resource)]
pub enum ActivePortal {
    Usb(std::sync::Mutex<UsbPortal>),
    Mock(MockPortal),
    None,
}

impl Default for ActivePortal {
    fn default() -> Self {
        Self::Mock(MockPortal::new())
    }
}

impl ActivePortal {
    pub fn poll_events(&mut self) -> Vec<PortalEvent> {
        match self {
            ActivePortal::Usb(usb_lock) => {
                let mut events = Vec::new();
                if let Ok(usb) = usb_lock.get_mut() {
                    while let Ok(Some(pkt)) = usb.poll_packet() {
                        if let Some(ev) = decode_portal_response(&pkt) {
                            events.push(ev.into());
                        }
                    }
                }
                events
            }
            ActivePortal::Mock(mock) => mock.poll_events().into_iter().map(Into::into).collect(),
            ActivePortal::None => Vec::new(),
        }
    }

    pub fn set_color(&mut self, zone: PortalZone, r: u8, g: u8, b: u8) {
        match self {
            ActivePortal::Usb(usb_lock) => {
                if let Ok(usb) = usb_lock.get_mut() {
                    let _ = usb.set_led_color(zone, r, g, b);
                }
            }
            ActivePortal::Mock(mock) => {
                mock.send_command(&encode_color_packet(zone, r, g, b));
            }
            ActivePortal::None => {}
        }
    }

    pub fn set_trap_light(&mut self, r: u8, g: u8, b: u8) {
        match self {
            ActivePortal::Usb(usb_lock) => {
                if let Ok(usb) = usb_lock.get_mut() {
                    let _ = usb.send_packet(&encode_trap_light_packet(r, g, b));
                }
            }
            ActivePortal::Mock(mock) => {
                mock.send_command(&encode_trap_light_packet(r, g, b));
            }
            ActivePortal::None => {}
        }
    }

    pub fn send_audio(&mut self, pkt: &[u8; REPORT_SIZE]) {
        match self {
            ActivePortal::Usb(usb_lock) => {
                if let Ok(usb) = usb_lock.get_mut() {
                    let _ = usb.send_packet(pkt);
                }
            }
            ActivePortal::Mock(mock) => {
                mock.send_command(pkt);
            }
            ActivePortal::None => {}
        }
    }
}

#[derive(Resource, Debug, Default, Clone)]
pub struct PortalGameplayState {
    pub tags: Vec<SkylanderTag>,
    pub active_hero_character_id: Option<u16>,
    pub active_villain_id: Option<u16>,
    pub portal_light_color: Option<(u8, u8, u8)>,
    /// Character IDs knocked out this level (`Portal_Manager.ai` "tag is defeated");
    /// cleared on level change.
    pub defeated_heroes: Vec<u16>,
}

#[derive(Resource, Debug, Clone)]
pub struct EnvironmentCollision {
    pub mesh: CollisionMesh,
    pub gravity: f32,
    pub jump_impulse: f32,
    pub spawn_point: Vec3,
    pub kill_plane_y: f32,
}

impl Default for EnvironmentCollision {
    fn default() -> Self {
        Self {
            mesh: CollisionMesh::synthetic_courtyard(),
            gravity: -24.0,
            jump_impulse: 9.0,
            spawn_point: Vec3::ZERO,
            kill_plane_y: -15.0,
        }
    }
}

/// Conversation in progress with an `Npc`; the HUD draws `current_line`.
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct ActiveDialogue {
    pub npc: Option<Entity>,
    /// NPC display name, used when a line has no speaker.
    pub npc_name: String,
    pub lines: Vec<skylands_formats::DialogueLine>,
    pub line: usize,
}

impl ActiveDialogue {
    pub fn is_active(&self) -> bool {
        self.npc.is_some()
    }

    /// `(speaker, text)` of the line on screen.
    pub fn current_line(&self) -> Option<(&str, &str)> {
        self.npc?;
        let l = self.lines.get(self.line)?;
        Some((l.speaker.as_deref().unwrap_or(&self.npc_name), &l.text))
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }
}

#[derive(Resource, Debug, Clone)]
pub struct GameStats {
    pub coins: u32,
    pub active_zone: String,
    pub active_prompt: Option<String>,
}

impl Default for GameStats {
    fn default() -> Self {
        Self {
            coins: 0,
            active_zone: "Courtyard".to_string(),
            active_prompt: None,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CaptureProgress {
    pub villain_id: u16,
    pub progress: f32,
    pub duration: f32,
    pub target_position: Vec3,
}

impl Default for CaptureProgress {
    fn default() -> Self {
        Self {
            villain_id: 0,
            progress: 0.0,
            duration: 2.0,
            target_position: Vec3::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveControlMode {
    #[default]
    Hero,
    Villain,
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TrappedVillainState {
    pub control_mode: ActiveControlMode,
    pub trapped_villain_id: Option<u16>,
    pub energy: crate::components::VillainEnergy,
    pub is_unlocked: bool,
    pub min_swap_energy: f32,
    pub swap_cooldown: f32,
}

impl Default for TrappedVillainState {
    fn default() -> Self {
        Self {
            control_mode: ActiveControlMode::Hero,
            trapped_villain_id: None,
            energy: crate::components::VillainEnergy::default(),
            is_unlocked: false,
            min_swap_energy: 15.0,
            swap_cooldown: 0.0,
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ZoneTransitionProgress {
    pub target_zone: String,
    pub target_level: Option<String>,
    pub target_spawn: Vec3,
    pub timer: f32,
    pub duration: f32,
}

/// Official Skylanders: Trap Team campaign chapter metadata.
pub const CHAPTER_LEVELS: &[(u8, &str, &str)] = &[
    (1, "Soda Springs", "Level_313_Tutorial.bld"),
    (2, "Know-It-All Island", "Level_300_BirdCage.bld"),
    (3, "Chompy Mountain", "Level_301_The_Machine.bld"),
    (4, "Phoenix Sanctuary", "Level_302_Chompywurm.bld"),
    (5, "Chef Zeppelin", "Level_303_DOTA.bld"),
    (6, "Rainfish Riviera", "Level_304_Water_Wave.bld"),
    (7, "Monster Marsh", "Level_305_WilikinFactory.bld"),
    (8, "Telescope Towers", "Level_306_Copter.bld"),
    (9, "Mystic Mill", "Level_307_clockworks.bld"),
    (
        10,
        "Secret Sewers of Supreme Stink",
        "Level_308_NiteLite.bld",
    ),
    (11, "Wilikin Workshop", "Level_309_Sewers.bld"),
    (12, "Time Town", "Level_310_DreamLand.bld"),
    (
        13,
        "The Future of Skylands",
        "Level_311_Arkeyan_Mechanism.bld",
    ),
    (
        14,
        "Operation: Troll Rocket Steal",
        "Level_312_Starlands.bld",
    ),
    (15, "Skyhighlands", "Level_316_PirateFortress.bld"),
    (16, "The Golden Desert", "Level_318_DOTAFortress.bld"),
    (17, "Lair of the Golden Queen", "Level_319_Pyramid.bld"),
    (18, "The Ultimate Weapon", "Level_317_FinalBattle.bld"),
];

pub fn chapter_title(chapter: u8) -> Option<&'static str> {
    CHAPTER_LEVELS
        .iter()
        .find(|(ch, _, _)| *ch == chapter)
        .map(|(_, title, _)| *title)
}

pub fn chapter_level_archive(chapter: u8) -> Option<&'static str> {
    CHAPTER_LEVELS
        .iter()
        .find(|(ch, _, _)| *ch == chapter)
        .map(|(_, _, archive)| *archive)
}

pub fn chapter_from_level_archive(archive_name: &str) -> Option<u8> {
    let norm = archive_name.to_ascii_lowercase();
    CHAPTER_LEVELS
        .iter()
        .find(|(_, _, archive)| archive.to_ascii_lowercase() == norm)
        .map(|(ch, _, _)| *ch)
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CampaignProgress {
    /// Highest unlocked chapter (1..=18).
    pub highest_unlocked_chapter: u8,
    /// Active chapter (None when in Hub or arena).
    pub current_chapter: Option<u8>,
    /// Completed chapter IDs.
    pub completed_chapters: Vec<u8>,
    /// Set of visited level archive names.
    pub visited_levels: Vec<String>,
}

impl Default for CampaignProgress {
    fn default() -> Self {
        Self {
            highest_unlocked_chapter: 1,
            current_chapter: None,
            completed_chapters: Vec::new(),
            visited_levels: vec!["Level_Hub_2014_01.bld".to_string()],
        }
    }
}

impl CampaignProgress {
    pub fn is_chapter_unlocked(&self, chapter: u8) -> bool {
        chapter <= self.highest_unlocked_chapter
    }

    pub fn complete_chapter(&mut self, chapter: u8) {
        if !self.completed_chapters.contains(&chapter) {
            self.completed_chapters.push(chapter);
            self.completed_chapters.sort_unstable();
        }
        let next_chapter = (chapter + 1).min(18);
        if next_chapter > self.highest_unlocked_chapter {
            self.highest_unlocked_chapter = next_chapter;
        }
    }

    pub fn record_level_visit(&mut self, level: &str) {
        if !self
            .visited_levels
            .iter()
            .any(|v| v.eq_ignore_ascii_case(level))
        {
            self.visited_levels.push(level.to_string());
        }
        self.current_chapter = chapter_from_level_archive(level);
    }

    pub fn chapter_for_level(&self, level: &str) -> Option<u8> {
        chapter_from_level_archive(level)
    }
}

#[derive(Resource, Debug, Default)]
pub struct PortalAudioStreaming {
    pub queue: VecDeque<[u8; REPORT_SIZE]>,
    pub sink: PortalSpeakerSink,
}

/// The active level's scripts, shared by every `ActorScript`.
#[derive(Resource, Clone)]
pub struct LevelScripts(pub std::sync::Arc<skylands_formats::ScriptProgram>);

/// Magic Moment: the portal-space sequence of a Skylander put on the portal
/// (`character/sky4_mm_streaming.bld` `mmsky4_controller.ai`, modes "world to portal",
/// "toy on", "show minion", "minion to world"). Times are seconds from placement; the
/// script's stage timers are the `MM_*` constants. `serial` changes on every placement so
/// the renderer rebuilds the scene even when the same character is placed again.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct MagicMoment {
    pub character_id: Option<u16>,
    pub name: String,
    pub elapsed: f32,
    /// Length of the hero's `ToyAlive` clip (animation state 80): the "show minion" stage 6
    /// timer.
    pub toy_alive: f32,
    pub serial: u32,
}

/// "world to portal": portal space appears after the stage 0 timer.
pub const MM_PORTAL_SPACE: f32 = 0.5;
/// "toy on" (rig "New Toy", neutral icon) lasts 1.2 s.
pub const MM_SHOW_MINION: f32 = MM_PORTAL_SPACE + 1.2;
/// "show minion" stage 1, after 1.0 s: the Skylander appears as a black silhouette in its
/// toy pose (`ToyAlive` at playback rate 0) fading to white over 0.8 s, with its 3D name;
/// rig "Magic Moment".
pub const MM_REVEAL: f32 = MM_SHOW_MINION + 1.0;
/// Stage 2 (+0.42 s): the cloud tunnel tint slides to the element colour over 0.4 s.
pub const MM_TUNNEL_TINT: f32 = MM_REVEAL + 0.42;
/// Stage 4 (+0.25 +0.15 s): element background and platform fade in over 0.3 s.
pub const MM_BACKGROUND: f32 = MM_TUNNEL_TINT + 0.25 + 0.15;
/// Stage 5 (+0.3 s): the white flash fades over 0.12 s and `ToyAlive` plays.
pub const MM_ALIVE: f32 = MM_BACKGROUND + 0.3;
/// "minion to world": rig "Hyper Jump (alt C)" (`uberModel_HyperJump_altC.ma`, 1.43 s),
/// back to the world when it ends.
pub const MM_HYPER_JUMP: f32 = 1.43;
/// `ToyAlive` length when the model has none (Gill Grunt 2.17 s, Spyro 3.33 s).
pub const MM_DEFAULT_TOY_ALIVE: f32 = 2.5;

impl MagicMoment {
    pub fn start(&mut self, character_id: u16, name: &str, toy_alive: Option<f32>) {
        self.character_id = Some(character_id);
        self.name = name.to_string();
        self.elapsed = 0.0;
        self.toy_alive = toy_alive.unwrap_or(MM_DEFAULT_TOY_ALIVE);
        self.serial = self.serial.wrapping_add(1);
    }

    /// "minion to world" start.
    pub fn to_world(&self) -> f32 {
        MM_ALIVE + self.toy_alive
    }

    pub fn duration(&self) -> f32 {
        self.to_world() + MM_HYPER_JUMP
    }

    pub fn is_active(&self) -> bool {
        self.character_id.is_some() && self.elapsed < self.duration()
    }
}
