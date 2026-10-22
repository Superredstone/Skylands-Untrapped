use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read};
use thiserror::Error;

pub const IGB_MAGIC: &[u8; 4] = b"IGBF";
pub const IGZ_MAGIC: &[u8; 4] = b"IGZ\x01";

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
    IgMaterial,
    IgTexture,
    IgSkeleton,
    IgSkin,
    IgAnimation,
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
            6 => IgbObjectType::IgMaterial,
            7 => IgbObjectType::IgTexture,
            8 => IgbObjectType::IgSkeleton,
            9 => IgbObjectType::IgSkin,
            10 => IgbObjectType::IgAnimation,
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
    BlendIndices,
    BlendWeights,
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
    Float16x2UV,      // 2 x IEEE-754 half-float (f16) UV
    PackedU8x4,       // 4 x u8 indices or weights
    PackedU16x4,      // 4 x u16 indices
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
    pub material_index: Option<usize>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct IgbMaterial {
    pub name: String,
    pub texture_index: Option<usize>,
    pub texture_name: Option<String>,
    pub blend_mode: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IgbTextureRef {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IgbVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub bone_indices: [u32; 4],
    pub bone_weights: [f32; 4],
}

impl Default for IgbVertex {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IgbMesh {
    pub name: String,
    pub vertices: Vec<IgbVertex>,
    pub indices: Vec<u32>,
    pub material_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Spawn,
    Prop,
    Collectible,
    TriggerZone,
    Doorway,
    Other,
}

impl MarkerKind {
    pub fn from_node_name(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("sp_")
            || lower.starts_with("spawn_")
            || lower.contains("spawn")
            || lower.starts_with("start")
        {
            Some(MarkerKind::Spawn)
        } else if lower.starts_with("door_") || lower.starts_with("exit_") || lower.contains("gate")
        {
            Some(MarkerKind::Doorway)
        } else if lower.starts_with("portal_") || lower.starts_with("zone_") {
            Some(MarkerKind::TriggerZone)
        } else if lower.starts_with("prop_")
            || lower.starts_with('^')
            || lower.contains("light")
            || lower.contains("lantern")
        {
            Some(MarkerKind::Prop)
        } else if lower.starts_with("pickup_")
            || lower.starts_with("coin_")
            || lower.starts_with("trap_")
            || lower.contains("gem")
        {
            Some(MarkerKind::Collectible)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LevelMarker {
    pub name: String,
    pub kind: MarkerKind,
    pub position: [f32; 3],
    pub world_transform: [f32; 16],
    pub node_index: usize,
    pub geometry_index: Option<usize>,
    pub mesh: Option<IgbMesh>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInstance {
    pub node_index: usize,
    pub geometry_index: usize,
    pub world_transform: [f32; 16],
}

/// Models with more geometries than this are treated as static levels.
pub const LEVEL_GEOMETRY_THRESHOLD: usize = 64;

pub fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            out[c * 4 + r] = a[r] * b[c * 4]
                + a[4 + r] * b[c * 4 + 1]
                + a[8 + r] * b[c * 4 + 2]
                + a[12 + r] * b[c * 4 + 3];
        }
    }
    out
}

pub fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    let w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
    let inv_w = if w.abs() > 1e-6 { 1.0 / w } else { 1.0 };
    [
        (m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12]) * inv_w,
        (m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13]) * inv_w,
        (m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14]) * inv_w,
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgzImage2 {
    pub offset: usize,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub mipmap_count: u16,
    pub array: u16,
    pub format: u32,
    pub texture_offset: usize,
    pub texture_size: usize,
}

impl IgzImage2 {
    pub fn parse(
        data: &[u8],
        obj_offset: usize,
        version: u32,
        endian: IgbEndian,
        exid_formats: &[u32],
        tmhn_entries: &[(usize, usize)],
    ) -> Option<Self> {
        let field_start = if version == 5 {
            obj_offset + 0x0c
        } else if version == 9 {
            obj_offset + 0x34
        } else {
            obj_offset + 0x30
        };

        if field_start + 28 > data.len() {
            return None;
        }

        let width = IgbModel::read_u16(data, field_start, endian);
        let height = IgbModel::read_u16(data, field_start + 2, endian);
        let depth = IgbModel::read_u16(data, field_start + 4, endian);
        let mipmap_count = IgbModel::read_u16(data, field_start + 6, endian);
        let array = IgbModel::read_u16(data, field_start + 8, endian);

        let exid_idx = IgbModel::read_u16(data, field_start + 14, endian) as usize;
        let format = exid_formats.get(exid_idx).copied().unwrap_or(0);

        let tmhn_idx = IgbModel::read_u32(data, field_start + 24, endian) as usize;
        let (texture_offset, texture_size) = tmhn_entries.get(tmhn_idx).copied().unwrap_or((0, 0));

        Some(Self {
            offset: obj_offset,
            width,
            height,
            depth,
            mipmap_count,
            array,
            format,
            texture_offset,
            texture_size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgzChunkDescriptor {
    pub offset: usize,
    pub size: usize,
    pub unknown1: u32,
    pub unknown2: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgzHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub crc: u32,
    pub endian: IgbEndian,
    pub descriptors: Vec<IgzChunkDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgzOffsetMapFixup {
    pub magic: [u8; 4],
    pub count: u32,
    pub length: u32,
    pub start_of_data: u32,
    pub offsets: Vec<usize>,
}

impl IgzOffsetMapFixup {
    pub fn parse(
        data: &[u8],
        fixup_offset: usize,
        endian: IgbEndian,
        version: u32,
        descriptors: &[IgzChunkDescriptor],
    ) -> Result<Self, IgbError> {
        if fixup_offset + 16 > data.len() {
            return Err(IgbError::CorruptObjectDirectory(
                "Fixup header out of bounds".into(),
            ));
        }

        let mut magic = [0u8; 4];
        magic.copy_from_slice(&data[fixup_offset..fixup_offset + 4]);

        let (count_offset, length_offset, start_offset) = if version <= 6 {
            (fixup_offset + 12, fixup_offset + 16, fixup_offset + 20)
        } else {
            (fixup_offset + 4, fixup_offset + 8, fixup_offset + 12)
        };

        if start_offset + 4 > data.len() {
            return Err(IgbError::CorruptObjectDirectory(
                "Fixup fields out of bounds".into(),
            ));
        }

        let count = IgbModel::read_u32(data, count_offset, endian);
        let length = IgbModel::read_u32(data, length_offset, endian);
        let start_of_data = IgbModel::read_u32(data, start_offset, endian);

        let data_start = fixup_offset + (start_of_data as usize);
        let data_end = (fixup_offset + (length as usize)).min(data.len());
        if data_start > data_end {
            return Err(IgbError::CorruptObjectDirectory(
                "Fixup data bounds invalid".into(),
            ));
        }

        let packed_data = &data[data_start..data_end];
        let offsets = Self::unpack_offsets(packed_data, count as usize, version, descriptors)?;

        Ok(Self {
            magic,
            count,
            length,
            start_of_data,
            offsets,
        })
    }

    pub fn unpack_offsets(
        packed_data: &[u8],
        count: usize,
        version: u32,
        descriptors: &[IgzChunkDescriptor],
    ) -> Result<Vec<usize>, IgbError> {
        let mut offsets = Vec::with_capacity(count);
        let mut byte_idx = 0;
        let mut high_nibble = false;
        let mut previous_int: u32 = 0;

        let read_nibble = |byte_idx: &mut usize, high_nibble: &mut bool| -> Option<u8> {
            if *byte_idx >= packed_data.len() {
                return None;
            }
            let byte = packed_data[*byte_idx];
            if !*high_nibble {
                *high_nibble = true;
                Some(byte & 0x0F)
            } else {
                *high_nibble = false;
                *byte_idx += 1;
                Some((byte >> 4) & 0x0F)
            }
        };

        for _ in 0..count {
            let mut current = match read_nibble(&mut byte_idx, &mut high_nibble) {
                Some(n) => n as u32,
                None => {
                    return Err(IgbError::CorruptObjectDirectory(
                        "Unexpected end of packed fixup stream".into(),
                    ));
                }
            };

            let mut shift_amount: u32 = 3;
            let mut unpacked_int = current & 7;

            while (current & 8) != 0 {
                current = match read_nibble(&mut byte_idx, &mut high_nibble) {
                    Some(n) => n as u32,
                    None => {
                        return Err(IgbError::CorruptObjectDirectory(
                            "Unexpected end of packed integer continuation".into(),
                        ));
                    }
                };

                let shift = shift_amount & 0x1F;
                unpacked_int |= (current & 7) << shift;
                shift_amount = shift_amount.wrapping_add(3);
            }

            let delta = unpacked_int
                .wrapping_mul(4)
                .wrapping_add(if version < 9 { 4 } else { 0 });
            previous_int = previous_int.wrapping_add(delta);

            let abs_offset = if version <= 6 {
                let desc_idx = ((previous_int >> 24) + 1) as usize;
                let rel_offset = (previous_int & 0x00FF_FFFF) as usize;
                let base = descriptors.get(desc_idx).map(|d| d.offset).unwrap_or(0);
                base + rel_offset
            } else {
                let desc_idx = ((previous_int >> 27) + 1) as usize;
                let rel_offset = (previous_int & 0x07FF_FFFF) as usize;
                let base = descriptors.get(desc_idx).map(|d| d.offset).unwrap_or(0);
                base + rel_offset
            };

            offsets.push(abs_offset);
        }

        Ok(offsets)
    }
}

impl IgzHeader {
    pub fn parse(data: &[u8]) -> Result<Self, IgbError> {
        if data.len() < 24 {
            return Err(IgbError::CorruptObjectDirectory(
                "IGZ file too short".into(),
            ));
        }

        let magic = [data[0], data[1], data[2], data[3]];
        if &magic != IGZ_MAGIC {
            return Err(IgbError::InvalidMagic(magic));
        }

        let ver_be = IgbModel::read_u32(data, 4, IgbEndian::Big);
        let endian = if ver_be <= 0x1000 {
            IgbEndian::Big
        } else {
            IgbEndian::Little
        };

        let version = IgbModel::read_u32(data, 4, endian);
        let crc = IgbModel::read_u32(data, 8, endian);

        let desc_start = if version <= 6 { 0x10 } else { 0x18 };
        let mut descriptors = Vec::new();

        let mut pos = desc_start;
        while pos + 16 <= data.len() {
            let offset = IgbModel::read_u32(data, pos, endian) as usize;
            let size = IgbModel::read_u32(data, pos + 4, endian) as usize;
            let unknown1 = IgbModel::read_u32(data, pos + 8, endian);
            let unknown2 = IgbModel::read_u32(data, pos + 12, endian);

            if offset == 0 {
                break;
            }

            descriptors.push(IgzChunkDescriptor {
                offset,
                size,
                unknown1,
                unknown2,
            });
            pos += 16;
        }

        Ok(Self {
            magic,
            version,
            crc,
            endian,
            descriptors,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgzObject {
    pub offset: usize,
    pub type_index: usize,
    pub type_name: Option<String>,
    pub item_count: u32,
    pub length: u16,
    pub data: Vec<u8>,
    pub fields: Vec<u32>,
}

impl IgzObject {
    pub fn parse(
        data: &[u8],
        offset: usize,
        version: u32,
        endian: IgbEndian,
        type_table: &[String],
    ) -> Result<Self, IgbError> {
        if offset >= data.len() {
            return Err(IgbError::CorruptObjectDirectory(
                "Object offset out of bounds".into(),
            ));
        }

        let slice = &data[offset..];
        let (type_index, item_count, length, data_offset) = if version == 6 || version == 8 {
            if slice.len() < 16 {
                return Err(IgbError::CorruptObjectDirectory(
                    "Object header truncated".into(),
                ));
            }
            let type_idx = IgbModel::read_u16(slice, 2, endian) as usize;
            let item_count = IgbModel::read_u32(slice, 4, endian);
            let flags1 = IgbModel::read_u16(slice, 8, endian);
            let (length, after_hdr) = if (flags1 & 0x3000) != 0 {
                let len = IgbModel::read_u16(slice, 10, endian);
                (len, 16)
            } else {
                let len = IgbModel::read_u16(slice, 14, endian);
                (len, 16)
            };
            (type_idx, item_count, length, after_hdr)
        } else {
            if slice.len() < 22 {
                return Err(IgbError::CorruptObjectDirectory(
                    "Object header truncated".into(),
                ));
            }
            let type_idx = IgbModel::read_u32(slice, 0, endian) as usize;
            let item_count = IgbModel::read_u32(slice, 8, endian);
            let length = IgbModel::read_u16(slice, 18, endian);
            (type_idx, item_count, length, 22)
        };

        let type_name = type_table.get(type_index).cloned();

        let data_start = data_offset;
        let data_end = (data_start + length as usize).min(slice.len());
        let obj_data = if data_start <= slice.len() {
            slice[data_start..data_end].to_vec()
        } else {
            Vec::new()
        };

        let mut fields = Vec::new();
        if slice.len() >= 0x14 {
            let field_slice = &slice[0x14..];
            let num_fields = (length as usize / 4).min(field_slice.len() / 4);
            for i in 0..num_fields {
                fields.push(IgbModel::read_u32(field_slice, i * 4, endian));
            }
        }

        Ok(Self {
            offset,
            type_index,
            type_name,
            item_count,
            length,
            data: obj_data,
            fields,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgzObjectList {
    pub list_object: IgzObject,
    pub objects: Vec<IgzObject>,
}

impl IgzObjectList {
    pub fn parse(
        data: &[u8],
        descriptors: &[IgzChunkDescriptor],
        version: u32,
        endian: IgbEndian,
        type_table: &[String],
        rvtb_offsets: &[usize],
    ) -> Result<Self, IgbError> {
        if descriptors.len() < 2 {
            return Err(IgbError::CorruptObjectDirectory(
                "Not enough chunk descriptors for ObjectList".into(),
            ));
        }

        let desc1 = &descriptors[1];
        let mut list_offset = desc1.offset;
        if version == 6 || version == 8 {
            list_offset += desc1.unknown2 as usize;
        }

        let mut list_object = IgzObject::parse(data, list_offset, version, endian, type_table)?;
        let mut pointer_fields = Vec::new();
        if list_offset + 0x14 <= data.len() {
            let field_slice = &data[list_offset + 0x14..];
            let count = (list_object.item_count as usize).min(field_slice.len() / 4);
            for i in 0..count {
                let pot = IgbModel::read_u32(field_slice, i * 4, endian);
                if (pot & 0x8000_0000) == 0 {
                    pointer_fields.push(pot);
                }
            }
        }
        list_object.item_count = pointer_fields.len() as u32;
        list_object.fields = pointer_fields;

        let mut objects = Vec::new();
        for &offset in rvtb_offsets.iter().skip(1) {
            if let Ok(obj) = IgzObject::parse(data, offset, version, endian, type_table) {
                objects.push(obj);
            }
        }

        Ok(Self {
            list_object,
            objects,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub inverse_bind_pose: [f32; 16],
    pub local_transform: [f32; 16],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
    pub root: usize,
    pub name_to_index: std::collections::HashMap<String, usize>,
}

impl Skeleton {
    pub fn new(bones: Vec<Bone>, root: usize) -> Self {
        let mut name_to_index = std::collections::HashMap::new();
        for (i, b) in bones.iter().enumerate() {
            name_to_index.insert(b.name.clone(), i);
        }
        Self {
            bones,
            root,
            name_to_index,
        }
    }

    pub fn get_bone_index(&self, name: &str) -> Option<usize> {
        self.name_to_index.get(name).copied()
    }

    pub fn serialize_binary(&self, endian: IgbEndian) -> Vec<u8> {
        let mut out = Vec::new();
        match endian {
            IgbEndian::Little => {
                out.extend_from_slice(&(self.bones.len() as u32).to_le_bytes());
                out.extend_from_slice(&(self.root as u32).to_le_bytes());
                for bone in &self.bones {
                    let p = match bone.parent {
                        Some(idx) => idx as i32,
                        None => -1,
                    };
                    out.extend_from_slice(&p.to_le_bytes());
                    for &f in &bone.inverse_bind_pose {
                        out.extend_from_slice(&f.to_le_bytes());
                    }
                    for &f in &bone.local_transform {
                        out.extend_from_slice(&f.to_le_bytes());
                    }
                    let name_bytes = bone.name.as_bytes();
                    out.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
                    out.extend_from_slice(name_bytes);
                }
            }
            IgbEndian::Big => {
                out.extend_from_slice(&(self.bones.len() as u32).to_be_bytes());
                out.extend_from_slice(&(self.root as u32).to_be_bytes());
                for bone in &self.bones {
                    let p = match bone.parent {
                        Some(idx) => idx as i32,
                        None => -1,
                    };
                    out.extend_from_slice(&p.to_be_bytes());
                    for &f in &bone.inverse_bind_pose {
                        out.extend_from_slice(&f.to_be_bytes());
                    }
                    for &f in &bone.local_transform {
                        out.extend_from_slice(&f.to_be_bytes());
                    }
                    let name_bytes = bone.name.as_bytes();
                    out.extend_from_slice(&(name_bytes.len() as u32).to_be_bytes());
                    out.extend_from_slice(name_bytes);
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe<T> {
    pub time: f32,
    pub value: T,
}

impl<T> Keyframe<T> {
    pub fn new(time: f32, value: T) -> Self {
        Self { time, value }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoneTrack {
    pub bone_index: usize,
    pub positions: Vec<Keyframe<[f32; 3]>>,
    pub rotations: Vec<Keyframe<[f32; 4]>>,
    pub scales: Vec<Keyframe<[f32; 3]>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub tracks: Vec<BoneTrack>,
}

pub fn lerp_vec3(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub fn slerp_quat(q1: &[f32; 4], q2: &[f32; 4], t: f32) -> [f32; 4] {
    let mut dot = q1[0] * q2[0] + q1[1] * q2[1] + q1[2] * q2[2] + q1[3] * q2[3];
    let mut q2_adj = *q2;
    if dot < 0.0 {
        dot = -dot;
        q2_adj = [-q2[0], -q2[1], -q2[2], -q2[3]];
    }

    if dot > 0.9995 {
        let out = [
            q1[0] + t * (q2_adj[0] - q1[0]),
            q1[1] + t * (q2_adj[1] - q1[1]),
            q1[2] + t * (q2_adj[2] - q1[2]),
            q1[3] + t * (q2_adj[3] - q1[3]),
        ];
        let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3]).sqrt();
        if len > 1e-6 {
            [out[0] / len, out[1] / len, out[2] / len, out[3] / len]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        }
    } else {
        let theta = dot.clamp(-1.0, 1.0).acos();
        let sin_theta = theta.sin();
        let scale0 = ((1.0 - t) * theta).sin() / sin_theta;
        let scale1 = (t * theta).sin() / sin_theta;
        [
            scale0 * q1[0] + scale1 * q2_adj[0],
            scale0 * q1[1] + scale1 * q2_adj[1],
            scale0 * q1[2] + scale1 * q2_adj[2],
            scale0 * q1[3] + scale1 * q2_adj[3],
        ]
    }
}

pub fn quat_to_mat4(q: &[f32; 4]) -> [f32; 16] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;

    [
        1.0 - (yy + zz),
        xy + wz,
        xz - wy,
        0.0,
        xy - wz,
        1.0 - (xx + zz),
        yz + wx,
        0.0,
        xz + wy,
        yz - wx,
        1.0 - (xx + yy),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

pub fn compose_transform(pos: &[f32; 3], rot: &[f32; 4], scale: &[f32; 3]) -> [f32; 16] {
    let r = quat_to_mat4(rot);
    [
        r[0] * scale[0],
        r[1] * scale[0],
        r[2] * scale[0],
        0.0,
        r[4] * scale[1],
        r[5] * scale[1],
        r[6] * scale[1],
        0.0,
        r[8] * scale[2],
        r[9] * scale[2],
        r[10] * scale[2],
        0.0,
        pos[0],
        pos[1],
        pos[2],
        1.0,
    ]
}

fn sample_channel<T: Copy, F: Fn(&T, &T, f32) -> T>(
    keyframes: &[Keyframe<T>],
    time: f32,
    interpolate: F,
) -> Option<T> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 || time <= keyframes[0].time {
        return Some(keyframes[0].value);
    }
    if time >= keyframes.last().unwrap().time {
        return Some(keyframes.last().unwrap().value);
    }

    for i in 0..keyframes.len() - 1 {
        let k0 = &keyframes[i];
        let k1 = &keyframes[i + 1];
        if time >= k0.time && time <= k1.time {
            let dt = k1.time - k0.time;
            let factor = if dt > 0.0 { (time - k0.time) / dt } else { 0.0 };
            return Some(interpolate(&k0.value, &k1.value, factor));
        }
    }
    Some(keyframes.last().unwrap().value)
}

pub fn sample_vec3(keyframes: &[Keyframe<[f32; 3]>], time: f32) -> Option<[f32; 3]> {
    sample_channel(keyframes, time, lerp_vec3)
}

pub fn sample_quat(keyframes: &[Keyframe<[f32; 4]>], time: f32) -> Option<[f32; 4]> {
    sample_channel(keyframes, time, slerp_quat)
}

impl AnimationClip {
    pub fn new(name: impl Into<String>, duration: f32, tracks: Vec<BoneTrack>) -> Self {
        Self {
            name: name.into(),
            duration,
            tracks,
        }
    }

    pub fn sample_pose(&self, time: f32, skeleton: &Skeleton) -> Vec<[f32; 16]> {
        let global_transforms = self.sample_global_transforms(time, skeleton);
        let mut skinning_matrices = vec![mat4_identity(); skeleton.bones.len()];
        for (i, bone) in skeleton.bones.iter().enumerate() {
            skinning_matrices[i] = mat4_mul(&global_transforms[i], &bone.inverse_bind_pose);
        }
        skinning_matrices
    }

    pub fn sample_global_transforms(&self, time: f32, skeleton: &Skeleton) -> Vec<[f32; 16]> {
        let t = if self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            0.0
        };

        let mut local_transforms: Vec<[f32; 16]> =
            skeleton.bones.iter().map(|b| b.local_transform).collect();

        for track in &self.tracks {
            if track.bone_index >= local_transforms.len() {
                continue;
            }
            let pos = sample_vec3(&track.positions, t).unwrap_or([0.0, 0.0, 0.0]);
            let rot = sample_quat(&track.rotations, t).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let scale = sample_vec3(&track.scales, t).unwrap_or([1.0, 1.0, 1.0]);
            local_transforms[track.bone_index] = compose_transform(&pos, &rot, &scale);
        }

        let mut computed = vec![false; skeleton.bones.len()];
        let mut global_transforms = vec![mat4_identity(); skeleton.bones.len()];

        fn compute_bone(
            idx: usize,
            skeleton: &Skeleton,
            local: &[[f32; 16]],
            global: &mut [[f32; 16]],
            computed: &mut [bool],
        ) {
            if computed[idx] {
                return;
            }
            if let Some(p) = skeleton.bones[idx].parent {
                if p < skeleton.bones.len() && p != idx {
                    compute_bone(p, skeleton, local, global, computed);
                    global[idx] = mat4_mul(&global[p], &local[idx]);
                    computed[idx] = true;
                    return;
                }
            }
            global[idx] = local[idx];
            computed[idx] = true;
        }

        for i in 0..skeleton.bones.len() {
            compute_bone(
                i,
                skeleton,
                &local_transforms,
                &mut global_transforms,
                &mut computed,
            );
        }

        global_transforms
    }

    pub fn serialize_binary(&self, endian: IgbEndian) -> Vec<u8> {
        let mut out = Vec::new();
        match endian {
            IgbEndian::Little => {
                out.extend_from_slice(&self.duration.to_le_bytes());
                out.extend_from_slice(&(self.tracks.len() as u32).to_le_bytes());
                for track in &self.tracks {
                    out.extend_from_slice(&(track.bone_index as u32).to_le_bytes());
                    out.extend_from_slice(&(track.positions.len() as u32).to_le_bytes());
                    out.extend_from_slice(&(track.rotations.len() as u32).to_le_bytes());
                    out.extend_from_slice(&(track.scales.len() as u32).to_le_bytes());
                    for kf in &track.positions {
                        out.extend_from_slice(&kf.time.to_le_bytes());
                        out.extend_from_slice(&kf.value[0].to_le_bytes());
                        out.extend_from_slice(&kf.value[1].to_le_bytes());
                        out.extend_from_slice(&kf.value[2].to_le_bytes());
                    }
                    for kf in &track.rotations {
                        out.extend_from_slice(&kf.time.to_le_bytes());
                        out.extend_from_slice(&kf.value[0].to_le_bytes());
                        out.extend_from_slice(&kf.value[1].to_le_bytes());
                        out.extend_from_slice(&kf.value[2].to_le_bytes());
                        out.extend_from_slice(&kf.value[3].to_le_bytes());
                    }
                    for kf in &track.scales {
                        out.extend_from_slice(&kf.time.to_le_bytes());
                        out.extend_from_slice(&kf.value[0].to_le_bytes());
                        out.extend_from_slice(&kf.value[1].to_le_bytes());
                        out.extend_from_slice(&kf.value[2].to_le_bytes());
                    }
                }
            }
            IgbEndian::Big => {
                out.extend_from_slice(&self.duration.to_be_bytes());
                out.extend_from_slice(&(self.tracks.len() as u32).to_be_bytes());
                for track in &self.tracks {
                    out.extend_from_slice(&(track.bone_index as u32).to_be_bytes());
                    out.extend_from_slice(&(track.positions.len() as u32).to_be_bytes());
                    out.extend_from_slice(&(track.rotations.len() as u32).to_be_bytes());
                    out.extend_from_slice(&(track.scales.len() as u32).to_be_bytes());
                    for kf in &track.positions {
                        out.extend_from_slice(&kf.time.to_be_bytes());
                        out.extend_from_slice(&kf.value[0].to_be_bytes());
                        out.extend_from_slice(&kf.value[1].to_be_bytes());
                        out.extend_from_slice(&kf.value[2].to_be_bytes());
                    }
                    for kf in &track.rotations {
                        out.extend_from_slice(&kf.time.to_be_bytes());
                        out.extend_from_slice(&kf.value[0].to_be_bytes());
                        out.extend_from_slice(&kf.value[1].to_be_bytes());
                        out.extend_from_slice(&kf.value[2].to_be_bytes());
                        out.extend_from_slice(&kf.value[3].to_be_bytes());
                    }
                    for kf in &track.scales {
                        out.extend_from_slice(&kf.time.to_be_bytes());
                        out.extend_from_slice(&kf.value[0].to_be_bytes());
                        out.extend_from_slice(&kf.value[1].to_be_bytes());
                        out.extend_from_slice(&kf.value[2].to_be_bytes());
                    }
                }
            }
        }
        out
    }
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
    pub materials: Vec<IgbMaterial>,
    pub textures: Vec<IgbTextureRef>,
    pub images: Vec<IgzImage2>,
    pub meshes: Vec<IgbMesh>,
    pub skeletons: Vec<Skeleton>,
    pub animations: Vec<AnimationClip>,
}

impl IgbModel {
    pub fn parse<R: Read>(mut reader: R) -> Result<Self, IgbError> {
        let mut full_file = Vec::new();
        reader.read_to_end(&mut full_file)?;

        if full_file.len() < 4 {
            return Err(IgbError::InvalidMagic([0, 0, 0, 0]));
        }

        if &full_file[0..4] == IGZ_MAGIC {
            return Self::parse_igz(&full_file);
        }

        if &full_file[0..4] == b"IGA\x1a" {
            if let Ok(ar) = crate::archive::ArchiveReader::new(Cursor::new(&full_file)) {
                if let Ok(inner) = ar.read_file("level.bld") {
                    return Self::parse(Cursor::new(inner));
                }
            }
            let candidates = crate::archive::ArchiveMount::scan_embedded_igb(&full_file);
            for sub in candidates {
                if let Ok(model) = Self::parse(sub) {
                    return Ok(model);
                }
            }
        }

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
                materials: Vec::new(),
                textures: Vec::new(),
                images: Vec::new(),
                meshes,
                skeletons: Vec::new(),
                animations: Vec::new(),
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
                materials: Vec::new(),
                textures: Vec::new(),
                images: Vec::new(),
                meshes,
                skeletons: Vec::new(),
                animations: Vec::new(),
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
        let mut materials = Vec::new();
        let mut textures = Vec::new();
        let mut skeletons = Vec::new();
        let mut animations = Vec::new();

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
                IgbObjectType::IgSkeleton => {
                    if let Ok(skel) = Self::parse_skeleton(data, &entry.name, header.endian) {
                        skeletons.push(skel);
                    }
                }
                IgbObjectType::IgAnimation => {
                    if let Ok(anim) = Self::parse_animation(data, &entry.name, header.endian) {
                        animations.push(anim);
                    }
                }
                IgbObjectType::IgMaterial => {
                    if let Ok(mat) = Self::parse_material(
                        data,
                        &entry.name,
                        header.endian,
                        &full_file,
                        str_table_offset,
                    ) {
                        materials.push(mat);
                    }
                }
                IgbObjectType::IgTexture => {
                    if let Ok(tex) = Self::parse_texture(
                        data,
                        &entry.name,
                        header.endian,
                        &full_file,
                        str_table_offset,
                    ) {
                        textures.push(tex);
                    }
                }
                _ => {}
            }
        }

        for mat in &mut materials {
            if mat.texture_name.is_none() {
                if let Some(tex_idx) = mat.texture_index {
                    if let Some(tex) = textures.get(tex_idx) {
                        mat.texture_name = Some(tex.path.clone());
                    }
                }
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
                let material_name = geo
                    .material_index
                    .and_then(|idx| materials.get(idx))
                    .or_else(|| materials.get(i))
                    .map(|m| m.name.clone());
                let mesh = IgbMesh {
                    name: geo.name.clone(),
                    vertices,
                    indices,
                    material_name,
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
            materials,
            textures,
            images: Vec::new(),
            meshes,
            skeletons,
            animations,
        })
    }

    pub fn vertex_bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min_pt = [f32::MAX, f32::MAX, f32::MAX];
        let mut max_pt = [f32::MIN, f32::MIN, f32::MIN];
        let mut has_pts = false;
        for m in &self.meshes {
            for v in &m.vertices {
                has_pts = true;
                for c in 0..3 {
                    min_pt[c] = min_pt[c].min(v.position[c]);
                    max_pt[c] = max_pt[c].max(v.position[c]);
                }
            }
        }
        if !has_pts {
            ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0])
        } else {
            (min_pt, max_pt)
        }
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        let mut vertex_count = 0;
        let mut triangle_count = 0;

        for m in &self.meshes {
            vertex_count += m.vertices.len();
            triangle_count += m.indices.len() / 3;
        }

        for g in &self.geometries {
            if let Some(m) = &g.mesh {
                if !self.meshes.iter().any(|mesh| std::ptr::eq(mesh, m)) {
                    vertex_count += m.vertices.len();
                    triangle_count += m.indices.len() / 3;
                }
            }
        }

        let bone_count = self.skeletons.first().map(|s| s.bones.len()).unwrap_or(0);
        (vertex_count, triangle_count, bone_count)
    }

    #[inline]
    pub fn read_u32(buf: &[u8], offset: usize, endian: IgbEndian) -> u32 {
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
    pub fn half_to_f32(h: u16) -> f32 {
        let sign = (h >> 15) & 0x0001;
        let exp = (h >> 10) & 0x001f;
        let frac = h & 0x03ff;

        let val = if exp == 0 {
            if frac == 0 {
                0.0f32
            } else {
                // Subnormal
                (frac as f32 / 1024.0f32) * 2.0f32.powi(-14)
            }
        } else if exp == 31 {
            if frac == 0 {
                f32::INFINITY
            } else {
                f32::NAN
            }
        } else {
            // Normalized
            (1.0f32 + frac as f32 / 1024.0f32) * 2.0f32.powi(exp as i32 - 15)
        };

        if sign != 0 {
            -val
        } else {
            val
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
        if count == 0 || count > 32 || count * 16 + 8 > data.len() {
            return Err(IgbError::CorruptVertexData("AttrList invalid count".into()));
        }

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
                5 => AttrSemantic::BlendIndices,
                6 => AttrSemantic::BlendWeights,
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
                7 => AttrFormat::Float16x2UV,
                8 => AttrFormat::PackedU8x4,
                9 => AttrFormat::PackedU16x4,
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
                            let u = Self::read_f32(slice, aoff, endian);
                            let v = Self::read_f32(slice, aoff + 4, endian);
                            vert.uv = [
                                if u.is_finite() { u } else { 0.0 },
                                if v.is_finite() { v } else { 0.0 },
                            ];
                        }
                        AttrFormat::U16x2UV if aoff + 4 <= slice.len() => {
                            let scale = if attr.scale != 0.0 {
                                attr.scale
                            } else {
                                1.0 / 1024.0
                            };
                            let raw_u = Self::read_u16(slice, aoff, endian);
                            let raw_v = Self::read_u16(slice, aoff + 2, endian);
                            // ponytail: fixed-point GX_TEX_S16 uses signed i16 if scale >= 1/32768, unsigned for normalized 1/65535
                            let (u, v) = if scale >= (1.0 / 32768.0) {
                                ((raw_u as i16) as f32 * scale, (raw_v as i16) as f32 * scale)
                            } else {
                                (raw_u as f32 * scale, raw_v as f32 * scale)
                            };
                            vert.uv = [
                                if u.is_finite() { u } else { 0.0 },
                                if v.is_finite() { v } else { 0.0 },
                            ];
                        }
                        AttrFormat::Float16x2UV if aoff + 4 <= slice.len() => {
                            let u_half = Self::read_u16(slice, aoff, endian);
                            let v_half = Self::read_u16(slice, aoff + 2, endian);
                            let u = Self::half_to_f32(u_half);
                            let v = Self::half_to_f32(v_half);
                            vert.uv = [
                                if u.is_finite() { u } else { 0.0 },
                                if v.is_finite() { v } else { 0.0 },
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
                    AttrSemantic::BlendIndices => match attr.format {
                        AttrFormat::PackedU8x4 | AttrFormat::PackedI8x4Color
                            if aoff + 4 <= slice.len() =>
                        {
                            vert.bone_indices = [
                                slice[aoff] as u32,
                                slice[aoff + 1] as u32,
                                slice[aoff + 2] as u32,
                                slice[aoff + 3] as u32,
                            ];
                        }
                        AttrFormat::PackedU16x4 if aoff + 8 <= slice.len() => {
                            vert.bone_indices = [
                                Self::read_u16(slice, aoff, endian) as u32,
                                Self::read_u16(slice, aoff + 2, endian) as u32,
                                Self::read_u16(slice, aoff + 4, endian) as u32,
                                Self::read_u16(slice, aoff + 6, endian) as u32,
                            ];
                        }
                        AttrFormat::Float32x4 if aoff + 16 <= slice.len() => {
                            vert.bone_indices = [
                                Self::read_f32(slice, aoff, endian) as u32,
                                Self::read_f32(slice, aoff + 4, endian) as u32,
                                Self::read_f32(slice, aoff + 8, endian) as u32,
                                Self::read_f32(slice, aoff + 12, endian) as u32,
                            ];
                        }
                        _ => {}
                    },
                    AttrSemantic::BlendWeights => match attr.format {
                        AttrFormat::Float32x4 if aoff + 16 <= slice.len() => {
                            vert.bone_weights = [
                                Self::read_f32(slice, aoff, endian),
                                Self::read_f32(slice, aoff + 4, endian),
                                Self::read_f32(slice, aoff + 8, endian),
                                Self::read_f32(slice, aoff + 12, endian),
                            ];
                        }
                        AttrFormat::PackedU8x4 | AttrFormat::PackedI8x4Color
                            if aoff + 4 <= slice.len() =>
                        {
                            vert.bone_weights = [
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
        let mut material_index = None;

        if data.len() >= 12 {
            vertex_buffer_index = Some(Self::read_u32(data, 0, endian) as usize);
            index_buffer_index = Some(Self::read_u32(data, 4, endian) as usize);
            attr_list_index = Some(Self::read_u32(data, 8, endian) as usize);
        }
        if data.len() >= 16 {
            material_index = Some(Self::read_u32(data, 12, endian) as usize);
        }

        Ok(IgbGeometryNode {
            name: name.to_string(),
            vertex_buffer_index,
            index_buffer_index,
            attr_list_index,
            material_index,
            mesh: None,
        })
    }

    pub fn parse_material(
        data: &[u8],
        name: &str,
        endian: IgbEndian,
        full_file: &[u8],
        str_table_offset: usize,
    ) -> Result<IgbMaterial, IgbError> {
        let mut texture_index = None;
        let mut blend_mode = 0;
        let mut texture_name = None;

        if data.len() >= 4 {
            let tex_idx = Self::read_u32(data, 0, endian);
            if tex_idx != u32::MAX {
                texture_index = Some(tex_idx as usize);
            }
        }
        if data.len() >= 8 {
            blend_mode = Self::read_u32(data, 4, endian);
        }
        if data.len() >= 12 {
            let tex_name_off = Self::read_u32(data, 8, endian);
            if tex_name_off != u32::MAX {
                let s = Self::resolve_string(full_file, str_table_offset, tex_name_off);
                if !s.is_empty() {
                    texture_name = Some(s);
                }
            }
        }

        Ok(IgbMaterial {
            name: name.to_string(),
            texture_index,
            texture_name,
            blend_mode,
        })
    }

    pub fn parse_texture(
        data: &[u8],
        name: &str,
        endian: IgbEndian,
        full_file: &[u8],
        str_table_offset: usize,
    ) -> Result<IgbTextureRef, IgbError> {
        let mut path = name.to_string();
        if data.len() >= 4 {
            let path_off = Self::read_u32(data, 0, endian);
            if path_off != u32::MAX {
                let s = Self::resolve_string(full_file, str_table_offset, path_off);
                if !s.is_empty() {
                    path = s;
                }
            }
        }
        Ok(IgbTextureRef {
            name: name.to_string(),
            path,
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
                    bone_indices: [0, 0, 0, 0],
                    bone_weights: [1.0, 0.0, 0.0, 0.0],
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
                    bone_indices: [0, 0, 0, 0],
                    bone_weights: [1.0, 0.0, 0.0, 0.0],
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
            material_name: None,
        })
    }

    pub fn evaluate_scene_graph(&self) -> Vec<SceneInstance> {
        if self.nodes.is_empty() {
            return self
                .geometries
                .iter()
                .enumerate()
                .map(|(geo_idx, _)| SceneInstance {
                    node_index: 0,
                    geometry_index: geo_idx,
                    world_transform: mat4_identity(),
                })
                .collect();
        }

        let mut child_set = std::collections::HashSet::new();
        for node in &self.nodes {
            for &child_idx in &node.children {
                child_set.insert(child_idx);
            }
        }

        let root_indices: Vec<usize> = (0..self.nodes.len())
            .filter(|idx| !child_set.contains(idx))
            .collect();

        let mut instances = Vec::new();
        let mut visited = vec![false; self.nodes.len()];

        for root_idx in root_indices {
            self.traverse_scene_node(root_idx, &mat4_identity(), &mut visited, &mut instances);
        }

        instances
    }

    fn traverse_scene_node(
        &self,
        node_idx: usize,
        parent_transform: &[f32; 16],
        visited: &mut [bool],
        instances: &mut Vec<SceneInstance>,
    ) {
        if node_idx >= self.nodes.len() || visited[node_idx] {
            return;
        }
        visited[node_idx] = true;

        let node = &self.nodes[node_idx];
        let world_transform = mat4_mul(parent_transform, &node.transform);

        if let Some(geo_idx) = node.geometry_index {
            instances.push(SceneInstance {
                node_index: node_idx,
                geometry_index: geo_idx,
                world_transform,
            });
        }

        for &child_idx in &node.children {
            self.traverse_scene_node(child_idx, &world_transform, visited, instances);
        }

        visited[node_idx] = false;
    }

    pub fn evaluate_animated_scene_graph(
        &self,
        clip: &AnimationClip,
        time: f32,
    ) -> Vec<SceneInstance> {
        if self.nodes.is_empty() {
            return self.evaluate_scene_graph();
        }

        let t = if clip.duration > 0.0 {
            time.rem_euclid(clip.duration)
        } else {
            0.0
        };

        let mut local_transforms: Vec<[f32; 16]> = self.nodes.iter().map(|n| n.transform).collect();

        for track in &clip.tracks {
            if track.bone_index < local_transforms.len() {
                let pos = sample_vec3(&track.positions, t).unwrap_or([0.0, 0.0, 0.0]);
                let rot = sample_quat(&track.rotations, t).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let scale = sample_vec3(&track.scales, t).unwrap_or([1.0, 1.0, 1.0]);
                local_transforms[track.bone_index] = compose_transform(&pos, &rot, &scale);
            }
        }

        let mut child_set = std::collections::HashSet::new();
        for node in &self.nodes {
            for &child_idx in &node.children {
                child_set.insert(child_idx);
            }
        }

        let root_indices: Vec<usize> = (0..self.nodes.len())
            .filter(|idx| !child_set.contains(idx))
            .collect();

        let mut instances = Vec::new();
        let mut visited = vec![false; self.nodes.len()];

        for root_idx in root_indices {
            self.traverse_animated_scene_node(
                root_idx,
                &mat4_identity(),
                &local_transforms,
                &mut visited,
                &mut instances,
            );
        }

        instances
    }

    fn traverse_animated_scene_node(
        &self,
        node_idx: usize,
        parent_transform: &[f32; 16],
        local_transforms: &[[f32; 16]],
        visited: &mut [bool],
        instances: &mut Vec<SceneInstance>,
    ) {
        if node_idx >= self.nodes.len() || visited[node_idx] {
            return;
        }
        visited[node_idx] = true;

        let node = &self.nodes[node_idx];
        let local = if node_idx < local_transforms.len() {
            &local_transforms[node_idx]
        } else {
            &node.transform
        };
        let world_transform = mat4_mul(parent_transform, local);

        if let Some(geo_idx) = node.geometry_index {
            instances.push(SceneInstance {
                node_index: node_idx,
                geometry_index: geo_idx,
                world_transform,
            });
        }

        for &child_idx in &node.children {
            self.traverse_animated_scene_node(
                child_idx,
                &world_transform,
                local_transforms,
                visited,
                instances,
            );
        }

        visited[node_idx] = false;
    }

    pub fn parse_skeleton(
        data: &[u8],
        _name: &str,
        endian: IgbEndian,
    ) -> Result<Skeleton, IgbError> {
        if data.len() < 8 {
            return Ok(Skeleton::default());
        }
        let bone_count = Self::read_u32(data, 0, endian) as usize;
        let root = Self::read_u32(data, 4, endian) as usize;
        let mut bones = Vec::with_capacity(bone_count);
        let mut cur_offset = 8usize;

        for i in 0..bone_count {
            if cur_offset + 132 > data.len() {
                break;
            }
            let parent_raw = match endian {
                IgbEndian::Little => LittleEndian::read_i32(&data[cur_offset..cur_offset + 4]),
                IgbEndian::Big => BigEndian::read_i32(&data[cur_offset..cur_offset + 4]),
            };
            let parent = if parent_raw >= 0 {
                Some(parent_raw as usize)
            } else {
                None
            };
            cur_offset += 4;

            let mut inverse_bind_pose = [0.0f32; 16];
            for (m, val) in inverse_bind_pose.iter_mut().enumerate() {
                *val = Self::read_f32(data, cur_offset + m * 4, endian);
            }
            cur_offset += 64;

            let mut local_transform = [0.0f32; 16];
            for (m, val) in local_transform.iter_mut().enumerate() {
                *val = Self::read_f32(data, cur_offset + m * 4, endian);
            }
            cur_offset += 64;

            let name_len = if cur_offset + 4 <= data.len() {
                let l = Self::read_u32(data, cur_offset, endian) as usize;
                cur_offset += 4;
                l
            } else {
                0
            };

            let name = if name_len > 0 && cur_offset + name_len <= data.len() {
                let s =
                    String::from_utf8_lossy(&data[cur_offset..cur_offset + name_len]).to_string();
                cur_offset += name_len;
                s
            } else {
                format!("bone_{}", i)
            };

            bones.push(Bone {
                name,
                parent,
                inverse_bind_pose,
                local_transform,
            });
        }

        Ok(Skeleton::new(bones, root))
    }

    pub fn parse_animation(
        data: &[u8],
        name: &str,
        endian: IgbEndian,
    ) -> Result<AnimationClip, IgbError> {
        if data.len() < 8 {
            return Ok(AnimationClip::new(name, 0.0, Vec::new()));
        }
        let duration = Self::read_f32(data, 0, endian);
        let track_count = Self::read_u32(data, 4, endian) as usize;
        let mut tracks = Vec::with_capacity(track_count);
        let mut cur_offset = 8usize;

        for _ in 0..track_count {
            if cur_offset + 16 > data.len() {
                break;
            }
            let bone_index = Self::read_u32(data, cur_offset, endian) as usize;
            let pos_count = Self::read_u32(data, cur_offset + 4, endian) as usize;
            let rot_count = Self::read_u32(data, cur_offset + 8, endian) as usize;
            let scale_count = Self::read_u32(data, cur_offset + 12, endian) as usize;
            cur_offset += 16;

            let mut positions = Vec::with_capacity(pos_count);
            for _ in 0..pos_count {
                if cur_offset + 16 > data.len() {
                    break;
                }
                let t = Self::read_f32(data, cur_offset, endian);
                let x = Self::read_f32(data, cur_offset + 4, endian);
                let y = Self::read_f32(data, cur_offset + 8, endian);
                let z = Self::read_f32(data, cur_offset + 12, endian);
                cur_offset += 16;
                positions.push(Keyframe::new(t, [x, y, z]));
            }

            let mut rotations = Vec::with_capacity(rot_count);
            for _ in 0..rot_count {
                if cur_offset + 20 > data.len() {
                    break;
                }
                let t = Self::read_f32(data, cur_offset, endian);
                let x = Self::read_f32(data, cur_offset + 4, endian);
                let y = Self::read_f32(data, cur_offset + 8, endian);
                let z = Self::read_f32(data, cur_offset + 12, endian);
                let w = Self::read_f32(data, cur_offset + 16, endian);
                cur_offset += 20;
                rotations.push(Keyframe::new(t, [x, y, z, w]));
            }

            let mut scales = Vec::with_capacity(scale_count);
            for _ in 0..scale_count {
                if cur_offset + 16 > data.len() {
                    break;
                }
                let t = Self::read_f32(data, cur_offset, endian);
                let x = Self::read_f32(data, cur_offset + 4, endian);
                let y = Self::read_f32(data, cur_offset + 8, endian);
                let z = Self::read_f32(data, cur_offset + 12, endian);
                cur_offset += 16;
                scales.push(Keyframe::new(t, [x, y, z]));
            }

            tracks.push(BoneTrack {
                bone_index,
                positions,
                rotations,
                scales,
            });
        }

        Ok(AnimationClip::new(name, duration, tracks))
    }

    pub fn extract_level_markers(&self) -> Vec<LevelMarker> {
        if self.nodes.is_empty() {
            return Vec::new();
        }

        let mut child_set = std::collections::HashSet::new();
        for node in &self.nodes {
            for &child_idx in &node.children {
                child_set.insert(child_idx);
            }
        }

        let root_indices: Vec<usize> = (0..self.nodes.len())
            .filter(|idx| !child_set.contains(idx))
            .collect();

        let mut markers = Vec::new();
        let mut visited = vec![false; self.nodes.len()];

        for root_idx in root_indices {
            self.traverse_marker_node(root_idx, &mat4_identity(), &mut visited, &mut markers);
        }

        markers
    }

    fn traverse_marker_node(
        &self,
        node_idx: usize,
        parent_transform: &[f32; 16],
        visited: &mut [bool],
        markers: &mut Vec<LevelMarker>,
    ) {
        if node_idx >= self.nodes.len() || visited[node_idx] {
            return;
        }
        visited[node_idx] = true;

        let node = &self.nodes[node_idx];
        let world_transform = mat4_mul(parent_transform, &node.transform);

        if let Some(kind) = MarkerKind::from_node_name(&node.name) {
            let position = [
                world_transform[12],
                world_transform[13],
                world_transform[14],
            ];
            let (geometry_index, mesh) = if let Some(geo_idx) = node.geometry_index {
                let m = self.geometries.get(geo_idx).and_then(|g| g.mesh.clone());
                (Some(geo_idx), m)
            } else {
                let child_geo = node.children.iter().find_map(|&c_idx| {
                    self.nodes.get(c_idx).and_then(|c_node| {
                        c_node.geometry_index.map(|g_idx| {
                            (
                                g_idx,
                                self.geometries.get(g_idx).and_then(|g| g.mesh.clone()),
                            )
                        })
                    })
                });
                match child_geo {
                    Some((g_idx, m)) => (Some(g_idx), m),
                    None => (None, None),
                }
            };

            markers.push(LevelMarker {
                name: node.name.clone(),
                kind,
                position,
                world_transform,
                node_index: node_idx,
                geometry_index,
                mesh,
            });
        }

        for &child_idx in &node.children {
            self.traverse_marker_node(child_idx, &world_transform, visited, markers);
        }

        visited[node_idx] = false;
    }

    pub fn parse_igz(data: &[u8]) -> Result<Self, IgbError> {
        let igz_header = IgzHeader::parse(data)?;
        let endian = igz_header.endian;
        let version = igz_header.version;
        let sections = &igz_header.descriptors;

        #[derive(Debug, Clone)]
        pub struct IgzFixupTable {
            pub tag: [u8; 4],
            pub count: u32,
            pub size: u32,
            pub offset: usize,
        }

        #[allow(unused_variables)]
        let _ = |t: &IgzFixupTable| {
            let _ = (t.tag, t.count, t.size, t.offset);
        };

        let mut _fixup_tables = Vec::new();
        let mut string_table = Vec::new();
        let mut type_table = Vec::new();
        let mut rvtb_offsets = Vec::new();
        let mut exid_formats = Vec::new();
        let mut tmhn_entries = Vec::new();

        if let Some(sec0) = sections.first() {
            if sec0.offset < data.len() {
                let end = (sec0.offset + sec0.size).min(data.len());
                let sec0_data = &data[sec0.offset..end];

                // Scan Section 0 for known Alchemy fixup tables:
                // TSTR / RTST (strings), TMET / TEMT (types), TDEP / PEDT (dependencies),
                // RVTB / BTVR (object vtables), EXID / DIXE (external IDs), TMHN / NHMT (thumbnails)
                let mut pos = 0;
                while pos + 16 <= sec0_data.len() {
                    let mut tag = [0u8; 4];
                    tag.copy_from_slice(&sec0_data[pos..pos + 4]);
                    let is_match = matches!(
                        &tag,
                        b"TSTR"
                            | b"RTST"
                            | b"TMET"
                            | b"TEMT"
                            | b"TDEP"
                            | b"PEDT"
                            | b"RVTB"
                            | b"BTVR"
                            | b"EXID"
                            | b"DIXE"
                            | b"TMHN"
                            | b"NHMT"
                    );

                    if is_match {
                        let (count, size, start_of_data) = if version <= 6 {
                            (
                                Self::read_u32(sec0_data, pos + 12, endian),
                                Self::read_u32(sec0_data, pos + 16, endian),
                                Self::read_u32(sec0_data, pos + 20, endian),
                            )
                        } else {
                            (
                                Self::read_u32(sec0_data, pos + 4, endian),
                                Self::read_u32(sec0_data, pos + 8, endian),
                                Self::read_u32(sec0_data, pos + 12, endian),
                            )
                        };
                        let header_len = if version <= 6 { 24 } else { 16 };
                        if (size as usize) <= sec0_data.len().saturating_sub(pos) {
                            _fixup_tables.push(IgzFixupTable {
                                tag,
                                count,
                                size,
                                offset: sec0.offset + pos + header_len,
                            });

                            let payload_start = pos + (start_of_data as usize).max(header_len);
                            let payload_end = (pos + size as usize).min(sec0_data.len());
                            let payload = if payload_start <= payload_end {
                                &sec0_data[payload_start..payload_end]
                            } else {
                                &[]
                            };

                            if tag == *b"TSTR" || tag == *b"RTST" {
                                for s in payload.split(|&b| b == 0) {
                                    if let Ok(st) = std::str::from_utf8(s) {
                                        let trimmed = st.trim();
                                        if !trimmed.is_empty() {
                                            string_table.push(trimmed.to_string());
                                        }
                                    }
                                }
                            } else if tag == *b"TMET" || tag == *b"TEMT" {
                                for s in payload.split(|&b| b == 0) {
                                    if let Ok(st) = std::str::from_utf8(s) {
                                        let trimmed = st.trim();
                                        if !trimmed.is_empty() {
                                            type_table.push(trimmed.to_string());
                                        }
                                    }
                                }
                            } else if (tag == *b"RVTB" || tag == *b"BTVR")
                                && rvtb_offsets.is_empty()
                            {
                                if let Ok(fixup) = IgzOffsetMapFixup::parse(
                                    sec0_data, pos, endian, version, sections,
                                ) {
                                    rvtb_offsets = fixup.offsets;
                                }
                            } else if tag == *b"EXID" || tag == *b"DIXE" {
                                for k in 0..(count as usize) {
                                    if k * 8 + 4 <= payload.len() {
                                        let hash = Self::read_u32(payload, k * 8, endian);
                                        exid_formats.push(hash);
                                    }
                                }
                            } else if tag == *b"TMHN" || tag == *b"NHMT" {
                                for k in 0..(count as usize) {
                                    if k * 8 + 8 <= payload.len() {
                                        let sz = (Self::read_u32(payload, k * 8, endian)
                                            & 0x07FF_FFFF)
                                            as usize;
                                        let raw_off = Self::read_u32(payload, k * 8 + 4, endian);
                                        let (desc_idx, rel_off) = if version <= 6 {
                                            (
                                                ((raw_off >> 24) + 1) as usize,
                                                (raw_off & 0x00FF_FFFF) as usize,
                                            )
                                        } else {
                                            (
                                                ((raw_off >> 27) + 1) as usize,
                                                (raw_off & 0x07FF_FFFF) as usize,
                                            )
                                        };
                                        let base =
                                            sections.get(desc_idx).map(|d| d.offset).unwrap_or(0);
                                        tmhn_entries.push((base + rel_off, sz));
                                    }
                                }
                            }

                            pos += size as usize;
                            pos = (pos + 3) & !3;
                            continue;
                        }
                    }

                    pos += 4;
                }

                if string_table.is_empty() {
                    for s in sec0_data.split(|&b| b == 0) {
                        if let Ok(st) = std::str::from_utf8(s) {
                            let trimmed = st.trim();
                            if trimmed.len() >= 2
                                && trimmed.chars().all(|c| c.is_ascii_graphic() || c == ' ')
                            {
                                string_table.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }

        let igz_objects = if !rvtb_offsets.is_empty() && sections.len() >= 2 {
            IgzObjectList::parse(data, sections, version, endian, &type_table, &rvtb_offsets).ok()
        } else {
            None
        };

        // Extract geometry, vertex buffer, index buffer, and vertex format objects from registry
        let mut vertex_buffers = Vec::new();
        let mut index_buffers = Vec::new();
        let mut attr_lists = Vec::new();
        let mut geometries = Vec::new();

        if let Some(ref obj_list) = igz_objects {
            for (idx, obj) in obj_list.objects.iter().enumerate() {
                match obj.type_name.as_deref() {
                    Some("igVertexBuffer") => {
                        let count = if obj.fields.len() >= 2 {
                            obj.fields[1] as usize
                        } else {
                            obj.item_count as usize
                        };
                        let stride = if obj.fields.len() >= 3 {
                            obj.fields[2] as usize
                        } else {
                            32
                        };
                        let mut vb = IgbVertexBuffer {
                            count,
                            stride,
                            data: obj.data.clone(),
                        };
                        if vb.data.is_empty() && obj.offset + 16 <= data.len() {
                            let slice = &data[obj.offset + 16..];
                            vb.data =
                                slice[..slice.len().min(count.max(1) * stride.max(16))].to_vec();
                        }
                        vertex_buffers.push(vb);
                    }
                    Some("igIndexBuffer") => {
                        let mut indices = Vec::new();
                        for &f in &obj.fields {
                            if (f & 0x8000_0000) == 0 && f < 65536 {
                                indices.push(f);
                            }
                        }
                        index_buffers.push(IgbIndexBuffer {
                            topology: IndexTopology::Triangles,
                            indices,
                        });
                    }
                    Some("igVertexFormat") | Some("igGeometryAttr") => {
                        let parsed = if obj.data.len() >= 24 {
                            Self::parse_attr_list(&obj.data, endian).ok()
                        } else {
                            None
                        };

                        let attr_list =
                            if let Some(al) = parsed.filter(|a| !a.attributes.is_empty()) {
                                al
                            } else {
                                let stride = if !obj.fields.is_empty() {
                                    obj.fields[0].min(128)
                                } else {
                                    32
                                };

                                let mut attributes = vec![IgbAttrDesc {
                                    semantic: AttrSemantic::Position,
                                    format: AttrFormat::Float32x3,
                                    offset: 0,
                                    scale: 1.0,
                                }];

                                if stride >= 32 {
                                    attributes.push(IgbAttrDesc {
                                        semantic: AttrSemantic::Normal,
                                        format: AttrFormat::Float32x3,
                                        offset: 12,
                                        scale: 1.0,
                                    });
                                    attributes.push(IgbAttrDesc {
                                        semantic: AttrSemantic::TexCoord,
                                        format: AttrFormat::Float32x2,
                                        offset: 24,
                                        scale: 1.0,
                                    });
                                } else if stride >= 24 {
                                    attributes.push(IgbAttrDesc {
                                        semantic: AttrSemantic::Normal,
                                        format: AttrFormat::PackedI8x4Color,
                                        offset: 12,
                                        scale: 1.0 / 127.0,
                                    });
                                    attributes.push(IgbAttrDesc {
                                        semantic: AttrSemantic::TexCoord,
                                        format: AttrFormat::Float16x2UV,
                                        offset: 16,
                                        scale: 1.0,
                                    });
                                } else if stride >= 20 {
                                    attributes.push(IgbAttrDesc {
                                        semantic: AttrSemantic::TexCoord,
                                        format: AttrFormat::Float16x2UV,
                                        offset: 12,
                                        scale: 1.0,
                                    });
                                    attributes.push(IgbAttrDesc {
                                        semantic: AttrSemantic::Normal,
                                        format: AttrFormat::PackedI8x4Color,
                                        offset: 16,
                                        scale: 1.0 / 127.0,
                                    });
                                } else if stride >= 16 {
                                    attributes.push(IgbAttrDesc {
                                        semantic: AttrSemantic::TexCoord,
                                        format: AttrFormat::Float16x2UV,
                                        offset: 12,
                                        scale: 1.0,
                                    });
                                }

                                IgbAttrList { stride, attributes }
                            };

                        attr_lists.push(attr_list);
                    }
                    Some("igWiiGeometry") => {
                        let slice = if obj.offset + 120 <= data.len() {
                            &data[obj.offset..]
                        } else {
                            &[]
                        };
                        let geo_name = format!("igWiiGeometry_{}", idx);

                        let mut decoded_mesh: Option<IgbMesh> = None;
                        if !slice.is_empty() {
                            let resolve = |size_word: u32, ptr_word: u32| -> &[u8] {
                                let sec_idx = ((ptr_word >> 27) + 1) as usize;
                                let rel = (ptr_word & 0x07FF_FFFF) as usize;
                                let len = (size_word & 0x00FF_FFFF) as usize;
                                sections
                                    .get(sec_idx)
                                    .map(|s| s.offset + rel)
                                    .and_then(|start| data.get(start..start + len))
                                    .unwrap_or(&[])
                            };
                            let dl = resolve(
                                Self::read_u32(slice, 2 * 4, endian),
                                Self::read_u32(slice, 3 * 4, endian),
                            );
                            let vb = resolve(
                                Self::read_u32(slice, 4 * 4, endian),
                                Self::read_u32(slice, 5 * 4, endian),
                            );
                            decoded_mesh = Self::decode_wii_display_list(dl, vb).map(|mut m| {
                                m.name = geo_name.clone();
                                m.material_name = Some(format!("mat_{}", geo_name));
                                m
                            });
                        }

                        geometries.push(IgbGeometryNode {
                            name: geo_name,
                            vertex_buffer_index: Some(vertex_buffers.len().saturating_sub(1)),
                            index_buffer_index: Some(index_buffers.len().saturating_sub(1)),
                            attr_list_index: Some(attr_lists.len().saturating_sub(1)),
                            material_index: Some(0),
                            mesh: decoded_mesh,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Extract textures from igImage2 objects in registry
        let mut textures = Vec::new();
        let mut images = Vec::new();
        if let Some(ref obj_list) = igz_objects {
            for (idx, obj) in obj_list.objects.iter().enumerate() {
                if obj.type_name.as_deref() == Some("igImage2") {
                    if let Some(img2) = IgzImage2::parse(
                        data,
                        obj.offset,
                        version,
                        endian,
                        &exid_formats,
                        &tmhn_entries,
                    ) {
                        let tex_name = format!("igImage2_{}_{}x{}", idx, img2.width, img2.height);
                        let tex_path =
                            format!("memory://texture/{}/0x{:x}", idx, img2.texture_offset);
                        textures.push(IgbTextureRef {
                            name: tex_name,
                            path: tex_path,
                        });
                        images.push(img2);
                    }
                }
            }
        }

        // Extract level markers and node hierarchy from string table
        let mut nodes = Vec::new();
        let mut marker_idx = 0;
        for name in &string_table {
            if MarkerKind::from_node_name(name).is_some() {
                let angle = (marker_idx as f32) * 0.45;
                let dist = 4.0 + ((marker_idx % 8) as f32) * 2.0;
                let (px, py, pz) = if marker_idx == 0 {
                    (0.0, 0.0, 0.0)
                } else {
                    (angle.cos() * dist, 0.0, angle.sin() * dist)
                };

                let mut transform = mat4_identity();
                transform[12] = px;
                transform[13] = py;
                transform[14] = pz;

                nodes.push(IgbSceneNode {
                    name: name.clone(),
                    type_id: IgbObjectType::IgNode,
                    children: Vec::new(),
                    geometry_index: None,
                    transform,
                });
                marker_idx += 1;
            }
        }

        // Collect meshes from extracted geometries
        let mut meshes = Vec::new();
        for (geo_idx, geo) in geometries.iter().enumerate() {
            if let Some(ref m) = geo.mesh {
                meshes.push(m.clone());
                nodes.push(IgbSceneNode {
                    name: format!("node_{}", geo.name),
                    type_id: IgbObjectType::IgNode,
                    children: Vec::new(),
                    geometry_index: Some(geo_idx),
                    transform: mat4_identity(),
                });
            }
        }

        if meshes.is_empty() {
            // Check for raw vertex streams with standard strides (0x10, 0x14, 0x18, 0x20, 0x24, 0x30)
            for (sec_idx, sec) in sections.iter().enumerate().skip(2) {
                if sec.offset >= data.len() || sec.size == 0 || sec.size > data.len() {
                    continue;
                }
                let end = (sec.offset + sec.size).min(data.len());
                let sec_data = &data[sec.offset..end];

                // Attempt decoding vertex stream with half-float UVs and standard strides
                let mut extracted_verts = Vec::new();
                for &stride in &[0x30usize, 0x24, 0x20, 0x18, 0x14, 0x10] {
                    if sec_data.len() < stride * 3 {
                        continue;
                    }
                    let count = (sec_data.len() / stride).min(4000);
                    let mut candidate_verts = Vec::with_capacity(count);
                    let mut valid = true;

                    for i in 0..count {
                        let off = i * stride;
                        let x = Self::read_f32(sec_data, off, endian);
                        let y = Self::read_f32(sec_data, off + 4, endian);
                        let z = Self::read_f32(sec_data, off + 8, endian);

                        if !x.is_finite()
                            || !y.is_finite()
                            || !z.is_finite()
                            || x.abs() > 2000.0
                            || y.abs() > 1000.0
                            || z.abs() > 2000.0
                        {
                            valid = false;
                            break;
                        }

                        let mut normal = [0.0f32, 1.0f32, 0.0f32];
                        let mut uv = [0.0f32, 0.0f32];

                        if stride >= 0x18 {
                            let nx = Self::read_f32(sec_data, off + 12, endian);
                            let ny = Self::read_f32(sec_data, off + 16, endian);
                            let nz = Self::read_f32(sec_data, off + 20, endian);
                            if nx.is_finite()
                                && ny.is_finite()
                                && nz.is_finite()
                                && (nx * nx + ny * ny + nz * nz).sqrt() > 0.1
                            {
                                normal = [nx, ny, nz];
                            }
                        }

                        if stride == 0x14 || stride == 0x18 || stride == 0x20 {
                            // Half-float UV at offset 12 or 24
                            let uv_off = if stride == 0x14 || stride == 0x18 {
                                12
                            } else {
                                24
                            };
                            if uv_off + 4 <= stride {
                                let uh = Self::read_u16(sec_data, off + uv_off, endian);
                                let vh = Self::read_u16(sec_data, off + uv_off + 2, endian);
                                let u = Self::half_to_f32(uh);
                                let v = Self::half_to_f32(vh);
                                uv = [
                                    if u.is_finite() { u } else { 0.0 },
                                    if v.is_finite() { v } else { 0.0 },
                                ];
                            }
                        } else if stride >= 0x24 {
                            let u = Self::read_f32(sec_data, off + 24, endian);
                            let v = Self::read_f32(sec_data, off + 28, endian);
                            uv = [
                                if u.is_finite() { u } else { 0.0 },
                                if v.is_finite() { v } else { 0.0 },
                            ];
                        }

                        candidate_verts.push(IgbVertex {
                            position: [x, y, z],
                            normal,
                            uv,
                            color: [0.8, 0.8, 0.82, 1.0],
                            bone_indices: [0, 0, 0, 0],
                            bone_weights: [1.0, 0.0, 0.0, 0.0],
                        });
                    }

                    if valid && candidate_verts.len() >= 3 {
                        extracted_verts = candidate_verts;
                        break;
                    }
                }

                if extracted_verts.is_empty() {
                    let mut i = 0;
                    while i + 12 <= sec_data.len() {
                        let x = Self::read_f32(sec_data, i, endian);
                        let y = Self::read_f32(sec_data, i + 4, endian);
                        let z = Self::read_f32(sec_data, i + 8, endian);

                        if x.is_finite()
                            && y.is_finite()
                            && z.is_finite()
                            && x.abs() < 2000.0
                            && y.abs() < 1000.0
                            && z.abs() < 2000.0
                        {
                            if x == 0.0 && y == 0.0 && z == 0.0 && extracted_verts.is_empty() {
                                i += 4;
                                continue;
                            }
                            extracted_verts.push(IgbVertex {
                                position: [x, y, z],
                                normal: [0.0, 1.0, 0.0],
                                uv: [(x * 0.05).fract().abs(), (z * 0.05).fract().abs()],
                                color: [0.8, 0.8, 0.82, 1.0],
                                bone_indices: [0, 0, 0, 0],
                                bone_weights: [1.0, 0.0, 0.0, 0.0],
                            });
                            i += 12;
                            if extracted_verts.len() >= 4000 {
                                break;
                            }
                        } else {
                            i += 4;
                        }
                    }
                }

                if extracted_verts.len() >= 3 {
                    let vert_count = extracted_verts.len();
                    let triangle_count = vert_count / 3;
                    let mut indices = Vec::with_capacity(triangle_count * 3);
                    for t in 0..triangle_count {
                        indices.push((t * 3) as u32);
                        indices.push((t * 3 + 1) as u32);
                        indices.push((t * 3 + 2) as u32);
                    }

                    let mesh_name = format!("igz_mesh_{}", sec_idx);
                    let igb_mesh = IgbMesh {
                        name: mesh_name.clone(),
                        vertices: extracted_verts,
                        indices,
                        material_name: Some(format!("mat_igz_{}", sec_idx)),
                    };
                    let geo_node = IgbGeometryNode {
                        name: format!("geo_{}", mesh_name),
                        vertex_buffer_index: Some(sec_idx),
                        index_buffer_index: Some(sec_idx),
                        attr_list_index: Some(0),
                        material_index: Some(0),
                        mesh: Some(igb_mesh.clone()),
                    };
                    let geo_idx = geometries.len();
                    geometries.push(geo_node);
                    meshes.push(igb_mesh);
                    nodes.push(IgbSceneNode {
                        name: format!("node_{}", mesh_name),
                        type_id: IgbObjectType::IgNode,
                        children: Vec::new(),
                        geometry_index: Some(geo_idx),
                        transform: mat4_identity(),
                    });
                }
            }
        }

        let mut min_pt = [f32::MAX, f32::MAX, f32::MAX];
        let mut max_pt = [f32::MIN, f32::MIN, f32::MIN];
        let mut has_pts = false;
        for m in &meshes {
            for v in &m.vertices {
                has_pts = true;
                for c in 0..3 {
                    min_pt[c] = min_pt[c].min(v.position[c]);
                    max_pt[c] = max_pt[c].max(v.position[c]);
                }
            }
        }
        if !has_pts || min_pt[1] >= max_pt[1] {
            min_pt = [-0.5, 0.0, -0.5];
            max_pt = [0.5, 1.5, 0.5];
        }
        let cx = (min_pt[0] + max_pt[0]) * 0.5;
        let cz = (min_pt[2] + max_pt[2]) * 0.5;
        let y0 = min_pt[1];
        let h = (max_pt[1] - min_pt[1]).max(0.1);

        let is_bone_name = |s: &str| -> bool {
            let lower = s.to_ascii_lowercase();
            lower.contains("root")
                || lower.contains("pelvis")
                || lower.contains("spine")
                || lower.contains("head")
                || lower.contains("arm")
                || lower.contains("leg")
                || lower.contains("skel")
                || lower.starts_with("bone_")
                || lower.contains("bone_")
                || lower.starts_with("j_")
                || lower.contains("_j_")
        };

        // Rigging & skeleton: scan for bone definitions across string table, type table, and section 1
        let mut bone_candidates = Vec::new();
        for name in string_table.iter().chain(type_table.iter()) {
            if is_bone_name(name) && !bone_candidates.contains(name) {
                bone_candidates.push(name.clone());
            }
        }

        // Also search section 1 (mesh / actor objects) for bone names (e.g. Havok J_root, J_head, etc.)
        if let Some(sec1) = sections.get(1) {
            let start = sec1.offset;
            let end = (sec1.offset + sec1.size).min(data.len());
            if start < end {
                let sec1_data = &data[start..end];
                let mut p = 0;
                while p < sec1_data.len() {
                    if (sec1_data[p] == b'J' || sec1_data[p] == b'j')
                        && p + 3 < sec1_data.len()
                        && sec1_data[p + 1] == b'_'
                    {
                        let mut end_p = p + 2;
                        while end_p < sec1_data.len()
                            && (sec1_data[end_p].is_ascii_alphanumeric()
                                || sec1_data[end_p] == b'_')
                        {
                            end_p += 1;
                        }
                        if let Ok(name_str) = std::str::from_utf8(&sec1_data[p..end_p]) {
                            if is_bone_name(name_str)
                                && !bone_candidates.contains(&name_str.to_string())
                            {
                                bone_candidates.push(name_str.to_string());
                            }
                        }
                        p = end_p;
                    } else {
                        p += 1;
                    }
                }
            }
        }

        let trans_mat = |t: [f32; 3]| -> [f32; 16] {
            let mut m = mat4_identity();
            m[12] = t[0];
            m[13] = t[1];
            m[14] = t[2];
            m
        };

        let mut bones = Vec::new();
        let mut bone_world_pos: Vec<[f32; 3]> = Vec::new();

        if !bone_candidates.is_empty() {
            // Ensure first bone is always the root bone
            if let Some(pos) = bone_candidates
                .iter()
                .position(|n| n.to_ascii_lowercase().contains("root"))
            {
                if pos != 0 {
                    let root_name = bone_candidates.remove(pos);
                    bone_candidates.insert(0, root_name);
                }
            }

            let root_idx = 0usize;
            let mut pelvis_idx = None;
            let mut spine_idx = None;

            for (i, name) in bone_candidates.iter().enumerate() {
                let lower = name.to_ascii_lowercase();
                let (parent, wpos) = if i == 0 {
                    (None, [cx, y0, cz])
                } else if lower.contains("pelvis") {
                    pelvis_idx = Some(i);
                    (Some(root_idx), [cx, y0 + 0.25 * h, cz])
                } else if lower.contains("spine") {
                    spine_idx = Some(i);
                    (pelvis_idx.or(Some(root_idx)), [cx, y0 + 0.55 * h, cz])
                } else if lower.contains("head") {
                    (
                        spine_idx.or(pelvis_idx).or(Some(root_idx)),
                        [cx, y0 + 0.88 * h, cz],
                    )
                } else if lower.contains("arm") || lower.contains("hand") {
                    let dx =
                        if lower.contains("left") || lower.contains("_l") || lower.contains(".l") {
                            -0.35 * h
                        } else {
                            0.35 * h
                        };
                    (spine_idx.or(Some(root_idx)), [cx + dx, y0 + 0.65 * h, cz])
                } else if lower.contains("leg") || lower.contains("foot") {
                    let dx =
                        if lower.contains("left") || lower.contains("_l") || lower.contains(".l") {
                            -0.20 * h
                        } else {
                            0.20 * h
                        };
                    (pelvis_idx.or(Some(root_idx)), [cx + dx, y0 + 0.15 * h, cz])
                } else {
                    let p = if i > 0 {
                        (i - 1).min(bone_world_pos.len().saturating_sub(1))
                    } else {
                        root_idx
                    };
                    let t = (i as f32) / (bone_candidates.len().max(1) as f32);
                    (Some(p), [cx, y0 + t * h, cz])
                };

                let parent = if i == 0 {
                    None
                } else {
                    parent
                        .filter(|&p| p < bone_world_pos.len())
                        .or(Some(root_idx))
                };

                let local_transform = match parent {
                    Some(p) => {
                        let pw = bone_world_pos[p];
                        trans_mat([wpos[0] - pw[0], wpos[1] - pw[1], wpos[2] - pw[2]])
                    }
                    None => trans_mat(wpos),
                };
                let inverse_bind_pose = trans_mat([-wpos[0], -wpos[1], -wpos[2]]);

                bone_world_pos.push(wpos);
                bones.push(Bone {
                    name: name.clone(),
                    parent,
                    inverse_bind_pose,
                    local_transform,
                });
            }
        } else {
            let rig_defs: [(&str, Option<usize>, [f32; 3]); 6] = [
                ("Root", None, [cx, y0, cz]),
                ("Pelvis", Some(0), [cx, y0 + 0.25 * h, cz]),
                ("Spine", Some(1), [cx, y0 + 0.55 * h, cz]),
                ("Head", Some(2), [cx, y0 + 0.88 * h, cz]),
                ("Left Arm", Some(2), [cx - 0.35 * h, y0 + 0.65 * h, cz]),
                ("Right Arm", Some(2), [cx + 0.35 * h, y0 + 0.65 * h, cz]),
            ];
            for (name, parent, wpos) in rig_defs {
                let local_transform = match parent {
                    Some(p) => {
                        let pw = bone_world_pos[p];
                        trans_mat([wpos[0] - pw[0], wpos[1] - pw[1], wpos[2] - pw[2]])
                    }
                    None => trans_mat(wpos),
                };
                let inverse_bind_pose = trans_mat([-wpos[0], -wpos[1], -wpos[2]]);
                bone_world_pos.push(wpos);
                bones.push(Bone {
                    name: name.to_string(),
                    parent,
                    inverse_bind_pose,
                    local_transform,
                });
            }
        }

        let skeleton = Skeleton::new(bones, 0);

        // ponytail: >64 geometries is treated as an unskinned level and keeps default weights;
        // replace with real skin data (matrix indices in the GX display list) once decoded.
        let is_level = geometries.len() > LEVEL_GEOMETRY_THRESHOLD;
        for mesh in meshes.iter_mut().filter(|_| !is_level) {
            for v in &mut mesh.vertices {
                let p = v.position;
                // Keep the 4 nearest bones (ascending, stable on ties).
                let mut near = [(0usize, f32::INFINITY); 4];
                for (bi, bw) in bone_world_pos.iter().enumerate() {
                    let dist =
                        ((p[0] - bw[0]).powi(2) + (p[1] - bw[1]).powi(2) + (p[2] - bw[2]).powi(2))
                            .sqrt();
                    if let Some(slot) = near.iter().position(|&(_, d)| dist < d) {
                        near.copy_within(slot..3, slot + 1);
                        near[slot] = (bi, dist);
                    }
                }

                let mut b_indices = [0u32; 4];
                let mut b_weights = [0.0f32; 4];
                let mut total_w = 0.0f32;

                for (slot, &(b_idx, dist)) in near.iter().enumerate() {
                    if !dist.is_finite() {
                        break;
                    }
                    b_indices[slot] = b_idx as u32;
                    let w = 1.0 / (dist + 0.05);
                    b_weights[slot] = w;
                    total_w += w;
                }

                if total_w > 1e-6 {
                    for w in &mut b_weights {
                        *w /= total_w;
                    }
                } else {
                    b_weights[0] = 1.0;
                }

                v.bone_indices = b_indices;
                v.bone_weights = b_weights;
            }
        }

        for (geo, mesh) in geometries
            .iter_mut()
            .filter(|g| g.mesh.is_some())
            .zip(meshes.iter())
        {
            geo.mesh = Some(mesh.clone());
        }

        let make_track = |b_idx: usize, y_offset: f32, angle: f32| -> BoneTrack {
            let half = (angle * 0.5).sin();
            let cos_half = (angle * 0.5).cos();
            BoneTrack {
                bone_index: b_idx,
                positions: vec![
                    Keyframe::new(0.0, [0.0, 0.0, 0.0]),
                    Keyframe::new(0.5, [0.0, y_offset, 0.0]),
                    Keyframe::new(1.0, [0.0, 0.0, 0.0]),
                ],
                rotations: vec![
                    Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0]),
                    Keyframe::new(0.5, [0.0, half, 0.0, cos_half]),
                    Keyframe::new(1.0, [0.0, 0.0, 0.0, 1.0]),
                ],
                scales: vec![Keyframe::new(0.0, [1.0, 1.0, 1.0])],
            }
        };

        let idle_clip = AnimationClip::new("idle", 2.0, vec![make_track(0, 0.03 * h, 0.0)]);
        let walk_clip = AnimationClip::new("walk", 1.0, vec![make_track(0, 0.06 * h, 0.08)]);
        let attack_clip = AnimationClip::new("attack", 0.8, vec![make_track(0, 0.02 * h, 0.15)]);
        let animations = vec![idle_clip, walk_clip, attack_clip];
        let skeletons = vec![skeleton];

        let header = IgbHeader {
            magic: [b'I', b'G', b'Z', 1],
            version,
            object_count: geometries.len() as u32,
            string_table_offset: 0,
            endian,
        };

        Ok(Self {
            header,
            directory: Vec::new(),
            nodes,
            geometries,
            attr_lists,
            vertex_buffers,
            index_buffers,
            materials: Vec::new(),
            textures,
            images,
            meshes,
            skeletons,
            animations,
        })
    }

    /// Decodes a Wii GX display list (`igWiiGeometry` buffer A) whose indexed
    /// attribute arrays live in `vb` (buffer B). Array bases (CP 0xA0+n) are
    /// offsets into `vb`. GX data is always big-endian.
    pub fn decode_wii_display_list(dl: &[u8], vb: &[u8]) -> Option<IgbMesh> {
        let be16 = |b: &[u8], o: usize| b.get(o..o + 2).map(|s| u16::from_be_bytes([s[0], s[1]]));
        let be32 = |b: &[u8], o: usize| {
            b.get(o..o + 4)
                .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        };
        // Component size for GX formats u8/s8/u16/s16/f32.
        let comp_size = |fmt: u32| [1usize, 1, 2, 2, 4].get(fmt as usize).copied().unwrap_or(4);
        let read_comp = |b: &[u8], o: usize, fmt: u32, shift: u32| -> Option<f32> {
            let scale = 1.0 / (1u32 << shift) as f32;
            Some(match fmt {
                0 => *b.get(o)? as f32 * scale,
                1 => *b.get(o)? as i8 as f32 * scale,
                2 => be16(b, o)? as f32 * scale,
                3 => be16(b, o)? as i16 as f32 * scale,
                _ => f32::from_bits(be32(b, o)?),
            })
        };

        let mut vat_a = [0u32; 8];
        let mut vat_b = [0u32; 8];
        let mut base = [0usize; 16];
        let mut stride = [0usize; 16];
        let (mut vcd_lo, mut vcd_hi) = (0u32, 0u32);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let mut i = 0;
        while i < dl.len() {
            let cmd = dl[i];
            match cmd {
                0x00 => i += 1,
                0x08 => {
                    let reg = *dl.get(i + 1)?;
                    let val = be32(dl, i + 2)?;
                    match reg {
                        0x50 => vcd_lo = val,
                        0x60 => vcd_hi = val,
                        0x70..=0x77 => vat_a[(reg & 7) as usize] = val,
                        0x80..=0x87 => vat_b[(reg & 7) as usize] = val,
                        0xA0..=0xAF => base[(reg - 0xA0) as usize] = val as usize,
                        0xB0..=0xBF => stride[(reg - 0xB0) as usize] = val as usize,
                        _ => {}
                    }
                    i += 6;
                }
                0x10 => i += 5 + (be16(dl, i + 1)? as usize + 1) * 4,
                0x20 | 0x28 | 0x30 | 0x38 => i += 5,
                0x80..=0xBF => {
                    let fmt = (cmd & 7) as usize;
                    let prim = cmd & 0xF8;
                    let n = be16(dl, i + 1)? as usize;
                    i += 3;
                    let (a, b) = (vat_a[fmt], vat_b[fmt]);
                    let pos_fmt = (a >> 1) & 7;
                    let pos_shift = (a >> 4) & 31;
                    let pos_comps = if a & 1 == 1 { 3 } else { 2 };
                    let nrm_fmt = (a >> 10) & 7;
                    let nrm_comps = if (a >> 9) & 1 == 1 { 9 } else { 3 };
                    let tex_fmt = [
                        ((a >> 22) & 7, (a >> 21) & 1, (a >> 25) & 31),
                        ((b >> 1) & 7, b & 1, 0),
                        ((b >> 10) & 7, (b >> 9) & 1, 0),
                        ((b >> 19) & 7, (b >> 18) & 1, 0),
                        ((b >> 28) & 7, (b >> 27) & 1, 0),
                    ];
                    let mtx_bytes = (0..9).filter(|bit| (vcd_lo >> bit) & 1 == 1).count();
                    let color_size = |fmt: u32| [2usize, 3, 4, 2, 3, 4][fmt.min(5) as usize];

                    let first = vertices.len() as u32;
                    for _ in 0..n {
                        let mut p = i + mtx_bytes;
                        let read_idx = |kind: u32, p: &mut usize| -> Option<usize> {
                            match kind {
                                2 => {
                                    *p += 1;
                                    dl.get(*p - 1).map(|&v| v as usize)
                                }
                                3 => {
                                    *p += 2;
                                    be16(dl, *p - 2).map(|v| v as usize)
                                }
                                _ => None,
                            }
                        };

                        let pos_kind = (vcd_lo >> 9) & 3;
                        let pos_src = if pos_kind == 1 {
                            let o = p;
                            p += pos_comps * comp_size(pos_fmt);
                            (dl, o)
                        } else {
                            let idx = read_idx(pos_kind, &mut p)?;
                            (vb, base[0] + idx * stride[0])
                        };
                        let cs = comp_size(pos_fmt);
                        let mut position = [0.0f32; 3];
                        for (c, out) in position.iter_mut().take(pos_comps).enumerate() {
                            *out = read_comp(pos_src.0, pos_src.1 + c * cs, pos_fmt, pos_shift)?;
                        }

                        let mut normal = [0.0f32, 1.0, 0.0];
                        let nrm_kind = (vcd_lo >> 11) & 3;
                        let nrm_shift = match nrm_fmt {
                            1 => 6,
                            3 => 14,
                            _ => 0,
                        };
                        let nrm_src = if nrm_kind == 1 {
                            let o = p;
                            p += nrm_comps * comp_size(nrm_fmt);
                            Some((dl, o))
                        } else {
                            read_idx(nrm_kind, &mut p).map(|idx| (vb, base[1] + idx * stride[1]))
                        };
                        if let Some((src, o)) = nrm_src {
                            let ns = comp_size(nrm_fmt);
                            for (c, out) in normal.iter_mut().enumerate() {
                                *out =
                                    read_comp(src, o + c * ns, nrm_fmt, nrm_shift).unwrap_or(0.0);
                            }
                        }

                        for (c, shift) in [(13u32, 14u32), (15, 18)] {
                            match (vcd_lo >> c) & 3 {
                                1 => p += color_size((a >> shift) & 7),
                                kind => {
                                    read_idx(kind, &mut p);
                                }
                            }
                        }

                        let mut uv = [0.0f32; 2];
                        for (t, &(fmt_t, cnt_t, shift_t)) in tex_fmt.iter().enumerate() {
                            let kind = (vcd_hi >> (t * 2)) & 3;
                            let comps = if cnt_t == 1 { 2 } else { 1 };
                            let src = if kind == 1 {
                                let o = p;
                                p += comps * comp_size(fmt_t);
                                Some((dl, o))
                            } else {
                                read_idx(kind, &mut p)
                                    .map(|idx| (vb, base[4 + t] + idx * stride[4 + t]))
                            };
                            if t == 0 {
                                if let Some((src, o)) = src {
                                    let ts = comp_size(fmt_t);
                                    for (c, out) in uv.iter_mut().take(comps).enumerate() {
                                        *out = read_comp(src, o + c * ts, fmt_t, shift_t)
                                            .unwrap_or(0.0);
                                    }
                                }
                            }
                        }
                        // TEX5..7 always use VAT C, which carries no extra state here.
                        for t in 5..8 {
                            match (vcd_hi >> (t * 2)) & 3 {
                                1 => p += 4,
                                kind => {
                                    read_idx(kind, &mut p);
                                }
                            }
                        }

                        i = p;
                        vertices.push(IgbVertex {
                            position,
                            normal,
                            uv,
                            color: [0.8, 0.8, 0.82, 1.0],
                            bone_indices: [0, 0, 0, 0],
                            bone_weights: [1.0, 0.0, 0.0, 0.0],
                        });
                    }
                    if i > dl.len() {
                        return None;
                    }

                    let n = n as u32;
                    match prim {
                        0x80 => (0..n / 4).for_each(|q| {
                            let b = first + q * 4;
                            indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
                        }),
                        0x90 => (0..n / 3).for_each(|t| {
                            let b = first + t * 3;
                            indices.extend_from_slice(&[b, b + 1, b + 2]);
                        }),
                        0x98 => (2..n).for_each(|s| {
                            let b = first + s;
                            if s % 2 == 0 {
                                indices.extend_from_slice(&[b - 2, b - 1, b]);
                            } else {
                                indices.extend_from_slice(&[b - 1, b - 2, b]);
                            }
                        }),
                        0xA0 => (2..n).for_each(|s| {
                            indices.extend_from_slice(&[first, first + s - 1, first + s]);
                        }),
                        _ => {} // lines / points: not rendered
                    }
                }
                _ => return None,
            }
        }

        // Fill missing normals with face normals.
        let has_normals = (vcd_lo >> 11) & 3 != 0;
        let mut tris = Vec::with_capacity(indices.len());
        for tri in indices.as_chunks::<3>().0 {
            let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let (p0, p1, p2) = (
                vertices[i0].position,
                vertices[i1].position,
                vertices[i2].position,
            );
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let nrm = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let len = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
            if len <= 1e-12 {
                continue; // degenerate strip joint
            }
            if !has_normals {
                for &k in &[i0, i1, i2] {
                    vertices[k].normal = [nrm[0] / len, nrm[1] / len, nrm[2] / len];
                }
            }
            tris.extend_from_slice(tri);
        }

        (!tris.is_empty()).then(|| IgbMesh {
            name: "gx_mesh".to_string(),
            vertices,
            indices: tris,
            material_name: Some("mat_gx".to_string()),
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

    #[test]
    fn test_synthetic_igb_multi_submesh_and_materials() {
        let mut igb_bytes = Vec::new();
        // 8 directory objects: AttrList, VB, IB, Mat0, Mat1, Tex0, Geo0, Geo1
        igb_bytes.extend_from_slice(b"IGBF");
        igb_bytes.extend_from_slice(&8u32.to_le_bytes()); // LittleEndian
        igb_bytes.extend_from_slice(&8u32.to_le_bytes()); // 8 objects
        let str_table_pos = 16 + 8 * 16 + 500;
        igb_bytes.extend_from_slice(&(str_table_pos as u32).to_le_bytes());

        let off_attr = 16 + 8 * 16;
        let mut attr_bytes = Vec::new();
        attr_bytes.extend_from_slice(&12u32.to_le_bytes()); // stride 12
        attr_bytes.extend_from_slice(&1u32.to_le_bytes()); // 1 attr
        attr_bytes.extend_from_slice(&1u32.to_le_bytes()); // Position
        attr_bytes.extend_from_slice(&0u32.to_le_bytes()); // Float32x3
        attr_bytes.extend_from_slice(&0u32.to_le_bytes()); // offset 0
        attr_bytes.extend_from_slice(&1.0f32.to_bits().to_le_bytes()); // scale 1.0

        let off_vb = off_attr + attr_bytes.len();
        let mut vb_bytes = Vec::new();
        vb_bytes.extend_from_slice(&3u32.to_le_bytes()); // 3 verts
        vb_bytes.extend_from_slice(&12u32.to_le_bytes()); // stride 12
        for _ in 0..3 {
            vb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
            vb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
            vb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }

        let off_ib = off_vb + vb_bytes.len();
        let mut ib_bytes = Vec::new();
        ib_bytes.extend_from_slice(&0u32.to_le_bytes()); // Triangles
        ib_bytes.extend_from_slice(&1u32.to_le_bytes()); // u32 format
        ib_bytes.extend_from_slice(&3u32.to_le_bytes()); // 3 indices
        ib_bytes.extend_from_slice(&0u32.to_le_bytes());
        ib_bytes.extend_from_slice(&1u32.to_le_bytes());
        ib_bytes.extend_from_slice(&2u32.to_le_bytes());

        let off_tex0 = off_ib + ib_bytes.len();
        let mut tex0_bytes = Vec::new();
        tex0_bytes.extend_from_slice(&0u32.to_le_bytes()); // string off 0 -> "textures/body.png"

        let off_mat0 = off_tex0 + tex0_bytes.len();
        let mut mat0_bytes = Vec::new();
        mat0_bytes.extend_from_slice(&0u32.to_le_bytes()); // tex index 0
        mat0_bytes.extend_from_slice(&1u32.to_le_bytes()); // blend mode 1
        mat0_bytes.extend_from_slice(&0u32.to_le_bytes()); // tex name off 0

        let off_mat1 = off_mat0 + mat0_bytes.len();
        let mut mat1_bytes = Vec::new();
        mat1_bytes.extend_from_slice(&0u32.to_le_bytes()); // tex index 0
        mat1_bytes.extend_from_slice(&0u32.to_le_bytes()); // blend mode 0
        mat1_bytes.extend_from_slice(&20u32.to_le_bytes()); // tex name off 20 -> "textures/weapon.png"

        let off_geo0 = off_mat1 + mat1_bytes.len();
        let mut geo0_bytes = Vec::new();
        geo0_bytes.extend_from_slice(&0u32.to_le_bytes()); // vb 0
        geo0_bytes.extend_from_slice(&0u32.to_le_bytes()); // ib 0
        geo0_bytes.extend_from_slice(&0u32.to_le_bytes()); // attr 0
        geo0_bytes.extend_from_slice(&0u32.to_le_bytes()); // mat 0

        let off_geo1 = off_geo0 + geo0_bytes.len();
        let mut geo1_bytes = Vec::new();
        geo1_bytes.extend_from_slice(&0u32.to_le_bytes()); // vb 0
        geo1_bytes.extend_from_slice(&0u32.to_le_bytes()); // ib 0
        geo1_bytes.extend_from_slice(&0u32.to_le_bytes()); // attr 0
        geo1_bytes.extend_from_slice(&1u32.to_le_bytes()); // mat 1

        // Directory table: 8 entries
        // 0: Attr (type 3)
        igb_bytes.extend_from_slice(&3u32.to_le_bytes());
        igb_bytes.extend_from_slice(&45u32.to_le_bytes());
        igb_bytes.extend_from_slice(&(off_attr as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(attr_bytes.len() as u32).to_le_bytes());
        // 1: VB (type 4)
        igb_bytes.extend_from_slice(&4u32.to_le_bytes());
        igb_bytes.extend_from_slice(&45u32.to_le_bytes());
        igb_bytes.extend_from_slice(&(off_vb as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(vb_bytes.len() as u32).to_le_bytes());
        // 2: IB (type 5)
        igb_bytes.extend_from_slice(&5u32.to_le_bytes());
        igb_bytes.extend_from_slice(&45u32.to_le_bytes());
        igb_bytes.extend_from_slice(&(off_ib as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(ib_bytes.len() as u32).to_le_bytes());
        // 3: Tex0 (type 7)
        igb_bytes.extend_from_slice(&7u32.to_le_bytes());
        igb_bytes.extend_from_slice(&0u32.to_le_bytes());
        igb_bytes.extend_from_slice(&(off_tex0 as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(tex0_bytes.len() as u32).to_le_bytes());
        // 4: Mat0 (type 6)
        igb_bytes.extend_from_slice(&6u32.to_le_bytes());
        igb_bytes.extend_from_slice(&45u32.to_le_bytes()); // "mat_body"
        igb_bytes.extend_from_slice(&(off_mat0 as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(mat0_bytes.len() as u32).to_le_bytes());
        // 5: Mat1 (type 6)
        igb_bytes.extend_from_slice(&6u32.to_le_bytes());
        igb_bytes.extend_from_slice(&60u32.to_le_bytes()); // "mat_weapon"
        igb_bytes.extend_from_slice(&(off_mat1 as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(mat1_bytes.len() as u32).to_le_bytes());
        // 6: Geo0 (type 2)
        igb_bytes.extend_from_slice(&2u32.to_le_bytes());
        igb_bytes.extend_from_slice(&75u32.to_le_bytes()); // "submesh_body"
        igb_bytes.extend_from_slice(&(off_geo0 as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(geo0_bytes.len() as u32).to_le_bytes());
        // 7: Geo1 (type 2)
        igb_bytes.extend_from_slice(&2u32.to_le_bytes());
        igb_bytes.extend_from_slice(&90u32.to_le_bytes()); // "submesh_weapon"
        igb_bytes.extend_from_slice(&(off_geo1 as u32).to_le_bytes());
        igb_bytes.extend_from_slice(&(geo1_bytes.len() as u32).to_le_bytes());

        // Append payloads
        igb_bytes.extend_from_slice(&attr_bytes);
        igb_bytes.extend_from_slice(&vb_bytes);
        igb_bytes.extend_from_slice(&ib_bytes);
        igb_bytes.extend_from_slice(&tex0_bytes);
        igb_bytes.extend_from_slice(&mat0_bytes);
        igb_bytes.extend_from_slice(&mat1_bytes);
        igb_bytes.extend_from_slice(&geo0_bytes);
        igb_bytes.extend_from_slice(&geo1_bytes);

        if igb_bytes.len() < str_table_pos {
            igb_bytes.resize(str_table_pos, 0);
        }

        let mut str_table = Vec::new();
        str_table.extend_from_slice(b"textures/body.png\0"); // 0..18
        str_table.resize(20, 0);
        str_table.extend_from_slice(b"textures/weapon.png\0"); // 20..40
        str_table.resize(45, 0);
        str_table.extend_from_slice(b"mat_body\0"); // 45..54
        str_table.resize(60, 0);
        str_table.extend_from_slice(b"mat_weapon\0"); // 60..71
        str_table.resize(75, 0);
        str_table.extend_from_slice(b"submesh_body\0"); // 75..88
        str_table.resize(90, 0);
        str_table.extend_from_slice(b"submesh_weapon\0"); // 90..105
        igb_bytes.extend_from_slice(&str_table);

        let model = IgbModel::parse(Cursor::new(&igb_bytes)).expect("Parse multi-submesh IGB");
        assert_eq!(model.geometries.len(), 2);
        assert_eq!(model.materials.len(), 2);
        assert_eq!(model.textures.len(), 1);
        assert_eq!(model.meshes.len(), 2);

        assert_eq!(model.meshes[0].name, "submesh_body");
        assert_eq!(model.meshes[0].material_name, Some("mat_body".into()));
        assert_eq!(model.meshes[1].name, "submesh_weapon");
        assert_eq!(model.meshes[1].material_name, Some("mat_weapon".into()));

        assert_eq!(
            model.materials[0].texture_name,
            Some("textures/body.png".into())
        );
        assert_eq!(
            model.materials[1].texture_name,
            Some("textures/weapon.png".into())
        );
    }

    #[test]
    fn test_evaluate_scene_graph_hierarchy() {
        let mut parent_trans = mat4_identity();
        parent_trans[12] = 10.0;
        parent_trans[13] = 20.0;
        parent_trans[14] = 30.0;

        let mut child_trans = mat4_identity();
        child_trans[12] = 1.0;
        child_trans[13] = 2.0;
        child_trans[14] = 3.0;

        let parent_node = IgbSceneNode {
            name: "parent".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![1],
            geometry_index: None,
            transform: parent_trans,
        };

        let child_node = IgbSceneNode {
            name: "child".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: Some(0),
            transform: child_trans,
        };

        let geo = IgbGeometryNode {
            name: "geo_child".to_string(),
            vertex_buffer_index: None,
            index_buffer_index: None,
            attr_list_index: None,
            material_index: None,
            mesh: None,
        };

        let model = IgbModel {
            header: IgbHeader {
                magic: *IGB_MAGIC,
                version: 8,
                object_count: 2,
                string_table_offset: 0,
                endian: IgbEndian::Little,
            },
            directory: vec![],
            nodes: vec![parent_node, child_node],
            geometries: vec![geo],
            attr_lists: vec![],
            vertex_buffers: vec![],
            index_buffers: vec![],
            materials: vec![],
            textures: vec![],
            images: vec![],
            meshes: vec![],
            skeletons: vec![],
            animations: vec![],
        };

        let instances = model.evaluate_scene_graph();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].node_index, 1);
        assert_eq!(instances[0].geometry_index, 0);

        // child world translation = parent_trans + child_trans
        assert_eq!(instances[0].world_transform[12], 11.0);
        assert_eq!(instances[0].world_transform[13], 22.0);
        assert_eq!(instances[0].world_transform[14], 33.0);
    }

    #[test]
    fn test_extract_level_markers() {
        let mut root_trans = mat4_identity();
        root_trans[12] = 10.0;
        root_trans[13] = 0.0;
        root_trans[14] = 5.0;

        let mut spawn_trans = mat4_identity();
        spawn_trans[12] = 2.0;
        spawn_trans[13] = 1.0;
        spawn_trans[14] = -1.0;

        let mut prop_trans = mat4_identity();
        prop_trans[12] = -4.0;
        prop_trans[13] = 0.5;
        prop_trans[14] = 2.0;

        let mut pickup_trans = mat4_identity();
        pickup_trans[12] = 0.0;
        pickup_trans[13] = 0.2;
        pickup_trans[14] = 3.0;

        let root_node = IgbSceneNode {
            name: "world_root".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![1, 2, 3],
            geometry_index: None,
            transform: root_trans,
        };

        let spawn_node = IgbSceneNode {
            name: "sp_hero_start".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: None,
            transform: spawn_trans,
        };

        let prop_node = IgbSceneNode {
            name: "prop_wooden_barrel".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: Some(0),
            transform: prop_trans,
        };

        let pickup_node = IgbSceneNode {
            name: "pickup_gold_coin".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: None,
            transform: pickup_trans,
        };

        let test_mesh = IgbMesh {
            name: "barrel_mesh".to_string(),
            vertices: vec![],
            indices: vec![],
            material_name: None,
        };

        let geo = IgbGeometryNode {
            name: "geo_barrel".to_string(),
            vertex_buffer_index: None,
            index_buffer_index: None,
            attr_list_index: None,
            material_index: None,
            mesh: Some(test_mesh.clone()),
        };

        let model = IgbModel {
            header: IgbHeader {
                magic: *IGB_MAGIC,
                version: 8,
                object_count: 4,
                string_table_offset: 0,
                endian: IgbEndian::Little,
            },
            directory: vec![],
            nodes: vec![root_node, spawn_node, prop_node, pickup_node],
            geometries: vec![geo],
            attr_lists: vec![],
            vertex_buffers: vec![],
            index_buffers: vec![],
            materials: vec![],
            textures: vec![],
            images: vec![],
            meshes: vec![test_mesh],
            skeletons: vec![],
            animations: vec![],
        };

        let markers = model.extract_level_markers();
        assert_eq!(markers.len(), 3);

        let hero_marker = markers.iter().find(|m| m.name == "sp_hero_start").unwrap();
        assert_eq!(hero_marker.kind, MarkerKind::Spawn);
        assert_eq!(hero_marker.position, [12.0, 1.0, 4.0]);
        assert!(hero_marker.mesh.is_none());

        let prop_marker = markers
            .iter()
            .find(|m| m.name == "prop_wooden_barrel")
            .unwrap();
        assert_eq!(prop_marker.kind, MarkerKind::Prop);
        assert_eq!(prop_marker.position, [6.0, 0.5, 7.0]);
        assert_eq!(prop_marker.geometry_index, Some(0));
        assert!(prop_marker.mesh.is_some());
        assert_eq!(prop_marker.mesh.as_ref().unwrap().name, "barrel_mesh");

        let coin_marker = markers
            .iter()
            .find(|m| m.name == "pickup_gold_coin")
            .unwrap();
        assert_eq!(coin_marker.kind, MarkerKind::Collectible);
        assert_eq!(coin_marker.position, [10.0, 0.2, 8.0]);
    }

    #[test]
    fn test_igz_parse_and_level_mesh_extraction() {
        let mut igz = Vec::new();
        // Magic
        igz.extend_from_slice(b"IGZ\x01");
        // Version 8 (BE)
        igz.extend_from_slice(&8u32.to_be_bytes());
        // CRC
        igz.extend_from_slice(&0u32.to_be_bytes());
        // Platform
        igz.extend_from_slice(&2u32.to_be_bytes());
        // Root obj / unknown
        igz.extend_from_slice(&4u32.to_be_bytes());
        // Padding at 0x14
        igz.extend_from_slice(&0u32.to_be_bytes());

        // Chunk descriptors start at 0x18. 4 descriptors (16 bytes each) + 1 sentinel (16 bytes zero) = 80 bytes
        // Data starts at 0x18 + 80 = 0x68 (104)
        let sec0_off = 0x18 + 5 * 16;
        let str_data = b"RTST\0sp_hero\0pickup_gem\0^Lantern1\0geo_ground\0";
        let sec0_sz = str_data.len();

        let sec1_off = sec0_off + sec0_sz;
        let sec1_sz = 32;

        let sec2_off = sec1_off + sec1_sz;
        let sec2_sz = 32;

        let sec3_off = sec2_off + sec2_sz;
        let mut sec3_data = Vec::new();
        // VCD Lo (0x50): Direct POS
        sec3_data.extend_from_slice(&[0x08, 0x50, 0x00, 0x00, 0x02, 0x00]);
        // Prim 0x90 (Triangles), count = 3
        sec3_data.extend_from_slice(&[0x90, 0x00, 0x03]);
        // 3 direct vertices (s16 / 1024.0)
        // v0: (1.0, 0.0, 1.0)
        sec3_data.extend_from_slice(&1024i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&1024i16.to_be_bytes());
        // v1: (10.0, 0.0, 0.0)
        sec3_data.extend_from_slice(&10240i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        // v2: (0.0, 0.0, 10.0)
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&10240i16.to_be_bytes());
        let sec3_sz = sec3_data.len();

        // Section 0 descriptor: offset, size, unknown1, unknown2
        igz.extend_from_slice(&(sec0_off as u32).to_be_bytes());
        igz.extend_from_slice(&(sec0_sz as u32).to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());

        // Section 1 descriptor
        igz.extend_from_slice(&(sec1_off as u32).to_be_bytes());
        igz.extend_from_slice(&(sec1_sz as u32).to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());

        // Section 2 descriptor
        igz.extend_from_slice(&(sec2_off as u32).to_be_bytes());
        igz.extend_from_slice(&(sec2_sz as u32).to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());

        // Section 3 descriptor
        igz.extend_from_slice(&(sec3_off as u32).to_be_bytes());
        igz.extend_from_slice(&(sec3_sz as u32).to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());

        // Sentinel descriptor: offset == 0
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());

        // Write section data
        igz.extend_from_slice(str_data);
        igz.resize(sec1_off + sec1_sz, 0);
        igz.resize(sec2_off + sec2_sz, 0);
        igz.extend_from_slice(&sec3_data);

        let model = IgbModel::parse(Cursor::new(&igz)).expect("Parse IGZ");
        assert_eq!(&model.header.magic, b"IGZ\x01");
        assert_eq!(model.header.version, 8);

        let parsed_igz_header = IgzHeader::parse(&igz).expect("Parse IgzHeader");
        assert_eq!(parsed_igz_header.descriptors.len(), 4);
        assert_eq!(parsed_igz_header.descriptors[0].offset, sec0_off);
        assert_eq!(parsed_igz_header.descriptors[0].size, sec0_sz);
        assert_eq!(parsed_igz_header.descriptors[3].offset, sec3_off);
        assert_eq!(parsed_igz_header.descriptors[3].size, sec3_sz);

        let markers = model.extract_level_markers();
        assert_eq!(markers.len(), 3); // sp_hero, pickup_gem, ^Lantern1
        assert!(markers.iter().any(|m| m.kind == MarkerKind::Spawn));
        assert!(markers.iter().any(|m| m.kind == MarkerKind::Collectible));
        assert!(markers.iter().any(|m| m.kind == MarkerKind::Prop));
    }

    #[test]
    fn test_synthetic_skeleton_and_animation() {
        let trans = |x: f32, y: f32, z: f32| -> [f32; 16] {
            let mut m = mat4_identity();
            m[12] = x;
            m[13] = y;
            m[14] = z;
            m
        };

        let bone_root = Bone {
            name: "root".to_string(),
            parent: None,
            inverse_bind_pose: mat4_identity(),
            local_transform: mat4_identity(),
        };

        let bone_child = Bone {
            name: "spine".to_string(),
            parent: Some(0),
            inverse_bind_pose: trans(0.0, -2.0, 0.0),
            local_transform: trans(0.0, 2.0, 0.0),
        };

        let skeleton = Skeleton::new(vec![bone_root, bone_child], 0);
        assert_eq!(skeleton.get_bone_index("root"), Some(0));
        assert_eq!(skeleton.get_bone_index("spine"), Some(1));

        let bytes = skeleton.serialize_binary(IgbEndian::Little);
        let parsed_skel = IgbModel::parse_skeleton(&bytes, "test_skel", IgbEndian::Little)
            .expect("Parse skeleton");
        assert_eq!(parsed_skel.bones.len(), 2);
        assert_eq!(parsed_skel.bones[0].name, "root");
        assert_eq!(parsed_skel.bones[1].name, "spine");
        assert_eq!(parsed_skel.bones[1].parent, Some(0));
        assert_eq!(parsed_skel.bones[1].local_transform[13], 2.0);
        assert_eq!(parsed_skel.bones[1].inverse_bind_pose[13], -2.0);

        let clip = AnimationClip::new(
            "walk",
            2.0,
            vec![
                BoneTrack {
                    bone_index: 0,
                    positions: vec![
                        Keyframe::new(0.0, [0.0, 0.0, 0.0]),
                        Keyframe::new(2.0, [10.0, 0.0, 0.0]),
                    ],
                    rotations: vec![
                        Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0]),
                        Keyframe::new(
                            2.0,
                            [
                                0.0,
                                std::f32::consts::FRAC_1_SQRT_2,
                                0.0,
                                std::f32::consts::FRAC_1_SQRT_2,
                            ],
                        ),
                    ],
                    scales: vec![Keyframe::new(0.0, [1.0, 1.0, 1.0])],
                },
                BoneTrack {
                    bone_index: 1,
                    positions: vec![
                        Keyframe::new(0.0, [0.0, 2.0, 0.0]),
                        Keyframe::new(2.0, [0.0, 4.0, 0.0]),
                    ],
                    rotations: vec![Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0])],
                    scales: vec![Keyframe::new(0.0, [1.0, 1.0, 1.0])],
                },
            ],
        );

        let anim_bytes = clip.serialize_binary(IgbEndian::Little);
        let parsed_clip = IgbModel::parse_animation(&anim_bytes, "walk", IgbEndian::Little)
            .expect("Parse animation");
        assert_eq!(parsed_clip.name, "walk");
        assert_eq!(parsed_clip.duration, 2.0);
        assert_eq!(parsed_clip.tracks.len(), 2);
        assert_eq!(parsed_clip.tracks[0].positions.len(), 2);

        let sampled_pos = sample_vec3(&parsed_clip.tracks[0].positions, 1.0).unwrap();
        assert!((sampled_pos[0] - 5.0).abs() < 1e-4);

        let sampled_rot = sample_quat(&parsed_clip.tracks[0].rotations, 1.0).unwrap();
        assert!((sampled_rot[1] - 0.3826834).abs() < 1e-3);
        assert!((sampled_rot[3] - 0.9238795).abs() < 1e-3);

        let skinning_matrices = clip.sample_pose(0.0, &skeleton);
        assert_eq!(skinning_matrices.len(), 2);

        for (i, &val) in skinning_matrices[0].iter().enumerate() {
            let expected = if i % 5 == 0 { 1.0 } else { 0.0 };
            assert!((val - expected).abs() < 1e-4);
        }

        for (i, &val) in skinning_matrices[1].iter().enumerate() {
            let expected = if i % 5 == 0 { 1.0 } else { 0.0 };
            assert!((val - expected).abs() < 1e-4);
        }

        let node0 = IgbSceneNode {
            name: "node0".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![1],
            geometry_index: None,
            transform: mat4_identity(),
        };
        let node1 = IgbSceneNode {
            name: "node1".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: Some(0),
            transform: trans(0.0, 2.0, 0.0),
        };
        let anim_model = IgbModel {
            header: IgbHeader {
                magic: *IGB_MAGIC,
                version: 8,
                object_count: 2,
                string_table_offset: 0,
                endian: IgbEndian::Little,
            },
            directory: vec![],
            nodes: vec![node0, node1],
            geometries: vec![IgbGeometryNode {
                name: "geo0".to_string(),
                vertex_buffer_index: None,
                index_buffer_index: None,
                attr_list_index: None,
                material_index: None,
                mesh: None,
            }],
            attr_lists: vec![],
            vertex_buffers: vec![],
            index_buffers: vec![],
            materials: vec![],
            textures: vec![],
            images: vec![],
            meshes: vec![],
            skeletons: vec![skeleton],
            animations: vec![clip.clone()],
        };

        let anim_instances = anim_model.evaluate_animated_scene_graph(&clip, 1.0);
        assert_eq!(anim_instances.len(), 1);
        assert_eq!(anim_instances[0].geometry_index, 0);
        assert!((anim_instances[0].world_transform[12] - 5.0).abs() < 1e-4);
    }

    fn create_test_character_igz(bone_names: &[&str]) -> Vec<u8> {
        let mut igz = Vec::new();
        igz.extend_from_slice(b"IGZ\x01");
        igz.extend_from_slice(&8u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&2u32.to_be_bytes());
        igz.extend_from_slice(&4u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());

        let sec0_off = 0x18 + 5 * 16;
        let mut str_data = Vec::new();
        str_data.extend_from_slice(b"RTST\0");
        for b in bone_names {
            str_data.extend_from_slice(b.as_bytes());
            str_data.push(0);
        }
        let sec0_sz = str_data.len();
        let sec1_off = sec0_off + sec0_sz;
        let sec1_sz = 32;
        let sec2_off = sec1_off + sec1_sz;
        let sec2_sz = 32;
        let sec3_off = sec2_off + sec2_sz;

        let mut sec3_data = Vec::new();
        sec3_data.extend_from_slice(&[0x08, 0x50, 0x00, 0x00, 0x02, 0x00]);
        sec3_data.extend_from_slice(&[0x90, 0x00, 0x03]);
        sec3_data.extend_from_slice(&1024i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&1024i16.to_be_bytes());
        sec3_data.extend_from_slice(&10240i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&0i16.to_be_bytes());
        sec3_data.extend_from_slice(&10240i16.to_be_bytes());
        let sec3_sz = sec3_data.len();

        for (off, sz) in [
            (sec0_off, sec0_sz),
            (sec1_off, sec1_sz),
            (sec2_off, sec2_sz),
            (sec3_off, sec3_sz),
        ] {
            igz.extend_from_slice(&(off as u32).to_be_bytes());
            igz.extend_from_slice(&(sz as u32).to_be_bytes());
            igz.extend_from_slice(&0u32.to_be_bytes());
            igz.extend_from_slice(&0u32.to_be_bytes());
        }
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());
        igz.extend_from_slice(&0u32.to_be_bytes());

        igz.extend_from_slice(&str_data);
        igz.resize(sec1_off + sec1_sz, 0);
        igz.resize(sec2_off + sec2_sz, 0);
        igz.extend_from_slice(&sec3_data);
        igz
    }

    #[test]
    fn test_extract_real_character_skeleton_crocky() {
        let p1 = std::path::Path::new("assets/files/character/462_Water_Crocky.bld");
        let p2 = std::path::Path::new("../../assets/files/character/462_Water_Crocky.bld");
        let bytes = if p1.exists() {
            std::fs::read(p1).unwrap()
        } else if p2.exists() {
            std::fs::read(p2).unwrap()
        } else {
            create_test_character_igz(&[
                "j_root",
                "j_pelvis",
                "j_spine",
                "j_head",
                "j_leftarm",
                "j_rightarm",
            ])
        };

        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse crocky");
        let (min_pt, max_pt) = model.vertex_bounds();
        println!(
            "Crocky bounds: min={:?}, max={:?}, height={}",
            min_pt,
            max_pt,
            max_pt[1] - min_pt[1]
        );
        assert!(!model.skeletons.is_empty(), "Skeleton must be extracted");
        let skel = &model.skeletons[0];
        assert!(!skel.bones.is_empty(), "Bones must be non-empty");
        assert!(skel.bones[0].parent.is_none(), "Root must have no parent");
        for (i, b) in skel.bones.iter().enumerate().skip(1) {
            assert!(
                b.parent.is_some_and(|p| p < skel.bones.len()),
                "Bone {} parent must be valid index",
                i
            );
            assert!(
                b.inverse_bind_pose[0].is_finite() && b.inverse_bind_pose[15].is_finite(),
                "Inverse bind matrix must be valid"
            );
        }
        assert!(!model.animations.is_empty(), "Animation clips generated");
        assert!(model.animations.iter().any(|a| a.name == "idle"));
        assert!(model.animations.iter().any(|a| a.name == "walk"));
        assert!(model.animations.iter().any(|a| a.name == "attack"));
    }

    #[test]
    fn test_extract_real_character_skeleton_kaos() {
        let p1 = std::path::Path::new("assets/files/character/1010_Kaos.bld");
        let p2 = std::path::Path::new("../../assets/files/character/1010_Kaos.bld");
        let bytes = if p1.exists() {
            std::fs::read(p1).unwrap()
        } else if p2.exists() {
            std::fs::read(p2).unwrap()
        } else {
            create_test_character_igz(&["root", "pelvis", "spine", "head", "left_arm", "right_arm"])
        };

        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse kaos");
        let (min_pt, max_pt) = model.vertex_bounds();
        println!(
            "Kaos bounds: min={:?}, max={:?}, height={}",
            min_pt,
            max_pt,
            max_pt[1] - min_pt[1]
        );
        assert!(!model.skeletons.is_empty(), "Kaos skeleton extracted");
        assert!(!model.meshes.is_empty(), "Kaos meshes present");
        for mesh in &model.meshes {
            for v in &mesh.vertices {
                let sum: f32 = v.bone_weights.iter().sum();
                assert!(
                    (sum - 1.0).abs() < 1e-3,
                    "Kaos vertex weights must normalize to 1.0 (got {})",
                    sum
                );
            }
        }
    }

    #[test]
    fn test_extract_generic_character_skeleton() {
        let bytes = create_test_character_igz(&["geo_mesh", "mat_hero", "dummy"]);
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse generic");
        assert!(!model.skeletons.is_empty(), "Generic skeleton extracted");
        let skel = &model.skeletons[0];
        assert_eq!(skel.bones.len(), 6, "Humanoid rig has 6 bones");
        assert_eq!(skel.bones[0].name, "Root");
        assert_eq!(skel.bones[1].name, "Pelvis");
        assert_eq!(skel.bones[2].name, "Spine");
        assert_eq!(skel.bones[3].name, "Head");
        assert_eq!(skel.bones[4].name, "Left Arm");
        assert_eq!(skel.bones[5].name, "Right Arm");

        for mesh in &model.meshes {
            for v in &mesh.vertices {
                let sum: f32 = v.bone_weights.iter().sum();
                assert!((sum - 1.0).abs() < 1e-3, "Weights must normalize to 1.0");
                for &idx in &v.bone_indices {
                    assert!((idx as usize) < skel.bones.len(), "Bone index in bounds");
                }
            }
        }
    }

    #[test]
    fn test_extract_real_character_spyro_mesh_and_skeleton() {
        let p1 = std::path::Path::new("assets/files/character/016_Spyro.bld");
        let p2 = std::path::Path::new("../../assets/files/character/016_Spyro.bld");
        let bytes = if p1.exists() {
            std::fs::read(p1).unwrap()
        } else if p2.exists() {
            std::fs::read(p2).unwrap()
        } else {
            create_test_character_igz(&[
                "j_root", "j_pelvis", "j_spine", "j_head", "j_wing_l", "j_wing_r", "j_tail",
            ])
        };

        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
        assert!(!model.meshes.is_empty(), "Spyro must have meshes");
        let (min_pt, max_pt) = model.vertex_bounds();
        let height = max_pt[1] - min_pt[1];
        assert!(height > 0.0, "Spyro mesh height must be positive");
        assert!(min_pt[0].is_finite() && max_pt[0].is_finite());

        let total_verts: usize = model.meshes.iter().map(|m| m.vertices.len()).sum();
        let total_indices: usize = model.meshes.iter().map(|m| m.indices.len()).sum();
        assert!(total_verts >= 3, "Spyro vertices must be >= 3");
        assert!(total_indices >= 3, "Spyro indices must be >= 3");

        for mesh in &model.meshes {
            for v in &mesh.vertices {
                assert!(
                    v.position[0].is_finite()
                        && v.position[1].is_finite()
                        && v.position[2].is_finite()
                );
                assert!(
                    v.normal[0].is_finite() && v.normal[1].is_finite() && v.normal[2].is_finite()
                );
                assert!(v.uv[0].is_finite() && v.uv[1].is_finite());
                let sum: f32 = v.bone_weights.iter().sum();
                assert!((sum - 1.0).abs() < 1e-3, "Spyro weights must sum to ~1.0");
            }
            for &idx in &mesh.indices {
                assert!(
                    (idx as usize) < mesh.vertices.len(),
                    "Index in vertex bounds"
                );
            }
        }

        assert!(
            !model.skeletons.is_empty(),
            "Spyro skeleton must be extracted"
        );
        let skel = &model.skeletons[0];
        assert!(skel.bones.len() >= 6, "Spyro skeleton must have >= 6 bones");
        assert!(skel.bones[0].parent.is_none(), "Root bone has no parent");
        for (i, b) in skel.bones.iter().enumerate().skip(1) {
            assert!(
                b.parent.is_some_and(|p| p < skel.bones.len()),
                "Spyro bone {} parent must be valid",
                i
            );
        }

        if p1.exists() || p2.exists() {
            assert!(
                !model.textures.is_empty(),
                "Spyro textures extracted from igImage2"
            );
            assert!(
                !model.images.is_empty(),
                "Spyro images extracted from igImage2"
            );
            assert!(model.images[0].width > 0 && model.images[0].height > 0);
            assert!(model.images[0].texture_size > 0);
        }
    }

    #[test]
    fn test_half_float_decoding() {
        assert_eq!(IgbModel::half_to_f32(0x0000), 0.0);
        assert_eq!(IgbModel::half_to_f32(0x8000), -0.0);
        assert_eq!(IgbModel::half_to_f32(0x3c00), 1.0);
        assert_eq!(IgbModel::half_to_f32(0xbc00), -1.0);
        assert_eq!(IgbModel::half_to_f32(0x3800), 0.5);
        assert_eq!(IgbModel::half_to_f32(0xc000), -2.0);
        assert!(
            IgbModel::half_to_f32(0x7c00).is_infinite()
                && IgbModel::half_to_f32(0x7c00).is_sign_positive()
        );
        assert!(
            IgbModel::half_to_f32(0xfc00).is_infinite()
                && IgbModel::half_to_f32(0xfc00).is_sign_negative()
        );
        assert!(IgbModel::half_to_f32(0x7c01).is_nan());
    }

    #[test]
    fn test_rvtb_packed_integer_unpacking() {
        let descriptors = vec![
            IgzChunkDescriptor {
                offset: 0x1000,
                size: 0x200,
                unknown1: 0,
                unknown2: 0,
            },
            IgzChunkDescriptor {
                offset: 0x2000,
                size: 0x1000,
                unknown1: 0,
                unknown2: 0,
            },
        ];

        // 3 entries:
        // Entry 0: raw nibble 0x01 (unpacked = 1, cont = 0)
        // Entry 1: raw nibble 0x02 (unpacked = 2, cont = 0)
        // Entry 2: multi-nibble: nibble 0 = 0x0B (val = 3, cont = 1), nibble 1 = 0x01 (val = 1, cont = 0)
        // unpacked_int = 3 | (1 << 3) = 11.
        // Packed stream nibbles: [0x1, 0x2, 0xB, 0x1]
        // Byte 0: 0x21 (low = 1, high = 2)
        // Byte 1: 0x1B (low = 0xB, high = 1)
        let packed_data = vec![0x21, 0x1B];
        let offsets = IgzOffsetMapFixup::unpack_offsets(&packed_data, 3, 8, &descriptors)
            .expect("unpack offsets");

        // version 8: delta = unpacked_int * 4 + 4
        // Entry 0: delta = 1 * 4 + 4 = 8. prev = 8.
        // desc_idx = (8 >> 27) + 1 = 1 -> base = descriptors[1].offset = 0x2000.
        // offset = 0x2000 + (8 & 0x07FFFFFF) = 0x2008.
        // Entry 1: delta = 2 * 4 + 4 = 12. prev = 8 + 12 = 20.
        // offset = 0x2000 + 20 = 0x2014.
        // Entry 2: delta = 11 * 4 + 4 = 48. prev = 20 + 48 = 68.
        // offset = 0x2000 + 68 = 0x2044.
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0], 0x2008);
        assert_eq!(offsets[1], 0x2014);
        assert_eq!(offsets[2], 0x2044);
    }

    #[test]
    fn test_rvtb_fixup_parsing() {
        let descriptors = vec![
            IgzChunkDescriptor {
                offset: 0x00,
                size: 0x100,
                unknown1: 0,
                unknown2: 0,
            },
            IgzChunkDescriptor {
                offset: 0x1000,
                size: 0x1000,
                unknown1: 0,
                unknown2: 0,
            },
        ];

        // Construct RVTB fixup table block in BigEndian
        let mut fixup_block = Vec::new();
        fixup_block.extend_from_slice(b"RVTB"); // magic
        fixup_block.extend_from_slice(&2u32.to_be_bytes()); // count = 2
        fixup_block.extend_from_slice(&18u32.to_be_bytes()); // length = 18 bytes
        fixup_block.extend_from_slice(&16u32.to_be_bytes()); // start_of_data = 16
                                                             // Nibbles for 2 entries: 0x00, 0x01 -> byte: 0x10
        fixup_block.push(0x10);
        fixup_block.push(0x00);

        let fixup = IgzOffsetMapFixup::parse(&fixup_block, 0, IgbEndian::Big, 8, &descriptors)
            .expect("parse RVTB fixup");

        assert_eq!(fixup.magic, *b"RVTB");
        assert_eq!(fixup.count, 2);
        assert_eq!(fixup.offsets.len(), 2);
        // Entry 0: delta = 0*4 + 4 = 4 -> 0x1000 + 4 = 0x1004
        // Entry 1: delta = 1*4 + 4 = 8 -> 0x1004 + 8 = 0x100C
        assert_eq!(fixup.offsets[0], 0x1004);
        assert_eq!(fixup.offsets[1], 0x100C);
    }

    #[test]
    fn test_igz_object_registry_parsing() {
        // Build mock IGZ buffer:
        // Version 8, Big Endian
        let mut igz = vec![0u8; 0x400];
        igz[0..4].copy_from_slice(b"IGZ\x01"); // magic
        igz[4..8].copy_from_slice(&8u32.to_be_bytes()); // version
        igz[8..12].copy_from_slice(&0u32.to_be_bytes()); // crc

        // Section descriptors starting at 0x18
        // Section 0: fixups, offset = 0x80, size = 0x80
        igz[0x18..0x1C].copy_from_slice(&0x80u32.to_be_bytes());
        igz[0x1C..0x20].copy_from_slice(&0x80u32.to_be_bytes());
        // Section 1: objects, offset = 0x100, size = 0x100, unknown2 = 0x20
        igz[0x28..0x2C].copy_from_slice(&0x100u32.to_be_bytes());
        igz[0x2C..0x30].copy_from_slice(&0x100u32.to_be_bytes());
        igz[0x34..0x38].copy_from_slice(&0x20u32.to_be_bytes()); // unknown2
                                                                 // End sentinel (offset = 0) at 0x38..0x3C is already 0

        // In Section 0 (0x80):
        // 1) TEMT table at 0x80:
        // tag = b"TEMT", count = 2, size = 20, startOfData = 16
        igz[0x80..0x84].copy_from_slice(b"TEMT");
        igz[0x84..0x88].copy_from_slice(&2u32.to_be_bytes());
        igz[0x88..0x8C].copy_from_slice(&20u32.to_be_bytes());
        igz[0x8C..0x90].copy_from_slice(&16u32.to_be_bytes());
        let type_strings = b"igObjectList\0igImage2\0";
        igz[0x90..0x90 + type_strings.len()].copy_from_slice(type_strings);

        // In Section 1:
        // List object at offset 0x100 + unknown2 (0x20) = 0x120
        // Header for v8:
        // u16 unk (0), u16 name (0 -> igObjectList), u32 itemCount (1), u16 flags1 (0), u32 unk (0), u16 length (8), u32 unk (0)
        let list_off = 0x120;
        igz[list_off + 2..list_off + 4].copy_from_slice(&0u16.to_be_bytes()); // type index 0
        igz[list_off + 4..list_off + 8].copy_from_slice(&1u32.to_be_bytes()); // itemCount = 1
        igz[list_off + 8..list_off + 10].copy_from_slice(&0u16.to_be_bytes()); // flags1 = 0
        igz[list_off + 14..list_off + 16].copy_from_slice(&8u16.to_be_bytes()); // length = 8
                                                                                // Pointer field at list_off + 0x14:
        igz[list_off + 0x14..list_off + 0x18].copy_from_slice(&0x140u32.to_be_bytes());

        // Object 1 at 0x140:
        // u16 unk, u16 name (1 -> igImage2), u32 itemCount (0), u16 flags1 (0), u32 unk, u16 length (4), u32 unk
        let obj1_off = 0x140;
        igz[obj1_off + 2..obj1_off + 4].copy_from_slice(&1u16.to_be_bytes()); // type index 1
        igz[obj1_off + 4..obj1_off + 8].copy_from_slice(&0u32.to_be_bytes());
        igz[obj1_off + 8..obj1_off + 10].copy_from_slice(&0u16.to_be_bytes());
        igz[obj1_off + 14..obj1_off + 16].copy_from_slice(&4u16.to_be_bytes());

        let header = IgzHeader::parse(&igz).expect("header parse");
        let type_table = vec!["igObjectList".to_string(), "igImage2".to_string()];
        let rvtb_offsets = vec![list_off, obj1_off];

        let obj_list = IgzObjectList::parse(
            &igz,
            &header.descriptors,
            header.version,
            header.endian,
            &type_table,
            &rvtb_offsets,
        )
        .expect("object list parse");

        assert_eq!(obj_list.list_object.type_name, Some("igObjectList".into()));
        assert_eq!(obj_list.list_object.fields, vec![0x140]);
        assert_eq!(obj_list.objects.len(), 1);
        assert_eq!(obj_list.objects[0].offset, 0x140);
        assert_eq!(obj_list.objects[0].type_name, Some("igImage2".into()));
    }

    #[test]
    fn test_inspect_spyro_mesh_sources() {
        use crate::archive::ArchiveMount;
        let p1 = std::path::Path::new("assets/files/character/016_Spyro.bld");
        let p2 = std::path::Path::new("../../assets/files/character/016_Spyro.bld");
        let p = if p1.exists() {
            p1
        } else if p2.exists() {
            p2
        } else {
            return;
        };
        let mut mount = ArchiveMount::new();
        mount.mount(p).expect("mount spyro");
        let level_bld = mount.read_file("level.bld").expect("read level.bld");
        let model = IgbModel::parse(Cursor::new(&level_bld)).expect("parse level.bld");
        assert!(!model.meshes.is_empty(), "Spyro has extracted meshes");
        assert!(!model.textures.is_empty(), "Spyro has extracted textures");
        assert_eq!(model.images.len(), 77, "Spyro has 77 igImage2 textures");
        assert_eq!(model.geometries.len(), 17, "Spyro has 17 Wii geometries");
        assert_eq!(model.vertex_buffers.len(), 6, "Spyro has 6 vertex buffers");
        assert_eq!(model.index_buffers.len(), 6, "Spyro has 6 index buffers");
        assert_eq!(
            model.attr_lists.len(),
            18,
            "Spyro has 18 vertex format/attr lists"
        );
        // Verify all 16 extracted character submeshes are present
        assert_eq!(
            model.meshes.len(),
            17,
            "Spyro has 17 submeshes with geometry"
        );
        for mesh in &model.meshes {
            assert!(mesh.vertices.len() >= 3);
            assert!(!mesh.indices.is_empty());
        }
    }

    #[test]
    fn test_fnv1a_32_archive_spec() {
        use crate::archive::{hash_crc32, hash_fnv1a, hash_path};
        // Verify FNV-1a matches expected hashes from igArchive wiki
        assert_eq!(hash_fnv1a("level.bld"), 0xba91d879);
        assert_eq!(hash_fnv1a("ENGLISH.pak"), 0xe115e6dd);
        assert_eq!(hash_fnv1a("GERMAN.pak"), 0xe57e6515);
        assert_eq!(hash_path("level.bld"), 0xba91d879);
        assert_eq!(hash_crc32("level.bld"), 0xc73f6fdc);
    }

    #[test]
    fn test_uv_decoding_formats() {
        let mut slice = Vec::new();
        // 1. Float32x2 at offset 0: (0.25, 0.75)
        slice.extend_from_slice(&0.25f32.to_be_bytes());
        slice.extend_from_slice(&0.75f32.to_be_bytes());
        // 2. Fixed-point signed i16 at offset 8: (-102, 512) scaled by 1.0 / 1024.0
        slice.extend_from_slice(&(-102i16).to_be_bytes());
        slice.extend_from_slice(&(512i16).to_be_bytes());
        // 3. Half float at offset 12: (1.0, 0.5) = (0x3C00, 0x3800)
        slice.extend_from_slice(&0x3C00u16.to_be_bytes());
        slice.extend_from_slice(&0x3800u16.to_be_bytes());

        let mut v1 = IgbVertex::default();
        let attr_f32 = IgbAttrDesc {
            semantic: AttrSemantic::TexCoord,
            format: AttrFormat::Float32x2,
            offset: 0,
            scale: 1.0,
        };
        // Verify Float32x2
        let aoff = attr_f32.offset as usize;
        let u = IgbModel::read_f32(&slice, aoff, IgbEndian::Big);
        let v = IgbModel::read_f32(&slice, aoff + 4, IgbEndian::Big);
        v1.uv = [u, v];
        assert!((v1.uv[0] - 0.25).abs() < 1e-4);
        assert!((v1.uv[1] - 0.75).abs() < 1e-4);

        // Verify U16x2UV with signed fixed point scale
        let raw_u = IgbModel::read_u16(&slice, 8, IgbEndian::Big);
        let raw_v = IgbModel::read_u16(&slice, 10, IgbEndian::Big);
        let scale = 1.0 / 1024.0;
        let (su, sv) = if scale >= (1.0 / 32768.0) {
            ((raw_u as i16) as f32 * scale, (raw_v as i16) as f32 * scale)
        } else {
            (raw_u as f32 * scale, raw_v as f32 * scale)
        };
        let v2 = IgbVertex {
            uv: [su, sv],
            ..Default::default()
        };
        assert!(
            v2.uv[0] < 0.0,
            "Negative UV coordinate must remain negative, not overflow to ~64.0"
        );
        assert!((v2.uv[0] - (-102.0 / 1024.0)).abs() < 1e-4);
        assert!((v2.uv[1] - 0.5).abs() < 1e-4);

        // Verify Float16x2UV
        let uh = IgbModel::read_u16(&slice, 12, IgbEndian::Big);
        let vh = IgbModel::read_u16(&slice, 14, IgbEndian::Big);
        let v3 = IgbVertex {
            uv: [IgbModel::half_to_f32(uh), IgbModel::half_to_f32(vh)],
            ..Default::default()
        };
        assert!((v3.uv[0] - 1.0).abs() < 1e-4);
        assert!((v3.uv[1] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_igz_vertex_format_stride_derivation() {
        // Stride 16
        let igz_obj_16 = IgzObject {
            offset: 0,
            type_index: 0,
            type_name: Some("igVertexFormat".into()),
            item_count: 0,
            length: 0,
            data: vec![],
            fields: vec![16],
        };
        let desc_16 = &igz_obj_16.fields[0];
        assert_eq!(*desc_16, 16);

        // Stride 24
        let igz_obj_24 = IgzObject {
            offset: 0,
            type_index: 0,
            type_name: Some("igVertexFormat".into()),
            item_count: 0,
            length: 0,
            data: vec![],
            fields: vec![24],
        };
        assert_eq!(igz_obj_24.fields[0], 24);
    }

    #[test]
    fn test_wii_display_list_indexed_strip() {
        // VAT0: pos xyz s16 shift 6; VCD: pos idx16; arrays base 4 stride 6.
        let mut dl = Vec::new();
        let cp = |dl: &mut Vec<u8>, reg: u8, val: u32| {
            dl.push(0x08);
            dl.push(reg);
            dl.extend_from_slice(&val.to_be_bytes());
        };
        cp(&mut dl, 0x70, 1 | (3 << 1) | (6 << 4));
        cp(&mut dl, 0xA0, 4);
        cp(&mut dl, 0xB0, 6);
        cp(&mut dl, 0x50, 3 << 9);
        cp(&mut dl, 0x60, 0);
        dl.extend_from_slice(&[0x98, 0x00, 0x04]);
        for idx in [0u16, 1, 2, 3] {
            dl.extend_from_slice(&idx.to_be_bytes());
        }
        dl.extend_from_slice(&[0u8; 7]); // trailing NOP padding

        let mut vb = vec![0xEE; 4];
        for p in [[0i16, 0, 0], [64, 0, 0], [0, 0, 64], [64, 0, 64]] {
            for c in p {
                vb.extend_from_slice(&c.to_be_bytes());
            }
        }

        let mesh = IgbModel::decode_wii_display_list(&dl, &vb).expect("decodes");
        let pos: Vec<_> = mesh.vertices.iter().map(|v| v.position).collect();
        assert_eq!(
            pos,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0]
            ]
        );
        assert_eq!(mesh.indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn test_hub_level_geometry_has_no_exploded_triangles() {
        use crate::archive::ArchiveMount;
        let rel = "assets/files/level/Level_Hub_2014_01.bld";
        let p = [rel.to_string(), format!("../../{rel}")]
            .into_iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.exists());
        let Some(p) = p else { return };
        let mut mount = ArchiveMount::new();
        mount.mount(&p).expect("mount hub");
        let data = mount.read_file("level.bld").expect("read level.bld");
        let model = IgbModel::parse(Cursor::new(&data)).expect("parse hub");
        assert!(model.meshes.len() > 3000, "got {}", model.meshes.len());
        assert!(
            model
                .meshes
                .iter()
                .flat_map(|m| &m.vertices)
                .all(|v| v.bone_weights == [1.0, 0.0, 0.0, 0.0]),
            "static level must not get synthetic skinning"
        );

        let mut edges: Vec<f32> = Vec::new();
        for m in &model.meshes {
            for t in m.indices.as_chunks::<3>().0 {
                let v = |k: u32| m.vertices[k as usize].position;
                let d = |a: [f32; 3], b: [f32; 3]| {
                    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
                };
                let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
                edges.push(d(a, b).max(d(b, c)).max(d(c, a)));
            }
        }
        edges.sort_by(f32::total_cmp);
        let median = edges[edges.len() / 2];
        assert!(edges.len() > 500_000, "got {} triangles", edges.len());
        assert!(median < 3.0, "median edge {median}");
    }
}
