pub mod archive;
pub mod dsp;
pub mod fsb;
pub mod igb;
pub mod loader;
pub mod texture;

pub use archive::{hash_path, ArchiveMount, ArchiveReader, FileEntry};
pub use dsp::{decode_dsp_adpcm, DspChannelState, DspError};
pub use fsb::{parse_fsb5, AudioClip, FsbError, FsbHeader};
pub use igb::{
    compose_transform, lerp_vec3, mat4_identity, mat4_mul, quat_to_mat4, sample_quat, sample_vec3,
    slerp_quat, AnimationClip, AttrFormat, AttrSemantic, Bone, BoneTrack, IgbAttrDesc, IgbAttrList,
    IgbDirectoryEntry, IgbEndian, IgbError, IgbGeometryNode, IgbHeader, IgbIndexBuffer,
    IgbMaterial, IgbMesh, IgbModel, IgbObjectType, IgbSceneNode, IgbTextureRef, IgbVertex,
    IgbVertexBuffer, IgzChunkDescriptor, IgzHeader, IgzImage2, IgzObject, IgzObjectList,
    IgzOffsetMapFixup, IndexTopology, Keyframe, LevelMarker, MarkerKind, SceneInstance, Skeleton,
};
pub use loader::{AssetError, AssetLoader};
pub use texture::{decode_cmpr, decode_dxt1, decode_dxt1_block, decode_rgb565, TextureError};
