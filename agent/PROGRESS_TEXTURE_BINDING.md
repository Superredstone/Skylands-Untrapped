# Progress Update: Texture Binding (Phase 1, Point 1)

## Summary
- Implemented Phase 1 Point 1: Connected decoded CMPR/DXT1 RGBA textures to `StandardMaterial.base_color_texture` in `crates/skylands-render/src/visual_sync.rs`.
- Injected `Assets<Image>` into `actor_visual_sync_system`, `CharacterVisualRegistry::get_or_load`, and `ModelVisualRegistry::get_or_load_model`.
- When texture is resolved from embedded `igImage2` or accompanying `.arc` archive, creates Bevy `Image` via `rgba8_to_bevy_image`, registers into `Assets<Image>`, binds handle to `StandardMaterial.base_color_texture`, and switches `base_color` to `Color::WHITE` to preserve natural diffuse shading.
- Fixed archive resolver in `crates/skylands-formats/src/archive.rs` (`ArchiveMount::read_file`) to prioritize exact archive name matching over prefix stem matching.
- Exported `hash_path` in `crates/skylands-formats/src/lib.rs`.
- Added unit test `test_model_visual_registry_binds_decoded_texture` asserting texture extraction, image asset insertion, and material binding.
- Verified all 71 workspace tests pass (`cargo test`), headless simulation passes (`cargo run -p skylands-app -- --headless --ticks 120`), and `cargo fmt` + `cargo clippy` pass with zero warnings.
