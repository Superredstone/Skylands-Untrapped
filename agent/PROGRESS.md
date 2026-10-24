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
- [x] Original player HUD (`sky4_hud.rs`, notes `agent/HUD.md`): sprites and layout from `global.bld` (`SpriteLibrary`, GX `RGBA8`), element badge, name, health bar + hp, XP bar + level, coins. Replaces the placeholder hero card / coin pill.
  - Tests: `hud_sprites_resolve_to_textures` (ROM), `test_decode_gx_rgba8_tile_order`, `sky4_hud::tests::*`.
  - [ ] Portrait, original font, P2 / ally / boss HUD, pause menu (`UI_PauseMenu_Streamed.bld`), title menu (`Title.bld`).
- [x] Skylanders-themed Debug Menu (`debug_menu.rs`):
  - F12 hotkey toggles debug menu (replaces direct free cam hotkey); Escape exits free cam.
  - Skylanders Trap Team visual styling: dark slate/obsidian dialog, antique gold runic borders, Traptanium cyan highlights, element-coded badge pills.
  - Selectable Skylanders roster (Trap Masters, Classics, Villains) with live model swapping and animation preview bar (Idle, Walk, Run, Attacks).
  - Level Warp changer: Academy Hub + all 18 Trap Team campaign chapters with seamless `ZoneTransitionEvent` / `ZoneTransitionProgress`.
  - Camera & Telemetry panel: Free Cam button activation with cursor management, live engine FPS/delta, Hero XYZ/velocity/health/coins, Camera eye/pitch/yaw, Portal device/tag/LED state, level collision triangle metrics.

### Extractor (`crates/skylands-extractor`)
- [x] First-run extraction via `wit` (devenv fallback), validation of `sys/main.dol`, `global.arc`, `global.bld`.

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
- [x] Fix character model deformation and multi-material skinning (`agent/CHARACTER_RENDERING.md`):
  - Primary skeleton resolution (`IgbModel::primary_skeleton_index`): picks the primary character rig (e.g. Spyro 50 bones, Snap Shot 43 bones, Kaos 46 bones, Food Fight 33 bones) instead of picking `skeletons.first()` (which picked 2-bone or 20-bone accessory/minigame rigs).
  - Preserved native model coordinates for skinned meshes: removed bounding-box rescale/shift that broke `Joint * IBP * vertex` alignment; initial joint world transforms set to `mat4_affine_inverse(&b.inverse_bind_pose)` ensuring mathematical identity `Joint * IBP = I` at bind pose.
  - Multi-material extraction (`decode_drawable_materials` / `load_image_rgba`): links `Drawable` -> `tfbRuntimeMaterial` -> `igCachedAttrList` -> `tfbTextureBindAttr` -> `igImage2` offset -> image index in `model.images`. Spawns child `SkinnedMesh` entities per submesh, binding distinct diffuse textures (e.g. Spyro body vs wings) and diffuse tint colors.
  - Submesh filtering: protected skinned submeshes from auxiliary billboard/ring heuristics so wings and details are preserved.
- Done: Spyro idle animates in `--viewer`.
- [x] Gameplay-driven animation: `animation_state_name` (core) picks `idle`/`walk`/`run`/`jump`/`fall`/`attack_*` for heroes, villains and enemies (`EnemyState::Windup` = `attack`). `character_animation_system` follows `ActorAnimation` (viewer clip only when `viewer_mode`; before, the always-present `ViewerSettings` pinned everyone to idle). `resolve_clip` maps states to file clip names with fallbacks (`combatidle`, `jumpstanding`, `falling`, `spinattack`...); attacks and jump play once.
  - Clip names are the last `_` segment of the `.ma` path (`CrockWarrior_CombatIdle.ma` -> `combatidle`). `AnimationClip.skeleton` records the Havok target rig; `IgbModel::primary_animations` drops secondary-rig clips (Kaos `kaosHead_Idle` 8.53 s vs `Kaos_Idle` 1.07 s).
  - [ ] Hurt (`damagefront`), landing, idle-to-walk transitions, blending between clips, walk playback rate from speed. Debug-menu animation preview is overridden by gameplay state outside viewer mode.

### Phase 2 - DOL tooling & Alchemy metadata
- [x] `tools/rea-bridge/rea_analyze.sh`: Ghidra headless import (`PowerPC:BE:32:Gekko_Broadway` / `PowerPC:BE:32:default`) + auto-analysis.
- [x] Locate `igMetaObject` name strings / registry; map class name to vtable/ctor (1,111 classes mapped via `igMetaObject::Register` at `0x801062e0`).
- [x] Dump `igMetaField` tables (name, type, offset, size) for `igWiiGeometry`, `igImage2`, `igEntity`, `igComponent`. Replace guessed `w[n]` indices (2,884 fields across 655 classes in `tools/rea-bridge/meta_fields.tsv`; structured `meta_fields` constants in `crates/skylands-formats/src/igb.rs`).
- [x] Commit address/name map only (`tools/rea-bridge/symbols.txt`, 3,336 symbols).

### Phase 3 - Entities & levels
- [x] Geometry placement: `IgbModel::decode_entity_geometry_links` walks `tfb*EntityInfo` `_drawables`/`_entities` -> `Drawable._geometry` (+ `igWiiGeometryAttr._geometry` +0x10) -> `Entity._transform` (`igTransform._m` +0x30), parent chain via `_parentIndex`. Hub: 4,854 links, only 26 non-identity (most level geometry is pre-baked in world space).
- [x] Marker positions: level placements are `tfbActorInfo` (hub: 1,260), name `igNamedObject._name` +0x08, position `AbstractPlacement._initMatrix` +0x24 (`tfbEulerTransform`: xyz, then Euler degrees at +0x30). `IgbModel::decode_actor_placements` feeds markers; hub now yields real coin/door positions (11 doorways, 0 spawns). Files without `tfbActorInfo` keep the old synthetic spiral.
- [x] Marker kind from script: `tfbActorInfo` (0xf8) derives from `Placement` (0xd8); `Placement._script` +0xa8 -> `tfbScriptInfo`, `_name` = `.ai` path (hub: 792/1260 actors scripted, e.g. `2D_Coin.ai` x248, `TransitCrystal.ai` x10). `MarkerKind::from_script` decides when a script exists; `from_node_name` only for script-less actors. +0xec -> `tfbPhysicsModel` (`.mdl` path, 259 script-less actors), unused so far.
- [x] Euler rotation (`_initMatrix` +0x30, degrees) applied via `mat4_from_euler_deg` (order `Rz*Ry*Rx` assumed; hub is yaw-only except ~10 actors, unverified).
- Note: IGZ v8 Wii tags are byte-reversed (`RTSR` = `RSTR` string refs, values are section pointers, not TSTR indices).
- [x] Doorway/zone link data; swap level archives on doorway; campaign progress:
  - Doorway resolution (`doorway.rs`): transit crystal reciprocal pairing (`Transit_XX_<A>-<B>` matches partner `<B>-<A>` and teleports directly to target crystal position), hub main entrance/Persephone room/horde arena routes, chapter gate level resolution, and non-hub level exits returning to hub.
  - Campaign progress (`CampaignProgress`): canonical chapter table for Chapters 1..=18, unlocks chapter upon completing previous chapter, tracks visited levels and active chapter.
  - Level swapping lifecycle (`state_transition_update_system`): tagged level entities (`LevelEntity`) despawned on level change, new level geometry/collision/markers loaded via `spawn_level_content`, player teleported to destination spawn while preserving hero/villain state across swap.
  - Visual sync (`LevelRenderMesh` & `level_render_mesh_sync_system`): detects `ActiveLevel` changes, cleans up old level meshes/materials, and instantiates the new level scene graph and textures.
- [x] Dedicated collision data (vs render mesh):
  - Architecture: Alchemy level files separate visual render meshes (`igWiiGeometry` with display lists) from collision primitives (`CollisionInfo`, 1,523 in hub).
  - Actor & prop collision: all 1,260 placed level actors link to dedicated `CollisionInfo` at `Placement._collision` (+0xc4):
    - `_shapeType` bitfield at +0x18: 0=Box, 1=Cylinder, 2=Capsule, 3=Sphere, 4=Mesh; properties include `_isSolid` (0x08), `_interactsWithWorld` (0x10), `_isActorSurface` (0x20).
    - `_transform` at +0x1c: local position (xyz) and Euler degrees rotation (xyz).
    - Extents at +0x34 (`_width`), +0x38 (`_height`), +0x3c (`_length`): full dimensions (e.g. Flynn statue 60x40x11, NPC bounds 3x6x3).
  - Environment collision separation: `CollisionMesh::from_igb` now separates visual display lists from analytical collision shapes (`CollisionPrimitive`), discarding non-colliding visual elements (vertical billboard foliage cards, flat decals), generating oriented collision boundaries from solid `CollisionInfo` volumes, and storing `CollisionMesh.primitives` for exact prop collision and vertical ground raycasts.

### Phase 4 - Gameplay logic from DOL
- [ ] Character stat tables (HP, speed, damage by element/level).
  - Finding: stats are NOT DOL tables. They are script VM literals (`OpSetValue` -> `ValueStack` -> `OpCreateVariable` name; RHS `RHSValueStack._value` +0x24, `_type` +0x20 = `EXNM` handle `IntMeasurement`/`FloatMeasurement`). Per character in `<id>_<Name>.bld` (`BaseDamage_PrimaryAttack_Fire` = 15 for Spyro, `BaseDamage_PrimaryAttack_Harpoon` = 20 for Gill Grunt); global formulas in `permanent/global.bld` (`max health`, `HP multiplier`, `return next exp target`, `Difficulty - * Damage Mult`), mostly computed, not literals.
  - [x] `IgbModel.script_constants` / `script_constant(name)` (`decode_script_constants`, `EXNM` parsed). Hero `AttackDamage` (primary/secondary/special, f32, rounded on hit; fallback 50/100/75) via `AttackDamage::from_constants`.
    - Survey of 283 hero `.bld`, 109 with `BaseDamage_*`: older characters (`000`-`115`, minis) name slots `PrimaryAttack`/`SecondaryAttack`. Trap Team (`450`-`485`, 40/46) don't, and use fractional values (e.g. Snap Shot `Arrow_main` 12.5, `Melee_slam` 30, `JumpSlam` 25).
    - Rule: named slot first, else file order. Special = first name containing projectile/missile/arrow/lob/shot/bullet/disc/frisbee/beam, else next unused entry, else primary. Heuristic, not verified against gameplay.
    - [x] `ProjectileParams {speed, lifetime}` from the script: first positive projectile/missile variable containing speed|velocity, and lifetime|lifespan. Spyro: 40 u/s, 0.35 s. Fallback 12 / 1.5. Radius is still 0.5.
  - Unit scale (measured): characters are native size in levels. `Placement._scaleVec` (+0xb8) = `[100,100,100]` on all 1,260 hub actors (a percentage). Primary-rig heights: Spyro 1.77, Gill Grunt 2.14, Kaos 2.25, Food Fight 2.29, Snap Shot 4.28, Chompy Mage 6.94. Hub median coin spacing is 4.0. The 3x6x3 `CollisionInfo` is a default volume (also on waypoints and dummies), not NPC size. So script/DOL units can be used raw and no hero/level rescale is needed.
  - [x] Per-character collider from mesh bounds (`IgbModel::primary_mesh_bounds` / `CharacterCollider::from_model`): calculates envelope height from meshes bound to the primary character rig (`m.skeleton == primary_skel`), deriving radius `(height * 0.28).clamp(0.45, 1.2)` and `step_height = (height * 0.2).clamp(0.35, 0.85)`. `actor_physics_system` tests feet and head spheres (capsule approximation) to prevent tall characters from clipping low ceilings.
  - [x] Movement from DOL `ModelParameters` (`ModelParameters::default()` / `from_model`):
    - Units reversed via DOL `0x80586ef4`: engine kinematics are imperial (`ft / sec`, `ft / sec / sec`, `deg / sec`). Converted via `FEET_TO_UNITS = 0.3048`: ground max speed 20 ft/s = 6.10 m/s (explaining our earlier 5.0 guess), jump initial speed 32 ft/s = 9.75 m/s (explaining earlier 9.0 impulse), ground accel 160 ft/s² = 48.77 m/s², ground decel 80 ft/s² = 24.38 m/s², turn rate 720 deg/s, air turn rate 1440 deg/s.
    - Wired into `hero_movement_system`: steering target velocity with rate-limited acceleration/deceleration, smooth yaw rotation using `turn_rate`, and jump impulse from `jump_initial_speed`.
  - [x] HP/XP curve data: `LevelupTable` (Init_Setup "Portal - Define Levelup Table": +11.1% max HP per level, 19 XP thresholds). Not yet applied to heroes (no XP/levels in core), see `agent/SCRIPT_VM.md`.
  - `ModelParameters` defaults come from DOL ctor `0x805a89fc` (SDA2 `r2` = `0x806f5a40`, `.sdata2` `0x806f05d4..`) with string unit definitions in `0x80586ef4`. Script base speed (`global.bld` `Return Value - Base Speed` 10/22) parsed when present.
  - Ghidra: `tools/rea-bridge/FindStatTables.java` (stat-string xrefs plus the `ModelParameters` ctor; writes local-only `/tmp/skylands-re/stats_dump.txt`).
- [x] Villain timer/energy, trap return rules, trapping sequence:
  - `VillainEnergy` (100.0 max, 3.33/s active drain ~30s duration, 5.0/s in-trap recharge) & `TrappedVillainState` (control mode, unlocked villain ID, swap cooldown).
  - Tag-team swap (`villain_tag_team_swap_system`): `WiiButton::Minus` (keyboard Minus/Tab/Escape/Backspace or gamepad Select) toggles control between Hero and Trapped Villain; controlled villain spawns with `Hero` + `ControlledVillain`, preserves position/velocity, loads villain bounds collider, kinematics (`ModelParameters`), and attacks; portal trap LED illuminates in element color; Traptanium speaker sink queues entrance speech.
  - Trap return rules (`villain_energy_system`):
    - Energy depletion (`current <= 0.0`): forced return to hero at villain position, portal speaker plays exhausted VO, flashing red trap LED.
    - Manual return (`WiiButton::Minus`): returns to hero, preserves remaining energy to recharge in trap.
    - Physical trap removal (`TagRemoved`): forces immediate return to hero, locks villain swap, extinguishes trap LED.
    - Damage conversion: combat damage dealt to controlled villain directly deducts energy instead of health.
  - Trapping sequence (`villain_capture_system` & `damage_system`):
    - Defeating enemy villain triggers capture when a trap tag is present in portal; pulses trap LED with element color, queues villain capture speech on Traptanium speaker sink (`audio_streaming.sink.queue_villain_speech`), sets `GameState::VillainCapture`.
    - Completing sequence unlocks villain in trap with 100% energy, drops capture bonus loot, and returns to `Hub`. If trap is missing, prompts `Insert a Trap to capture Villain!`.
  - Tests: `test_villain_tag_team_swap_and_manual_return`, `test_villain_energy_drain_and_forced_trap_return`, `test_damage_drains_controlled_villain_energy`, `test_in_trap_energy_recharging`, `test_trap_tag_removal_forces_return_to_hero`, `test_enemy_defeat_trapping_sequence_and_unlock`.
- [x] Enemy AI state machines (melee, ranged). Arenas/encounter waves deferred until after Phase 5 (`OpSpawn` controller scripts need the VM).
  - Survey of all 86 level `.bld` (47k `tfbActorInfo`): enemies are `Enemy_S4_<Type>[_<Part>].ai` (`_Enemies/__Sky4_Enemies/...`), e.g. Chompy x268, Troll_Near x255, AOESwarmer x132. Helper scripts (`_Projectile`, `_Controller`, `_Impact`, spawners, `Template`) are not enemies.
  - `MarkerKind::Enemy(EnemyClass)` via `EnemyClass::from_script_stem`. Near/Far split follows `LevelSystems.ai` `XpShares_Near` / `XpShares_Far` / `XpShares_HeavyNear` / `XpShares_HeavyFar`. Far = stem keyword (`lobber`, `shooter`, `gunner`, `far`, `mage`, `magnet`, `tank`, ...). Keyword table, unverified in-game.
  - Per-script constants are reachable: `tfbScriptInfo._opList` +0x14 -> `OpCodeList` (`+0x08` count, `+0x14` ptr array). Enemy scripts hold only state literals. Ranges used: `Enemy_S4_BirdHazard.ai` `aggroRange = 45`, `Enemy_S4_UndeadLobber.ai` / `StunGun.ai` `attackRange = 60`/`50`, `WilikinLobberGooSpitter.ai` `myProjectileSpeed = 50` (all ft, so x `FEET_TO_UNITS`).
  - `Enemy` component (`Idle -> Chase -> Windup -> Recover`), `enemy_ai_system` after `hero_combat_system`. It targets the nearest hero (ties go to entity order) and steers with `ModelParameters` accel. Near = melee `DamageEvent`, Far = `Projectile { hostile: true }` that only hits heroes. No RNG, so the game stays deterministic.
  - Enemy defeat: `Combat.ai` "Creature Death Generate Loot" -> `LootSystem.ai` `LootValue = 5`, one `Collectible { value: 5 }`. Fixed, no rarity/drop-rate roll.
  - Hero defeat: `Portal_Manager.ai` "Tag Defeated" / "Request Player Needs Tag Message". The hero despawns, its character ID goes to `PortalGameplayState.defeated_heroes`, the same tag is refused until the level changes, and a prompt asks for another Skylander.
  - Enemy HP/damage come from the script VM (`EnemyTuning`, Phase 5); fallback 100 HP, melee 25, ranged 20. Still fixed: windup 0.5 s, recover 1.0 s, melee reach 1.5.
  - Tests: `test_enemy_script_classification`, `test_level_enemy_markers` (ROM), `test_melee_enemy_chases_and_hits_hero`, `test_enemy_ignores_hero_outside_aggro_range`, `test_ranged_enemy_projectile_hurts_hero_not_enemies`, `test_enemy_defeat_drops_loot`, `test_defeated_hero_is_out_until_level_change`, `test_enemy_ai_is_deterministic`.
- [ ] Save profile layout.
- Constants go into Rust with a `// DOL 0x80xxxxxx` source note; no pasted decompiler output.

### Phase 5 - Scripts, text, audio, misc
- [x] `ScriptSet` / script VM (`skylands-formats/src/script.rs`, notes `agent/SCRIPT_VM.md`).
  - Decoder for `tfbScriptInfo` op lists (set/inc/dec/check/loop/jump/macro define/use/params), `global.bld` handle resolution via `EXID` namespace + `TOOR` roots.
  - `Vm`: deterministic evaluator (step and depth budget, unbound engine reads = 0 and listed). Reference/spawn/move ops are not executed.
  - Tests: `vm_*` (synthetic), `rom_global_handles_resolve`, `rom_enemy_stats`, `rom_levelup_table`, core `test_level_enemy_tuning_from_scripts`.
- [ ] Encounter arenas / waves (`Arena_Challenge_*`, `_ArenaMaster2.ai`, `BattleMaster.ai`, `Enemy_Setup_00x_Controller.ai`), after the VM.
- [x] Enemy tuning: `LevelSystems.ai` "Game - Define Enemy Stats" (archetype 3 Near / 4 Far, difficulty level 1, normal mode) -> `EnemyTuning`, computed on level load. Level 302: Near 55 HP / 11 dmg, Far 40 HP / 7 dmg.
  - [ ] Per-enemy-type stats (call sites pass `"enemy type"` references + difficulty levels), difficulty mode selection (save param via a `global.bld` macro).
- [ ] Loot rarity from `LootSystem.ai` "Check For Loot Value Based on Drop Rate" (needs `kRHSfuncRandom` + a seeded RNG).
- [ ] Localization strings (EU en).
- [ ] Sound bank to event mapping.
- [ ] Mock tag persistence (`.tag`). Real tag write-back + hotplug need human review (portal protocol).
- [ ] Load perf: cache decoded level geometry (only if load time hurts).

### RE rules
- Notes and addresses only in git; no binaries, decompiled dumps, or extracted assets.
- Portal protocol findings need human review before merge.
- Every decoded format ships a test that skips when ROM is absent.

## Verification
- `devenv shell -- cargo fmt --all -- --check`
- `devenv shell -- cargo clippy --workspace --all-targets -- -D warnings`
- `devenv shell -- cargo test --workspace`
- `devenv shell -- cargo run -p skylands-app -- --headless --ticks 120`
- Viewer: `devenv shell -- cargo run -p skylands-app -- --viewer --character spyro`
