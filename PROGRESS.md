# Model Extraction & Rendering Progress

## Status
- [x] Task 1: Fix IGZ Chunk Descriptor & Header Parsing
- [x] Task 2: Implement RVTB Packed Integer Fixup Unpacker
- [x] Task 3: Implement IGZ Object Registry & Type Resolution (`TEMT` / `RVTB`)
- [x] Task 4: Extract `igWiiGeometry`, `igVertexBuffer`, `igIndexBuffer`, `igVertexFormat`
- [x] Task 5: Bind Character Textures (`igImage2` / `TMHN` -> Bevy Material)
- [x] Task 6: Verify Model Rendering in Viewer & Headless Mode

---

## Technical Details

### Root Cause Analysis
1. **IGZ Header Descriptors Offset (`crates/skylands-formats/src/igb.rs:2052-2077`)**:
   - IGZ v8 header starts chunk descriptors at `0x18`, not `0x14`.
   - Termination sentinel is `offset == 0`, not fake section count at `0x10`.
   - 4-byte misalignment invalidated all section offsets.

2. **RVTB / BTVR Fixup Table Not Unpacked**:
   - Alchemy v8.1.0 stores all 7,145 objects behind `BTVR` (RVTB) fixup table.
   - Offsets encoded with variable-length 3-bit / 4-bit nibbles (`IgzOffsetMapFixup`).
   - Objects must be resolved against `TEMT` type table.

3. **Fallback to Random Float Scanning**:
   - Misaligned section reading triggered heuristic float scanner (`igb.rs:2244-2365`).
   - Heuristic converted random binary data into sequential triangle noise.

4. **Missing Texture Binding**:
   - `visual_sync.rs` applied flat single-color materials instead of extracting `igImage2` textures.

---

## Execution Steps

1. **Fix IGZ Descriptors Parsing**:
   - Replace bogus `num_sections` loop with `(offset, size, unk1, unk2)` descriptor reader starting at `0x18` until `offset == 0`.
   - Store real chunk list with exact byte slices.

2. **Port `IgzOffsetMapFixup` to Rust**:
   - Implement bitstream reader for 3-bit packed integers with continuation bit.
   - Compute absolute offsets across chunk descriptors (`(prev >> 0x1B) + 1` descriptor index).
   - Unpack `BTVR` (RVTB) and `TEMT` (types).

3. **Extract Real Mesh Data**:
   - Locate `igWiiGeometry` and associated `igVertexBuffer` / `igIndexBuffer`.
   - Parse vertex format attributes (position, normal, UV).
   - Convert Wii GX / fixed / half-float vertex data and triangle indices into `IgbMesh`.

4. **Connect Textures**:
   - Extract `igImage2` images via `TMHN` thumbnails and `EXID` formats.
   - Feed CMPR / DXT1 texture buffers to Bevy `Image` / `StandardMaterial`.

5. **Validation**:
   - `cargo test -p skylands-formats`
   - `cargo test -p skylands-render`
   - Run headless test tick simulation and inspect vertex bounds.
