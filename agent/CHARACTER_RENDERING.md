# Character Model Rendering Reverse Engineering & Fix

## Findings

1. **Skinning vs Scaling Math**:
   - `igb_model_to_bevy_mesh` baked an arbitrary bounding-box translation (`-center_x, -min_y, -center_z`) and scale (`target_height / orig_height`) directly into vertex coordinates.
   - However, `SkinnedMeshInverseBindposes` (IBP) and Havok animation matrices are computed in original Alchemy model space.
   - Bevy evaluates skinning on GPU as:
     `skinned_pos = sum(weight_i * Joint_i * IBP_i * vertex_pos)`
   - When `vertex_pos` is scaled and shifted while `IBP_i` and `Joint_i` are in original model space, `IBP_i * (scale * vertex_pos - offset)` deforms vertices around wrong origins, projecting vertices into infinity and tearing polygons across the screen (the exact visual in Image 1).

2. **Multi-Skeleton Rig Architecture**:
   - Alchemy character archives contain multiple `igSkeleton2` objects:
     - Spyro: S0 (50 bones - body), S1 (9 bones - tail), S2 (9 bones - tail series 3).
     - Snap Shot: S0 (36 bones - cinematic head), S1 (43 bones - Trap Master body).
     - Kaos: S0 (20 bones - boss head), S1 (46 bones - KaosAlly playable villain).
     - Food Fight: S0 (2 bones), S1 (33 bones - character body).
   - Previously, `visual_sync.rs` hardcoded `skeletons.first()`. For Kaos, Snap Shot, and Food Fight, this selected the wrong rig (2 bones or 20 bones instead of 46 bones), completely corrupting skinning indices.
   - The primary character skeleton is identified by selecting the skeleton that skins character geometry with the highest bone count and standard character rig bones (`J_root`/`J_origin`).

3. **Submesh Merging & Outlier Filter**:
   - `is_auxiliary_character_submesh` discarded Spyro's wings (`igGeometryAttr_5058`, width 5.56m, thickness 0.09m) under the heuristic that thin flat meshes are "horizontal ground decals".
   - Submeshes with `m.skeleton == Some(primary_skel)` are intrinsic character body parts and must never be filtered out by billboard/ring heuristics.
   - Meshes belonging to secondary skeletons (e.g. separate tails) or unskinned geometry must not be combined into the primary skinned mesh.

4. **Multi-Material Texture Mapping**:
   - `tfbBodyEntityInfo` contains a `_drawables` list (`DrawableList`).
   - Each `Drawable`:
     - `_geometry` (+0x0c) -> `igGeometryAttr` (mesh)
     - `_material` (+0x08) -> `tfbRuntimeMaterial`
   - Inside `tfbRuntimeMaterial`:
     - `_materialList` -> `igCachedAttrListList` -> `igCachedAttrList` -> `tfbTextureBindAttr`:
       `_texture` (+0x10) -> `igTextureAttr2` -> `_image` (+0x30) -> `igImage2` offset in file.
     - `igColorAttr`: diffuse color `[r, g, b, a]`.
   - In Spyro, body meshes use `igImage2` at `0x5764c` (256x128), while wings use `igImage2` at `0x5813c` (256x128).
   - `load_first_image_rgba` previously picked an unrelated 256x256 image and applied it to all submeshes.

5. **Initial Joint World Transforms**:
   - Joint entities were spawned with `b.local_transform` (parent-relative), but spawned as flat direct children of the actor root.
   - At spawn before the first animation tick, joints collapsed into a heap at the root.
   - Initial transform of each joint must be `mat4_affine_inverse(&b.inverse_bind_pose)` (bind pose in root/model space).

6. **Animation tracks bound to the wrong bones** (fixed):
   - Symptom: bind pose (animation off) rendered correctly in `--viewer`; any clip crumpled the mesh (tail stretched, body folded, wings floating).
   - Havok tracks follow the Havok rig skeleton order, not `igSkeleton2` order. Spyro `hkaSkeleton`: `J_vertical, J_root, J_root_B, J_hip_BL..J_foot_BL, J_hip_BR..J_foot_BR, J_tail_1..J_tail_0, J_root_F, J_hip_FL.., J_hip_FR.., J_neck, J_head, J_eye_*, J_jaw, J_lipCorners, J_pupil_*, J_wing_*` (49). `igSkeleton2`: `Unnamed bone, J_vertical, J_root, J_root_F, J_neck, J_head, J_jaw, ...` (50). The `hkaAnimationBinding` indices (identity, `originalSkeletonName` `SpyroJr:J_vertical`) index the Havok rig; we applied them to `igSkeleton2` with a `+1` offset, so head bones got leg tracks.
   - Rig location: `tfbPhysicsBody` `+0x14` size (low 27 bits), `+0x18` -> Havok packfile (`hkRootLevelContainer` -> `hkaAnimationContainer` -> `hkaSkeleton`); `+0x28` -> `tfbPhysicsCombinerLink` `+0x0c` -> the matching `igSkeleton2`. One rig per skeleton (Spyro: body + 2 tails, Kaos: head + body).
   - `hkaSkeleton` (2013, BE, 32-bit): `_name` +0x08, `_parentIndices` +0x0c, `_bones` +0x18 (`hkaBone` 8 bytes, name first), `_referencePose` +0x24 (`hkQsTransform` 48 bytes). Arrays are (ptr, size, capacity); pointers via `__data__` local fixups, classes via virtual fixups.
   - Fix: `parse_havok_skeleton` + `HavokRig`; `parse_havok_animation` maps track -> rig bone name -> `igSkeleton2` bone (namespace stripped: `Kaos_export:J_origin`). Rig = one bone per track, else most name hits. Unmatched tracks are dropped.
   - Scaled rigs (Kaos `J_root` / hips scale 1.4): Havok composes `hkQsTransform` (`t = t_p + R_p t_c`, `R = R_p R_c`, `s = s_p s_c`), so a parent's scale does not scale child translations. `sample_global_transforms` now composes that way; rest locals are re-derived from the bind pose in the same convention. Frame-0 vs rest bone-length mismatch: Kaos 0.118 -> 0.010, Spyro 0.17 -> < 0.01.

7. **Weapons and accessories** (fixed):
   - Symptom: Gill Grunt holds his arms out with empty hands; no backpack. Same with animation off: only `igSkeleton2`-skinned meshes were rendered.
   - Weapons are separate rigid models (`tfbPhysicsModel` without a combiner link, e.g. `GillGrunt_HarpoonGun.mdl`), mounted on the rig model's attach nodes (`tfbPhysicsModel` `+0x28` `BoneAttachInfoList`; `BoneAttachInfo` `+0x08` name, `+0x1c` `_boneName`, `+0x24` `_transform` xyz + Euler degrees).
   - The character script picks the models (`014 - Set Up Sprite Attachments`: `OpChangeMembership` adds `014_Gun_Set_00` / `014_Attach_Backpack_00` to `014_Portal_Toy_Attaches` by upgrade state). The templates are `tfbSpriteInfo` with `+0xd8` -> model; `AbstractPlacement._ID` (`+0x20`) is the attach node index (Gill Grunt gun 0 `gun`, harpoon 16 `harpoon tip`, backpack 17 `back pack`; Snap Shot bow 0, arrow 7; Flame Knight hilt 15, blade 0, armor 13; Stealth Elf knives 0 and 1) and `_initMatrix` rotation (`+0x30`) orients the model.
   - Implemented: `IgbModel.attachments` (`decode_attachments`), rendered as unskinned children of the joint entity (`AttachmentVisual`, `CharacterAttachmentEntity`). Base variant per node = shortest `.mdl` name (heuristic until script membership ops are evaluated; upgrades are ignored).
