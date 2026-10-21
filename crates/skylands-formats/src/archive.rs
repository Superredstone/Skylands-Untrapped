use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use crc32fast::Hasher;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use thiserror::Error;

pub const IGA_MAGIC: &[u8; 4] = b"IGA\x1a";
pub const CHUNK_SIZE: usize = 32768;
pub const MEDIUM_BLOCK_THRESHOLD: u32 = 256 * 1024;

#[derive(Error, Debug)]
pub enum ArchiveError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid magic: expected IGA\\x1a, got {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),
    #[error("File not found in archive: hash 0x{0:08x}")]
    FileNotFound(u32),
    #[error("Decompression failed: {0}")]
    DecompressionFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Big,
    Little,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionFormat {
    Uncompressed,
    Deflate,
    Lzma,
    Unknown(u32),
}

impl From<u32> for CompressionFormat {
    fn from(val: u32) -> Self {
        match val {
            0 => CompressionFormat::Uncompressed,
            1 => CompressionFormat::Deflate,
            2 => CompressionFormat::Lzma,
            other => CompressionFormat::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub toc_size: u32,
    pub num_files: u32,
    pub sector_size: u32,
    pub hash_search_divider: u32,
    pub hash_search_slop: u32,
    pub num_large_file_blocks: u32,
    pub num_medium_file_blocks: u32,
    pub num_small_file_blocks: u32,
    pub name_table_offset: u64,
    pub name_table_size: u64,
    pub endian: Endianness,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub hash: u32,
    pub offset: u64,
    pub v1: u32,
    pub uncompressed_size: u32,
    pub v3: u32,
    pub format: CompressionFormat,
    pub first_block: u32,
    pub is_compressed: bool,
}

pub struct ArchiveReader<R: Read + Seek> {
    reader: Mutex<R>,
    pub header: ArchiveHeader,
    pub entries: HashMap<u32, FileEntry>,
    pub medium_file_blocks: Vec<u16>,
    pub small_file_blocks: Vec<u8>,
    pub file_names: HashMap<u32, String>,
    pub name_to_hash: HashMap<String, u32>,
    pub source_name: String,
}

pub fn hash_path(path: &str) -> u32 {
    hash_fnv1a(path)
}

pub fn hash_crc32(path: &str) -> u32 {
    let normalized = path.replace('\\', "/");
    let mut hasher = Hasher::new();
    hasher.update(normalized.as_bytes());
    hasher.finalize()
}

pub fn hash_fnv1a(path: &str) -> u32 {
    let normalized = path.replace('\\', "/");
    let mut hash: u32 = 0x811c_9dc5;
    for &b in normalized.as_bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

impl<R: Read + Seek> ArchiveReader<R> {
    pub fn new(mut reader: R) -> Result<Self, ArchiveError> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != IGA_MAGIC {
            return Err(ArchiveError::InvalidMagic(magic));
        }

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

        let mut rest_header = [0u8; 48];
        reader.read_exact(&mut rest_header)?;
        let mut cur = Cursor::new(&rest_header);

        let (
            toc_size,
            num_files,
            sector_size,
            hash_search_divider,
            hash_search_slop,
            num_large_file_blocks,
            num_medium_file_blocks,
            num_small_file_blocks,
            name_table_offset,
            name_table_size,
        ) = match endian {
            Endianness::Little => (
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u64::<LittleEndian>()?,
                cur.read_u64::<LittleEndian>()?,
            ),
            Endianness::Big => (
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u32::<BigEndian>()?,
                cur.read_u64::<BigEndian>()?,
                cur.read_u64::<BigEndian>()?,
            ),
        };

        let sector_size = if sector_size == 0 { 2048 } else { sector_size };

        let header = ArchiveHeader {
            magic,
            version,
            toc_size,
            num_files,
            sector_size,
            hash_search_divider,
            hash_search_slop,
            num_large_file_blocks,
            num_medium_file_blocks,
            num_small_file_blocks,
            name_table_offset,
            name_table_size,
            endian,
        };

        // 1. _fileIdTable: [u32; num_files]
        let mut file_ids = Vec::with_capacity(num_files as usize);
        for _ in 0..num_files {
            let hash = match endian {
                Endianness::Little => reader.read_u32::<LittleEndian>()?,
                Endianness::Big => reader.read_u32::<BigEndian>()?,
            };
            file_ids.push(hash);
        }

        // 2. _fileInfoTable: num_files entries (16 bytes each: offset: u32, v1: u32, uncompressed_size: u32, v3: u32)
        let mut entries = HashMap::with_capacity(num_files as usize);
        for &hash in &file_ids {
            let (raw_offset, v1, uncompressed_size, v3) = match endian {
                Endianness::Little => (
                    reader.read_u32::<LittleEndian>()?,
                    reader.read_u32::<LittleEndian>()?,
                    reader.read_u32::<LittleEndian>()?,
                    reader.read_u32::<LittleEndian>()?,
                ),
                Endianness::Big => (
                    reader.read_u32::<BigEndian>()?,
                    reader.read_u32::<BigEndian>()?,
                    reader.read_u32::<BigEndian>()?,
                    reader.read_u32::<BigEndian>()?,
                ),
            };

            let format_val = v3 >> 28;
            let first_block = v3 & 0x0fff_ffff;
            let format = CompressionFormat::from(format_val);
            let is_compressed = (v3 != 0xffff_ffff) && (format != CompressionFormat::Uncompressed);

            entries.insert(
                hash,
                FileEntry {
                    hash,
                    offset: raw_offset as u64,
                    v1,
                    uncompressed_size,
                    v3,
                    format,
                    first_block,
                    is_compressed,
                },
            );
        }

        // 3. Block tables:
        // _mediumFileBlockTable: [u16; num_medium_file_blocks]
        let mut medium_file_blocks = Vec::with_capacity(num_medium_file_blocks as usize);
        for _ in 0..num_medium_file_blocks {
            let val = match endian {
                Endianness::Little => reader.read_u16::<LittleEndian>()?,
                Endianness::Big => reader.read_u16::<BigEndian>()?,
            };
            medium_file_blocks.push(val);
        }

        // _smallFileBlockTable: [u8; num_small_file_blocks]
        let mut small_file_blocks = vec![0u8; num_small_file_blocks as usize];
        if num_small_file_blocks > 0 {
            reader.read_exact(&mut small_file_blocks)?;
        }

        // 4. Name table: offsets [u32; num_files] followed by null-terminated strings
        // ponytail: parse full name table if present; fallback to hash-only when stripped
        let mut file_names = HashMap::new();
        let mut name_to_hash = HashMap::new();
        if header.name_table_offset > 0 && header.name_table_size >= (num_files as u64) * 4 {
            reader.seek(SeekFrom::Start(header.name_table_offset))?;
            let mut nt_buf = vec![0u8; header.name_table_size as usize];
            reader.read_exact(&mut nt_buf)?;

            let mut cur = Cursor::new(&nt_buf);
            let mut offsets = Vec::with_capacity(num_files as usize);
            for _ in 0..num_files {
                let off = match endian {
                    Endianness::Little => cur.read_u32::<LittleEndian>()?,
                    Endianness::Big => cur.read_u32::<BigEndian>()?,
                };
                offsets.push(off as usize);
            }

            for (i, &off) in offsets.iter().enumerate() {
                if off < nt_buf.len() && i < file_ids.len() {
                    let hash = file_ids[i];
                    let rest = &nt_buf[off..];
                    let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
                    if let Ok(name_str) = std::str::from_utf8(&rest[..len]) {
                        let normalized = name_str.replace('\\', "/");
                        file_names.insert(hash, normalized.clone());
                        name_to_hash.insert(normalized.to_ascii_lowercase(), hash);
                        name_to_hash.insert(normalized, hash);
                    }
                }
            }
        }

        Ok(Self {
            reader: Mutex::new(reader),
            header,
            entries,
            medium_file_blocks,
            small_file_blocks,
            file_names,
            name_to_hash,
            source_name: String::new(),
        })
    }

    pub fn read_file_by_hash(&self, hash: u32) -> Result<Vec<u8>, ArchiveError> {
        let entry = self
            .entries
            .get(&hash)
            .ok_or(ArchiveError::FileNotFound(hash))?;

        if entry.uncompressed_size == 0 {
            return Ok(Vec::new());
        }

        if !entry.is_compressed {
            let mut r = self.reader.lock().unwrap();
            r.seek(SeekFrom::Start(entry.offset))?;
            let mut buf = vec![0u8; entry.uncompressed_size as usize];
            r.read_exact(&mut buf)?;
            return Ok(buf);
        }

        let num_blocks = (entry.uncompressed_size as usize).div_ceil(CHUNK_SIZE);
        let use_medium = entry.uncompressed_size > MEDIUM_BLOCK_THRESHOLD;
        let start_idx = entry.first_block as usize;

        let mut decompressed = Vec::with_capacity(entry.uncompressed_size as usize);
        let mut remaining = entry.uncompressed_size as usize;

        let mut r = self.reader.lock().unwrap();

        for i in 0..num_blocks {
            let chunk_expected = remaining.min(CHUNK_SIZE);
            let idx = start_idx + i;

            let (is_block_compressed, sec_start, sec_end) = if use_medium {
                if idx + 1 >= self.medium_file_blocks.len() {
                    return Err(ArchiveError::DecompressionFailed(format!(
                        "Medium block index {} out of bounds (len {})",
                        idx,
                        self.medium_file_blocks.len()
                    )));
                }
                let cur = self.medium_file_blocks[idx];
                let next = self.medium_file_blocks[idx + 1];
                (
                    (cur & 0x8000) != 0,
                    (cur & 0x7fff) as u64,
                    (next & 0x7fff) as u64,
                )
            } else {
                if idx + 1 >= self.small_file_blocks.len() {
                    return Err(ArchiveError::DecompressionFailed(format!(
                        "Small block index {} out of bounds (len {})",
                        idx,
                        self.small_file_blocks.len()
                    )));
                }
                let cur = self.small_file_blocks[idx];
                let next = self.small_file_blocks[idx + 1];
                ((cur & 0x80) != 0, (cur & 0x7f) as u64, (next & 0x7f) as u64)
            };

            let chunk_offset = entry.offset + sec_start * self.header.sector_size as u64;
            let compressed_sec_bytes =
                (sec_end.saturating_sub(sec_start) * self.header.sector_size as u64) as usize;

            r.seek(SeekFrom::Start(chunk_offset))?;

            if !is_block_compressed {
                let mut raw = vec![0u8; chunk_expected];
                r.read_exact(&mut raw)?;
                decompressed.extend_from_slice(&raw);
            } else {
                match entry.format {
                    CompressionFormat::Lzma => {
                        let c_size = match self.header.endian {
                            Endianness::Little => r.read_u16::<LittleEndian>()?,
                            Endianness::Big => r.read_u16::<BigEndian>()?,
                        };
                        let mut props = [0u8; 5];
                        r.read_exact(&mut props)?;

                        let mut compressed_stream = vec![0u8; c_size as usize];
                        r.read_exact(&mut compressed_stream)?;

                        let mut lzma_header = Vec::with_capacity(13 + compressed_stream.len());
                        lzma_header.extend_from_slice(&props);
                        lzma_header.extend_from_slice(&(chunk_expected as u64).to_le_bytes());
                        lzma_header.extend_from_slice(&compressed_stream);

                        let mut output = Vec::with_capacity(chunk_expected);
                        lzma_rs::lzma_decompress(&mut Cursor::new(&lzma_header), &mut output)
                            .map_err(|e| {
                                ArchiveError::DecompressionFailed(format!("LZMA error: {e:?}"))
                            })?;

                        if output.len() != chunk_expected {
                            return Err(ArchiveError::DecompressionFailed(format!(
                                "LZMA size mismatch: expected {chunk_expected}, got {}",
                                output.len()
                            )));
                        }
                        decompressed.extend_from_slice(&output);
                    }
                    CompressionFormat::Deflate => {
                        let mut compressed_data = vec![0u8; compressed_sec_bytes];
                        r.read_exact(&mut compressed_data)?;

                        let decompressed_chunk =
                            miniz_oxide::inflate::decompress_to_vec_zlib(&compressed_data)
                                .map_err(|e| {
                                    ArchiveError::DecompressionFailed(format!(
                                        "Deflate error: {e:?}"
                                    ))
                                })?;

                        let chunk_slice = if decompressed_chunk.len() > chunk_expected {
                            &decompressed_chunk[..chunk_expected]
                        } else {
                            &decompressed_chunk
                        };

                        if chunk_slice.len() != chunk_expected {
                            return Err(ArchiveError::DecompressionFailed(format!(
                                "Deflate size mismatch: expected {chunk_expected}, got {}",
                                chunk_slice.len()
                            )));
                        }
                        decompressed.extend_from_slice(chunk_slice);
                    }
                    _ => {
                        return Err(ArchiveError::DecompressionFailed(format!(
                            "Unsupported compression format: {:?}",
                            entry.format
                        )));
                    }
                }
            }

            remaining -= chunk_expected;
        }

        Ok(decompressed)
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        let normalized = path.replace('\\', "/");
        if let Some(&hash) = self.name_to_hash.get(&normalized) {
            return self.read_file_by_hash(hash);
        }
        let lower = normalized.to_ascii_lowercase();
        if let Some(&hash) = self.name_to_hash.get(&lower) {
            return self.read_file_by_hash(hash);
        }
        let fnv = hash_fnv1a(&normalized);
        if let Ok(data) = self.read_file_by_hash(fnv) {
            return Ok(data);
        }
        let fnv_lower = hash_fnv1a(&lower);
        if let Ok(data) = self.read_file_by_hash(fnv_lower) {
            return Ok(data);
        }
        let crc = hash_crc32(&normalized);
        if let Ok(data) = self.read_file_by_hash(crc) {
            return Ok(data);
        }
        let crc_lower = hash_crc32(&lower);
        self.read_file_by_hash(crc_lower)
    }

    pub fn contains_path(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        let lower = normalized.to_ascii_lowercase();
        let fnv = hash_fnv1a(&normalized);
        let fnv_lower = hash_fnv1a(&lower);
        let crc = hash_crc32(&normalized);
        let crc_lower = hash_crc32(&lower);
        self.name_to_hash.contains_key(&normalized)
            || self.name_to_hash.contains_key(&lower)
            || self.entries.contains_key(&fnv)
            || self.entries.contains_key(&fnv_lower)
            || self.entries.contains_key(&crc)
            || self.entries.contains_key(&crc_lower)
    }

    pub fn list_files(&self) -> Vec<&str> {
        self.file_names.values().map(|s| s.as_str()).collect()
    }

    pub fn file_name_by_hash(&self, hash: u32) -> Option<&str> {
        self.file_names.get(&hash).map(|s| s.as_str())
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
        let p = path.as_ref();
        let file = File::open(p)?;
        let mut archive = ArchiveReader::new(file)?;
        if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
            archive.source_name = fname.to_string();
        }
        self.archives.push(archive);
        Ok(())
    }

    pub fn mount_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, ArchiveError> {
        let mut count = 0;
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Ok(0);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                count += self.mount_dir(&path)?;
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if (ext.eq_ignore_ascii_case("arc") || ext.eq_ignore_ascii_case("bld"))
                    && self.mount(&path).is_ok()
                {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    pub fn count(&self) -> usize {
        self.archives.len()
    }

    pub fn read_file_by_hash(&self, hash: u32) -> Result<Vec<u8>, ArchiveError> {
        for archive in &self.archives {
            if archive.entries.contains_key(&hash) {
                return archive.read_file_by_hash(hash);
            }
        }
        Err(ArchiveError::FileNotFound(hash))
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        let normalized = path.replace('\\', "/");
        let lower = normalized.to_ascii_lowercase();

        // 1. Direct exact match on archive name (e.g. "Level_Hub_2014_01.bld" -> inner "level.bld")
        for archive in &self.archives {
            let arch_name = archive.source_name.to_ascii_lowercase();
            if arch_name == lower && archive.contains_path("level.bld") {
                return archive.read_file("level.bld");
            }
        }

        // Direct stem match fallback on archive name
        for archive in &self.archives {
            let arch_name = archive.source_name.to_ascii_lowercase();
            if !arch_name.is_empty() {
                let stem = arch_name
                    .strip_suffix(".bld")
                    .or_else(|| arch_name.strip_suffix(".arc"))
                    .unwrap_or(&arch_name);
                let cand_stem = lower
                    .strip_suffix(".bld")
                    .or_else(|| lower.strip_suffix(".arc"))
                    .or_else(|| lower.strip_suffix(".igb"))
                    .unwrap_or(&lower);
                let target_stem = cand_stem.rsplit('/').next().unwrap_or(cand_stem);

                if target_stem == stem && archive.contains_path("level.bld") {
                    return archive.read_file("level.bld");
                }
            }
        }

        // 2. Archive-scoped path: "Archive_Name/inner_file.ext"
        if let Some((arch_part, inner_part)) = lower.split_once('/') {
            for archive in &self.archives {
                let arch_lower = archive.source_name.to_ascii_lowercase();
                let arch_stem = arch_lower
                    .strip_suffix(".bld")
                    .or_else(|| arch_lower.strip_suffix(".arc"))
                    .unwrap_or(&arch_lower);
                if (arch_lower == arch_part || arch_stem == arch_part)
                    && archive.contains_path(inner_part)
                {
                    return archive.read_file(inner_part);
                }
            }
        }

        // 3. Regular file lookup by name/hash
        for archive in &self.archives {
            if let Some(&hash) = archive.name_to_hash.get(&normalized) {
                return archive.read_file_by_hash(hash);
            }
            if let Some(&hash) = archive.name_to_hash.get(&lower) {
                return archive.read_file_by_hash(hash);
            }
        }
        let fnv = hash_fnv1a(&normalized);
        if let Ok(data) = self.read_file_by_hash(fnv) {
            return Ok(data);
        }
        let fnv_lower = hash_fnv1a(&lower);
        if let Ok(data) = self.read_file_by_hash(fnv_lower) {
            return Ok(data);
        }
        let crc = hash_crc32(&normalized);
        if let Ok(data) = self.read_file_by_hash(crc) {
            return Ok(data);
        }
        let crc_lower = hash_crc32(&lower);
        self.read_file_by_hash(crc_lower)
    }

    pub fn contains_path(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        let lower = normalized.to_ascii_lowercase();

        for archive in &self.archives {
            let arch_name = archive.source_name.to_ascii_lowercase();
            if !arch_name.is_empty() {
                let stem = arch_name
                    .strip_suffix(".bld")
                    .or_else(|| arch_name.strip_suffix(".arc"))
                    .unwrap_or(&arch_name);
                let cand_stem = lower
                    .strip_suffix(".bld")
                    .or_else(|| lower.strip_suffix(".arc"))
                    .or_else(|| lower.strip_suffix(".igb"))
                    .unwrap_or(&lower);
                let target_stem = cand_stem.rsplit('/').next().unwrap_or(cand_stem);

                if (arch_name == lower || target_stem == stem) && archive.contains_path("level.bld")
                {
                    return true;
                }
            }
        }

        if let Some((arch_part, inner_part)) = lower.split_once('/') {
            for archive in &self.archives {
                let arch_lower = archive.source_name.to_ascii_lowercase();
                let arch_stem = arch_lower
                    .strip_suffix(".bld")
                    .or_else(|| arch_lower.strip_suffix(".arc"))
                    .unwrap_or(&arch_lower);
                if (arch_lower == arch_part || arch_stem == arch_part)
                    && archive.contains_path(inner_part)
                {
                    return true;
                }
            }
        }

        let fnv = hash_fnv1a(&normalized);
        let fnv_lower = hash_fnv1a(&lower);
        let crc = hash_crc32(&normalized);
        let crc_lower = hash_crc32(&lower);
        for archive in &self.archives {
            if archive.name_to_hash.contains_key(&normalized)
                || archive.name_to_hash.contains_key(&lower)
                || archive.entries.contains_key(&fnv)
                || archive.entries.contains_key(&fnv_lower)
                || archive.entries.contains_key(&crc)
                || archive.entries.contains_key(&crc_lower)
            {
                return true;
            }
        }
        false
    }

    pub fn list_files(&self) -> Vec<&str> {
        let mut files = Vec::new();
        for archive in &self.archives {
            files.extend(archive.list_files());
        }
        files
    }

    fn find_matching_archive(&self, pattern: &str) -> Option<&ArchiveReader<File>> {
        let clean = pattern.replace('\\', "/");
        let name = clean.rsplit('/').next().unwrap_or(&clean);
        let lower = name.to_ascii_lowercase();

        if let Some(a) = self
            .archives
            .iter()
            .find(|a| a.source_name.to_ascii_lowercase() == lower)
        {
            return Some(a);
        }

        let stem = lower
            .strip_suffix(".bld")
            .or_else(|| lower.strip_suffix(".arc"))
            .unwrap_or(&lower);

        if lower.ends_with(".arc") {
            if let Some(a) = self.archives.iter().find(|a| {
                let a_lower = a.source_name.to_ascii_lowercase();
                a_lower.ends_with(".arc")
                    && a_lower.strip_suffix(".arc").unwrap_or(&a_lower) == stem
            }) {
                return Some(a);
            }
        } else if lower.ends_with(".bld") {
            if let Some(a) = self.archives.iter().find(|a| {
                let a_lower = a.source_name.to_ascii_lowercase();
                a_lower.ends_with(".bld")
                    && a_lower.strip_suffix(".bld").unwrap_or(&a_lower) == stem
            }) {
                return Some(a);
            }
        }

        self.archives.iter().find(|a| {
            let a_lower = a.source_name.to_ascii_lowercase();
            let a_stem = a_lower
                .strip_suffix(".bld")
                .or_else(|| a_lower.strip_suffix(".arc"))
                .unwrap_or(&a_lower);
            a_stem == stem || a_lower.contains(stem)
        })
    }

    pub fn has_archive(&self, pattern: &str) -> bool {
        self.find_matching_archive(pattern).is_some()
    }

    pub fn list_archive_files(&self, archive_pattern: &str) -> Vec<String> {
        self.find_matching_archive(archive_pattern)
            .map(|a| a.list_files().into_iter().map(String::from).collect())
            .unwrap_or_default()
    }

    pub fn find_archive_texture_files(&self, archive_pattern: &str) -> Vec<String> {
        let files = self.list_archive_files(archive_pattern);
        files
            .into_iter()
            .filter(|f| {
                let lower = f.to_ascii_lowercase();
                lower.ends_with(".tex.igz")
                    || lower.ends_with(".png.igb.tex.igz")
                    || lower.ends_with(".igb.tex")
                    || lower.ends_with(".png")
                    || lower.ends_with(".dds")
                    || lower.ends_with(".cmpr")
            })
            .collect()
    }

    pub fn read_from_archive(
        &self,
        archive_pattern: &str,
        file_name: &str,
    ) -> Result<Vec<u8>, ArchiveError> {
        if let Some(archive) = self.find_matching_archive(archive_pattern) {
            return archive.read_file(file_name);
        }
        Err(ArchiveError::FileNotFound(0))
    }

    pub fn scan_embedded_igb(data: &[u8]) -> Vec<&[u8]> {
        let mut results = Vec::new();
        if data.len() < 16 {
            return results;
        }

        if data.len() >= 4 && &data[0..4] == b"IGZ\x01" {
            results.push(data);
            return results;
        }

        let mut offset = 0;
        while offset + 4 <= data.len() {
            if &data[offset..offset + 4] == b"IGBF" {
                let sub = &data[offset..];
                if Cursor::new(sub).read_exact(&mut [0u8; 16]).is_ok() {
                    let mut cur = Cursor::new(&sub[4..16]);
                    let ver_le = cur.read_u32::<LittleEndian>().unwrap_or(u32::MAX);
                    let (version, obj_count) = if ver_le < 0x1000 {
                        (ver_le, cur.read_u32::<LittleEndian>().unwrap_or(0))
                    } else {
                        let mut cur_be = Cursor::new(&sub[4..16]);
                        (
                            cur_be.read_u32::<BigEndian>().unwrap_or(u32::MAX),
                            cur_be.read_u32::<BigEndian>().unwrap_or(0),
                        )
                    };
                    if (1..=20).contains(&version) || obj_count > 0 {
                        results.push(sub);
                    }
                }
                offset += 4;
            } else if &data[offset..offset + 4] == b"IGZ\x01" {
                results.push(&data[offset..]);
                offset += 4;
            } else {
                offset += 1;
            }
        }
        results
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
        data.extend_from_slice(&100u32.to_le_bytes()); // toc_size
        data.extend_from_slice(&1u32.to_le_bytes()); // num_files = 1
        data.extend_from_slice(&2048u32.to_le_bytes()); // sector_size
        data.extend_from_slice(&1431655765u32.to_le_bytes()); // hash_search_divider
        data.extend_from_slice(&2u32.to_le_bytes()); // hash_search_slop
        data.extend_from_slice(&0u32.to_le_bytes()); // num_large_file_blocks
        data.extend_from_slice(&0u32.to_le_bytes()); // num_medium_file_blocks
        data.extend_from_slice(&0u32.to_le_bytes()); // num_small_file_blocks
        data.extend_from_slice(&0u64.to_le_bytes()); // name_table_offset
        data.extend_from_slice(&0u64.to_le_bytes()); // name_table_size

        // 1 file hash
        data.extend_from_slice(&0x12345678u32.to_le_bytes());
        // 1 file info: offset: 2048, v1: 0, uncomp_size: 4, v3: 0xffffffff (uncompressed)
        data.extend_from_slice(&2048u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(&0xffff_ffffu32.to_le_bytes());

        // Pad up to offset 2048
        data.resize(2048, 0);
        data.extend_from_slice(b"test");

        let reader = ArchiveReader::new(Cursor::new(data)).expect("Valid archive");
        assert_eq!(reader.header.endian, Endianness::Little);
        assert_eq!(reader.header.version, 11);
        assert_eq!(reader.header.num_files, 1);
        assert_eq!(reader.header.sector_size, 2048);

        let content = reader.read_file_by_hash(0x12345678).expect("Read file");
        assert_eq!(content, b"test");
    }

    #[test]
    fn test_synthetic_lzma_roundtrip() {
        let original_data = b"Hello from Skylands compressed block test! Repeat Repeat Repeat";
        let mut lzma_header_and_stream = Vec::new();
        lzma_rs::lzma_compress(&mut Cursor::new(original_data), &mut lzma_header_and_stream)
            .expect("compress");

        let props = &lzma_header_and_stream[0..5];
        let stream = &lzma_header_and_stream[13..];
        let c_size = stream.len() as u16;

        let mut data = Vec::new();
        data.extend_from_slice(b"IGA\x1a");
        data.extend_from_slice(&11u32.to_le_bytes());
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // num_files = 1
        data.extend_from_slice(&2048u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes()); // 2 small blocks
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());

        let file_hash = hash_path("data/test.bin");
        data.extend_from_slice(&file_hash.to_le_bytes());

        // File info: offset: 2048, v1: 0, uncomp: len, v3: (2 << 28) | 0
        let v3 = 2u32 << 28;
        data.extend_from_slice(&2048u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(original_data.len() as u32).to_le_bytes());
        data.extend_from_slice(&v3.to_le_bytes());

        // Small blocks: [0x80, 1] (compressed block at sector 0 to sector 1)
        data.push(0x80);
        data.push(1);

        data.resize(2048, 0);
        data.extend_from_slice(&c_size.to_le_bytes());
        data.extend_from_slice(props);
        data.extend_from_slice(stream);

        let reader = ArchiveReader::new(Cursor::new(data)).expect("Valid archive");
        let content = reader.read_file("data/test.bin").expect("Read file");
        assert_eq!(content, original_data);
    }

    #[test]
    fn test_synthetic_deflate_roundtrip() {
        let original_data = b"Deflate test payload! Skylands Untrapped VFS Decompression Test.";
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(original_data, 6);

        let mut data = Vec::new();
        data.extend_from_slice(b"IGA\x1a");
        data.extend_from_slice(&11u32.to_le_bytes());
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&2048u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());

        let file_hash = hash_path("archive/deflate.dat");
        data.extend_from_slice(&file_hash.to_le_bytes());

        let v3 = 1u32 << 28; // Deflate = 1
        data.extend_from_slice(&2048u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(original_data.len() as u32).to_le_bytes());
        data.extend_from_slice(&v3.to_le_bytes());

        data.push(0x80);
        data.push(1);

        data.resize(2048, 0);
        data.extend_from_slice(&compressed);
        data.resize(2048 + 2048, 0);

        let reader = ArchiveReader::new(Cursor::new(data)).expect("Valid archive");
        let content = reader.read_file("archive/deflate.dat").expect("Read file");
        assert_eq!(content, original_data);
    }

    #[test]
    fn test_synthetic_name_table_roundtrip() {
        let mut data = Vec::new();
        data.extend_from_slice(b"IGA\x1a");
        data.extend_from_slice(&11u32.to_le_bytes()); // version
        data.extend_from_slice(&100u32.to_le_bytes()); // toc_size
        data.extend_from_slice(&2u32.to_le_bytes()); // num_files = 2
        data.extend_from_slice(&2048u32.to_le_bytes()); // sector_size
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // 0 small blocks

        let name0 = b"models/hero.igb\0";
        let name1 = b"textures/hero.png\0";
        let nt_off0 = 8u32;
        let nt_off1 = nt_off0 + name0.len() as u32;
        let nt_size = (nt_off1 + name1.len() as u32) as u64;
        let nt_offset = 4096u64;

        data.extend_from_slice(&nt_offset.to_le_bytes());
        data.extend_from_slice(&nt_size.to_le_bytes());

        // File IDs (sorted)
        data.extend_from_slice(&0x1111_1111u32.to_le_bytes());
        data.extend_from_slice(&0x2222_2222u32.to_le_bytes());

        // File 0 info: offset 2048, uncomp 13, uncompressed (v3 = 0xffffffff)
        data.extend_from_slice(&2048u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&13u32.to_le_bytes());
        data.extend_from_slice(&0xffff_ffffu32.to_le_bytes());

        // File 1 info: offset 2048 + 16, uncomp 15, uncompressed
        data.extend_from_slice(&2064u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&15u32.to_le_bytes());
        data.extend_from_slice(&0xffff_ffffu32.to_le_bytes());

        // Payload at sector 1 (offset 2048)
        data.resize(2048, 0);
        data.extend_from_slice(b"HeroModelIGBF");
        data.resize(2064, 0);
        data.extend_from_slice(b"HeroTextureRGBA");

        // Name table at offset 4096
        data.resize(4096, 0);
        data.extend_from_slice(&nt_off0.to_le_bytes());
        data.extend_from_slice(&nt_off1.to_le_bytes());
        data.extend_from_slice(name0);
        data.extend_from_slice(name1);

        let reader = ArchiveReader::new(Cursor::new(data)).expect("Valid archive");
        assert_eq!(reader.list_files().len(), 2);
        assert!(reader.contains_path("models/hero.igb"));
        assert!(reader.contains_path("MODELS/HERO.IGB"));
        assert!(reader.contains_path("textures/hero.png"));
        assert_eq!(
            reader.file_name_by_hash(0x1111_1111),
            Some("models/hero.igb")
        );

        let hero = reader.read_file("models/hero.igb").expect("read hero");
        assert_eq!(hero, b"HeroModelIGBF");
        let tex = reader.read_file("textures/hero.png").expect("read tex");
        assert_eq!(tex, b"HeroTextureRGBA");
    }

    #[test]
    fn test_real_kaos_archive_name_resolution() {
        let bld_path = Path::new("assets/files/character/1010_Kaos.bld");
        if bld_path.exists() {
            let mut mount = ArchiveMount::new();
            mount.mount(bld_path).expect("mount Kaos.bld");
            assert!(mount.contains_path("level.bld"));
            assert!(mount.contains_path("ENGLISH.pak"));
            assert!(mount.contains_path("GERMAN.pak"));

            let files = mount.list_files();
            assert_eq!(files.len(), 3);
            assert!(files.contains(&"level.bld"));

            let english_pak = mount.read_file("ENGLISH.pak").expect("read ENGLISH.pak");
            assert_eq!(english_pak.len(), 148836);
            assert_eq!(&english_pak[0..4], b"IGZ\x01");
        }

        let arc_path = Path::new("assets/files/character/1010_Kaos.arc");
        if arc_path.exists() {
            let mut mount = ArchiveMount::new();
            mount.mount(arc_path).expect("mount Kaos.arc");
            let files = mount.list_files();
            assert_eq!(files.len(), 269);
            assert!(mount.contains_path(
                "c:/tfb/build/wii/levels/criminal_audio_sky4/kaos/vo_kaos_transform_004a.wav/0x9beb0991.wav.hz.wav.enc"
            ));
        }
    }

    #[test]
    fn test_real_global_bld_and_arc() {
        let bld_path = Path::new("assets/files/permanent/global.bld");
        if bld_path.exists() {
            let mut mount = ArchiveMount::new();
            mount.mount(bld_path).expect("mount global.bld");
            assert_eq!(mount.count(), 1);

            // Hash 0xe115e6dd is ENGLISH.pak (148836 bytes)
            let english_pak = mount
                .read_file_by_hash(0xe115e6dd)
                .expect("read ENGLISH.pak");
            assert_eq!(english_pak.len(), 148836);
            assert_eq!(&english_pak[0..4], b"IGZ\x01");

            // Hash 0xba91d879 is level.bld (9624560 bytes)
            let level_bld = mount.read_file_by_hash(0xba91d879).expect("read level.bld");
            assert_eq!(level_bld.len(), 9624560);
            assert_eq!(&level_bld[0..4], b"IGZ\x01");
        }

        let arc_path = Path::new("assets/files/permanent/global.arc");
        if arc_path.exists() {
            let mut mount = ArchiveMount::new();
            mount.mount(arc_path).expect("mount global.arc");
            assert_eq!(mount.count(), 1);

            // Hash 0x728cb8 is first uncompressed file (137312 bytes)
            let file0 = mount
                .read_file_by_hash(0x728cb8)
                .expect("read file 0x728cb8");
            assert_eq!(file0.len(), 137312);
            assert_eq!(&file0[0..4], b"FSB5");
        }
    }

    #[test]
    fn test_all_categories_sample_bld_loading() {
        let samples = [
            "assets/files/character/016_Spyro.bld",
            "assets/files/character/1001_ChompyMage.bld",
            "assets/files/item/Item_HelpingHand.bld",
            "assets/files/outfit/Hat_114.bld",
            "assets/files/misc/StoreItem_Hat_021.bld",
            "assets/files/level/Arena_Mesh_300.bld",
        ];

        for path_str in samples {
            let p1 = Path::new(path_str);
            let p2 = Path::new("../../").join(path_str);
            let path = if p1.exists() {
                p1
            } else if p2.exists() {
                &p2
            } else {
                continue;
            };

            let mut mount = ArchiveMount::new();
            assert!(mount.mount(path).is_ok(), "Mount failed for {}", path_str);
            let level_bld = mount.read_file("level.bld");
            assert!(level_bld.is_ok(), "read level.bld failed for {}", path_str);
            let bytes = level_bld.unwrap();
            assert!(!bytes.is_empty(), "level.bld is empty for {}", path_str);
        }
    }

    #[test]
    fn test_inspect_extracted_targets() {
        let targets = [
            "assets/files/character/462_Water_Crocky.bld",
            "assets/files/character/462_Water_Crocky.arc",
            "assets/files/character/1010_Kaos.bld",
            "assets/files/character/1010_Kaos.arc",
            "assets/files/level/Level_Hub_2014_01.bld",
        ];

        for path_str in targets {
            let p1 = Path::new(path_str);
            let p2 = Path::new("../../").join(path_str);
            let path = if p1.exists() {
                p1
            } else if p2.exists() {
                &p2
            } else {
                continue;
            };

            let mut mount = ArchiveMount::new();
            mount.mount(path).expect("mount target");
            println!("INSPECT: {} (count={})", path_str, mount.count());
            let files = mount.list_files();
            println!("  Files count: {}", files.len());
            for f in &files[..files.len().min(5)] {
                let data = mount.read_file(f).ok();
                let head = data
                    .as_ref()
                    .map(|d| if d.len() >= 4 { &d[..4] } else { &d[..] })
                    .unwrap_or(&[]);
                let igb_chunks = data
                    .as_ref()
                    .map(|d| ArchiveMount::scan_embedded_igb(d).len())
                    .unwrap_or(0);
                println!(
                    "    file: {}, size: {}, magic: {:?}, igb_chunks: {}",
                    f,
                    data.as_ref().map(|d| d.len()).unwrap_or(0),
                    String::from_utf8_lossy(head),
                    igb_chunks
                );
            }
        }
    }
}
