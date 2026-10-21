# Agent Roadmap & Milestone Tracking

Reference: Full implementation plan written to `PROGRESS.md`.

## Active Priority: Phase 1 & Phase 2
1. Texture binding: [x] connect decoded CMPR/DXT1 RGBA textures to `StandardMaterial.base_color_texture` in `visual_sync.rs`.
2. UV decoding: [x] signed fixed-point i16, half-float, and vertex format stride derivation for character & environment meshes.
3. Automated texture pairing: [x] resolve `.arc` texture bundle alongside `.bld` mesh bundle.
4. Real level loading: [x] replace procedural courtyard in `setup_default_scene` with parsed `Level_Hub_2014_01.bld` / `Level_313_Tutorial.bld`.
5. Level collision: [x] generate `EnvironmentCollision` mesh from level IGB `col_*` geometry nodes.
6. Level markers: [x] spawn player, pickups, and zone triggers from parsed level scene graph markers.
