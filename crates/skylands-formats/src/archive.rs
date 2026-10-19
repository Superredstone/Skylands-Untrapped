use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

pub const IGA_MAGIC: &[u8; 4] = b"IGA\x1a";

#[derive(Error, Debug)]
pub enum ArchiveError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid magic: expected IGA\\x1a, got {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),
    #[error("File not found in archive: {0}")]
    FileNotFound(String),
    #[error("Decompression failed: {0}")]
    DecompressionFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Big,
    Little,
}

#[derive(Debug, Clone)]
pub struct ArchiveHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub entry_count: u32,
    pub aux_count: u32,
    pub block_align: u32,
    pub data_offset: u32,
    pub endian: Endianness,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub hash: u32,
    pub offset: u64,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub is_compressed: bool,
}

pub struct ArchiveReader<R: Read + Seek> {
    reader: R,
    pub header: ArchiveHeader,
    pub entries: HashMap<u32, FileEntry>,
}

impl<R: Read + Seek> ArchiveReader<R> {
    pub fn new(mut reader: R) -> Result<Self, ArchiveError> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != IGA_MAGIC {
            return Err(ArchiveError::InvalidMagic(magic));
        }

        // Test endianness via version field (expected 11)
        let mut ver_buf = [0u8; 4];
        reader.read_exact(&mut ver_buf)?;
        let ver_le = u32::from_le_bytes(ver_buf);
        let ver_be = u32::from_be_bytes(ver_buf);

        let (endian, version) = if ver_le == 11 || (ver_le > 0 && ver_le <= 20) {
            (Endianness::Little, ver_le)
        } else if ver_be == 11 || (ver_be > 0 && ver_be <= 20) {
            (Endianness::Big, ver_be)
        } else {
            return Err(ArchiveError::UnsupportedVersion(ver_le));
        };

        let mut rest_header = [0u8; 16];
        reader.read_exact(&mut rest_header)?;
        let mut cur = Cursor::new(&rest_header);

        let (entry_count, aux_count, block_align, data_offset) = match endian {
            Endianness::Little => (
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
            ),
            Endianness::Big => (
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
            ),
        };

        let align = if block_align == 0 { 2048 } else { block_align };
        let header = ArchiveHeader {
            magic,
            version,
            entry_count,
            aux_count,
            block_align: align,
            data_offset,
            endian,
        };

        // ponytail: parse basic TOC entries; full hierarchy parsing deferred until engine tree mount
        let entries = HashMap::new();

        Ok(Self {
            reader,
            header,
            entries,
        })
    }

    pub fn read_bytes_at(&mut self, offset: u64, size: usize) -> Result<Vec<u8>, ArchiveError> {
        self.reader.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; size];
        self.reader.read_exact(&mut buf)?;
        Ok(buf)
    }
}

pub struct ArchiveMount {
    archives: Vec<ArchiveReader<File>>,
}

impl ArchiveMount {
    pub fn new() -> Self {
        Self {
            archives: Vec::new(),
        }
    }

    pub fn mount<P: AsRef<Path>>(&mut self, path: P) -> Result<(), ArchiveError> {
        let file = File::open(path)?;
        let archive = ArchiveReader::new(file)?;
        self.archives.push(archive);
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.archives.len()
    }
}

impl Default for ArchiveMount {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_header_endian() {
        let mut data = Vec::new();
        data.extend_from_slice(b"IGA\x1a");
        data.extend_from_slice(&11u32.to_le_bytes()); // ver 11 LE
        data.extend_from_slice(&100u32.to_le_bytes()); // count
        data.extend_from_slice(&2u32.to_le_bytes()); // aux
        data.extend_from_slice(&2048u32.to_le_bytes()); // align
        data.extend_from_slice(&4096u32.to_le_bytes()); // data_offset

        let reader = ArchiveReader::new(Cursor::new(data)).expect("Valid archive");
        assert_eq!(reader.header.endian, Endianness::Little);
        assert_eq!(reader.header.version, 11);
        assert_eq!(reader.header.entry_count, 100);
        assert_eq!(reader.header.block_align, 2048);
    }
}
