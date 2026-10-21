# Agent Roadmap & Milestone Tracking

Reference: Full implementation plan written to `PROGRESS.md`.

## Active Priority: Phase 1 & Phase 2
1. Texture binding: connect decoded CMPR/DXT1 RGBA textures to `StandardMaterial.base_color_texture` in `visual_sync.rs`.
2. Real level loading: replace procedural courtyard in `setup_default_scene` with parsed `Level_Hub_2014_01.bld` / `Level_313_Tutorial.bld`.
3. Level collision: generate `EnvironmentCollision` mesh from level IGB `col_*` geometry nodes.
4. Level markers: spawn player, pickups, and zone triggers from parsed level scene graph markers.
