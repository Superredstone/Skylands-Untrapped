# Progress Update: Automated Texture Pairing (Phase 1, Point 3)

## Summary
- Implemented Phase 1 Point 3: Automated `.arc` texture bundle resolution alongside `.bld` mesh bundle in `skylands-formats` and `skylands-render`.
- Added archive texture query helpers to `ArchiveMount` (`find_matching_archive`, `list_archive_files`, `find_archive_texture_files`). Handles exact name matching and extension-aware fallback (`.arc` vs `.bld` stems).
- Added `AssetLoader::load_companion_texture_rgba`: locates companion `<name>.arc` archive from `<name>.bld`, resolves matching texture file by preferred material name or primary texture IGZ, parses `igImage2` header (`EXID`, `TMHN`), and decodes CMPR/DXT1 payload to RGBA8.
- Updated `AssetLoader::load_first_image_rgba` to transparently fall back to companion `.arc` texture extraction when `.bld` geometry lacks embedded `igImage2` textures.
- Updated `ModelVisualRegistry` and `CharacterVisualRegistry` in `crates/skylands-render/src/visual_sync.rs` to automatically pair companion `.arc` textures with natural dimensions `(w, h)` and bind them to `StandardMaterial.base_color_texture`.
- Added unit tests `test_synthetic_companion_texture_pairing` and tested real asset pairing (`StoreItem_Hat_009.bld` + `StoreItem_Hat_009.arc`).
- Verified all 74 workspace unit tests pass (`cargo test`), headless simulation passes (`cargo run -p skylands-app -- --headless --ticks 120`), and `cargo fmt` + `cargo clippy` pass with zero warnings.
