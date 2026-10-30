# Title main menu (`level/Title.bld`)

Code: `crates/skylands-core/src/title_menu.rs` (state, input, `GameState::Title`),
`crates/skylands-render/src/title_menu.rs` (3D scene). The windowed app boots to the title
unless `--level` or `--viewer` is given; headless runs start in the hub.

## Scripts
- `MainMenuMacros.ai` "Menu - Main - Create": rotating-rig page ("Menu - 3d - Style - Rotating Rig
  - Create": `bounce selected item` 1, `initial selection` 1, `rotate rig` 1), rig
  `MainMenuCarousel` (uniform scale 60), title string `Main Menu Title`, `loop 3` buttons, button
  `actions` in id order: 0 "Menu - Main - Horde Mode", 1 "Menu - Main - Adventure" ("Title - Choose
  Play - Adventure Mode"), 2 "Menu - Main - Arena Mode". `action 4 actions` = "Menu - Main -
  Settings". Sets `Main Menu Buttons Silver` / `Gold` scale 85. Glow template `MainMenuGlow`.
  Per-button `camera look at` / `from` are null (handle 6).
- `ui_menu_3d_carousel.ai`:
  - "Button Index to Node Index": `button % node count`; button i sits on attach node i.
  - "Select": no wrap (`selected == 0` or `>= items - 1` is invalid); rig turns `360 / attach
    nodes` (20 deg), `-` for next, slid over 0.2 s when `anim`. "Page Layout" selects next
    `initial selection` times without `anim`. Right = `next` 1, Left = `next` 0.
  - Bounce: rig `animation palette` state = node index, `playback rate` 125 sliding to 25 over 9 s.
  - "Button Focus": highlight (gold) spawned on the button, alpha 0 -> 255 over 2.5 s, displaced
    1 at `heading + 180`; glow uniform scale 82, alpha 0 -> 255 over 0.25 s; button and highlight
    scale to `scale * 1.2` over 0.2 s. "Button UnFocus" removes both and scales back over 0.2 s.
  - Only buttons within `rig visible node count / 2` (5 / 2) of the selection exist.
- `Title_Mein.ai` "Title - Show 3d Environment": `"3d menu pos"` = `Logo Pos` (1524, 0, 1524),
  background `$5de` there at scale 25, mood box, camera `look at` / `look from` = `"3d menu pos"`,
  `look from distance offset` 100 (feet) at its heading, `fov` 68, `far clip` 2000.

## Models (`Title.bld` `tfbActorInfo`)
`MainMenuCarousel` = `UI_SKY4/Screens/Test/MDL/LevelSelect_Base.mdl`: 18 attach nodes
`J_Node_1..18` about 30 units from the centre, animation states 0..17 (bounce per node).
`MainMenu Button S 000..002` / `G 000..002` = `UI_SKY4/Screens/PauseMenu/MainMenu_{Horde,Story,
Arena}_{Silver,Gold}.mdl` (about 6 units across).

## Not done / assumed
- `MainMenuGlow` has a placement but no decoded model: no focus glow is drawn.
- Background (`$5de`, `PVP_BLands_BackDrop_01.mdl`?), mood box, lights, title logo
  (`AnimatedLogo.mdl`), press-start screen, save slot / difficulty pages, MOTD, prompts,
  Settings, Horde and Arena pages, menu sounds and music: not rebuilt. Adventure goes straight
  to the hub with the save already loaded.
- Highlight displacement is taken as outward (toward the camera).
- Stick deflection of 0.5 counts as a press.
