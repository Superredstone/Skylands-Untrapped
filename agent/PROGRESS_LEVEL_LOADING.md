# Progress Update: Real Level Loading (Phase 2, Points 4, 5 & 6)

## Summary
- Implemented Phase 2 Points 4, 5, and 6:
  - Added `ActiveLevel` and `GameAssetServer` resources to `skylands-core`.
  - Updated `setup_default_scene` in `skylands-core` to load `ActiveLevel` from VFS:
    - Generates `EnvironmentCollision` mesh from level IGB (`CollisionMesh::from_igb`).
    - Spawns player spawn point from `MarkerKind::Spawn`.
    - Spawns doorways and trigger zones from `MarkerKind::Doorway` and `MarkerKind::TriggerZone`.
    - Spawns props and collectibles from `MarkerKind::Prop` and `MarkerKind::Collectible`.
    - Gracefully falls back to procedural courtyard entities when running without game assets.
  - Updated `setup_environment_and_assets` in `skylands-render`:
    - Automatically loads level mesh bundle (`.bld`) and pairs companion texture bundle (`.arc`).
    - Combines level geometry into Bevy `Mesh3d` entities with textured materials.
    - Preserves showcase turntable support in `--viewer` mode.
  - Added CLI flag `--level <NAME>` in `skylands-app` to configure active level.
  - Added unit tests for `ActiveLevel` defaults, fallback scene spawning, and level visual asset loading.
  - All workspace unit tests pass, headless simulation passes (`120 ticks`), `cargo fmt` and `cargo clippy` pass with zero warnings.
