use crate::protocol::{
    PortalEvent, PortalZone, CMD_QUERY, CMD_READY, REPORT_SIZE,
};
use std::collections::HashMap;

pub struct MockPortal {
    pub antenna_active: bool,
    pub colors: HashMap<u8, (u8, u8, u8)>,
    pub trap_light: (u8, u8, u8),
    pub loaded_tags: HashMap<u8, Vec<[u8; 16]>>,
    pending_events: Vec<PortalEvent>,
}

impl MockPortal {
    pub fn new() -> Self {
        Self {
            antenna_active: false,
            colors: HashMap::new(),
            trap_light: (0, 0, 0),
            loaded_tags: HashMap::new(),
            pending_events: Vec::new(),
        }
    }

    pub fn insert_tag(&mut self, slot: u8, blocks: Vec<[u8; 16]>) {
        self.loaded_tags.insert(slot, blocks);
        let zone = if slot == 0 {
            PortalZone::TrapSlot
        } else {
            PortalZone::LeftRing
        };
        self.pending_events.push(PortalEvent::TagArrived {
            tag_index: slot,
            zone,
        });
    }

    pub fn remove_tag(&mut self, slot: u8) {
        self.loaded_tags.remove(&slot);
        self.pending_events.push(PortalEvent::TagRemoved { tag_index: slot });
    }

    pub fn send_command(&mut self, pkt: &[u8; REPORT_SIZE]) -> Option<[u8; REPORT_SIZE]> {
        match pkt[0] {
            CMD_READY => {
                let mut resp = [0u8; REPORT_SIZE];
                resp[0] = CMD_READY;
                resp[1] = 1; // Major
                resp[2] = 0; // Minor
                Some(resp)
            }
            crate::protocol::CMD_ANTENNA => {
                self.antenna_active = pkt[1] != 0;
                None
            }
            crate::protocol::CMD_COLOR => {
                self.colors.insert(pkt[1], (pkt[2], pkt[3], pkt[4]));
                None
            }
            crate::protocol::CMD_TRAP_LIGHT => {
                self.trap_light = (pkt[1], pkt[2], pkt[3]);
                None
            }
            CMD_QUERY => {
                let slot = pkt[1];
                let block = pkt[2] as usize;
                if let Some(blocks) = self.loaded_tags.get(&slot) {
                    if block < blocks.len() {
                        let mut resp = [0u8; REPORT_SIZE];
                        resp[0] = CMD_QUERY;
                        resp[1] = slot;
                        resp[2] = block as u8;
                        resp[3..19].copy_from_slice(&blocks[block]);
                        return Some(resp);
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn poll_events(&mut self) -> Vec<PortalEvent> {
        let events = self.pending_events.clone();
        self.pending_events.clear();
        events
    }
}

impl Default for MockPortal {
    fn default() -> Self {
        Self::new()
    }
}
