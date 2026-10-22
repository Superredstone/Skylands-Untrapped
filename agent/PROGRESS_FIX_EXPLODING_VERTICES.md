# Progress: Fix Exploding Vertices in Level Geometry

## Root Cause (actual)
`igWiiGeometry` buffers were misread in `crates/skylands-formats/src/igb.rs`:
- `w[2]/w[3]` (buffer A, size/ptr) is a **GX display list** (`08 77..` CP VAT loads,
  `08 A0/B0` array base/stride, `08 50/60` VCD, `0x98` strip primitives), not positions.
- `w[4]/w[5]` (buffer B) holds the indexed attribute arrays. CP `0xA0+n` bases are offsets into it.
- Old code read display-list bytes as i16 xyz, remapped them into the AABB from `w[18..23]`/`w[24..29]`
  (invalid on 922/3214 hub geometries), and stitched every vertex into one fake strip.
  The earlier edge-length clamp only hid some of the symptoms.

## Fix
- New `IgbModel::decode_wii_display_list(dl, vb)`: full GX vertex decode driven by VCD + VAT A/B
  (matrix-index bytes, pos/nrm/clr0/clr1/tex0-7; direct / idx8 / idx16; fixed-point shift).
  Per-primitive triangulation (quads, tris, strips, fans), degenerate strip joints dropped.
- Pointers resolved via section index in the pointer high bits (`>>27`), not hardcoded section 2.
- Removed AABB remap, edge clamp, strip fallback, and the Section 3 GX heuristic.
- Fixed geometry/mesh zip misalignment when a geometry yields no mesh.

## Verification
- Hub: 3214 geometries, ~1.18M verts, ~933k tris, median edge about 1 unit.
- Spyro: 17 submeshes, positions match the stored AABBs.
- Tests: `test_wii_display_list_indexed_strip` (synthetic), `test_hub_level_geometry_has_no_exploded_triangles` (real asset).

## Not done
Normals NBT3, vertex colors, and skinning matrix indices are parsed past but not decoded.

## Performance follow-up
- `Cargo.toml`: dev profile `opt-level = 1`, deps `opt-level = 3`.
- `parse_igz`: synthetic bone weighting skipped for levels (`LEVEL_GEOMETRY_THRESHOLD` = 64 geometries).
  It cost 337 bones x 1.18M verts plus a Vec sort per vertex. Characters now use a fixed top-4 array.
- `visual_sync`: level instances merged into one mesh/material (was 3214 meshes, materials, texture lookups).
- Hub headless load: 91 s -> 15 s (debug). Remaining time: LZMA decompression and GX decoding.
