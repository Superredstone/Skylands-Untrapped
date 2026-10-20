use crate::components::AttackKind;
use bevy::prelude::*;
use skylands_portal::protocol::PortalZone;

#[derive(Debug, Clone, PartialEq)]
pub enum WorldAudioCue {
    PickupCollected,
    HeroAttack(AttackKind),
    AttackHit,
    PropDestroyed,
    ZoneEntered(String),
}

#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub enum PortalEvent {
    Ready {
        firmware_major: u8,
        firmware_minor: u8,
    },
    TagArrived {
        tag_index: u8,
        zone: PortalZone,
    },
    TagRemoved {
        tag_index: u8,
    },
    TagData {
        tag_index: u8,
        block: u8,
        data: [u8; 16],
    },
    WriteAck {
        tag_index: u8,
        block: u8,
        success: bool,
    },
}

impl From<skylands_portal::protocol::PortalEvent> for PortalEvent {
    fn from(ev: skylands_portal::protocol::PortalEvent) -> Self {
        match ev {
            skylands_portal::protocol::PortalEvent::Ready {
                firmware_major,
                firmware_minor,
            } => Self::Ready {
                firmware_major,
                firmware_minor,
            },
            skylands_portal::protocol::PortalEvent::TagArrived { tag_index, zone } => {
                Self::TagArrived { tag_index, zone }
            }
            skylands_portal::protocol::PortalEvent::TagRemoved { tag_index } => {
                Self::TagRemoved { tag_index }
            }
            skylands_portal::protocol::PortalEvent::TagData {
                tag_index,
                block,
                data,
            } => Self::TagData {
                tag_index,
                block,
                data,
            },
            skylands_portal::protocol::PortalEvent::WriteAck {
                tag_index,
                block,
                success,
            } => Self::WriteAck {
                tag_index,
                block,
                success,
            },
        }
    }
}

#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct DamageEvent {
    pub target: Entity,
    pub damage: u32,
    pub position: Vec3,
}

#[derive(Event, Debug, Clone, PartialEq)]
pub struct WorldAudioCueEvent(pub WorldAudioCue);

#[derive(Event, Debug, Clone, PartialEq)]
pub struct ZoneTransitionEvent {
    pub target_zone: String,
    pub target_spawn: Vec3,
}
