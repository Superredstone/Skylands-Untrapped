use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

use crate::archive::{ArchiveError, ArchiveMount};
use crate::fsb::{parse_fsb5, AudioClip, FsbError};
use crate::igb::{IgbError, IgbModel};
use crate::texture::{decode_cmpr, decode_dxt1, TextureError};

#[derive(Error, Debug)]
pub enum AssetError {
    #[error("Archive error: {0}")]
    Archive(#[from] ArchiveError),
    #[error("IGB error: {0}")]
    Igb(#[from] IgbError),
    #[error("Texture error: {0}")]
    Texture(#[from] TextureError),
    #[error("FSB audio error: {0}")]
    Fsb(#[from] FsbError),
    #[error("Asset not found: {0}")]
    NotFound(String),
}

pub struct AssetLoader {
    vfs: ArchiveMount,
    raw_cache: Mutex<HashMap<String, Arc<Vec<u8>>>>,
    igb_cache: Mutex<HashMap<String, Arc<IgbModel>>>,
}

impl AssetLoader {
    pub fn new(vfs: ArchiveMount) -> Self {
        Self {
            vfs,
            raw_cache: Mutex::new(HashMap::new()),
            igb_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn vfs(&self) -> &ArchiveMount {
        &self.vfs
    }

    pub fn vfs_mut(&mut self) -> &mut ArchiveMount {
        &mut self.vfs
    }

    pub fn mount<P: AsRef<Path>>(&mut self, path: P) -> Result<(), ArchiveError> {
        self.vfs.mount(path)
    }

    pub fn mount_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, ArchiveError> {
        self.vfs.mount_dir(dir)
    }

    pub fn has_file(&self, path: &str) -> bool {
        self.vfs.contains_path(path) || self.vfs.has_archive(path)
    }

    pub fn load_raw(&self, path: &str) -> Result<Arc<Vec<u8>>, AssetError> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        {
            let cache = self.raw_cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        let bytes = self.vfs.read_file(path)?;
        let arc_data = Arc::new(bytes);
        // ponytail: unbounded raw cache; add LRU capacity bound when resident memory exceeds 512MB
        self.raw_cache.lock().unwrap().insert(key, arc_data.clone());
        Ok(arc_data)
    }

    pub fn load_igb(&self, path: &str) -> Result<Arc<IgbModel>, AssetError> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        {
            let cache = self.igb_cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        let raw = self.load_raw(path)?;
        let model = match IgbModel::parse(&raw[..]) {
            Ok(m) => m,
            Err(e) => {
                let candidates = ArchiveMount::scan_embedded_igb(&raw);
                if let Some(first_valid) = candidates
                    .into_iter()
                    .find_map(|sub| IgbModel::parse(sub).ok())
                {
                    first_valid
                } else {
                    return Err(AssetError::Igb(e));
                }
            }
        };
        let arc_model = Arc::new(model);
        // ponytail: unbounded IGB cache; add LRU capacity bound when model count exceeds 256
        self.igb_cache
            .lock()
            .unwrap()
            .insert(key, arc_model.clone());
        Ok(arc_model)
    }

    pub fn load_level_models(&self, path: &str) -> Result<Vec<Arc<IgbModel>>, AssetError> {
        let raw = self.load_raw(path)?;
        let chunks = ArchiveMount::scan_embedded_igb(&raw);
        let mut models = Vec::new();
        for chunk in chunks {
            if let Ok(model) = IgbModel::parse(chunk) {
                models.push(Arc::new(model));
            }
        }
        if models.is_empty() {
            if let Ok(single) = IgbModel::parse(&raw[..]) {
                models.push(Arc::new(single));
            }
        }
        if models.is_empty() {
            Err(AssetError::NotFound(format!(
                "No valid IGB models in {}",
                path
            )))
        } else {
            Ok(models)
        }
    }

    pub fn load_cmpr_rgba(
        &self,
        path: &str,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, AssetError> {
        let raw = if path.ends_with(".bld") || path.ends_with(".arc") {
            if let Ok(inner) = self.vfs.read_from_archive(path, "level.bld") {
                Arc::new(inner)
            } else {
                self.load_raw(path)?
            }
        } else {
            self.load_raw(path)?
        };
        let bytes: &[u8] = if raw.len() >= 64 && &raw[0..4] == b"IGZ\x01" {
            let soff = u32::from_be_bytes([raw[56], raw[57], raw[58], raw[59]]) as usize;
            if soff < raw.len() {
                &raw[soff..]
            } else {
                &raw[..]
            }
        } else {
            &raw[..]
        };
        let rgba = decode_cmpr(bytes, width, height)?;
        Ok(rgba)
    }

    pub fn load_dxt1_rgba(
        &self,
        path: &str,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, AssetError> {
        let raw = self.load_raw(path)?;
        let rgba = decode_dxt1(&raw, width, height)?;
        Ok(rgba)
    }

    pub fn load_companion_texture_rgba(
        &self,
        model_path: &str,
        preferred_name: Option<&str>,
    ) -> Result<(u32, u32, Vec<u8>), AssetError> {
        let clean = model_path.replace('\\', "/");
        let name = clean.rsplit('/').next().unwrap_or(&clean);
        let arc_name = name.strip_suffix(".bld").unwrap_or(name).to_string() + ".arc";

        if !self.vfs.has_archive(&arc_name) {
            return Err(AssetError::NotFound(format!(
                "Companion archive {} not found for {}",
                arc_name, model_path
            )));
        }

        let tex_files = self.vfs.find_archive_texture_files(&arc_name);
        let chosen_file = if let Some(pref) = preferred_name {
            let pref_clean = pref.replace('\\', "/");
            let pref_stem = pref_clean
                .rsplit('/')
                .next()
                .unwrap_or(&pref_clean)
                .to_ascii_lowercase();
            tex_files
                .iter()
                .find(|f| f.to_ascii_lowercase().contains(&pref_stem))
                .cloned()
                .or_else(|| tex_files.first().cloned())
        } else {
            tex_files.first().cloned()
        };

        let raw = if let Some(ref file) = chosen_file {
            self.vfs.read_from_archive(&arc_name, file)?
        } else if let Ok(inner) = self.vfs.read_from_archive(&arc_name, "level.bld") {
            inner
        } else {
            return Err(AssetError::NotFound(format!(
                "No texture files in companion archive {}",
                arc_name
            )));
        };

        if raw.len() >= 4 && &raw[0..4] == b"IGZ\x01" {
            if let Ok(tex_model) = IgbModel::parse_igz(&raw) {
                if let Some(first_img) = tex_model.images.first() {
                    let width = first_img.width as u32;
                    let height = first_img.height as u32;
                    if width > 0 && height > 0 && first_img.texture_size > 0 {
                        let end =
                            (first_img.texture_offset + first_img.texture_size).min(raw.len());
                        if first_img.texture_offset < end {
                            let slice = &raw[first_img.texture_offset..end];
                            if let Ok(rgba) = decode_cmpr(slice, width, height) {
                                return Ok((width, height, rgba));
                            }
                        }
                    }
                }
            }
            if raw.len() >= 64 {
                let soff = u32::from_be_bytes([raw[56], raw[57], raw[58], raw[59]]) as usize;
                let slice = if soff < raw.len() {
                    &raw[soff..]
                } else {
                    &raw[..]
                };
                if let Ok(rgba) = decode_cmpr(slice, 128, 128) {
                    return Ok((128, 128, rgba));
                }
            }
        }

        let rgba = decode_cmpr(&raw, 128, 128)?;
        Ok((128, 128, rgba))
    }

    pub fn load_first_image_rgba(
        &self,
        model_path: &str,
    ) -> Result<(u32, u32, Vec<u8>), AssetError> {
        let model = self.load_igb(model_path);
        if let Ok(ref m) = model {
            if let Some(first_img) = m.images.first() {
                let width = first_img.width as u32;
                let height = first_img.height as u32;
                if width > 0 && height > 0 && first_img.texture_size > 0 {
                    let raw = if model_path.ends_with(".bld") || model_path.ends_with(".arc") {
                        if let Ok(inner) = self.vfs.read_from_archive(model_path, "level.bld") {
                            Arc::new(inner)
                        } else {
                            self.load_raw(model_path)?
                        }
                    } else {
                        self.load_raw(model_path)?
                    };
                    let end = (first_img.texture_offset + first_img.texture_size).min(raw.len());
                    if first_img.texture_offset < end {
                        let slice = &raw[first_img.texture_offset..end];
                        if let Ok(rgba) = decode_cmpr(slice, width, height) {
                            return Ok((width, height, rgba));
                        }
                    }
                }
            }
            let preferred_name = m
                .materials
                .iter()
                .find_map(|mat| mat.texture_name.as_deref());
            if let Ok(paired) = self.load_companion_texture_rgba(model_path, preferred_name) {
                return Ok(paired);
            }
        }

        if let Ok(paired) = self.load_companion_texture_rgba(model_path, None) {
            return Ok(paired);
        }

        let fallback_rgba = self.load_cmpr_rgba(model_path, 128, 128)?;
        Ok((128, 128, fallback_rgba))
    }

    pub fn load_audio(&self, path: &str) -> Result<AudioClip, AssetError> {
        let raw = self.load_raw(path)?;
        let clip = parse_fsb5(&raw)?;
        Ok(clip)
    }

    pub fn clear_cache(&self) {
        self.raw_cache.lock().unwrap().clear();
        self.igb_cache.lock().unwrap().clear();
    }

    pub fn cached_raw_count(&self) -> usize {
        self.raw_cache.lock().unwrap().len()
    }

    pub fn cached_igb_count(&self) -> usize {
        self.igb_cache.lock().unwrap().len()
    }
}

impl Default for AssetLoader {
    fn default() -> Self {
        Self::new(ArchiveMount::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn create_test_archive_with_files(
        path: &Path,
        files: &[(&str, &[u8])],
    ) -> Result<(), std::io::Error> {
        use crate::archive::hash_path;
        let mut buf = Vec::new();
        let num_files = files.len() as u32;

        let mut hashed_files: Vec<(u32, &str, &[u8])> = files
            .iter()
            .map(|(name, data)| (hash_path(name), *name, *data))
            .collect();
        hashed_files.sort_by_key(|(h, _, _)| *h);

        buf.extend_from_slice(b"IGA\x1a");
        buf.extend_from_slice(&11u32.to_le_bytes()); // version
        buf.extend_from_slice(&100u32.to_le_bytes()); // toc_size
        buf.extend_from_slice(&num_files.to_le_bytes()); // num_files
        buf.extend_from_slice(&2048u32.to_le_bytes()); // sector_size
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // small blocks

        let mut nt_payload = Vec::new();
        let mut nt_offsets = Vec::new();
        let mut cur_offset = num_files * 4;

        for (_, name, _) in &hashed_files {
            nt_offsets.push(cur_offset);
            let name_bytes = name.as_bytes();
            cur_offset += name_bytes.len() as u32 + 1;
        }

        for off in &nt_offsets {
            nt_payload.extend_from_slice(&off.to_le_bytes());
        }
        for (_, name, _) in &hashed_files {
            nt_payload.extend_from_slice(name.as_bytes());
            nt_payload.push(0);
        }

        let nt_offset = 2048u64;
        let nt_size = nt_payload.len() as u64;
        buf.extend_from_slice(&nt_offset.to_le_bytes());
        buf.extend_from_slice(&nt_size.to_le_bytes());

        for (h, _, _) in &hashed_files {
            buf.extend_from_slice(&h.to_le_bytes());
        }

        // File headers start at 4096
        let file_start_offset = 4096u32;
        let mut cur_file_offset = file_start_offset;
        for (_, _, data) in &hashed_files {
            buf.extend_from_slice(&cur_file_offset.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
            cur_file_offset += data.len() as u32;
        }

        // Name table at 2048
        buf.resize(2048, 0);
        buf.extend_from_slice(&nt_payload);

        // Files at 4096
        buf.resize(4096, 0);
        for (_, _, data) in &hashed_files {
            buf.extend_from_slice(data);
        }

        let mut file = File::create(path)?;
        file.write_all(&buf)?;
        Ok(())
    }

    #[test]
    fn test_asset_loader_cache_and_resolution() {
        let tmp_dir = std::env::temp_dir().join("skylands_loader_test");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let arc_path = tmp_dir.join("test_assets.arc");

        // Construct a synthetic IGB payload
        let mut igb_bytes = Vec::new();
        igb_bytes.extend_from_slice(b"IGBF");
        igb_bytes.extend_from_slice(&8u32.to_le_bytes()); // version
        igb_bytes.extend_from_slice(&1u32.to_le_bytes()); // object_count
        igb_bytes.extend_from_slice(&0u32.to_le_bytes()); // string_table_offset

        // Geometry: 1 vertex, 0 indices
        igb_bytes.extend_from_slice(&1u32.to_le_bytes()); // vert_count
        igb_bytes.extend_from_slice(&0u32.to_le_bytes()); // index_count
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // pos.x
        igb_bytes.extend_from_slice(&2.0f32.to_le_bytes()); // pos.y
        igb_bytes.extend_from_slice(&3.0f32.to_le_bytes()); // pos.z
        igb_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // norm.x
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // norm.y
        igb_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // norm.z
        igb_bytes.extend_from_slice(&0.5f32.to_le_bytes()); // uv.u
        igb_bytes.extend_from_slice(&0.5f32.to_le_bytes()); // uv.v
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // col.r
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // col.g
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // col.b
        igb_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // col.a

        let dummy_text = b"Skylands Untrapped VFS Test Payload";

        create_test_archive_with_files(
            &arc_path,
            &[
                ("models/hero.igb", &igb_bytes),
                ("data/config.txt", dummy_text),
            ],
        )
        .expect("create test archive");

        let mut loader = AssetLoader::default();
        let mounted = loader.mount_dir(&tmp_dir).expect("mount dir");
        assert!(mounted >= 1);

        assert!(loader.has_file("models/hero.igb"));
        assert!(loader.has_file("data/config.txt"));
        assert!(!loader.has_file("missing/file.dat"));

        // 1. Raw cache test
        assert_eq!(loader.cached_raw_count(), 0);
        let raw1 = loader.load_raw("data/config.txt").expect("load raw 1");
        assert_eq!(raw1.as_slice(), dummy_text);
        assert_eq!(loader.cached_raw_count(), 1);

        // Subsequent call hits cache
        let raw2 = loader.load_raw("data/config.txt").expect("load raw 2");
        assert_eq!(raw2.as_slice(), dummy_text);
        assert_eq!(loader.cached_raw_count(), 1);

        // 2. IGB model cache test
        assert_eq!(loader.cached_igb_count(), 0);
        let model1 = loader.load_igb("models/hero.igb").expect("load igb 1");
        assert_eq!(model1.header.version, 8);
        assert_eq!(model1.meshes.len(), 1);
        assert_eq!(model1.meshes[0].vertices.len(), 1);
        assert_eq!(
            model1.meshes[0].vertices[0].position,
            [1.0f32, 2.0f32, 3.0f32]
        );
        assert_eq!(loader.cached_igb_count(), 1);

        let model2 = loader.load_igb("models/hero.igb").expect("load igb 2");
        assert_eq!(model2.meshes.len(), 1);
        assert_eq!(loader.cached_igb_count(), 1);

        // 3. Cache clearing
        loader.clear_cache();
        assert_eq!(loader.cached_raw_count(), 0);
        assert_eq!(loader.cached_igb_count(), 0);

        // 4. Embedded IGB chunk extraction
        let mut level_bld_bytes = Vec::new();
        level_bld_bytes.extend_from_slice(b"IGZ\x01\0\0\0\0dummy header padding");
        level_bld_bytes.resize(64, 0);
        level_bld_bytes.extend_from_slice(&igb_bytes);

        let scanned = ArchiveMount::scan_embedded_igb(&level_bld_bytes);
        assert_eq!(scanned.len(), 1);
        let models = loader
            .load_level_models("models/hero.igb")
            .expect("load level models");
        assert_eq!(models.len(), 1);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_load_real_level_bld_archive() {
        let p1 = Path::new("assets/files/level/Arena_Mesh_300.bld");
        let p2 = Path::new("../../assets/files/level/Arena_Mesh_300.bld");
        let path = if p1.exists() { p1 } else { p2 };
        if path.exists() {
            let mut loader = AssetLoader::default();
            loader.mount(path).expect("mount Arena_Mesh_300");
            assert!(loader.has_file("Arena_Mesh_300.bld"));
            let models = loader
                .load_level_models("Arena_Mesh_300.bld")
                .expect("load level models");
            assert!(!models.is_empty());
            assert!(!models[0].meshes.is_empty());
        }
    }

    #[test]
    fn test_mount_dir_and_resolve_targets() {
        let p1 = Path::new("assets/files");
        let p2 = Path::new("../../assets/files");
        let root = if p1.exists() { p1 } else { p2 };
        if root.exists() {
            let mut loader = AssetLoader::default();
            let mounted = loader.mount_dir(root).expect("mount_dir assets/files");
            assert!(mounted > 0);

            assert!(loader.has_file("462_Water_Crocky.bld"));
            let crocky = loader
                .load_raw("462_Water_Crocky.bld")
                .expect("load crocky");
            assert_eq!(&crocky[0..4], b"IGZ\x01");

            assert!(loader.has_file("1010_Kaos.bld"));
            let kaos = loader.load_raw("1010_Kaos.bld").expect("load kaos");
            assert_eq!(&kaos[0..4], b"IGZ\x01");

            assert!(loader.has_file("Level_Hub_2014_01.bld"));
            let hub = loader.load_raw("Level_Hub_2014_01.bld").expect("load hub");
            assert_eq!(&hub[0..4], b"IGZ\x01");

            // Test automated companion texture pairing on real assets
            if loader.has_file("StoreItem_Hat_009.bld") {
                let paired = loader.load_companion_texture_rgba("StoreItem_Hat_009.bld", None);
                assert!(paired.is_ok(), "StoreItem_Hat_009 companion texture pairs");
                let (w, h, rgba) = paired.unwrap();
                assert!(w > 0 && h > 0);
                assert_eq!(rgba.len(), (w * h * 4) as usize);
            }
        }
    }

    #[test]
    fn test_synthetic_companion_texture_pairing() {
        let tmp_dir = std::env::temp_dir().join("skylands_companion_pairing_test");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let bld_path = tmp_dir.join("companion_hero.bld");
        let arc_path = tmp_dir.join("companion_hero.arc");

        let mut igb_bytes = Vec::new();
        igb_bytes.extend_from_slice(b"IGBF");
        igb_bytes.extend_from_slice(&8u32.to_le_bytes());
        igb_bytes.extend_from_slice(&1u32.to_le_bytes());
        igb_bytes.extend_from_slice(&0u32.to_le_bytes());
        igb_bytes.extend_from_slice(&1u32.to_le_bytes());
        igb_bytes.extend_from_slice(&0u32.to_le_bytes());
        for _ in 0..12 {
            igb_bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }

        create_test_archive_with_files(&bld_path, &[("level.bld", &igb_bytes)])
            .expect("create test bld");

        let mut cmpr_data = vec![0u8; 32 * 16 * 16]; // 128x128 CMPR
        for chunk in cmpr_data.as_chunks_mut::<8>().0 {
            chunk[0..2].copy_from_slice(&0x07E0u16.to_be_bytes()); // green
            chunk[2..4].copy_from_slice(&0x0000u16.to_be_bytes());
        }

        create_test_archive_with_files(&arc_path, &[("textures/hero.png.igb.tex.igz", &cmpr_data)])
            .expect("create test arc");

        let mut loader = AssetLoader::default();
        let mounted = loader.mount_dir(&tmp_dir).expect("mount dir");
        assert!(mounted >= 2);

        let paired = loader
            .load_companion_texture_rgba("companion_hero.bld", Some("hero.png"))
            .expect("load companion texture");
        assert_eq!(paired.0, 128);
        assert_eq!(paired.1, 128);
        assert_eq!(paired.2.len(), 128 * 128 * 4);

        let auto = loader
            .load_first_image_rgba("companion_hero.bld")
            .expect("auto paired texture via load_first_image_rgba");
        assert_eq!(auto.0, 128);
        assert_eq!(auto.1, 128);
        assert_eq!(auto.2.len(), 128 * 128 * 4);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
