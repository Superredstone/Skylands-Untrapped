use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkylanderTag {
    pub uid: [u8; 4],
    pub character_id: u16,
    pub variant: u16,
    pub nickname: String,
    pub xp: u32,
    pub money: u16,
    pub is_trap: bool,
    pub villain_id: Option<u16>,
}

impl SkylanderTag {
    pub fn parse_blocks(block0: &[u8; 16], block1: &[u8; 16], block8: &[u8; 16]) -> Self {
        let mut uid = [0u8; 4];
        uid.copy_from_slice(&block0[..4]);

        let mut rdr1 = Cursor::new(block1);
        let character_id = rdr1.read_u16::<LittleEndian>().unwrap_or(0);
        let variant = rdr1.read_u16::<LittleEndian>().unwrap_or(0);

        let mut rdr8 = Cursor::new(block8);
        let xp = rdr8.read_u32::<LittleEndian>().unwrap_or(0);
        let money = rdr8.read_u16::<LittleEndian>().unwrap_or(0);

        // Traps character ID range: 3000..3100
        let is_trap = (3000..=3100).contains(&character_id);
        let villain_id = if is_trap { Some(variant) } else { None };

        Self {
            uid,
            character_id,
            variant,
            nickname: String::new(),
            xp,
            money,
            is_trap,
            villain_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trap_tag_detection() {
        let block0 = [0x11, 0x22, 0x33, 0x44, 0,0,0,0,0,0,0,0,0,0,0,0];
        let mut block1 = [0u8; 16];
        block1[0..2].copy_from_slice(&3010u16.to_le_bytes()); // Kaos Trap
        block1[2..4].copy_from_slice(&1010u16.to_le_bytes()); // Kaos Villain ID
        let block8 = [0u8; 16];

        let tag = SkylanderTag::parse_blocks(&block0, &block1, &block8);
        assert!(tag.is_trap);
        assert_eq!(tag.character_id, 3010);
        assert_eq!(tag.villain_id, Some(1010));
    }
}
