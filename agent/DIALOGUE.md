# Dialogue and localized text

Code: `crates/skylands-formats/src/text.rs` (`parse_language_pack`, `LevelText`, `npc_dialogue`),
core `Npc` / `ActiveDialogue` / `npc_interaction_system`, render `setup_dialogue_box` /
`dialogue_box_system`.

## Language pack (`<LANGUAGE>.pak`, IGZ, big endian)
- Every level archive holds `ENGLISH.pak` next to `level.bld` (`global.bld` also has `GERMAN.pak`).
  Load it scoped to the archive: `Level_Hub_2014_01/ENGLISH.pak`.
- Objects: one `LanguagePackInfo`, then `tfbSoundDataInfo` + `tfbPhysicsAnimationViseme` pairs (VO lip sync).
- `LanguagePackInfo`: `_name` +0x08 (`"ENGLISH"`), pool size +0x18 (bit 31 set), pool pointer +0x1c.
- Pool: NUL-terminated UTF-16BE strings.

## Level strings
- `StringInfo` objects in `level.bld` are 0x28 bytes: only `_name` +0x08 (`vo_flynn_00163`,
  `Name_Flynn`, `Auric_YES/NO_String`, ...). The text is filled at load time.
- String *i* of the pool belongs to `StringInfo` *i* in object-list order (hub: 503 = 503).
- `vo_<speaker>_<n>` keys name the speaking character.

## NPC conversations
- An NPC script's `OpUserBehavior` blocks are its conversations. Lines are the `"dialogue strings"`
  arguments (`OpMacroParameter._RHS` +0x30, a `RHSReferenceStack`) of the dialogue macros
  (`Cutscene Sequence - Dialogue `, `Cutscene - Designer CS - Add Dialogue`).
- The argument path's first word is a `StringInfo` pointer, an own-file `EXID` handle (namespace
  `level.bld`, export id - `export_base` indexes `TOOR`), or a string variable. Variables take the
  value of the last `OpSetReference` (target `ReferenceStack` +0x1c, source +0x20) in script order.
- Not resolved: sets bound through `OpChangeMembership` "set [set] to" with the engine handle
  `$9b61ad43` (namespace `49db4e67`). `NPC_Buzz.ai` cycles ambient lines that way
  ("Dialog Strings" + "Dialog Index"), as do most hub NPCs.
- Fallback: an NPC whose script yields nothing says its own `vo_<name>_*` lines, one per talk,
  sorted by key. Lines from cutscene scripts (`Brock_Reveal.ai`, `CS_Persephone.ai`, intros) are
  mixed in, and story gating (`Levels_Completed`, `OpSetBehavior`) is ignored: talks cycle.
- Hub result: Cali (3 conversations) and Flynn, Mags (1 each) from scripts; Brock, Buzz, Persephone,
  T-Bone, Tessa from the fallback. Auric, Batterson, Whiskers have no lines (no talk prompt).
