//! Save profile layout from `character/Init_Setup.bld` `Init_Setup_SaveGame.ai`.
//!
//! "Save Init - Define All Save Params" calls "Save Init - Define Save Param" once per
//! param (`is header param`, `save param` reference, `signed`, `fields total`,
//! `field bit size`, `limit to val`); the param joins `"all header save params"` or
//! `"all save params"`. "Save Init - Finalize Save Params and Construct SaveGame" then
//! walks each set in order: `bit index = bit ct`, `bit ct += num bits * fields total`,
//! and the savegame holds `bit ct / 32 + 1` 32-bit words.

use crate::script::{Op, OpaqueOperand, Operand, Ref, ScriptSet, Value};
use std::collections::HashMap;

const DEFINE_SAVE_PARAM: &str = "Save Init - Define Save Param";

/// `fields total` of a param: a literal, or a value the engine or a toy table supplies
/// at boot (`"level count"` = `$72e.count`, `"track i"` from the toy load lists).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveCount {
    Fixed(u32),
    Runtime(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaveParamDef {
    /// Name of the referenced `System - Save - Save Param` (`"header - savegame version"`).
    pub name: String,
    pub header: bool,
    pub signed: bool,
    pub fields: SaveCount,
    /// `field bit size` (`num bits`) per field.
    pub bits: u32,
    /// `limit to val` (0 = none).
    pub limit: i32,
}

impl SaveParamDef {
    pub fn total_bits(&self, counts: &dyn Fn(&str) -> Option<u32>) -> Option<u32> {
        let fields = match &self.fields {
            SaveCount::Fixed(n) => *n,
            SaveCount::Runtime(name) => counts(name)?,
        };
        Some(fields * self.bits)
    }
}

/// Every save param in definition order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SaveSchema {
    pub params: Vec<SaveParamDef>,
}

impl SaveSchema {
    pub fn from_scripts(init_setup: &ScriptSet) -> Option<Self> {
        let script = init_setup.script("Init_Setup_SaveGame.ai")?;
        let define_at = script.ops.iter().find_map(|i| match &i.op {
            Op::DefineMacro { name } if name == DEFINE_SAVE_PARAM => Some(i.offset),
            _ => None,
        })?;
        // Locals only ever set to one literal (`"minion count" = 160`). `"track i"` starts at
        // 0 and is summed in a loop, so it stays a runtime count.
        let mut locals: HashMap<usize, Option<Value>> = HashMap::new();
        for ins in &script.ops {
            match &ins.op {
                Op::SetValue { lhs, rhs } => {
                    if let [Ref::Obj(v)] = lhs[..] {
                        let lit = match (&rhs.a, &rhs.b) {
                            (Operand::Lit(val), None) => Some(*val),
                            _ => None,
                        };
                        let e = locals.entry(v).or_insert(lit);
                        if *e != lit {
                            *e = None;
                        }
                    }
                }
                Op::IncValue { lhs } | Op::DecValue { lhs } => {
                    if let [Ref::Obj(v)] = lhs[..] {
                        locals.insert(v, None);
                    }
                }
                _ => {}
            }
        }
        let locals: HashMap<usize, Value> = locals
            .into_iter()
            .filter_map(|(k, v)| Some((k, v?)))
            .collect();
        let params = script
            .ops
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                i.op == Op::UseMacro {
                    target: Some(define_at),
                }
            })
            .filter_map(|(at, _)| Self::call(init_setup, &script.ops[at + 1..], &locals))
            // `^` names are dangling references into structures of other files (toy and
            // figure data, mobile), not params of this savegame.
            .filter(|p| !p.name.starts_with('^'))
            .collect();
        Some(Self { params })
    }

    /// One "Define Save Param" call site: `ops` starts at its interface.
    fn call(
        set: &ScriptSet,
        ops: &[crate::script::Instr],
        locals: &HashMap<usize, Value>,
    ) -> Option<SaveParamDef> {
        let iface = ops.first().filter(|i| i.op == Op::Interface)?;
        let mut def = SaveParamDef {
            name: String::new(),
            header: false,
            signed: false,
            fields: SaveCount::Fixed(1),
            bits: 32,
            limit: 0,
        };
        for ins in ops.iter().skip(1).take(iface.br as usize) {
            let Op::Param { var, rhs, .. } = &ins.op else {
                continue;
            };
            let name = set.var_name(*var);
            if name == "save param" {
                def.name = set
                    .opaque_operands
                    .get(&ins.offset)
                    .into_iter()
                    .flatten()
                    .find_map(|(_, o)| match o {
                        OpaqueOperand::Path { stack, path } if stack == "RHSReferenceStack" => {
                            path.last().map(|r| match r {
                                Ref::Obj(o) => set.var_name(*o).to_string(),
                                Ref::Handle(_) => set.path_str(&[*r]),
                            })
                        }
                        _ => None,
                    })?;
                continue;
            }
            let Some(rhs) = rhs else { continue };
            let value = match (&rhs.a, &rhs.b) {
                (Operand::Lit(v), None) => Ok(*v),
                (Operand::Read { path, .. }, None) => match path[..] {
                    [Ref::Obj(v)] if locals.contains_key(&v) => Ok(locals[&v]),
                    _ => Err(set.path_str(path).trim_matches('"').to_string()),
                },
                _ => Err(String::new()),
            };
            match (name, value) {
                ("is header param", Ok(v)) => def.header = v.as_f32() != 0.0,
                ("signed", Ok(v)) => def.signed = v.as_f32() != 0.0,
                ("fields total", Ok(v)) => def.fields = SaveCount::Fixed(v.as_f32() as u32),
                ("fields total", Err(n)) => def.fields = SaveCount::Runtime(n),
                ("field bit size", Ok(v)) => def.bits = v.as_f32() as u32,
                ("limit to val", Ok(v)) => def.limit = v.as_f32() as i32,
                _ => {}
            }
        }
        Some(def)
    }

    /// Bit index of each param of the header (`header`) or body bitstream, as Finalize
    /// assigns them. `None` from the first param whose size depends on an unknown count.
    pub fn layout(
        &self,
        header: bool,
        counts: &dyn Fn(&str) -> Option<u32>,
    ) -> Vec<(&SaveParamDef, Option<u32>)> {
        let mut bit = Some(0u32);
        self.params
            .iter()
            .filter(|p| p.header == header)
            .map(|p| {
                let at = bit;
                bit = bit.zip(p.total_bits(counts)).map(|(b, n)| b + n);
                (p, at)
            })
            .collect()
    }

    /// 32-bit words of the header or body: `bit ct / 32 + 1`.
    pub fn words(&self, header: bool, counts: &dyn Fn(&str) -> Option<u32>) -> Option<u32> {
        self.params
            .iter()
            .filter(|p| p.header == header)
            .try_fold(0u32, |b, p| Some(b + p.total_bits(counts)?))
            .map(|bits| bits / 32 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_follows_finalize() {
        let p = |name: &str, header, fields, bits| SaveParamDef {
            name: name.into(),
            header,
            signed: false,
            fields,
            bits,
            limit: 0,
        };
        let schema = SaveSchema {
            params: vec![
                p("version", true, SaveCount::Fixed(1), 8),
                p("levels", false, SaveCount::Runtime("level count".into()), 4),
                p("flags", false, SaveCount::Fixed(180), 1),
                p("slots", true, SaveCount::Fixed(4), 8),
            ],
        };
        let none = |_: &str| None;
        let header: Vec<_> = schema
            .layout(true, &none)
            .iter()
            .map(|(p, b)| (p.name.as_str(), *b))
            .collect();
        assert_eq!(header, [("version", Some(0)), ("slots", Some(8))]);
        assert_eq!(schema.words(true, &none), Some(2));
        // Body positions after an unknown count are unknown until it is supplied.
        assert_eq!(schema.layout(false, &none)[1].1, None);
        let counts = |n: &str| (n == "level count").then_some(20);
        assert_eq!(schema.layout(false, &counts)[1].1, Some(80));
        assert_eq!(schema.words(false, &counts), Some(260 / 32 + 1));
    }

    #[test]
    fn rom_save_schema() {
        let Some(bytes) = ["assets/files/character", "../../assets/files/character"]
            .iter()
            .map(|d| std::path::Path::new(d).join("Init_Setup.bld"))
            .find(|p| p.exists())
            .and_then(|p| {
                let mut m = crate::archive::ArchiveMount::new();
                m.mount(&p).ok()?;
                m.read_file("level.bld").ok()
            })
        else {
            return;
        };
        let set = ScriptSet::parse(&bytes).unwrap();
        let schema = SaveSchema::from_scripts(&set).expect("schema");
        let get = |n: &str| schema.params.iter().find(|p| p.name == n).unwrap();
        let first = &schema.params[0];
        assert_eq!(first.name, "header - savegame version");
        assert!(first.header);
        assert_eq!((first.fields.clone(), first.bits), (SaveCount::Fixed(1), 8));
        assert!(schema.params.iter().all(|p| !p.name.starts_with('^')));

        let rank = get("portalmaster - rank");
        assert_eq!((rank.header, rank.bits, rank.limit), (false, 6, 40));
        assert_eq!(get("portalmaster - stars").fields, SaveCount::Fixed(180));
        assert_eq!(get("village generic vars").fields, SaveCount::Fixed(100));
        assert!(get("village generic vars").signed);
        assert_eq!(
            get("level flags").fields,
            SaveCount::Runtime("level count".into())
        );
        assert_eq!(
            get("collection - toys").fields,
            SaveCount::Runtime("track i".into())
        );
        assert!(matches!(
            get("collection - toys - items").fields,
            SaveCount::Runtime(_)
        ));

        // Header bit indices up to the first per-slot field.
        let none = |_: &str| None;
        let header = schema.layout(true, &none);
        let at = |n: &str| header.iter().find(|(p, _)| p.name == n).unwrap().1;
        assert_eq!(at("header - expansion unlocks"), Some(8));
        assert_eq!(at("header - options sound vols"), Some(12));
        assert_eq!(at("portalmaster ID"), Some(48));
        assert_eq!(at("header - slot level IDs"), Some(112));
        assert_eq!(at("header - slot minions collected"), None);
    }
}
