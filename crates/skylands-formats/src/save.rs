//! Save profile layout from `character/Init_Setup.bld` `Init_Setup_SaveGame.ai`.
//!
//! "Save Init - Define All Save Params" calls "Save Init - Define Save Param" once per
//! param (`is header param`, `save param` reference, `signed`, `fields total`,
//! `field bit size`, `limit to val`); the param joins `"all header save params"` or
//! `"all save params"`. "Save Init - Finalize Save Params and Construct SaveGame" then
//! walks each set in order: `bit index = bit ct`, `bit ct += num bits * fields total`,
//! and the savegame holds `bit ct / 32 + 1` 32-bit words.
//!
//! On disc (EU Wii `Save_Header`, `Save_Slot_1..4`, checked against real saves): the
//! words big-endian, bit `i` of the stream at word `i / 32`, bit `i % 32` from the LSB,
//! a field's bits LSB first; then FNV-1a 32 of every preceding byte, big-endian.

use crate::script::{Op, OpaqueOperand, Operand, Ref, Rhs, ScriptSet};
use std::collections::HashMap;

const DEFINE_SAVE_PARAM: &str = "Save Init - Define Save Param";
const DEFAULT_MAPPING: &str = "Save Init - Add Save Param Default Mapping";

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
    /// "Save Init - Add Save Param Default Mapping": (field index, value) for a fresh
    /// save; index -1 sets every field.
    pub defaults: Vec<(i32, i64)>,
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
    /// `"level count"` set (`global.bld` `$72e`), which sizes every per-level param
    /// (`level flags`, `collection - story tablets`...). Field order is not this list's
    /// but `"adventure data"` after "Global - Order Adventure Data" (story order).
    pub levels: Vec<String>,
}

/// Words of `Save_Header` (380 bytes on disc) and of each `Save_Slot_N` (1004 bytes),
/// measured on EU Wii saves. The toy-table counts that would give them are not resolved.
pub const HEADER_WORDS: usize = 94;
pub const SLOT_WORDS: usize = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// Not a whole number of words plus the checksum.
    Size(usize),
    /// Stored FNV-1a does not match the data.
    Checksum { stored: u32, computed: u32 },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Size(n) => write!(f, "save file size {n} is not 4 * words + 4"),
            SaveError::Checksum { stored, computed } => {
                write!(
                    f,
                    "save checksum {stored:08x}, data hashes to {computed:08x}"
                )
            }
        }
    }
}

impl std::error::Error for SaveError {}

/// FNV-1a 32 (the save trailer).
pub fn fnv1a32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |h, &b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    })
}

/// One save file: the bitstream words, without the checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    pub words: Vec<u32>,
}

impl SaveFile {
    pub fn zeroed(words: usize) -> Self {
        Self {
            words: vec![0; words],
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, SaveError> {
        if bytes.len() < 4 || !bytes.len().is_multiple_of(4) {
            return Err(SaveError::Size(bytes.len()));
        }
        let (data, tail) = bytes.split_at(bytes.len() - 4);
        let stored = u32::from_be_bytes(tail.try_into().unwrap());
        let computed = fnv1a32(data);
        if stored != computed {
            return Err(SaveError::Checksum { stored, computed });
        }
        Ok(Self {
            words: data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_be_bytes(*c))
                .collect(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self.words.iter().flat_map(|w| w.to_be_bytes()).collect();
        let sum = fnv1a32(&out);
        out.extend_from_slice(&sum.to_be_bytes());
        out
    }

    fn bit(&self, i: u32) -> Option<bool> {
        let w = self.words.get((i / 32) as usize)?;
        Some((w >> (i % 32)) & 1 == 1)
    }

    /// `width` bits from bit `at`, LSB first. `None` past the end.
    pub fn get_bits(&self, at: u32, width: u32) -> Option<u64> {
        (0..width).try_fold(0u64, |v, i| Some(v | (self.bit(at + i)? as u64) << i))
    }

    /// Writes the low `width` bits of `value` from bit `at`. `false` past the end.
    pub fn set_bits(&mut self, at: u32, width: u32, value: u64) -> bool {
        if (at + width).div_ceil(32) as usize > self.words.len() {
            return false;
        }
        for i in 0..width {
            let b = at + i;
            let w = &mut self.words[(b / 32) as usize];
            let mask = 1u32 << (b % 32);
            if (value >> i) & 1 == 1 {
                *w |= mask;
            } else {
                *w &= !mask;
            }
        }
        true
    }
}

/// A param placed in its bitstream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SaveField<'a> {
    pub def: &'a SaveParamDef,
    pub bit: u32,
    pub fields: u32,
}

impl SaveField<'_> {
    pub fn get(&self, file: &SaveFile, index: u32) -> Option<i64> {
        if index >= self.fields {
            return None;
        }
        let bits = self.def.bits;
        let raw = file.get_bits(self.bit + index * bits, bits)?;
        Some(
            if self.def.signed && bits < 64 && raw >> (bits - 1) & 1 == 1 {
                raw as i64 - (1i64 << bits)
            } else {
                raw as i64
            },
        )
    }

    /// Stores `value`, capped at `limit to val` when the param has one.
    pub fn set(&self, file: &mut SaveFile, index: u32, value: i64) -> bool {
        if index >= self.fields {
            return false;
        }
        let value = if self.def.limit > 0 {
            value.min(self.def.limit as i64)
        } else {
            value
        };
        file.set_bits(
            self.bit + index * self.def.bits,
            self.def.bits,
            value as u64,
        )
    }
}

/// `count` field of a set (`$39b1ddf4`, FNV-1a of `"count"`).
const COUNT_FIELD: u32 = 0x39b1_ddf4;

/// Straight-line evaluation of the save-definition macros: tracks literal and computed
/// locals in execution order, recurses into helper macros, and records every
/// "Define Save Param" call. Loop bodies are not unrolled: what they write is unknown.
struct Walk<'a> {
    set: &'a ScriptSet,
    global: Option<&'a ScriptSet>,
    /// `None`: written with a value this walk can't know (loop bodies, engine reads).
    env: HashMap<Vec<Ref>, Option<f32>>,
    params: Vec<SaveParamDef>,
    levels: Vec<String>,
    depth: u32,
}

impl Walk<'_> {
    /// Name of the `OpDefineMacro` at `offset` (names repeat across macro copies, so
    /// calls are matched by name).
    fn macro_name(&self, offset: usize) -> Option<&str> {
        self.set
            .scripts
            .iter()
            .flat_map(|s| &s.ops)
            .find_map(|i| match &i.op {
                Op::DefineMacro { name } if i.offset == offset => Some(name.as_str()),
                _ => None,
            })
    }

    fn run_macro(&mut self, si: usize, oi: usize) {
        if self.depth > 16 {
            return;
        }
        self.depth += 1;
        let ops = &self.set.scripts[si].ops;
        let end = (oi + 1 + ops[oi].br as usize).min(ops.len());
        let mut at = oi + 1;
        while at < end {
            let ins = &ops[at];
            match &ins.op {
                Op::SetValue { lhs, rhs } => {
                    if let Some(members) = self.set_members(rhs) {
                        if let [Ref::Obj(v)] = lhs[..] {
                            if self.set.var_name(v) == "level count" {
                                self.levels = members.to_vec();
                            }
                        }
                    }
                    let v = self.eval(rhs);
                    self.env.insert(lhs.clone(), v);
                }
                Op::IncValue { lhs } | Op::DecValue { lhs } => {
                    self.env.insert(lhs.clone(), None);
                }
                Op::LoopValue { .. } => {
                    for i in &ops[at + 1..(at + 1 + ins.br as usize).min(end)] {
                        if let Op::SetValue { lhs, .. }
                        | Op::IncValue { lhs }
                        | Op::DecValue { lhs } = &i.op
                        {
                            self.env.insert(lhs.clone(), None);
                        }
                    }
                    at += ins.br as usize;
                }
                Op::UseMacro { target: Some(t) }
                    if self.macro_name(*t) == Some(DEFINE_SAVE_PARAM) =>
                {
                    if let Some(p) = self.call(&ops[at + 1..]) {
                        self.params.push(p);
                    }
                }
                Op::UseMacro { target: Some(t) }
                    if self.macro_name(*t) == Some(DEFAULT_MAPPING) =>
                {
                    self.default_mapping(&ops[at + 1..]);
                }
                Op::UseMacro { target: Some(t) } => {
                    let target = self.set.scripts.iter().enumerate().find_map(|(s, sc)| {
                        sc.ops
                            .iter()
                            .position(|i| i.offset == *t && matches!(i.op, Op::DefineMacro { .. }))
                            .map(|o| (s, o))
                    });
                    if let Some((s, o)) = target {
                        self.run_macro(s, o);
                    }
                }
                _ => {}
            }
            at += 1;
        }
        self.depth -= 1;
    }

    /// Static set behind `<set>.count`: a `global.bld` export or a set of this file.
    fn set_of(&self, path: &[Ref]) -> Option<&[String]> {
        let [prefix @ .., Ref::Handle(field)] = path else {
            return None;
        };
        if self.set.handle_ids.get(*field as usize) != Some(&COUNT_FIELD) {
            return None;
        }
        match prefix {
            [Ref::Handle(h)] => self.set.global_set(*h, self.global?),
            [Ref::Obj(o)] => self.set.sets.get(o).map(Vec::as_slice),
            _ => None,
        }
    }

    fn set_members(&self, rhs: &Rhs) -> Option<&[String]> {
        match (&rhs.a, &rhs.b) {
            (Operand::Read { path, .. }, None) => self.set_of(path),
            _ => None,
        }
    }

    fn read(&self, path: &[Ref]) -> Option<f32> {
        if let Some(v) = self.env.get(path) {
            return *v;
        }
        if let Some(members) = self.set_of(path) {
            return Some(members.len() as f32);
        }
        let name = |r: &Ref| match r {
            Ref::Obj(o) => self.set.var_name(*o),
            Ref::Handle(_) => "",
        };
        // `$648."<param>"."fields total"`: another param's count.
        if let [.., param, last] = path {
            if name(last) == "fields total" {
                let p = self.params.iter().find(|p| p.name == name(param))?;
                return match p.fields {
                    SaveCount::Fixed(n) => Some(n as f32),
                    SaveCount::Runtime(_) => None,
                };
            }
        }
        match path {
            [Ref::Obj(o)] => self.set.vars.get(o)?.init.map(|v| v.as_f32()),
            [Ref::Handle(h)] => self
                .set
                .global_handle(*h, self.global?)?
                .init
                .map(|v| v.as_f32()),
            _ => None,
        }
    }

    fn operand(&self, o: &Operand) -> Option<f32> {
        match o {
            Operand::Lit(v) => Some(v.as_f32()),
            Operand::Read { path, .. } => self.read(path),
            Operand::Expr(r) => self.eval(r),
        }
    }

    fn eval(&self, rhs: &Rhs) -> Option<f32> {
        if rhs.func != 0 {
            return None;
        }
        let a = self.operand(&rhs.a)?;
        match (&rhs.b, rhs.arith) {
            (Some(b), op) if op >= 0 => {
                let b = self.operand(b)?;
                Some(match op {
                    0 => a + b,
                    1 => a - b,
                    2 => a * b,
                    3 if b != 0.0 => a / b,
                    _ => return None,
                })
            }
            _ => Some(a),
        }
    }

    /// Name of the param a `"save param"`-style reference argument points at.
    fn ref_name(&self, offset: usize) -> Option<String> {
        let set = self.set;
        set.opaque_operands
            .get(&offset)
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
            })
    }

    /// "Save Init - Add Save Param Default Mapping" call site: `ops` starts at its interface.
    fn default_mapping(&mut self, ops: &[crate::script::Instr]) {
        let Some(iface) = ops.first().filter(|i| i.op == Op::Interface) else {
            return;
        };
        let (mut name, mut index, mut val) = (None, None, None);
        for ins in ops.iter().skip(1).take(iface.br as usize) {
            let Op::Param { var, rhs, .. } = &ins.op else {
                continue;
            };
            match (self.set.var_name(*var), rhs) {
                ("input save param", _) => name = self.ref_name(ins.offset),
                ("field index", Some(r)) => index = self.eval(r),
                ("field val", Some(r)) => val = self.eval(r),
                _ => {}
            }
        }
        // Calls sit in the callback of the param's own define and pass its `"save param"`.
        if let (Some(name), Some(index), Some(val)) = (name, index, val) {
            let param = match name.as_str() {
                "save param" => self.params.last_mut(),
                n => self.params.iter_mut().rev().find(|p| p.name == n),
            };
            if let Some(p) = param {
                p.defaults.push((index as i32, val as i64));
            }
        }
    }

    /// One "Define Save Param" call site: `ops` starts at its interface.
    fn call(&self, ops: &[crate::script::Instr]) -> Option<SaveParamDef> {
        let set = self.set;
        let iface = ops.first().filter(|i| i.op == Op::Interface)?;
        let mut def = SaveParamDef {
            name: String::new(),
            header: false,
            signed: false,
            fields: SaveCount::Fixed(1),
            bits: 32,
            limit: 0,
            defaults: Vec::new(),
        };
        for ins in ops.iter().skip(1).take(iface.br as usize) {
            let Op::Param { var, rhs, .. } = &ins.op else {
                continue;
            };
            let name = set.var_name(*var);
            if name == "save param" {
                def.name = self.ref_name(ins.offset)?;
                continue;
            }
            let Some(rhs) = rhs else { continue };
            let value = self.eval(rhs);
            match (name, value) {
                ("is header param", Some(v)) => def.header = v != 0.0,
                ("signed", Some(v)) => def.signed = v != 0.0,
                ("fields total", Some(v)) => def.fields = SaveCount::Fixed(v as u32),
                ("fields total", None) => {
                    let text = match (&rhs.a, &rhs.b) {
                        (Operand::Read { path, .. }, None) => set.path_str(path),
                        _ => String::new(),
                    };
                    def.fields = SaveCount::Runtime(text.trim_matches('"').to_string());
                }
                ("field bit size", Some(v)) => def.bits = v as u32,
                ("limit to val", Some(v)) => def.limit = v as i32,
                _ => {}
            }
        }
        Some(def)
    }
}

impl SaveSchema {
    /// `global` resolves `<set>.count` counts from static `global.bld` sets (level and
    /// save slot lists) and global values; without it they stay `SaveCount::Runtime`.
    /// Params come in execution order of "Save Init - Define All Save Params", nested
    /// helper macros included (the toy collection is defined before the header ends).
    pub fn from_scripts(init_setup: &ScriptSet, global: Option<&ScriptSet>) -> Option<Self> {
        let (si, oi) = init_setup.find_macro("Save Init - Define All Save Params")?;
        let mut walk = Walk {
            set: init_setup,
            global,
            env: HashMap::new(),
            params: Vec::new(),
            levels: Vec::new(),
            depth: 0,
        };
        walk.run_macro(si, oi);
        let params = walk
            .params
            .into_iter()
            // `^` names are dangling references into structures of other files (toy and
            // figure data); the one `^mobile` header param is only defined off-Wii.
            .filter(|p| !p.name.starts_with('^'))
            .collect();
        Some(Self {
            params,
            levels: walk.levels,
        })
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

    /// `name` in the header (`header`) or slot bitstream, when its position is known.
    pub fn field(&self, header: bool, name: &str) -> Option<SaveField<'_>> {
        let none = |_: &str| None;
        let (def, bit) = self
            .layout(header, &none)
            .into_iter()
            .find(|(p, _)| p.name == name)?;
        let SaveCount::Fixed(fields) = def.fields else {
            return None;
        };
        Some(SaveField {
            def,
            bit: bit?,
            fields,
        })
    }

    /// A fresh header or slot: zeroes plus every default mapping with a known position
    /// (an empty EU slot is exactly this).
    pub fn new_file(&self, header: bool) -> SaveFile {
        let words = if header { HEADER_WORDS } else { SLOT_WORDS };
        let mut file = SaveFile::zeroed(words);
        let none = |_: &str| None;
        for (def, bit) in self.layout(header, &none) {
            let (Some(_), SaveCount::Fixed(_)) = (bit, &def.fields) else {
                continue;
            };
            let field = self.field(header, &def.name).unwrap();
            for &(index, val) in &def.defaults {
                if index < 0 {
                    (0..field.fields).for_each(|i| {
                        field.set(&mut file, i, val);
                    });
                } else {
                    field.set(&mut file, index as u32, val);
                }
            }
        }
        file
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
            defaults: Vec::new(),
        };
        let schema = SaveSchema {
            params: vec![
                p("version", true, SaveCount::Fixed(1), 8),
                p("levels", false, SaveCount::Runtime("level count".into()), 4),
                p("flags", false, SaveCount::Fixed(180), 1),
                p("slots", true, SaveCount::Fixed(4), 8),
            ],
            levels: Vec::new(),
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
    fn save_file_bits_and_checksum() {
        let mut f = SaveFile::zeroed(3);
        assert!(f.set_bits(30, 4, 0b1011));
        assert_eq!(f.words, [0xc000_0000, 0b10, 0]);
        assert_eq!(f.get_bits(30, 4), Some(0b1011));
        assert!(!f.set_bits(90, 8, 1));
        let bytes = f.to_bytes();
        assert_eq!(bytes.len(), 16);
        assert_eq!(SaveFile::parse(&bytes), Ok(f.clone()));
        let mut bad = bytes.clone();
        bad[0] ^= 1;
        assert!(matches!(
            SaveFile::parse(&bad),
            Err(SaveError::Checksum { .. })
        ));
        assert_eq!(SaveFile::parse(&bytes[..15]), Err(SaveError::Size(15)));
        // FNV-1a test vector.
        assert_eq!(fnv1a32(b"a"), 0xe40c_292c);

        let def = SaveParamDef {
            name: "pct".into(),
            header: true,
            signed: true,
            fields: SaveCount::Fixed(2),
            bits: 8,
            limit: 0,
            defaults: Vec::new(),
        };
        let field = SaveField {
            def: &def,
            bit: 4,
            fields: 2,
        };
        assert!(field.set(&mut f, 1, -1));
        assert_eq!(field.get(&f, 1), Some(-1));
        assert_eq!(f.get_bits(12, 8), Some(0xff));
        assert_eq!(field.get(&f, 2), None);
    }

    /// Local-only: the user's EU Wii saves (`Save_Header`, `Save_Slot_1` after chapter 1,
    /// empty `Save_Slot_2..4`) at the repository root, gitignored.
    fn real_save(name: &str) -> Option<Vec<u8>> {
        [".", "../.."]
            .iter()
            .map(|d| std::path::Path::new(d).join(name))
            .find(|p| p.exists())
            .and_then(|p| std::fs::read(p).ok())
    }

    fn rom_schema() -> Option<SaveSchema> {
        let read = |dir: &str, name: &str| {
            ["assets/files", "../../assets/files"]
                .iter()
                .map(|d| {
                    std::path::Path::new(d)
                        .join(dir)
                        .join(format!("{name}.bld"))
                })
                .find(|p| p.exists())
                .and_then(|p| {
                    let mut m = crate::archive::ArchiveMount::new();
                    m.mount(&p).ok()?;
                    ScriptSet::parse(&m.read_file("level.bld").ok()?)
                })
        };
        SaveSchema::from_scripts(
            &read("character", "Init_Setup")?,
            read("permanent", "global").as_ref(),
        )
    }

    #[test]
    fn real_saves_round_trip() {
        let (Some(schema), Some(header), Some(slot1), Some(empty)) = (
            rom_schema(),
            real_save("Save_Header"),
            real_save("Save_Slot_1"),
            real_save("Save_Slot_2"),
        ) else {
            return;
        };
        let h = SaveFile::parse(&header).expect("header checksum");
        let s1 = SaveFile::parse(&slot1).expect("slot 1 checksum");
        assert_eq!((h.words.len(), s1.words.len()), (HEADER_WORDS, SLOT_WORDS));
        assert_eq!(h.to_bytes(), header);
        assert_eq!(s1.to_bytes(), slot1);

        // A fresh slot from the script defaults is byte-identical to an empty one.
        assert_eq!(schema.new_file(false).to_bytes(), empty);

        let get = |file: &SaveFile, header, name: &str, i| {
            schema.field(header, name).unwrap().get(file, i).unwrap()
        };
        assert_eq!(get(&h, true, "header - options sound vols", 3), 50);
        assert_eq!(get(&h, true, "header - slot complete pct", 0), 1);
        assert_eq!(get(&h, true, "header - slot complete pct", 1), -1);
        assert_eq!(get(&h, true, "portalmaster - total stars collected", 0), 1);
        assert_eq!(get(&s1, false, "portalmaster - rank", 0), 1);
        assert_eq!(get(&s1, false, "portalmaster - stars", 0), 1);
        assert_eq!(get(&s1, false, "level flags - objectives", 0), 3);
        assert_eq!(get(&s1, false, "level flags", 0), 229_377);

        // Editing one field and writing back changes only that field and the checksum.
        let mut edited = s1.clone();
        let rank = schema.field(false, "portalmaster - rank").unwrap();
        assert!(rank.set(&mut edited, 0, 99));
        assert_eq!(rank.get(&edited, 0), Some(40), "capped at limit to val");
        let out = SaveFile::parse(&edited.to_bytes()).unwrap();
        let diff: Vec<usize> = (0..SLOT_WORDS)
            .filter(|&w| out.words[w] != s1.words[w])
            .collect();
        assert_eq!(diff, [(rank.bit / 32) as usize]);
    }

    #[test]
    fn rom_save_schema() {
        let read = |dir: &str, name: &str| {
            ["assets/files", "../../assets/files"]
                .iter()
                .map(|d| {
                    std::path::Path::new(d)
                        .join(dir)
                        .join(format!("{name}.bld"))
                })
                .find(|p| p.exists())
                .and_then(|p| {
                    let mut m = crate::archive::ArchiveMount::new();
                    m.mount(&p).ok()?;
                    ScriptSet::parse(&m.read_file("level.bld").ok()?)
                })
        };
        let (Some(set), Some(global)) =
            (read("character", "Init_Setup"), read("permanent", "global"))
        else {
            return;
        };
        let unresolved = SaveSchema::from_scripts(&set, None).expect("schema");
        assert!(matches!(
            &unresolved.params.iter().find(|p| p.name == "level flags").unwrap().fields,
            SaveCount::Runtime(n) if n == "level count"
        ));
        let schema = SaveSchema::from_scripts(&set, Some(&global)).expect("schema");
        assert_eq!(schema.levels.len(), 22);
        assert_eq!(schema.levels[0], "Level_300_BirdCage");
        assert_eq!(schema.levels[2], "Level_302_ChompyWurm");
        assert_eq!(schema.levels[21], "Level_327_Kaos_Exp");
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
        assert_eq!(get("level flags").fields, SaveCount::Fixed(22));
        assert_eq!(get("header - slot level IDs").fields, SaveCount::Fixed(4));
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
        // Slot fields resolve (4 slots); the toy collection (`track i`) stays unknown.
        assert_eq!(at("header - slot minions collected"), Some(144));
        assert_eq!(at("header - slot complete pct"), Some(153));
        // Execution order: the toy collection macro runs between "completion flags" and
        // "game difficulty completion flags"; its sizes (`track i`) stay unknown.
        assert_eq!(at("header - completion flags"), Some(601));
        assert_eq!(at("collection - toys"), Some(603));
        assert_eq!(at("collection - toys - items"), None);
        assert_eq!(at("header - game difficulty completion flags"), None);
    }
}
