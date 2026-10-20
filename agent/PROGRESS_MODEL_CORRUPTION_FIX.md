# Progress Update: Wii Geometry Extraction & Display List Corruption Fix

## Summary
- Implemented `agent/MODEL_CORRUPTION_ANALYSIS.md` in `crates/skylands-formats/src/igb.rs`.
- Extracted `igWiiGeometry` submeshes from Section 2 using vertex counts (`w[12]`), buffer offsets (`w[2..6]`), and local bounding boxes (`w[24..29]`), quantizing vertex coordinates, decoding UVs, calculating surface normals, and generating triangle strip indices.
- Prevented false GX display list injection from Section 3 audio/particle streams by restricting decoding to cases where submesh geometry is absent.
- Verified with unit tests (`cargo test`), headless simulation (`cargo run -p skylands-app -- --headless --ticks 120`), `cargo fmt`, and `cargo clippy`.
