# Progress Update: Model Extraction & Texture Binding

## Summary
- Implemented Task 4: Extraction of `igWiiGeometry`, `igVertexBuffer`, `igIndexBuffer`, `igVertexFormat` from the IGZ object registry into structured models.
- Implemented Task 5: Extraction of `igImage2` texture structures, `EXID` pixel format hashes (`dxt1_tile_big_wii`), and `TMHN` thumbnail offsets and dimensions. Added RGBA extraction pipeline in `AssetLoader` and integrated texture loading in `visual_sync`.
- Implemented Task 6: Verified model extraction and headless execution simulation (`cargo run -p skylands-app -- --headless --ticks 120`). All 38 formats tests, 17 render tests, and core tests pass.
