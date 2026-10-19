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
    #[error("Corrupt vertex data: {0}")]
    CorruptVertexData(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgbEndian {
    Little,
    Big,
}

#[derive(Debug, Clone)]
pub struct IgbHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub object_count: u32,
    pub string_table_offset: u32,
    pub endian: IgbEndian,
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

        let version = cur.read_u32::<LittleEndian>()?;
        let (version, object_count, string_table_offset, endian) = if version < 0x1000 {
            (
                version,
                cur.read_u32::<LittleEndian>()?,
                cur.read_u32::<LittleEndian>()?,
                IgbEndian::Little,
            )
        } else {
            let mut cur_be = Cursor::new(&rest);
            (
                cur_be.read_u32::<BigEndian>()?,
                cur_be.read_u32::<BigEndian>()?,
                cur_be.read_u32::<BigEndian>()?,
                IgbEndian::Big,
            )
        };

        Ok(Self {
            magic,
            version,
            object_count,
            string_table_offset,
            endian,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IgbVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl Default for IgbVertex {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IgbMesh {
    pub name: String,
    pub vertices: Vec<IgbVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct IgbModel {
    pub header: IgbHeader,
    pub meshes: Vec<IgbMesh>,
}

impl IgbModel {
    pub fn parse<R: Read>(mut reader: R) -> Result<Self, IgbError> {
        let header = IgbHeader::parse(&mut reader)?;
        let mut remaining = Vec::new();
        reader.read_to_end(&mut remaining)?;

        // ponytail: parse geometry chunks when present; fallback to empty mesh list on pure metadata
        let mut meshes = Vec::new();
        if !remaining.is_empty() {
            if let Ok(mesh) = Self::parse_mesh_stream(&remaining, header.endian) {
                meshes.push(mesh);
            }
        }

        Ok(Self { header, meshes })
    }

    pub fn parse_mesh_stream(data: &[u8], endian: IgbEndian) -> Result<IgbMesh, IgbError> {
        if data.len() < 8 {
            return Err(IgbError::CorruptVertexData("Stream too short".into()));
        }

        let mut cur = Cursor::new(data);
        let (vert_count, index_count) = match endian {
            IgbEndian::Little => (
                cur.read_u32::<LittleEndian>()? as usize,
                cur.read_u32::<LittleEndian>()? as usize,
            ),
            IgbEndian::Big => (
                cur.read_u32::<BigEndian>()? as usize,
                cur.read_u32::<BigEndian>()? as usize,
            ),
        };

        // Each vertex: 3 * f32 pos + 3 * f32 norm + 2 * f32 uv + 4 * f32 color = 12 * 4 = 48 bytes
        const VERTEX_SIZE: usize = 48;
        let expected_bytes = 8 + vert_count * VERTEX_SIZE + index_count * 4;
        if data.len() < expected_bytes {
            return Err(IgbError::CorruptVertexData(format!(
                "Expected {} bytes for {} verts and {} indices, got {}",
                expected_bytes,
                vert_count,
                index_count,
                data.len()
            )));
        }

        let mut vertices = Vec::with_capacity(vert_count);
        for _ in 0..vert_count {
            let v = match endian {
                IgbEndian::Little => IgbVertex {
                    position: [
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                    ],
                    normal: [
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                    ],
                    uv: [
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                    ],
                    color: [
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                        cur.read_f32::<LittleEndian>()?,
                    ],
                },
                IgbEndian::Big => IgbVertex {
                    position: [
                        cur.read_f32::<BigEndian>()?,
                        cur.read_f32::<BigEndian>()?,
                        cur.read_f32::<BigEndian>()?,
                    ],
                    normal: [
                        cur.read_f32::<BigEndian>()?,
                        cur.read_f32::<BigEndian>()?,
                        cur.read_f32::<BigEndian>()?,
                    ],
                    uv: [cur.read_f32::<BigEndian>()?, cur.read_f32::<BigEndian>()?],
                    color: [
                        cur.read_f32::<BigEndian>()?,
                        cur.read_f32::<BigEndian>()?,
                        cur.read_f32::<BigEndian>()?,
                        cur.read_f32::<BigEndian>()?,
                    ],
                },
            };
            vertices.push(v);
        }

        let mut indices = Vec::with_capacity(index_count);
        for _ in 0..index_count {
            let idx = match endian {
                IgbEndian::Little => cur.read_u32::<LittleEndian>()?,
                IgbEndian::Big => cur.read_u32::<BigEndian>()?,
            };
            indices.push(idx);
        }

        Ok(IgbMesh {
            name: "mesh_0".to_string(),
            vertices,
            indices,
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
        assert_eq!(hdr.endian, IgbEndian::Little);
    }

    #[test]
    fn test_igb_mesh_stream() {
        let mut data = Vec::new();
        // 1 vertex, 3 indices
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        // position: 1.0, 2.0, 3.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());
        // normal: 0.0, 1.0, 0.0
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // uv: 0.5, 0.5
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // color: 1.0, 1.0, 1.0, 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // indices: 0, 0, 0
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let mesh = IgbModel::parse_mesh_stream(&data, IgbEndian::Little).expect("Parse mesh");
        assert_eq!(mesh.vertices.len(), 1);
        assert_eq!(mesh.indices.len(), 3);
        assert_eq!(mesh.vertices[0].position, [1.0, 2.0, 3.0]);
    }
}
