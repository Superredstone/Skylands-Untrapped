# Skylands Untrapped - Progress & Roadmap

Single source of truth for progress. Update this file; do not add per-task fragments.

## Status
- Target: EU (en) *Skylanders: Trap Team* (`rom.eu.wbfs`)
- Platform: Linux (`udev`/`hidapi-rs`), Windows (`hidapi-winapi`)
- Runtime: Bevy 0.15, fixed 60Hz tick, decoupled render

## Done

### Formats (`crates/skylands-formats`)
- [x] Alchemy v8.1.0 archives (`.arc` / `.bld`), BE/LE, Deflate/LZMA.
- [x] VFS (`ArchiveMount`, `AssetLoader`), path normalization, exact-name archive match before stem match.
- [x] IGZ object registry, `RVTB` fixups, `igVertexFormat` stride derivation.
- [x] `igWiiGeometry` decode via GX display list (`IgbModel::decode_wii_display_list`):
  - `w[2]/w[3]` = display list (buffer A); `w[4]/w[5]` = attribute arrays (buffer B), CP `0xA0+n` bases are offsets into B.
  - Pointer section from high bits (`>>27`).
  - VCD + VAT A/B driven: matrix-index bytes, POS, NRM, CLR0, TEX0 (direct / idx8 / idx16, fixed-point shift).
  - Quads, tris, strips, fans; degenerate strip joints dropped.
  - CLR0 decoded for all GX formats (RGB565, RGB8, RGBX8, RGBA4, RGBA6, RGBA8) into `IgbVertex.color`.
  - Face normals only when NRM absent.
- [x] `igGeometryAttr` decode (`IgbModel::decode_geometry_attr`): skinned character bodies (e.g. `FoodFight_Body`, 1612 verts) live here, not in `igWiiGeometry` (accessories only).
  - Attr `+0x10` VB, `+0x14` IB. VB/IB data via TMHN handle at `+0x14`. VF elements (12 B, `0x2c` end): POS s16x3, NRM s8x3, UV s16x2; shift from VB `+0x24` table. Streams 32-byte aligned.
  - IB = GX `0x98` strips, k repeated u8/u16 indices per vertex; width/k probed (verified on all 1622 attrs in `files/character`).
  - Blend weights (usage 6, u8x4 unorm, sum `0xff`, normalized) and indices (usage 8, u8x4) stored in `bone_weights`/`bone_indices`. Indices are bone-palette slots, not skeleton bones (eyes: slot `0x19`, weight in lane 3).
- [x] GX PNMTXIDX (VCD bit 0) stored as `bone_indices[0] = byte / 3` (palette slot); TEX0-7MTXIDX still skipped.
- [x] `igSkeleton2` decode (`IgbModel::decode_skeleton2`), verified on all 714 character files (671 skeletons, 15576 bones):
  - Skeleton `+0x08` name, `+0x0c` -> `igSkeletonBoneList` (`+0x08` count, `+0x14` -> ptr array), `+0x14` -> inverse bind matrices (column-major 4x4, indexed by blend index; count == skinned bones).
  - `igSkeletonBone` (0x20): `+0x08` name, `+0x0c` parent i32 (always < self), `+0x10` blend index i32 (-1 = unskinned, bone 0 `Unnamed bone`), `+0x14` translation f32x3 (parent-local; scaled rigs differ, so bind pose = inverse(IBP)).
  - Mesh link (`skinned_attr_links`): `tfbPhysicsBody` `+0x28` -> `tfbPhysicsCombinerLink` `+0x0c` -> skeleton; `+0x30` -> `tfbBodyEntityInfo` `+0x14` -> `DrawableList` -> `Drawable` `+0x0c` -> `igGeometryAttr` (1630/1630 linked). `IgbMesh.skeleton` holds the index.
  - Palette `tfbBodyEntityInfo` `+0x24` -> `igIntListList` is identity in every file, so palette slot == blend index; remapped to bone index at decode.
- [x] Synthetic rig and nearest-bone weighting removed; no `igSkeleton2` = empty `skeletons`.
- [x] Textures: CMPR (`dxt1_tile_big_wii`) and DXT1, `igImage2` `EXID`/`TMHN`.
- [x] Companion texture pairing: `<name>.bld` resolves `<name>.arc` (`AssetLoader::load_companion_texture_rgba`).
- [x] Audio: DSP ADPCM, FSB5.
- [x] Skeleton/animation clip parsing; level markers (`LevelMarker`, `MarkerKind`).
- [x] Level AABB: `w[18..23]` valid for levels, `w[24..29]` for characters (bounds only, not used for decode).

### Portal (`crates/skylands-portal`)
- [x] USB HID Traptanium (VID `0x1430`, PID `0x0150`), `MockPortal` with virtual tags.
- [x] LED ring / trap light / antenna, tag arrive/depart, 8kHz mono speaker streaming.

### Core (`crates/skylands-core`)
- [x] Deterministic 60Hz fixed tick. States: `Hub`, `VillainCapture`, `ZoneTransition`.
- [x] Character archive name resolution (`resolve_character_model_archive`): canonical mappings for standard Skylander IDs (0..=32, 400..=407, 1001..=1010) and filtering out minigame/auxiliary archives (`musicmini`, `outro`, `stream`, `card`, `ally_icon`, `_alt`, `legendary`).
- [x] Hero spawn on ring tag, villain on trap tag. gilrs + keyboard input.
- [x] Movement, gravity, jump, ground raycast; combo/sweep/projectile combat; coins/gems.
- [x] `ActiveLevel` from VFS, `--level <NAME>`; `CollisionMesh::from_igb` with 2D XZ spatial grid.
- [x] Tick interpolation (`interpolation.rs`): `TickTransform` restored in `FixedFirst`, recorded in `FixedPostUpdate`, blended by `overstep_fraction()` after the fixed loop. Auto-added to `Velocity` entities; snap above `SNAP_DISTANCE` (5 units).
- [x] Marker spawning (spawn, doorways, triggers, props, collectibles); ground raycast fallback from `(0,50,0)`; kill plane `-350`.

### Render (`crates/skylands-render`)
- [x] PBR pipeline, textures bound to `StandardMaterial.base_color_texture`.
- [x] Double-sided rendering (`cull_mode: None`, `double_sided: true`) on character and model materials to prevent missing faces from Wii winding.
- [x] Character submesh scene graph transformation (`igb_model_to_bevy_mesh`): applies node hierarchy transforms to all submeshes before normalization.
- [x] Submesh outlier & auxiliary geometry filtering (`is_auxiliary_character_submesh`): strips oversized outlier meshes (>15.0 units, e.g. background domes/VFX), 2D billboard quads/cards (thickness < 0.05 with span > 2.0 or trivial vert count <= 4), and horizontal ground decal rings (`y_span < 0.15` and `x_span > 3.0`) to avoid bounding-box collapse during normalization.
- [x] Level instances merged into one mesh/material; positions and normals (`transform_dir`) in world space.
- [x] Cameras: follow, free-fly, showcase orbit. Viewer `--viewer`. HUD.
- [x] Viewer mode isolates hero showcase without triggering Kaos trap capture state.
- [x] Dynamic HUD stats in viewer mode (vertex & triangle count from active character mesh).

### Extractor (`crates/skylands-extractor`)
- [x] First-run extraction via `wit` (nix-shell fallback), validation of `sys/main.dol`, `global.arc`, `global.bld`.

### Performance
- Dev profile `opt-level = 1`, deps `opt-level = 3`.
- Synthetic bone weighting skipped for levels (`LEVEL_GEOMETRY_THRESHOLD` = 64 geometries).
- Hub headless load: 91 s to 15 s (debug). Remaining: LZMA + GX decode.

## Known gaps
- CLR1, NBT3 binormal/tangent, TEX matrix-index bytes: parsed past, not stored.
- `igWiiGeometry` PNMTXIDX slots (`bone_indices[0]`) not yet linked to a skeleton (`IgbMesh.skeleton = None`).
- `transform_dir` ignores non-uniform scale (no inverse-transpose).
- Interpolation not visually verified at 144Hz in GUI. `Update` writes to interpolated entities are dropped at the next tick (only viewer turntable does this; harmless).
- Normal/specular maps not loaded.
- CLR0 vertex colors not yet visually verified in GUI (Bevy multiplies `ATTRIBUTE_COLOR` into base color).

## Reverse engineering roadmap (ordered)

Current gaps: `main.dol` is converted to ELF but not analyzed, gameplay constants are guessed.

### Phase 1 (priority) - Skinning & animation
- [x] Store GX matrix-index bytes per vertex + `igGeometryAttr` blend weights/indices.
- [x] Decode real `igSkeleton2`: bone names, parents, inverse bind poses. Drop synthetic rig.
- [x] Decode real character animation tracks: character files use Havok 2013 (`hk_2013.1.0-r1`) spline compressed animations inside `tfbPhysicsAnimation`. Implemented B-spline NURBS curve & quaternion evaluation (POLAR32, THREECOMP40, THREECOMP48).
- [x] Wire Bevy `SkinnedMesh`, remove hardcoded `bone_count = 32` in `viewer.rs`. Spawn joint hierarchy, bind `SkinnedMeshInverseBindposes`, add `CharacterAnimator` updating GPU skinning transforms.
- [x] Enable skinning and animation for villains (`Villain`) via `CharacterVisualRegistry` and sanitize unskinned meshes (`ensure_unskinned_mesh`) to avoid WGPU bind group layout mismatch crashes in Bevy PBR prepass.
- Done: Spyro idle animates in `--viewer`.

### Phase 2 - DOL tooling & Alchemy metadata
- [x] `tools/rea-bridge/rea_analyze.sh`: Ghidra headless import (`PowerPC:BE:32:Gekko_Broadway` / `PowerPC:BE:32:default`) + auto-analysis.
- [x] Locate `igMetaObject` name strings / registry; map class name to vtable/ctor (1,111 classes mapped via `igMetaObject::Register` at `0x801062e0`).
- [x] Dump `igMetaField` tables (name, type, offset, size) for `igWiiGeometry`, `igImage2`, `igEntity`, `igComponent`. Replace guessed `w[n]` indices (2,884 fields across 655 classes in `tools/rea-bridge/meta_fields.tsv`; structured `meta_fields` constants in `crates/skylands-formats/src/igb.rs`).
- [x] Commit address/name map only (`tools/rea-bridge/symbols.txt`, 3,336 symbols).

### Phase 3 - Entities & levels
- [x] Decode `igEntity`/component layout in level `.bld`; replace synthetic spiral positioning with real `Entity` transforms (`IgbModel::decode_entity_geometry_links` links `tfbWorldEntityInfo` / `tfbEntityInfo` drawables to `Entity._transform` matrices, positioning geometries and markers in world coordinates).
- [ ] Doorway/zone link data; swap level archives on doorway; campaign progress.
- [ ] Find dedicated collision data (vs render mesh).

### Phase 4 - Gameplay logic from DOL
- [ ] Character stat tables (HP, speed, damage by element/level).
- [ ] Villain timer/energy, trap return rules, trapping sequence.
- [ ] Enemy AI state machines (melee, ranged), encounter arenas.
- [ ] Save profile layout.
- Constants go into Rust with a `// DOL 0x80xxxxxx` source note; no pasted decompiler output.

### Phase 5 - Scripts, text, audio, misc
- [ ] `ScriptSet` / script VM (see `tools/igArchiveExtractor/Types/ScriptSet.cs`).
- [ ] Localization strings (EU en).
- [ ] Sound bank to event mapping.
- [ ] Mock tag persistence (`.tag`). Real tag write-back + hotplug need human review (portal protocol).
- [ ] Load perf: cache decoded level geometry (only if load time hurts).

### RE rules
- Notes and addresses only in git; no binaries, decompiled dumps, or extracted assets.
- Portal protocol findings need human review before merge.
- Every decoded format ships a test that skips when ROM is absent.

## Verification
- `nix-shell --run "cargo fmt --all -- --check"`
- `nix-shell --run "cargo clippy --workspace --all-targets -- -D warnings"`
- `nix-shell --run "cargo test --workspace"`
- `nix-shell --run "cargo run -p skylands-app -- --headless --ticks 120"`
- Viewer: `nix-shell --run "cargo run -p skylands-app -- --viewer --character spyro"`
