pub mod archive;
pub mod dsp;
pub mod fsb;
pub mod havok;
pub mod igb;
pub mod loader;
pub mod texture;

pub use archive::{hash_path, ArchiveMount, ArchiveReader, FileEntry};
pub use dsp::{decode_dsp_adpcm, DspChannelState, DspError};
pub use fsb::{parse_fsb5, AudioClip, FsbError, FsbHeader};
pub use havok::{parse_havok_animation, HAVOK_TAG};
pub use igb::{
    compose_transform, lerp_vec3, mat4_identity, mat4_mul, quat_to_mat4, sample_quat, sample_vec3,
    slerp_quat, transform_point, AnimationClip, AttrFormat, AttrSemantic, Bone, BoneTrack,
    IgbAttrDesc, IgbAttrList, IgbDirectoryEntry, IgbEndian, IgbError, IgbGeometryNode, IgbHeader,
    IgbIndexBuffer, IgbMaterial, IgbMesh, IgbModel, IgbObjectType, IgbSceneNode, IgbTextureRef,
    IgbVertex, IgbVertexBuffer, IgzChunkDescriptor, IgzHeader, IgzImage2, IgzObject, IgzObjectList,
    IgzOffsetMapFixup, IndexTopology, Keyframe, LevelMarker, MarkerKind, SceneInstance, Skeleton,
};
pub use loader::{AssetError, AssetLoader};
pub use texture::{decode_cmpr, decode_dxt1, decode_dxt1_block, decode_rgb565, TextureError};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::Path;

    #[test]
    fn test_meta_objects_and_symbols_registry() {
        let meta_path = Path::new("../../tools/rea-bridge/meta_objects.tsv");
        let sym_path = Path::new("../../tools/rea-bridge/symbols.txt");

        if !meta_path.exists() || !sym_path.exists() {
            return;
        }

        let meta_file = File::open(meta_path).expect("open meta_objects.tsv");
        let reader = BufReader::new(meta_file);
        let mut class_sizes: HashMap<String, u64> = HashMap::new();

        for (i, line) in reader.lines().enumerate() {
            let line = line.expect("read line");
            if i == 0 {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let name = parts[0].to_string();
                let size_str = parts[1].trim_start_matches("0x");
                if let Ok(sz) = u64::from_str_radix(size_str, 16) {
                    class_sizes.insert(name, sz);
                }
            }
        }

        assert!(
            class_sizes.len() >= 1000,
            "expected >= 1000 classes, found {}",
            class_sizes.len()
        );
        assert_eq!(class_sizes.get("igMetaObject"), Some(&0x48));
        assert_eq!(class_sizes.get("igSkeleton2"), Some(&0x18));
        assert_eq!(class_sizes.get("igWiiGeometry"), Some(&0x40));
        assert_eq!(class_sizes.get("igImage2"), Some(&0x60));

        let sym_file = File::open(sym_path).expect("open symbols.txt");
        let sym_reader = BufReader::new(sym_file);
        let mut symbols = HashMap::new();

        for line in sym_reader.lines() {
            let line = line.expect("read symbol line");
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                symbols.insert(parts[0].to_string(), parts[1].to_string());
            }
        }

        assert_eq!(
            symbols.get("0x801062e0"),
            Some(&"igMetaObject::Register".to_string())
        );
    }
}
