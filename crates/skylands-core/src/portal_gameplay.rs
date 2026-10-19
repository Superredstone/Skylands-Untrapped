use crate::actor::World;
use crate::audio::AudioEngine;
use crate::state::{GameState, GameStateMachine};
use skylands_portal::protocol::{
    encode_color_packet, encode_query_packet, encode_trap_light_packet, PortalEvent, PortalZone,
    REPORT_SIZE,
};
use skylands_portal::tag::SkylanderTag;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    Air,
    Life,
    Undead,
    Earth,
    Fire,
    Water,
    Magic,
    Tech,
    Kaos,
}

impl Element {
    pub fn rgb(&self) -> (u8, u8, u8) {
        match self {
            Element::Air => (0, 220, 255),
            Element::Life => (0, 255, 50),
            Element::Undead => (160, 0, 255),
            Element::Earth => (210, 180, 30),
            Element::Fire => (255, 40, 0),
            Element::Water => (0, 140, 255),
            Element::Magic => (255, 0, 180),
            Element::Tech => (255, 120, 0),
            Element::Kaos => (255, 0, 60),
        }
    }
}

pub fn get_character_info(character_id: u16) -> (&'static str, Element) {
    match character_id {
        400 => ("Snap Shot", Element::Water),
        401 => ("Food Fight", Element::Life),
        402 => ("Wildfire", Element::Fire),
        403 => ("Wallop", Element::Earth),
        404 => ("Gearshift", Element::Tech),
        405 => ("Krypt King", Element::Undead),
        406 => ("Enigma", Element::Magic),
        407 => ("Gusto", Element::Air),
        1010 => ("Kaos", Element::Kaos),
        1001 => ("The Gulper", Element::Water),
        1002 => ("Wolfgang", Element::Undead),
        1003 => ("Chompy Mage", Element::Life),
        1004 => ("Golden Queen", Element::Earth),
        1005 => ("Chef Pepper Jack", Element::Fire),
        1006 => ("Dr. Krankcase", Element::Tech),
        1007 => ("Dreamcatcher", Element::Air),
        1008 => ("Pain-Yatta", Element::Magic),
        3000..=3100 => ("Trap", Element::Kaos),
        _ => ("Unknown Skylander", Element::Magic),
    }
}

pub struct PortalGameplay {
    pub active_tags: HashMap<u8, (PortalZone, SkylanderTag)>,
    pending_blocks: HashMap<u8, HashMap<u8, [u8; 16]>>,
}

impl PortalGameplay {
    pub fn new() -> Self {
        Self {
            active_tags: HashMap::new(),
            pending_blocks: HashMap::new(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: PortalEvent,
        world: &mut World,
        state: &mut GameStateMachine,
        audio: &mut AudioEngine,
    ) -> Vec<[u8; REPORT_SIZE]> {
        let mut outgoing = Vec::new();

        match event {
            PortalEvent::TagArrived { tag_index, zone } => {
                outgoing.push(encode_query_packet(tag_index, 0));
                outgoing.push(encode_query_packet(tag_index, 1));
                outgoing.push(encode_query_packet(tag_index, 8));
                self.pending_blocks.insert(tag_index, HashMap::new());

                let is_trap = zone == PortalZone::TrapSlot;
                let default_tag = SkylanderTag {
                    uid: [tag_index, 0, 0, 0],
                    character_id: if is_trap { 3010 } else { 400 },
                    variant: if is_trap { 1010 } else { 0 },
                    nickname: String::new(),
                    xp: 0,
                    money: 0,
                    is_trap,
                    villain_id: if is_trap { Some(1010) } else { None },
                };
                let cmds = self.on_tag_identified(
                    tag_index,
                    zone,
                    default_tag,
                    world,
                    state,
                    audio,
                );
                outgoing.extend(cmds);
            }
            PortalEvent::TagData {
                tag_index,
                block,
                data,
            } => {
                if let Some(blocks) = self.pending_blocks.get_mut(&tag_index) {
                    blocks.insert(block, data);
                    if blocks.contains_key(&0) && blocks.contains_key(&1) && blocks.contains_key(&8) {
                        let b0 = blocks[&0];
                        let b1 = blocks[&1];
                        let b8 = blocks[&8];
                        let parsed = SkylanderTag::parse_blocks(&b0, &b1, &b8);
                        let zone = if tag_index == 0 {
                            PortalZone::TrapSlot
                        } else {
                            PortalZone::LeftRing
                        };
                        let cmds = self.on_tag_identified(
                            tag_index,
                            zone,
                            parsed,
                            world,
                            state,
                            audio,
                        );
                        outgoing.extend(cmds);
                    }
                }
            }
            PortalEvent::TagRemoved { tag_index } => {
                outgoing.extend(self.on_tag_removed(tag_index, world, state));
            }
            _ => {}
        }

        outgoing
    }

    pub fn on_tag_identified(
        &mut self,
        slot: u8,
        zone: PortalZone,
        tag: SkylanderTag,
        world: &mut World,
        state: &mut GameStateMachine,
        audio: &mut AudioEngine,
    ) -> Vec<[u8; REPORT_SIZE]> {
        let mut cmds = Vec::new();
        let (name, elem) = get_character_info(tag.character_id);

        if tag.is_trap || zone == PortalZone::TrapSlot {
            let villain = tag.villain_id.unwrap_or(tag.variant);
            state.set_state(GameState::VillainCapture {
                villain_id: villain,
                progress: 0.0,
            });
            audio.on_villain_trapped(villain);

            let (r, g, b) = elem.rgb();
            cmds.push(encode_trap_light_packet(r, g, b));
        } else {
            world.spawn(
                tag.character_id,
                tag.variant,
                name,
                [0.0, 0.2, 0.0],
                false,
            );
            audio.on_hero_summoned();

            let (r, g, b) = elem.rgb();
            cmds.push(encode_color_packet(zone, r, g, b));
        }

        self.active_tags.insert(slot, (zone, tag));
        cmds
    }

    pub fn on_tag_removed(
        &mut self,
        slot: u8,
        world: &mut World,
        state: &mut GameStateMachine,
    ) -> Vec<[u8; REPORT_SIZE]> {
        let mut cmds = Vec::new();
        self.pending_blocks.remove(&slot);

        if let Some((zone, tag)) = self.active_tags.remove(&slot) {
            if tag.is_trap || zone == PortalZone::TrapSlot {
                cmds.push(encode_trap_light_packet(0, 0, 0));
                if state.is_villain_capture() {
                    state.set_state(GameState::Hub {
                        academy_area: "Academy Grounds".into(),
                    });
                }
            } else {
                world.despawn_by_character_id(tag.character_id);
                cmds.push(encode_color_packet(zone, 0, 0, 0));
            }
        }

        cmds
    }
}

impl Default for PortalGameplay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hero_tag_spawn_and_lighting() {
        let mut pg = PortalGameplay::new();
        let mut world = World::new();
        let mut state = GameStateMachine::new();
        let mut audio = AudioEngine::new();

        let event = PortalEvent::TagArrived {
            tag_index: 1,
            zone: PortalZone::LeftRing,
        };

        let cmds = pg.handle_event(event, &mut world, &mut state, &mut audio);
        assert!(!cmds.is_empty());
        assert_eq!(world.actors.len(), 1);
        assert_eq!(world.actors[0].character_id, 400);
    }

    #[test]
    fn test_trap_tag_villain_capture() {
        let mut pg = PortalGameplay::new();
        let mut world = World::new();
        let mut state = GameStateMachine::new();
        let mut audio = AudioEngine::new();

        let event = PortalEvent::TagArrived {
            tag_index: 0,
            zone: PortalZone::TrapSlot,
        };

        let cmds = pg.handle_event(event, &mut world, &mut state, &mut audio);
        assert!(!cmds.is_empty());
        assert!(state.is_villain_capture());
        assert!(audio.portal.is_streaming());
    }
}
