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
