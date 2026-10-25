# Magic Moment (Skylander placed on the portal)

The "Magic Moment" (`magic_moment_2014`) is the portal-space sequence played when a Skylander
is put on the portal. Code: `crates/skylands-core/src/resources.rs` (`MagicMoment`, `MM_*`
timeline), `crates/skylands-render/src/magic_moment.rs` (scene).

## Files
- `character/sky4_mm_streaming.bld`: script `mmsky4_controller.ai` and all art.
  - `MMRig` (`MMsky4_uberModel.mdl`): 17-bone rig, no visible mesh. Attach nodes
    (`BoneAttachInfoList`, index = script `attach nodes` index): 0 `camera` `J_camera`,
    1 `Skylander`, 2 `platform`, 3 `icon` `J_elementalIcon`, 4 `text`, 5 `name`
    `J_skylanderName`, 6-8 `tunnel light A/B/C` `J_RTLight*`, 9 `icon (swap force)`,
    10 `crystal`, 11 `skylander light`, 12 `waterWave` `J_elementalSludge`.
  - Rig animation states (`tfbAnimationState._id`): 0 Blank, 1 Portal Master Space, 2 LOOP,
    3 New Toy (1.17 s), 4 New Toy LOOP, 5 Magic Moment (1.90 s), 6 MM LOOP, 7 Hyper Jump,
    8/9 Hyper Jump alt B/C (1.43 s), 10 Hyper Jump LOOP, 13/14/15 Swap Force variants,
    24-27 crystal, 35 villain, 50-52 post capture.
  - `MM_dualEndedTunnelTube` (cloud walls, UV scroll v -0.775/s, tint), `MM_fastStars` /
    `MM_slowStars` (sparks on black, scroll 0.5 / 0.1), `MMicon` (`MMsky4_icon_random`),
    `MMicon_element_NN_<element>`, `MM_elemental_BG_NN_<element>` (articulated: drawables
    follow `Entity._parentIndex` joints; looping clip state `MMsky4_elemental_bg_<e>_vfxAnim`),
    `MM_platform_rig`, `MMCrystalRig` (crystal stamp).
  - `ScriptColorInfo` `MM_element_NN_<element>_color` / `MM_element_neutral_color`
    (`_value` +0x18, `0xAABBGGRR`): tunnel colours. Neutral `0xFFDD822C` = the script's
    initial `CloudTube.tint` literal.
- `item/3d_font_streamed.bld`: `fonts/3Dmodels/letter_A.mdl` ... (1.8 units tall).
- Hero files: state 80 `ToyAlive`, 81 `Toy` (Spyro `SpyroJr_ToyAlive.ma` 3.33 s).

## Script (`mmsky4_controller.ai`, modes = "mmsky4 - <mode> - init/update/clear")
Placed hero: world to portal -> toy on -> show minion -> minion to world.
- world to portal: stage 0 timer 0.5 s ("mmsky4 - set camera state" pull-out: `target zoomed
  out fov add` 21 on the Wii). Stage 1: `mm pos` displaced 7000 down / 7000 back, spawn
  `MMRig`, tunnel (tint `-2260436`), stars (fast alpha 255, slow 0), three `MM_LightsTrail`
  on tunnel lights (tints `-1763461`, `-10710234`, `-5885026`). Stage 2: `default.field of
  view` 89 (Wii, `Hardware Platform` 3; 120 otherwise) -> 68 over 0.5 s.
- toy on: rig state 3 (13 Swap Force), `MMicon` on node 3, timer 1.2 s.
- show minion: stage 0 element icon; +1.0 s. Stage 1: display actor on node 1, `ToyAlive`
  with `playback rate` 0, `replace color` black at 100% sliding to white over 0.8 s, 3D name
  on node 5 (`name scale` 25), rig state 5; +0.42 s. Stage 2: tunnel tint -> element colour
  over 0.4 s; +0.25 s, +0.15 s. Stage 4: element background on node 12 and platform on node 2
  fade in over 0.3 s; +0.3 s. Stage 5: `replace color percentage` 100 -> 0 over 0.12 s,
  playing animations `playback rate` 100. Stage 6: timer = `ToyAlive` length. Stage 9: state 380.
- minion to world: slow stars alpha -> 255 over 0.5 s, background alpha -> 0 over 2 s, rig
  state 7 + 2 (alt C), hero state 81 (`Toy`), FOV -> 89 over the rig clip, then
  `MM_sky4_inGameClouds` in the world.

## Engine handles (FNV-1a, `name_hash`)
`attach node` `$5ba11be8`, `attach nodes` `$d99e7501`, `tint` `$5c35de8c`, `alpha`
`$5d8b6dab`, `replace color` `$ad69c81c`, `replace color percentage` `$1602b664`,
`playback rate` `$d56cbb96`, `animation palette` `$8d18097e` (`OpStartSequence` + state id),
`default` `$933b5bde` . `field of view` `$a9bf5e19` / `far clip` `$291b093c`,
`playing animations` `$b967bb9c`, `visible` `$54fa99a1`.

## Rendering notes
- `J_camera` looks down its +Z (Bevy: -Z); FOV is horizontal.
- Effect meshes' GX vertex colours are 0..0.5 for full brightness (doubled).
- The tubes' origin is behind the camera: drawn first via `depth_bias`.
- The world and HUD cameras are off while in portal space.

## Assumed (not from the game)
- Pull-out curve (quadratic ease to +21 degrees over 0.5 s); keyframer targets in
  "mmsky4 - set camera state" (`-24.685`, `76.463`, `89.718`) not decoded.
- Light intensities / ranges of the tunnel and Skylander lights and the key light
  (the game's light actors were not decoded).
- Hero state 380 (stage 9, no clip in hero files) is skipped: "minion to world" follows at once.
- Letter spacing: 10% of a space between glyph bounds.
