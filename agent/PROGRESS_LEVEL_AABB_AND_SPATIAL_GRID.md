# Progress: Level Geometry AABB Decoding & Collision Spatial Broadphase

## Changes

1. **`igWiiGeometry` AABB Decoding (`crates/skylands-formats/src/igb.rs`)**:
   - Check `w[18..23]` for valid AABB bounds (`min < max` and finite).
   - Fall back to `w[24..29]` for character models.
   - Restores coordinates for submeshes in level archives like `Level_Hub_2014_01.bld`.

2. **Collision Spatial Grid Broadphase (`crates/skylands-core/src/collision.rs`)**:
   - Added uniform 2D XZ spatial hash grid to `CollisionMesh`.
   - Accelerated `raycast_down` and `resolve_sphere` to query local cells rather than full triangle scan.

3. **Level Ground Spawn & Kill Plane Resolution (`crates/skylands-core/src/systems.rs`)**:
   - Updated `setup_default_scene` to raycast down from `(0, 50, 0)` onto level collision mesh if no spawn marker present.
   - Updated `env.kill_plane_y` to `-350.0` to accommodate level depths.
   - Updated `portal_gameplay_system` to spawn hero and villain relative to `env.spawn_point`.

## Verification
- `cargo test`: all tests passing (60 unit tests across formats, core, portal, render, extractor, app).
- `cargo run -p skylands-app -- --headless --ticks 120`: 120 ticks run successfully.
- `cargo fmt --all && cargo clippy --all-targets --all-features`: clean.
