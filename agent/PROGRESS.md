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
- [x] Debug build symbol bloat (`target/` 46GB, ~900MB per crate test binary, OOM during linking). Fixed via `.cargo/config.toml` disabling debuginfo symbols (`debug = 0`) on dev and test profiles. Binary size 900MB -> 9MB.
- [x] Hero spawn on ring tag, villain on trap tag. gilrs + keyboard input.
- [x] Movement, gravity, jump, ground raycast; combo/sweep/projectile combat; coins/gems.
- [x] Character projectiles (`IgbModel.projectile`, `decode_projectile`): `tfbActorInfo` templates (`+0x08` name, `+0xec` rigid `tfbPhysicsModel`), spawned by `OpSpawn` (`+0x48` -> template, or a variable). Picked by name tier, direct spawns first; upgrade / Wow Pow / prop / `inviso` actors skipped. Gill Grunt `014_HarpoonSpear_Projectile`, Spyro `016_Fire`, Eruptor `009_LavaProjectile`, Gusto `450_Boomerang`, Trigger Happy `019_GoldBullet`; Stealth Elf none.
  - Characters with one fire it on Attack 1 (`ProjectileParams.primary_style == Projectile`, primary damage); rendered with the model via `Projectile.owner`. Speed literals are ft/s (`HarpoonVelocity` 150).
  - Melee characters (Stealth Elf, Ignitor, Tree Rex, Jawbreaker...) use `PrimaryAttackStyle::Melee` even when owning secondary projectile actors (`010_Fireball`, etc.).
  - Breath/stream characters (Spyro, Life Dragon, Sunburn...) use `PrimaryAttackStyle::Breath` decoded from `BaseDamage_PrimaryAttack_*` / `*breath*` animations: fires high-rate piercing projectile bursts matching original 0.25-0.35s lifetime and 40 ft/s speed (`016_Fire` `Turret_Elemental_Proj_Fire.mdl`).
  - [ ] Projectile particles (`tfbParticleInfo` trails), lobbed arcs (Eruptor), boomerang return, upgrade variants, enemy projectile models.
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
- [x] Spyro wing alpha cut-outs (`AlphaMode::Mask`), `igTextureAttr2` wrap modes on character textures, tail rig meshes remapped onto the body rig (`IgbModel::primary_rig_meshes`). See `agent/CHARACTER_RENDERING.md` item 8. Tests: `test_secondary_rig_meshes_remap_onto_primary`, `test_spyro_tail_and_texture_wrap`, `test_spyro_tail_wing_alpha_and_tiled_body`, `test_model_texture_uses_wrap_modes`, `test_alpha_mode_for_rgba`.
- [x] Gameplay-driven animation: `animation_state_name` (core) picks `idle`/`walk`/`run`/`jump`/`fall`/`attack_*` for heroes, villains and enemies (`EnemyState::Windup` = `attack`). `character_animation_system` follows `ActorAnimation` (viewer clip only when `viewer_mode`; before, the always-present `ViewerSettings` pinned everyone to idle). `resolve_clip` maps states to file clip names with fallbacks (`combatidle`, `jumpstanding`, `falling`, `spinattack`...); attacks and jump play once.
  - Clip names are the last `_` segment of the `.ma` path (`CrockWarrior_CombatIdle.ma` -> `combatidle`). `AnimationClip.skeleton` records the Havok target rig; `IgbModel::primary_animations` drops secondary-rig clips (Kaos `kaosHead_Idle` 8.53 s vs `Kaos_Idle` 1.07 s).
  - [x] Tracks are bound by bone name through the Havok rig skeleton (`tfbPhysicsBody` `+0x18`, `hkaSkeleton`), not by index; poses compose as `hkQsTransform` (scaled rigs). See `agent/CHARACTER_RENDERING.md` item 6. Tests: `test_havok_tracks_follow_rig_bone_names`, `test_havok_scaled_rig_kaos`, `test_map_tracks_by_name`, `test_pose_parent_scale_does_not_scale_child_translation`.
  - [x] Visual sync no longer rebuilds skinned multi-submesh characters every frame (`ActorVisual` marker; animation restarted each frame and `Children` leaked). Actor despawns are recursive so joints/submeshes don't outlive their actor.
  - [x] Weapons/accessories (`IgbModel.attachments`): rigid models from attach templates (`tfbSpriteInfo` `_ID` = rig attach node index) parented to joints. Base variant only (shortest `.mdl` name); upgrade selection needs `OpChangeMembership` in the VM. See `agent/CHARACTER_RENDERING.md` item 7. Tests: `test_character_attachments_from_attach_templates`, `test_attachment_follows_its_joint`.
  - [x] Portal entrance: a hero placed on the Traptanium (`TagArrived`) gets `PortalSpawn` (0.75 s), plays `spawn` (-> `spawn02` in hero files, `spawn` for Kaos) once, and ignores movement/attack input meanwhile. Duration is fixed in core (clips are 0.67-1.0 s). Test: `test_portal_tag_plays_spawn_animation_and_locks_input`.
  - [x] Magic Moment (portal-space sequence of a placed Skylander), rebuilt from `character/sky4_mm_streaming.bld` (`mmsky4_controller.ai`, `MMRig` uber model, tunnel/star tubes, element icons and backgrounds) and the 3D letters of `item/3d_font_streamed.bld`. Core `MagicMoment` holds the timeline (script stage timers, hero `ToyAlive` length); `render/magic_moment.rs` draws it on `MM_LAYER` (2). The in-world entrance (`PortalSpawn`) waits for it. Notes: `agent/MAGIC_MOMENT.md`. Tests: `test_portal_tag_starts_magic_moment`, `test_magic_moment_stage_times_follow_controller_script`, `test_magic_moment_stream_rig_states_and_scroll`, `test_hero_toy_alive_state`, `magic_moment::tests::*`.
    - [ ] Not done: sounds (`ui_mm_*`, needs `tfbSoundDataInfo` playback), particles (`PAR - *`, light trails, icon VFX, power blast), "Portal Master Space" / trap / villain / crystal modes, Swap Force variants, catchphrase VO, the in-game clouds after the hyper jump.
  - [x] State transitions and clip blending from the character file (`tfbAnimationStateData._transitions`). `tfbAnimationTransition*` (0x28): `+0x08` name, `+0x10` requested state id (-1 = automatic when the source clip ends), `+0x1c` blend seconds, `+0x20` source `tfbAnimationStateList` (empty = any state), `+0x24` destination list (the state played). Decoded into `IgbModel.animation_transitions`; `IgbModel::animation_graph()` pairs them with primary-rig state clips. `CharacterAnimator::advance` maps a request to its state, takes the specific-source transition before the any-source one, plays the destination and cross-fades the held old pose over the blend time; states with an auto transition hold their last frame and then move on. Spyro: idle -> walk plays `IdletoWalk` (0.133 s), then walk; run -> walk 0.267 s; default 0.064 s; `JumpStanding` auto-lands into `Landing`. Magic Moment poses skip the graph. Tests: `test_spyro_animation_transitions` (ROM), `test_animator_follows_state_transitions_and_blends`.
  - [ ] Hurt (`damagefront`: Spyro has no such state), core never requests `Landing` on touchdown (only reached via `JumpStanding`'s auto transition), tag-timed `tfbAnimationTransitionTagTarget` treated as basic (tag at `+0x0c` ignored), blend curve (`_blendShape`) assumed linear, walk playback rate from speed (no data found; `AnimationClip` `+0x20` = 100.0 and `+0x18` = loop flag unverified). Debug-menu animation preview is overridden by gameplay state outside viewer mode.

### Phase 2 - DOL tooling & Alchemy metadata
- [x] `tools/rea-bridge/rea_analyze.sh`: Ghidra headless import (`PowerPC:BE:32:Gekko_Broadway` / `PowerPC:BE:32:default`) + auto-analysis.
- [x] Locate `igMetaObject` name strings / registry; map class name to vtable/ctor (1,111 classes mapped via `igMetaObject::Register` at `0x801062e0`).
- [x] Dump `igMetaField` tables (name, type, offset, size) for `igWiiGeometry`, `igImage2`, `igEntity`, `igComponent`. Replace guessed `w[n]` indices (2,884 fields across 655 classes in `tools/rea-bridge/meta_fields.tsv`; structured `meta_fields` constants in `crates/skylands-formats/src/igb.rs`).
- [x] Commit address/name map only (`tools/rea-bridge/symbols.txt`, 3,336 symbols).

### Phase 3 - Entities & levels
- [x] Geometry placement: `IgbModel::decode_entity_geometry_links` walks `tfb*EntityInfo` `_drawables`/`_entities` -> `Drawable._geometry` (+ `igWiiGeometryAttr._geometry` +0x10) -> `Entity._transform` (`igTransform._m` +0x30), parent chain via `_parentIndex`. Hub: 4,854 links, only 26 non-identity (most level geometry is pre-baked in world space).
- [x] Placed model props: `tfbActorInfo._physicsModel` (+0xec) -> `tfbPhysicsModel` `+0x68` -> `tfbBodyEntityInfo` drawables, in model space. `IgbModel::decode_model_placements` instances them at `_initMatrix` x `_scaleVec`/100 (statues, NPCs, doors, birds, jump pads). Only actors with `_activityState` (+0x54) == 4; 5 marks templates scripts spawn later (debris, `_IGC` copies, digits, icons, cutscene NPC copies) and those stay hidden. Hub: 13 -> 777 placed instances; before, ~500 props piled up at the origin.
- [x] Actor collision/visual cleanup: `CollisionInfo._properties` bits = shape (3 bits), `_isSolid` 0x08, `_interactsWithWorld` 0x10, `_isActorSurface` 0x20, `_isActorPhysicsObject` 0x40. Every actor carries a solid default 3x6x3 box (0x18), incl. cameras, sound emitters (`emt_*`), coin layers, zone names, door openers, so all 830 hub "solid" volumes became invisible walls. `CollisionPrimitive.blocks_movement` = active, no drawn model, surface or physics bit, not mesh-shaped: hub 830 -> 23 (named blockers, cabinets, `*_Collision_Hack`). Model meshes collide only when the placing actor is solid (`IgbModel.non_colliding_nodes`). The level no longer draws models of enemy/collectible actors (their entities draw them; `ActorPlacement::draws_level_model`). Props whose model the level draws get `LevelDrawnModel` (no placeholder cube); inactive template props and the `light`/`lantern` name guess (matched camera dummies, Light element items, waypoints) no longer spawn props. Open: progression blockers (`Blocker_MainDoor`, `Geode_Blocker`) are permanent until scripts can remove them; shape 1 extents look box-like (e.g. 40x25x3 walls), so the shape enum order may be off.
- [x] Sky layer: a level is split into `tfbPhysicsWorld` layers, one per source scene (`*_ART_MASTER.ma`, `*_Interiors.ma`, `*_Collision.ma`, `*_Grass.ma`, `Levels/_SKY/<sky>/<sky>.ma`); `+0x28` -> `tfbWorldEntityInfo`. Sky layer domes are 1,000-30,000 units across and centred far from the level, so they rendered as huge misplaced parts and were fed to collision. `decode_sky_geometries` moves them to `IgbModel.sky_instances` (out of the scene graph); the renderer draws them unlit on `SKY_LAYER` with a `SkyCamera` (order -1, far 200k) that copies only the main camera's rotation. Open: `level_326_design_boss_alt.ma` (sky-like materials, likely script-toggled), Level 325 `Train_Skydome` actor model, and Level 326 hero spawn (no spawn marker, hero starts inside the origin dome).
- [x] Level textures: each level mesh already had its `Drawable._material` image; `spawn_level_render_mesh` now groups by (`image_index`, wrap, tint) and decodes images with `AssetLoader::load_images_rgba` (one file read) instead of one companion `.arc` texture for the whole level. Not done: multi-texture/vertex-alpha blend layers (302 terrain tiles show gaps), alpha blending (`AlphaMode::Mask` only), skinned NPC props render in bind pose.
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
  - [x] Hero level-1 max HP from scripts (`hero_max_hp`, `MinionTuning`, `GameAssetServer::hero_max_hp`): the `<id> <Name> Def` macro passes `"max hp" = "<id>_HP_local"`, a sum of `global.bld` tiers (`HP_Minion_VeryLow`..`VeryHigh`, `Titan_*`, `NudgeAmount`). Those exports are 1.0 placeholders in `global.bld`; `Init_Setup_Minions.ai` "Minion - Tune Minions" sets them at boot: VeryLow 200, Low 240, Medium 270, High 300, VeryHigh 340, Nudge 10, Titan 400/430/460. Spyro = Medium + Nudge = 280, Gill Grunt = Medium = 270 (Eons Elite `* "Elite - HP Mult"` branch skipped). Replaces the 500 default for heroes (kept as fallback; villains still 500). Tests: `rom_hero_max_hp_from_minion_tuning`, `test_hero_stats_from_character_script`.
  - [x] Hero movement from scripts (`hero_movement`, `GameAssetServer::hero_movement`): each Skylander's `<id>_<Name>.ai` startup calls global `Minion.ai` "Minion - setup pre descriptions" with `default speed` / `default air speed` / turn rates; that macro writes them to `my.movement` (ground max speed = default speed; `+ Player_Speed_Multiplier` 12 only while `Speed_FX_State` > 0, a speed power-up). Tiers from "Minion - Tune Minions": `Speed_VerySlow` 22, `Slow` 23, `Med` 24, `Fast` 25, `VeryFast` 26.5 ft/s; `TurnRate_*` 1440 deg/s for both heroes checked. Spyro 24 ground / 25 air, Gill Grunt 23 / 25. Overrides the 20 ft/s / 720 deg/s DOL `ModelParameters` defaults for heroes; accel/decel/jump stay DOL. `*_local` stat variables are evaluated from their Def-script assignments first (Gill Grunt's `014_Movement_Speed_local` has a stale initial 18). Tests: `rom_hero_max_hp_from_minion_tuning`, `test_hero_stats_from_character_script`.
    - [ ] Level scaling (`LevelupTable::hp_mult`) not applied (no hero XP/levels in core); armor / crit ("Minion - Get Base Values": level x 2, Spyro 6 / 6) not read (no armor/crit in combat).
  - [x] Villain stats from scripts: the same movement call drives villain speed (Chompy Mage `Speed_Med` 24, Dr. Krankcase 24) for trapped and controlled villains. Villains have no `_HP_local`; their Def passes `"max hp"` directly (Chompy Mage literal 1000), used for the portal `Villain` entity. Dr. Krankcase passes `$5f8."^Ally - HP High"`, a dangling `^` field with no value anywhere, so it keeps the 500 default. Controlled villains still use `VillainEnergy` as health.
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
  - [x] Layout decoded from script, not DOL (`skylands-formats/src/save.rs`, `SaveSchema::from_scripts`): `character/Init_Setup.bld` `Init_Setup_SaveGame.ai` "Save Init - Define All Save Params" calls "Save Init - Define Save Param" per param (`is header param`, `save param` reference name, `signed`, `fields total`, `field bit size`, `limit to val`), adding it to `"all header save params"` or `"all save params"`. "Save Init - Finalize Save Params and Construct SaveGame" packs each set as a bitstream in definition order (`bit index = bit ct`, `bit ct += num bits * fields total`, `bit ct / 32 + 1` u32 words); `SaveSchema::layout` / `words` reproduce that. Header: savegame version (8), expansion unlocks (4x1), sound vols (4x8), option flags (4x1), portalmaster ID (2x32 signed), then per-slot fields. Body: collections, arena, flags, `level flags` (20 bits x level), objectives, areas, portalmaster stars (180x1) / rewards / rank (6 bits, limit 40), village generic vars (100x32 signed), horde mode... Reference args are now kept in `ScriptSet::opaque_operands` for params without a value.
  - Counts set at boot are kept as `SaveCount::Runtime` when they can't be resolved statically (`track i` from the toy load tables, legendary treasure count), so bit indices after the first one are unknown until supplied. The skipped `^mobile - virtual portal` header param is assumed not to join the set (dangling reference), unverified. `^` names (toy/figure and mobile params referenced across files) are skipped. Tests: `save::tests::layout_follows_finalize`, `rom_save_schema`.
  - [x] Set counts resolved from `global.bld` static `ScriptSet`s (`ScriptSet.sets`: `_list` +0x18 -> `ScriptSetList` of `StringInfo` names; `ScriptSet::global_set`). `"level count"` = `$72e.count` = 22 levels in save-field order (`SaveSchema.levels`: `Level_300_BirdCage`, `301_TheMachine`, `302_ChompyWurm`, ... `313/314_Tutorial*`, `316`, `318`, `319`, `326_SciFi`, then the adventure packs `308_NiteLite`, `320_Light_Exp`, `325_FlynnXP`, `327_Kaos_Exp`); `"save slots"` = `$9a1.count` = 4 (`SaveName_Slot_(1..4)`). Header laid out through `collection - toys` (bit 607). `SaveSchema::from_scripts(init_setup, Some(global))`.
  - [ ] Resolve the toy-table counts (`track i`, legendary treasure `$716`, ally quests), per-field bit order inside a param and word endianness (engine side, unverified), field defaults ("Add Save Param Default Mapping"), Skylander figure save data (`^exp`, `^money`, `^hat`... on the tag), and an actual profile store wired to `CampaignProgress` / `Levels_Completed`.
- Constants go into Rust with a `// DOL 0x80xxxxxx` source note; no pasted decompiler output.

### Phase 5 - Scripts, text, audio, misc
- [x] `ScriptSet` / script VM (`skylands-formats/src/script.rs`, notes `agent/SCRIPT_VM.md`).
  - Decoder for `tfbScriptInfo` op lists (set/inc/dec/check/loop/jump/macro define/use/params), `global.bld` handle resolution via `EXID` namespace + `TOOR` roots.
  - `Vm`: deterministic evaluator (step and depth budget, unbound engine reads = 0 and listed). Reference/spawn/move ops are not executed.
  - Tests: `vm_*` (synthetic), `rom_global_handles_resolve`, `rom_enemy_stats`, `rom_levelup_table`, core `test_level_enemy_tuning_from_scripts`.
- [x] Actor scripts run in the world (`script_runtime.rs` `ScriptProgram` / `ActorScriptState`, core `ActorScript` + `actor_script_system`, render `ScriptedActorMesh`). See `agent/SCRIPT_VM.md` "Actor runtime".
  - A level-drawn actor whose `.ai` script writes `my.orientation.*`, `my.vertical offset`, `my.scale.uniform`, or does `OpDisplace` / `OpRemove` on `my` is a prop with its own mesh (`IgbModel::animated_markers` / `node_owner`), not part of the merged level mesh. Hub: 142 actors (birds, windmill, chandeliers, floating steps, elemental tiles, fish...), 0 VM errors.
  - `OpStartUp` once, then the `OpPreScript` body every 60 Hz tick; `OpSlideValue` sliders (looping, ping-pong), relative facing/tilt/roll rates per tick, `Script Timer`. Self-remove only when no unresolved state (save flags, other files) was read that tick.
  - Tests: `script_runtime::tests::*`, `rom_hub_scripts_animate_props`, core `test_actor_script_spins_placed_prop`, `test_actor_script_remove_despawns_prop`, `test_hub_props_run_their_scripts`.
  - [ ] Not run: `OpSpawn`, `OpStartSequence` (animations), `OpFindVariable` (treated as not found), `tint`, ease in/out. Run: `OpUserBehavior` / `OpSetBehavior` state machines, `visible` (hash `0x54fa99a1`), `Levels_Completed` global story progression gating, RHS result functions. Not visually verified in the GUI; per-tick angle units and facing axis assumed.
- [ ] NPCs (started).
  - [x] `MarkerKind::Npc` from `NPC_<Name>[_<Variant>].ai`, `Auric.ai` (store), `Persephone_2014.ai` (upgrades); `npc_name` gives the display name. `NPC_WantedPosters.ai` (no model) is not one. Inactive placements are templates and are skipped. Hub: Auric, Batterson, Brock, Buzz, Cali, Flynn, Flynn (`_Injured`), Mags, Persephone, T-Bone, Tessa, Whiskers.
  - [x] Core `Npc` entity (level draws the model), `npc_interaction_system`: "Press A to talk to X" within `NPC_TALK_RADIUS` (3.0, guessed) of the nearest NPC with lines. A press starts its next conversation (`ActiveDialogue` + `NpcInteractEvent`), A steps lines, the last closes it; walking away closes it. Hero movement and attacks are held while talking.
  - [x] Real dialogue text from the level `ENGLISH.pak` (`skylands-formats/src/text.rs`, notes `agent/DIALOGUE.md`), dialogue box in the render HUD (speaker, text, `[A] Next` / `[A] Close`).
  - [x] State-driven actor animations: `ActorAnimation.state` routed through `actor_script_system` and `CharacterAnimator::advance_state`. Dialogue switches NPC state to talk clip (state 9).
  - Tests: igb `test_actor_placement_kind_and_transform`, text `speaker_names_from_keys`, `rom_hub_language_pack_matches_string_infos`, `rom_hub_npc_conversations`, core `test_npc_talk_prompt_and_dialogue`, `test_hero_holds_still_while_talking`, `test_hub_npcs_spawn` (ROM), render `test_dialogue_box_shows_current_line`, `test_character_animation_system_routes_actor_animation_state`.
  - [x] Story gating of conversations (`Levels_Completed`, `OpSetBehavior`), `OpFindVariable` dynamic variable lookup across script op indices, which Flynn variant shows, visible (hash `0x54fa99a1`). Still unhandled: the `$9b61ad43` set handle (most ambient lines use the `vo_<name>_*` fallback), VO playback (`tfbSoundDataInfo`), yes/no questions, collision, Auric store / Persephone upgrade menus.
- [x] Localization strings (EU en): level `ENGLISH.pak` pools (`text.rs`). Global/UI strings not wired yet.
- [ ] Encounter arenas / waves (`Arena_Challenge_*`, `_ArenaMaster2.ai`, `BattleMaster.ai`, `Enemy_Setup_00x_Controller.ai`), after the VM.
- [x] Enemy tuning: `LevelSystems.ai` "Game - Define Enemy Stats" (archetype 3 Near / 4 Far, difficulty level 1, normal mode) -> `EnemyTuning`, computed on level load. Level 302: Near 55 HP / 11 dmg, Far 40 HP / 7 dmg.
  - [ ] Per-enemy-type stats (call sites pass `"enemy type"` references + difficulty levels), difficulty mode selection (save param via a `global.bld` macro).
- [ ] Loot rarity from `LootSystem.ai` "Check For Loot Value Based on Drop Rate" (needs `kRHSfuncRandom` + a seeded RNG).
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

## Character table (2026-10-25)
- Character archive numbers in `assets/files/character` are internal file IDs, not the IDs the name table assumed, so names and models were shifted (e.g. 1004 was named "Golden Queen" but loaded `1004_Chef.bld`).
- `portal_gameplay::CHARACTERS` is now the single source for name, element, archive and portrait; `get_character_info`, `get_character_asset_paths`, `resolve_character_model_archive` and `resolve_character_id_by_name` all read from it.
- Fixed hero archives: Wildfire → `458_HeatShield`, Wallop → `454_Earth_Hunter`, Gearshift → `471_Tech_Helix`, Krypt King → `478_KingCut`.
- Villain IDs now follow the archive IDs: 1002 Dr. Krankcase, 1003 Wolfgang, 1004 Chef Pepper Jack, 1007 Golden Queen, 1008 Dreamcatcher, 1040 Pain-Yatta.
- Light/Dark characters (482-485, `1005_Thief`, `1006_LightVillain`) are left out until `Element` has Light/Dark.
- The debug menu roster lists every entry in `CHARACTERS` and scrolls with the mouse wheel.
