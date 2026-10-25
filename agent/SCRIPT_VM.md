# Script VM (`tfbScriptInfo` / `OpCodeList`)

Code: `crates/skylands-formats/src/script.rs` (`ScriptSet` decoder, `Vm` evaluator,
`enemy_stats`, `LevelupTable`). RE aid: `cargo test -p skylands-formats dump_scripts -- --ignored`
(`SKY_BLD=level/Level_302_Chompywurm`, `permanent/global`, `character/Init_Setup`), listings go to
`/tmp/skylands-re/<name>/` and stay local. Ops without an `Op` variant (`OpSpawn`, `OpStartSequence`,
`OpControl`, `OpChangeMembership`, ...) list their operands from `ScriptSet::opaque_operands`
(spawn templates as `tfbActorInfo name#AbstractPlacement._ID`).

## Layout (IGZ v6, big endian, 32-bit pointers)
- `tfbScriptInfo`: `_name` +0x08, `_opList` +0x14 (`OpCodeList`), `_masterVarList` +0x18.
- `igDataList` / `igObjectList`: `_count` +0x08, `_data` +0x14.
- `OpCode`: `_flags` u16 +0x18, `_branchPC` u16 +0x1a. `execute` is vtable slot +0x60.
- `OpSetValue`: LHS `ValueStack` +0x1c, RHS +0x20. `OpCheckValue`: LHS +0x1c, `RelOp` +0x20, RHS +0x24.
  `OpIncValue` / `OpDecValue`: LHS +0x1c. `OpLoopValue`: count RHS +0x20.
- `ValueRHSVariant`: `_varOp1` +0x08, `_arithOperator` +0x0c, `_varOp2` +0x10, `_resultFunc` +0x14,
  `_conditioner` +0x18.
- `RHSValueStack`: path list (as `igDataList`), `_type` +0x20 (`EXNM` handle: `IntMeasurement`,
  `FloatMeasurement`, ...), literal `_value` +0x24 when the path is empty.
- `OpCreateVariable._varName` +0x1c. `ValueInfo`: name +0x08, `_type` +0x1c, `_value` +0x20.
- `OpDefineMacro`: name +0x08. `OpUseMacro`: target stack +0x1c (path contains the `OpDefineMacro`).
  `OpMacroParameter`: name +0x1c, RHS +0x30, owning macro +0x38.
- Path words: `0x80000000 | i` = `EXID` handle `i`, otherwise an object pointer.

## Enums (DOL string tables near `0x806f1614`)
- `RelOp`: 0 LE, 1 EQ, 2 GE, 3 LT, 4 GT, 5 NE, 6 BEQ (bits set), 7 BNE.
- `ArithOp`: -1 none, 0 ADD, 1 SUB, 2 MULT, 3 DIV (data only uses these).
- `kRHSfunc*`: None, Random, Integer, Round, Floor, Ceiling, Absolute_value, Sign, Square, ...
  No script in global, Init_Setup or Level_302 uses a non-zero `_resultFunc`; the VM rejects it.

## Control flow (checked against listings)
- A failed check skips the `br` ops after it (DOL `0x8008fd70`, `0x80073b28`).
- `jump` (`OpFlow*`, `OpIfElse`, `OpBranch`) skips `br` ops. Flag `0x0008` on a check marks an
  else part: the then-block ends with a `jump` over it.
- Block ops own the `br` ops after them: `OpDefineMacro` (body), `OpMacroInterface` (params +
  `OpFlowMacro`), `OpUseMacro` (interface + callback body run after the call), `OpLoopValue`.
- `OpFlowBehavior` = return from the macro.
- A path whose first object is a check op means that check's LHS; a loop op means its counter
  (0-based). Op objects are shared between positions (same `br`).

## Macro calls
- Interface params with flag `0x8000` are declarations with defaults. At a call site, `0x8000`
  params are the definition's own objects (so outputs need no copy), `0x0000` params are
  arguments with their own objects, matched to the definition by name and copied back after.
- Target in another file (`use ?`): every call-site param keeps its literal.

## Handles
- `EXID` entry = (id, namespace). Namespace = FNV-1a of the lower-case file name (`global.bld`
  `0x2694be8e`, `level.bld` `0xba91d879`).
- For `global.bld`, `id - global EXNM count (47)` indexes global's `TOOR` root list. Level 302:
  `0xad4` = `Average_Minion_Damage` 10.0, `0xad6` = `Average_Minion_HP` 100.0,
  `0x5f8` = `Minion Global Tuning`.
- Other namespaces hold engine names (hash ids such as `$9b61ad43`); unresolved.

## Evaluator rules (`Vm`)
- One variable store keyed by canonical path, seeded from `ValueInfo` initial values and global
  handles. Engine state (unset handle/field paths) reads as 0 and is listed in `Vm::unbound`.
- `OpCheckReference` / `OpCheckMembership` cannot be evaluated without live objects: `NE` is true
  (the reference is set), everything else false.
- Int op Int stays Int (except DIV), mixed is Float, DIV by 0 = 0. Budget: 1M steps, call depth 32.
- Not run: spawn/move/sound/reference ops (`OpSetReference`, `OpSpawn`, ...).

## Data found
- Enemy stats: level `LevelSystems.ai` "Game - Define Enemy Stats" (not `Minion_Tuning.ai`).
  `HP = Average_Minion_Damage * Num_Hits_To_Kill`, `damage = Average_Minion_HP / Num_Hits_For_This_Archetype_To_Kill_Minions`,
  scaled by `Difficulty Level` (1-10), difficulty mode, coop, horde, challenge and trackerbot.
  Archetype 3 (Near) 5.5 / 9 hits = 55 HP / 11.1 dmg; 4 (Far) 4 / 15 = 40 HP / 6.7 dmg (level 1, normal).
  `XP` comes from `EnemySystem_Globals.XP_Value_*` (engine-filled).
- Hero levelling: `character/Init_Setup.bld` `Init_Setup_SaveGame_Portal.ai`
  "Portal - Define Levelup Table": `level hp mult` 0.111, XP 1000, 2200, ..., 197500 (19 levels).
  Global `PortalObject_Minion.ai` "Minion - Get HP Mult for Level" = `(level - 1) * 0.111 + 1`;
  "Portal - Minion - Stats - Get Max HP" = `stats.hp max * mult + added hp`.
- Hero base HP: `<id>_Def.ai` `set "<id>_HP_local" = HP_Minion_<tier> [+ HP_Minion_NudgeAmount]`.
  The tiers are 1.0 in `global.bld`; `Init_Setup_Minions.ai` "Minion - Tune Minions" writes the real
  values (Medium 270, Nudge 10, ...). `Vm::global_writes` collects them by export name.

## Actor runtime (`script_runtime.rs`)
- Script shape: `OpPreScript` (br = whole script) holds `OpStartUp` (block, run once), `OpShutDown`
  (block, not run) and the per-tick body. `OpUserBehavior` blocks after it are event handlers (not run).
- Engine fields are `EXID` name hashes (FNV-1a, lower case), cracked against `main.dol` strings:
  `my` `4c2dfe93`, `Script Timer` `6fb9e9ff` (seconds), `my.<0xc545bc21>.facing|tilt|roll`
  (degrees; parent name unknown), `relative facing|tilt|roll` (added every tick: windmill -2),
  `my.vertical offset` `6c749d1e`, `my.scale.uniform` (percent), `visible` `54fa99a1`,
  `tint.alpha`, `distance`, `location`, `movement.ground max speed`, `collision volume.*`.
- `OpSlideValue`: `_LHS` +0x1c, `_RHS` +0x20, `_secondsRHS` +0x24, `_easeOutRHS` +0x28, `_easeInRHS` +0x2c.
  Its `br` block configures the slider (`@slide.looping`, `@slide.playback mode` 1 = ping-pong).
- `OpDisplace`: `_NP` +0x1c (target), `_combineMode` +0x20, `_lenNP` +0x24, `_headNP` +0x28, `_pitchNP` +0x2c.
  `OpRemove`: `_NP` +0x1c.
- Other ops with a `br` block (find variable, spawn, for each, ...) skip their block in the VM.
- Engine state lives in `Vm::engine` keyed by name-hash path; script locals stay per actor.
