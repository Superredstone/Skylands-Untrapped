//! Alchemy visual-script (`tfbScriptInfo` / `OpCodeList`) decoder and a small evaluator for
//! pure value logic (macros, set/check value, arithmetic). Layout notes: `agent/SCRIPT_VM.md`.

use crate::igb::{IgbEndian, IgbModel, IgzFile};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Script-visible number. Bools are `Int` 0/1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
}

impl Value {
    pub fn as_f32(self) -> f32 {
        match self {
            Self::Int(v) => v as f32,
            Self::Float(v) => v,
        }
    }
    fn bits(self) -> i32 {
        match self {
            Self::Int(v) => v,
            Self::Float(v) => v as i32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumKind {
    Int,
    Float,
    Other,
}

/// One step of a value path: a script object (by file offset) or an engine handle
/// (`EXID` index; name hash in `ScriptSet::handle_hash`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ref {
    Obj(usize),
    Handle(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Lit(Value),
    Read { path: Vec<Ref>, kind: NumKind },
    Expr(Box<Rhs>),
}

/// `ValueRHSVariant`: `a <op> b`, then `_resultFunc`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rhs {
    pub a: Operand,
    /// `ArithOp`: -1 none, 0 add, 1 sub, 2 mult, 3 div.
    pub arith: i32,
    pub b: Option<Operand>,
    pub func: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    SetValue {
        lhs: Vec<Ref>,
        rhs: Rhs,
    },
    IncValue {
        lhs: Vec<Ref>,
    },
    DecValue {
        lhs: Vec<Ref>,
    },
    /// `RelOp`: 0 LE, 1 EQ, 2 GE, 3 LT, 4 GT, 5 NE, 6 BEQ, 7 BNE.
    CheckValue {
        lhs: Vec<Ref>,
        rel: i32,
        rhs: Rhs,
    },
    /// `OpCheckReference` / `OpCheckMembership`: needs live objects, so the evaluator
    /// treats `NE` (reference set) as true and everything else as false.
    CheckRef {
        rel: i32,
        desc: String,
    },
    /// Unconditional `pc += br` (else / skip blocks).
    Jump,
    /// `OpFlowBehavior` "flow end": return from the current macro.
    End,
    LoopValue {
        count: Rhs,
    },
    DefineMacro {
        name: String,
    },
    /// `OpMacroInterface`: the next `br` ops are the macro's parameters.
    Interface,
    UseMacro {
        target: Option<usize>,
    },
    /// Macro parameter declaration (interface) or argument (call site).
    Param {
        var: usize,
        rhs: Option<Rhs>,
        macro_def: Option<usize>,
    },
    CreateVariable {
        var: usize,
    },
    /// `OpSlideValue`: move `lhs` to `target` over `seconds` (latent; owns a
    /// completion block of `br` ops).
    SlideValue {
        lhs: Vec<Ref>,
        target: Rhs,
        seconds: Option<Rhs>,
    },
    /// `OpDisplace`: move the object at `target` by `len` along heading/pitch (degrees).
    Displace {
        target: Vec<Ref>,
        mode: i32,
        len: Option<Rhs>,
        heading: Option<Rhs>,
        pitch: Option<Rhs>,
    },
    /// `OpRemove`: despawn the object at `target`.
    Remove {
        target: Vec<Ref>,
    },
    /// Event blocks of a script (`OpPreScript` / `OpStartUp` / `OpShutDown`).
    PreScript,
    StartUp,
    ShutDown,
    /// Anything else (spawn, movement, sound...): skipped by the evaluator.
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instr {
    pub offset: usize,
    pub flags: u16,
    pub br: u16,
    pub op: Op,
}

#[derive(Debug, Clone)]
pub struct Script {
    pub name: String,
    pub ops: Vec<Instr>,
}

/// Static type of a variable object.
#[derive(Debug, Clone, PartialEq)]
pub struct VarInfo {
    pub name: String,
    pub type_name: String,
    /// `ValueInfo._value` initial value.
    pub init: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptSet {
    pub scripts: Vec<Script>,
    pub vars: HashMap<usize, VarInfo>,
    /// `EXID` index -> first word: export id when `handle_ns` is `global.bld`, else
    /// FNV-1a of the lower-case engine name.
    pub handle_ids: Vec<u32>,
    /// `EXID` index -> FNV-1a of the source file (`global.bld`, `level.bld`, ...).
    pub handle_ns: Vec<u32>,
    /// `TOOR` root objects; export id `n` names `roots[n - export_base]`.
    pub roots: Vec<usize>,
    /// Export ids start after the file's `EXNM` handles.
    pub export_base: usize,
    /// Operands of ops the decoder has no variant for (`OpSpawn`, `OpStartSequence`,
    /// `OpControl`...), keyed by op offset: (field offset, operand). Listing only.
    pub opaque_operands: HashMap<usize, Vec<(usize, OpaqueOperand)>>,
}

/// Ops whose operands `ScriptSet::opaque_operands` records.
const OPAQUE_LISTED: &[&str] = &[
    "OpSpawn",
    "OpStartSequence",
    "OpControl",
    "OpChangeMembership",
    "OpForEach",
    "OpFindVariable",
];

/// Field of an op without an `Op` variant, for listings.
#[derive(Debug, Clone, PartialEq)]
pub enum OpaqueOperand {
    /// `ValueStack` / `ReferenceStack` / `ScriptGroupStack` ... path.
    Path {
        stack: String,
        path: Vec<Ref>,
    },
    Rhs(Rhs),
    /// Any other object (`tfbActorInfo` template, sequence data...): type and `+0x08` name.
    Object {
        ty: String,
        name: String,
        offset: usize,
    },
}

/// FNV-1a over the lower-cased name, as used by `EXID` entries.
pub fn name_hash(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ b.to_ascii_lowercase() as u32).wrapping_mul(0x0100_0193)
    })
}

struct Reader<'a> {
    data: &'a [u8],
    file: &'a IgzFile,
    types: HashMap<usize, &'a str>,
}

impl Reader<'_> {
    fn u32(&self, o: usize) -> Option<u32> {
        self.data
            .get(o..o + 4)
            .map(|_| IgbModel::read_u32(self.data, o, self.file.endian))
    }
    fn u16(&self, o: usize) -> u16 {
        let b = self.data.get(o..o + 2).unwrap_or(&[0, 0]);
        match self.file.endian {
            IgbEndian::Big => u16::from_be_bytes([b[0], b[1]]),
            IgbEndian::Little => u16::from_le_bytes([b[0], b[1]]),
        }
    }
    fn decode_ptr(&self, w: u32) -> Option<usize> {
        if w == 0 || w & 0x8000_0000 != 0 {
            return None;
        }
        let s = self.file.sections.get(((w >> 27) + 1) as usize)?;
        Some(s.offset + (w & 0x07FF_FFFF) as usize)
    }
    fn ptr(&self, o: usize) -> Option<usize> {
        self.decode_ptr(self.u32(o)?)
    }
    fn ty(&self, o: usize) -> &str {
        self.types.get(&o).copied().unwrap_or("")
    }
    fn cstr(&self, o: usize) -> String {
        self.ptr(o)
            .and_then(|s| {
                let len = self.data.get(s..)?.iter().position(|&b| b == 0)?;
                std::str::from_utf8(&self.data[s..s + len]).ok()
            })
            .unwrap_or_default()
            .to_string()
    }
    fn ext(&self, o: usize) -> &str {
        self.u32(o)
            .and_then(|i| self.file.external_types.get((i & 0x7FFF_FFFF) as usize))
            .map_or("", String::as_str)
    }
    /// `igDataList` (`_count` +0x08, `_data` +0x14) of raw words.
    fn list(&self, o: usize) -> Vec<u32> {
        let n = self.u32(o + 0x08).unwrap_or(0).min(4096) as usize;
        let Some(arr) = self.ptr(o + 0x14) else {
            return Vec::new();
        };
        (0..n).filter_map(|k| self.u32(arr + k * 4)).collect()
    }
    fn path(&self, stack: usize) -> Vec<Ref> {
        self.list(stack)
            .into_iter()
            .filter_map(|w| {
                if w & 0x8000_0000 != 0 {
                    Some(Ref::Handle(w & 0x7FFF_FFFF))
                } else {
                    self.decode_ptr(w).map(Ref::Obj)
                }
            })
            .collect()
    }
    fn kind(&self, o: usize) -> NumKind {
        match self.ext(o) {
            "IntMeasurement" | "EnumMeasurement" | "ButtonMeasurement" => NumKind::Int,
            "FloatMeasurement" => NumKind::Float,
            _ => NumKind::Other,
        }
    }
    fn operand(&self, o: usize, depth: u32) -> Option<Operand> {
        if depth > 32 {
            return None;
        }
        match self.ty(o) {
            "ValueRHSVariant" => Some(Operand::Expr(Box::new(self.rhs(o, depth + 1)?))),
            "RHSValueStack" => {
                let kind = self.kind(o + 0x20);
                let path = self.path(o);
                if path.is_empty() {
                    let raw = self.u32(o + 0x24)?;
                    Some(Operand::Lit(match kind {
                        NumKind::Float => Value::Float(f32::from_bits(raw)),
                        _ => Value::Int(raw as i32),
                    }))
                } else {
                    Some(Operand::Read { path, kind })
                }
            }
            _ => None,
        }
    }
    /// `_RHS` target: a `ValueRHSVariant` or a bare `RHSValueStack`.
    /// Fields of an op from `+0x1c` up to the next object (at most `0x60` bytes).
    fn opaque_operands(&self, o: usize) -> Vec<(usize, OpaqueOperand)> {
        let mut out = Vec::new();
        for f in (0x1c..0x7c).step_by(4) {
            if f > 0x1c && self.types.contains_key(&(o + f)) {
                break;
            }
            let Some(p) = self.ptr(o + f) else { continue };
            let ty = self.ty(p);
            let operand = match ty {
                "" => continue,
                "ValueRHSVariant" => match self.rhs(p, 0) {
                    Some(r) => OpaqueOperand::Rhs(r),
                    None => continue,
                },
                t if t.ends_with("Stack") => OpaqueOperand::Path {
                    stack: t.into(),
                    path: self.path(p),
                },
                // Spawn templates: `tfbActorInfo` name and `AbstractPlacement._ID` (`+0x20`,
                // the attach node index when parented to a rig).
                "AbstractPlacementList" => OpaqueOperand::Object {
                    ty: "AbstractPlacementList".into(),
                    name: self
                        .list(p)
                        .into_iter()
                        .filter_map(|w| self.decode_ptr(w))
                        .map(|a| {
                            format!(
                                "{}#{}",
                                self.cstr(a + 0x08),
                                self.u32(a + 0x20).unwrap_or(0)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(","),
                    offset: p,
                },
                t => OpaqueOperand::Object {
                    ty: t.into(),
                    name: self.cstr(p + 0x08),
                    offset: p,
                },
            };
            out.push((f, operand));
        }
        out
    }

    fn rhs_at(&self, field: usize) -> Option<Rhs> {
        let o = self.ptr(field)?;
        match self.ty(o) {
            "ValueRHSVariant" => self.rhs(o, 0),
            _ => Some(Rhs {
                a: self.operand(o, 0)?,
                arith: -1,
                b: None,
                func: 0,
            }),
        }
    }
    fn rhs(&self, o: usize, depth: u32) -> Option<Rhs> {
        Some(Rhs {
            a: self.operand(self.ptr(o + 0x08)?, depth)?,
            arith: self.u32(o + 0x0c)? as i32,
            b: self.ptr(o + 0x10).and_then(|p| self.operand(p, depth)),
            func: self.u32(o + 0x14).unwrap_or(0),
        })
    }
    fn lhs(&self, field: usize) -> Vec<Ref> {
        self.ptr(field).map(|s| self.path(s)).unwrap_or_default()
    }
    fn op(&self, o: usize) -> Op {
        let t = self.ty(o);
        match t {
            "OpSetValue" => match self.rhs_at(o + 0x20) {
                Some(rhs) => Op::SetValue {
                    lhs: self.lhs(o + 0x1c),
                    rhs,
                },
                None => Op::Other(t.into()),
            },
            "OpIncValue" => Op::IncValue {
                lhs: self.lhs(o + 0x1c),
            },
            "OpDecValue" => Op::DecValue {
                lhs: self.lhs(o + 0x1c),
            },
            "OpCheckValue" => match self.rhs_at(o + 0x24) {
                Some(rhs) => Op::CheckValue {
                    lhs: self.lhs(o + 0x1c),
                    rel: self.u32(o + 0x20).unwrap_or(1) as i32,
                    rhs,
                },
                None => Op::Other(t.into()),
            },
            "OpAbstractFlow" | "OpFlow" | "OpIfElse" | "OpBranch" | "OpFlowLoop" => Op::Jump,
            "OpFlowBehavior" => Op::End,
            "OpLoopValue" => match self.rhs_at(o + 0x20) {
                Some(count) => Op::LoopValue { count },
                None => Op::Other(t.into()),
            },
            "OpDefineMacro" | "OpDefineMacroSpecialization" => Op::DefineMacro {
                name: self.cstr(o + 0x08),
            },
            "OpUseMacro" => Op::UseMacro {
                target: self.ptr(o + 0x1c).and_then(|s| {
                    self.path(s).into_iter().find_map(|r| match r {
                        Ref::Obj(p) if self.ty(p).starts_with("OpDefineMacro") => Some(p),
                        _ => None,
                    })
                }),
            },
            "OpMacroParameter" => Op::Param {
                var: o,
                rhs: self.rhs_at(o + 0x30),
                macro_def: self.ptr(o + 0x38),
            },
            "OpCreateVariable" => Op::CreateVariable { var: o },
            "OpMacroInterface" => Op::Interface,
            "OpSlideValue" => match self.rhs_at(o + 0x20) {
                Some(target) => Op::SlideValue {
                    lhs: self.lhs(o + 0x1c),
                    target,
                    seconds: self.rhs_at(o + 0x24),
                },
                None => Op::Other(t.into()),
            },
            "OpDisplace" => Op::Displace {
                target: self.lhs(o + 0x1c),
                mode: self.u32(o + 0x20).unwrap_or(0) as i32,
                len: self.rhs_at(o + 0x24),
                heading: self.rhs_at(o + 0x28),
                pitch: self.rhs_at(o + 0x2c),
            },
            "OpRemove" => Op::Remove {
                target: self.lhs(o + 0x1c),
            },
            "OpPreScript" => Op::PreScript,
            "OpStartUp" => Op::StartUp,
            "OpShutDown" => Op::ShutDown,
            "OpCheckReference" | "OpCheckMembership" | "OpSetReference" => {
                let set = t == "OpSetReference";
                let p = |f: usize| {
                    self.ptr(o + f)
                        .map(|s| format!("{}{:?}", self.ty(s), self.path(s)))
                        .unwrap_or_else(|| "null".into())
                };
                let rel = self.u32(o + 0x20).unwrap_or(0) as i32;
                let desc = format!(
                    "{t} lhs={} rel={rel} rhs={}",
                    p(0x1c),
                    p(if set { 0x20 } else { 0x24 })
                );
                if set {
                    Op::Other(desc)
                } else {
                    Op::CheckRef { rel, desc }
                }
            }
            _ => Op::Other(t.into()),
        }
    }
}

impl ScriptSet {
    /// Decode every `tfbScriptInfo` in an IGZ (`level.bld`) file.
    pub fn parse(data: &[u8]) -> Option<Self> {
        Self::from_file(data, &IgbModel::scan_igz(data).ok()?)
    }

    /// `parse` over an already scanned file.
    pub fn from_file(data: &[u8], file: &IgzFile) -> Option<Self> {
        let objects = &file.objects.as_ref()?.objects;
        let r = Reader {
            data,
            file,
            types: objects
                .iter()
                .filter_map(|o| Some((o.offset, o.type_name.as_deref()?)))
                .collect(),
        };
        let mut vars = HashMap::new();
        for o in objects {
            let t = o.type_name.as_deref().unwrap_or("");
            let info = match t {
                "OpCreateVariable" | "OpMacroParameter" | "OpCreateVariableArray" => VarInfo {
                    name: r.cstr(o.offset + 0x1c),
                    type_name: t.into(),
                    init: None,
                },
                "ValueInfo" => {
                    let kind = r.kind(o.offset + 0x1c);
                    let raw = r.u32(o.offset + 0x20).unwrap_or(0);
                    VarInfo {
                        name: r.cstr(o.offset + 0x08),
                        type_name: t.into(),
                        init: Some(match kind {
                            NumKind::Float => Value::Float(f32::from_bits(raw)),
                            _ => Value::Int(raw as i32),
                        }),
                    }
                }
                "ScriptStructure" => VarInfo {
                    name: r.cstr(o.offset + 0x08),
                    type_name: t.into(),
                    init: None,
                },
                _ => continue,
            };
            vars.insert(o.offset, info);
        }
        let mut opaque_operands = HashMap::new();
        let scripts = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("tfbScriptInfo"))
            .filter_map(|info| {
                let list = r.ptr(info.offset + 0x14)?;
                let ops = r
                    .list(list)
                    .into_iter()
                    .filter_map(|w| r.decode_ptr(w))
                    .map(|o| {
                        let op = r.op(o);
                        if matches!(&op, Op::Other(t) if OPAQUE_LISTED.contains(&t.as_str())) {
                            opaque_operands
                                .entry(o)
                                .or_insert_with(|| r.opaque_operands(o));
                        }
                        Instr {
                            offset: o,
                            flags: r.u16(o + 0x18),
                            br: r.u16(o + 0x1a),
                            op,
                        }
                    })
                    .collect();
                Some(Script {
                    name: r.cstr(info.offset + 0x08),
                    ops,
                })
            })
            .collect();
        Some(Self {
            scripts,
            vars,
            handle_ids: file.exid_formats.clone(),
            handle_ns: file.exid_namespaces.clone(),
            roots: file.roots.clone(),
            export_base: file.external_types.len(),
            opaque_operands,
        })
    }

    /// Name hashes of an engine path (`my.orientation.facing`): every step is a
    /// handle outside `global.bld`. `None` for script objects and global exports.
    pub fn engine_key(&self, path: &[Ref]) -> Option<Vec<u32>> {
        let global = name_hash("global.bld");
        path.iter()
            .map(|r| match *r {
                Ref::Handle(h) if self.handle_ns.get(h as usize) != Some(&global) => {
                    self.handle_ids.get(h as usize).copied()
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .filter(|k| !k.is_empty())
    }

    pub fn var_name(&self, var: usize) -> &str {
        self.vars.get(&var).map_or("", |v| v.name.as_str())
    }

    /// Object behind `EXID` handle `h` when it is exported by `global`.
    pub fn global_handle<'g>(&self, h: u32, global: &'g ScriptSet) -> Option<&'g VarInfo> {
        let i = h as usize;
        if *self.handle_ns.get(i)? != name_hash("global.bld") {
            return None;
        }
        let id = (*self.handle_ids.get(i)? as usize).checked_sub(global.export_base)?;
        global.vars.get(global.roots.get(id)?)
    }

    pub fn script(&self, name_suffix: &str) -> Option<&Script> {
        self.scripts.iter().find(|s| s.name.ends_with(name_suffix))
    }

    /// `(script index, op index)` of the macro named `name`.
    pub fn find_macro(&self, name: &str) -> Option<(usize, usize)> {
        self.scripts.iter().enumerate().find_map(|(si, s)| {
            s.ops
                .iter()
                .position(|i| matches!(&i.op, Op::DefineMacro { name: n } if n == name))
                .map(|oi| (si, oi))
        })
    }

    fn ref_name(&self, r: &Ref) -> String {
        match r {
            Ref::Obj(o) => self
                .vars
                .get(o)
                .map_or_else(|| format!("@{o:x}"), |v| format!("\"{}\"", v.name)),
            Ref::Handle(h) => format!(
                "${:08x}",
                self.handle_ids.get(*h as usize).copied().unwrap_or(0)
            ),
        }
    }
    fn path_str(&self, p: &[Ref]) -> String {
        p.iter()
            .map(|r| self.ref_name(r))
            .collect::<Vec<_>>()
            .join(".")
    }
    fn operand_str(&self, o: &Operand) -> String {
        match o {
            Operand::Lit(Value::Int(v)) => v.to_string(),
            Operand::Lit(Value::Float(v)) => format!("{v:?}"),
            Operand::Read { path, kind } => format!("{}:{kind:?}", self.path_str(path)),
            Operand::Expr(r) => format!("({})", self.rhs_str(r)),
        }
    }
    fn rhs_str(&self, r: &Rhs) -> String {
        let mut s = self.operand_str(&r.a);
        if let Some(b) = &r.b {
            let op = ["+", "-", "*", "/"]
                .get(r.arith as usize)
                .copied()
                .unwrap_or("?op");
            let _ = write!(s, " {op}[{}] {}", r.arith, self.operand_str(b));
        }
        if r.func != 0 {
            let _ = write!(s, " func={}", r.func);
        }
        s
    }

    /// Readable listing of ops `[from, to)` of a script (RE aid / dump tests).
    pub fn listing(&self, script: &Script, from: usize, to: usize) -> String {
        let mut out = String::new();
        for (i, ins) in script.ops.iter().enumerate().take(to).skip(from) {
            let body = match &ins.op {
                Op::SetValue { lhs, rhs } => {
                    format!("set {} = {}", self.path_str(lhs), self.rhs_str(rhs))
                }
                Op::IncValue { lhs } => format!("inc {}", self.path_str(lhs)),
                Op::DecValue { lhs } => format!("dec {}", self.path_str(lhs)),
                Op::CheckValue { lhs, rel, rhs } => {
                    let r = ["<=", "==", ">=", "<", ">", "!=", "&", "!&"]
                        .get(*rel as usize)
                        .copied()
                        .unwrap_or("?rel");
                    format!(
                        "check {} {r}[{rel}] {}",
                        self.path_str(lhs),
                        self.rhs_str(rhs)
                    )
                }
                Op::CheckRef { desc, .. } => desc.clone(),
                Op::Interface => "interface".into(),
                Op::Jump => "jump".into(),
                Op::End => "end".into(),
                Op::LoopValue { count } => format!("loop {}", self.rhs_str(count)),
                Op::DefineMacro { name } => format!("MACRO \"{name}\""),
                Op::UseMacro { target } => format!(
                    "use {}",
                    target
                        .and_then(|t| script
                            .ops
                            .iter()
                            .chain(self.scripts.iter().flat_map(|s| s.ops.iter()))
                            .find(|i| i.offset == t))
                        .map_or("?".into(), |i| format!("{:?}", i.op))
                ),
                Op::Param {
                    var,
                    rhs,
                    macro_def,
                } => format!(
                    "param {} = {} (def @{:x})",
                    self.ref_name(&Ref::Obj(*var)),
                    rhs.as_ref().map_or("-".into(), |r| self.rhs_str(r)),
                    macro_def.unwrap_or(0)
                ),
                Op::CreateVariable { var } => format!("var {}", self.ref_name(&Ref::Obj(*var))),
                Op::SlideValue {
                    lhs,
                    target,
                    seconds,
                } => format!(
                    "slide {} -> {} over {}",
                    self.path_str(lhs),
                    self.rhs_str(target),
                    seconds.as_ref().map_or("-".into(), |r| self.rhs_str(r))
                ),
                Op::Displace {
                    target,
                    mode,
                    len,
                    heading,
                    pitch,
                } => {
                    let r = |r: &Option<Rhs>| r.as_ref().map_or("-".into(), |r| self.rhs_str(r));
                    format!(
                        "displace {} mode={mode} len={} heading={} pitch={}",
                        self.path_str(target),
                        r(len),
                        r(heading),
                        r(pitch)
                    )
                }
                Op::Remove { target } => format!("remove {}", self.path_str(target)),
                Op::PreScript => "PRESCRIPT".into(),
                Op::StartUp => "STARTUP".into(),
                Op::ShutDown => "SHUTDOWN".into(),
                Op::Other(t) => {
                    let mut s = t.clone();
                    for (f, operand) in self.opaque_operands.get(&ins.offset).into_iter().flatten()
                    {
                        let v = match operand {
                            OpaqueOperand::Path { stack, path } => {
                                format!("{stack}[{}]", self.path_str(path))
                            }
                            OpaqueOperand::Rhs(r) => self.rhs_str(r),
                            OpaqueOperand::Object { ty, name, offset } => {
                                format!("{ty}@{offset:x} {name:?}")
                            }
                        };
                        let _ = write!(s, " +{f:#x}={v}");
                    }
                    s
                }
            };
            let _ = writeln!(
                out,
                "[{i}] f={:04x} br={} @{:x} {body}",
                ins.flags, ins.br, ins.offset
            );
        }
        out
    }
}

/// `OpCode._flags` bit on a macro parameter that is a declaration/output, not an argument.
const PARAM_OUTPUT: u16 = 0x8000;
const STEP_BUDGET: u32 = 1_000_000;
const MAX_DEPTH: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    UnknownMacro(String),
    UnknownParam(String),
    /// Step or call-depth budget exhausted.
    Budget,
    /// `ArithOp` / `RelOp` / `_resultFunc` the evaluator does not model.
    Unsupported(String),
}

enum Flow {
    Next,
    Return,
}

/// Deterministic evaluator for the value logic of one `ScriptSet`.
///
/// Branching: a failed check skips its `br` following ops, `jump` skips `br`, a block
/// op (macro definition, loop, macro use) owns the `br` ops after it.
/// Variables live in one store keyed by canonical path; macro outputs share the
/// definition's parameter objects, so no frames are needed.
pub struct Vm<'a> {
    set: &'a ScriptSet,
    global: Option<&'a ScriptSet>,
    pub(crate) vars: HashMap<Vec<Ref>, Value>,
    loops: HashMap<usize, i32>,
    index: Cow<'a, VmIndex>,
    steps: u32,
    depth: u32,
    /// Engine state by name-hash path (`ScriptSet::engine_key`): reads fall back to it,
    /// writes to engine paths land here.
    pub engine: HashMap<Vec<u32>, Value>,
    /// Engine actions the scripts asked for, in execution order.
    pub effects: Vec<Effect>,
    /// Engine state read before any write (spawned objects, save data, other files),
    /// in first-read order. Such reads yield 0.
    pub unbound: Vec<String>,
}

/// Engine action requested by a script op.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// `OpSlideValue` at op `op` started: `lhs` goes `from` -> `to` over `seconds`.
    Slide {
        op: usize,
        lhs: Vec<Ref>,
        from: f32,
        to: f32,
        seconds: f32,
    },
    /// `OpDisplace` of the engine object `target` (name-hash path).
    Displace {
        target: Vec<u32>,
        len: f32,
        heading: f32,
        pitch: f32,
    },
    /// `OpRemove` of the engine object `target` (name-hash path).
    Remove { target: Vec<u32> },
}

/// Per-set lookup tables, shared by every `Vm` over the set.
#[derive(Debug, Clone, Default)]
pub struct VmIndex {
    /// A path through a check op means its LHS; through a loop op, its counter.
    aliases: HashMap<usize, (usize, usize)>,
    macros: HashMap<usize, (usize, usize)>,
}

impl VmIndex {
    pub fn new(set: &ScriptSet) -> Self {
        let mut index = Self::default();
        for (si, s) in set.scripts.iter().enumerate() {
            for (oi, ins) in s.ops.iter().enumerate() {
                match &ins.op {
                    Op::CheckValue { .. } | Op::LoopValue { .. } => {
                        index.aliases.insert(ins.offset, (si, oi));
                    }
                    Op::DefineMacro { .. } => {
                        index.macros.entry(ins.offset).or_insert((si, oi));
                    }
                    _ => {}
                }
            }
        }
        index
    }
}

impl<'a> Vm<'a> {
    /// `global` resolves `global.bld` handles to their initial values.
    pub fn new(set: &'a ScriptSet, global: Option<&'a ScriptSet>) -> Self {
        Self::with_index(set, global, Cow::Owned(VmIndex::new(set)))
    }

    /// `Vm` over a prebuilt index (one per set, many runs).
    pub fn with_index(
        set: &'a ScriptSet,
        global: Option<&'a ScriptSet>,
        index: Cow<'a, VmIndex>,
    ) -> Self {
        Self {
            set,
            global,
            vars: HashMap::new(),
            loops: HashMap::new(),
            index,
            steps: 0,
            depth: 0,
            engine: HashMap::new(),
            effects: Vec::new(),
            unbound: Vec::new(),
        }
    }

    /// Seed engine state the scripts read.
    pub fn set_var(&mut self, path: Vec<Ref>, v: Value) {
        self.vars.insert(path, v);
    }

    /// Run macro `name` with `args` bound by parameter name. Returns every interface
    /// parameter (inputs and outputs) after the call, in declaration order.
    pub fn call(
        &mut self,
        name: &str,
        args: &[(&str, Value)],
    ) -> Result<Vec<(String, Value)>, VmError> {
        let (si, oi) = self
            .set
            .find_macro(name)
            .ok_or_else(|| VmError::UnknownMacro(name.into()))?;
        let (params, _) = self.params(si, oi);
        let names: Vec<&str> = params.iter().map(|&v| self.set.var_name(v)).collect();
        if let Some((n, _)) = args.iter().find(|(n, _)| !names.contains(n)) {
            return Err(VmError::UnknownParam((*n).into()));
        }
        let args: Vec<(String, Value)> = args.iter().map(|(n, v)| (n.to_string(), *v)).collect();
        self.run_macro(si, oi, &args)?;
        Ok(params
            .iter()
            .map(|&v| (self.set.var_name(v).to_string(), self.read(&[Ref::Obj(v)])))
            .collect())
    }

    /// Parameter vars of the interface right after op `at`, and the index past it.
    fn params(&self, si: usize, at: usize) -> (Vec<usize>, usize) {
        let ops = &self.set.scripts[si].ops;
        match ops.get(at + 1) {
            Some(i) if i.op == Op::Interface => {
                let end = (at + 2 + i.br as usize).min(ops.len());
                let vars = ops[at + 2..end]
                    .iter()
                    .filter_map(|i| match i.op {
                        Op::Param { var, .. } => Some(var),
                        _ => None,
                    })
                    .collect();
                (vars, end)
            }
            _ => (Vec::new(), at + 1),
        }
    }

    fn param_rhs(&self, si: usize, at: usize) -> Vec<(usize, u16, Option<&'a Rhs>)> {
        let set: &'a ScriptSet = self.set;
        let ops = &set.scripts[si].ops;
        let (_, end) = self.params(si, at);
        ops[(at + 1).min(end)..end]
            .iter()
            .filter_map(|i| match &i.op {
                Op::Param { var, rhs, .. } => Some((*var, i.flags, rhs.as_ref())),
                _ => None,
            })
            .collect()
    }

    fn run_macro(&mut self, si: usize, oi: usize, args: &[(String, Value)]) -> Result<(), VmError> {
        if self.depth >= MAX_DEPTH {
            return Err(VmError::Budget);
        }
        let ops = &self.set.scripts[si].ops;
        let end = (oi + 1 + ops[oi].br as usize).min(ops.len());
        let (_, body) = self.params(si, oi);
        let params = self.param_rhs(si, oi);
        for &(var, _, rhs) in &params {
            if let Some(rhs) = rhs {
                let v = self.eval(rhs)?;
                self.write(&[Ref::Obj(var)], v);
            }
        }
        for (name, v) in args {
            if let Some(&(var, ..)) = params.iter().find(|p| self.set.var_name(p.0) == name) {
                self.write(&[Ref::Obj(var)], *v);
            }
        }
        self.depth += 1;
        let r = self.run(si, body, end);
        self.depth -= 1;
        r.map(|_| ())
    }

    /// Macro use at `at`: bind arguments, run the target, copy results into the call
    /// site's own parameter objects, then continue into the use's callback body.
    fn use_macro(&mut self, si: usize, at: usize, target: Option<usize>) -> Result<usize, VmError> {
        let (_, after) = self.params(si, at);
        let site = self.param_rhs(si, at);
        let Some(&(tsi, toi)) = target.and_then(|t| self.index.macros.get(&t)) else {
            // Defined in another file: every parameter keeps its literal.
            let what = format!("macro @{:x}", target.unwrap_or(0));
            if !self.unbound.contains(&what) {
                self.unbound.push(what);
            }
            for (var, _, rhs) in site {
                if let Some(rhs) = rhs {
                    let v = self.eval(rhs)?;
                    self.write(&[Ref::Obj(var)], v);
                }
            }
            return Ok(after);
        };
        let mut args = Vec::new();
        for &(var, flags, rhs) in &site {
            if let (0, Some(rhs)) = (flags & PARAM_OUTPUT, rhs) {
                args.push((self.set.var_name(var).to_string(), self.eval(rhs)?));
            }
        }
        self.run_macro(tsi, toi, &args)?;
        let (defs, _) = self.params(tsi, toi);
        for &(var, ..) in &site {
            let name = self.set.var_name(var);
            if let Some(&d) = defs
                .iter()
                .find(|&&d| d != var && self.set.var_name(d) == name)
            {
                let v = self.read(&[Ref::Obj(d)]);
                self.write(&[Ref::Obj(var)], v);
            }
        }
        Ok(after)
    }

    /// Run ops `[from, to)` of script `si`.
    pub fn run_range(&mut self, si: usize, from: usize, to: usize) -> Result<(), VmError> {
        self.run(si, from, to).map(|_| ())
    }

    fn run(&mut self, si: usize, from: usize, to: usize) -> Result<Flow, VmError> {
        let set: &'a ScriptSet = self.set;
        let ops = &set.scripts[si].ops;
        let mut pc = from;
        while pc < to {
            self.steps += 1;
            if self.steps > STEP_BUDGET {
                return Err(VmError::Budget);
            }
            let ins = &ops[pc];
            let skip = ins.br as usize;
            pc += 1;
            match &ins.op {
                Op::SetValue { lhs, rhs } => {
                    let v = self.eval(rhs)?;
                    self.write(lhs, v);
                }
                Op::IncValue { lhs } | Op::DecValue { lhs } => {
                    let d = if matches!(ins.op, Op::IncValue { .. }) {
                        0
                    } else {
                        1
                    };
                    let v = arith(self.read(lhs), d, Value::Int(1))?;
                    self.write(lhs, v);
                }
                Op::CheckValue { lhs, rel, rhs } => {
                    let a = self.read(lhs);
                    let b = self.eval(rhs)?;
                    if !compare(a, *rel, b)? {
                        pc += skip;
                    }
                }
                Op::CheckRef { rel, .. } => {
                    if *rel != 5 {
                        pc += skip;
                    }
                }
                Op::Jump | Op::DefineMacro { .. } => pc += skip,
                Op::End => return Ok(Flow::Return),
                Op::LoopValue { count } => {
                    let n = self.eval(count)?.as_f32() as i32;
                    let end = (pc + skip).min(to);
                    for i in 0..n {
                        self.loops.insert(ins.offset, i);
                        if let Flow::Return = self.run(si, pc, end)? {
                            return Ok(Flow::Return);
                        }
                    }
                    pc = end;
                }
                Op::UseMacro { target } => pc = self.use_macro(si, pc - 1, *target)?,
                // Event blocks other than the one being run.
                Op::StartUp | Op::ShutDown => pc += skip,
                // The block after a slide configures its slider (`looping`, ...).
                Op::SlideValue {
                    lhs,
                    target,
                    seconds,
                } => {
                    let from = self.read(lhs).as_f32();
                    let to = self.eval(target)?.as_f32();
                    let seconds = match seconds {
                        Some(r) => self.eval(r)?.as_f32(),
                        None => 0.0,
                    };
                    if let Ok(lhs) = self.canon(lhs) {
                        self.effects.push(Effect::Slide {
                            op: ins.offset,
                            lhs,
                            from,
                            to,
                            seconds,
                        });
                    }
                }
                Op::Displace {
                    target,
                    len,
                    heading,
                    pitch,
                    ..
                } => {
                    if let Some(target) = self.set.engine_key(target) {
                        let mut f = |r: &Option<Rhs>| -> Result<f32, VmError> {
                            Ok(match r {
                                Some(r) => self.eval(r)?.as_f32(),
                                None => 0.0,
                            })
                        };
                        let (len, heading, pitch) = (f(len)?, f(heading)?, f(pitch)?);
                        self.effects.push(Effect::Displace {
                            target,
                            len,
                            heading,
                            pitch,
                        });
                    }
                }
                // Only when nothing unresolved (save data, other files) led here:
                // `Persephone_2014.ai` removes herself on a save flag we cannot read.
                Op::Remove { target } => {
                    if let Some(target) = self.set.engine_key(target) {
                        if self.unbound.is_empty() {
                            self.effects.push(Effect::Remove { target });
                        }
                    }
                }
                // Blocks of ops that need live objects (find variable, spawn, for each,
                // user behaviors): skipped, as if nothing was found.
                Op::Other(_) => pc += skip,
                Op::Interface | Op::Param { .. } | Op::CreateVariable { .. } | Op::PreScript => {}
            }
        }
        Ok(Flow::Next)
    }

    fn eval(&mut self, rhs: &Rhs) -> Result<Value, VmError> {
        if rhs.func != 0 {
            return Err(VmError::Unsupported(format!("result func {}", rhs.func)));
        }
        let a = self.operand(&rhs.a)?;
        match &rhs.b {
            Some(b) if rhs.arith >= 0 => {
                let b = self.operand(b)?;
                arith(a, rhs.arith, b)
            }
            _ => Ok(a),
        }
    }

    fn operand(&mut self, o: &Operand) -> Result<Value, VmError> {
        match o {
            Operand::Lit(v) => Ok(*v),
            Operand::Read { path, .. } => Ok(self.read(path)),
            Operand::Expr(r) => self.eval(r),
        }
    }

    /// `Err(counter)` for a loop counter.
    fn canon(&self, path: &[Ref]) -> Result<Vec<Ref>, i32> {
        let mut p = path.to_vec();
        for _ in 0..8 {
            let Some(Ref::Obj(o)) = p.first() else { break };
            let alias = self
                .index
                .aliases
                .get(o)
                .map(|&(si, oi)| &self.set.scripts[si].ops[oi].op);
            match alias {
                Some(Op::CheckValue { lhs, .. }) => {
                    p = lhs.iter().chain(&p[1..]).copied().collect();
                }
                Some(Op::LoopValue { .. }) if p.len() == 1 => {
                    return Err(self.loops.get(o).copied().unwrap_or(0));
                }
                _ => break,
            }
        }
        Ok(p)
    }

    fn read(&mut self, path: &[Ref]) -> Value {
        let path = match self.canon(path) {
            Ok(p) => p,
            Err(i) => return Value::Int(i),
        };
        if let Some(v) = self.vars.get(&path) {
            return *v;
        }
        if let Some(v) = self.set.engine_key(&path).and_then(|k| self.engine.get(&k)) {
            return *v;
        }
        let init = match path[..] {
            [Ref::Obj(o)] => {
                return self
                    .set
                    .vars
                    .get(&o)
                    .and_then(|v| v.init)
                    .unwrap_or(Value::Int(0))
            }
            [Ref::Handle(h)] => self
                .global
                .and_then(|g| self.set.global_handle(h, g))
                .and_then(|v| v.init),
            _ => None,
        };
        init.unwrap_or_else(|| {
            let what = self.set.path_str(&path);
            if !self.unbound.contains(&what) {
                self.unbound.push(what);
            }
            Value::Int(0)
        })
    }

    fn write(&mut self, path: &[Ref], v: Value) {
        if let Ok(p) = self.canon(path) {
            match self.set.engine_key(&p) {
                Some(k) => {
                    self.engine.insert(k, v);
                }
                None => {
                    self.vars.insert(p, v);
                }
            }
        }
    }
}

/// `Int op Int` stays `Int` except division; anything with a `Float` is `Float`.
/// Division by zero yields 0.
fn arith(a: Value, op: i32, b: Value) -> Result<Value, VmError> {
    use Value::{Float, Int};
    Ok(match (op, a, b) {
        (0, Int(x), Int(y)) => Int(x.wrapping_add(y)),
        (1, Int(x), Int(y)) => Int(x.wrapping_sub(y)),
        (2, Int(x), Int(y)) => Int(x.wrapping_mul(y)),
        (0, ..) => Float(a.as_f32() + b.as_f32()),
        (1, ..) => Float(a.as_f32() - b.as_f32()),
        (2, ..) => Float(a.as_f32() * b.as_f32()),
        (3, ..) if b.as_f32() == 0.0 => Float(0.0),
        (3, ..) => Float(a.as_f32() / b.as_f32()),
        _ => return Err(VmError::Unsupported(format!("arith {op}"))),
    })
}

fn compare(a: Value, rel: i32, b: Value) -> Result<bool, VmError> {
    let (x, y) = (a.as_f32(), b.as_f32());
    Ok(match rel {
        0 => x <= y,
        1 => x == y,
        2 => x >= y,
        3 => x < y,
        4 => x > y,
        5 => x != y,
        6 => a.bits() & b.bits() != 0,
        7 => a.bits() & b.bits() == 0,
        _ => return Err(VmError::Unsupported(format!("rel {rel}"))),
    })
}

/// Result of `LevelSystems.ai` "Game - Define Enemy Stats".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnemyStats {
    pub hp: f32,
    pub damage: f32,
}

/// "Game - Define Enemy Stats" for `archetype` (1-8; 3 Near, 4 Far) at
/// `difficulty_level` (1-10), single player, normal difficulty mode, not challenge/horde.
/// `HP = Average_Minion_Damage * hits-to-kill`, `damage = Average_Minion_HP / hits-to-kill-hero`,
/// each scaled by the difficulty level.
pub fn enemy_stats(
    level: &ScriptSet,
    global: &ScriptSet,
    archetype: i32,
    difficulty_level: i32,
) -> Result<EnemyStats, VmError> {
    let out = Vm::new(level, Some(global)).call(
        "Game - Define Enemy Stats",
        &[
            ("Archetype ID", Value::Int(archetype)),
            ("Difficulty Level", Value::Int(difficulty_level)),
        ],
    )?;
    let get = |n: &str| {
        out.iter()
            .find(|(k, _)| k == n)
            .map(|(_, v)| v.as_f32())
            .ok_or_else(|| VmError::UnknownParam(n.into()))
    };
    Ok(EnemyStats {
        hp: get("Return_HP")?,
        damage: get("Return_Damage")?,
    })
}

/// `Init_Setup_SaveGame_Portal.ai` "Portal - Define Levelup Table" defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelupTable {
    /// `"level hp mult"`: max-HP gain per level above 1.
    pub hp_mult_per_level: f32,
    /// XP needed to reach level `i + 2`.
    pub xp: Vec<u32>,
}

impl LevelupTable {
    pub fn from_scripts(init_setup: &ScriptSet) -> Result<Self, VmError> {
        let out = Vm::new(init_setup, None).call("Portal - Define Levelup Table", &[])?;
        let mut xp = Vec::new();
        let mut hp_mult_per_level = None;
        for (name, v) in out {
            if name == "level hp mult" {
                hp_mult_per_level = Some(v.as_f32());
            } else if name.starts_with("level xp (") {
                xp.push(v.as_f32() as u32);
            }
        }
        Ok(Self {
            hp_mult_per_level: hp_mult_per_level
                .ok_or_else(|| VmError::UnknownParam("level hp mult".into()))?,
            xp,
        })
    }

    /// Global `PortalObject_Minion.ai` "Minion - Get HP Mult for Level":
    /// `(level - 1) * level hp mult + 1`.
    pub fn hp_mult(&self, level: u32) -> f32 {
        level.saturating_sub(1) as f32 * self.hp_mult_per_level + 1.0
    }

    /// Level (1-based) reached with `xp` experience.
    pub fn level_for_xp(&self, xp: u32) -> u32 {
        1 + self.xp.iter().take_while(|&&t| xp >= t).count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(offset: usize, br: u16, op: Op) -> Instr {
        Instr {
            offset,
            flags: 0,
            br,
            op,
        }
    }
    fn lit(v: Value) -> Rhs {
        Rhs {
            a: Operand::Lit(v),
            arith: -1,
            b: None,
            func: 0,
        }
    }
    fn rd(var: usize) -> Operand {
        Operand::Read {
            path: vec![Ref::Obj(var)],
            kind: NumKind::Other,
        }
    }
    fn bin(a: Operand, arith: i32, b: Operand) -> Rhs {
        Rhs {
            a,
            arith,
            b: Some(b),
            func: 0,
        }
    }
    fn set(var: usize, rhs: Rhs) -> Op {
        Op::SetValue {
            lhs: vec![Ref::Obj(var)],
            rhs,
        }
    }
    fn param(var: usize, flags: u16, rhs: Option<Rhs>) -> Instr {
        Instr {
            offset: var,
            flags,
            br: 0,
            op: Op::Param {
                var,
                rhs,
                macro_def: None,
            },
        }
    }
    fn script_set(ops: Vec<Instr>, names: &[(usize, &str)]) -> ScriptSet {
        ScriptSet {
            scripts: vec![Script {
                name: "test.ai".into(),
                ops,
            }],
            vars: names
                .iter()
                .map(|&(o, n)| {
                    (
                        o,
                        VarInfo {
                            name: n.into(),
                            type_name: "OpMacroParameter".into(),
                            init: None,
                        },
                    )
                })
                .collect(),
            ..Default::default()
        }
    }

    const I: fn(i32) -> Value = Value::Int;
    const F: fn(f32) -> Value = Value::Float;

    /// MACRO "clamp" (x, out): out = x * 2; if out > 10 { out = 10 } else { out = out + 1 }
    fn clamp_set() -> ScriptSet {
        let (x, out) = (0x100, 0x110);
        script_set(
            vec![
                ins(
                    1,
                    9,
                    Op::DefineMacro {
                        name: "clamp".into(),
                    },
                ),
                ins(2, 2, Op::Interface),
                param(x, PARAM_OUTPUT, Some(lit(I(0)))),
                param(out, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(3, 0, set(out, bin(rd(x), 2, Operand::Lit(I(2))))),
                ins(
                    4,
                    2,
                    Op::CheckValue {
                        lhs: vec![Ref::Obj(out)],
                        rel: 4,
                        rhs: lit(I(10)),
                    },
                ),
                ins(5, 0, set(out, lit(I(10)))),
                ins(6, 1, Op::Jump),
                ins(7, 0, set(out, bin(rd(out), 0, Operand::Lit(I(1))))),
                ins(8, 0, Op::Other("OpFlowBuiltInBehavior".into())),
            ],
            &[(x, "x"), (out, "out")],
        )
    }

    fn out_of(r: Vec<(String, Value)>, n: &str) -> Value {
        r.into_iter().find(|(k, _)| k == n).unwrap().1
    }

    #[test]
    fn vm_branches_and_arith() {
        let s = clamp_set();
        let mut vm = Vm::new(&s, None);
        assert_eq!(
            out_of(vm.call("clamp", &[("x", I(3))]).unwrap(), "out"),
            I(7)
        );
        assert_eq!(
            out_of(vm.call("clamp", &[("x", I(8))]).unwrap(), "out"),
            I(10)
        );
        assert_eq!(
            out_of(vm.call("clamp", &[("x", F(1.5))]).unwrap(), "out"),
            F(4.0)
        );
        assert!(vm.unbound.is_empty());
    }

    #[test]
    fn vm_errors() {
        let s = clamp_set();
        let mut vm = Vm::new(&s, None);
        assert_eq!(
            vm.call("nope", &[]).unwrap_err(),
            VmError::UnknownMacro("nope".into())
        );
        assert_eq!(
            vm.call("clamp", &[("y", I(1))]).unwrap_err(),
            VmError::UnknownParam("y".into())
        );
        assert_eq!(
            arith(I(1), 9, I(1)),
            Err(VmError::Unsupported("arith 9".into()))
        );
        assert_eq!(arith(I(7), 3, I(0)), Ok(F(0.0)));
        assert_eq!(arith(I(7), 3, I(2)), Ok(F(3.5)));
    }

    #[test]
    fn vm_macro_call_loop_and_aliases() {
        // "outer": n = 0; loop 4 { check loop == 2 { skip } ; use clamp(x = loop) -> n += out }
        let mut s = clamp_set();
        let (n, site_x, loop_op, chk) = (0x200, 0x210, 0x30, 0x31);
        let read_loop = Operand::Read {
            path: vec![Ref::Obj(loop_op)],
            kind: NumKind::Int,
        };
        let mut ops = vec![
            ins(
                0x20,
                14,
                Op::DefineMacro {
                    name: "outer".into(),
                },
            ),
            ins(0x21, 1, Op::Interface),
            param(n, PARAM_OUTPUT, Some(lit(I(0)))),
            ins(0x22, 0, set(n, lit(I(0)))),
            ins(loop_op, 9, Op::LoopValue { count: lit(I(4)) }),
            ins(
                chk,
                7,
                Op::CheckValue {
                    lhs: vec![Ref::Obj(loop_op)],
                    rel: 5,
                    rhs: lit(I(2)),
                },
            ),
            ins(0x32, 4, Op::UseMacro { target: Some(1) }),
            ins(0x33, 2, Op::Interface),
            param(
                site_x,
                0,
                Some(Rhs {
                    a: read_loop,
                    ..lit(I(0))
                }),
            ),
            param(0x110, PARAM_OUTPUT, Some(lit(I(0)))),
            ins(0x34, 0, set(n, bin(rd(n), 0, rd(0x110)))),
            // Reads through the check op alias its LHS (the loop counter).
            ins(
                0x35,
                1,
                Op::CheckValue {
                    lhs: vec![Ref::Obj(chk)],
                    rel: 1,
                    rhs: lit(I(3)),
                },
            ),
            ins(0x36, 0, Op::End),
            ins(0x37, 0, Op::Other("OpFlowBuiltInBehavior".into())),
            ins(0x38, 0, Op::Other("OpFlowBuiltInBehavior".into())),
        ];
        s.scripts[0].ops.append(&mut ops);
        s.vars.insert(
            site_x,
            VarInfo {
                name: "x".into(),
                type_name: "OpMacroParameter".into(),
                init: None,
            },
        );
        s.vars.insert(
            n,
            VarInfo {
                name: "n".into(),
                type_name: "OpMacroParameter".into(),
                init: None,
            },
        );
        let mut vm = Vm::new(&s, None);
        // i = 0, 1 run clamp (1 + 3); i = 2 skipped; i = 3 runs clamp (7) then returns.
        assert_eq!(out_of(vm.call("outer", &[]).unwrap(), "n"), I(11));
    }

    #[test]
    fn vm_unresolved_macro_keeps_literals_and_records_unbound() {
        let (v, r) = (0x100, 0x110);
        let s = script_set(
            vec![
                ins(1, 7, Op::DefineMacro { name: "m".into() }),
                ins(2, 1, Op::Interface),
                param(r, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(
                    3,
                    2,
                    Op::UseMacro {
                        target: Some(0xdead),
                    },
                ),
                ins(4, 1, Op::Interface),
                param(v, PARAM_OUTPUT, Some(lit(I(-1)))),
                ins(
                    5,
                    0,
                    set(
                        r,
                        bin(
                            rd(v),
                            0,
                            Operand::Read {
                                path: vec![Ref::Handle(7)],
                                kind: NumKind::Int,
                            },
                        ),
                    ),
                ),
                ins(
                    6,
                    0,
                    Op::CheckRef {
                        rel: 5,
                        desc: String::new(),
                    },
                ),
            ],
            &[(v, "val"), (r, "r")],
        );
        let mut vm = Vm::new(&s, None);
        assert_eq!(out_of(vm.call("m", &[]).unwrap(), "r"), I(-1));
        assert_eq!(
            vm.unbound,
            vec!["macro @dead".to_string(), "$00000000".into()]
        );
    }

    #[test]
    fn vm_budget() {
        let x = 0x100;
        let s = script_set(
            vec![
                ins(
                    1,
                    3,
                    Op::DefineMacro {
                        name: "spin".into(),
                    },
                ),
                ins(
                    2,
                    1,
                    Op::LoopValue {
                        count: lit(I(i32::MAX)),
                    },
                ),
                ins(
                    3,
                    0,
                    Op::IncValue {
                        lhs: vec![Ref::Obj(x)],
                    },
                ),
            ],
            &[(x, "x")],
        );
        assert_eq!(Vm::new(&s, None).call("spin", &[]), Err(VmError::Budget));
    }

    fn read_bld(dir: &str, name: &str) -> Option<Vec<u8>> {
        let p = [
            format!("assets/files/{dir}"),
            format!("../../assets/files/{dir}"),
        ]
        .into_iter()
        .map(|d| std::path::Path::new(&d).join(format!("{name}.bld")))
        .find(|p| p.exists())?;
        let mut m = crate::archive::ArchiveMount::new();
        m.mount(&p).ok()?;
        m.read_file("level.bld").ok()
    }

    fn rom_sets() -> Option<(ScriptSet, ScriptSet)> {
        let g = ScriptSet::parse(&read_bld("permanent", "global")?)?;
        let l = ScriptSet::parse(&read_bld("level", "Level_302_Chompywurm")?)?;
        Some((g, l))
    }

    #[test]
    fn rom_global_handles_resolve() {
        let Some((g, l)) = rom_sets() else { return };
        let name = |id: u32| {
            let h = l.handle_ids.iter().position(|&v| v == id).unwrap() as u32;
            let v = l.global_handle(h, &g).unwrap();
            (v.name.clone(), v.init)
        };
        assert_eq!(name(0xad4), ("Average_Minion_Damage".into(), Some(F(10.0))));
        assert_eq!(name(0xad6), ("Average_Minion_HP".into(), Some(F(100.0))));
        assert_eq!(name(0x5f8).0, "Minion Global Tuning");
    }

    #[test]
    fn rom_enemy_stats() {
        let Some((g, l)) = rom_sets() else { return };
        let near = enemy_stats(&l, &g, 3, 1).unwrap();
        let far = enemy_stats(&l, &g, 4, 1).unwrap();
        // 10 dmg * 5.5 hits, 100 HP / 9 hits; 10 * 4, 100 / 15.
        assert!((near.hp - 55.0).abs() < 1e-3, "{near:?}");
        assert!((near.damage - 100.0 / 9.0).abs() < 1e-3, "{near:?}");
        assert!((far.hp - 40.0).abs() < 1e-3, "{far:?}");
        assert!((far.damage - 100.0 / 15.0).abs() < 1e-3, "{far:?}");
        // Level 10: HP x2 (power stat max), damage x6 (enemy damage max).
        let hard = enemy_stats(&l, &g, 3, 10).unwrap();
        assert!((hard.hp - 110.0).abs() < 1e-3, "{hard:?}");
        assert!((hard.damage - 600.0 / 9.0).abs() < 1e-3, "{hard:?}");
        assert_eq!(enemy_stats(&l, &g, 3, 1).unwrap(), near, "deterministic");
    }

    #[test]
    fn rom_levelup_table() {
        let Some(b) = read_bld("character", "Init_Setup") else {
            return;
        };
        let t = LevelupTable::from_scripts(&ScriptSet::parse(&b).unwrap()).unwrap();
        assert!((t.hp_mult_per_level - 0.111).abs() < 1e-6);
        assert_eq!(t.xp.len(), 19);
        assert_eq!((t.xp[0], t.xp[18]), (1000, 197_500));
        assert!(t.xp.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(t.level_for_xp(0), 1);
        assert_eq!(t.level_for_xp(2200), 3);
        assert_eq!(t.level_for_xp(u32::MAX), 20);
        assert!((t.hp_mult(20) - (1.0 + 19.0 * 0.111)).abs() < 1e-5);
    }

    /// RE aid: `cargo test -p skylands-formats dump_scripts -- --ignored`.
    /// `SKY_BLD=dir/name` (default `level/Level_302_Chompywurm`); writes one listing per
    /// script to `/tmp/skylands-re/<name>/` (local only, never committed).
    #[test]
    #[ignore]
    fn dump_scripts() {
        let arg = std::env::var("SKY_BLD").unwrap_or("level/Level_302_Chompywurm".into());
        let (dir, name) = arg.split_once('/').unwrap();
        let Some(b) = read_bld(dir, name) else { return };
        let s = ScriptSet::parse(&b).unwrap();
        let out = std::path::Path::new("/tmp/skylands-re").join(name);
        std::fs::create_dir_all(&out).unwrap();
        for sc in &s.scripts {
            let file = sc.name.rsplit('/').next().unwrap_or("x");
            let text = s.listing(sc, 0, sc.ops.len());
            std::fs::write(out.join(format!("{file}.txt")), text).unwrap();
        }
    }
}
