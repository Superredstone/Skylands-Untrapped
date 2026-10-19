pub const TRAPTANIUM_VID: u16 = 0x1430;
pub const TRAPTANIUM_PID: u16 = 0x0150;
pub const SWAPFORCE_PID: u16 = 0x01ec;

pub const REPORT_SIZE: usize = 32;

// Protocol Commands
pub const CMD_READY: u8 = b'R'; // 0x52
pub const CMD_ANTENNA: u8 = b'A'; // 0x41
pub const CMD_COLOR: u8 = b'C'; // 0x43
pub const CMD_QUERY: u8 = b'Q'; // 0x51
pub const CMD_WRITE: u8 = b'W'; // 0x57
pub const CMD_AUDIO: u8 = b'V'; // 0x56 - Villain speaker audio stream
pub const CMD_TRAP_LIGHT: u8 = b'J'; // 0x4A - Trap slot light

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalZone {
    TrapSlot = 0,
    LeftRing = 1,
    RightRing = 2,
    All = 0xFF,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub fn encode_ready_packet() -> [u8; REPORT_SIZE] {
    let mut pkt = [0u8; REPORT_SIZE];
    pkt[0] = CMD_READY;
    pkt
}

pub fn encode_antenna_packet(enable: bool) -> [u8; REPORT_SIZE] {
    let mut pkt = [0u8; REPORT_SIZE];
    pkt[0] = CMD_ANTENNA;
    pkt[1] = if enable { 1 } else { 0 };
    pkt
}

pub fn encode_color_packet(zone: PortalZone, r: u8, g: u8, b: u8) -> [u8; REPORT_SIZE] {
    let mut pkt = [0u8; REPORT_SIZE];
    pkt[0] = CMD_COLOR;
    pkt[1] = zone as u8;
    pkt[2] = r;
    pkt[3] = g;
    pkt[4] = b;
    pkt
}

pub fn encode_trap_light_packet(r: u8, g: u8, b: u8) -> [u8; REPORT_SIZE] {
    let mut pkt = [0u8; REPORT_SIZE];
    pkt[0] = CMD_TRAP_LIGHT;
    pkt[1] = r;
    pkt[2] = g;
    pkt[3] = b;
    pkt
}

pub fn encode_query_packet(tag_index: u8, block: u8) -> [u8; REPORT_SIZE] {
    let mut pkt = [0u8; REPORT_SIZE];
    pkt[0] = CMD_QUERY;
    pkt[1] = tag_index;
    pkt[2] = block;
    pkt
}

pub fn encode_write_packet(tag_index: u8, block: u8, data: &[u8; 16]) -> [u8; REPORT_SIZE] {
    let mut pkt = [0u8; REPORT_SIZE];
    pkt[0] = CMD_WRITE;
    pkt[1] = tag_index;
    pkt[2] = block;
    pkt[3..19].copy_from_slice(data);
    pkt
}

pub fn encode_audio_packet(seq: u8, pcm_chunk: &[u8]) -> [u8; REPORT_SIZE] {
    let mut pkt = [0u8; REPORT_SIZE];
    pkt[0] = CMD_AUDIO;
    pkt[1] = seq;
    let len = pcm_chunk.len().min(REPORT_SIZE - 2);
    pkt[2..2 + len].copy_from_slice(&pcm_chunk[..len]);
    pkt
}

pub fn decode_portal_response(pkt: &[u8]) -> Option<PortalEvent> {
    if pkt.is_empty() {
        return None;
    }

    match pkt[0] {
        CMD_READY if pkt.len() >= 3 => Some(PortalEvent::Ready {
            firmware_major: pkt[1],
            firmware_minor: pkt[2],
        }),
        CMD_QUERY if pkt.len() >= 19 => {
            let mut data = [0u8; 16];
            data.copy_from_slice(&pkt[3..19]);
            Some(PortalEvent::TagData {
                tag_index: pkt[1],
                block: pkt[2],
                data,
            })
        }
        CMD_WRITE if pkt.len() >= 4 => Some(PortalEvent::WriteAck {
            tag_index: pkt[1],
            block: pkt[2],
            success: pkt[3] == 0x01,
        }),
        b'S' if pkt.len() >= 2 => {
            // Tag status change
            let status = pkt[1];
            if status & 0x01 != 0 {
                Some(PortalEvent::TagArrived {
                    tag_index: 0,
                    zone: PortalZone::TrapSlot,
                })
            } else {
                Some(PortalEvent::TagRemoved { tag_index: 0 })
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode() {
        let pkt = encode_color_packet(PortalZone::TrapSlot, 255, 0, 128);
        assert_eq!(pkt[0], CMD_COLOR);
        assert_eq!(pkt[1], 0);
        assert_eq!(pkt[2], 255);
        assert_eq!(pkt[4], 128);

        let query_resp = [
            CMD_QUERY, 0x01, 0x02, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let event = decode_portal_response(&query_resp).expect("Parsed event");
        match event {
            PortalEvent::TagData {
                tag_index,
                block,
                data,
            } => {
                assert_eq!(tag_index, 1);
                assert_eq!(block, 2);
                assert_eq!(data[0], 1);
                assert_eq!(data[15], 16);
            }
            _ => panic!("Expected TagData"),
        }
    }
}
