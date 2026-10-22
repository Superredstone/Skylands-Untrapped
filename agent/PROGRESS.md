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
  - Blend weights/indices (usage 6/8) parsed past, not stored.
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
- CLR1, NBT3 binormal/tangent, matrix-index bytes: parsed past, not stored.
- `transform_dir` ignores non-uniform scale (no inverse-transpose).
- Skinning not wired in Bevy: viewer `bone_count` hardcoded to 32 (`viewer.rs`).
- Interpolation not visually verified at 144Hz in GUI. `Update` writes to interpolated entities are dropped at the next tick (only viewer turntable does this; harmless).
- Normal/specular maps not loaded.
- CLR0 vertex colors not yet visually verified in GUI (Bevy multiplies `ATTRIBUTE_COLOR` into base color).

## Reverse engineering roadmap (ordered)

Current gaps: skeleton is a synthetic 6-bone rig (`igb.rs` `rig_defs`), clips are synthetic idle/walk/attack, `main.dol` is converted to ELF but not analyzed, gameplay constants are guessed.

### Phase 1 (priority) - Skinning & animation
- [ ] Store GX matrix-index bytes per vertex (currently parsed past).
- [ ] Decode real `igSkeleton2`: bone names, parents, inverse bind poses. Drop synthetic rig.
- [ ] Decode `igAnimation2` tracks (quantized quats, key times). Reference: sampler function in DOL.
- [ ] Wire Bevy `SkinnedMesh`, remove hardcoded `bone_count = 32` in `viewer.rs`.
- Done: Spyro idle animates in `--viewer`.
- Field offsets needed here come from targeted DOL lookups (Phase 2 tooling) for these classes only.

### Phase 2 - DOL tooling & Alchemy metadata
- [ ] `tools/rea-bridge/rea_analyze.sh`: Ghidra headless import (`PowerPC:BE:32:Gekko_Broadway`) + auto-analysis.
- [ ] Locate `igMetaObject` name strings / registry; map class name to vtable/ctor.
- [ ] Dump `igMetaField` tables (name, type, offset, size) for `igWiiGeometry`, `igImage2`, `igEntity`, `igComponent`. Replace guessed `w[n]` indices.
- [ ] Commit address/name map only (`tools/rea-bridge/symbols.txt`).

### Phase 3 - Entities & levels
- [ ] Decode `igEntity`/component layout in level `.bld`; replace name-heuristic `MarkerKind::from_node_name`.
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
