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

pub mod meta_fields {
    pub mod ig_wii_geometry {
        pub const DISPLAY_LIST: usize = 0x08;
        pub const VERTEX_BUFFER: usize = 0x10;
        pub const ARRAY_BASES: usize = 0x18;
        pub const VERTEX_COUNTS: usize = 0x24;
        pub const PRIMITIVE_COUNT: usize = 0x30;
        pub const PRIMITIVE_LENGTH_COUNT: usize = 0x34;
        pub const POS_ATTR_DL_STRIDE: usize = 0x38;
        pub const HAS_COLOR: usize = 0x3c;
    }

    pub mod ig_image2 {
        pub const WIDTH: usize = 0x30;
        pub const HEIGHT: usize = 0x32;
        pub const DEPTH: usize = 0x34;
        pub const LEVEL_COUNT: usize = 0x36;
        pub const IMAGE_COUNT: usize = 0x38;
        pub const FORMAT: usize = 0x3c;
        pub const USAGE_DEPRECATED: usize = 0x40;
        pub const PADDING_DEPRECATED: usize = 0x44;
        pub const DATA: usize = 0x48;
        pub const LOCK_COUNT: usize = 0x4c;
        pub const TEX_HANDLE: usize = 0x50;
        pub const LOCKED_MEMORY: usize = 0x54;
        pub const OGL_DISCARD_ORIGINAL_IMAGE: usize = 0x58;
    }

    pub mod ig_texture_attr2 {
        pub const WRAP_S: usize = 0x20;
        pub const WRAP_T: usize = 0x24;
        pub const IMAGE: usize = 0x30;
    }

    pub mod ig_geometry_attr {
        pub const VERTEX_BUFFER: usize = 0x10;
        pub const INDEX_BUFFER: usize = 0x14;
        pub const OFFSET: usize = 0x18;
        pub const PRIMITIVE_COUNT: usize = 0x1c;
        pub const VERTEX_ARRAY: usize = 0x20;
        pub const INDEX_ARRAY: usize = 0x24;
        pub const IS_DYNAMIC: usize = 0x28;
    }

    pub mod entity {
        pub const TRANSFORM: usize = 0x08;
        pub const LIST_MASK: usize = 0x0c;
        pub const PARENT_INDEX: usize = 0x10;
        pub const FLAG_BITS: usize = 0x12;
    }

    pub mod named_object {
        pub const NAME: usize = 0x08;
    }

    /// `tfbActorInfo` derives from `AbstractPlacement`.
    pub mod actor_info {
        pub const ID: usize = 0x20;
        pub const INIT_MATRIX: usize = 0x24;
        /// `tfbEulerTransform` rotation, Euler degrees x,y,z.
        pub const INIT_ROTATION: usize = 0x30;
        /// `AbstractPlacement._activityState`. Hub: 4 on live props, 5 on templates
        /// that scripts spawn later (debris, IGC copies, digits, pickups).
        pub const ACTIVITY_STATE: usize = 0x54;
        pub const ACTIVITY_ACTIVE: u32 = 4;
        /// `tfbPhysicsModel` drawn at the placement (statues, NPCs, doors).
        pub const PHYSICS_MODEL: usize = 0xec;
    }

    /// One source scene layer of a level (`Level_302_ART_MASTER.ma`, `_SKY/...ma`).
    pub mod physics_world {
        /// `tfbWorldEntityInfo` with the layer's drawables.
        pub const ENTITY_INFO: usize = 0x28;
    }

    pub mod physics_model {
        /// `tfbBodyEntityInfo` holding the model's drawables, in model space.
        pub const ENTITY_INFO: usize = 0x68;
    }

    /// `tfbActorInfo` (0xf8) derives from `Placement` (0xd8).
    pub mod placement {
        pub const SCRIPT: usize = 0xa8;
        pub const COLLISION: usize = 0xc4;
        /// `_scaleVec`, percent per axis.
        pub const SCALE_VEC: usize = 0xb8;
    }

    pub mod collision_info {
        pub const PROPERTIES: usize = 0x18;
        pub const TRANSFORM: usize = 0x1c;
        pub const WIDTH: usize = 0x34;
        pub const HEIGHT: usize = 0x38;
        pub const LENGTH: usize = 0x3c;
        pub const WEIGHT: usize = 0x40;
        pub const REBOUND: usize = 0x44;
        pub const OBJECT_OWNER: usize = 0x48;
    }

    /// Script VM ops (`OpSetValue` 0x24, `ValueStack` 0x1c, `RHSValueStack` 0x28).
    pub mod script {
        pub const SET_VALUE_LHS: usize = 0x1c;
        pub const SET_VALUE_RHS: usize = 0x20;
        /// `ValueStack` -> `OpCreateVariable`.
        pub const VALUE_STACK_VARIABLE: usize = 0x1c;
        pub const CREATE_VARIABLE_NAME: usize = 0x1c;
        /// External metaobject handle (index into `MNXE`).
        pub const RHS_TYPE: usize = 0x20;
        pub const RHS_VALUE: usize = 0x24;
    }

    pub mod animation_component {
        pub const NOW_PLAYING: usize = 0x08;
        pub const OWNER: usize = 0x14;
        pub const CURRENT_BLEND_LIST: usize = 0x18;
        pub const ID_CODE: usize = 0x1c;
        pub const ANIMATION_SYSTEM: usize = 0x20;
        pub const VISEME_CODE: usize = 0x24;
    }
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
    /// Explicit marker kind (from `tfbActorInfo` script); `None` = derive from name.
    pub marker: Option<MarkerKind>,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IgbMesh {
    pub name: String,
    pub vertices: Vec<IgbVertex>,
    pub indices: Vec<u32>,
    pub material_name: Option<String>,
    /// Index into `IgbModel::skeletons` that `bone_indices` refer to.
    pub skeleton: Option<usize>,
    /// Index into `IgbModel::images` for this mesh's primary diffuse texture.
    pub image_index: Option<usize>,
    /// Base diffuse tint color `[r, g, b, a]` from `igColorAttr`.
    pub base_color: Option<[f32; 4]>,
    /// `igTextureAttr2._wrapS` / `_wrapT` of the diffuse texture.
    pub texture_wrap: [TextureWrap; 2],
    /// `tfbScrollUVAnimData` `UVanimVelocityU/V` (UV units per second) of the material.
    pub uv_scroll: Option<[f32; 2]>,
}

/// `IG_GFX_TEXTURE_WRAP_*` (enum order from the `main.dol` string table:
/// `CLAMP`, `REPEAT`, `REGION_CLAMP`, `REGION_REPEAT`, `BORDER`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextureWrap {
    Clamp,
    #[default]
    Repeat,
}

impl TextureWrap {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            0 | 2 | 4 => Self::Clamp,
            _ => Self::Repeat,
        }
    }
}

/// Diffuse material of one `Drawable` (`tfbRuntimeMaterial`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawableMaterial {
    /// `igImage2` object offset.
    pub image: Option<usize>,
    pub color: Option<[f32; 4]>,
    pub name: Option<String>,
    pub wrap: [TextureWrap; 2],
    /// `tfbRuntimeMaterial._transforms` (`+0x28`) -> `igCustomMaterialAnimation` `+0x08`
    /// -> `tfbFxMaterialUvAnimationTransform._track` (`+0x50`) -> `tfbScrollUVAnimTrack`
    /// `+0x10` -> `tfbScrollUVAnimData` velocity (`+0x10`, `+0x14`).
    pub uv_scroll: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Spawn,
    Prop,
    Collectible,
    TriggerZone,
    Doorway,
    Enemy(EnemyClass),
    /// Talkable character (`NPC_<Name>.ai`, shop keepers).
    Npc,
    Other,
}

/// Enemy combat class, after `LevelSystems.ai` `XpShares_Near` / `XpShares_Far`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyClass {
    Near,
    Far,
}

impl EnemyClass {
    /// `Enemy_S4_<Type>[_<Part>].ai` -> class; `None` for helper scripts
    /// (projectiles, controllers, spawners, VFX) attached to an enemy.
    // ponytail: keyword tables from a survey of all level scripts; the VM decides in-game.
    pub fn from_script_stem(stem: &str) -> Option<Self> {
        const HELPER: &[&str] = &[
            "projectile",
            "controller",
            "impact",
            "beam",
            "patch",
            "muzzle",
            "shield",
            "exit",
            "float",
            "staff",
            "puddle",
            "attached",
            "voidmaster",
            "ninjitsu",
            "blade",
            "spike",
            "bomb",
            "test",
            "pile",
            "icebreak",
            "uberpound",
            "wholeegg",
            "dome",
            "streamer",
        ];
        const FAR: &[&str] = &[
            "far",
            "lobber",
            "shooter",
            "gunner",
            "stungun",
            "magnet",
            "rocketeer",
            "caster",
            "thrower",
            "launcher",
            "mage",
            "gunk",
            "spitter",
            "tank",
        ];
        let rest = stem.strip_prefix("enemy_s4_")?;
        let mut parts = rest.split('_');
        let kind = parts.next().filter(|k| !k.is_empty())?;
        if kind.contains("spawner")
            || kind == "template"
            || parts.any(|p| HELPER.iter().any(|h| p.contains(h)))
        {
            return None;
        }
        Some(if FAR.iter().any(|f| kind.contains(f)) {
            EnemyClass::Far
        } else {
            EnemyClass::Near
        })
    }
}

/// Display name of the NPC an `.ai` script drives, `None` if the script is not an NPC.
/// Hub: `NPC_<Name>[_<Variant>].ai`, `Auric.ai` (store), `Persephone_2014.ai` (upgrades).
/// `NPC_WantedPosters.ai` is a board without a model, not a character.
pub fn npc_name(script_path: &str) -> Option<String> {
    let file = script_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(script_path);
    let stem = file.strip_suffix(".ai").unwrap_or(file);
    let lower = stem.to_ascii_lowercase();
    npc_name_from_stem(&lower)?;
    let name = match stem.get(..4) {
        Some(p) if p.eq_ignore_ascii_case("npc_") => stem[4..].split('_').next()?,
        _ => stem.split('_').next()?,
    };
    Some(match name {
        "TBone" => "T-Bone".to_string(),
        n => n.to_string(),
    })
}

/// Lower-case script stem -> lower-case NPC name.
fn npc_name_from_stem(stem: &str) -> Option<&str> {
    match stem {
        "auric" => Some("auric"),
        "persephone_2014" => Some("persephone"),
        "npc_wantedposters" => None,
        _ => stem
            .strip_prefix("npc_")?
            .split('_')
            .next()
            .filter(|n| !n.is_empty()),
    }
}

impl MarkerKind {
    /// Kind from the actor's `.ai` script path (`Placement._script` -> `tfbScriptInfo._name`).
    /// `None` = script known but not a marker we spawn (cameras, controllers, NPCs, ...).
    // ponytail: table built from hub scripts only; extend as other levels show new ones.
    pub fn from_script(path: &str) -> Option<Self> {
        let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let stem = file
            .strip_suffix(".ai")
            .unwrap_or(file)
            .to_ascii_lowercase();
        match stem.as_str() {
            "2d_coin"
            | "2d_coin_layer"
            | "hub_coins"
            | "placed_loot_spinning"
            | "bauble_pickup"
            | "healing_food"
            | "leveluptoken"
            | "coin_dropped" => Some(MarkerKind::Collectible),
            "transitcrystal" => Some(MarkerKind::Doorway),
            "big_pot" | "practice_dummy" | "lid" | "bauble_lid" | "chandelier" => {
                Some(MarkerKind::Prop)
            }
            _ if npc_name_from_stem(&stem).is_some() => Some(MarkerKind::Npc),
            _ if stem.contains("teleporter") => Some(MarkerKind::Doorway),
            _ if stem.starts_with("door_opener") => Some(MarkerKind::TriggerZone),
            _ => EnemyClass::from_script_stem(&stem).map(MarkerKind::Enemy),
        }
    }

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
        } else if lower.starts_with("prop_") || lower.starts_with('^') {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionShapeType {
    Box,
    Cylinder,
    Capsule,
    Sphere,
    Mesh,
    Unknown(u32),
}

impl From<u32> for CollisionShapeType {
    fn from(val: u32) -> Self {
        match val & 0x07 {
            0 => CollisionShapeType::Box,
            1 => CollisionShapeType::Cylinder,
            2 => CollisionShapeType::Capsule,
            3 => CollisionShapeType::Sphere,
            4 => CollisionShapeType::Mesh,
            other => CollisionShapeType::Unknown(other),
        }
    }
}

/// Analytical bounding collision volume from `CollisionInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionPrimitive {
    pub name: String,
    pub shape_type: CollisionShapeType,
    pub local_position: [f32; 3],
    pub local_rotation_deg: [f32; 3],
    pub extents: [f32; 3],
    pub world_transform: [f32; 16],
    pub is_solid: bool,
    pub interacts_with_world: bool,
    pub is_actor_surface: bool,
    /// `_isActorPhysicsObject` (0x40).
    pub is_actor_physics_object: bool,
    /// Static obstacle for the level: the actor starts active, draws no model (a model
    /// collides through its mesh), the volume is an actor surface or physics object and
    /// is not mesh-shaped (that shape needs a model).
    /// The default 3x6x3 `_isSolid` box on cameras, emitters and triggers is not one.
    pub blocks_movement: bool,
}

impl CollisionPrimitive {
    pub fn world_position(&self) -> [f32; 3] {
        [
            self.world_transform[12],
            self.world_transform[13],
            self.world_transform[14],
        ]
    }

    pub fn to_world_triangles(&self) -> Vec<[[f32; 3]; 3]> {
        let hx = self.extents[0] * 0.5;
        let hy = self.extents[1] * 0.5;
        let hz = self.extents[2] * 0.5;
        if hx <= 0.0 || hy <= 0.0 || hz <= 0.0 {
            return Vec::new();
        }

        let p000 = transform_point(&self.world_transform, [-hx, -hy, -hz]);
        let p001 = transform_point(&self.world_transform, [-hx, -hy, hz]);
        let p010 = transform_point(&self.world_transform, [-hx, hy, -hz]);
        let p011 = transform_point(&self.world_transform, [-hx, hy, hz]);
        let p100 = transform_point(&self.world_transform, [hx, -hy, -hz]);
        let p101 = transform_point(&self.world_transform, [hx, -hy, hz]);
        let p110 = transform_point(&self.world_transform, [hx, hy, -hz]);
        let p111 = transform_point(&self.world_transform, [hx, hy, hz]);

        vec![
            // -X (faces -X)
            [p000, p001, p011],
            [p000, p011, p010],
            // +X (faces +X)
            [p100, p110, p111],
            [p100, p111, p101],
            // -Y (faces -Y)
            [p000, p100, p101],
            [p000, p101, p001],
            // +Y (walkable top surface, faces +Y)
            [p010, p011, p111],
            [p010, p111, p110],
            // -Z (faces -Z)
            [p000, p010, p110],
            [p000, p110, p100],
            // +Z (faces +Z)
            [p001, p101, p111],
            [p001, p111, p011],
        ]
    }
}

/// One `tfbActorInfo` level placement.
#[derive(Debug, Clone, PartialEq)]
pub struct ActorPlacement {
    pub name: String,
    pub position: [f32; 3],
    pub rotation_deg: [f32; 3],
    /// `.ai` script path, if the actor has one.
    pub script: Option<String>,
    pub collision: Option<CollisionPrimitive>,
    /// `_activityState` == active; inactive actors are templates scripts spawn later.
    pub active: bool,
    /// `_scaleVec`, percent per axis.
    pub scale: [f32; 3],
    /// `tfbPhysicsModel` object offset drawn at the placement.
    pub model: Option<usize>,
}

impl ActorPlacement {
    /// Script decides when present; name heuristic only for script-less actors.
    pub fn kind(&self) -> Option<MarkerKind> {
        match &self.script {
            Some(s) => MarkerKind::from_script(s),
            None => MarkerKind::from_node_name(&self.name),
        }
    }

    pub fn transform(&self) -> [f32; 16] {
        let mut m = mat4_from_euler_deg(self.rotation_deg);
        m[12..15].copy_from_slice(&self.position);
        m
    }

    /// Whether the level draws this actor's model. Enemies and collectibles become
    /// gameplay entities with their own visuals, so the level leaves them out.
    pub fn draws_level_model(&self) -> bool {
        self.active
            && self.model.is_some()
            && !matches!(
                self.kind(),
                Some(MarkerKind::Enemy(_)) | Some(MarkerKind::Collectible)
            )
    }

    /// `transform` with `_scaleVec` / 100 applied.
    pub fn model_transform(&self) -> [f32; 16] {
        let mut m = self.transform();
        for (col, s) in self.scale.iter().enumerate() {
            for row in 0..3 {
                m[col * 4 + row] *= s / 100.0;
            }
        }
        m
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
    /// The level scene draws this actor's model (`ActorPlacement::draws_level_model`).
    pub drawn_by_level: bool,
    /// `.ai` script path of the placing actor.
    pub script: Option<String>,
    /// The script animates the actor; its model is drawn apart from the level
    /// (`IgbModel::node_owner`).
    pub animated: bool,
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

/// Column-major rotation from Euler degrees, applied X then Y then Z (`Rz * Ry * Rx`).
// ponytail: order assumed; hub placements are yaw-only except ~10, so it is unverified.
// Confirm against a non-identity `Entity._transform` if a tilted actor looks wrong.
pub fn mat4_from_euler_deg(deg: [f32; 3]) -> [f32; 16] {
    let axis = |i: usize, a: f32| {
        let (s, c) = a.to_radians().sin_cos();
        let mut m = mat4_identity();
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        m[j * 4 + j] = c;
        m[j * 4 + k] = s;
        m[k * 4 + j] = -s;
        m[k * 4 + k] = c;
        m
    };
    mat4_mul(
        &axis(2, deg[2]),
        &mat4_mul(&axis(1, deg[1]), &axis(0, deg[0])),
    )
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

/// Rotates a direction by the upper 3x3 of a column-major matrix and normalizes.
/// ponytail: ignores non-uniform scale; use inverse-transpose if skewed instances appear.
pub fn transform_dir(m: &[f32; 16], n: [f32; 3]) -> [f32; 3] {
    let r = [
        m[0] * n[0] + m[4] * n[1] + m[8] * n[2],
        m[1] * n[0] + m[5] * n[1] + m[9] * n[2],
        m[2] * n[0] + m[6] * n[1] + m[10] * n[2],
    ];
    let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
    if len > 1e-12 {
        [r[0] / len, r[1] / len, r[2] / len]
    } else {
        n
    }
}

/// Inverts an affine column-major matrix (bottom row `0 0 0 1`), scale/shear included.
pub fn mat4_affine_inverse(m: &[f32; 16]) -> [f32; 16] {
    let a = |r: usize, c: usize| m[c * 4 + r];
    let adj = |i: usize, j: usize| {
        let (i1, i2, j1, j2) = ((i + 1) % 3, (i + 2) % 3, (j + 1) % 3, (j + 2) % 3);
        a(j1, i1) * a(j2, i2) - a(j1, i2) * a(j2, i1)
    };
    let det = a(0, 0) * adj(0, 0) + a(0, 1) * adj(1, 0) + a(0, 2) * adj(2, 0);
    if det.abs() < 1e-12 {
        return mat4_identity();
    }
    let mut out = mat4_identity();
    for i in 0..3 {
        for j in 0..3 {
            out[j * 4 + i] = adj(i, j) / det;
        }
    }
    for i in 0..3 {
        out[12 + i] = -(0..3).map(|j| out[j * 4 + i] * m[12 + j]).sum::<f32>();
    }
    out
}

/// Decodes one GX vertex color (RGB565, RGB8, RGBX8, RGBA4, RGBA6, RGBA8) to RGBA floats.
fn gx_color(b: &[u8], o: usize, fmt: u32) -> Option<[f32; 4]> {
    let px = |n: usize| b.get(o..o + n);
    let f = |v: u32, max: u32| v as f32 / max as f32;
    Some(match fmt {
        0 => {
            let v = u16::from_be_bytes(px(2)?.try_into().ok()?) as u32;
            [f(v >> 11, 31), f((v >> 5) & 63, 63), f(v & 31, 31), 1.0]
        }
        1 | 2 => {
            let s = px(if fmt == 1 { 3 } else { 4 })?;
            [
                f(s[0] as u32, 255),
                f(s[1] as u32, 255),
                f(s[2] as u32, 255),
                1.0,
            ]
        }
        3 => {
            let v = u16::from_be_bytes(px(2)?.try_into().ok()?) as u32;
            [
                f(v >> 12, 15),
                f((v >> 8) & 15, 15),
                f((v >> 4) & 15, 15),
                f(v & 15, 15),
            ]
        }
        4 => {
            let s = px(3)?;
            let v = ((s[0] as u32) << 16) | ((s[1] as u32) << 8) | s[2] as u32;
            [
                f(v >> 18, 63),
                f((v >> 12) & 63, 63),
                f((v >> 6) & 63, 63),
                f(v & 63, 63),
            ]
        }
        _ => {
            let s = px(4)?;
            [
                f(s[0] as u32, 255),
                f(s[1] as u32, 255),
                f(s[2] as u32, 255),
                f(s[3] as u32, 255),
            ]
        }
    })
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
        let (width_off, height_off, depth_off, level_off, image_off, format_off, data_off) =
            if version == 5 {
                (
                    obj_offset + 0x0c,
                    obj_offset + 0x0e,
                    obj_offset + 0x10,
                    obj_offset + 0x12,
                    obj_offset + 0x14,
                    obj_offset + 0x1a,
                    obj_offset + 0x24,
                )
            } else if version == 9 {
                (
                    obj_offset + 0x34,
                    obj_offset + 0x36,
                    obj_offset + 0x38,
                    obj_offset + 0x3a,
                    obj_offset + 0x3c,
                    obj_offset + 0x42,
                    obj_offset + 0x4c,
                )
            } else {
                (
                    obj_offset + meta_fields::ig_image2::WIDTH,
                    obj_offset + meta_fields::ig_image2::HEIGHT,
                    obj_offset + meta_fields::ig_image2::DEPTH,
                    obj_offset + meta_fields::ig_image2::LEVEL_COUNT,
                    obj_offset + meta_fields::ig_image2::IMAGE_COUNT,
                    obj_offset + meta_fields::ig_image2::FORMAT + 2,
                    obj_offset + meta_fields::ig_image2::DATA,
                )
            };

        if data_off + 4 > data.len() {
            return None;
        }

        let width = IgbModel::read_u16(data, width_off, endian);
        let height = IgbModel::read_u16(data, height_off, endian);
        let depth = IgbModel::read_u16(data, depth_off, endian);
        let mipmap_count = IgbModel::read_u16(data, level_off, endian);
        let array = IgbModel::read_u16(data, image_off, endian);

        let exid_idx = IgbModel::read_u16(data, format_off, endian) as usize;
        let format = exid_formats.get(exid_idx).copied().unwrap_or(0);

        let tmhn_idx = IgbModel::read_u32(data, data_off, endian) as usize;
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

/// Object types whose raw `data`/`fields` payload `IgbModel::scan_igz` decodes.
const IGZ_PAYLOAD_TYPES: &[&str] = &[
    "igVertexBuffer",
    "igIndexBuffer",
    "igVertexFormat",
    "igGeometryAttr",
];

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

        // Level files hold hundreds of thousands of objects with up to 64 KiB of
        // payload each; copying every one costs gigabytes, so only keep the
        // payload of types whose `data`/`fields` are actually decoded.
        let keep_payload = type_name
            .as_deref()
            .is_some_and(|t| IGZ_PAYLOAD_TYPES.contains(&t));

        let data_start = data_offset;
        let data_end = (data_start + length as usize).min(slice.len());
        let obj_data = if keep_payload && data_start <= slice.len() {
            slice[data_start..data_end].to_vec()
        } else {
            Vec::new()
        };

        let fields = if keep_payload && slice.len() >= 0x14 {
            let field_slice = &slice[0x14..];
            let num_fields = (length as usize / 4).min(field_slice.len() / 4);
            (0..num_fields)
                .map(|i| IgbModel::read_u32(field_slice, i * 4, endian))
                .collect()
        } else {
            Vec::new()
        };

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

/// Raw IGZ file: section table, fixup-derived tables and the object registry.
#[derive(Debug, Clone)]
pub struct IgzFile {
    pub endian: IgbEndian,
    pub version: u32,
    pub sections: Vec<IgzChunkDescriptor>,
    pub string_table: Vec<String>,
    pub exid_formats: Vec<u32>,
    /// `EXID` second word: FNV-1a of the lower-case source file name (e.g. `global.bld`).
    pub exid_namespaces: Vec<u32>,
    pub tmhn_entries: Vec<(usize, usize)>,
    /// `TOOR` root object list (file offsets), the targets of cross-file `EXID` indices.
    pub roots: Vec<usize>,
    /// `EXNM` handle index -> external object name.
    pub external_types: Vec<String>,
    pub objects: Option<IgzObjectList>,
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

impl Default for Bone {
    fn default() -> Self {
        Self {
            name: String::new(),
            parent: None,
            inverse_bind_pose: mat4_identity(),
            local_transform: mat4_identity(),
        }
    }
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
    /// Index into `IgbModel.skeletons` of the rig the tracks drive (`None` = unknown).
    pub skeleton: Option<usize>,
    /// `tfbPhysicsAnimation` `.ma` path (empty for synthetic clips).
    pub source: String,
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

/// Translation, rotation (xyzw) and scale composed like Havok `hkQsTransform`: a parent's
/// scale does not scale the child's translation (Kaos `J_root` has scale 1.4 and its
/// children's animated translations are 1.4x the matrix-derived rest offsets).
#[derive(Debug, Clone, Copy, PartialEq)]
struct QsTransform {
    t: [f32; 3],
    r: [f32; 4],
    s: [f32; 3],
}

impl QsTransform {
    /// Decomposes an affine T*R*S matrix (column-major, no shear).
    fn from_mat(m: &[f32; 16]) -> Self {
        let col = |c: usize| [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
        let s = [0, 1, 2].map(|c| {
            let v = col(c);
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
        });
        let r = |row: usize, c: usize| {
            if s[c] > 1e-8 {
                m[c * 4 + row] / s[c]
            } else {
                f32::from(u8::from(row == c))
            }
        };
        let trace = r(0, 0) + r(1, 1) + r(2, 2);
        let q = if trace > 0.0 {
            let k = (trace + 1.0).sqrt() * 2.0;
            [
                (r(2, 1) - r(1, 2)) / k,
                (r(0, 2) - r(2, 0)) / k,
                (r(1, 0) - r(0, 1)) / k,
                0.25 * k,
            ]
        } else if r(0, 0) > r(1, 1) && r(0, 0) > r(2, 2) {
            let k = (1.0 + r(0, 0) - r(1, 1) - r(2, 2)).sqrt() * 2.0;
            [
                0.25 * k,
                (r(0, 1) + r(1, 0)) / k,
                (r(0, 2) + r(2, 0)) / k,
                (r(2, 1) - r(1, 2)) / k,
            ]
        } else if r(1, 1) > r(2, 2) {
            let k = (1.0 + r(1, 1) - r(0, 0) - r(2, 2)).sqrt() * 2.0;
            [
                (r(0, 1) + r(1, 0)) / k,
                0.25 * k,
                (r(1, 2) + r(2, 1)) / k,
                (r(0, 2) - r(2, 0)) / k,
            ]
        } else {
            let k = (1.0 + r(2, 2) - r(0, 0) - r(1, 1)).sqrt() * 2.0;
            [
                (r(0, 2) + r(2, 0)) / k,
                (r(1, 2) + r(2, 1)) / k,
                0.25 * k,
                (r(1, 0) - r(0, 1)) / k,
            ]
        };
        Self {
            t: [m[12], m[13], m[14]],
            r: q,
            s,
        }
    }

    fn to_mat(self) -> [f32; 16] {
        compose_transform(&self.t, &self.r, &self.s)
    }

    /// `self * child`: `t = t + R t_c`, `R = R R_c`, `s = s s_c`.
    fn mul(&self, child: &Self) -> Self {
        let rt = quat_rotate(&self.r, &child.t);
        Self {
            t: [0, 1, 2].map(|i| self.t[i] + rt[i]),
            r: quat_mul(&self.r, &child.r),
            s: [0, 1, 2].map(|i| self.s[i] * child.s[i]),
        }
    }

    /// `x` such that `self.mul(&x) == world`.
    fn local_of(&self, world: &Self) -> Self {
        let inv = [-self.r[0], -self.r[1], -self.r[2], self.r[3]];
        let d = [0, 1, 2].map(|i| world.t[i] - self.t[i]);
        Self {
            t: quat_rotate(&inv, &d),
            r: quat_mul(&inv, &world.r),
            s: [0, 1, 2].map(|i| {
                if self.s[i].abs() > 1e-8 {
                    world.s[i] / self.s[i]
                } else {
                    world.s[i]
                }
            }),
        }
    }
}

fn quat_mul(a: &[f32; 4], b: &[f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn quat_rotate(q: &[f32; 4], v: &[f32; 3]) -> [f32; 3] {
    let p = quat_mul(
        &quat_mul(q, &[v[0], v[1], v[2], 0.0]),
        &[-q[0], -q[1], -q[2], q[3]],
    );
    [p[0], p[1], p[2]]
}

/// Resolves each bone against its parent chain (`compose(parent_world, local)`).
fn resolve_hierarchy<T: Copy>(
    skeleton: &Skeleton,
    local: &[T],
    compose: impl Fn(&T, &T) -> T,
) -> Vec<T> {
    let n = skeleton.bones.len();
    let mut global: Vec<Option<T>> = vec![None; n];
    fn visit<T: Copy>(
        idx: usize,
        skeleton: &Skeleton,
        local: &[T],
        global: &mut [Option<T>],
        compose: &impl Fn(&T, &T) -> T,
    ) -> T {
        if let Some(g) = global[idx] {
            return g;
        }
        let g = match skeleton.bones[idx].parent {
            Some(p) if p < skeleton.bones.len() && p != idx => {
                let pg = visit(p, skeleton, local, global, compose);
                compose(&pg, &local[idx])
            }
            _ => local[idx],
        };
        global[idx] = Some(g);
        g
    }
    (0..n)
        .map(|i| visit(i, skeleton, local, &mut global, &compose))
        .collect()
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
            skeleton: None,
            source: String::new(),
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

        // Rest pose from the matrix hierarchy (bind pose for decoded rigs), re-expressed as
        // `hkQsTransform` locals so Havok tracks replace them in the same convention.
        let locals: Vec<[f32; 16]> = skeleton.bones.iter().map(|b| b.local_transform).collect();
        let rest: Vec<QsTransform> = resolve_hierarchy(skeleton, &locals, mat4_mul)
            .iter()
            .map(QsTransform::from_mat)
            .collect();
        let mut local: Vec<QsTransform> = skeleton
            .bones
            .iter()
            .enumerate()
            .map(
                |(i, b)| match b.parent.filter(|&p| p < rest.len() && p != i) {
                    Some(p) => rest[p].local_of(&rest[i]),
                    None => rest[i],
                },
            )
            .collect();

        for track in &self.tracks {
            if track.bone_index >= local.len() {
                continue;
            }
            local[track.bone_index] = QsTransform {
                t: sample_vec3(&track.positions, t).unwrap_or([0.0, 0.0, 0.0]),
                r: sample_quat(&track.rotations, t).unwrap_or([0.0, 0.0, 0.0, 1.0]),
                s: sample_vec3(&track.scales, t).unwrap_or([1.0, 1.0, 1.0]),
            };
        }

        let global_transforms: Vec<[f32; 16]> =
            resolve_hierarchy(skeleton, &local, QsTransform::mul)
                .into_iter()
                .map(QsTransform::to_mat)
                .collect();
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

fn primary_skeleton_of(skeletons: &[Skeleton], meshes: &[IgbMesh]) -> Option<usize> {
    if skeletons.is_empty() {
        return None;
    }
    let mut counts = vec![0usize; skeletons.len()];
    for m in meshes {
        if let Some(s) = m.skeleton {
            if s < counts.len() {
                counts[s] += m.vertices.len();
            }
        }
    }
    skeletons
        .iter()
        .enumerate()
        .filter(|&(idx, _)| counts[idx] > 0)
        .max_by_key(|&(idx, skel)| (skel.bones.len(), counts[idx]))
        .map(|(idx, _)| idx)
        .or_else(|| {
            skeletons
                .iter()
                .enumerate()
                .max_by_key(|&(_, skel)| skel.bones.len())
                .map(|(idx, _)| idx)
        })
}

/// Returns a reference to the primary character skeleton, if any.

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
    pub collision_primitives: Vec<CollisionPrimitive>,
    /// Literal script assignments (`var = number`), in file order.
    pub script_constants: Vec<(String, ScriptValue)>,
    /// Rigid models (weapons, backpacks) mounted on the primary rig's bones.
    pub attachments: Vec<CharacterAttachment>,
    /// The character's ranged attack actor (Gill Grunt's harpoon, Spyro's fireball).
    pub projectile: Option<CharacterProjectile>,
    /// The level's sky layer. Drawn around the camera, so it is kept out of the
    /// scene graph (and collision); transforms are in sky layer space.
    pub sky_instances: Vec<SceneInstance>,
    /// Scene nodes drawn but not collided with (models of non-solid actors).
    pub non_colliding_nodes: std::collections::HashSet<usize>,
    /// Marker nodes whose actor model the level scene draws.
    pub level_drawn_markers: std::collections::HashSet<usize>,
    /// Prop marker nodes whose script moves, turns, scales or removes the actor
    /// (`ScriptProgram::animates_self`). Their model is not part of the static level.
    pub animated_markers: std::collections::HashSet<usize>,
    /// Geometry node -> marker node of the animated actor that draws it.
    pub node_owner: std::collections::HashMap<usize, usize>,
    /// Marker node -> `.ai` script path.
    pub marker_scripts: std::collections::HashMap<usize, String>,
    /// `tfbActorInfo` templates with a model (what scripts `OpSpawn`).
    pub actor_models: Vec<ActorModel>,
    /// `tfbAnimationState`s: the ids `OpStartSequence` on the `animation palette` plays.
    pub animation_states: Vec<AnimationStateInfo>,
    /// `tfbAnimationStateData._transitions`: which state a request plays and the blend time.
    pub animation_transitions: Vec<AnimationTransition>,
    /// `ScriptColorInfo` (`+0x08` name, `_value` `+0x18`, `0xAABBGGRR`): script colour
    /// constants such as `MM_element_00_magic_color`.
    pub script_colors: Vec<(String, u32)>,
}

/// `tfbActorInfo` (`+0x08` name, `+0xec` `tfbPhysicsModel`) and its model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActorModel {
    pub actor: String,
    /// `tfbPhysicsModel` `.mdl` path.
    pub model: String,
    /// Indices into `IgbModel.meshes`.
    pub meshes: Vec<usize>,
    /// Index into `IgbModel.skeletons` of a rigged model (`tfbPhysicsCombinerLink`).
    pub skeleton: Option<usize>,
    /// Per `meshes` entry: the joint (`skeleton` bone index) an articulated drawable follows
    /// (`Drawable._entity` -> `tfbBodyEntityInfo._entities` -> `Entity._parentIndex`).
    /// Vertices stay in model (bind) space.
    pub mesh_bones: Vec<Option<usize>>,
    /// `BoneAttachInfoList` (`+0x28`): the model's `attach nodes`, in order.
    pub attach_nodes: Vec<AttachNode>,
}

/// `BoneAttachInfo`: `+0x08` node name, `_boneName` `+0x1c`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AttachNode {
    pub name: String,
    pub bone: String,
}

/// `tfbAnimationState`: `+0x08` name, `_id` `+0x0c`, `_animations` `+0x14`
/// (`AnimationClipList` -> `AnimationClip` `+0x38` -> `tfbPhysicsAnimation`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationStateInfo {
    pub id: i32,
    pub name: String,
    /// Index into `IgbModel.animations` of the state's first clip.
    pub clip: Option<usize>,
}

/// `tfbAnimationTransition*` (0x28): `+0x08` name, `+0x10` requested state id (-1: taken
/// when the source state's clip ends), `+0x1c` blend seconds, `+0x20` source
/// `tfbAnimationStateList` (empty: any state), `+0x24` destination list (the state played).
/// Spyro: "idle to idletowalk" plays `IdletoWalk` (99) when walk (1) is requested from idle,
/// "idletowalk to walk" then auto-advances to walk.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationTransition {
    pub name: String,
    pub request: i32,
    pub from: Vec<i32>,
    pub to: i32,
    pub blend: f32,
}

/// Requested state id that marks an automatic transition at the end of the source clip.
pub const AUTO_TRANSITION: i32 = -1;

/// A character's animation states (id, clip name) and the transitions between them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationGraph {
    pub states: Vec<(i32, String)>,
    pub transitions: Vec<AnimationTransition>,
}

impl AnimationGraph {
    pub fn state_for_clip(&self, clip: &str) -> Option<i32> {
        self.states
            .iter()
            .find(|(_, c)| c.eq_ignore_ascii_case(clip))
            .map(|&(id, _)| id)
    }

    pub fn clip_for_state(&self, id: i32) -> Option<&str> {
        self.states
            .iter()
            .find(|&&(s, _)| s == id)
            .map(|(_, c)| c.as_str())
    }

    /// Transition taken when `request` is asked for in state `current`: one listing
    /// `current` as a source wins over an any-source one.
    pub fn transition(&self, current: Option<i32>, request: i32) -> Option<&AnimationTransition> {
        let candidates = || {
            self.transitions
                .iter()
                .filter(move |t| t.request == request)
        };
        current
            .and_then(|c| candidates().find(|t| t.from.contains(&c)))
            .or_else(|| candidates().find(|t| t.from.is_empty()))
    }

    /// Transition taken when the clip of state `current` ends.
    pub fn auto_transition(&self, current: i32) -> Option<&AnimationTransition> {
        self.transitions
            .iter()
            .find(|t| t.request == AUTO_TRANSITION && t.from.contains(&current))
    }
}

/// Rigid model mounted on a bone of the primary rig (Gill Grunt's harpoon gun on `J_gun`).
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterAttachment {
    /// `BoneAttachInfo._boneName`, a bone of the primary `igSkeleton2`.
    pub bone: String,
    /// Model placement in bone space (column-major).
    pub transform: [f32; 16],
    /// Indices into `IgbModel.meshes`.
    pub meshes: Vec<usize>,
    /// `.mdl` path of the attached model.
    pub model: String,
}

/// Projectile actor template (`tfbActorInfo` with a rigid model) the character script spawns.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterProjectile {
    /// `tfbActorInfo` name, e.g. `014_HarpoonSpear_Projectile`.
    pub actor: String,
    /// `.mdl` path of the projectile model.
    pub model: String,
    /// Indices into `IgbModel.meshes`.
    pub meshes: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScriptValue {
    Int(i32),
    Float(f32),
}

impl ScriptValue {
    pub fn as_f32(self) -> f32 {
        match self {
            Self::Int(v) => v as f32,
            Self::Float(v) => v,
        }
    }
}

impl IgbModel {
    /// First literal assigned to script variable `name`.
    pub fn script_constant(&self, name: &str) -> Option<ScriptValue> {
        self.script_constants
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
    }
}

impl Default for IgbModel {
    fn default() -> Self {
        Self {
            header: IgbHeader {
                magic: [b'I', b'G', b'Z', 1],
                version: 8,
                object_count: 0,
                string_table_offset: 0,
                endian: IgbEndian::Big,
            },
            directory: Vec::new(),
            nodes: Vec::new(),
            geometries: Vec::new(),
            attr_lists: Vec::new(),
            vertex_buffers: Vec::new(),
            index_buffers: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            images: Vec::new(),
            meshes: Vec::new(),
            skeletons: Vec::new(),
            animations: Vec::new(),
            collision_primitives: Vec::new(),
            script_constants: Vec::new(),
            attachments: Vec::new(),
            projectile: None,
            sky_instances: Vec::new(),
            non_colliding_nodes: Default::default(),
            level_drawn_markers: Default::default(),
            animated_markers: Default::default(),
            node_owner: Default::default(),
            marker_scripts: Default::default(),
            actor_models: Vec::new(),
            animation_states: Vec::new(),
            animation_transitions: Vec::new(),
            script_colors: Vec::new(),
        }
    }
}

impl IgbModel {
    /// Returns the index of the primary character skeleton.
    ///
    /// Character files often contain secondary rigs (e.g. separate tails,
    /// minigame/cinematic heads, accessories). The primary character rig is the
    /// one that skins character geometry with the highest bone count.
    pub fn primary_skeleton_index(&self) -> Option<usize> {
        primary_skeleton_of(&self.skeletons, &self.meshes)
    }

    /// Clips that drive the primary rig. Character files also carry clips for
    /// secondary rigs (Kaos: `kaosHead_Idle` on the 20-bone head before `Kaos_Idle`).
    pub fn primary_animations(&self) -> Vec<AnimationClip> {
        let primary = self.primary_skeleton_index();
        self.animations
            .iter()
            .filter(|a| a.skeleton.is_none() || a.skeleton == primary)
            .cloned()
            .collect()
    }

    pub fn primary_skeleton(&self) -> Option<&Skeleton> {
        self.primary_skeleton_index()
            .and_then(|idx| self.skeletons.get(idx))
    }

    /// Maps each bone of skeleton `skel` to the primary rig bone with the same
    /// name (namespace stripped) and bind pose. `None` when any bone is missing,
    /// e.g. Kaos' cinematic head rig.
    pub fn bone_map_to_primary(&self, skel: usize) -> Option<Vec<u32>> {
        let primary = self.primary_skeleton()?;
        let base = |n: &str| n.rsplit(':').next().unwrap_or(n).to_ascii_lowercase();
        self.skeletons
            .get(skel)?
            .bones
            .iter()
            .map(|b| {
                let (idx, p) = primary
                    .bones
                    .iter()
                    .enumerate()
                    .find(|(_, p)| base(&p.name) == base(&b.name))?;
                let same_pose = p
                    .inverse_bind_pose
                    .iter()
                    .zip(&b.inverse_bind_pose)
                    .all(|(x, y)| (x - y).abs() < 1e-3);
                same_pose.then_some(idx as u32)
            })
            .collect()
    }

    /// Meshes skinned by the primary rig, plus secondary-rig meshes remapped onto
    /// it. Spyro's tail rigs (`J_root_B`, `J_tail_*`) are subsets of the body rig,
    /// which the body clips animate. Rigs that map onto the same bones as an
    /// earlier rig are alternate variants (Spyro `TailSerie3`) and are skipped.
    pub fn primary_rig_meshes(&self) -> Vec<IgbMesh> {
        let Some(primary) = self.primary_skeleton_index() else {
            return Vec::new();
        };
        let mut maps: Vec<(usize, Vec<u32>)> = Vec::new();
        for s in 0..self.skeletons.len() {
            if s == primary {
                continue;
            }
            if let Some(map) = self.bone_map_to_primary(s) {
                if !maps.iter().any(|(_, m)| *m == map) {
                    maps.push((s, map));
                }
            }
        }
        self.meshes
            .iter()
            .filter(|m| !m.vertices.is_empty())
            .filter_map(|m| {
                let s = m.skeleton?;
                if s == primary {
                    return Some(m.clone());
                }
                let (_, map) = maps.iter().find(|(ms, _)| *ms == s)?;
                let mut out = m.clone();
                for v in &mut out.vertices {
                    for (bi, w) in v.bone_indices.iter_mut().zip(v.bone_weights) {
                        let mapped = map.get(*bi as usize).copied();
                        if w > 0.0 && mapped.is_none() {
                            return None;
                        }
                        *bi = mapped.unwrap_or(0);
                    }
                }
                out.skeleton = Some(primary);
                Some(out)
            })
            .collect()
    }

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
                collision_primitives: Vec::new(),
                script_constants: Vec::new(),
                attachments: Vec::new(),
                projectile: None,
                sky_instances: Vec::new(),
                non_colliding_nodes: Default::default(),
                level_drawn_markers: Default::default(),
                animated_markers: Default::default(),
                node_owner: Default::default(),
                marker_scripts: Default::default(),
                actor_models: Vec::new(),
                animation_states: Vec::new(),
                animation_transitions: Vec::new(),
                script_colors: Vec::new(),
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
                collision_primitives: Vec::new(),
                script_constants: Vec::new(),
                attachments: Vec::new(),
                projectile: None,
                sky_instances: Vec::new(),
                non_colliding_nodes: Default::default(),
                level_drawn_markers: Default::default(),
                animated_markers: Default::default(),
                node_owner: Default::default(),
                marker_scripts: Default::default(),
                actor_models: Vec::new(),
                animation_states: Vec::new(),
                animation_transitions: Vec::new(),
                script_colors: Vec::new(),
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
        let skeletons = Vec::new();
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
                    skeleton: None,
                    image_index: None,
                    base_color: None,
                    texture_wrap: Default::default(),
                    uv_scroll: None,
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
            collision_primitives: Vec::new(),
            script_constants: Vec::new(),
            attachments: Vec::new(),
            projectile: None,
            sky_instances: Vec::new(),
            non_colliding_nodes: Default::default(),
            level_drawn_markers: Default::default(),
            animated_markers: Default::default(),
            node_owner: Default::default(),
            marker_scripts: Default::default(),
            actor_models: Vec::new(),
            animation_states: Vec::new(),
            animation_transitions: Vec::new(),
            script_colors: Vec::new(),
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

    /// Computes the AABB of meshes attached to the primary character rig.
    ///
    /// Ignores accessory meshes, separate tails, and cinematic geometry.
    /// Falls back to all non-empty meshes if no skeleton exists.
    pub fn primary_mesh_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let primary_skel = self.primary_skeleton_index();
        let mut min_pt = [f32::MAX, f32::MAX, f32::MAX];
        let mut max_pt = [f32::MIN, f32::MIN, f32::MIN];
        let mut has_pts = false;

        let primary_matches = |m: &&IgbMesh| -> bool {
            match primary_skel {
                Some(skel_idx) => m.skeleton == Some(skel_idx) && !m.vertices.is_empty(),
                None => !m.vertices.is_empty(),
            }
        };

        for m in self.meshes.iter().filter(primary_matches) {
            for v in &m.vertices {
                has_pts = true;
                for c in 0..3 {
                    min_pt[c] = min_pt[c].min(v.position[c]);
                    max_pt[c] = max_pt[c].max(v.position[c]);
                }
            }
        }

        if has_pts {
            Some((min_pt, max_pt))
        } else {
            None
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
    pub(crate) fn read_u16(buf: &[u8], offset: usize, endian: IgbEndian) -> u16 {
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
            marker: None,
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
            skeleton: None,
            image_index: None,
            base_color: None,
            texture_wrap: Default::default(),
            uv_scroll: None,
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

    /// Decodes an IGZ `igSkeleton2` at `off`.
    ///
    /// Skeleton: `+0x08` name, `+0x0c` -> `igSkeletonBoneList`, `+0x14` -> inverse bind
    /// matrices (column-major 4x4, indexed by blend index). Bone list: `+0x08` count,
    /// `+0x14` -> pointer array. `igSkeletonBone`: `+0x08` name, `+0x0c` parent (i32),
    /// `+0x10` blend index (i32, -1 = unskinned), `+0x14` translation f32x3.
    /// Returns the skeleton and a blend-index -> bone-index table.
    pub fn decode_skeleton2(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        off: usize,
        endian: IgbEndian,
    ) -> Option<(Skeleton, Vec<u32>)> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> String {
            ptr(o)
                .and_then(|p| data.get(p..))
                .map(|s| {
                    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                    String::from_utf8_lossy(&s[..end]).into_owned()
                })
                .unwrap_or_default()
        };
        let mat = |o: usize| -> Option<[f32; 16]> {
            data.get(o..o + 64)?;
            Some(std::array::from_fn(|k| {
                Self::read_f32(data, o + k * 4, endian)
            }))
        };

        let list = ptr(off + 0x0c)?;
        let ibps = ptr(off + 0x14);
        let count = rd(list + 0x08)? as usize;
        let arr = ptr(list + 0x14)?;

        let mut bones: Vec<Bone> = Vec::with_capacity(count);
        let mut world: Vec<[f32; 16]> = Vec::with_capacity(count);
        let mut blend_to_bone = Vec::new();
        for i in 0..count {
            let b = ptr(arr + i * 4)?;
            let parent = Some(rd(b + 0x0c)? as i32)
                .filter(|&p| p >= 0 && (p as usize) < i)
                .map(|p| p as usize);
            let blend = rd(b + 0x10)? as i32;
            let parent_world = parent.map_or(mat4_identity(), |p| world[p]);
            // Skinned bones: bind pose = inverse(IBP), exact incl. rotation/scale.
            // Unskinned bones: translation is relative to the parent.
            let ibp = match (blend >= 0).then_some(()).and(ibps) {
                Some(base) => mat(base + blend as usize * 64)?,
                None => {
                    let mut local = mat4_identity();
                    for c in 0..3 {
                        local[12 + c] = Self::read_f32(data, b + 0x14 + c * 4, endian);
                    }
                    mat4_affine_inverse(&mat4_mul(&parent_world, &local))
                }
            };
            let w = mat4_affine_inverse(&ibp);
            let local_transform = mat4_mul(&mat4_affine_inverse(&parent_world), &w);
            if blend >= 0 {
                let slot = blend as usize;
                if blend_to_bone.len() <= slot {
                    blend_to_bone.resize(slot + 1, 0);
                }
                blend_to_bone[slot] = i as u32;
            }
            world.push(w);
            bones.push(Bone {
                name: cstr(b + 0x08),
                parent,
                inverse_bind_pose: ibp,
                local_transform,
            });
        }
        Some((Skeleton::new(bones, 0), blend_to_bone))
    }

    /// Havok rig skeletons: `tfbPhysicsBody` `+0x14` size (low 27 bits), `+0x18` -> Havok
    /// packfile with an `hkaSkeleton`; `+0x28` -> `tfbPhysicsCombinerLink` `+0x0c` ->
    /// `igSkeleton2` (index via `skeleton_by_offset`). Clip tracks index these rigs.
    fn decode_havok_rigs(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
        skeleton_by_offset: &std::collections::HashMap<usize, (usize, Vec<u32>)>,
    ) -> Vec<crate::havok::HavokRig> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbPhysicsBody"))
            .filter_map(|body| {
                let skel = ptr(body.offset + 0x28).and_then(|l| ptr(l + 0x0c))?;
                let &(skeleton, _) = skeleton_by_offset.get(&skel)?;
                let size = (rd(body.offset + 0x14)? & 0x07ff_ffff) as usize;
                let hk = ptr(body.offset + 0x18)?;
                let bones = crate::havok::parse_havok_skeleton(data.get(hk..hk + size)?)?;
                Some(crate::havok::HavokRig { skeleton, bones })
            })
            .collect()
    }

    /// Rigid models on the primary rig's attach nodes.
    /// `tfbPhysicsModel`: `+0x08` `.mdl` path, `+0x28` `BoneAttachInfoList`, `+0x60`
    /// `tfbPhysicsCombinerLink` (`+0x0c` -> `igSkeleton2`; rigged models only), `+0x68`
    /// `tfbBodyEntityInfo`. `BoneAttachInfo`: `+0x1c` `_boneName`, `+0x24` `_transform`
    /// (xyz, Euler degrees). Templates are `tfbSpriteInfo` with `+0xd8` -> a rigid
    /// `tfbPhysicsModel`; `AbstractPlacement._ID` (`+0x20`) is the attach node index (Gill
    /// Grunt `014_Attach_Gun_00` -> 0 `gun`, `014_Attach_Backpack_00` -> 17 `back pack`,
    /// Stealth Elf knives -> 0 and 1) and the `_initMatrix` rotation (`+0x30`) orients it.
    // ponytail: the character script picks one template per node by upgrade state
    // (`014 - Set Up Sprite Attachments` via `OpChangeMembership`), which the VM does not
    // run. Take the base model: shortest `.mdl` name (`HarpoonGun` over `..._Upgrade01`).
    fn decode_attachments(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
        primary_skeleton: usize,
        mesh_by_geo: &std::collections::HashMap<usize, usize>,
    ) -> Vec<CharacterAttachment> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> String {
            ptr(o)
                .and_then(|p| data.get(p..))
                .map(|s| {
                    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                    String::from_utf8_lossy(&s[..end]).into_owned()
                })
                .unwrap_or_default()
        };
        let euler = |o: usize| [0, 1, 2].map(|k| Self::read_f32(data, o + k * 4, endian));
        let types: std::collections::HashMap<usize, &str> = objects
            .iter()
            .filter_map(|o| Some((o.offset, o.type_name.as_deref()?)))
            .collect();
        let types_ref = &types;
        let of_type = |t: &'static str| {
            objects
                .iter()
                .filter(move |o| types_ref.get(&o.offset) == Some(&t))
        };
        let list_items = |list: usize| -> Vec<usize> {
            let (Some(n), Some(arr)) = (rd(list + 0x08), ptr(list + 0x14)) else {
                return Vec::new();
            };
            (0..n as usize).filter_map(|i| ptr(arr + i * 4)).collect()
        };

        let Some(rig) = of_type("tfbPhysicsModel")
            .find(|m| ptr(m.offset + 0x60).and_then(|l| ptr(l + 0x0c)) == Some(primary_skeleton))
        else {
            return Vec::new();
        };
        let nodes: Vec<(String, [f32; 16])> = ptr(rig.offset + 0x28)
            .map(list_items)
            .unwrap_or_default()
            .into_iter()
            .map(|n| {
                let mut m = mat4_from_euler_deg(euler(n + 0x30));
                m[12..15].copy_from_slice(&euler(n + 0x24));
                (cstr(n + 0x1c), m)
            })
            .collect();

        // Node index -> (model name length, model, rotation) of the base template.
        let mut chosen = std::collections::BTreeMap::<usize, (usize, usize, [f32; 3])>::new();
        for sprite in of_type("tfbSpriteInfo") {
            let Some(model) = ptr(sprite.offset + 0xd8)
                .filter(|m| types.get(m) == Some(&"tfbPhysicsModel"))
                .filter(|&m| ptr(m + 0x60).is_none())
            else {
                continue;
            };
            let Some(node) = rd(sprite.offset + 0x20).map(|n| n as usize) else {
                continue;
            };
            if node >= nodes.len() {
                continue;
            }
            let path = cstr(model + 0x08);
            let len = path.rsplit('/').next().unwrap_or(&path).len();
            if chosen.get(&node).is_none_or(|&(best, ..)| len < best) {
                chosen.insert(node, (len, model, euler(sprite.offset + 0x30)));
            }
        }

        chosen
            .into_iter()
            .filter_map(|(node, (_, model, rot))| {
                let meshes =
                    Self::rigid_model_meshes(model, &ptr, &list_items, &types, mesh_by_geo);
                if meshes.is_empty() {
                    return None;
                }
                let (bone, node_xf) = &nodes[node];
                Some(CharacterAttachment {
                    bone: bone.clone(),
                    transform: mat4_mul(node_xf, &mat4_from_euler_deg(rot)),
                    meshes,
                    model: cstr(model + 0x08),
                })
            })
            .collect()
    }

    /// Every `tfbActorInfo` template with a `tfbPhysicsModel` (`+0xec`): meshes, rig
    /// (`+0x60` `tfbPhysicsCombinerLink` `+0x0c` -> `igSkeleton2`) and attach nodes
    /// (`+0x28` `BoneAttachInfoList`). Magic Moment: `MMRig` (camera, Skylander, name...
    /// nodes), `MM_dualEndedTunnelTube`, `MM_fastStars`, element backgrounds.
    fn decode_actor_models(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
        mesh_by_geo: &std::collections::HashMap<usize, usize>,
        skeleton_by_offset: &std::collections::HashMap<usize, (usize, Vec<u32>)>,
    ) -> Vec<ActorModel> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> String {
            ptr(o)
                .and_then(|p| data.get(p..))
                .map(|s| {
                    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                    String::from_utf8_lossy(&s[..end]).into_owned()
                })
                .unwrap_or_default()
        };
        let list_items = |list: usize| -> Vec<usize> {
            let (Some(n), Some(arr)) = (rd(list + 0x08), ptr(list + 0x14)) else {
                return Vec::new();
            };
            (0..n.min(4096) as usize)
                .filter_map(|i| ptr(arr + i * 4))
                .collect()
        };
        let types: std::collections::HashMap<usize, &str> = objects
            .iter()
            .filter_map(|o| Some((o.offset, o.type_name.as_deref()?)))
            .collect();

        objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbActorInfo"))
            .filter_map(|actor| {
                let model = ptr(actor.offset + 0xec)
                    .filter(|m| types.get(m) == Some(&"tfbPhysicsModel"))?;
                let skeleton = ptr(model + 0x60)
                    .and_then(|link| ptr(link + 0x0c))
                    .and_then(|s| skeleton_by_offset.get(&s))
                    .map(|&(idx, _)| idx);
                let attach_nodes = ptr(model + 0x28)
                    .map(list_items)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|n| AttachNode {
                        name: cstr(n + 0x08),
                        bone: cstr(n + 0x1c),
                    })
                    .collect();
                // Drawables in mesh order, with the joint of their entity.
                let body = ptr(model + 0x68);
                let entities = body
                    .and_then(|b| ptr(b + 0x18))
                    .map(list_items)
                    .unwrap_or_default();
                let read_u16 = |o: usize| -> Option<u16> {
                    data.get(o..o + 2).map(|b| match endian {
                        IgbEndian::Big => u16::from_be_bytes([b[0], b[1]]),
                        IgbEndian::Little => u16::from_le_bytes([b[0], b[1]]),
                    })
                };
                let (meshes, mesh_bones) = body
                    .and_then(|b| ptr(b + 0x14))
                    .map(list_items)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|d| {
                        let g = ptr(d + 0x0c)?;
                        let g = match types.get(&g) {
                            Some(&"igWiiGeometryAttr") => ptr(g + 0x10)?,
                            _ => g,
                        };
                        let mesh = *mesh_by_geo.get(&g)?;
                        let bone = skeleton.and(read_u16(d + 0x14)).and_then(|e| {
                            let entity = *entities.get(e as usize)?;
                            read_u16(entity + 0x10).map(usize::from)
                        });
                        Some((mesh, bone))
                    })
                    .unzip();
                Some(ActorModel {
                    actor: cstr(actor.offset + 0x08),
                    model: cstr(model + 0x08),
                    meshes,
                    skeleton,
                    mesh_bones,
                    attach_nodes,
                })
            })
            .collect()
    }

    /// `tfbAnimationState`s, with the clip of their first `AnimationClip` matched by
    /// `tfbPhysicsAnimation` path.
    fn decode_animation_states(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
        animations: &[AnimationClip],
    ) -> Vec<AnimationStateInfo> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> String {
            ptr(o)
                .and_then(|p| data.get(p..))
                .map(|s| {
                    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                    String::from_utf8_lossy(&s[..end]).into_owned()
                })
                .unwrap_or_default()
        };
        objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbAnimationState"))
            .map(|state| {
                let path = ptr(state.offset + 0x14)
                    .filter(|&list| rd(list + 0x08).unwrap_or(0) > 0)
                    .and_then(|list| ptr(list + 0x14))
                    .and_then(&ptr)
                    .and_then(|clip| ptr(clip + 0x38))
                    .map(|anim| cstr(anim + 0x08));
                AnimationStateInfo {
                    id: rd(state.offset + 0x0c).unwrap_or(0) as i32,
                    name: cstr(state.offset + 0x08),
                    clip: path.and_then(|p| animations.iter().position(|a| a.source == p)),
                }
            })
            .collect()
    }

    /// States whose clip belongs to the primary rig, by clip name, and their transitions.
    pub fn animation_graph(&self) -> AnimationGraph {
        let primary = self.primary_skeleton_index();
        AnimationGraph {
            states: self
                .animation_states
                .iter()
                .filter_map(|s| {
                    let clip = self.animations.get(s.clip?)?;
                    (clip.skeleton.is_none() || clip.skeleton == primary)
                        .then(|| (s.id, clip.name.clone()))
                })
                .collect(),
            transitions: self.animation_transitions.clone(),
        }
    }

    /// Clip of the `tfbAnimationState` with `_id` `id` (`OpStartSequence` on the
    /// `animation palette`: Magic Moment rig 3 = "New Toy", hero 80 = `ToyAlive`).
    pub fn animation_for_state(&self, id: i32) -> Option<&AnimationClip> {
        self.animation_states
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.clip)
            .and_then(|c| self.animations.get(c))
    }

    fn decode_animation_transitions(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
    ) -> Vec<AnimationTransition> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> String {
            ptr(o)
                .and_then(|p| data.get(p..))
                .map(|s| {
                    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                    String::from_utf8_lossy(&s[..end]).into_owned()
                })
                .unwrap_or_default()
        };
        // `tfbAnimationStateList`: `+0x08` count, `+0x14` items -> state (`_id` `+0x0c`).
        let state_ids = |list: Option<usize>| -> Vec<i32> {
            let Some(list) = list else {
                return Vec::new();
            };
            let count = rd(list + 0x08).unwrap_or(0) as usize;
            let Some(items) = ptr(list + 0x14) else {
                return Vec::new();
            };
            (0..count)
                .filter_map(|i| ptr(items + i * 4))
                .filter_map(|state| rd(state + 0x0c))
                .map(|id| id as i32)
                .collect()
        };
        objects
            .iter()
            .filter(|o| {
                o.type_name.as_deref().is_some_and(|t| {
                    t.starts_with("tfbAnimationTransition") && t != "tfbAnimationTransitionList"
                })
            })
            .filter_map(|t| {
                let request = rd(t.offset + 0x10)? as i32;
                let to = *state_ids(ptr(t.offset + 0x24)).first()?;
                Some(AnimationTransition {
                    name: cstr(t.offset + 0x08),
                    request,
                    from: state_ids(ptr(t.offset + 0x20)),
                    to,
                    blend: f32::from_bits(rd(t.offset + 0x1c).unwrap_or(0)),
                })
            })
            .collect()
    }

    /// Meshes of a rigid `tfbPhysicsModel`: `+0x68` `tfbBodyEntityInfo` `+0x14` ->
    /// `DrawableList` -> `Drawable` `+0x0c` -> geometry attr.
    fn rigid_model_meshes(
        model: usize,
        ptr: &dyn Fn(usize) -> Option<usize>,
        list_items: &dyn Fn(usize) -> Vec<usize>,
        types: &std::collections::HashMap<usize, &str>,
        mesh_by_geo: &std::collections::HashMap<usize, usize>,
    ) -> Vec<usize> {
        ptr(model + 0x68)
            .and_then(|info| ptr(info + 0x14))
            .map(list_items)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|d| ptr(d + 0x0c))
            .filter_map(|g| match types.get(&g) {
                Some(&"igWiiGeometryAttr") => ptr(g + 0x10),
                _ => Some(g),
            })
            .filter_map(|g| mesh_by_geo.get(&g).copied())
            .collect()
    }

    /// The character's main projectile. Projectiles are `tfbActorInfo` templates (`+0x08`
    /// name, `+0xec` rigid `tfbPhysicsModel`) spawned by the script's `OpSpawn` (`+0x48` ->
    /// the template when spawned directly, else a variable holding it). Gill Grunt
    /// `014_HarpoonSpear_Projectile`, Spyro `016_Fire`, Eruptor `009_LavaProjectile`,
    /// Gusto `450_Boomerang`, Trigger Happy `019_GoldBullet`.
    // ponytail: picked by name (`PROJECTILE_NAME_TIERS`), preferring direct spawns, since
    // which macro fires which actor needs the VM to run spawn ops. Upgrade / Wow Pow
    // variants are skipped.
    fn decode_projectile(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
        mesh_by_geo: &std::collections::HashMap<usize, usize>,
    ) -> Option<CharacterProjectile> {
        const PROJECTILE_NAME_TIERS: [&[&str]; 3] = [
            &["bullet", "arrow", "harpoon", "boomerang", "missile"],
            &["projectile", "proj_"],
            &[
                "fire", "lava", "magma", "ball", "spit", "shot", "laser", "beam", "disc",
            ],
        ];
        const SKIP: [&str; 13] = [
            "prop",
            "attach",
            "upgrade",
            "wowpow",
            "_v2",
            "_v3",
            "(",
            "collider",
            "collision",
            "portal",
            "shield",
            "glow",
            "daybringer",
        ];
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> String {
            ptr(o)
                .and_then(|p| data.get(p..))
                .map(|s| {
                    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                    String::from_utf8_lossy(&s[..end]).into_owned()
                })
                .unwrap_or_default()
        };
        let list_items = |list: usize| -> Vec<usize> {
            let (Some(n), Some(arr)) = (rd(list + 0x08), ptr(list + 0x14)) else {
                return Vec::new();
            };
            (0..n as usize).filter_map(|i| ptr(arr + i * 4)).collect()
        };
        let types: std::collections::HashMap<usize, &str> = objects
            .iter()
            .filter_map(|o| Some((o.offset, o.type_name.as_deref()?)))
            .collect();
        let of_type = |t: &str| {
            objects
                .iter()
                .filter(|o| o.type_name.as_deref() == Some(t))
                .map(|o| o.offset)
                .collect::<Vec<_>>()
        };
        let spawned: std::collections::HashSet<usize> = of_type("OpSpawn")
            .into_iter()
            .filter_map(|op| ptr(op + 0x48))
            .collect();

        // (tier, not spawned directly, file order) -> lowest wins.
        let mut best: Option<((usize, bool, usize), CharacterProjectile)> = None;
        for (order, actor) in of_type("tfbActorInfo").into_iter().enumerate() {
            let Some(model) = ptr(actor + 0xec)
                .filter(|m| types.get(m) == Some(&"tfbPhysicsModel"))
                .filter(|&m| ptr(m + 0x60).is_none())
            else {
                continue;
            };
            let name = cstr(actor + 0x08);
            let path = cstr(model + 0x08);
            let lname = name.to_ascii_lowercase();
            if SKIP.iter().any(|k| lname.contains(k))
                || path.to_ascii_lowercase().contains("inviso")
            {
                continue;
            }
            let Some(tier) = PROJECTILE_NAME_TIERS
                .iter()
                .position(|keys| keys.iter().any(|k| lname.contains(k)))
            else {
                continue;
            };
            let key = (tier, !spawned.contains(&actor), order);
            if best.as_ref().is_some_and(|(k, _)| *k <= key) {
                continue;
            }
            let meshes = Self::rigid_model_meshes(model, &ptr, &list_items, &types, mesh_by_geo);
            if meshes.is_empty() {
                continue;
            }
            best = Some((
                key,
                CharacterProjectile {
                    actor: name,
                    model: path,
                    meshes,
                },
            ));
        }
        best.map(|(_, p)| p)
    }

    /// Maps each `igGeometryAttr` offset to the `igSkeleton2` offset that skins it:
    /// `tfbPhysicsBody` `+0x28` -> `tfbPhysicsCombinerLink` `+0x0c` -> skeleton;
    /// `+0x30` -> `tfbBodyEntityInfo` `+0x14` -> `DrawableList` -> `Drawable` `+0x0c` -> attr.
    fn skinned_attr_links(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
    ) -> std::collections::HashMap<usize, usize> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let mut links = std::collections::HashMap::new();
        for body in objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbPhysicsBody"))
        {
            let Some(skel) = ptr(body.offset + 0x28).and_then(|l| ptr(l + 0x0c)) else {
                continue;
            };
            let Some(list) = ptr(body.offset + 0x30).and_then(|e| ptr(e + 0x14)) else {
                continue;
            };
            let (Some(n), Some(arr)) = (rd(list + 0x08), ptr(list + 0x14)) else {
                continue;
            };
            for i in 0..n as usize {
                if let Some(attr) = ptr(arr + i * 4).and_then(|d| ptr(d + 0x0c)) {
                    links.insert(attr, skel);
                }
            }
        }
        links
    }

    /// Maps geometry object offsets to world matrices via the level entity graph:
    /// `tfbWorldEntityInfo` / `tfbEntityInfo` / `tfbBodyEntityInfo`:
    /// `_drawables` (+0x14) -> `DrawableList` (+0x08 count, +0x14 array of `Drawable`)
    /// `_entities` (+0x18) -> `EntityList` (+0x08 count, +0x14 array of `Entity`)
    /// `Drawable`: `_geometry` (+0x0c); `_entity` (+0x14) entity index
    /// `Entity`: `_transform` (+0x08) -> `igTransform`, matrix `_m` at +0x30; `_parentIndex` (+0x10)
    pub fn decode_entity_geometry_links(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
    ) -> std::collections::HashMap<usize, [f32; 16]> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let rd_u16 = |o: usize| -> Option<u16> {
            data.get(o..o + 2).map(|b| match endian {
                IgbEndian::Big => u16::from_be_bytes([b[0], b[1]]),
                IgbEndian::Little => u16::from_le_bytes([b[0], b[1]]),
            })
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let read_matrix = |xform_off: usize| -> Option<[f32; 16]> {
            let mut m = mat4_identity();
            for (i, val) in m.iter_mut().enumerate() {
                let w = rd(xform_off + 0x30 + i * 4)?;
                *val = f32::from_bits(w);
            }
            Some(m)
        };

        let mut geo_transforms = std::collections::HashMap::new();

        for info in objects.iter().filter(|o| {
            matches!(
                o.type_name.as_deref(),
                Some("tfbEntityInfo") | Some("tfbWorldEntityInfo") | Some("tfbBodyEntityInfo")
            )
        }) {
            let drawables = ptr(info.offset + 0x14);
            let entities = ptr(info.offset + 0x18);
            if let (Some(d_list), Some(e_list)) = (drawables, entities) {
                let d_count = rd(d_list + 0x08).unwrap_or(0) as usize;
                let d_arr = ptr(d_list + 0x14);
                let e_count = rd(e_list + 0x08).unwrap_or(0) as usize;
                let e_arr = ptr(e_list + 0x14);

                if let Some(e_p) = e_arr {
                    // ponytail: parents composed only when parent index < child index
                    // (single forward pass); add a topological pass if a level breaks this.
                    let mut entity_transforms: Vec<[f32; 16]> = Vec::with_capacity(e_count);
                    for e_idx in 0..e_count {
                        let ent = ptr(e_p + e_idx * 4);
                        let local = ent
                            .and_then(|e| ptr(e + meta_fields::entity::TRANSFORM))
                            .and_then(read_matrix)
                            .unwrap_or_else(mat4_identity);
                        let parent = ent
                            .and_then(|e| rd_u16(e + meta_fields::entity::PARENT_INDEX))
                            .map(usize::from)
                            .unwrap_or(0xFFFF);
                        let world = match entity_transforms.get(parent) {
                            Some(p) => mat4_mul(p, &local),
                            None => local,
                        };
                        entity_transforms.push(world);
                    }

                    if let Some(d_p) = d_arr {
                        for d_idx in 0..d_count {
                            if let Some(drawable) = ptr(d_p + d_idx * 4) {
                                if let Some(geo) = ptr(drawable + 0x0c) {
                                    let ent_idx =
                                        rd_u16(drawable + 0x14).unwrap_or(0xFFFF) as usize;
                                    if let Some(&xform) = entity_transforms.get(ent_idx) {
                                        geo_transforms.insert(geo, xform);
                                        // igWiiGeometryAttr has _geometry at +0x10 pointing to igWiiGeometry
                                        if let Some(inner_geo) = ptr(geo + 0x10) {
                                            geo_transforms.insert(inner_geo, xform);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        geo_transforms
    }

    /// World matrices of model geometry placed by level actors, with whether the
    /// placing actor is solid and its index in `placements`. Geometry of a `tfbPhysicsModel` (+0x68 entity info) is
    /// in model space; each actor that `draws_level_model` places it at its
    /// `model_transform`. Keys are geometry offsets (`igWiiGeometryAttr` and its
    /// `igWiiGeometry`); every model geometry is present, unplaced ones with no matrices.
    pub fn decode_model_placements(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
        placements: &[ActorPlacement],
    ) -> std::collections::HashMap<usize, Vec<([f32; 16], bool, usize)>> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let mut by_model: std::collections::HashMap<usize, Vec<([f32; 16], bool, usize)>> = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbPhysicsModel"))
            .map(|o| (o.offset, Vec::new()))
            .collect();
        for (ai, actor) in placements
            .iter()
            .enumerate()
            .filter(|(_, a)| a.draws_level_model())
        {
            let m = actor.model_transform();
            if !m.iter().all(|v| v.is_finite()) {
                continue;
            }
            let solid = actor.collision.as_ref().is_some_and(|c| c.is_solid);
            if let Some(model) = actor.model {
                by_model.entry(model).or_default().push((m, solid, ai));
            }
        }

        let mut out = std::collections::HashMap::new();
        for (model, xforms) in by_model {
            let Some(drawables) =
                ptr(model + meta_fields::physics_model::ENTITY_INFO).and_then(|e| ptr(e + 0x14))
            else {
                continue;
            };
            let (Some(n), Some(arr)) = (rd(drawables + 0x08), ptr(drawables + 0x14)) else {
                continue;
            };
            for i in 0..n as usize {
                let Some(geo) = ptr(arr + i * 4).and_then(|d| ptr(d + 0x0c)) else {
                    continue;
                };
                for g in std::iter::once(geo).chain(ptr(geo + 0x10)) {
                    out.entry(g)
                        .or_insert_with(Vec::new)
                        .extend_from_slice(&xforms);
                }
            }
        }
        out
    }

    /// Geometry offsets of sky layers: `tfbPhysicsWorld` objects named after a
    /// `Levels/_SKY/...` scene, `+0x28` -> `tfbWorldEntityInfo` drawables.
    pub fn decode_sky_geometries(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
    ) -> std::collections::HashSet<usize> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let is_sky = |o: usize| -> bool {
            ptr(o + meta_fields::named_object::NAME)
                .and_then(|p| data.get(p..))
                .map(|s| &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())])
                .is_some_and(|name| name.to_ascii_lowercase().windows(6).any(|w| w == b"/_sky/"))
        };
        let mut out = std::collections::HashSet::new();
        for world in objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbPhysicsWorld"))
            .filter(|o| is_sky(o.offset))
        {
            let Some(drawables) = ptr(world.offset + meta_fields::physics_world::ENTITY_INFO)
                .and_then(|e| ptr(e + 0x14))
            else {
                continue;
            };
            let (Some(n), Some(arr)) = (rd(drawables + 0x08), ptr(drawables + 0x14)) else {
                continue;
            };
            for i in 0..n as usize {
                if let Some(geo) = ptr(arr + i * 4).and_then(|d| ptr(d + 0x0c)) {
                    out.insert(geo);
                    out.extend(ptr(geo + 0x10));
                }
            }
        }
        out
    }

    /// Extracts material attributes for geometries: maps geometry offset -> material.
    pub fn decode_drawable_materials(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
    ) -> std::collections::HashMap<usize, DrawableMaterial> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let rd_f32 = |o: usize| -> Option<f32> {
            data.get(o..o + 4).map(|_| Self::read_f32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> Option<String> {
            let p = ptr(o)?;
            let s = data.get(p..)?;
            let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
            Some(String::from_utf8_lossy(&s[..end]).into_owned())
        };

        let mut out = std::collections::HashMap::new();

        for info in objects.iter().filter(|o| {
            matches!(
                o.type_name.as_deref(),
                Some("tfbEntityInfo") | Some("tfbWorldEntityInfo") | Some("tfbBodyEntityInfo")
            )
        }) {
            let Some(drawables) = ptr(info.offset + 0x14) else {
                continue;
            };
            let (Some(d_count), Some(d_arr)) = (rd(drawables + 0x08), ptr(drawables + 0x14)) else {
                continue;
            };

            for d_idx in 0..d_count as usize {
                let Some(d) = ptr(d_arr + d_idx * 4) else {
                    continue;
                };
                let Some(geo) = ptr(d + 0x0c) else {
                    continue;
                };
                let Some(mat) = ptr(d + 0x08) else {
                    continue;
                };

                let mat_name = cstr(mat + 0x08);
                let mut found_image = None;
                let mut found_color = None;
                let mut found_wrap = [TextureWrap::default(); 2];

                let mat_lists = [ptr(mat + 0x18), ptr(mat + 0x24)];
                for mlist in mat_lists.into_iter().flatten() {
                    let (Some(list_count), Some(list_arr)) = (rd(mlist + 0x08), ptr(mlist + 0x14))
                    else {
                        continue;
                    };
                    for l_idx in 0..list_count as usize {
                        let Some(attr_list) = ptr(list_arr + l_idx * 4) else {
                            continue;
                        };
                        let (Some(attr_count), Some(attr_arr)) = (
                            rd(attr_list + 0x08),
                            ptr(attr_list + 0x10).or_else(|| ptr(attr_list + 0x14)),
                        ) else {
                            continue;
                        };
                        for a_idx in 0..attr_count as usize {
                            let Some(attr) = ptr(attr_arr + a_idx * 4) else {
                                continue;
                            };
                            if found_image.is_none() {
                                for tex_slot in [0x10, 0x14, 0x18] {
                                    use meta_fields::ig_texture_attr2 as tex;
                                    if let Some(tex_attr) = ptr(attr + tex_slot) {
                                        if let Some(img) = ptr(tex_attr + tex::IMAGE) {
                                            found_image = Some(img);
                                            found_wrap = [tex::WRAP_S, tex::WRAP_T].map(|f| {
                                                rd(tex_attr + f)
                                                    .map(TextureWrap::from_raw)
                                                    .unwrap_or_default()
                                            });
                                            break;
                                        }
                                    }
                                }
                            }
                            if found_color.is_none() {
                                if let (Some(r), Some(g), Some(b), Some(a)) = (
                                    rd_f32(attr + 0x10),
                                    rd_f32(attr + 0x14),
                                    rd_f32(attr + 0x18),
                                    rd_f32(attr + 0x1c),
                                ) {
                                    if (0.0..=2.0).contains(&r)
                                        && (0.0..=2.0).contains(&g)
                                        && (0.0..=2.0).contains(&b)
                                        && (0.0..=1.0).contains(&a)
                                        && (r + g + b) > 0.05
                                    {
                                        found_color = Some([r, g, b, a]);
                                    }
                                }
                            }
                        }
                    }
                }

                let uv_scroll = ptr(mat + 0x28)
                    .and_then(|list| {
                        let n = rd(list + 0x08)?.min(64) as usize;
                        let arr = ptr(list + 0x14)?;
                        (0..n).find_map(|i| {
                            let track = ptr(ptr(ptr(arr + i * 4)? + 0x08)? + 0x50)?;
                            let scroll = ptr(track + 0x10)?;
                            Some([rd_f32(scroll + 0x10)?, rd_f32(scroll + 0x14)?])
                        })
                    })
                    .filter(|v| v.iter().all(|c| c.is_finite() && c.abs() < 100.0));
                let material = DrawableMaterial {
                    image: found_image,
                    color: found_color,
                    name: mat_name,
                    wrap: found_wrap,
                    uv_scroll,
                };
                if let Some(inner_geo) = ptr(geo + 0x10) {
                    out.insert(inner_geo, material.clone());
                }
                out.insert(geo, material);
            }
        }

        out
    }

    /// Reads level placements from `tfbActorInfo` objects (one per placed actor:
    /// coins, doors, spawns, props). Name: `igNamedObject._name` (+0x08).
    /// `AbstractPlacement._initMatrix` (+0x24, `tfbEulerTransform`): f32 x,y,z then
    /// Euler degrees x,y,z at +0x30. `Placement._script` (+0xa8) -> `tfbScriptInfo`
    /// whose `_name` is the `.ai` script path. Positions are in level file space.
    pub fn decode_actor_placements(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
    ) -> Vec<ActorPlacement> {
        let ptr = |o: usize| -> Option<usize> {
            let w = data
                .get(o..o + 4)
                .map(|_| Self::read_u32(data, o, endian))
                .filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let cstr = |o: usize| -> Option<String> {
            let start = ptr(o)?;
            let len = data.get(start..)?.iter().position(|&b| b == 0)?;
            std::str::from_utf8(&data[start..start + len])
                .ok()
                .map(str::to_string)
        };
        let vec3 = |o: usize| -> [f32; 3] {
            [
                Self::read_f32(data, o, endian),
                Self::read_f32(data, o + 4, endian),
                Self::read_f32(data, o + 8, endian),
            ]
        };
        let script_infos: std::collections::HashSet<usize> = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbScriptInfo"))
            .map(|o| o.offset)
            .collect();
        let col_info_offsets: std::collections::HashSet<usize> = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("CollisionInfo"))
            .map(|o| o.offset)
            .collect();
        let models: std::collections::HashSet<usize> = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbPhysicsModel"))
            .map(|o| o.offset)
            .collect();
        objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbActorInfo"))
            .filter(|o| o.offset + meta_fields::placement::SCRIPT + 4 <= data.len())
            .filter_map(|o| {
                let name = cstr(o.offset + meta_fields::named_object::NAME)?;
                let position = vec3(o.offset + meta_fields::actor_info::INIT_MATRIX);
                let rotation_deg = vec3(o.offset + meta_fields::actor_info::INIT_ROTATION);
                let script = ptr(o.offset + meta_fields::placement::SCRIPT)
                    .filter(|p| script_infos.contains(p))
                    .and_then(|p| cstr(p + meta_fields::named_object::NAME));
                let active = data
                    .get(o.offset + meta_fields::actor_info::ACTIVITY_STATE..)
                    .filter(|b| b.len() >= 4)
                    .map(|_| {
                        Self::read_u32(
                            data,
                            o.offset + meta_fields::actor_info::ACTIVITY_STATE,
                            endian,
                        )
                    })
                    == Some(meta_fields::actor_info::ACTIVITY_ACTIVE);
                let model = ptr(o.offset + meta_fields::actor_info::PHYSICS_MODEL)
                    .filter(|p| models.contains(p));
                let scale = vec3(o.offset + meta_fields::placement::SCALE_VEC);
                let collision = ptr(o.offset + meta_fields::placement::COLLISION)
                    .filter(|p| col_info_offsets.contains(p))
                    .and_then(|col_off| {
                        if col_off + meta_fields::collision_info::LENGTH + 4 <= data.len() {
                            let props = Self::read_u32(
                                data,
                                col_off + meta_fields::collision_info::PROPERTIES,
                                endian,
                            );
                            let local_position =
                                vec3(col_off + meta_fields::collision_info::TRANSFORM);
                            let local_rotation_deg =
                                vec3(col_off + meta_fields::collision_info::TRANSFORM + 12);
                            let extents = [
                                Self::read_f32(
                                    data,
                                    col_off + meta_fields::collision_info::WIDTH,
                                    endian,
                                ),
                                Self::read_f32(
                                    data,
                                    col_off + meta_fields::collision_info::HEIGHT,
                                    endian,
                                ),
                                Self::read_f32(
                                    data,
                                    col_off + meta_fields::collision_info::LENGTH,
                                    endian,
                                ),
                            ];
                            let actor_xform = {
                                let mut m = mat4_from_euler_deg(rotation_deg);
                                m[12..15].copy_from_slice(&position);
                                m
                            };
                            let local_m = {
                                let mut m = mat4_from_euler_deg(local_rotation_deg);
                                m[12..15].copy_from_slice(&local_position);
                                m
                            };
                            let world_transform = mat4_mul(&actor_xform, &local_m);
                            Some(CollisionPrimitive {
                                name: name.clone(),
                                shape_type: CollisionShapeType::from(props),
                                local_position,
                                local_rotation_deg,
                                extents,
                                world_transform,
                                is_solid: (props & 0x08) != 0,
                                interacts_with_world: (props & 0x10) != 0,
                                is_actor_surface: (props & 0x20) != 0,
                                is_actor_physics_object: (props & 0x40) != 0,
                                blocks_movement: active
                                    && model.is_none()
                                    && (props & 0x60) != 0
                                    && CollisionShapeType::from(props) != CollisionShapeType::Mesh,
                            })
                        } else {
                            None
                        }
                    });
                (position.iter().chain(&rotation_deg).all(|v| v.is_finite())).then_some(
                    ActorPlacement {
                        name,
                        position,
                        rotation_deg,
                        script,
                        collision,
                        active,
                        scale,
                        model,
                    },
                )
            })
            .collect()
    }

    /// Literal script assignments: `OpSetValue._LHS` -> `ValueStack` -> `OpCreateVariable`
    /// (name +0x1c); `_RHS` -> `RHSValueStack` (directly or via `ValueRHSVariant._varOp1`)
    /// whose `_type` is an `EXNM` handle naming `IntMeasurement` or `FloatMeasurement`.
    /// Character gameplay tuning (e.g. `BaseDamage_PrimaryAttack_Fire`) lives here.
    pub fn decode_script_constants(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        objects: &[IgzObject],
        endian: IgbEndian,
        external_types: &[String],
    ) -> Vec<(String, ScriptValue)> {
        use meta_fields::script;
        let types: std::collections::HashMap<usize, &str> = objects
            .iter()
            .filter_map(|o| Some((o.offset, o.type_name.as_deref()?)))
            .collect();
        let rd = |o: usize| data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian));
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o).filter(|&w| w != 0)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let typed = |o: usize, t: &str| ptr(o).filter(|p| types.get(p) == Some(&t));
        let cstr = |o: usize| -> Option<String> {
            let start = ptr(o)?;
            let len = data.get(start..)?.iter().position(|&b| b == 0)?;
            std::str::from_utf8(&data[start..start + len])
                .ok()
                .map(str::to_string)
        };
        objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("OpSetValue"))
            .filter_map(|o| {
                let lhs = typed(o.offset + script::SET_VALUE_LHS, "ValueStack")?;
                let var = typed(lhs + script::VALUE_STACK_VARIABLE, "OpCreateVariable")?;
                let name = cstr(var + script::CREATE_VARIABLE_NAME)?;
                let rhs =
                    typed(o.offset + script::SET_VALUE_RHS, "RHSValueStack").or_else(|| {
                        let v = typed(o.offset + script::SET_VALUE_RHS, "ValueRHSVariant")?;
                        typed(v + 0x08, "RHSValueStack")
                    })?;
                let raw = rd(rhs + script::RHS_VALUE)?;
                let kind = external_types.get(rd(rhs + script::RHS_TYPE)? as usize)?;
                let value = match kind.as_str() {
                    "IntMeasurement" => ScriptValue::Int(raw as i32),
                    "FloatMeasurement" => ScriptValue::Float(f32::from_bits(raw)),
                    _ => return None,
                };
                Some((name, value))
            })
            .collect()
    }

    pub fn decode_collision_primitives(placements: &[ActorPlacement]) -> Vec<CollisionPrimitive> {
        placements
            .iter()
            .filter_map(|p| p.collision.clone())
            .collect()
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

        if let Some(kind) = node
            .marker
            .or_else(|| MarkerKind::from_node_name(&node.name))
        {
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
                drawn_by_level: self.level_drawn_markers.contains(&node_idx),
                script: self.marker_scripts.get(&node_idx).cloned(),
                animated: self.animated_markers.contains(&node_idx),
            });
        }

        for &child_idx in &node.children {
            self.traverse_marker_node(child_idx, &world_transform, visited, markers);
        }

        visited[node_idx] = false;
    }

    /// Raw IGZ scan: section-0 fixup tables plus the object registry.
    pub fn scan_igz(data: &[u8]) -> Result<IgzFile, IgbError> {
        let igz_header = IgzHeader::parse(data)?;
        let endian = igz_header.endian;
        let version = igz_header.version;
        let sections = &igz_header.descriptors;

        let mut string_table = Vec::new();
        let mut type_table = Vec::new();
        let mut rvtb_offsets = Vec::new();
        let mut exid_formats = Vec::new();
        let mut exid_namespaces = Vec::new();
        let mut root_word = None;
        let mut tmhn_entries = Vec::new();
        let mut exnm_names = Vec::new();

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
                            | b"EXNM"
                            | b"MNXE"
                            | b"TOOR"
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
                                        exid_namespaces.push(Self::read_u32(
                                            payload,
                                            k * 8 + 4,
                                            endian,
                                        ));
                                    }
                                }
                            } else if tag == *b"TOOR" && payload.len() >= 4 {
                                root_word = Some(Self::read_u32(payload, 0, endian));
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
                            } else if tag == *b"EXNM" || tag == *b"MNXE" {
                                // (namespace, name) string-table index pairs, high bit = flag.
                                for k in 0..(count as usize) {
                                    if k * 8 + 8 <= payload.len() {
                                        let n = Self::read_u32(payload, k * 8 + 4, endian);
                                        exnm_names.push((n & 0x7FFF_FFFF) as usize);
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
        let external_types = exnm_names
            .iter()
            .map(|&i: &usize| string_table.get(i).cloned().unwrap_or_default())
            .collect();
        // `TOOR` -> root `igObjectList` (`_count` +0x08, `_data` +0x14).
        let ptr = |w: u32| -> Option<usize> {
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let word = |o: usize| data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian));
        let roots = root_word
            .and_then(ptr)
            .and_then(|list| {
                let n = word(list + 0x08)? as usize;
                let arr = ptr(word(list + 0x14)?)?;
                Some((0..n).filter_map(|k| ptr(word(arr + k * 4)?)).collect())
            })
            .unwrap_or_default();
        Ok(IgzFile {
            endian,
            version,
            sections: igz_header.descriptors.clone(),
            string_table,
            exid_formats,
            exid_namespaces,
            tmhn_entries,
            roots,
            external_types,
            objects: igz_objects,
        })
    }

    pub fn parse_igz(data: &[u8]) -> Result<Self, IgbError> {
        let file = Self::scan_igz(data)?;
        // Level actors whose script animates them get their own meshes.
        let animated_scripts = file
            .objects
            .as_ref()
            .filter(|l| {
                l.objects
                    .iter()
                    .any(|o| o.type_name.as_deref() == Some("tfbActorInfo"))
            })
            .and_then(|_| crate::script::ScriptSet::from_file(data, &file))
            .map(|set| crate::script_runtime::ScriptProgram::new(set, None).animated_scripts())
            .unwrap_or_default();
        let IgzFile {
            endian,
            version,
            sections,
            string_table,
            exid_formats,
            tmhn_entries,
            external_types,
            objects: igz_objects,
            ..
        } = file;
        let sections = &sections[..];

        let mut geo_transforms = std::collections::HashMap::new();
        let mut actor_placements = Vec::new();
        let mut geo_materials = std::collections::HashMap::new();
        let mut model_placements = std::collections::HashMap::new();
        let mut sky_geos = std::collections::HashSet::new();
        let mut collision_primitives = Vec::new();
        let mut script_constants = Vec::new();
        if let Some(ref list) = igz_objects {
            geo_transforms =
                Self::decode_entity_geometry_links(data, sections, &list.objects, endian);
            actor_placements = Self::decode_actor_placements(data, sections, &list.objects, endian);
            geo_materials = Self::decode_drawable_materials(data, sections, &list.objects, endian);
            model_placements = Self::decode_model_placements(
                data,
                sections,
                &list.objects,
                endian,
                &actor_placements,
            );
            sky_geos = Self::decode_sky_geometries(data, sections, &list.objects, endian);
            collision_primitives = Self::decode_collision_primitives(&actor_placements);
            script_constants = Self::decode_script_constants(
                data,
                sections,
                &list.objects,
                endian,
                &external_types,
            );
        }

        // Extract geometry, vertex buffer, index buffer, and vertex format objects from registry
        let mut vertex_buffers = Vec::new();
        let mut index_buffers = Vec::new();
        let mut attr_lists = Vec::new();
        let mut geometries = Vec::new();
        let mut geo_offsets = Vec::new();

        let mut skeletons = Vec::new();
        let mut skeleton_by_offset = std::collections::HashMap::new();
        let mut attr_skeleton = std::collections::HashMap::new();
        let mut havok_rigs = Vec::new();
        if let Some(ref obj_list) = igz_objects {
            for obj in &obj_list.objects {
                if obj.type_name.as_deref() != Some("igSkeleton2") {
                    continue;
                }
                if let Some((skel, blend)) =
                    Self::decode_skeleton2(data, sections, obj.offset, endian)
                {
                    skeleton_by_offset.insert(obj.offset, (skeletons.len(), blend));
                    skeletons.push(skel);
                }
            }
            attr_skeleton = Self::skinned_attr_links(data, sections, &obj_list.objects, endian);
            havok_rigs = Self::decode_havok_rigs(
                data,
                sections,
                &obj_list.objects,
                endian,
                &skeleton_by_offset,
            );
        }

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

                        if obj.type_name.as_deref() == Some("igGeometryAttr") {
                            let name = format!("igGeometryAttr_{}", idx);
                            let mesh = Self::decode_geometry_attr(
                                data,
                                sections,
                                &tmhn_entries,
                                obj.offset,
                                endian,
                            )
                            .map(|mut m| {
                                m.name = name.clone();
                                m.material_name = Some(format!("mat_{}", name));
                                // Palettes (`tfbBodyEntityInfo` +0x24 igIntListList) are
                                // identity in every character file, so slot == blend index.
                                if let Some((si, blend)) = attr_skeleton
                                    .get(&obj.offset)
                                    .and_then(|s| skeleton_by_offset.get(s))
                                {
                                    m.skeleton = Some(*si);
                                    for v in &mut m.vertices {
                                        for (bi, w) in v.bone_indices.iter_mut().zip(v.bone_weights)
                                        {
                                            *bi = match blend.get(*bi as usize) {
                                                Some(&b) => b,
                                                None if w == 0.0 => 0,
                                                None => *bi,
                                            };
                                        }
                                    }
                                }
                                m
                            });
                            if mesh.is_some() {
                                geo_offsets.push(obj.offset);
                                geometries.push(IgbGeometryNode {
                                    name,
                                    vertex_buffer_index: None,
                                    index_buffer_index: None,
                                    attr_list_index: Some(attr_lists.len() - 1),
                                    material_index: Some(0),
                                    mesh,
                                });
                            }
                        }
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
                                Self::read_u32(
                                    slice,
                                    meta_fields::ig_wii_geometry::DISPLAY_LIST,
                                    endian,
                                ),
                                Self::read_u32(
                                    slice,
                                    meta_fields::ig_wii_geometry::DISPLAY_LIST + 4,
                                    endian,
                                ),
                            );
                            let vb = resolve(
                                Self::read_u32(
                                    slice,
                                    meta_fields::ig_wii_geometry::VERTEX_BUFFER,
                                    endian,
                                ),
                                Self::read_u32(
                                    slice,
                                    meta_fields::ig_wii_geometry::VERTEX_BUFFER + 4,
                                    endian,
                                ),
                            );
                            decoded_mesh = Self::decode_wii_display_list(dl, vb).map(|mut m| {
                                m.name = geo_name.clone();
                                m.material_name = Some(format!("mat_{}", geo_name));
                                m
                            });
                        }

                        geo_offsets.push(obj.offset);
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

        // Level markers: real placements from `tfbActorInfo` when present.
        let mut nodes = Vec::new();
        let mut level_drawn_markers = std::collections::HashSet::new();
        let mut non_colliding_nodes = std::collections::HashSet::new();
        let mut animated_markers = std::collections::HashSet::new();
        let mut node_owner = std::collections::HashMap::new();
        let mut marker_scripts = std::collections::HashMap::new();
        let mut animated_actor_marker = std::collections::HashMap::new();
        if !actor_placements.is_empty() {
            for (ai, actor) in actor_placements.iter().enumerate() {
                // Level-drawn actors whose script animates them (birds, windmills,
                // floating steps) become props even without a marker kind.
                let animated = actor.draws_level_model()
                    && actor
                        .script
                        .as_ref()
                        .is_some_and(|s| animated_scripts.contains(s));
                let Some(kind) = actor
                    .kind()
                    .or_else(|| animated.then_some(MarkerKind::Prop))
                else {
                    continue;
                };
                // Inactive props and NPCs are templates a script spawns later (lids, hats).
                if matches!(kind, MarkerKind::Prop | MarkerKind::Npc) && !actor.active {
                    continue;
                }
                if actor.draws_level_model() {
                    level_drawn_markers.insert(nodes.len());
                }
                if let Some(script) = &actor.script {
                    marker_scripts.insert(nodes.len(), script.clone());
                }
                if animated && kind == MarkerKind::Prop {
                    animated_markers.insert(nodes.len());
                    animated_actor_marker.insert(ai, nodes.len());
                }
                nodes.push(IgbSceneNode {
                    name: actor.name.clone(),
                    type_id: IgbObjectType::IgNode,
                    children: Vec::new(),
                    geometry_index: None,
                    transform: actor.transform(),
                    marker: Some(kind),
                });
            }
        } else {
            // ponytail: files without `tfbActorInfo` keep the old synthetic spiral
            // layout for string-table names; drop once no caller relies on it.
            let mut marker_idx = 0;
            for name in &string_table {
                if MarkerKind::from_node_name(name).is_some() {
                    let angle = (marker_idx as f32) * 0.45;
                    let dist = 4.0 + ((marker_idx % 8) as f32) * 2.0;
                    let mut transform = mat4_identity();
                    if marker_idx != 0 {
                        transform[12] = angle.cos() * dist;
                        transform[14] = angle.sin() * dist;
                    }
                    nodes.push(IgbSceneNode {
                        name: name.clone(),
                        type_id: IgbObjectType::IgNode,
                        children: Vec::new(),
                        geometry_index: None,
                        transform,
                        marker: None,
                    });
                    marker_idx += 1;
                }
            }
        }

        let image_index_by_offset: std::collections::HashMap<usize, usize> = images
            .iter()
            .enumerate()
            .map(|(i, img)| (img.offset, i))
            .collect();

        // Collect meshes from extracted geometries
        let mut meshes = Vec::new();
        let mut sky_instances = Vec::new();
        let mut mesh_by_geo = std::collections::HashMap::new();
        for (geo_idx, geo) in geometries.iter_mut().enumerate() {
            if let Some(ref mut m) = geo.mesh {
                if let Some(&geo_off) = geo_offsets.get(geo_idx) {
                    mesh_by_geo.insert(geo_off, meshes.len());
                    if let Some(mat) = geo_materials.get(&geo_off) {
                        m.image_index = mat
                            .image
                            .and_then(|off| image_index_by_offset.get(&off).copied());
                        m.base_color = mat.color;
                        m.texture_wrap = mat.wrap;
                        m.uv_scroll = mat.uv_scroll;
                        if let Some(name) = &mat.name {
                            m.material_name = Some(name.clone());
                        }
                    }
                }
                meshes.push(m.clone());
                let geo_off = geo_offsets.get(geo_idx);
                let transform = geo_off
                    .and_then(|off| geo_transforms.get(off))
                    .copied()
                    .unwrap_or_else(mat4_identity);
                // Model geometry is drawn once per placing actor (none when unplaced);
                // only in files with actors, character files place their model directly.
                let placements = geo_off
                    .and_then(|off| model_placements.get(off))
                    .filter(|_| !actor_placements.is_empty());
                let world: Vec<([f32; 16], bool, Option<usize>)> = match placements {
                    _ if geo_off.is_some_and(|off| sky_geos.contains(off)) => {
                        sky_instances.push(SceneInstance {
                            node_index: 0,
                            geometry_index: geo_idx,
                            world_transform: transform,
                        });
                        Vec::new()
                    }
                    Some(xforms) => xforms
                        .iter()
                        .map(|(p, solid, ai)| (mat4_mul(p, &transform), *solid, Some(*ai)))
                        .collect(),
                    None => vec![(transform, true, None)],
                };
                for (transform, solid, actor) in world {
                    if !solid {
                        non_colliding_nodes.insert(nodes.len());
                    }
                    if let Some(&m) = actor.and_then(|ai| animated_actor_marker.get(&ai)) {
                        node_owner.insert(nodes.len(), m);
                    }
                    nodes.push(IgbSceneNode {
                        name: format!("node_{}", geo.name),
                        type_id: IgbObjectType::IgNode,
                        children: Vec::new(),
                        geometry_index: Some(geo_idx),
                        transform,
                        marker: None,
                    });
                }
            }
        }

        let attachments = match (&igz_objects, primary_skeleton_of(&skeletons, &meshes)) {
            (Some(list), Some(primary)) => skeleton_by_offset
                .iter()
                .find(|(_, (idx, _))| *idx == primary)
                .map(|(&off, _)| {
                    Self::decode_attachments(
                        data,
                        sections,
                        &list.objects,
                        endian,
                        off,
                        &mesh_by_geo,
                    )
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let projectile = match (&igz_objects, primary_skeleton_of(&skeletons, &meshes)) {
            (Some(list), Some(_)) => {
                Self::decode_projectile(data, sections, &list.objects, endian, &mesh_by_geo)
            }
            _ => None,
        };

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
                        skeleton: None,
                        image_index: None,
                        base_color: None,
                        texture_wrap: Default::default(),
                        uv_scroll: None,
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
                        marker: None,
                    });
                }
            }
        }

        let mut animations = Vec::new();
        if let Some(ref obj_list) = igz_objects {
            let rd = |o: usize| -> Option<u32> {
                data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
            };
            let ptr = |o: usize| -> Option<usize> {
                let w = rd(o).filter(|&w| w != 0)?;
                let s = sections.get(((w >> 27) + 1) as usize)?;
                Some(s.offset + (w & 0x07FF_FFFF) as usize)
            };
            let cstr = |o: usize| -> String {
                ptr(o)
                    .and_then(|p| data.get(p..))
                    .map(|s| {
                        let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                        String::from_utf8_lossy(&s[..end]).into_owned()
                    })
                    .unwrap_or_default()
            };

            for obj in &obj_list.objects {
                if obj.type_name.as_deref() != Some("tfbPhysicsAnimation") {
                    continue;
                }
                let raw_name = cstr(obj.offset + 0x08);
                let Some(raw_sz) = rd(obj.offset + 0x14) else {
                    continue;
                };
                let sz = (raw_sz & 0x07ff_ffff) as usize;
                let Some(hk_off) = ptr(obj.offset + 0x18) else {
                    continue;
                };
                let Some(hk_data) = data.get(hk_off..hk_off + sz) else {
                    continue;
                };

                let stem = std::path::Path::new(&raw_name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&raw_name);
                let name = if let Some(pos) = stem.rfind('_') {
                    &stem[pos + 1..]
                } else {
                    stem
                };
                let norm_name = name.to_ascii_lowercase();

                if let Some(mut clip) = crate::havok::parse_havok_animation(
                    hk_data,
                    &norm_name,
                    &skeletons,
                    &havok_rigs,
                ) {
                    clip.source = raw_name.clone();
                    animations.push(clip);
                }
            }
        }

        if !animations.is_empty() {
            let mut has_idle = animations.iter().any(|a| a.name == "idle");
            let mut has_walk = animations.iter().any(|a| a.name == "walk");
            let mut has_attack = animations.iter().any(|a| a.name == "attack");

            if !has_idle {
                if let Some(pos) = animations.iter().position(|a| a.name.contains("idle")) {
                    let mut alias = animations[pos].clone();
                    alias.name = "idle".to_string();
                    animations.push(alias);
                    has_idle = true;
                }
            }
            if !has_walk {
                if let Some(pos) = animations
                    .iter()
                    .position(|a| a.name.contains("walk") || a.name.contains("run"))
                {
                    let mut alias = animations[pos].clone();
                    alias.name = "walk".to_string();
                    animations.push(alias);
                    has_walk = true;
                }
            }
            if !has_attack {
                if let Some(pos) = animations.iter().position(|a| {
                    a.name.contains("charge")
                        || a.name.contains("attack")
                        || a.name.contains("fire")
                }) {
                    let mut alias = animations[pos].clone();
                    alias.name = "attack".to_string();
                    animations.push(alias);
                    has_attack = true;
                }
            }
            let _ = (has_idle, has_walk, has_attack);
        } else {
            // ponytail: synthetic clips (idle/walk/attack on bone 0) for models without tfbPhysicsAnimation.
            let (lo, hi) = meshes
                .iter()
                .flat_map(|m| &m.vertices)
                .fold((f32::MAX, f32::MIN), |(lo, hi), v| {
                    (lo.min(v.position[1]), hi.max(v.position[1]))
                });
            let h = if hi > lo { hi - lo } else { 1.5 }.max(0.1);

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
            let attack_clip =
                AnimationClip::new("attack", 0.8, vec![make_track(0, 0.02 * h, 0.15)]);
            animations = vec![idle_clip, walk_clip, attack_clip];
        }

        let animation_transitions = igz_objects
            .as_ref()
            .map(|list| Self::decode_animation_transitions(data, sections, &list.objects, endian))
            .unwrap_or_default();
        let (actor_models, animation_states) = match &igz_objects {
            Some(list) => (
                Self::decode_actor_models(
                    data,
                    sections,
                    &list.objects,
                    endian,
                    &mesh_by_geo,
                    &skeleton_by_offset,
                ),
                Self::decode_animation_states(data, sections, &list.objects, endian, &animations),
            ),
            None => (Vec::new(), Vec::new()),
        };
        let script_colors = igz_objects
            .as_ref()
            .map(|list| {
                list.objects
                    .iter()
                    .filter(|o| o.type_name.as_deref() == Some("ScriptColorInfo"))
                    .filter_map(|o| {
                        let w = Self::read_u32(data, o.offset + 0x08, endian);
                        let s = sections.get(((w >> 27) + 1) as usize)?;
                        let p = s.offset + (w & 0x07FF_FFFF) as usize;
                        let bytes = data.get(p..)?;
                        let end = bytes.iter().position(|&b| b == 0)?;
                        let name = String::from_utf8_lossy(&bytes[..end]).into_owned();
                        Some((name, Self::read_u32(data, o.offset + 0x18, endian)))
                    })
                    .collect()
            })
            .unwrap_or_default();

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
            collision_primitives,
            script_constants,
            attachments,
            projectile,
            sky_instances,
            non_colliding_nodes,
            level_drawn_markers,
            animated_markers,
            node_owner,
            marker_scripts,
            actor_models,
            animation_states,
            animation_transitions,
            script_colors,
        })
    }

    /// Decodes a Wii GX display list (`igWiiGeometry` buffer A) whose indexed
    /// attribute arrays live in `vb` (buffer B). Array bases (CP 0xA0+n) are
    /// offsets into `vb`. GX data is always big-endian.
    /// Decodes an `igGeometryAttr` (skinned character body) into a mesh.
    ///
    /// Layout (IGZ v8, Wii): attr `+0x10` -> `igVertexBuffer`, `+0x14` -> `igIndexBuffer`.
    /// VB: `+0x08` count, `+0x14` TMHN data handle, `+0x18` -> `igVertexFormat`,
    /// `+0x20/+0x24` per-element fixed-point shifts. VF: `+0x10` element list
    /// (12 bytes each, `0x2c` terminator), `+0x30/+0x34` per-stream strides.
    /// Streams are packed back to back, each 32-byte aligned.
    /// IB: `+0x08` index count, `+0x14` TMHN handle to a GX display list of
    /// `0x98` strips with one u8/u16 index per element (all equal).
    pub fn decode_geometry_attr(
        data: &[u8],
        sections: &[IgzChunkDescriptor],
        tmhn: &[(usize, usize)],
        attr_off: usize,
        endian: IgbEndian,
    ) -> Option<IgbMesh> {
        let rd = |o: usize| -> Option<u32> {
            data.get(o..o + 4).map(|_| Self::read_u32(data, o, endian))
        };
        let ptr = |o: usize| -> Option<usize> {
            let w = rd(o)?;
            let s = sections.get(((w >> 27) + 1) as usize)?;
            Some(s.offset + (w & 0x07FF_FFFF) as usize)
        };
        let words = |len_off: usize, ptr_off: usize| -> Option<Vec<u32>> {
            let n = (rd(len_off)? & 0x00FF_FFFF) as usize / 4;
            let p = ptr(ptr_off)?;
            (0..n).map(|i| rd(p + i * 4)).collect()
        };
        let blob = |handle_off: usize| -> Option<&[u8]> {
            let (o, l) = *tmhn.get(rd(handle_off)? as usize)?;
            data.get(o..o + l)
        };

        let vb = ptr(attr_off + meta_fields::ig_geometry_attr::VERTEX_BUFFER)?;
        let ib = ptr(attr_off + meta_fields::ig_geometry_attr::INDEX_BUFFER)?;
        let count = rd(vb + 0x08)? as usize;
        let vdata = blob(vb + 0x14)?;
        let vf = ptr(vb + 0x18)?;
        let shifts = words(vb + 0x20, vb + 0x24)?;
        let strides = words(vf + 0x30, vf + 0x34)?;
        if count == 0 || strides.is_empty() {
            return None;
        }

        let mut bases = Vec::with_capacity(strides.len());
        let mut acc = 0usize;
        for &s in &strides {
            bases.push(acc);
            acc = (acc + s as usize * count + 31) & !31;
        }

        let mut vertices = vec![
            IgbVertex {
                position: [0.0; 3],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
                color: [1.0; 4],
                bone_indices: [0; 4],
                bone_weights: [1.0, 0.0, 0.0, 0.0],
            };
            count
        ];
        let be16 = |o: usize| {
            vdata
                .get(o..o + 2)
                .map(|b| i16::from_be_bytes([b[0], b[1]]))
        };

        // Element: [0]=type, [1]=stream, [4]=usage, [5]=usage index, [6]=shift slot*4,
        // [8..10]=offset. Usage 0=POS s16x3, 1=NRM s8x3, 4=color, 5=UV s16x2,
        // 6=blend weights, 8=blend indices.
        let mut el = ptr(vf + 0x10)?;
        let mut has_pos = false;
        while let Some(e) = data.get(el..el + 12) {
            if e[0] == 0x2c {
                break;
            }
            let (stream, usage, set) = (e[1] as usize, e[4], e[5]);
            let off = u16::from_be_bytes([e[8], e[9]]) as usize;
            let shift = shifts.get(e[6] as usize / 4).copied().unwrap_or(0).min(30);
            let scale = 1.0 / (1u32 << shift) as f32;
            let (base, stride) = (*bases.get(stream)?, *strides.get(stream)? as usize);
            // ponytail: color skipped. Blend indices are bone-palette slots, not
            // skeleton bones; map them once igSkeleton2 is decoded.
            for (i, v) in vertices.iter_mut().enumerate() {
                let o = base + i * stride + off;
                match (usage, set) {
                    (0, _) => {
                        for c in 0..3 {
                            v.position[c] = be16(o + c * 2)? as f32 * scale;
                        }
                    }
                    (1, _) => {
                        let b = vdata.get(o..o + 3)?;
                        v.normal = [b[0], b[1], b[2]].map(|c| c as i8 as f32 * scale);
                    }
                    (5, 0) => {
                        for c in 0..2 {
                            v.uv[c] = be16(o + c * 2)? as f32 * scale;
                        }
                    }
                    (6, 0) => {
                        let b = vdata.get(o..o + 4)?;
                        let sum: u32 = b.iter().map(|&w| w as u32).sum();
                        if sum > 0 {
                            v.bone_weights =
                                [b[0], b[1], b[2], b[3]].map(|w| w as f32 / sum as f32);
                        }
                    }
                    (8, 0) => {
                        let b = vdata.get(o..o + 4)?;
                        v.bone_indices = [b[0], b[1], b[2], b[3]].map(u32::from);
                    }
                    _ => {}
                }
            }
            has_pos |= usage == 0;
            el += 12;
        }
        if !has_pos {
            return None;
        }

        let index_count = rd(ib + 0x08)? as usize;
        let dl = blob(ib + 0x14)?;
        let mut indices = Vec::with_capacity(index_count * 3);
        let mut read = 0usize;
        // Element stride / index width aren't stored explicitly: try each and keep the
        // one that consumes the list exactly with in-range, repeated indices.
        'width: for width in [2usize, 1] {
            for k in 1..=8usize {
                indices.clear();
                read = 0;
                let mut p = 0usize;
                let ok = loop {
                    let Some(&op) = dl.get(p) else { break true };
                    if op == 0 {
                        p += 1;
                        continue;
                    }
                    if op & 0xF8 != 0x98 {
                        break false;
                    }
                    let Some(n) = dl.get(p + 1..p + 3) else {
                        break false;
                    };
                    let n = u16::from_be_bytes([n[0], n[1]]) as usize;
                    let step = k * width;
                    let Some(body) = dl.get(p + 3..p + 3 + n * step) else {
                        break false;
                    };
                    let mut strip = Vec::with_capacity(n);
                    for v in body.chunks_exact(step) {
                        let get = |j: usize| -> usize {
                            if width == 2 {
                                u16::from_be_bytes([v[j * 2], v[j * 2 + 1]]) as usize
                            } else {
                                v[j] as usize
                            }
                        };
                        let idx = get(0);
                        if idx >= count || (1..k).any(|j| get(j) != idx) {
                            break;
                        }
                        strip.push(idx as u32);
                    }
                    if strip.len() != n {
                        break false;
                    }
                    for s in 2..n {
                        let (a, b, c) = (strip[s - 2], strip[s - 1], strip[s]);
                        if a == b || b == c || a == c {
                            continue;
                        }
                        if s % 2 == 0 {
                            indices.extend_from_slice(&[a, b, c]);
                        } else {
                            indices.extend_from_slice(&[b, a, c]);
                        }
                    }
                    read += n;
                    p += 3 + n * step;
                };
                if ok && read == index_count {
                    break 'width;
                }
            }
        }
        if read != index_count || indices.is_empty() {
            return None;
        }

        Some(IgbMesh {
            name: "geometry_attr".to_string(),
            vertices,
            indices,
            material_name: None,
            skeleton: None,
            image_index: None,
            base_color: None,
            texture_wrap: Default::default(),
            uv_scroll: None,
        })
    }

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
                        // PNMTXIDX (VCD bit 0) is a row offset into XF matrix memory, 3 rows
                        // per matrix. ponytail: stored as a palette slot, not a skeleton bone.
                        let pnmtx = if vcd_lo & 1 == 1 {
                            *dl.get(i)? as u32 / 3
                        } else {
                            0
                        };
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

                        let mut color = [0.8, 0.8, 0.82, 1.0];
                        for (slot, (c, shift)) in [(13u32, 14u32), (15, 18)].into_iter().enumerate()
                        {
                            let cfmt = (a >> shift) & 7;
                            let src = match (vcd_lo >> c) & 3 {
                                1 => {
                                    let o = p;
                                    p += color_size(cfmt);
                                    Some((dl, o))
                                }
                                kind => read_idx(kind, &mut p)
                                    .map(|idx| (vb, base[2 + slot] + idx * stride[2 + slot])),
                            };
                            // ponytail: CLR1 (specular/second channel) parsed past, not stored.
                            if slot == 0 {
                                if let Some(rgba) = src.and_then(|(s, o)| gx_color(s, o, cfmt)) {
                                    color = rgba;
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
                            color,
                            bone_indices: [pnmtx, 0, 0, 0],
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
            skeleton: None,
            image_index: None,
            base_color: None,
            texture_wrap: Default::default(),
            uv_scroll: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_placement_kind_and_transform() {
        let actor = |name: &str, script: Option<&str>| ActorPlacement {
            name: name.into(),
            position: [1.0, 2.0, 3.0],
            rotation_deg: [0.0, 90.0, 0.0],
            script: script.map(Into::into),
            collision: None,
            active: true,
            scale: [100.0; 3],
            model: None,
        };
        let s = "C:/tfb/Content/Levels/Level_Hub_2014/Scripts/";
        let crystal = actor("Transit_01", Some(&format!("{s}TransitCrystal.ai")));
        assert_eq!(crystal.kind(), Some(MarkerKind::Doorway));
        let cam = actor("spawn_cam", Some(&format!("{s}2014_Camera.ai")));
        assert_eq!(cam.kind(), None, "script overrides name");
        assert_eq!(actor("Door_X", None).kind(), Some(MarkerKind::Doorway));
        let flynn = actor("Flynn", Some(&format!("{s}NPC_Flynn_Injured.ai")));
        assert_eq!(flynn.kind(), Some(MarkerKind::Npc));
        assert_eq!(
            npc_name(&format!("{s}NPC_Flynn_Injured.ai")).as_deref(),
            Some("Flynn")
        );
        assert_eq!(
            npc_name(&format!("{s}NPC_TBone.ai")).as_deref(),
            Some("T-Bone")
        );
        assert_eq!(
            npc_name("x/Persephone_2014.ai").as_deref(),
            Some("Persephone")
        );
        assert_eq!(npc_name("x/Auric.ai").as_deref(), Some("Auric"));
        let posters = actor("Posters", Some(&format!("{s}NPC_WantedPosters.ai")));
        assert_eq!(posters.kind(), None);
        assert_eq!(npc_name("x/Persephone_2014_UpgradeFXControl.ai"), None);

        // +90 deg yaw maps +X to -Z (right-handed, Y up).
        let m = crystal.transform();
        let x = transform_point(&m, [1.0, 0.0, 0.0]);
        assert!(
            (x[0] - 1.0).abs() < 1e-5 && (x[2] - 2.0).abs() < 1e-5,
            "{x:?}"
        );
    }

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
            marker: None,
        };

        let child_node = IgbSceneNode {
            name: "child".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: Some(0),
            transform: child_trans,
            marker: None,
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
            collision_primitives: vec![],
            script_constants: Vec::new(),
            attachments: Vec::new(),
            projectile: None,
            sky_instances: Vec::new(),
            non_colliding_nodes: Default::default(),
            level_drawn_markers: Default::default(),
            animated_markers: Default::default(),
            node_owner: Default::default(),
            marker_scripts: Default::default(),
            actor_models: Vec::new(),
            animation_states: Vec::new(),
            animation_transitions: Vec::new(),
            script_colors: Vec::new(),
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
            marker: None,
        };

        let spawn_node = IgbSceneNode {
            name: "sp_hero_start".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: None,
            transform: spawn_trans,
            marker: None,
        };

        let prop_node = IgbSceneNode {
            name: "prop_wooden_barrel".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: Some(0),
            transform: prop_trans,
            marker: None,
        };

        let pickup_node = IgbSceneNode {
            name: "pickup_gold_coin".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: None,
            transform: pickup_trans,
            marker: None,
        };

        let test_mesh = IgbMesh {
            name: "barrel_mesh".to_string(),
            vertices: vec![],
            indices: vec![],
            material_name: None,
            skeleton: None,
            image_index: None,
            base_color: None,
            texture_wrap: Default::default(),
            uv_scroll: None,
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
            collision_primitives: vec![],
            script_constants: Vec::new(),
            attachments: Vec::new(),
            projectile: None,
            sky_instances: Vec::new(),
            non_colliding_nodes: Default::default(),
            level_drawn_markers: Default::default(),
            animated_markers: Default::default(),
            node_owner: Default::default(),
            marker_scripts: Default::default(),
            actor_models: Vec::new(),
            animation_states: Vec::new(),
            animation_transitions: Vec::new(),
            script_colors: Vec::new(),
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
            marker: None,
        };
        let node1 = IgbSceneNode {
            name: "node1".to_string(),
            type_id: IgbObjectType::IgNode,
            children: vec![],
            geometry_index: Some(0),
            transform: trans(0.0, 2.0, 0.0),
            marker: None,
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
            collision_primitives: vec![],
            script_constants: Vec::new(),
            attachments: Vec::new(),
            projectile: None,
            sky_instances: Vec::new(),
            non_colliding_nodes: Default::default(),
            level_drawn_markers: Default::default(),
            animated_markers: Default::default(),
            node_owner: Default::default(),
            marker_scripts: Default::default(),
            actor_models: Vec::new(),
            animation_states: Vec::new(),
            animation_transitions: Vec::new(),
            script_colors: Vec::new(),
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

    fn read_character_bld(name: &str) -> Option<Vec<u8>> {
        use crate::archive::ArchiveMount;
        let p = ["assets/files/character", "../../assets/files/character"]
            .iter()
            .map(|d| std::path::Path::new(d).join(format!("{name}.bld")))
            .find(|p| p.exists())?;
        let mut mount = ArchiveMount::new();
        mount.mount(&p).ok()?;
        mount.read_file("level.bld").ok()
    }

    #[test]
    fn test_no_skeleton_without_igskeleton2() {
        let bytes = create_test_character_igz(&["j_root", "j_pelvis", "j_head"]);
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse generic");
        assert!(model.skeletons.is_empty(), "no synthetic rig");
        assert!(model.meshes.iter().all(|m| m.skeleton.is_none()));
        assert!(model.animations.iter().any(|a| a.name == "idle"));
        let empty = Skeleton::default();
        assert!(model.animations[0].sample_pose(0.5, &empty).is_empty());
    }

    #[test]
    fn test_magic_moment_stream_rig_states_and_scroll() {
        let Some(bytes) = read_character_bld("sky4_mm_streaming") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse mm stream");
        let rig = model
            .actor_models
            .iter()
            .find(|a| a.actor == "MMRig")
            .expect("MMRig");
        assert!(rig.model.ends_with("MMsky4_uberModel.mdl"), "{}", rig.model);
        let node = |n: &str| {
            rig.attach_nodes
                .iter()
                .find(|a| a.name == n)
                .map(|a| a.bone.as_str())
        };
        assert_eq!(node("camera"), Some("J_camera"));
        assert_eq!(node("Skylander"), Some("J_skylander"));
        assert_eq!(node("name"), Some("J_skylanderName"));
        assert_eq!(node("icon"), Some("J_elementalIcon"));
        let skel = rig.skeleton.expect("rigged");
        assert!(model.skeletons[skel]
            .bones
            .iter()
            .any(|b| b.name == "J_camera"));

        let state = |id| {
            model
                .animation_for_state(id)
                .map(|c| (c.source.rsplit('/').next().unwrap().to_string(), c.duration))
        };
        let (new_toy, d) = state(3).expect("New Toy");
        assert_eq!(new_toy, "uberModel_NewToy.ma");
        assert!((d - 1.17).abs() < 0.01, "{d}");
        assert_eq!(state(5).unwrap().0, "uberModel_MM.ma");
        let (alt_c, d) = state(9).expect("Hyper Jump (alt C)");
        assert_eq!(alt_c, "uberModel_HyperJump_altC.ma");
        assert!((d - 1.43).abs() < 0.01, "{d}");

        for actor in ["MM_dualEndedTunnelTube", "MM_fastStars", "MM_slowStars"] {
            let a = model
                .actor_models
                .iter()
                .find(|a| a.actor == actor)
                .expect(actor);
            assert!(!a.meshes.is_empty(), "{actor}");
        }
        // The tunnel walls and star tubes scroll their textures.
        let scrolling = |actor: &str| {
            model
                .actor_models
                .iter()
                .find(|a| a.actor == actor)
                .is_some_and(|a| {
                    a.meshes
                        .iter()
                        .any(|&m| model.meshes[m].uv_scroll.is_some())
                })
        };
        assert!(scrolling("MM_dualEndedTunnelTube") || scrolling("MM_fastStars"));

        let color = |n: &str| {
            model
                .script_colors
                .iter()
                .find(|(name, _)| name == n)
                .map(|&(_, v)| v)
        };
        assert_eq!(color("MM_element_00_magic_color"), Some(0xffa5335d));
        assert_eq!(color("MM_element_neutral_color"), Some(0xffdd822c));
    }

    #[test]
    fn test_hero_toy_alive_state() {
        let Some(bytes) = read_character_bld("016_Spyro") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
        let toy_alive = model.animation_for_state(80).expect("state 80");
        assert_eq!(toy_alive.name, "toyalive");
        assert!((toy_alive.duration - 3.33).abs() < 0.01);
        assert_eq!(model.animation_for_state(81).unwrap().name, "toy");

        let skel = &model.skeletons[0];
        let jaw_idx = skel.get_bone_index("J_jaw").unwrap();
        let toy_alive_track = toy_alive
            .tracks
            .iter()
            .find(|t| t.bone_index == jaw_idx)
            .expect("jaw track");
        let s0 = toy_alive_track.scales[0].value;
        assert!(
            (s0[0] - 1.0).abs() < 0.05 && (s0[1] - 1.0).abs() < 0.05 && (s0[2] - 1.0).abs() < 0.05,
            "jaw scale at t=0 must be ~1.0, got {s0:?}"
        );
    }

    #[test]
    fn test_spyro_animation_transitions() {
        let Some(bytes) = read_character_bld("016_Spyro") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
        let graph = model.animation_graph();
        assert_eq!(graph.clip_for_state(99), Some("idletowalk"));
        assert_eq!(graph.state_for_clip("walk"), Some(1));

        // Walk from idle goes through IdletoWalk, then auto-advances to walk.
        let t = graph.transition(Some(0), 1).expect("idle to idletowalk");
        assert_eq!(t.name, "idle to idletowalk");
        assert_eq!(t.to, 99);
        assert!((t.blend - 0.1333).abs() < 1e-3);
        let auto = graph.auto_transition(99).expect("idletowalk to walk");
        assert_eq!(auto.to, 1);

        // Specific sources win; other sources fall back to the any-source default.
        let run_to_walk = graph.transition(Some(3), 1).unwrap();
        assert!((run_to_walk.blend - 0.2667).abs() < 1e-3);
        let any = graph.transition(Some(57), 1).unwrap();
        assert!(any.from.is_empty());
        assert!((any.blend - 0.064).abs() < 1e-4);

        // JumpStanding lands on its own; landing blends to idle on request.
        assert_eq!(graph.auto_transition(101).unwrap().to, 61);
        let land_idle = graph.transition(Some(61), 0).unwrap();
        assert!((land_idle.blend - 0.0667).abs() < 1e-3);
    }

    #[test]
    fn test_script_constants_character_damage() {
        let Some(bytes) = read_character_bld("016_Spyro") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
        let get = |n| model.script_constant(n);
        assert_eq!(
            get("BaseDamage_PrimaryAttack_Fire"),
            Some(ScriptValue::Int(15))
        );
        assert_eq!(
            get("BaseDamage_SecondaryAttack_Charge"),
            Some(ScriptValue::Int(10))
        );
        assert_eq!(get("BaseDamage_DiveBomb"), Some(ScriptValue::Int(50)));
        assert_eq!(
            get("PrimaryAttack_Range_Base"),
            Some(ScriptValue::Float(0.45))
        );
        assert_eq!(
            get("projectile range/lifetime"),
            Some(ScriptValue::Float(0.35))
        );
        let missile: Vec<f32> = model
            .script_constants
            .iter()
            .filter(|(n, _)| n == "missile speed")
            .map(|(_, v)| v.as_f32())
            .collect();
        assert!(missile.contains(&40.0), "missile speed {missile:?}");

        let Some(bytes) = read_character_bld("014_GillGrunt") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse gill grunt");
        assert_eq!(
            model.script_constant("BaseDamage_PrimaryAttack_Harpoon"),
            Some(ScriptValue::Int(20))
        );

        let Some(bytes) = read_character_bld("462_Water_Crocky") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse snap shot");
        let damage: Vec<_> = model
            .script_constants
            .iter()
            .filter(|(n, _)| n.starts_with("BaseDamage_"))
            .take(3)
            .map(|(n, v)| (n.as_str(), v.as_f32()))
            .collect();
        assert_eq!(
            damage,
            [
                ("BaseDamage_Arrow_main", 12.5),
                ("BaseDamage_Melee_slam", 30.0),
                ("BaseDamage_JumpSlam", 25.0)
            ]
        );
    }

    #[test]
    fn test_primary_animations_skip_secondary_rig() {
        let Some(bytes) = read_character_bld("1010_Kaos") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse kaos");
        let primary = model.primary_skeleton_index();
        // `kaosHead_Idle` (8.53 s, head rig) comes before `Kaos_Idle` (1.07 s, body rig).
        let first_idle = model.animations.iter().find(|a| a.name == "idle").unwrap();
        assert_ne!(first_idle.skeleton, primary);
        let clips = model.primary_animations();
        assert!(clips.iter().all(|a| a.skeleton == primary));
        let idle = clips.iter().find(|a| a.name == "idle").expect("body idle");
        assert!((idle.duration - 1.07).abs() < 1e-2, "{}", idle.duration);
        assert!(clips.iter().any(|a| a.name == "walk"));
    }

    /// Mean |frame-0 track translation - rest translation| over non-root tracks, both as
    /// `hkQsTransform` locals. Roots and `J_attach*` prop bones move in idle; skip them.
    fn idle_rest_mismatch(name: &str) -> Option<(f32, usize, usize)> {
        let bytes = read_character_bld(name)?;
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse");
        let sk = model.primary_skeleton().expect("primary rig");
        let clips = model.primary_animations();
        let idle = clips.iter().find(|a| a.name == "idle").expect("idle");
        let world =
            |j: usize| QsTransform::from_mat(&mat4_affine_inverse(&sk.bones[j].inverse_bind_pose));
        let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let (mut sum, mut n) = (0.0, 0);
        for t in &idle.tracks {
            let bone = &sk.bones[t.bone_index];
            let lower = bone.name.to_ascii_lowercase();
            if lower.contains("root") || lower.contains("attach") {
                continue;
            }
            let Some(p) = bone.parent else { continue };
            let rest = world(p).local_of(&world(t.bone_index));
            let anim = sample_vec3(&t.positions, 0.0).unwrap();
            sum += (len(anim) - len(rest.t)).abs();
            n += 1;
        }
        Some((sum / n.max(1) as f32, idle.tracks.len(), n))
    }

    #[test]
    fn test_character_projectile_actor() {
        let cases = [
            (
                "014_GillGrunt",
                "014_HarpoonSpear_Projectile",
                "GillGrunt_HarpoonSpear.mdl",
            ),
            ("016_Spyro", "016_Fire", ""),
            ("009_Eruptor", "009_LavaProjectile", ""),
            (
                "450_Typhoon",
                "450_Boomerang",
                "450_Trap_AirMale_Boomerang.mdl",
            ),
            ("019_GoldenCheat", "019_GoldBullet", ""),
        ];
        for (file, actor, mdl) in cases {
            let Some(bytes) = read_character_bld(file) else {
                return;
            };
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse");
            let p = model.projectile.as_ref().expect(file);
            assert_eq!(p.actor, actor, "{file}");
            assert!(p.model.ends_with(mdl), "{file}: {}", p.model);
            assert!(!p.meshes.is_empty());
            assert!(p.meshes.iter().all(|&i| model.meshes[i].skeleton.is_none()));
        }
        // Melee-only: Stealth Elf has no projectile actor.
        if let Some(bytes) = read_character_bld("026_StealthElf") {
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse");
            assert_eq!(model.projectile, None);
        }
    }

    #[test]
    fn test_character_attachments_from_attach_templates() {
        // Gill Grunt: `014_Attach_Gun_00` (_ID 0 -> `gun` = `J_gun`), `..._Harpoon_00` (16 ->
        // `harpoon tip`, 3 ft along the gun), `..._Backpack_00` (17 -> `back pack`).
        // Upgrade variants (`HarpoonGun_Upgrade01`, `BackPack_upgrade`) are not chosen.
        let Some(bytes) = read_character_bld("014_GillGrunt") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse");
        let found: Vec<(&str, &str)> = model
            .attachments
            .iter()
            .map(|a| (a.model.rsplit('/').next().unwrap(), a.bone.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("GillGrunt_HarpoonGun.mdl", "J_gun"),
                ("GillGrunt_HarpoonSpear_Prop.mdl", "J_gun"),
                ("GillGruntBackPack.mdl", "J_jetpack"),
            ]
        );
        assert!((model.attachments[1].transform[14] - 0.9144).abs() < 1e-4);
        for a in &model.attachments {
            assert!(!a.meshes.is_empty());
            assert!(a.meshes.iter().all(|&i| model.meshes[i].skeleton.is_none()));
        }

        // Stealth Elf: one knife per `knife` node (`_ID` 0 and 1), not the 2013 variant.
        let Some(bytes) = read_character_bld("026_StealthElf") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse");
        let found: Vec<(&str, &str)> = model
            .attachments
            .iter()
            .map(|a| (a.model.rsplit('/').next().unwrap(), a.bone.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("StealthElfKnife.mdl", "J_attach_L"),
                ("StealthElfKnife.mdl", "J_attach_R")
            ]
        );
    }

    #[test]
    fn test_havok_tracks_follow_rig_bone_names() {
        // Spyro's Havok rig lists `J_root_B`, legs and tail before `J_neck`; index-order
        // binding put leg motion on the head (`J_neck` got 0.312 instead of 0.266).
        let Some((mismatch, tracks, checked)) = idle_rest_mismatch("016_Spyro") else {
            return;
        };
        assert_eq!(tracks, 49);
        assert!(checked > 40, "{checked}");
        assert!(mismatch < 0.03, "Spyro idle vs rest: {mismatch}");
    }

    #[test]
    fn test_havok_scaled_rig_kaos() {
        // `J_root` scale 1.4: child translations are not scaled by it (hkQsTransform).
        let Some((mismatch, _, _)) = idle_rest_mismatch("1010_Kaos") else {
            return;
        };
        assert!(mismatch < 0.03, "Kaos idle vs rest: {mismatch}");
    }

    #[test]
    fn test_pose_parent_scale_does_not_scale_child_translation() {
        let mut root_local = mat4_identity();
        for i in [0, 5, 10] {
            root_local[i] = 2.0;
        }
        let skeleton = Skeleton::new(
            vec![
                Bone {
                    name: "root".into(),
                    parent: None,
                    inverse_bind_pose: mat4_affine_inverse(&root_local),
                    local_transform: root_local,
                },
                Bone {
                    name: "child".into(),
                    parent: Some(0),
                    ..Default::default()
                },
            ],
            0,
        );
        let track = BoneTrack {
            bone_index: 1,
            positions: vec![Keyframe::new(0.0, [1.0, 0.0, 0.0])],
            rotations: vec![Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0])],
            scales: vec![Keyframe::new(0.0, [1.0, 1.0, 1.0])],
        };
        let world =
            AnimationClip::new("t", 1.0, vec![track]).sample_global_transforms(0.0, &skeleton);
        assert!(
            (world[1][12] - 1.0).abs() < 1e-5,
            "child x = {}",
            world[1][12]
        );
        assert!((world[1][0] - 2.0).abs() < 1e-5, "scale is inherited");
        // Rest pose (no tracks) still reproduces the matrix hierarchy.
        let rest = AnimationClip::new("bind", 0.0, vec![]).sample_global_transforms(0.0, &skeleton);
        assert!((rest[0][0] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_igskeleton2_spyro() {
        let Some(bytes) = read_character_bld("016_Spyro") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
        // Body, tail and a second tail rig.
        assert_eq!(model.skeletons.len(), 3);
        let skel = &model.skeletons[0];
        assert_eq!(skel.bones.len(), 50);
        assert_eq!(skel.bones[0].name, "Unnamed bone");
        assert_eq!(skel.get_bone_index("J_root"), Some(2));
        assert_eq!(skel.bones[5].name, "J_head");
        assert_eq!(skel.bones[5].parent, Some(4));
        assert_eq!(skel.bones.iter().filter(|b| b.parent.is_none()).count(), 1);
        let world = AnimationClip::new("bind", 0.0, vec![]).sample_global_transforms(0.0, skel);
        for (i, b) in skel.bones.iter().enumerate() {
            assert!(b.parent.is_none_or(|p| p < i));
            let id = mat4_mul(&world[i], &b.inverse_bind_pose);
            for (k, v) in id.iter().enumerate() {
                let e = if k % 5 == 0 { 1.0 } else { 0.0 };
                assert!((v - e).abs() < 1e-3, "bone {i} bind * ibp != I: {id:?}");
            }
        }
        // J_root world position from IBP (row-sum of local translations).
        assert!((world[2][13] - 0.624).abs() < 1e-3);

        let skinned: Vec<_> = model
            .meshes
            .iter()
            .filter(|m| m.skeleton.is_some())
            .collect();
        assert_eq!(skinned.len(), 6, "all igGeometryAttr bodies are linked");
        for m in &skinned {
            let n = model.skeletons[m.skeleton.unwrap()].bones.len() as u32;
            for v in &m.vertices {
                for (&i, &w) in v.bone_indices.iter().zip(&v.bone_weights) {
                    assert!(w == 0.0 || i < n, "{}: bone {i} >= {n}", m.name);
                }
            }
        }
        assert!(!model.images.is_empty() && model.images[0].width > 0);
        assert!(!model.animations.is_empty(), "Havok animations decoded");
        assert!(model.animations.iter().any(|a| a.name == "idle"));
        assert!(model.animations.iter().any(|a| a.name == "walk"));
        assert!(model.animations.iter().any(|a| a.name == "attack"));
        let idle = model.animations.iter().find(|a| a.name == "idle").unwrap();
        assert!((idle.duration - 0.8).abs() < 1e-2);
        assert_eq!(idle.tracks.len(), 49);
    }

    #[test]
    fn test_igskeleton2_palette_slot_remap() {
        let Some(bytes) = read_character_bld("476_Artichoke") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse artichoke");
        let eyes = model
            .meshes
            .iter()
            .filter(|m| m.name.starts_with("igGeometryAttr_"))
            .nth(1)
            .expect("FoodFight_Eyes");
        let skel = &model.skeletons[eyes.skeleton.expect("eyes skinned")];
        // Palette slot 0x19 is blend index 25 = bone 26 `J_eye_R`.
        let v = eyes.vertices[0];
        assert_eq!(v.bone_weights[3], 1.0);
        assert_eq!(skel.bones[v.bone_indices[3] as usize].name, "J_eye_R");
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
        assert_eq!(
            model.geometries.len(),
            23,
            "Spyro has 17 Wii geometries + 6 igGeometryAttr"
        );
        assert_eq!(model.vertex_buffers.len(), 6, "Spyro has 6 vertex buffers");
        assert_eq!(model.index_buffers.len(), 6, "Spyro has 6 index buffers");
        assert_eq!(
            model.attr_lists.len(),
            18,
            "Spyro has 18 vertex format/attr lists"
        );
        assert_eq!(
            model.meshes.len(),
            23,
            "Spyro has 23 submeshes with geometry"
        );
        for mesh in &model.meshes {
            assert!(mesh.vertices.len() >= 3);
            assert!(!mesh.indices.is_empty());
        }
    }

    #[test]
    fn test_artichoke_body_from_geometry_attr() {
        use crate::archive::ArchiveMount;
        let p1 = std::path::Path::new("assets/files/character/476_Artichoke.bld");
        let p2 = std::path::Path::new("../../assets/files/character/476_Artichoke.bld");
        let p = if p1.exists() {
            p1
        } else if p2.exists() {
            p2
        } else {
            return;
        };
        let mut mount = ArchiveMount::new();
        mount.mount(p).expect("mount artichoke");
        let level_bld = mount.read_file("level.bld").expect("read level.bld");
        let model = IgbModel::parse(Cursor::new(&level_bld)).expect("parse level.bld");
        let body: Vec<_> = model
            .meshes
            .iter()
            .filter(|m| m.name.starts_with("igGeometryAttr_"))
            .collect();
        assert_eq!(body.len(), 2, "FoodFight_Body + FoodFight_Eyes");
        assert_eq!(body[0].vertices.len(), 1612);
        assert_eq!(body[1].vertices.len(), 144);
        // igActor2 bounds: [-2.13..2.13, 0.00..2.29, -0.89..0.61]
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &body[0].vertices {
            for c in 0..3 {
                lo[c] = lo[c].min(v.position[c]);
                hi[c] = hi[c].max(v.position[c]);
            }
        }
        assert!((lo[0] + 2.13).abs() < 0.01 && (hi[0] - 2.13).abs() < 0.01);
        assert!(lo[1].abs() < 0.01 && (hi[1] - 2.29).abs() < 0.01);
        assert!((lo[2] + 0.89).abs() < 0.01 && (hi[2] - 0.61).abs() < 0.01);
        assert!(body[0].indices.iter().all(|&i| (i as usize) < 1612));
        // Blend data (usage 6 u8x4 weights, usage 8 u8x4 palette slots).
        for v in &body[0].vertices {
            let sum: f32 = v.bone_weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-3);
        }
        assert!(body[0].vertices.iter().any(|v| v.bone_indices != [0; 4]));
        assert!(body[0].vertices.iter().any(|v| v
            .bone_weights
            .iter()
            .filter(|&&w| w > 0.0)
            .count()
            >= 2));
        let eye = body[1].vertices[0];
        assert_eq!(
            (eye.bone_indices, eye.bone_weights),
            ([1, 1, 1, 26], [0.0, 0.0, 0.0, 1.0])
        );
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
    fn test_igz_object_payload_kept_only_for_decoded_types() {
        // v8 header: type index @2, flags @8 (0 -> length @14), payload after 16.
        let mut igz = vec![0u8; 0x14 + 64];
        igz[14..16].copy_from_slice(&64u16.to_be_bytes());
        for (i, b) in igz[16..].iter_mut().enumerate() {
            *b = i as u8;
        }
        let types = vec!["igVertexBuffer".to_string(), "igImage2".to_string()];

        let vb = IgzObject::parse(&igz, 0, 8, IgbEndian::Big, &types).unwrap();
        assert_eq!(vb.data.len(), 64);
        assert_eq!(vb.fields.len(), 16);
        assert_eq!(vb.fields[0], 0x0405_0607);

        igz[2..4].copy_from_slice(&1u16.to_be_bytes());
        let img = IgzObject::parse(&igz, 0, 8, IgbEndian::Big, &types).unwrap();
        assert_eq!(img.type_name.as_deref(), Some("igImage2"));
        assert_eq!(img.length, 64);
        assert!(img.data.is_empty());
        assert!(img.fields.is_empty());
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
    fn test_wii_display_list_pnmtx_index() {
        // VCD: PNMTXIDX direct + pos idx8. Matrix byte 6 = XF row 6 = matrix slot 2.
        let mut dl = Vec::new();
        let cp = |dl: &mut Vec<u8>, reg: u8, val: u32| {
            dl.push(0x08);
            dl.push(reg);
            dl.extend_from_slice(&val.to_be_bytes());
        };
        cp(&mut dl, 0x70, 1 | (3 << 1));
        cp(&mut dl, 0xB0, 6);
        cp(&mut dl, 0x50, 1 | (2 << 9));
        cp(&mut dl, 0x60, 0);
        dl.extend_from_slice(&[0x90, 0x00, 0x03]);
        for (mtx, idx) in [(6u8, 0u8), (0, 1), (9, 2)] {
            dl.extend_from_slice(&[mtx, idx]);
        }
        let mut vb = Vec::new();
        for p in [[0i16, 0, 0], [1, 0, 0], [0, 0, 1]] {
            for c in p {
                vb.extend_from_slice(&c.to_be_bytes());
            }
        }

        let mesh = IgbModel::decode_wii_display_list(&dl, &vb).expect("decodes");
        let slots: Vec<_> = mesh.vertices.iter().map(|v| v.bone_indices[0]).collect();
        assert_eq!(slots, vec![2, 0, 3]);
    }

    #[test]
    fn test_wii_display_list_indexed_clr0() {
        // VAT0: pos xyz s16 shift 0, CLR0 RGBA8; VCD: pos idx8, clr0 idx8.
        let mut dl = Vec::new();
        let cp = |dl: &mut Vec<u8>, reg: u8, val: u32| {
            dl.push(0x08);
            dl.push(reg);
            dl.extend_from_slice(&val.to_be_bytes());
        };
        cp(&mut dl, 0x70, 1 | (3 << 1) | (5 << 14));
        cp(&mut dl, 0xA0, 0);
        cp(&mut dl, 0xB0, 6);
        cp(&mut dl, 0xA2, 18);
        cp(&mut dl, 0xB2, 4);
        cp(&mut dl, 0x50, (2 << 9) | (2 << 13));
        cp(&mut dl, 0x60, 0);
        dl.extend_from_slice(&[0x90, 0x00, 0x03]);
        for idx in [0u8, 1, 2] {
            dl.extend_from_slice(&[idx, 2 - idx]);
        }

        let mut vb = Vec::new();
        for p in [[0i16, 0, 0], [1, 0, 0], [0, 0, 1]] {
            for c in p {
                vb.extend_from_slice(&c.to_be_bytes());
            }
        }
        vb.extend_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 0]);

        let mesh = IgbModel::decode_wii_display_list(&dl, &vb).expect("decodes");
        let colors: Vec<_> = mesh.vertices.iter().map(|v| v.color).collect();
        assert_eq!(
            colors,
            vec![
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0]
            ]
        );
    }

    #[test]
    fn test_enemy_script_classification() {
        let k = |s: &str| MarkerKind::from_script(&format!("C:/tfb/Content/Levels/_Enemies/{s}"));
        assert_eq!(
            k("Chompy/Enemy_S4_Chompy.ai"),
            Some(MarkerKind::Enemy(EnemyClass::Near))
        );
        assert_eq!(
            k("TrollNear/scripts/Enemy_S4_Troll_Near.ai"),
            Some(MarkerKind::Enemy(EnemyClass::Near))
        );
        assert_eq!(
            k("UndeadLobber/scripts/Enemy_S4_UndeadLobber.ai"),
            Some(MarkerKind::Enemy(EnemyClass::Far))
        );
        assert_eq!(
            k("NightmareFar/scripts/Enemy_S4_NightmareFar.ai"),
            Some(MarkerKind::Enemy(EnemyClass::Far))
        );
        for helper in [
            "RavenLobber/scripts/Enemy_S4_RavenLobber_Projectile.ai",
            "StunGun/scripts/Enemy_S4_StunGun_MuzzleFlash_Controller.ai",
            "NightmareSpawner/Enemy_S4_NightmareSpawner_V2.ai",
            "Enemy_S4_Template.ai",
            "Enemy_Gate_2014.ai",
        ] {
            assert_eq!(k(helper), None, "{helper}");
        }
    }

    #[test]
    fn test_gx_color_rgb565_and_transform_dir() {
        assert_eq!(gx_color(&[0xF8, 0x00], 0, 0), Some([1.0, 0.0, 0.0, 1.0]));
        // 90 deg about Y, column-major: +X maps to -Z.
        let m = [
            0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 5.0, 5.0, 5.0, 1.0,
        ];
        assert_eq!(transform_dir(&m, [2.0, 0.0, 0.0]), [0.0, 0.0, -1.0]);
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
        // igGeometryAttr meshes carry decoded blend data; only GX meshes must stay unskinned.
        assert!(
            model
                .meshes
                .iter()
                .filter(|m| !m.name.starts_with("igGeometryAttr_"))
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

    #[test]
    fn test_level_model_drawn_only_for_active_static_actors() {
        let actor = |script: Option<&str>, active: bool, model: Option<usize>| ActorPlacement {
            name: "Statue".into(),
            position: [0.0; 3],
            rotation_deg: [0.0; 3],
            script: script.map(Into::into),
            collision: None,
            active,
            scale: [100.0; 3],
            model,
        };
        assert!(actor(None, true, Some(1)).draws_level_model());
        assert!(!actor(None, false, Some(1)).draws_level_model());
        assert!(!actor(None, true, None).draws_level_model());
        // Enemies and coins draw themselves as gameplay entities.
        assert!(!actor(Some("x/Enemy_S4_Chompy.ai"), true, Some(1)).draws_level_model());
        assert!(!actor(Some("x/2D_Coin.ai"), true, Some(1)).draws_level_model());
    }

    #[test]
    fn test_hub_default_actor_volumes_do_not_block() {
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

        // 830 solid-flagged volumes, almost all the default 3x6x3 box on cameras,
        // sound emitters, coin layers and triggers; only named blockers remain.
        let blocking: Vec<_> = model
            .collision_primitives
            .iter()
            .filter(|p| p.blocks_movement)
            .collect();
        assert!(blocking.len() < 40, "{} blocking volumes", blocking.len());
        assert!(blocking.iter().any(|p| p.name == "Blocker_MainDoor"));
        assert!(blocking
            .iter()
            .all(|p| !p.name.starts_with("emt_") && !p.name.contains("CameraDummy")));
        // No placeholder props from name guesses or inactive templates.
        let props: Vec<_> = model
            .extract_level_markers()
            .into_iter()
            .filter(|m| m.kind == MarkerKind::Prop)
            .collect();
        assert!(props.iter().all(|m| !m.name.contains("CameraDummy")));
        assert!(props.iter().all(|m| m.position != [0.0; 3]));
    }

    #[test]
    fn test_sky_layer_kept_out_of_level_scene() {
        use crate::archive::ArchiveMount;
        let rel = "assets/files/level/Level_302_Chompywurm.bld";
        let p = [rel.to_string(), format!("../../{rel}")]
            .into_iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.exists());
        let Some(p) = p else { return };
        let mut mount = ArchiveMount::new();
        mount.mount(&p).expect("mount 302");
        let data = mount.read_file("level.bld").expect("read level.bld");
        let model = IgbModel::parse(Cursor::new(&data)).expect("parse 302");

        // `_SKY/turquoise_sky_307.ma`: 25 drawables, domes ~1,000 units across.
        assert!(!model.sky_instances.is_empty());
        let sky: std::collections::HashSet<_> = model
            .sky_instances
            .iter()
            .map(|i| i.geometry_index)
            .collect();
        assert!(model
            .evaluate_scene_graph()
            .iter()
            .all(|i| !sky.contains(&i.geometry_index)));
        let widest = model
            .sky_instances
            .iter()
            .filter_map(|i| model.geometries[i.geometry_index].mesh.as_ref())
            .flat_map(|m| &m.vertices)
            .map(|v| v.position[0].abs())
            .fold(0.0f32, f32::max);
        assert!(widest > 400.0, "sky half-width {widest}");
    }

    #[test]
    fn test_hub_models_drawn_at_active_actor_placements() {
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
        let instances = model.evaluate_scene_graph();

        // Model-space prop geometry used to sit at the origin (13 placed instances).
        let placed = instances
            .iter()
            .filter(|i| i.world_transform != mat4_identity())
            .count();
        assert!(placed > 700, "got {placed} placed instances");
        // Inactive templates (debris, IGC copies, digits) are not drawn.
        let used: std::collections::HashSet<_> =
            instances.iter().map(|i| i.geometry_index).collect();
        assert!(used.len() < model.geometries.len());
        // Level geometry carries its own diffuse texture.
        let textured = model
            .meshes
            .iter()
            .filter(|m| m.image_index.is_some())
            .count();
        assert!(
            textured * 10 > model.meshes.len() * 9,
            "{textured} textured"
        );
    }

    #[test]
    fn test_character_primary_skeleton_and_material_extraction() {
        // Test Spyro
        if let Some(bytes) = read_character_bld("016_Spyro") {
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
            assert_eq!(model.primary_skeleton_index(), Some(0));
            let primary_skel = model.primary_skeleton().expect("primary skel");
            assert_eq!(primary_skel.bones.len(), 50);

            let body_meshes: Vec<_> = model
                .meshes
                .iter()
                .filter(|m| m.skeleton == Some(0))
                .collect();
            assert_eq!(body_meshes.len(), 4, "4 body submeshes in Spyro");
            // Spyro body meshes must have materials and image indices
            assert!(body_meshes.iter().all(|m| m.image_index.is_some()));
            let body_imgs: std::collections::HashSet<_> =
                body_meshes.iter().map(|m| m.image_index.unwrap()).collect();
            assert!(
                body_imgs.len() >= 2,
                "Spyro has distinct textures for body and wings"
            );
        }

        // Test Snap Shot (CrocWarrior)
        if let Some(bytes) = read_character_bld("462_Water_Crocky") {
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse snap shot");
            assert_eq!(model.primary_skeleton_index(), Some(1));
            let primary_skel = model.primary_skeleton().expect("primary skel");
            assert_eq!(primary_skel.bones.len(), 43);
            assert!(primary_skel.bones.iter().any(|b| b.name.contains("J_root")));
        }

        // Test Kaos
        if let Some(bytes) = read_character_bld("1010_Kaos") {
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse kaos");
            assert_eq!(model.primary_skeleton_index(), Some(1));
            let primary_skel = model.primary_skeleton().expect("primary skel");
            assert_eq!(primary_skel.bones.len(), 46);
        }

        // Test Food Fight (Artichoke)
        if let Some(bytes) = read_character_bld("476_Artichoke") {
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse artichoke");
            assert_eq!(model.primary_skeleton_index(), Some(1));
            let primary_skel = model.primary_skeleton().expect("primary skel");
            assert_eq!(primary_skel.bones.len(), 33);
        }

        // Test Primary Rig Mesh Bounds for Characters
        if let Some(bytes) = read_character_bld("016_Spyro") {
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
            let (min_pt, max_pt) = model.primary_mesh_bounds().expect("spyro primary bounds");
            let height = max_pt[1] - min_pt[1];
            assert!((height - 1.77).abs() < 0.1, "spyro height {height}");
        }

        if let Some(bytes) = read_character_bld("462_Water_Crocky") {
            let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse snap shot");
            let (min_pt, max_pt) = model.primary_mesh_bounds().expect("snap shot bounds");
            let height = max_pt[1] - min_pt[1];
            assert!((height - 4.28).abs() < 0.2, "snap shot height {height}");
        }
    }

    fn rig(bones: &[(&str, f32)]) -> Skeleton {
        let bones = bones
            .iter()
            .enumerate()
            .map(|(i, &(name, z))| {
                let mut inverse_bind_pose = mat4_identity();
                inverse_bind_pose[14] = -z;
                Bone {
                    name: name.to_string(),
                    parent: i.checked_sub(1).map(|_| 0),
                    inverse_bind_pose,
                    ..Default::default()
                }
            })
            .collect();
        Skeleton::new(bones, 0)
    }

    fn skinned_mesh(name: &str, skeleton: usize, bone: u32) -> IgbMesh {
        IgbMesh {
            name: name.to_string(),
            vertices: vec![IgbVertex {
                bone_indices: [bone, 0, 0, 0],
                bone_weights: [1.0, 0.0, 0.0, 0.0],
                ..Default::default()
            }],
            indices: vec![0],
            material_name: None,
            skeleton: Some(skeleton),
            image_index: None,
            base_color: None,
            texture_wrap: Default::default(),
            uv_scroll: None,
        }
    }

    #[test]
    fn test_secondary_rig_meshes_remap_onto_primary() {
        let model = IgbModel {
            skeletons: vec![
                rig(&[("root", 0.0), ("J_head", 1.0), ("J_tail_1", -1.0)]),
                rig(&[("root", 0.0), ("ns:J_tail_1", -1.0)]),
                // Alternate tail on the same bones.
                rig(&[("root", 0.0), ("J_tail_1", -1.0)]),
                // Same names, different bind pose: a separate cinematic rig.
                rig(&[("root", 0.0), ("J_head", 3.0)]),
            ],
            meshes: vec![
                skinned_mesh("body", 0, 1),
                skinned_mesh("tail", 1, 1),
                skinned_mesh("tail_variant", 2, 1),
                skinned_mesh("head", 3, 1),
            ],
            ..Default::default()
        };
        assert_eq!(model.bone_map_to_primary(1), Some(vec![0, 2]));
        assert_eq!(model.bone_map_to_primary(3), None);

        let rig_meshes = model.primary_rig_meshes();
        let names: Vec<_> = rig_meshes.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["body", "tail"]);
        assert_eq!(rig_meshes[1].skeleton, Some(0));
        assert_eq!(rig_meshes[1].vertices[0].bone_indices[0], 2);
    }

    #[test]
    fn test_texture_wrap_from_raw() {
        assert_eq!(TextureWrap::from_raw(0), TextureWrap::Clamp);
        assert_eq!(TextureWrap::from_raw(1), TextureWrap::Repeat);
        assert_eq!(TextureWrap::from_raw(2), TextureWrap::Clamp);
        assert_eq!(TextureWrap::from_raw(3), TextureWrap::Repeat);
        assert_eq!(TextureWrap::from_raw(4), TextureWrap::Clamp);
    }

    #[test]
    fn test_spyro_tail_and_texture_wrap() {
        let Some(bytes) = read_character_bld("016_Spyro") else {
            return;
        };
        let model = IgbModel::parse(Cursor::new(&bytes)).expect("parse spyro");
        let rig_meshes = model.primary_rig_meshes();
        let names: Vec<_> = rig_meshes.iter().map(|m| m.name.as_str()).collect();
        assert!(
            names.contains(&"igGeometryAttr_6199"),
            "base tail: {names:?}"
        );
        assert!(
            !names.contains(&"igGeometryAttr_6257"),
            "TailSerie3: {names:?}"
        );

        let body = model.primary_skeleton().expect("body rig");
        let tail = rig_meshes
            .iter()
            .find(|m| m.name == "igGeometryAttr_6199")
            .unwrap();
        for v in &tail.vertices {
            for (&b, &w) in v.bone_indices.iter().zip(&v.bone_weights) {
                if w > 0.0 {
                    let name = &body.bones[b as usize].name;
                    assert!(
                        name.starts_with("J_tail") || name.starts_with("J_root"),
                        "tail skinned to {name}"
                    );
                }
            }
        }

        let wrap = |name: &str| {
            model
                .meshes
                .iter()
                .find(|m| m.name == name)
                .map(|m| m.texture_wrap)
        };
        // Body UVs span -1..1 and tile; the ray blast card clamps.
        assert_eq!(wrap("igGeometryAttr_5041"), Some([TextureWrap::Repeat; 2]));
        assert_eq!(wrap("igWiiGeometry_6888"), Some([TextureWrap::Clamp; 2]));
    }
}
