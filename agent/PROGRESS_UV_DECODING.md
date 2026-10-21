# Progress Update: UV Coordinate Decoding (Phase 1, Point 2)

## Summary
- Implemented Phase 1 Point 2: Correct UV coordinate decoding for character and environment submeshes in `crates/skylands-formats/src/igb.rs`.
- Fixed signed fixed-point decoding in `igWiiGeometry` Buffer B: replaced `u16` unsigned reads with `i16` signed reads divided by `1024.0`, preventing negative wrap underflow (which mapped `-0.01` to `~64.0`).
- Fixed GX display list UV decoding (`decode_gx_display_list`): replaced unsigned `u16` with signed `i16` for Direct, Index16, and Index8 coordinate modes.
- Added dynamic `igVertexFormat` attribute list derivation from object payload and vertex stride (16, 20, 24, 32 bytes) with half-float and float UV descriptors.
- Added bounds checks in `parse_attr_list` (`count == 0 || count > 32 || count * 16 + 8 > data.len()`) to prevent uncontrolled allocations on corrupt descriptors.
- Sanitized decoded coordinates against NaN and infinity.
- Added unit tests `test_uv_decoding_formats` and `test_igz_vertex_format_stride_derivation`.
- Verified all 73 workspace tests pass (`cargo test`), headless simulation passes (`cargo run -p skylands-app -- --headless --ticks 120`), and `cargo fmt` + `cargo clippy` pass with zero warnings.
