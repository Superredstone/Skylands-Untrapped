//! Localized text: `<LANGUAGE>.pak` language packs and the level `StringInfo` objects they
//! fill, plus NPC conversations read from the `.ai` scripts. Notes: `agent/DIALOGUE.md`.

use crate::igb::{IgbModel, IgzFile};
use crate::script::{Op, ScriptSet};
use std::collections::HashMap;

/// `LanguagePackInfo`: `_name` +0x08, text pool size +0x18 (bit 31 = flag), pool +0x1c.
mod language_pack {
    pub const POOL_SIZE: usize = 0x18;
    pub const POOL: usize = 0x1c;
}

/// `OpMacroParameter._RHS` (+0x30), a `RHSReferenceStack` for string parameters.
const PARAM_RHS: usize = 0x30;
/// `OpSetReference`: target `ReferenceStack` +0x1c, source `RHSReferenceStack` +0x20.
const SET_REF_LHS: usize = 0x1c;
const SET_REF_RHS: usize = 0x20;

/// Decodes the strings of a language pack (`ENGLISH.pak` next to `level.bld`).
/// The pool is NUL-terminated UTF-16BE strings, one per `StringInfo` of the level
/// file in object-list order.
pub fn parse_language_pack(data: &[u8]) -> Option<Vec<String>> {
    let file = IgbModel::scan_igz(data).ok()?;
    let info = file
        .objects
        .as_ref()?
        .objects
        .iter()
        .find(|o| o.type_name.as_deref() == Some("LanguagePackInfo"))?;
    let size = (word(data, &file, info.offset + language_pack::POOL_SIZE)? & 0x7FFF_FFFF) as usize;
    let start = ptr(data, &file, info.offset + language_pack::POOL)?;
    let pool = data.get(start..start.checked_add(size)?)?;
    let units: Vec<u16> = pool
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_be_bytes(c))
        .collect();
    let mut out: Vec<String> = units
        .split(|&u| u == 0)
        .map(String::from_utf16_lossy)
        .collect();
    // The pool ends with a terminator, which leaves one empty tail.
    if out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    Some(out)
}

/// One localized level string.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelString {
    /// `StringInfo._name` (`vo_flynn_00163`, `Name_Flynn`, ...).
    pub key: String,
    pub text: String,
}

/// The level's `StringInfo` objects with their language pack text, by object offset.
#[derive(Debug, Clone, Default)]
pub struct LevelText {
    pub strings: HashMap<usize, LevelString>,
}

impl LevelText {
    /// Pairs `level.bld` `StringInfo` objects with `pack` strings by position.
    pub fn new(level: &[u8], pack: &[String]) -> Option<Self> {
        let file = IgbModel::scan_igz(level).ok()?;
        let strings = file
            .objects
            .as_ref()?
            .objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("StringInfo"))
            .zip(pack)
            .map(|(o, text)| {
                let key = ptr(level, &file, o.offset + 0x08)
                    .map(|p| cstr(level, p))
                    .unwrap_or_default();
                (
                    o.offset,
                    LevelString {
                        key,
                        text: text.clone(),
                    },
                )
            })
            .collect();
        Some(Self { strings })
    }

    pub fn by_key(&self, key: &str) -> Option<&str> {
        self.strings
            .values()
            .find(|s| s.key == key)
            .map(|s| s.text.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DialogueLine {
    /// From a `vo_<speaker>_<n>` key; `None` for keys without a speaker.
    pub speaker: Option<String>,
    pub text: String,
}

/// Lines one `OpUserBehavior` block of an NPC script shows, in script order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Conversation {
    pub lines: Vec<DialogueLine>,
}

/// Conversations of the script named `script_name` (path suffix). Each `OpUserBehavior`
/// block is one conversation; its lines are the `"dialogue strings"` macro arguments that
/// point straight at a `StringInfo`. Arguments routed through script variables (intro
/// strings, prompts) are not followed.
pub fn npc_conversations(
    level: &[u8],
    set: &ScriptSet,
    text: &LevelText,
    script_name: &str,
) -> Vec<Conversation> {
    let Ok(file) = IgbModel::scan_igz(level) else {
        return Vec::new();
    };
    let Some(script) = set.script(script_name) else {
        return Vec::new();
    };
    let ops = &script.ops;
    // A path's first word: an object pointer or an own-file `EXID` handle.
    let first = |stack: usize| -> Option<usize> {
        (word(level, &file, stack + 0x08)? > 0).then_some(())?;
        let entry = ptr(level, &file, stack + 0x14)?;
        match word(level, &file, entry)? {
            w if w & 0x8000_0000 != 0 => own_root(set, w & 0x7FFF_FFFF),
            _ => ptr(level, &file, entry),
        }
    };
    // String variables hold the `StringInfo` the last `OpSetReference` put there,
    // in script order (scripts set them right before the dialogue call).
    let mut assigned: HashMap<usize, usize> = HashMap::new();
    let mut out = Vec::new();
    let mut current: Option<Vec<DialogueLine>> = None;
    let mut block_end = 0;
    for (i, ins) in ops.iter().enumerate() {
        if current.is_some() && i >= block_end {
            out.extend(
                current
                    .take()
                    .filter(|l| !l.is_empty())
                    .map(|lines| Conversation { lines }),
            );
        }
        match &ins.op {
            Op::Other(t) if t == "OpUserBehavior" => {
                out.extend(
                    current
                        .take()
                        .filter(|l| !l.is_empty())
                        .map(|lines| Conversation { lines }),
                );
                current = Some(Vec::new());
                block_end = i + 1 + ins.br as usize;
            }
            Op::CheckRef { desc, .. } if desc.starts_with("OpSetReference") => {
                let lhs = ptr(level, &file, ins.offset + SET_REF_LHS).and_then(first);
                let rhs = ptr(level, &file, ins.offset + SET_REF_RHS).and_then(first);
                if let (Some(var), Some(target)) = (lhs, rhs) {
                    let target = assigned.get(&target).copied().unwrap_or(target);
                    assigned.insert(var, target);
                }
            }
            Op::Param { var, .. } if set.var_name(*var) == "dialogue strings" => {
                let Some(lines) = current.as_mut() else {
                    continue;
                };
                let line = ptr(level, &file, var + PARAM_RHS)
                    .and_then(first)
                    .map(|t| assigned.get(&t).copied().unwrap_or(t))
                    .and_then(|t| text.strings.get(&t))
                    .filter(|s| !s.text.trim().is_empty());
                if let Some(s) = line {
                    lines.push(DialogueLine {
                        speaker: speaker_from_key(&s.key),
                        text: s.text.trim().to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    out.extend(
        current
            .take()
            .filter(|l| !l.is_empty())
            .map(|lines| Conversation { lines }),
    );
    out
}

/// What an NPC says: its script conversations, or, when none decode, its own
/// `vo_<name>_<n>` level lines one per talk. Scripts such as `NPC_Buzz.ai` cycle
/// ambient lines from a set bound through an engine handle (`$9b61ad43`, unresolved),
/// so the fallback stands in for that set.
pub fn npc_dialogue(
    level: &[u8],
    set: &ScriptSet,
    text: &LevelText,
    script_name: &str,
    npc_name: &str,
) -> Vec<Conversation> {
    let scripted = npc_conversations(level, set, text, script_name);
    if !scripted.is_empty() {
        return scripted;
    }
    let prefix = format!(
        "vo_{}_",
        npc_name.to_ascii_lowercase().replace(['-', ' '], "")
    );
    let mut own: Vec<_> = text
        .strings
        .values()
        .filter(|s| s.key.to_ascii_lowercase().starts_with(&prefix) && !s.text.trim().is_empty())
        .collect();
    own.sort_by(|a, b| a.key.cmp(&b.key));
    own.dedup_by(|a, b| a.text == b.text);
    own.into_iter()
        .map(|s| Conversation {
            lines: vec![DialogueLine {
                speaker: speaker_from_key(&s.key),
                text: s.text.trim().to_string(),
            }],
        })
        .collect()
}

/// `EXID` handle into this file's own `TOOR` roots (namespace `level.bld`).
fn own_root(set: &ScriptSet, handle: u32) -> Option<usize> {
    let h = handle as usize;
    (*set.handle_ns.get(h)? == crate::script::name_hash("level.bld")).then_some(())?;
    let id = *set.handle_ids.get(h)? as usize;
    set.roots.get(id.checked_sub(set.export_base)?).copied()
}

/// `vo_tbone_02835` -> `T-Bone`, `vo_flynn_00163` -> `Flynn`.
pub fn speaker_from_key(key: &str) -> Option<String> {
    let rest = key.strip_prefix("vo_")?;
    let who = rest.rsplit_once('_')?.0;
    let mut chars = who.chars();
    let first = chars.next()?;
    Some(match who {
        "tbone" => "T-Bone".to_string(),
        _ => first.to_uppercase().chain(chars).collect(),
    })
}

fn word(data: &[u8], file: &IgzFile, o: usize) -> Option<u32> {
    data.get(o..o + 4)
        .map(|_| IgbModel::read_u32(data, o, file.endian))
}

fn ptr(data: &[u8], file: &IgzFile, o: usize) -> Option<usize> {
    let w = word(data, file, o)?;
    if w == 0 || w & 0x8000_0000 != 0 {
        return None;
    }
    let s = file.sections.get(((w >> 27) + 1) as usize)?;
    Some(s.offset + (w & 0x07FF_FFFF) as usize)
}

fn cstr(data: &[u8], o: usize) -> String {
    let bytes = data.get(o..).unwrap_or_default();
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_names_from_keys() {
        assert_eq!(speaker_from_key("vo_flynn_00163").as_deref(), Some("Flynn"));
        assert_eq!(
            speaker_from_key("vo_tbone_02835").as_deref(),
            Some("T-Bone")
        );
        assert_eq!(speaker_from_key("Name_Flynn"), None);
    }

    fn hub() -> Option<(Vec<u8>, Vec<u8>)> {
        let p = ["assets/files/level", "../../assets/files/level"]
            .into_iter()
            .map(|d| std::path::Path::new(d).join("Level_Hub_2014_01.bld"))
            .find(|p| p.exists())?;
        let mut m = crate::archive::ArchiveMount::new();
        m.mount(&p).ok()?;
        Some((
            m.read_file("level.bld").ok()?,
            m.read_file("ENGLISH.pak").ok()?,
        ))
    }

    #[test]
    fn rom_hub_language_pack_matches_string_infos() {
        let Some((level, pak)) = hub() else { return };
        let pack = parse_language_pack(&pak).expect("language pack");
        let text = LevelText::new(&level, &pack).expect("level text");
        assert_eq!(text.strings.len(), pack.len());
        assert_eq!(text.by_key("Name_Flynn"), Some("Flynn"));
        assert_eq!(text.by_key("Auric_YES/NO_String"), Some("Enter the store?"));
        assert!(text
            .by_key("vo_flynn_00163")
            .is_some_and(|t| t.contains("talk to Persephone")));
    }

    #[test]
    fn rom_hub_npc_conversations() {
        let Some((level, pak)) = hub() else { return };
        let text = LevelText::new(&level, &parse_language_pack(&pak).unwrap()).unwrap();
        let set = ScriptSet::parse(&level).unwrap();
        let flynn = npc_conversations(&level, &set, &text, "NPC_Flynn.ai");
        assert!(!flynn.is_empty());
        assert!(flynn
            .iter()
            .flat_map(|c| &c.lines)
            .all(|l| !l.text.is_empty()));
        assert!(flynn
            .iter()
            .flat_map(|c| &c.lines)
            .any(|l| l.speaker.as_deref() == Some("Flynn")));
        for npc in [
            "NPC_Batterson.ai",
            "NPC_Brock.ai",
            "NPC_Buzz.ai",
            "NPC_Cali.ai",
            "NPC_Flynn_Injured.ai",
            "NPC_Mags.ai",
            "NPC_TBone.ai",
            "NPC_Tessa.ai",
            "NPC_Whiskers.ai",
            "Auric.ai",
            "Persephone_2014.ai",
        ] {
            let c = npc_conversations(&level, &set, &text, npc);
            eprintln!(
                "{npc}: {}",
                c.iter()
                    .map(|c| c.lines.len().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        // Buzz's ambient set is engine-bound, so his own lines stand in, sorted by key.
        let buzz = npc_dialogue(&level, &set, &text, "NPC_Buzz.ai", "Buzz");
        assert!(buzz.len() > 5, "{buzz:?}");
        assert!(buzz
            .iter()
            .all(|c| c.lines.len() == 1 && c.lines[0].speaker.as_deref() == Some("Buzz")));
        assert!(npc_dialogue(&level, &set, &text, "NPC_Whiskers.ai", "Whiskers").is_empty());
        let tbone = npc_dialogue(&level, &set, &text, "NPC_TBone.ai", "T-Bone");
        assert_eq!(tbone[0].lines[0].speaker.as_deref(), Some("T-Bone"));
    }
}
