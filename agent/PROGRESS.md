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
- [x] Hero spawn on ring tag, villain on trap tag. gilrs + keyboard input.
- [x] Movement, gravity, jump, ground raycast; combo/sweep/projectile combat; coins/gems.
- [x] `ActiveLevel` from VFS, `--level <NAME>`; `CollisionMesh::from_igb` with 2D XZ spatial grid.
- [x] Tick interpolation (`interpolation.rs`): `TickTransform` restored in `FixedFirst`, recorded in `FixedPostUpdate`, blended by `overstep_fraction()` after the fixed loop. Auto-added to `Velocity` entities; snap above `SNAP_DISTANCE` (5 units).
- [x] Marker spawning (spawn, doorways, triggers, props, collectibles); ground raycast fallback from `(0,50,0)`; kill plane `-350`.

### Render (`crates/skylands-render`)
- [x] PBR pipeline, textures bound to `StandardMaterial.base_color_texture`.
- [x] Level instances merged into one mesh/material; positions and normals (`transform_dir`) in world space.
- [x] Cameras: follow, free-fly, showcase orbit. Viewer `--viewer`. HUD.

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

## Next (ordered)
1. Store matrix-index bytes, map `Skeleton` to Bevy `SkinnedMesh`, sample `AnimationClip`.
2. Villain timer / energy, trap return.
3. Mock tag persistence to disk (`.tag`). Real tag write-back + hotplug need human review (portal protocol).
4. Enemy AI (melee, ranged), encounter arenas, villain trapping sequence.
5. Zone transitions (swap level archives on doorway), campaign progress, save profile.
6. Load perf: cache decoded level geometry after first extraction (only if load time hurts).

## Verification
- `nix-shell --run "cargo fmt --all -- --check"`
- `nix-shell --run "cargo clippy --workspace --all-targets -- -D warnings"`
- `nix-shell --run "cargo test --workspace"`
- `nix-shell --run "cargo run -p skylands-app -- --headless --ticks 120"`
- Viewer: `nix-shell --run "cargo run -p skylands-app -- --viewer --character spyro"`
