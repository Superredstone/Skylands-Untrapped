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

pub fn get_character_voice_path(character_id: u16) -> Option<&'static str> {
    match character_id {
        1010 => Some("audio/dialogue/vo_kaos_trapped.wav"),
        1001 => Some("audio/dialogue/vo_thegulper_trapped.wav"),
        1002 => Some("audio/dialogue/vo_wolfgang_trapped.wav"),
        1003 => Some("audio/dialogue/vo_chompymage_trapped.wav"),
        1004 => Some("audio/dialogue/vo_goldenqueen_trapped.wav"),
        1005 => Some("audio/dialogue/vo_chefpepperjack_trapped.wav"),
        1006 => Some("audio/dialogue/vo_drkrankcase_trapped.wav"),
        1007 => Some("audio/dialogue/vo_dreamcatcher_trapped.wav"),
        1008 => Some("audio/dialogue/vo_painyatta_trapped.wav"),
        _ => None,
    }
}

pub fn get_character_asset_paths(character_id: u16) -> (&'static str, Option<&'static str>) {
    match character_id {
        400 | 462 => (
            "462_Water_Crocky.bld",
            Some("textures/characters/snapshot.png"),
        ),
        1010 => ("1010_Kaos.bld", Some("textures/characters/kaos.png")),
        401 => (
            "models/characters/foodfight.igb",
            Some("textures/characters/foodfight.png"),
        ),
        402 => (
            "models/characters/wildfire.igb",
            Some("textures/characters/wildfire.png"),
        ),
        403 => (
            "models/characters/wallop.igb",
            Some("textures/characters/wallop.png"),
        ),
        404 => (
            "models/characters/gearshift.igb",
            Some("textures/characters/gearshift.png"),
        ),
        405 => (
            "models/characters/kryptking.igb",
            Some("textures/characters/kryptking.png"),
        ),
        406 => (
            "models/characters/enigma.igb",
            Some("textures/characters/enigma.png"),
        ),
        407 => (
            "models/characters/gusto.igb",
            Some("textures/characters/gusto.png"),
        ),
        1001 => (
            "models/characters/thegulper.igb",
            Some("textures/characters/thegulper.png"),
        ),
        1002 => (
            "models/characters/wolfgang.igb",
            Some("textures/characters/wolfgang.png"),
        ),
        1003 => (
            "models/characters/chompymage.igb",
            Some("textures/characters/chompymage.png"),
        ),
        1004 => (
            "models/characters/goldenqueen.igb",
            Some("textures/characters/goldenqueen.png"),
        ),
        1005 => (
            "models/characters/chefpepperjack.igb",
            Some("textures/characters/chefpepperjack.png"),
        ),
        1006 => (
            "models/characters/drkrankcase.igb",
            Some("textures/characters/drkrankcase.png"),
        ),
        1007 => (
            "models/characters/dreamcatcher.igb",
            Some("textures/characters/dreamcatcher.png"),
        ),
        1008 => (
            "models/characters/painyatta.igb",
            Some("textures/characters/painyatta.png"),
        ),
        3000..=3100 => (
            "models/characters/trap.igb",
            Some("textures/characters/trap.png"),
        ),
        _ => ("models/hero.igb", None),
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
                let cmds =
                    self.on_tag_identified(tag_index, zone, default_tag, world, state, audio);
                outgoing.extend(cmds);
            }
            PortalEvent::TagData {
                tag_index,
                block,
                data,
            } => {
                if let Some(blocks) = self.pending_blocks.get_mut(&tag_index) {
                    blocks.insert(block, data);
                    if blocks.contains_key(&0) && blocks.contains_key(&1) && blocks.contains_key(&8)
                    {
                        let b0 = blocks[&0];
                        let b1 = blocks[&1];
                        let b8 = blocks[&8];
                        let parsed = SkylanderTag::parse_blocks(&b0, &b1, &b8);
                        let zone = if tag_index == 0 {
                            PortalZone::TrapSlot
                        } else {
                            PortalZone::LeftRing
                        };
                        let cmds =
                            self.on_tag_identified(tag_index, zone, parsed, world, state, audio);
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
        // If slot already has an active tag with different character, remove it first
        let mut cmds = if let Some((_, old_tag)) = self.active_tags.get(&slot) {
            let old_id = if old_tag.is_trap {
                old_tag.villain_id.unwrap_or(old_tag.variant)
            } else {
                old_tag.character_id
            };
            let new_id = if tag.is_trap || zone == PortalZone::TrapSlot {
                tag.villain_id.unwrap_or(tag.variant)
            } else {
                tag.character_id
            };
            if old_id != new_id {
                self.on_tag_removed(slot, world, state)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

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

            let (model_path, tex_path) = get_character_asset_paths(villain);
            if world.find_by_character_id(villain).is_none() {
                world.spawn(
                    villain,
                    tag.variant,
                    name,
                    [0.0, 0.2, 0.0],
                    true,
                    Some(model_path.to_string()),
                    tex_path.map(|s| s.to_string()),
                );
            }
        } else {
            let (model_path, tex_path) = get_character_asset_paths(tag.character_id);
            if world.find_by_character_id(tag.character_id).is_none() {
                world.spawn(
                    tag.character_id,
                    tag.variant,
                    name,
                    [0.0, 0.2, 0.0],
                    false,
                    Some(model_path.to_string()),
                    tex_path.map(|s| s.to_string()),
                );
                audio.on_hero_summoned();
            }

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
                let villain = tag.villain_id.unwrap_or(tag.variant);
                world.despawn_by_character_id(villain);
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
        assert_eq!(
            world.actors[0].model_path,
            Some("462_Water_Crocky.bld".to_string())
        );
        assert_eq!(
            world.actors[0].texture_path,
            Some("textures/characters/snapshot.png".to_string())
        );
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
        assert_eq!(world.actors.len(), 1);
        assert_eq!(world.actors[0].character_id, 1010);
        assert_eq!(
            world.actors[0].model_path,
            Some("1010_Kaos.bld".to_string())
        );
        assert_eq!(
            world.actors[0].texture_path,
            Some("textures/characters/kaos.png".to_string())
        );
    }

    #[test]
    fn test_get_character_asset_paths() {
        assert_eq!(
            get_character_asset_paths(400),
            (
                "462_Water_Crocky.bld",
                Some("textures/characters/snapshot.png")
            )
        );
        assert_eq!(
            get_character_asset_paths(1010),
            ("1010_Kaos.bld", Some("textures/characters/kaos.png"))
        );
        assert_eq!(
            get_character_asset_paths(401),
            (
                "models/characters/foodfight.igb",
                Some("textures/characters/foodfight.png")
            )
        );
        assert_eq!(get_character_asset_paths(9999), ("models/hero.igb", None));
    }
}
