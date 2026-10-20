use crate::audio::PortalSpeakerSink;
use crate::collision::CollisionMesh;
use crate::events::PortalEvent;
use bevy::prelude::*;
use skylands_portal::protocol::{
    decode_portal_response, encode_color_packet, encode_trap_light_packet, PortalZone, REPORT_SIZE,
};
use skylands_portal::{MockPortal, SkylanderTag, UsbPortal};
use std::collections::VecDeque;

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

#[derive(Resource, Debug, Clone, Default)]
pub struct CaptureProgress {
    pub villain_id: u16,
    pub progress: f32,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ZoneTransitionProgress {
    pub target_zone: String,
    pub target_spawn: Vec3,
    pub timer: f32,
    pub duration: f32,
}

#[derive(Resource, Debug, Default)]
pub struct PortalAudioStreaming {
    pub queue: VecDeque<[u8; REPORT_SIZE]>,
    pub sink: PortalSpeakerSink,
}
