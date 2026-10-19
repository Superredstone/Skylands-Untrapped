use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read};
use thiserror::Error;

pub const IGB_MAGIC: &[u8; 4] = b"IGBF";

#[derive(Error, Debug)]
pub enum IgbError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid IGB magic: expected IGBF, got {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("Unsupported IGB version: {0}")]
    UnsupportedVersion(u32),
}

#[derive(Debug, Clone)]
pub struct IgbHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub object_count: u32,
    pub string_table_offset: u32,
}

impl IgbHeader {
    pub fn parse<R: Read>(mut reader: R) -> Result<Self, IgbError> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != IGB_MAGIC {
            return Err(IgbError::InvalidMagic(magic));
        }

        let mut rest = [0u8; 12];
        reader.read_exact(&mut rest)?;
        let mut cur = Cursor::new(&rest);

        // Try little endian first, fall back to big endian
        let version = cur.read_u32::<LittleEndian>()?;
        let (version, object_count, string_table_offset) = if version < 0x1000 {
            (
                version,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
            )
        } else {
            let mut cur_be = Cursor::new(&rest);
            (
                cur_be.read_u32::<BigEndian>()?,
                cur_be.read_u32::<BigEndian>()?,
                cur_be.read_u32::<BigEndian>()?,
            )
        };

        Ok(Self {
            magic,
            version,
            object_count,
            string_table_offset,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_igb_parse() {
        let mut data = Vec::new();
        data.extend_from_slice(b"IGBF");
        data.extend_from_slice(&0x08u32.to_le_bytes()); // version 8
        data.extend_from_slice(&15u32.to_le_bytes()); // 15 objects
        data.extend_from_slice(&0x400u32.to_le_bytes()); // string table

        let hdr = IgbHeader::parse(Cursor::new(data)).expect("Valid IGB");
        assert_eq!(&hdr.magic, b"IGBF");
        assert_eq!(hdr.version, 8);
        assert_eq!(hdr.object_count, 15);
    }
}
