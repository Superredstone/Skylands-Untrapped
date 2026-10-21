# Skylands Untrapped - Implementation Progress & Roadmap

## Status Overview
- Target: EU (en) *Skylanders: Trap Team* PC reimplementation (`rom.eu.wbfs`)
- Platform: Linux (`udev`/`hidapi-rs`) & Windows (`hidapi-winapi`)
- Runtime: Bevy 0.15 (fixed 60Hz tick loop, decoupled render)

---

## Completed Milestones

### Formats (`crates/skylands-formats`)
- [x] Alchemy v8.1.0 archive reader (`.arc` / `.bld`), Big & Little Endian support, Deflate/LZMA compression.
- [x] VFS mount system (`ArchiveMount`, `AssetLoader`) with path normalization.
- [x] IGZ object registry and directory parser (`igObjectList`, `igObject`, `RVTB` fixup decoding).
- [x] Wii geometry extractor: `igWiiGeometry` submeshes decoded from Section 2 with quantized 16-bit vertices, UV coordinates, and normal generation.
- [x] False GX display list injection fixed (prevented corrupted Section 3 audio streams decoded as geometry).
- [x] Texture decoding: CMPR (DXT1 tile big wii) and DXT1 format decoder with tile untangling.
- [x] Audio decoding: DSP ADPCM decoder and FSB5 container parser.
- [x] Skeleton and level marker extraction from scene graph nodes (`LevelMarker`, `MarkerKind`).

### Portal Driver (`crates/skylands-portal`)
- [x] USB HID driver for Traptanium Portal (VID: `0x1430`, PID: `0x0150`).
- [x] Software `MockPortal` with virtual tag injection.
- [x] Portal protocol: LED ring color control, trap slot light, antenna power, tag arrive/depart detection.
- [x] 8kHz mono audio streaming protocol to Traptanium speaker.

### Core Gameplay Loop (`crates/skylands-core`)
- [x] Deterministic 60Hz fixed update tick rate (`Time::<Fixed>::from_hz(60.0)`).
- [x] State machine: `Hub`, `VillainCapture`, `ZoneTransition`.
- [x] Tag gameplay resolution: hero spawn on ring tag, villain spawn on trap tag.
- [x] Input mapping: dual gamepad (`gilrs`) and keyboard support.
- [x] Character movement, gravity, jumping, ground raycasting.
- [x] Combat mechanics: primary combo, sweep attack, projectile firing, damage events, popups.
- [x] Collectibles: gold coins and elemental gems with pickup collision.

### Rendering & Viewer (`crates/skylands-render`)
- [x] Bevy 0.15 PBR render pipeline.
- [x] Mesh converter: multi-submesh combine, bounding box normalization to standard height.
- [x] Camera modes: third-person follow, free-fly debug camera, showcase orbit camera.
- [x] Viewer mode (`--viewer`) with character turntable and showcase cycling.
- [x] In-game HUD UI: health bar, coin counter, zone display, interaction prompt.

### Extractor (`crates/skylands-extractor`)
- [x] Automated initial-run extraction from `rom.eu.wbfs` via `wit` (with nix-shell fallback).
- [x] Asset validation checks for critical files (`sys/main.dol`, `global.arc`, `global.bld`).

---

## Next Steps & Roadmap

### Phase 1: Texture Binding & Visual Pipeline
- [x] Bind decoded CMPR/DXT1 textures to Bevy `StandardMaterial.base_color_texture` instead of placeholder solid colors.
- [x] Correct UV coordinate decoding for character and environment submeshes.
- [ ] Automated character texture pairing: resolve `.arc` texture bundle alongside `.bld` mesh bundle.
- [ ] Support normal maps and specular maps where present in `igImage2` registries.

### Phase 2: Real Level Loading & Collision Environment
- [x] Level loader: load full scene geometry from `Level_Hub_2014_01.bld` and `Level_313_Tutorial.bld` (fixed `igWiiGeometry` AABB offsets w[18..23] vs w[24..29]).
- [x] Build world `EnvironmentCollision` directly from level IGB collision meshes (`col_*`, `trig_*`) via `CollisionMesh::from_igb` with 2D XZ spatial grid broadphase.
- [x] Scene marker instantiation: auto-spawn player at level spawn marker or ground raycast, doors at doorways, collectibles at pickup markers, kill plane at -350.0.
- [x] Replace synthetic courtyard startup scene with loaded level geometry.
- [x] Correct visual scene graph placement via node transforms and dynamic edge-length / aspect-ratio bridge rejection (eliminating 154k+ vertex spikes).
- [x] Level texture binding fix: filter UI/portrait textures from level companion loader; multi-submesh material binding with neutral stone/earth PBR terrain fallback.

### Phase 3: Trapping System & Tag Persistence
- [ ] Villain timer / energy meter: timed villain summons, cooldowns, and return to trap.
- [ ] RFID tag memory persistence: write updated gold, experience, level, and hat back to tag blocks.
- [ ] Software portal tag persistence: save/load mock tags to disk (`.tag` files).
- [ ] USB portal hotplug recovery: detect disconnection and re-enumerate device cleanly.

### Phase 4: Enemy AI & Encounter Triggers
- [ ] Enemy archetypes: basic melee (Chompy) and ranged enemy AI state machines.
- [ ] Encounter arenas: lock doorway markers on combat trigger, unlock on wave clear.
- [ ] Boss encounter & villain trapping sequence: weaken villain -> prompt trap placement -> stream villain capture voice lines to portal speaker -> capture villain entity.

### Phase 5: Skeletal Rigging, Animation & 144Hz Interpolation
- [ ] Map extracted `Skeleton` joint hierarchies to Bevy skinned meshes (`SkinnedMesh`).
- [ ] Sample and evaluate keyframe curves from `AnimationClip` tracks (idle, walk, run, attacks).
- [ ] Transform interpolation between 60Hz fixed ticks for stutter-free 144Hz+ rendering.

### Phase 6: Game Progression & Save System
- [ ] Zone transition execution: stream and swap level archives on entering doorway triggers.
- [ ] Campaign progress tracking: level completion, collected stars, elemental gates opened.
- [ ] Save game manager: serialize player progression to local save profile.

---

## Verification Commands
- Run headless tick simulation: `nix-shell --run "cargo run -p skylands-app -- --headless --ticks 120"`
- Run showcase viewer: `nix-shell --run "cargo run -p skylands-app -- --viewer --character spyro"`
- Full test suite: `nix-shell --run "cargo test"`
- Format & lint: `nix-shell --run "cargo fmt --all && cargo clippy --all-targets --all-features"`
