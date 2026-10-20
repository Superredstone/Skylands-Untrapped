# Root Cause Analysis: Spyro Model Triangle Noise & Geometry Fix

## Problem Description
When rendering characters such as Spyro (character ID 16) from disc archives (`assets/files/character/016_Spyro.bld`), random triangles explode across the screen instead of the actual 3D character mesh.

---

## Technical Root Cause Analysis

1. **False Display List Decoding on Section 3 (`crates/skylands-formats/src/igb.rs:2776-2806`)**:
   - `IgbModel::parse_igz` invoked `decode_gx_display_list` indiscriminately on Section 3 (`0xbf478`).
   - Section 3 in character `.bld` files contains streamed audio and particle data (`tfbSoundDataInfo` pointers at `0x23284` resolve to Section 3 offsets).
   - `decode_gx_display_list` scanned Section 3 for bytes matching GX primitive headers (`0x80`, `0x90`, `0x98`).
   - For indexed vertex attributes (`Index8`), the function used display list opcode offsets (`pos_off = cp_pos_base + pos_idx * stride`) into `sec3_data` itself instead of a valid vertex buffer.
   - Result: Display list opcodes (`08 77 5c f7...`) were read as signed 16-bit vertex coordinates (`sx, sy, sz`), producing a single corrupted mesh (`gx_mesh`) with 10,516 vertices spanning from -24.2 to +25.2.

2. **Unextracted `igWiiGeometry` Submeshes (`crates/skylands-formats/src/igb.rs:2704-2715`)**:
   - The IGZ object registry contains 17 real `igWiiGeometry` nodes representing the submeshes of Spyro (head, horns, wings, torso, tail, pedestal base, and shadow plane).
   - In `parse_igz`, these nodes were pushed into `model.geometries` with `mesh: None` and their underlying geometry buffers were ignored.
   - The actual vertex data for all 17 submeshes resides in Section 2 (`0x72098`):
     - `w[2] & 0x00FFFFFF` / `w[3] & 0x07FFFFFF`: Submesh Buffer A size and offset in Section 2.
     - `w[4] & 0x00FFFFFF` / `w[5] & 0x07FFFFFF`: Submesh Buffer B size and offset in Section 2.
     - `w[12]`: Submesh vertex count.
     - `w[24..29]`: Exact 3D local bounding box (`[min_x, min_y, min_z]` and `[max_x, max_y, max_z]`).
     - Vertices are stored as 16-bit signed quantized integers spanning the bounding box.

3. **Renderer Ingestion (`crates/skylands-render/src/visual_sync.rs`)**:
   - `visual_sync` took `model.meshes`, which contained only the corrupted 10,516-vertex `gx_mesh`.
   - `combine_or_normalize_model_meshes` scaled the chaotic coordinates into a 1.8-meter bounding box, resulting in a dense cloud of random triangles on screen.

---

## Action Plan

1. **Decode `igWiiGeometry` into `IgbMesh` (`crates/skylands-formats/src/igb.rs`)**:
   - Extract vertex positions from Section 2 for each `igWiiGeometry` using `w[12]` (vertex count).
   - Map 16-bit signed quantized integers (`sx, sy, sz`) into model-space floats using `w[24..29]` bounding boxes:
     `pos[c] = min_pt[c] + (raw[c] - min_raw[c]) / span_raw * (max_pt[c] - min_pt[c])`.
   - Extract UV coordinates and compute proper surface normals.
   - Generate valid triangle strip indices.
   - Attach decoded submeshes to `igWiiGeometry` nodes and push to `model.meshes`.

2. **Prevent False Display List Injection (`crates/skylands-formats/src/igb.rs`)**:
   - Remove unconditional execution of `decode_gx_display_list` on Section 3.
   - Only execute display list decoding when section contains valid, verified direct display list data.

3. **Verification**:
   - Run `nix-shell --run "cargo test"`.
   - Run `nix-shell --run "cargo run -p skylands-app -- --headless --ticks 120"`.
   - Ensure `cargo fmt` and `cargo clippy` pass cleanly.
