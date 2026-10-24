# Original menus / HUD (`UI_SKY4`)

Trap Team has no Scaleform UI. Menus and the HUD are engine sprites (`tfbSpriteInfo`) and 3D
models placed by `.ai` scripts inside level IGZ files:

| Screen | File | Scripts |
|---|---|---|
| Player / ally / boss HUD | `permanent/global.bld` | `Macros_PlayerHUD.ai`, `Macros_UI_Main.ai` |
| Pause menu (3D buttons `UI_SKY4/Screens/PauseMenu/Pause_*.mdl`) | `misc/UI_PauseMenu_Streamed.bld` | `PauseMenuController.ai`, `Menu3dMacros.ai` |
| Title / main menu | `level/Title.bld` | `UI_title_master.ai`, `MainMenuMacros.ai`, `Menu_CapsuleButtons.ai` |

Code: `crates/skylands-formats/src/sprite.rs` (`SpriteLibrary`), `decode_gx_rgba8` in
`texture.rs`, `crates/skylands-render/src/sky4_hud.rs` (player 1 HUD).

## Sprites
- `tfbSpriteInfo`: name +0x08, `SpriteInfoList` +0x74. List entry +0x20 = texture path or another
  sprite's name (`Generic_Container` -> `Generic_GraySquare` -> `gray.png`).
- `AnimatableTextureDataInfo`: path +0x08, natural size +0x24/+0x28 (screen units, about Wii
  pixels), `igTextureAttr2` +0x44 -> `igImage2` +0x30.
- `format_8888` art is GX `RGBA8` (format handle hash `0x0342d599`): 4x4 tiles, AR then GB.
- Portraits (`UI_SKY4/HUD/Portraits/2014/256x128/*`) are streamed from `.arc` files and have no
  image in `global.bld`.

## Script sprite fields
Field names are FNV-1a hashes (`name_hash`) of these engine names: `location` `$0bf5a9a6`, `x`
`$fd0c5087`, `y` `$fc0c4ef4`, `width` `$95876e1f`, `height` `$d5bdbb42`, `alignment` `$b1dd655e`,
`align to parent` `$03520355`, `priority` `$94e4e309`, `color` `$3d7e6258`, `alpha` `$5d8b6dab`,
`visible` `$54fa99a1`, `justify` `$ac1b6f0f`, `point size` `$3848bb22`, `numeric value`
`$aa66a0e9`, `scale` `$82971c71` (`uniform` `$de375a53`), `texture flip` `$e838ef97`, `blend mode`
`$00b359fb`, `natural size` `$7c61524b`.
- `ScreenMeasurement` literal (`location`): `y << 16 | x`, both signed 12.4 fixed point, y down.
- `SpriteAlignment` (DOL enum strings): 0 TOP_LEFT ... 4 MID_CENTER ... 6 BOTTOM_LEFT,
  7 BOTTOM_CENTER, 8 BOTTOM_RIGHT. Player 1 HUD = 6, player 2 = 8, ally HUD = 7.
- `ColorMeasurement` literal: `0xAABBGGRR`, 0-255 per channel (health colours: low `0x780000aa`
  red, mid `0x7800aaaa` yellow, full `0x7800aa00` green).
- `AlphaBlendModes`: OPAQUE, CROSSFADE, ADDITIVE, CUTOUT (3), RECEIVE_CUTOUT (4), CLEAR_CUTOUT (5),
  ... Bar fills receive the cutout of zero-width cap sprites.
- Priority: lower draws in front; treated as relative to the parent sprite.

## Player 1 layout (script units)
"Player HUD - Create": main 237x96, bottom-left, priority -25. "Minion HUD - Create" (justify 3;
player 2 mirrors x with justify 5):
- Container A (+0, +30): backdrop container (-128, 2) prio 200 -> `box_left` (mid-left to
  mid-right, `0xc8713720`) -> `dialog_back_edge` width 200 (`0xff713720`); `sky4 hud portrait base`
  line (0, -18) `0xff98987a`.
- Container B (same box): element base + icon (-105, 2); name text mid-left (32, -27) width 175;
  health bar container mid-left (30, -6) width = `hud - player health bar width`; XP container
  (-43, 13) prio -150 with base (11, 0), trim, 70-wide bar at (-25, 0), level number (-33, 1);
  money container centre to B mid-right (-90, 14), number mid-left, coin at (-w/2 - 2, 0).
- "UI - Health Bar - Create": base and trim scaled by width / 177, prio -2 / -4; fill prio -5;
  hp number mid-left (6, 1), width 70, prio -12. Texts get a 4-copy black stroke ("UI - Create
  Stroke").

## Assumed (not found yet)
- `UI Constants` initial values: health bar width (177 used), `font point size - small/smaller`
  (16 / 13), `trim gold`. `trim silver` = `0xff808080` read from the op after its declaration.
- Title-safe inset (16 units) and the 480-unit screen height.
- Fill cutout masks approximated by a 2-unit inset.

## Not done
- Portrait sprite (streamed `.arc` textures), the original bitmap font (`igCharMetrics`), text
  gradient, shine sweep, damage pulse / shake, XP and level data (core has none), player 2,
  ally / boss HUDs.
- Pause menu and title menu (3D `.mdl` buttons; need IGZ model loading of those files).
