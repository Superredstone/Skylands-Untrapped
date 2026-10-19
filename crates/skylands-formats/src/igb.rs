use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
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
    #[error("Corrupt index data: {0}")]
    CorruptIndexData(String),
    #[error("Corrupt object directory: {0}")]
    CorruptObjectDirectory(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgbObjectType {
    IgNode,
    IgGeometry,
    IgAttrList,
    IgVertexBuffer,
    IgIndexBuffer,
    Unknown(u32),
}

impl From<u32> for IgbObjectType {
    fn from(val: u32) -> Self {
        match val {
            1 => IgbObjectType::IgNode,
            2 => IgbObjectType::IgGeometry,
            3 => IgbObjectType::IgAttrList,
            4 => IgbObjectType::IgVertexBuffer,
            5 => IgbObjectType::IgIndexBuffer,
            other => IgbObjectType::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IgbDirectoryEntry {
    pub type_id: IgbObjectType,
    pub name_offset: u32,
    pub data_offset: u32,
    pub data_size: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrSemantic {
    Position,
    Normal,
    TexCoord,
    Color,
    Unknown(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrFormat {
    Float32x3,
    Float32x2,
    Float32x4,
    WiiGxI16x3Fixed,  // 3 x i16 fixed point, scaled by 1.0 / (1 << frac_bits)
    PackedI8x3Normal, // 3 x i8 normal, scaled by 1.0 / 127.0
    PackedI8x4Color,  // 4 x u8 or i8 color normalized
    U16x2UV,          // 2 x u16 UV, scaled by 1.0 / (1 << frac_bits) or 1.0 / 65535.0
    Unknown(u32),
}

#[derive(Debug, Clone)]
pub struct IgbAttrDesc {
    pub semantic: AttrSemantic,
    pub format: AttrFormat,
    pub offset: u32,
    pub scale: f32,
}

#[derive(Debug, Clone)]
pub struct IgbAttrList {
    pub stride: u32,
    pub attributes: Vec<IgbAttrDesc>,
}

#[derive(Debug, Clone)]
pub struct IgbVertexBuffer {
    pub count: usize,
    pub stride: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexTopology {
    Triangles,
    TriangleStrip,
}

#[derive(Debug, Clone)]
pub struct IgbIndexBuffer {
    pub topology: IndexTopology,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct IgbGeometryNode {
    pub name: String,
    pub vertex_buffer_index: Option<usize>,
    pub index_buffer_index: Option<usize>,
    pub attr_list_index: Option<usize>,
    pub mesh: Option<IgbMesh>,
}

#[derive(Debug, Clone)]
pub struct IgbSceneNode {
    pub name: String,
    pub type_id: IgbObjectType,
    pub children: Vec<usize>,
    pub geometry_index: Option<usize>,
    pub transform: [f32; 16],
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
    pub directory: Vec<IgbDirectoryEntry>,
    pub nodes: Vec<IgbSceneNode>,
    pub geometries: Vec<IgbGeometryNode>,
    pub attr_lists: Vec<IgbAttrList>,
    pub vertex_buffers: Vec<IgbVertexBuffer>,
    pub index_buffers: Vec<IgbIndexBuffer>,
    pub meshes: Vec<IgbMesh>,
}

impl IgbModel {
    pub fn parse<R: Read>(mut reader: R) -> Result<Self, IgbError> {
        let mut full_file = Vec::new();
        reader.read_to_end(&mut full_file)?;

        let header = IgbHeader::parse(Cursor::new(&full_file))?;

        if header.object_count == 0 {
            let mut meshes = Vec::new();
            if full_file.len() > 16 {
                if let Ok(mesh) = Self::parse_mesh_stream(&full_file[16..], header.endian) {
                    meshes.push(mesh);
                }
            }
            return Ok(Self {
                header,
                directory: Vec::new(),
                nodes: Vec::new(),
                geometries: Vec::new(),
                attr_lists: Vec::new(),
                vertex_buffers: Vec::new(),
                index_buffers: Vec::new(),
                meshes,
            });
        }

        // Try directory table parsing starting right after header (offset 16)
        let dir_offset = 16usize;
        let entry_size = 16usize;
        let dir_end = dir_offset + (header.object_count as usize) * entry_size;

        if full_file.len() < dir_end {
            let mut meshes = Vec::new();
            if full_file.len() > 16 {
                if let Ok(mesh) = Self::parse_mesh_stream(&full_file[16..], header.endian) {
                    meshes.push(mesh);
                }
            }
            return Ok(Self {
                header,
                directory: Vec::new(),
                nodes: Vec::new(),
                geometries: Vec::new(),
                attr_lists: Vec::new(),
                vertex_buffers: Vec::new(),
                index_buffers: Vec::new(),
                meshes,
            });
        }

        let mut directory = Vec::with_capacity(header.object_count as usize);
        let str_table_offset = header.string_table_offset as usize;

        for i in 0..(header.object_count as usize) {
            let entry_slice =
                &full_file[dir_offset + i * entry_size..dir_offset + (i + 1) * entry_size];
            let type_num = Self::read_u32(entry_slice, 0, header.endian);
            let name_offset = Self::read_u32(entry_slice, 4, header.endian);
            let data_offset = Self::read_u32(entry_slice, 8, header.endian);
            let data_size = Self::read_u32(entry_slice, 12, header.endian);

            let name = Self::resolve_string(&full_file, str_table_offset, name_offset);

            directory.push(IgbDirectoryEntry {
                type_id: IgbObjectType::from(type_num),
                name_offset,
                data_offset,
                data_size,
                name,
            });
        }

        let mut nodes = Vec::new();
        let mut geometries = Vec::new();
        let mut attr_lists = Vec::new();
        let mut vertex_buffers = Vec::new();
        let mut index_buffers = Vec::new();

        // Parse parsed components by object directory type
        for entry in &directory {
            let start = entry.data_offset as usize;
            let end = (entry.data_offset + entry.data_size) as usize;
            if start > full_file.len() || end > full_file.len() || start >= end {
                continue;
            }
            let data = &full_file[start..end];

            match entry.type_id {
                IgbObjectType::IgAttrList => {
                    if let Ok(attr) = Self::parse_attr_list(data, header.endian) {
                        attr_lists.push(attr);
                    }
                }
                IgbObjectType::IgVertexBuffer => {
                    if let Ok(vb) = Self::parse_vertex_buffer(data, header.endian) {
                        vertex_buffers.push(vb);
                    }
                }
                IgbObjectType::IgIndexBuffer => {
                    if let Ok(ib) = Self::parse_index_buffer(data, header.endian) {
                        index_buffers.push(ib);
                    }
                }
                IgbObjectType::IgGeometry => {
                    if let Ok(geo) = Self::parse_geometry(data, &entry.name, header.endian) {
                        geometries.push(geo);
                    }
                }
                IgbObjectType::IgNode => {
                    if let Ok(node) = Self::parse_node(data, &entry.name, header.endian) {
                        nodes.push(node);
                    }
                }
                _ => {}
            }
        }

        let mut meshes = Vec::new();
        // Construct meshes from igGeometry nodes
        for (i, geo) in geometries.iter_mut().enumerate() {
            let vb = geo
                .vertex_buffer_index
                .and_then(|idx| vertex_buffers.get(idx))
                .or_else(|| vertex_buffers.get(i))
                .or_else(|| vertex_buffers.first());
            let ib = geo
                .index_buffer_index
                .and_then(|idx| index_buffers.get(idx))
                .or_else(|| index_buffers.get(i))
                .or_else(|| index_buffers.first());
            let attr = geo
                .attr_list_index
                .and_then(|idx| attr_lists.get(idx))
                .or_else(|| attr_lists.get(i))
                .or_else(|| attr_lists.first());

            if let (Some(vb), Some(ib), Some(attr)) = (vb, ib, attr) {
                let vertices = Self::decode_vertices(vb, attr, header.endian);
                let indices = ib.indices.clone();
                let mesh = IgbMesh {
                    name: geo.name.clone(),
                    vertices,
                    indices,
                };
                geo.mesh = Some(mesh.clone());
                meshes.push(mesh);
            }
        }

        // If no meshes parsed via structured directory, fallback to raw stream parser
        if meshes.is_empty() && full_file.len() > dir_end {
            if let Ok(mesh) = Self::parse_mesh_stream(&full_file[dir_end..], header.endian) {
                meshes.push(mesh);
            } else if let Ok(mesh) = Self::parse_mesh_stream(&full_file[16..], header.endian) {
                meshes.push(mesh);
            }
        }

        Ok(Self {
            header,
            directory,
            nodes,
            geometries,
            attr_lists,
            vertex_buffers,
            index_buffers,
            meshes,
        })
    }

    #[inline]
    fn read_u32(buf: &[u8], offset: usize, endian: IgbEndian) -> u32 {
        if offset + 4 > buf.len() {
            return 0;
        }
        match endian {
            IgbEndian::Little => LittleEndian::read_u32(&buf[offset..offset + 4]),
            IgbEndian::Big => BigEndian::read_u32(&buf[offset..offset + 4]),
        }
    }

    #[inline]
    fn read_u16(buf: &[u8], offset: usize, endian: IgbEndian) -> u16 {
        if offset + 2 > buf.len() {
            return 0;
        }
        match endian {
            IgbEndian::Little => LittleEndian::read_u16(&buf[offset..offset + 2]),
            IgbEndian::Big => BigEndian::read_u16(&buf[offset..offset + 2]),
        }
    }

    #[inline]
    fn read_i16(buf: &[u8], offset: usize, endian: IgbEndian) -> i16 {
        if offset + 2 > buf.len() {
            return 0;
        }
        match endian {
            IgbEndian::Little => LittleEndian::read_i16(&buf[offset..offset + 2]),
            IgbEndian::Big => BigEndian::read_i16(&buf[offset..offset + 2]),
        }
    }

    #[inline]
    fn read_f32(buf: &[u8], offset: usize, endian: IgbEndian) -> f32 {
        if offset + 4 > buf.len() {
            return 0.0;
        }
        match endian {
            IgbEndian::Little => LittleEndian::read_f32(&buf[offset..offset + 4]),
            IgbEndian::Big => BigEndian::read_f32(&buf[offset..offset + 4]),
        }
    }

    pub fn resolve_string(data: &[u8], str_table_offset: usize, offset: u32) -> String {
        let actual_pos = str_table_offset.saturating_add(offset as usize);
        if actual_pos >= data.len() {
            return String::new();
        }
        let rest = &data[actual_pos..];
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        String::from_utf8_lossy(&rest[..end]).to_string()
    }

    pub fn parse_attr_list(data: &[u8], endian: IgbEndian) -> Result<IgbAttrList, IgbError> {
        if data.len() < 8 {
            return Err(IgbError::CorruptVertexData("AttrList too short".into()));
        }
        let stride = Self::read_u32(data, 0, endian);
        let count = Self::read_u32(data, 4, endian) as usize;

        let mut attributes = Vec::with_capacity(count);
        let mut cur_offset = 8usize;
        for _ in 0..count {
            if cur_offset + 16 > data.len() {
                break;
            }
            let sem_id = Self::read_u32(data, cur_offset, endian);
            let fmt_id = Self::read_u32(data, cur_offset + 4, endian);
            let offset = Self::read_u32(data, cur_offset + 8, endian);
            let scale_u32 = Self::read_u32(data, cur_offset + 12, endian);
            let scale = if scale_u32 == 0 {
                1.0
            } else {
                f32::from_bits(scale_u32)
            };

            let semantic = match sem_id {
                0 | 1 => AttrSemantic::Position,
                2 => AttrSemantic::Normal,
                3 => AttrSemantic::TexCoord,
                4 => AttrSemantic::Color,
                other => AttrSemantic::Unknown(other),
            };

            let format = match fmt_id {
                0 => AttrFormat::Float32x3,
                1 => AttrFormat::Float32x2,
                2 => AttrFormat::Float32x4,
                3 => AttrFormat::WiiGxI16x3Fixed,
                4 => AttrFormat::PackedI8x3Normal,
                5 => AttrFormat::PackedI8x4Color,
                6 => AttrFormat::U16x2UV,
                other => AttrFormat::Unknown(other),
            };

            attributes.push(IgbAttrDesc {
                semantic,
                format,
                offset,
                scale,
            });
            cur_offset += 16;
        }

        Ok(IgbAttrList { stride, attributes })
    }

    pub fn parse_vertex_buffer(
        data: &[u8],
        endian: IgbEndian,
    ) -> Result<IgbVertexBuffer, IgbError> {
        if data.len() < 8 {
            return Err(IgbError::CorruptVertexData("VertexBuffer too short".into()));
        }
        let count = Self::read_u32(data, 0, endian) as usize;
        let stride = Self::read_u32(data, 4, endian) as usize;
        let buf_data = data[8..].to_vec();

        Ok(IgbVertexBuffer {
            count,
            stride,
            data: buf_data,
        })
    }

    pub fn parse_index_buffer(data: &[u8], endian: IgbEndian) -> Result<IgbIndexBuffer, IgbError> {
        if data.len() < 12 {
            return Err(IgbError::CorruptIndexData(
                "IndexBuffer header too short".into(),
            ));
        }
        let topo_id = Self::read_u32(data, 0, endian);
        let index_format = Self::read_u32(data, 4, endian); // 0: u16, 1: u32
        let count = Self::read_u32(data, 8, endian) as usize;

        let raw_indices = &data[12..];
        let mut raw_list = Vec::with_capacity(count);

        if index_format == 0 {
            // u16
            if raw_indices.len() < count * 2 {
                return Err(IgbError::CorruptIndexData("Index buffer truncated".into()));
            }
            for i in 0..count {
                raw_list.push(Self::read_u16(raw_indices, i * 2, endian) as u32);
            }
        } else {
            // u32
            if raw_indices.len() < count * 4 {
                return Err(IgbError::CorruptIndexData("Index buffer truncated".into()));
            }
            for i in 0..count {
                raw_list.push(Self::read_u32(raw_indices, i * 4, endian));
            }
        }

        let topology = if topo_id == 1 {
            IndexTopology::TriangleStrip
        } else {
            IndexTopology::Triangles
        };

        let indices = match topology {
            IndexTopology::Triangles => raw_list,
            IndexTopology::TriangleStrip => Self::strip_to_triangles(&raw_list),
        };

        Ok(IgbIndexBuffer { topology, indices })
    }

    pub fn strip_to_triangles(strip: &[u32]) -> Vec<u32> {
        if strip.len() < 3 {
            return Vec::new();
        }
        let mut triangles = Vec::with_capacity((strip.len() - 2) * 3);
        for i in 0..strip.len() - 2 {
            let (v0, v1, v2) = if i % 2 == 0 {
                (strip[i], strip[i + 1], strip[i + 2])
            } else {
                (strip[i + 1], strip[i], strip[i + 2])
            };
            // skip degenerate triangles
            if v0 != v1 && v1 != v2 && v0 != v2 {
                triangles.push(v0);
                triangles.push(v1);
                triangles.push(v2);
            }
        }
        triangles
    }

    pub fn decode_vertices(
        vb: &IgbVertexBuffer,
        attr_list: &IgbAttrList,
        endian: IgbEndian,
    ) -> Vec<IgbVertex> {
        let stride = if attr_list.stride > 0 {
            attr_list.stride as usize
        } else if vb.stride > 0 {
            vb.stride
        } else {
            48
        };

        let count = vb.count;
        let mut vertices = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * stride;
            if offset + stride > vb.data.len() {
                break;
            }
            let slice = &vb.data[offset..offset + stride];
            let mut vert = IgbVertex::default();

            for attr in &attr_list.attributes {
                let aoff = attr.offset as usize;
                match attr.semantic {
                    AttrSemantic::Position => match attr.format {
                        AttrFormat::Float32x3 if aoff + 12 <= slice.len() => {
                            vert.position = [
                                Self::read_f32(slice, aoff, endian),
                                Self::read_f32(slice, aoff + 4, endian),
                                Self::read_f32(slice, aoff + 8, endian),
                            ];
                        }
                        AttrFormat::WiiGxI16x3Fixed if aoff + 6 <= slice.len() => {
                            let scale = if attr.scale != 0.0 {
                                attr.scale
                            } else {
                                1.0 / 1024.0
                            };
                            vert.position = [
                                Self::read_i16(slice, aoff, endian) as f32 * scale,
                                Self::read_i16(slice, aoff + 2, endian) as f32 * scale,
                                Self::read_i16(slice, aoff + 4, endian) as f32 * scale,
                            ];
                        }
                        _ => {}
                    },
                    AttrSemantic::Normal => match attr.format {
                        AttrFormat::Float32x3 if aoff + 12 <= slice.len() => {
                            vert.normal = [
                                Self::read_f32(slice, aoff, endian),
                                Self::read_f32(slice, aoff + 4, endian),
                                Self::read_f32(slice, aoff + 8, endian),
                            ];
                        }
                        AttrFormat::PackedI8x3Normal if aoff + 3 <= slice.len() => {
                            let scale = if attr.scale != 0.0 {
                                attr.scale
                            } else {
                                1.0 / 127.0
                            };
                            vert.normal = [
                                (slice[aoff] as i8) as f32 * scale,
                                (slice[aoff + 1] as i8) as f32 * scale,
                                (slice[aoff + 2] as i8) as f32 * scale,
                            ];
                        }
                        _ => {}
                    },
                    AttrSemantic::TexCoord => match attr.format {
                        AttrFormat::Float32x2 if aoff + 8 <= slice.len() => {
                            vert.uv = [
                                Self::read_f32(slice, aoff, endian),
                                Self::read_f32(slice, aoff + 4, endian),
                            ];
                        }
                        AttrFormat::U16x2UV if aoff + 4 <= slice.len() => {
                            let scale = if attr.scale != 0.0 {
                                attr.scale
                            } else {
                                1.0 / 1024.0
                            };
                            vert.uv = [
                                Self::read_u16(slice, aoff, endian) as f32 * scale,
                                Self::read_u16(slice, aoff + 2, endian) as f32 * scale,
                            ];
                        }
                        _ => {}
                    },
                    AttrSemantic::Color => match attr.format {
                        AttrFormat::Float32x4 if aoff + 16 <= slice.len() => {
                            vert.color = [
                                Self::read_f32(slice, aoff, endian),
                                Self::read_f32(slice, aoff + 4, endian),
                                Self::read_f32(slice, aoff + 8, endian),
                                Self::read_f32(slice, aoff + 12, endian),
                            ];
                        }
                        AttrFormat::PackedI8x4Color if aoff + 4 <= slice.len() => {
                            vert.color = [
                                slice[aoff] as f32 / 255.0,
                                slice[aoff + 1] as f32 / 255.0,
                                slice[aoff + 2] as f32 / 255.0,
                                slice[aoff + 3] as f32 / 255.0,
                            ];
                        }
                        _ => {}
                    },
                    AttrSemantic::Unknown(_) => {}
                }
            }

            vertices.push(vert);
        }

        vertices
    }

    pub fn parse_geometry(
        data: &[u8],
        name: &str,
        endian: IgbEndian,
    ) -> Result<IgbGeometryNode, IgbError> {
        let mut vertex_buffer_index = None;
        let mut index_buffer_index = None;
        let mut attr_list_index = None;

        if data.len() >= 12 {
            vertex_buffer_index = Some(Self::read_u32(data, 0, endian) as usize);
            index_buffer_index = Some(Self::read_u32(data, 4, endian) as usize);
            attr_list_index = Some(Self::read_u32(data, 8, endian) as usize);
        }

        Ok(IgbGeometryNode {
            name: name.to_string(),
            vertex_buffer_index,
            index_buffer_index,
            attr_list_index,
            mesh: None,
        })
    }

    pub fn parse_node(
        data: &[u8],
        name: &str,
        endian: IgbEndian,
    ) -> Result<IgbSceneNode, IgbError> {
        let mut transform = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        let mut geometry_index = None;
        let mut children = Vec::new();

        if data.len() >= 64 {
            for (i, elem) in transform.iter_mut().enumerate() {
                *elem = Self::read_f32(data, i * 4, endian);
            }
        }
        if data.len() >= 68 {
            geometry_index = Some(Self::read_u32(data, 64, endian) as usize);
        }
        if data.len() >= 72 {
            let child_count = Self::read_u32(data, 68, endian) as usize;
            for i in 0..child_count {
                let off = 72 + i * 4;
                if off + 4 <= data.len() {
                    children.push(Self::read_u32(data, off, endian) as usize);
                }
            }
        }

        Ok(IgbSceneNode {
            name: name.to_string(),
            type_id: IgbObjectType::IgNode,
            children,
            geometry_index,
            transform,
        })
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
        data.extend_from_slice(&0u32.to_le_bytes()); // 0 objects
        data.extend_from_slice(&0x400u32.to_le_bytes()); // string table

        let hdr = IgbHeader::parse(Cursor::new(&data)).expect("Valid IGB");
        assert_eq!(&hdr.magic, b"IGBF");
        assert_eq!(hdr.version, 8);
        assert_eq!(hdr.object_count, 0);
        assert_eq!(hdr.endian, IgbEndian::Little);

        let model = IgbModel::parse(Cursor::new(&data)).expect("Valid IgbModel");
        assert_eq!(model.header.version, 8);
        assert!(model.meshes.is_empty());
    }

    #[test]
    fn test_igb_mesh_stream() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let mesh = IgbModel::parse_mesh_stream(&data, IgbEndian::Little).expect("Parse mesh");
        assert_eq!(mesh.vertices.len(), 1);
        assert_eq!(mesh.indices.len(), 3);
        assert_eq!(mesh.vertices[0].position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_object_directory_and_attribute_decoding() {
        // Build mock IGB with:
        // - AttrList (i16x3 position, i8x3 normal, u16x2 uv)
        // - VertexBuffer
        // - IndexBuffer (triangle strip with degenerate indices)
        // - IgGeometry
        // - IgNode
        // - String table

        let mut igb_bytes = Vec::new();
        // 1. Header (16 bytes)
        igb_bytes.extend_from_slice(b"IGBF");
        igb_bytes.extend_from_slice(&8u32.to_be_bytes()); // BigEndian Wii
        igb_bytes.extend_from_slice(&5u32.to_be_bytes()); // 5 objects
        let str_table_pos = 16 + 5 * 16 + 1000; // Will pad later
        igb_bytes.extend_from_slice(&(str_table_pos as u32).to_be_bytes());

        // We will layout objects after directory table:
        // Directory table is 16..96 (5 entries * 16 bytes = 80 bytes)
        let off_attr = 96usize;
        let mut attr_bytes = Vec::new();
        attr_bytes.extend_from_slice(&14u32.to_be_bytes()); // stride 14: 6 pos + 4 norm + 4 uv
        attr_bytes.extend_from_slice(&3u32.to_be_bytes()); // 3 attributes
                                                           // Attr 0: Position, WiiGxI16x3Fixed (offset 0, scale 1/1000 = 0.001)
        attr_bytes.extend_from_slice(&1u32.to_be_bytes()); // Semantic Position
        attr_bytes.extend_from_slice(&3u32.to_be_bytes()); // Format WiiGxI16x3Fixed
        attr_bytes.extend_from_slice(&0u32.to_be_bytes()); // Offset 0
        attr_bytes.extend_from_slice(&0.001f32.to_bits().to_be_bytes()); // Scale
                                                                         // Attr 1: Normal, PackedI8x3Normal (offset 6, scale 1.0/127.0)
        attr_bytes.extend_from_slice(&2u32.to_be_bytes()); // Semantic Normal
        attr_bytes.extend_from_slice(&4u32.to_be_bytes()); // Format PackedI8x3Normal
        attr_bytes.extend_from_slice(&6u32.to_be_bytes()); // Offset 6
        attr_bytes.extend_from_slice(&(1.0f32 / 127.0f32).to_bits().to_be_bytes());
        // Attr 2: TexCoord, U16x2UV (offset 10, scale 1/1000 = 0.001)
        attr_bytes.extend_from_slice(&3u32.to_be_bytes()); // Semantic TexCoord
        attr_bytes.extend_from_slice(&6u32.to_be_bytes()); // Format U16x2UV
        attr_bytes.extend_from_slice(&10u32.to_be_bytes()); // Offset 10
        attr_bytes.extend_from_slice(&0.001f32.to_bits().to_be_bytes());

        let off_vb = off_attr + attr_bytes.len();
        let mut vb_bytes = Vec::new();
        vb_bytes.extend_from_slice(&4u32.to_be_bytes()); // 4 vertices
        vb_bytes.extend_from_slice(&14u32.to_be_bytes()); // stride 14
                                                          // Vertex 0: pos=(1000, 2000, 3000), norm=(0, 127, 0), uv=(500, 500)
        vb_bytes.extend_from_slice(&1000i16.to_be_bytes());
        vb_bytes.extend_from_slice(&2000i16.to_be_bytes());
        vb_bytes.extend_from_slice(&3000i16.to_be_bytes());
        vb_bytes.extend_from_slice(&[0i8 as u8, 127i8 as u8, 0i8 as u8, 0u8]); // 3 norm + 1 pad
        vb_bytes.extend_from_slice(&500u16.to_be_bytes());
        vb_bytes.extend_from_slice(&500u16.to_be_bytes());
        // Vertex 1: pos=(0, 0, 0), norm=(0, 127, 0), uv=(0, 0)
        vb_bytes.extend_from_slice(&0i16.to_be_bytes());
        vb_bytes.extend_from_slice(&0i16.to_be_bytes());
        vb_bytes.extend_from_slice(&0i16.to_be_bytes());
        vb_bytes.extend_from_slice(&[0i8 as u8, 127i8 as u8, 0i8 as u8, 0u8]);
        vb_bytes.extend_from_slice(&0u16.to_be_bytes());
        vb_bytes.extend_from_slice(&0u16.to_be_bytes());
        // Vertex 2: pos=(1000, 0, 0), norm=(0, 127, 0), uv=(1000, 0)
        vb_bytes.extend_from_slice(&1000i16.to_be_bytes());
        vb_bytes.extend_from_slice(&0i16.to_be_bytes());
        vb_bytes.extend_from_slice(&0i16.to_be_bytes());
        vb_bytes.extend_from_slice(&[0i8 as u8, 127i8 as u8, 0i8 as u8, 0u8]);
        vb_bytes.extend_from_slice(&1000u16.to_be_bytes());
        vb_bytes.extend_from_slice(&0u16.to_be_bytes());
        // Vertex 3: pos=(1000, 2000, 0), norm=(0, 127, 0), uv=(1000, 1000)
        vb_bytes.extend_from_slice(&1000i16.to_be_bytes());
        vb_bytes.extend_from_slice(&2000i16.to_be_bytes());
        vb_bytes.extend_from_slice(&0i16.to_be_bytes());
        vb_bytes.extend_from_slice(&[0i8 as u8, 127i8 as u8, 0i8 as u8, 0u8]);
        vb_bytes.extend_from_slice(&1000u16.to_be_bytes());
        vb_bytes.extend_from_slice(&1000u16.to_be_bytes());

        let off_ib = off_vb + vb_bytes.len();
        let mut ib_bytes = Vec::new();
        ib_bytes.extend_from_slice(&1u32.to_be_bytes()); // topology 1: TriangleStrip
        ib_bytes.extend_from_slice(&0u32.to_be_bytes()); // format 0: u16
        ib_bytes.extend_from_slice(&5u32.to_be_bytes()); // 5 indices
                                                         // strip: 0, 1, 2, 2, 3 (2, 2 degenerate)
        ib_bytes.extend_from_slice(&0u16.to_be_bytes());
        ib_bytes.extend_from_slice(&1u16.to_be_bytes());
        ib_bytes.extend_from_slice(&2u16.to_be_bytes());
        ib_bytes.extend_from_slice(&2u16.to_be_bytes());
        ib_bytes.extend_from_slice(&3u16.to_be_bytes());

        let off_geo = off_ib + ib_bytes.len();
        let mut geo_bytes = Vec::new();
        geo_bytes.extend_from_slice(&0u32.to_be_bytes()); // vb index 0
        geo_bytes.extend_from_slice(&0u32.to_be_bytes()); // ib index 0
        geo_bytes.extend_from_slice(&0u32.to_be_bytes()); // attr index 0

        let off_node = off_geo + geo_bytes.len();
        let mut node_bytes = Vec::new();
        for _ in 0..16 {
            node_bytes.extend_from_slice(&0.0f32.to_be_bytes());
        }
        node_bytes.extend_from_slice(&0u32.to_be_bytes()); // geometry index 0
        node_bytes.extend_from_slice(&0u32.to_be_bytes()); // 0 children

        // Object directory entries
        // Entry 0: AttrList
        igb_bytes.extend_from_slice(&3u32.to_be_bytes());
        igb_bytes.extend_from_slice(&0u32.to_be_bytes()); // str off 0 -> "attrs"
        igb_bytes.extend_from_slice(&(off_attr as u32).to_be_bytes());
        igb_bytes.extend_from_slice(&(attr_bytes.len() as u32).to_be_bytes());

        // Entry 1: VertexBuffer
        igb_bytes.extend_from_slice(&4u32.to_be_bytes());
        igb_bytes.extend_from_slice(&6u32.to_be_bytes()); // str off 6 -> "verts"
        igb_bytes.extend_from_slice(&(off_vb as u32).to_be_bytes());
        igb_bytes.extend_from_slice(&(vb_bytes.len() as u32).to_be_bytes());

        // Entry 2: IndexBuffer
        igb_bytes.extend_from_slice(&5u32.to_be_bytes());
        igb_bytes.extend_from_slice(&12u32.to_be_bytes()); // str off 12 -> "indices"
        igb_bytes.extend_from_slice(&(off_ib as u32).to_be_bytes());
        igb_bytes.extend_from_slice(&(ib_bytes.len() as u32).to_be_bytes());

        // Entry 3: Geometry
        igb_bytes.extend_from_slice(&2u32.to_be_bytes());
        igb_bytes.extend_from_slice(&20u32.to_be_bytes()); // str off 20 -> "hero_mesh"
        igb_bytes.extend_from_slice(&(off_geo as u32).to_be_bytes());
        igb_bytes.extend_from_slice(&(geo_bytes.len() as u32).to_be_bytes());

        // Entry 4: Node
        igb_bytes.extend_from_slice(&1u32.to_be_bytes());
        igb_bytes.extend_from_slice(&30u32.to_be_bytes()); // str off 30 -> "root_node"
        igb_bytes.extend_from_slice(&(off_node as u32).to_be_bytes());
        igb_bytes.extend_from_slice(&(node_bytes.len() as u32).to_be_bytes());

        // Append object payloads
        igb_bytes.extend_from_slice(&attr_bytes);
        igb_bytes.extend_from_slice(&vb_bytes);
        igb_bytes.extend_from_slice(&ib_bytes);
        igb_bytes.extend_from_slice(&geo_bytes);
        igb_bytes.extend_from_slice(&node_bytes);

        // Pad to string table
        let cur_len = igb_bytes.len();
        if cur_len < str_table_pos {
            igb_bytes.resize(str_table_pos, 0);
        }

        // String table
        // 0: "attrs\0"
        // 6: "verts\0"
        // 12: "indices\0"
        // 20: "hero_mesh\0"
        // 30: "root_node\0"
        igb_bytes.extend_from_slice(b"attrs\0verts\0indices\0hero_mesh\0root_node\0");

        let model = IgbModel::parse(Cursor::new(&igb_bytes)).expect("Parse structured IGB");
        assert_eq!(model.directory.len(), 5);
        assert_eq!(model.directory[3].name, "hero_mesh");
        assert_eq!(model.directory[4].name, "root_node");

        assert_eq!(model.attr_lists.len(), 1);
        assert_eq!(model.vertex_buffers.len(), 1);
        assert_eq!(model.index_buffers.len(), 1);
        assert_eq!(model.geometries.len(), 1);
        assert_eq!(model.nodes.len(), 1);

        assert_eq!(model.meshes.len(), 1);
        let mesh = &model.meshes[0];
        assert_eq!(mesh.name, "hero_mesh");
        assert_eq!(mesh.vertices.len(), 4);
        assert!((mesh.vertices[0].position[0] - 1.0).abs() < 1e-4);
        assert!((mesh.vertices[0].position[1] - 2.0).abs() < 1e-4);
        assert!((mesh.vertices[0].position[2] - 3.0).abs() < 1e-4);
        assert!((mesh.vertices[0].uv[0] - 0.5).abs() < 1e-4);
        assert!((mesh.vertices[0].uv[1] - 0.5).abs() < 1e-4);
        // Normal scaled to 127 / 127 = 1.0
        assert!((mesh.vertices[0].normal[1] - 1.0).abs() < 1e-4);
        // Stripped triangles: [0, 1, 2] valid, [1, 2, 2] degenerate skipped, [2, 2, 3] degenerate skipped
        assert_eq!(mesh.indices, vec![0, 1, 2]);
    }
}
