# Script VM (`tfbScriptInfo` / `OpCodeList`)

Code: `crates/skylands-formats/src/script.rs` (`ScriptSet` decoder, `Vm` evaluator,
`enemy_stats`, `LevelupTable`). RE aid: `cargo test -p skylands-formats dump_scripts -- --ignored`
(`SKY_BLD=level/Level_302_Chompywurm`, `permanent/global`, `character/Init_Setup`), listings go to
`/tmp/skylands-re/<name>/` and stay local. Ops without an `Op` variant (`OpControl`,
`OpChangeMembership`, `OpForEach`, ...) list their operands from `ScriptSet::opaque_operands`.

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
- Not run: move/sound ops.

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
- Hero movement: `<id>_<Name>.ai` startup `use` of "Minion - setup pre descriptions" passes
  `default speed` (Spyro `Speed_Med` 24), `default air speed`, `default ground/air turn rate`.
  `*_local` initial values can be stale; apply their Def-script `set`s first.

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
  Execute `0x8008f6ac` fills a `Slider` (0x70, vtable `0x80506e44`) and appends it to the list at
  `0x806db9ec`: elapsed +0x5c = 0, seconds +0x60, from +0x3c, to +0x48/+0x50, ease out +0x64 and
  ease in +0x68 = `0.02 * value` (SDA2 `r2` = `0x806f5a40`, `r2-0x41f4` = 0.02; 0 when unset).
  Tick `0x8008ecb8` (called from `0x8008c520` / `0x8008f584`; uses Gekko paired singles, so stock
  Ghidra cannot decompile it): elapsed += dt (scaled by +0x20 percent, not modelled); mode +0x1c 0:
  looping wraps by `fmod(elapsed, seconds)`, else ends at `to` once `elapsed >= seconds`; mode 1
  (ping-pong): span `2 * seconds`, ends at `from`. `u = |elapsed / seconds|`; `u == 1` writes `to`;
  `u > 1` -> `2 - u` (same ends, curve replayed backwards). Value = `from + (to - from) * E(u)` with
  `E` = `0x8008ec38`: `a` = ease out, `b` = ease in; `a + b == 0` linear; `a + b > 1` scales both to sum 1;
  `v = 1 / (2 - a - b)`; `u < a`: `v u^2 / a`; `u < 1 - b`: `v (2u - a)`; else `1 - v (1-u)^2 / b`
  (trapezoid speed: ease out shapes the start, ease in the end). `script_runtime::ease_curve`.
- `OpDisplace`: `_NP` +0x1c (target), `_combineMode` +0x20, `_lenNP` +0x24, `_headNP` +0x28, `_pitchNP` +0x2c.
  `OpRemove`: `_NP` +0x1c.
- Other ops with a `br` block (find variable, spawn, for each, ...) skip their block in the VM.
- Engine state lives in `Vm::engine` keyed by name-hash path; script locals stay per actor.
- `tint.alpha` (`5c35de8c.5d8b6dab`, 0..255; the only `tint` field hub scripts use, 124 times):
  starts at `Placement._tint` (+0xc8, `igVec4uc` RGBA; all 1260 hub actors `80 80 80 ff`) for
  placed actors and `OpSpawn` clones (template copy) instead of an unset 0. Core publishes
  `my.tint.alpha / 255` as `ActorTint`; render `actor_tint_system` draws the actor's parts
  (`ScriptedActorMesh` of its marker, `SpawnedActor` descendants) with a blended copy of their
  material at that alpha, and restores the material at 255.
- Clones by reference: `OpSetReference "My Lid" = @spawn` stores `RefTarget::Spawned(serial)`.
  After the setup block, writes and effects through that variable (`set "My Lid".tint.alpha`,
  `slide`, `remove`, `displace`, `turn`, `start sequence`) resolve to `Target::Spawned` and leave
  the VM as `Effect::ForClone { serial, effect }` (keys as the clone's `my` path; writes are also
  kept locally so the spawner reads them back). Core `clone_effect_system` hands them to the
  `SpawnedActor` with that parent and serial: scripted clones `apply` them to their state
  (`Set`, `Slide` included), script-less ones take only `tint.alpha` and `remove`. A slide on the
  clone inside its setup block (`slide @spawn.tint.alpha -> 255`) is a setup effect, run by the
  clone. A slide's `from` through a reference is the spawner's local copy, not the clone's
  current value. Not done: other actors than the script's own clones (`"tr data ref"."glow"`,
  placed props, macro outputs: `RefTarget::Unknown`), whole-colour `tint` slides
  (`slide my.tint -> "tint normal"`, an `Other` value), tint RGB, sliders on script-less clones.

## `OpSpawn` (`Op::Spawn`)
- Class size 0x30. Fields: template +0x20, a reference path stored as an `AbstractPlacementList`
  (one placement: `tfbActorInfo` x185, `tfbParticleInfo` x64, `tfbSpriteInfo` x1 in the hub; or
  an `EXID` handle: `global.bld` export such as `$f2`, engine name such as `$9b61ad43`; or a
  variable / macro param / other spawn holding a reference, optionally followed by fields such as
  `"CauldronLoot"#1.$144f0c62`), `_RHS` +0x28 (`PositionStack`, nearly always `my`), `_facingRHS` +0x2c,
  op +0x1c = saved current object, op +0x24 = spawned object while its block runs.
- Vtable `0x80507e44` (`getMeta` `0x800949e4` -> meta slot `r13-0x42d8`). Execute (+0x60) `0x8007c6bc`:
  1. Resolve +0x20 (`0x8008c800`). Failure: `pc += br` (setup block skipped).
  2. Not an `AbstractPlacement` (meta `r13-0x46e0`): a new instance of the resolved object's
     class goes into op +0x24 and the block runs with it (data objects, not actors).
  3. Else resolve +0x28 and clone (`0x8007a158`): new instance of the template's class with the
     template's fields; `_initMatrix` = the +0x28 object's `_currentMatrix`; `_parent` (+0x5c) =
     that object; `_activator` (+0x60) = the current object; facing from +0x2c when it evaluates
     (`0x80091790` -> `0x80098aa0`); then activate (`0x8007a698`).
  4. `0x8007c438`: no clone or clone `_activityState` >= 3 -> `pc += br`. State 2: activator =
     current object, activate. Otherwise `_activeTimeLayer` (+0x9c) = current layer. Then push a
     (script, pc, op) frame, save the current object (`r13-0x3c88`) in op +0x1c and make the clone
     current, op +0x24 = clone, so the `br` block (`set @op.facing = ...`, `displace @op`,
     `OpSetReference x = @op`) configures the clone.
  5. Block end (slot +0x60 `0x8007c3d0`): current object restored from op +0x1c, op +0x24 cleared.
- Open: whether a literal `facing 0` (`_facingRHS` with an empty path) counts as "evaluates"
  (most hub spawns); the meaning of the current object `r13-0x3c88` vs `my`.
- VM (`Op::Spawn`): only a single-placement template (`actor`) at an engine object resolves;
  everything else skips the block. The block runs at once with the op object meaning the clone:
  writes go to `Effect::Spawn.seed` (keyed as the clone's `my.*`), `displace` / `turn` / `remove`
  of it to `setup`, a reference to it is `RefTarget::Spawned(serial)` (per-actor counter,
  `ActorScriptState.spawned`). `my` stays the spawner. `ActorScriptState.spawns` queues the
  effects for the engine. A literal `facing` is applied (`Some(0)` for most hub spawns).
- Core (`spawn_script_clone`, `SpawnedActor`): `actor_script_system` drains
  `ActorScriptState.spawns` after each tick. A clone at `my` starts at the spawner's current
  transform, yaw = `facing` when given, scale = template `_scaleVec` / 100, its engine state
  seeded and its setup effects applied (`ActorScriptState::apply`), and runs the template's
  script (`SpawnTemplate.script`, `Placement._script` +0xa8) from the next tick. Clones of
  script-less templates are static. Clones draw their template's model: `IgbModel::template_geometries`
  (inactive `tfbActorInfo` -> `tfbPhysicsModel` geometry, keyed by the object offset `OpSpawn`
  points at) is built into `TemplateMeshes` with the level; `spawned_actor_mesh_system` adds one
  child part per material to each `SpawnedActor`. Rigged templates (`ActorModel.skeleton`, keyed by
  `ActorModel.offset`) go to `TemplateRigs` instead: joints at bind pose, skinned parts, articulated
  parts rigid on their joint (`mesh_bones`), and a `CharacterAnimator` whose graph is the template's
  `tfbAnimationStateData` states (`tfbPhysicsModel` +0x20 -> +0x0c list; state `_animations` +0x14 ->
  first `AnimationClip` +0x38 -> `tfbPhysicsAnimation` +0x08 path, matched to the clip on the
  template's own rig first, since rigs share paths), and its transitions
  (`tfbAnimationStateData` +0x10 `tfbAnimationTransitionList` -> `ActorModel.transitions`; Tessa's all
  end in her own states). `OpStartSequence` state -> `ActorAnimation.state` -> that state's transition
  chain. `CharacterAnimator.requested` takes a repeated request once, so intermediate clips play out to
  their auto transition (before, the next tick re-requested from the intermediate state and cut it). Hub rigged templates with states: Tessa, Mabu_Warrior, T-Bone/skaletones, Lobber,
  BackgroundBird, Pennant, Earth gate boulders (rigid). Not done: clones at other objects,
  particle/sprite templates (`tfbParticleInfo` x64), parenting (clones do not follow the parent),
  the default state of a clone with no `OpStartSequence` (plays `idle` or the first clip).
- Hub templates: 185 `tfbActorInfo` (82 with a script: level up tokens, baubles, `Hub_Coins.ai`,
  `2D_Coin.ai`, ...), 64 `tfbParticleInfo`, 1 `tfbSpriteInfo`.
- Hub: 485 spawns. `2D_Coin.ai` spawns `Coin_Art_01#0` at `my`, block = reference + `"Value" = 1`.
