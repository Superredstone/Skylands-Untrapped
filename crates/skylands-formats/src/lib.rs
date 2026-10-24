pub mod archive;
pub mod dsp;
pub mod fsb;
pub mod havok;
pub mod igb;
pub mod loader;
pub mod script;
pub mod sprite;
pub mod texture;

pub use archive::{hash_path, ArchiveMount, ArchiveReader, FileEntry};
pub use dsp::{decode_dsp_adpcm, DspChannelState, DspError};
pub use fsb::{parse_fsb5, AudioClip, FsbError, FsbHeader};
pub use havok::{parse_havok_animation, parse_havok_skeleton, HavokRig, HAVOK_TAG};
pub use igb::{
    compose_transform, lerp_vec3, mat4_identity, mat4_mul, meta_fields, quat_to_mat4, sample_quat,
    sample_vec3, slerp_quat, transform_point, ActorPlacement, AnimationClip, AttrFormat,
    AttrSemantic, Bone, BoneTrack, CollisionPrimitive, CollisionShapeType, IgbAttrDesc,
    IgbAttrList, IgbDirectoryEntry, IgbEndian, IgbError, IgbGeometryNode, IgbHeader,
    IgbIndexBuffer, IgbMaterial, IgbMesh, IgbModel, IgbObjectType, IgbSceneNode, IgbTextureRef,
    IgbVertex, IgbVertexBuffer, IgzChunkDescriptor, IgzFile, IgzHeader, IgzImage2, IgzObject,
    IgzObjectList, IgzOffsetMapFixup, IndexTopology, Keyframe, LevelMarker, MarkerKind,
    SceneInstance, ScriptValue, Skeleton,
};
pub use loader::{AssetError, AssetLoader};
pub use script::{enemy_stats, EnemyStats, LevelupTable, ScriptSet, Vm, VmError};
pub use sprite::{SpriteDef, SpriteLibrary};
pub use texture::{
    decode_cmpr, decode_dxt1, decode_dxt1_block, decode_gx_rgba8, decode_rgb565, TextureError,
};

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

    #[test]
    fn test_meta_fields_registry() {
        let fields_path = Path::new("../../tools/rea-bridge/meta_fields.tsv");
        if !fields_path.exists() {
            return;
        }

        let file = File::open(fields_path).expect("open meta_fields.tsv");
        let reader = BufReader::new(file);
        let mut fields: HashMap<(String, String), (String, usize, usize)> = HashMap::new();

        for (i, line) in reader.lines().enumerate() {
            let line = line.expect("read line");
            if i == 0 {
                continue;
            }
            let p: Vec<&str> = line.split('\t').collect();
            if p.len() >= 5 {
                let cls = p[0].to_string();
                let fld = p[1].to_string();
                let ty = p[2].to_string();
                let off = usize::from_str_radix(p[3].trim_start_matches("0x"), 16).unwrap_or(0);
                let sz = usize::from_str_radix(p[4].trim_start_matches("0x"), 16).unwrap_or(0);
                fields.insert((cls, fld), (ty, off, sz));
            }
        }

        assert!(
            fields.len() >= 2000,
            "expected >= 2000 fields, found {}",
            fields.len()
        );

        // Verify igWiiGeometry fields match meta_fields constants
        let dl = fields
            .get(&("igWiiGeometry".to_string(), "_displayList".to_string()))
            .expect("igWiiGeometry._displayList");
        assert_eq!(dl.1, crate::igb::meta_fields::ig_wii_geometry::DISPLAY_LIST);

        let vb = fields
            .get(&("igWiiGeometry".to_string(), "_vertexBuffer".to_string()))
            .expect("igWiiGeometry._vertexBuffer");
        assert_eq!(
            vb.1,
            crate::igb::meta_fields::ig_wii_geometry::VERTEX_BUFFER
        );

        // Verify igImage2 fields match meta_fields constants
        let width = fields
            .get(&("igImage2".to_string(), "_width".to_string()))
            .expect("igImage2._width");
        assert_eq!(width.1, crate::igb::meta_fields::ig_image2::WIDTH);

        let height = fields
            .get(&("igImage2".to_string(), "_height".to_string()))
            .expect("igImage2._height");
        assert_eq!(height.1, crate::igb::meta_fields::ig_image2::HEIGHT);

        let fmt = fields
            .get(&("igImage2".to_string(), "_format".to_string()))
            .expect("igImage2._format");
        assert_eq!(fmt.1, crate::igb::meta_fields::ig_image2::FORMAT);

        let data = fields
            .get(&("igImage2".to_string(), "_data".to_string()))
            .expect("igImage2._data");
        assert_eq!(data.1, crate::igb::meta_fields::ig_image2::DATA);

        // Verify igGeometryAttr fields match meta_fields constants
        let ga_vb = fields
            .get(&("igGeometryAttr".to_string(), "_vertexBuffer".to_string()))
            .expect("igGeometryAttr._vertexBuffer");
        assert_eq!(
            ga_vb.1,
            crate::igb::meta_fields::ig_geometry_attr::VERTEX_BUFFER
        );

        let ga_ib = fields
            .get(&("igGeometryAttr".to_string(), "_indexBuffer".to_string()))
            .expect("igGeometryAttr._indexBuffer");
        assert_eq!(
            ga_ib.1,
            crate::igb::meta_fields::ig_geometry_attr::INDEX_BUFFER
        );

        // Verify CollisionInfo and Placement collision fields
        let col_w = fields
            .get(&("CollisionInfo".to_string(), "_width".to_string()))
            .expect("CollisionInfo._width");
        assert_eq!(col_w.1, crate::igb::meta_fields::collision_info::WIDTH);

        let place_col = fields
            .get(&("Placement".to_string(), "_collision".to_string()))
            .expect("Placement._collision");
        assert_eq!(place_col.1, crate::igb::meta_fields::placement::COLLISION);
    }
}
