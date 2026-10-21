# Progress: Fix Exploding Vertices and Texture Binding in Level Geometry

## Root Cause
1. **Triangle Strip Bridge Degenerates & Coordinate Typos**:
   - `igWiiGeometry` generates alternating triangle strip indices `(t, t+1, t+2)` / `(t+1, t, t+2)`.
   - Disconnected submesh geometry or strip restart boundaries produced long bridge triangles (> 15m up to 140m) connecting distant vertices across the map, causing 154,000+ geometry spikes.
   - Coordinate indexing bugs in distance calculations (`(p1[2] - p2[1])` and `(p2[2] - p0[1])`).
   - Fallback `if indices.is_empty()` blindly dumped unfiltered triangles on degenerate strips.
   - `aabb_18` vs `aabb_24` fallback did not validate `aabb_24`, collapsing positions when `aabb_18` had NaN.
2. **2D UI Portrait Texture Stretching Across Level**:
   - `Level_Hub_2014_01.arc` only contains 2D UI portraits and card game textures (`hud/portraits/`, `screens/cardgame/`).
   - `load_companion_texture_rgba` picked the first texture in `.arc` (a 256x128 NPC portrait) and stretched it over all 673 level submeshes.
   - Level was merged into a single mesh with a single material.

## Fix
1. **`crates/skylands-formats/src/igb.rs`**:
   - Fixed Z-coordinate subtraction typos in distance metrics.
   - Dynamic edge-length clamp `max_edge = (diag * 0.45).clamp(3.0, 15.0)`. Discards all strip bridge triangles with edge length exceeding local bounds or extreme aspect ratio / zero area.
   - Safe validation fallback for both `aabb_18` and `aabb_24` (`[-1.0, 1.0]` fallback if neither is valid).
   - Strip fallback restricted to small local meshes (`diag <= 5.0`).
2. **`crates/skylands-formats/src/loader.rs`**:
   - Added `is_ui_texture_path` rejecting `hud/`, `portraits/`, `cardgame/`, `wantedposters/` from being paired as companion textures for 3D level geometry.
3. **`crates/skylands-render/src/visual_sync.rs`**:
   - Multi-submesh support: instances are grouped by submesh/material group.
   - Each submesh group receives its own `BevyMesh` transformed by node `world_transform`, and its own `StandardMaterial`.
   - When no dedicated environment texture is available, applies neutral stone/earth PBR material (`Color::srgb(0.72, 0.70, 0.65)`, roughness 0.85, reflectance 0.20, double sided, cull mode None) without stretching 2D UI textures.

## Verification
- `cargo test`: all 80 workspace tests pass clean.
- `cargo run -p skylands-app -- --headless --ticks 120`: 120 ticks run successfully.
- `cargo fmt --all && cargo clippy --all-targets --all-features`: 0 errors, 0 warnings.
