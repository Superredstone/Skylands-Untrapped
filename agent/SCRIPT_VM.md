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
- Target in another file (`use ?`, a stub `OpDefineMacro` with only a name): when it is a
  `global.bld` macro, it runs in a nested `Vm` over `global.bld` (`Vm::call_global`, index built
  once per `ScriptProgram`). The nested VM shares the actor's engine state, board, set counts,
  globals, rng, spawn counter and step budget; arguments go in by parameter name (values from
  call-site RHS, references from the call-site `RHSReferenceStack` +0x30), outputs (values and
  references) come back by name. `global.bld` locals/references persist per actor
  (`global_vars` / `global_refs`). Effects come back with slider paths re-expressed through the
  level's handles (a slide of a `global.bld` local is dropped). If the nested run errors (an op
  the VM does not model), nothing is kept and the call falls back: every call-site param keeps
  its literal. Save-param macros keep their own handling. Hub: 5832 calls / 44 macros, arena
  300/000: 1007 / 28, none falling back (2 s / 5 s runs).

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
- `OpFindVariable "name" on <tfbActorInfo>`: the named variable of that placed actor's script
  (`tfbScriptInfo._masterVarList` +0x18, `ScriptSet::master_vars`). Found: the block runs and the op
  means that variable (reads/writes go to `Vm::board`); not found (not a board actor, a
  `PlacementReference` such as `Current_BattleMaster`, a reference variable): the block is skipped.
  Core keeps a board entry for every placed actor some script targets (`ScriptProgram::find_targets`).
- `<global set>.count` (`$39b1ddf4`) reads `Vm::set_counts` by set name (`ScriptSet::set_names`), else
  the static member count. Engine-filled: `global.bld` `$51a` "Minions" = heroes in play.
- Sets (enum tables `0x805b5658` membership, `0x805b5880` combine, `0x805b58b0` order: name pointers
  then values). `OpChangeMembership` `_LHS` +0x1c set, `_combineOp` +0x20 (0 include, 1 exclude,
  2 intersect_with, 3 be_replaced_by, 4 add, 5 exclude_all), `_RHS` +0x24 group. `OpCheckMembership`
  `_LHS` +0x1c, `_membershipOp` +0x20 (0 includes, 1 excludes, 2 intersects_with, 3 includes_all),
  `_RHS` +0x24. `OpForEach` (0x34) set +0x20, order +0x28 (0 forward, 1 backward, 2 randomly).
  VM (`Vm::sets`, kept per actor): a group is a stored set's members or the one object a reference
  resolves to (`RefTarget`); members the VM cannot identify (macro outputs, `null`) are not added,
  so counts include only known members. include skips duplicates, add does not. `OpForEach` runs
  the block per member with the op as a reference to it (effects reach clones via `ForClone`);
  over a set the VM does not hold (engine-filled `Minions`) it runs zero times. Membership checks
  are evaluated for a stored set and a one-member group (where the four ops agree pairwise),
  otherwise the old reference fallback. Not done: set members that are engine objects of other
  actors, `OpFindSubSet`, set-valued macro params beyond reference passing.
- `PlacementReference` (`ScriptSet::placement_refs`; 4 in an arena challenge): a level-wide reference
  object. Core keeps `PlacementRefs` (reference -> entity) and, before each tick, writes each one
  into the actor's `refs` as that actor sees it (`my` for itself, `Spawned(serial)` for its own
  clones, else `RefTarget::Actor(entity bits)`); a changed value after the tick is resolved by
  `clone_effect_system` once new clones exist. Paths through an `Actor` reference become
  `Target::Actor` -> `Effect::ForActor` (writes, slides, displace, remove...), delivered to that
  entity's script state (script-less actors: tint and remove only). `OpFindVariable` through a
  reference uses the held actor's board key (placement offset for placed actors, else entity
  bits); actors held by a `PlacementReference` are on the board. `OpSpawn ... at <reference>` is
  `SpawnAt::Actor`, the clone starts at that actor's current transform.
- `OpFindSubSet` (0x30, object vtable `0x80506bc4`; `Op::FindSubSet`, run). `+0x1c` path = set, a null
  entry, then a member field (`"Minions".distance`); `+0x20` comparison, values 0-4 in the arena
  file (`RelOp` range; "distance < influence radius" is 3 = LT); `+0x24` RHS threshold;
  `_setRemainder` +0x28 = number of path entries after the null, computed by the init slot
  (+0x24 `0x800900c4`), 0 in the file. Execute (+0x60 `0x800901c8`): remainder 0 -> `pc += br`;
  else for each member (walked from the end) with it as the current object, evaluate the field,
  compare, collect `(value, member)` pairs, build a new `ScriptSet` (+0x2c) and run the block with
  the op meaning it. Unconfirmed: whether the pairs are sorted by value (the VM keeps set order).
  VM: members from a script set or an engine-filled `global.bld` set (`Vm::engine_sets`; core puts
  the heroes in "Minions" as `RefTarget::Actor`); the field is asked of the engine
  (`MemberFields`, relative to `Vm::self_id`); members it cannot answer for do not match. Matches:
  the op is that subset (a set, and a reference to its first member); none: `pc += br`. Core
  `ActorFieldSnapshot` answers `distance` `2eb31462`, `horizontal distance` `9059dae4` (XZ) and
  `heading` `9f481715` (degrees towards the member, 0 = +Z like `OpDisplace`; the engine's
  definition not reversed) from this tick's actor and hero positions. Reading a field of a
  member through the op (`@subset.<x>.heading`) is not answered yet. Dumps list its operands;
  `tools/rea-bridge/ppcdis.py` disassembles such Gekko code.
- Engine names cracked against `main.dol` strings (hash every printable string): `collision volume`
  `dad64577`, `weight` `50a09eb9`, `waypoints` `7a0abb2b` (`first` `4881d841`, `last` `63e1d819`),
  `collision` `2176db8f`, `facing` `86d05dcf`, `tilt` `603a6206`, `ID` `37386ae0`, `shadow cast`
  `414b070c`, `Normalized Angle` `e000decb`; `velocity` `32741c32` / `length` `83d03615` /
  `heading` `9f481715` / `pitch` `bd324ab1`, `vertical difference` `ecfb5226`, `horizontal length`
  `faa02963`.
- Movement: `global.bld` "Movement - Set Velocity" (`movementUpdate.ai`) ends at once when the
  mover's `collision volume.weight == 0`, else writes `<mover>.velocity.length / heading / pitch`.
  Core seeds `weight` from `CollisionInfo._weight` (+0x40; templates via +0xc4) and moves every
  scripted actor by `velocity.length` units/s along heading/pitch (degrees; heading 0 = +Z, pitch
  90 = up, as `OpDisplace`; the engine mover itself is not reversed). A path through a reference
  that holds an engine object (`"mover" = my`) is that object's engine path (`engine_ref_key`).
  Arena transport: weight 100, its scripts set `velocity.length = 0`; it does not fly yet because
  "Navigate Waypoints" steers to `"Nav - next wp"` from the "Transport Path" `waypoints`
  (`ActorWaypointList`, `tfbActorInfo` +0xf4), which the engine does not supply yet.
- Waypoints (no reflected fields): `tfbActorInfo` +0xf4 -> `ActorWaypointList` (0x18; count +0x08,
  array +0x14) -> `ActorWaypoint` (0x38): position +0x1c/+0x20/+0x24 (small values, likely local to
  the owning placement), +0x28..+0x34 zero in the arena paths (likely the orientation scripts read
  as `<wp>.$c545bc21.roll` = stop type, `.tilt` = wait seconds, `.facing`). Arena 300/000: 26 owners
  (`Arena_DS_Path_01..04`, `Arena_DS_Path_Center_Mech`, 14 `landingQueue(n)`, some enemies and rule
  controllers); the first dropship's "Transport Path" is `Arena_DS_Path_04` (4 waypoints).
  `Transport_Macros.ai` keeps navigation in an `"info"` record: `"Nav - wp index"` steps through
  `"Nav - Flight Path".waypoints` (`.count`), `"Nav - next wp"` is set by an `OpSetReference` with
  an index: `OpSetReference._indexRHS` +0x24 (`OpCheckReference._indexRHS` +0x28) picks element
  `i` of the list the right side names (`[info, "Nav - Flight Path", waypoints]`), and steering
  reads the waypoint's `heading` / `pitch` / `distance` from the transport.
  VM: `ScriptSet::waypoints` (by placement), `ScriptSet::ref_index` (by op), `RefTarget::Waypoint
  { owner, index }`; `<placement ref>.waypoints.count`; a path whose prefix is a stored reference
  reads that object's field (`referenced_field`): a waypoint's `orientation.facing/tilt/roll` from
  its angles (order assumed facing = +0x2c, tilt = +0x28, roll = +0x30, all zero in the arena),
  `distance/horizontal distance/heading/pitch` of a waypoint (core: its position as is,
  world space; see 27) or an actor from the engine (`MemberFields`).
- Debugging: `ActorScriptState::trace = Some(vec![])` records the ops one actor's scripts run
  (`(global.bld?, op offset)`, nested `global.bld` runs included); map offsets to the
  `dump_scripts` listings (`@offset`).
- Found with it in the arena transport ("Navigate Waypoints"), each fixed and tested:
  1. `Sky_Traps.ai` "Capture Sequence - Check State" asks `"trapping data"."defeat data".
     "defeated actor" != null`; the old "an unknown reference is set" fallback made every
     transport think a defeat sequence was running (velocity 0, end). Now a path from a script's
     own `ValueInfo` / `OpCreateVariable` / `ScriptStructure` variable that no reference was ever
     stored in (nor in a prefix) is `null`. Macro params keep the old fallback.
  2. Dynamic `use ?`: the target is a `ScriptGroupStack` path to a variable holding a macro
     (`"info"."Transport Type".<macro>`, `_editType` +0x18 = the empty base such as "Transport -
     Movement"); the override ("Airship - Movement", an `OpDefineMacroSpecialization`) is stored
     there with `OpSetReference`. `ScriptSet::use_paths` keeps such paths, a reference to a macro
     definition is `RefTarget::Macro`, and the call runs the held macro (else the base its
     call-site params belong to, `OpMacroParameter` +0x38).
  3. Records passed by reference: a reference argument whose path is a plain variable no reference
     was stored in (`"My Info"` passed as `"info"`) becomes `RefTarget::Alias(path)`; `canon`
     rewrites paths through the param to the caller's variable.
  4. Records: a variable with members (a path goes through it to another object,
     `ScriptSet::records`) is an object; an unset path naming one means the record itself
     (`Alias`), an unset leaf field stays `null`. `canon` follows an alias at any prefix.
  5. Data objects: `"Transport - Startup"` fills `"New Info"`, `spawn "New Info"` (a record, not a
     placement: `0x8007c6bc` step 2 makes a data object) and returns `"returned info" = @spawn`.
     The VM makes `@spawn` an alias of the template record (copy semantics not reversed; exact for
     one instance per actor, several instances from one record would share it).
  After these the transport's records resolve (`"My Info"` -> `"New Info"`, `"Transport Type"` ->
  `"Helecopter Dropship"` with its "Airship - *" macros) and it runs its real startup sequence
  ("Transport - Activate and Attach Enemies", `"startup state"` 7, attaching passengers).
  6. `global.bld` state is one level-wide table (creature registry `"active"`, locomotion
     `"actor control datas"`...): its variables, references and sets live in core
     (`ScriptVarBoard::global_*`) and move into each actor's state for its tick
     (`ActorScriptState::global_*`, `Vm::global_sets`). References stored there are absolute
     (`Vm::absolute`: `my` and this actor's clones as `RefTarget::Actor(engine id)`); reference
     and membership comparisons use absolute forms; an `Actor` reference to oneself is `my`.
  7. Data-object instances: `spawn <record>` makes a fresh object (`Obj(INSTANCE_BASE + n)`,
     counter at `[Obj(INSTANCE_BASE)]`) with the template's current values, references and sets
     copied under it; instances live in the shared `global.bld` store (`Vm::shared`, `vars_of` /
     `refs_of` / `sets_of`), so every actor reads the same one. A bare instance path is the object
     (`Alias`). (Supersedes the earlier "alias the template" shortcut.)
  8. Indexed `OpSetReference` into a set: `<set>[i]` is its i-th member (`null` out of range).
  9. Lists: `@e678c."active"` is a growable list of creature-info records, not a set creatures
     join: `Combat.ai` "Get/Adjust Creature Info Struct" runs `inc "active".count` (append a new
     empty record instance; `dec` drops the last) and `"info" = "active".last` (`first` /
     `last`, `global.bld` handle 188 = `63e1d819`). `Vm::list_of_count`, `new_instance`.
  10. Writes of references into shared state (`global.bld`, instances) are absolute too.
  11. References across actors: `RefBoard` beside `VarBoard` (a bound `OpFindVariable` reads /
      writes references there, absolute); every script clone is on the board; board actors
      export their declared references (`ScriptProgram::export_refs` / `import_refs`), seeded
      before their first tick.
  12. Records are classified by type, not by use: a variable's `+0x24` names an
      `OpDefineStructure` (a record), an `OpDefineMacro` or an engine handle (engine class /
      value). Only a bare record variable is an object (`Alias`); a record-typed field is a
      reference, `null` until set (`"trapping data"."villain just captured data"`).
  13. `OpSetReference` rebinds the variable (`Vm::ref_slot`: only the path before the last element
      is canonicalized), not the record an alias variable stands for.
  14. `find variable` on the clone being set up (`@spawn` in its setup block) binds a slot that
      travels with `Effect::Spawn` (`named_vars` / `named_refs`); core imports it into the
      clone's script state by name ("flight path" = "Transport Path").
  15. A reference parameter without an argument takes its own default (`RHSReferenceStack`
      +0x30: "Transport - Startup"'s "transport" = `my`), else `null`.
  16. A field read through a reference to oneself reads one's own engine state
      ("info"."Transport".distance), not an engine measurement.
  17. Placement tags: a `tfbActorInfo`'s `_tagList` (+0x18, `TagList`: `_initialTags` count
      +0x1c, data +0x24) lists `OpParameter` "tag" variables (`_varName` +0x1c, default `_RHS`
      +0x30) that every actor of that placement owns besides its script's `_masterVarList`
      ("flight path", "payload", "enemy1L".. on `GroupTransport_Helecopter`; "armored",
      "noticeRange" on enemies). Tag objects are shared between placements; each actor holds
      its own value. `OpFindVariable` finds them (`ScriptSet::tags`, `tag_refs`,
      `ActorScriptState::tags`). Reference defaults are `null` or an engine handle
      (`$9b61ad43` for "flight path"; unnamed).
      `OpFindVariable` execute is `0x80083bdc` (vtable `0x80508c08`, +0x60): it resolves
      `_owner` (+0x30, `_indexRHS` +0x34), asks the owner's virtual +0x40 for its variable list
      (`_filter` +0x28) and binary-searches it by name and container / contents types; a miss
      skips the block. Not reversed: which lists the virtual returns.
  18. Result funcs 12 / 13 (`0x80071a4c` / `0x80071a98`) are `cos` / `tan` of degrees (fdlibm
      `0x803d25f4` / `0x803d2bc8`): f32 `* 0.017453292`, double math, rounded to f32 (11 = `sin`).
  19. Actor measurements: `heading` `9f481715`, `pitch` `bd324ab1`, `vertical difference`
      `ecfb5226`, `horizontal length` `faa02963`, `distance` `2eb31462`, `vertical offset`
      `6c749d1e`, `horizontal distance` `9059dae4` sit together in `main.dol` (0x804a18xx):
      polar coordinates of an object relative to `my`, and settable (Transport_Macros places a
      passenger with `distance = 0`, then displaces it 40 up). `my.distance` is 0, so the
      dropship spawned on its first waypoint counts as arrived and docks there (that waypoint's
      roll flags a dock), waiting for "Command - go". The engine code behind the strings is not
      found (no direct pointers / `lis`+`addi` loads).
  20. Stage 1 (Auto Offload 0) unloads its dropships itself ("Stage 1 - 01 - Unload") once
      "ArenaMaster - Modify Artillery" succeeds, i.e. the artillery mod's controller is
      "ready". Mod switching ("ArenaMaster - Change Mod" -> `ArenaRules_AM2.ai`):
      - Mod ids are `StringInfo` constants ("Mod - Artillery"), compared by identity
        (`RefTarget::String`, `ScriptSet::strings`).
      - An `else if` check names the first check op (`@186d30`): its LHS, for
        `OpCheckReference` as for `OpCheckValue`.
      - A variable's container type (+0x20) `ScriptReference` makes it a reference to a
        record ("Selected Mod", "Current Mod"); `ScriptStructure` is the record itself.
      - Another actor's record is `RefTarget::Record { actor, var }`; actors export their
        records so, and the owner reads one as its own record (`Alias`, also through `canon`).
      - `find variable` on the actor itself binds its own variable (`Vm::own_vars`): "Artillery -
        Startup" keeps the spawned controller in "Mod Controller" on its own placement.
  21. Indexed references wrap (`0x8008c99c`, at `0x8008ca38`): `i % count`, plus `count` when
      negative; an empty list gives `null`. Loop counters are `OpLoop._index` (+0x1c), 0-based
      (`OpLoopValue` execute `0x8008fecc` zeroes it, `0x8009009c` steps it), so "Stage 1 - 01 -
      Unload"'s `"active dropships"[count - loop]` visits element 0 first.
  22. Position fields are registered at `0x805bc578` (`0x8001a078(name, unit, min, max,
      getter, setter)`, names as offsets from `0x804a08f0`): `distance` getter `0x80075154`,
      setter `0x80075224`; `heading` `0x800754b0` / `0x8007562c`. The getter returns
      `|object - reference| * 3.2808399` (metres to feet), where the reference is the engine
      global `r13-0x3c98`. `0x80090880` points that global at the `+0x18` `PositionStack` of
      the value / reference stack being read; with none it stays as it was (`my`).
      So each read names what it is measured from: Transport_Macros checks
      `"info"."Transport".distance` from "Nav - Move Target", and the move target's from
      "Nav - next wp". VM: `ScriptSet::relative` (per op, its LHS stack's `+0x18`), used by
      `OpCheckValue` and `OpFindSubSet` (`Vm::origin`). Not modelled: reference points on
      RHS reads, and the feet factor (core's `ActorFieldSnapshot` answers in world units).
  23. Dropship flight:
      - `spawn $4f8 at my` spawns a `global.bld` `tfbActorInfo` (`ScriptSet::global_template`).
        Core keeps script-less clones in `ActorFieldSnapshot` and moves them by `OpDisplace`.
        The move target is such a marker.
      - A reference held in a record field ("info"."Nav - Move Target", "info"."Transport")
        resolves like one in a variable (`Vm::held_ref`) for effects and engine paths.
      - `displace my.velocity len heading pitch` adds that vector to the velocity (length,
        heading, pitch, horizontal length, vertical difference). This is how
        Transport_Macros accelerates.
  24. Units: scripts measure in feet, the world is in metres. The `distance` getter
      multiplies by 3.2808399, its setter (`0x80075348`) and `OpDisplace` (`0x8007d58c`)
      by 0.3048. Core answers `distance` / `horizontal distance` in feet. It keeps
      `ActorScriptState::displacement` (and `vertical offset`, velocity) in feet and places
      actors and markers at `x 0.3048` (`FEET_PER_METRE`, `METRES_PER_FOOT`).
  25. Passengers:
      - "BattleMaster - Spawn Dropship" spawns the dropship, then each passenger, and
        writes `find variable "enemy1L" on "transport" = @spawn`. In the game a spawned
        actor exists at once; here clones are made after the tick. So a find on a clone
        spawned earlier in the run binds a slot that `Vm::fold_spawn_slots` puts into its
        `Effect::Spawn`. Core reserves every clone's entity first, so `Spawned(serial)`
        between the spawner's clones of a tick become their entities.
      - Writing another object's `distance` (`Effect::Place`) moves it along the line from
        the write's reference point to that many feet ("Activate Enemy":
        `"enemy".distance = 0` puts it on the transport, then `displace 40 up`).
      - `<object>.attach node = <parent>` (`Effect::Attach`, `attach node` `5ba11be8`)
        keeps it at its offset from the parent (`AttachedTo`, `attach_system`), plus what
        its script displaces it since; `null` lets go where it is.
      - Not modelled: the transport model's `attach nodes` (`d99e7501`, handle 38), so
        passengers hang at the transport root plus the script's offsets, all at one point.
  26. Right-hand reads have reference points too (`RHSValueStack` `+0x18`,
      `ScriptSet::relative_reads` by (op, path), `Vm::cur_op`): "Navigate Waypoints" steers
      the move target by `"Nav - next wp".heading` / `pitch` measured from the move
      target. Core runs `attach_system` after the script tick as well as after effects, so
      a released passenger is let go where it was.
  27. Docking and unload:
      - `ActorWaypoint` +0x1c..+0x24 are world positions, not in the path placement's
        space: the arena's dock waypoints sit ~270 m from where the transformed ones
        were, next to `Landing_Queues`. `+0x28` / `+0x2c` / `+0x30` (tilt, facing, roll)
        flag stops: roll 1 drops a payload, roll 2 docks (stop within "WP Tolerance -
        Stop", detach passengers, "Command - docked" = 1); tilt < 0 waits for
        "Command - go", > 0 waits that many seconds.
      - Fields of another actor's record are shared (`record_key`; the owner exports
        its records' fields and imports others' writes; `Vm::remote_record`). "My Info"
        is a data-object instance, which is shared anyway.
      - Static `ScriptSet`s keep their member objects (`ScriptSet::set_members`); one
        passed by reference is an alias of itself, and placements in it are measured by
        `MemberFields::placement_field` (`find subset of "available landing queues"
        where .distance < 200`).
  28. Vertical flight: "Nav - vert accel" maps the transport's `vertical offset`
      (`6c749d1e`) measured from "Nav - Move Target" onto [max vert accel, 0]. That read
      is a macro argument: reads in an argument are keyed by the parameter op
      (`Vm::cur_op` = the parameter while its argument evaluates). Core answers
      `vertical offset` / `vertical difference` (`ecfb5226`) as the object's height above
      the reference, in feet.
  29. Passenger activation runs through `EnemySystem_Global.ai`: after the unload,
      "Enemy System - State Logic - Return From Disabled Logic" (currentQueue / alertLevel
      0) calls "Enemy System - State - Set State", which looks the state up with `find
      subset of "enemyInfo"."enemyTypeData"."states" where ."key" == stateName`.
      `OpFindSubSet` on a script field (not an engine handle) now reads `<member>.<field>`
      for record members.
      - A reference argument, or a reference parameter's default, naming a variable no
        reference was stored in, or a field of a data-object instance, stands for that
        variable (`Vm::ref_arg`). "stateName" defaults to
        `"EnemySystem_Globals"."State_Idle"` (a `ScriptReference` to a `ValueInfo`), so
        state keys are values, not strings. "enemyInfo" now binds to
        `<creature description instance>."creature info"."enemyInfo"`.
      - Still empty: that "enemyInfo"'s "enemyTypeData" is `null`. The creature's setup
        ("returned - need things defined" = 1) runs "Enemy System - State - Add State"
        through `call_global` with `"enemyInfo"."enemyTypeData"`, where its own
        "enemyInfo" = `"returned - description"."creature info"."enemyInfo"`. Those are
        `ScriptReference` fields, `null` here.
      - The creature's setup is the level stub "Enemy System - System - Init" (a
        `global.bld` macro): it spawns `"EnemySystem_Globals"."ForCloning_EnemyInfo"` as
        its "enemyInfo", finds its type in `"EnemySystem_Globals"."EnemyTypeDatas"` by
        `lookupID` or appends one ("returned - need things defined" = 1 for the first of
        a type). `lookupID` is `"Object Namer"."Name checksum"` (`9aae6bad` /
        `ba57384d`, cracked from `main.dol`) after `"Object Namer".$06254b22 =
        enemyType`: a checksum of the enemy type placement's name. The engine's checksum
        is not reversed; the VM uses the name hash (`Vm::object_namer_checksum`), which
        keeps equal names equal. Writes through `<set>.first` / `.last` inside a longer
        path (`"EnemyTypeDatas".last."lookupID" = ...`) land in that member's record
        (`canon`). The arena now has one entry per enemy type (4). sets `"enemyInfo"."enemyTypeData" = @find.first` (or
        `.last`), and `"return creature info "."enemyInfo" = "enemyInfo"` (@147c04,
        which works).
      - `<find subset op>.first` / `.last` apply to the op as written: the op also stands
        for its first match, which `canon` follows.
      - A `jump` past the end of the block being run leaves the block (`Flow::Goto`):
        "Combat - Get/Adjust Creature Info Struct" searches `@e678c."active"` for the
        creature in a loop and jumps out on a match. The VM used to end only that loop
        iteration, so every call appended a new entry and the creature's description and
        "Init" held different creature-info records. Now they share one.
      - Data-object instances are shared between files, but each file has its own copy
        of a structure (`global.bld` "creature info" is `914656`, the arena's `605040`).
        Fields of an instance are now keyed by name (`FIELD_BASE | name_hash`,
        `Vm::field_keys`): `canon` walks the path as written and uses name keys for
        lookups and its result; `ref_slot` and `instantiate` key the same way. With that,
        the type data's "states" fill (6-7 per enemy type).
      - Still not activating: each state's `key` reads 0. "Add State" sets
        `."key" = "stateName"`, a reference to `"EnemySystem_Globals".State_*`, which no
        script assigns; the lookup compares it with `"stateName":Int`. So the engine
        must compare references by identity there. Two attempts were reverted (they did
        not make the keys match): reading a reference to an unset value variable as its
        name identity, and passing value variables into `call_global` by name (the level's
        `[Handle(29), Obj(605708)]` means nothing inside `global.bld`).
      - Engine side: the value evaluator `0x80091790` calls a function pointer stored in
        the RHS object at `+0x14` (set when it loads), so the read path of a reference is
        not visible statically; the level reaches `State_*` through `global.bld`'s
        exported record (`Handle(29)`), so in the game both sides point at the same
        objects, and identity is the likely meaning.
      - A second try (identity reads, by-name cross-file arguments, and a reference to a
        by-name variable staying a reference) put the right reference into the state's
        `key` write, but the write's slot kept `.last` unresolved; fixed (`canon` now
        resolves a trailing `.first` / `.last`, which `ref_slot`'s prefix ends with). The
        identity changes were reverted again, then redone on top of the `.last` fix:
        - A reference read as a value is the referenced variable's identity, when that
          variable holds no value (`Vm::referenced_identity`; name hash, since each file
          reaches the variable through its own copy of the field).
        - Into `global.bld` (`call_global`), an unset reference argument passes an instance
          path as itself, or a value variable by name (`named_var`); a reference to such a
          by-name variable stays a reference (`ref_target`).
        With these, the passengers get a `currentState` and run the state logic after the
        unload ("currentState"."key" against `State_Spawn` / `State_Idle`, patrols).
      - Behaviour switches: "Add State" stores the creature's own behaviour in the state
        record ("behavior"), and the prescript runs `set behavior <state>."behavior"`
        (`OpSetBehavior` `_NP` +0x1c is a path; `RefTarget::Behavior`, VM index of
        `OpUserBehavior` ops). The switch runs in the `global.bld` VM, so `call_global`
        carries `behavior` in and back. `canon` tries `.first` / `.last` before following
        a prefix alias (`@find.first."behavior"`: the find op is also a reference to its
        first match). `ScriptProgram::tick` also keeps a `set behavior` made in the start-up
        block (it used to be overwritten by the first behaviour).
      - The starting-state switch (`@14bb10`) now resolves to the creature's "disabled"
        behaviour (the right one while carried). The later switch on a state change
        (`@14cb54`, when "changeState" = 1) is not reached yet: passengers stay
        "disabled". Next: why "Set State" does not set "changeState" after the unload.
        `call_global` binds reference arguments with plain `ref_target`.
  Now: stage 1 dropships climb to their dock waypoint (y 155.9), dock (~10 s) and
  unload; the arena moves on through "Stage 1 - 02 / 03 - Unload". Not right yet: the
  passengers stay "disabled" (29). After unloading, the dropships fly on past their last
  waypoint and keep climbing; what should end their flight is not traced.
  `tools/rea-bridge/maptrace.py` maps a trace onto the dumps; `ActorScriptState::references()`
  lists an actor's stored references.
- Script time steps by `Time<Fixed>::timestep()` (1/60 s per `FixedUpdate`), also under test and
  headless runners that run the schedule directly.
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
  clone, and so are writes through a reference to the clone whose block is still running
  (`TransitCrystal.ai`: `"my Prop" = @spawn; "my Prop".tint.alpha = 200` seeds it).
  A slide's `from` through a reference is the spawner's local copy, not the clone's
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
- The template may be a reference that holds a placement (`RefTarget::Placement`: a reference
  variable or macro param set to a `tfbActorInfo`); it spawns `ScriptSet::templates[offset]`.
  Script macro calls bind reference arguments by name from the call site's `RHSReferenceStack`
  (param +0x30), resolved in the caller; reference params without an argument start `null`.
  Engine-side calls (`Vm::call`) leave reference params unknown (enemy stats rely on that).
- `at` may also name a placed `tfbActorInfo` (`ScriptSet::placements`): `SpawnAt::Placement`, the clone
  starts at that placement's transform (arena controllers spawn templates at their own placement).
  Any other object path (reference variables, macro params) still skips the spawn.
- Hub templates: 185 `tfbActorInfo` (82 with a script: level up tokens, baubles, `Hub_Coins.ai`,
  `2D_Coin.ai`, ...), 64 `tfbParticleInfo`, 1 `tfbSpriteInfo`.
- Hub: 485 spawns. `2D_Coin.ai` spawns `Coin_Art_01#0` at `my`, block = reference + `"Value" = 1`.
