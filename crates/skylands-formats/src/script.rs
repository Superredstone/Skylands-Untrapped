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
    /// `OpCheckReference` / `OpCheckMembership`: `==` / `!=` of two references when the VM
    /// knows both (`lhs` +0x1c, `rhs` +0x24); else `NE` is true and everything else false.
    CheckRef {
        rel: i32,
        desc: String,
        lhs: Vec<Ref>,
        rhs: Vec<Ref>,
    },
    /// `OpSetReference`: point the reference `lhs` (+0x1c) at `rhs` (+0x20).
    SetRef {
        lhs: Vec<Ref>,
        rhs: Vec<Ref>,
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
        /// `_easeOutRHS` / `_easeInRHS`: shape of the start / end (`ease_curve`).
        ease_out: Option<Rhs>,
        ease_in: Option<Rhs>,
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
    /// `OpStartSequence`: play state `index` of the sequence group at `group` (`my`'s
    /// `animation palette`). Owns a setup block of `br` ops, skipped when nothing resolves.
    StartSequence {
        group: Vec<Ref>,
        index: Option<Rhs>,
    },
    /// `OpTurnTo`: turn the object at `target` (+0x20; +0x24 picks a group member) to
    /// `facing` degrees.
    TurnTo {
        facing: Rhs,
        target: Vec<Ref>,
    },
    /// `OpUserBehavior`: named block run only while it is the actor's current behaviour.
    Behavior {
        name: String,
    },
    /// `OpSetBehavior`: make the `OpUserBehavior` at op offset `target` current.
    SetBehavior {
        target: Option<usize>,
    },
    /// Event blocks of a script (`OpPreScript` / `OpStartUp` / `OpShutDown`).
    PreScript,
    StartUp,
    ShutDown,
    /// `OpFindVariable`: look up a variable by name on a target object/group path.
    FindVariable {
        name: String,
        target: Vec<Ref>,
    },
    /// `OpSpawn` (execute `0x8007c6bc`): clone the `AbstractPlacement` that the path
    /// `template` (+0x20) resolves to, at the current transform of the object at `at`
    /// (`_RHS` +0x28; it becomes the clone's parent), turned to `facing` (`_facingRHS` +0x2c)
    /// when that evaluates. Owns a setup block of `br` ops in which the op object means the
    /// clone. `actor`: the placement itself when the path is just one. Notes: `agent/SCRIPT_VM.md`.
    Spawn {
        template: Vec<Ref>,
        actor: Option<SpawnTemplate>,
        at: Vec<Ref>,
        facing: Option<Rhs>,
    },
    /// Anything else (movement, sound...): skipped by the evaluator.
    Other(String),
}

/// Placement an `OpSpawn` clones: object offset, type, `+0x08` name and
/// `AbstractPlacement._ID` (+0x20). The clone copies the template's fields
/// (`0x80156328`), so also its `Placement._script` (+0xa8, `.ai` path),
/// `_scaleVec` (+0xb8, percent) and `_tint` (+0xc8, RGBA).
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnTemplate {
    pub offset: usize,
    pub ty: String,
    pub name: String,
    pub id: u32,
    pub script: Option<String>,
    pub scale: [f32; 3],
    pub tint: [u8; 4],
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
    /// Operands of ops the decoder has no variant for (`OpSpawn`, `OpControl`...), keyed by op offset: (field offset, operand). Listing only.
    pub opaque_operands: HashMap<usize, Vec<(usize, OpaqueOperand)>>,
    /// `ScriptSet` objects (`_list` +0x18 -> members, `+0x08` names): static sets such as
    /// `global.bld` `$72e` (the 22 campaign levels) and `$9a1` (the 4 save slots).
    pub sets: HashMap<usize, Vec<String>>,
    /// Name of every `OpDefineMacro` object, including stubs of macros defined in other
    /// files (`global.bld` "System - Save - Get Save Param"), which no script lists.
    pub macro_names: HashMap<usize, String>,
}

/// Ops whose operands `ScriptSet::opaque_operands` records.
const OPAQUE_LISTED: &[&str] = &[
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
                let value = if path.is_empty() {
                    let raw = self.u32(o + 0x24)?;
                    Operand::Lit(match kind {
                        NumKind::Float => Value::Float(f32::from_bits(raw)),
                        _ => Value::Int(raw as i32),
                    })
                } else {
                    Operand::Read { path, kind }
                };
                // `_properties` (+0x1c) low half: result func id (`random`, `abs`...).
                let func = self.u32(o + 0x1c).unwrap_or(0) & 0xffff;
                Some(match func {
                    0 => value,
                    _ => Operand::Expr(Box::new(Rhs {
                        a: value,
                        arith: -1,
                        b: None,
                        func,
                    })),
                })
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
                    ease_out: self.rhs_at(o + 0x28),
                    ease_in: self.rhs_at(o + 0x2c),
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
            "OpStartSequence" => Op::StartSequence {
                group: self.lhs(o + 0x1c),
                index: self.rhs_at(o + 0x20),
            },
            "OpTurnTo" => match self.rhs_at(o + 0x1c) {
                Some(facing) => Op::TurnTo {
                    facing,
                    target: self.lhs(o + 0x20),
                },
                None => Op::Other(t.into()),
            },
            "OpUserBehavior" => Op::Behavior {
                name: self.cstr(o + 0x08),
            },
            "OpSetBehavior" => Op::SetBehavior {
                target: self.ptr(o + 0x1c).and_then(|s| {
                    self.path(s).into_iter().find_map(|r| match r {
                        Ref::Obj(p) if self.ty(p) == "OpUserBehavior" => Some(p),
                        _ => None,
                    })
                }),
            },
            "OpSpawn" => {
                let template = self.lhs(o + 0x20);
                // Ops and variables in the path hold references; resolved at run time.
                let actor = match template[..] {
                    [Ref::Obj(a)] if !self.ty(a).starts_with("Op") => Some(SpawnTemplate {
                        offset: a,
                        ty: self.ty(a).into(),
                        name: self.cstr(a + 0x08),
                        id: self.u32(a + 0x20).unwrap_or(0),
                        script: self
                            .ptr(a + 0xa8)
                            .filter(|&p| self.ty(p) == "tfbScriptInfo")
                            .map(|p| self.cstr(p + 0x08)),
                        scale: [0, 4, 8]
                            .map(|k| f32::from_bits(self.u32(a + 0xb8 + k).unwrap_or(0x42c8_0000))),
                        tint: self
                            .data
                            .get(a + 0xc8..a + 0xcc)
                            .map_or([0x80, 0x80, 0x80, 0xff], |b| [b[0], b[1], b[2], b[3]]),
                    }),
                    _ => None,
                };
                Op::Spawn {
                    template,
                    actor,
                    at: self.lhs(o + 0x28),
                    facing: self.rhs_at(o + 0x2c),
                }
            }
            "OpPreScript" => Op::PreScript,
            "OpStartUp" => Op::StartUp,
            "OpShutDown" => Op::ShutDown,
            "OpFindVariable" => Op::FindVariable {
                name: self.cstr(o + 0x24),
                target: self.lhs(o + 0x30),
            },
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
                let lhs = self.lhs(o + 0x1c);
                if set {
                    Op::SetRef {
                        lhs,
                        rhs: self.lhs(o + 0x20),
                        desc,
                    }
                } else {
                    Op::CheckRef {
                        rel,
                        desc,
                        lhs,
                        rhs: self.lhs(o + 0x24),
                    }
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
        let sets = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("ScriptSet"))
            .map(|o| {
                let members = r
                    .ptr(o.offset + 0x18)
                    .map(|l| r.list(l))
                    .unwrap_or_default();
                let names = members
                    .into_iter()
                    .filter_map(|w| r.decode_ptr(w))
                    .map(|m| r.cstr(m + 0x08))
                    .collect();
                (o.offset, names)
            })
            .collect();
        let macro_names = objects
            .iter()
            .filter(|o| {
                o.type_name
                    .as_deref()
                    .is_some_and(|t| t.starts_with("OpDefineMacro"))
            })
            .map(|o| (o.offset, r.cstr(o.offset + 0x08)))
            .collect();
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
                        // Reference arguments (`"save param" = -`) live in a param's opaque fields.
                        if matches!(&op, Op::Other(t) if OPAQUE_LISTED.contains(&t.as_str()))
                            || matches!(op, Op::Param { rhs: None, .. })
                        {
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
            sets,
            macro_names,
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

    /// Members of the static `ScriptSet` behind `EXID` handle `h` exported by `global`.
    pub fn global_set<'g>(&self, h: u32, global: &'g ScriptSet) -> Option<&'g [String]> {
        let i = h as usize;
        if *self.handle_ns.get(i)? != name_hash("global.bld") {
            return None;
        }
        let id = (*self.handle_ids.get(i)? as usize).checked_sub(global.export_base)?;
        global.sets.get(global.roots.get(id)?).map(Vec::as_slice)
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
    pub(crate) fn path_str(&self, p: &[Ref]) -> String {
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
    pub(crate) fn rhs_str(&self, r: &Rhs) -> String {
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
                Op::CheckRef { desc, .. } | Op::SetRef { desc, .. } => desc.clone(),
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
                    ..
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
                Op::StartSequence { group, index } => format!(
                    "start sequence {} [{}]",
                    self.path_str(group),
                    index.as_ref().map_or("-".into(), |r| self.rhs_str(r))
                ),
                Op::TurnTo { facing, target } => {
                    format!("turn {} to {}", self.path_str(target), self.rhs_str(facing))
                }
                Op::Behavior { name } => format!("behavior {name:?}"),
                Op::SetBehavior { target } => format!(
                    "set behavior {}",
                    target.map_or("?".into(), |t| format!("@{t:x}"))
                ),
                Op::Spawn {
                    template,
                    actor,
                    at,
                    facing,
                } => format!(
                    "spawn {} at {} facing {}",
                    actor.as_ref().map_or_else(
                        || self.path_str(template),
                        |a| format!("{}{:?}#{}", a.ty, a.name, a.id)
                    ),
                    self.path_str(at),
                    facing.as_ref().map_or("-".into(), |r| self.rhs_str(r))
                ),
                Op::PreScript => "PRESCRIPT".into(),
                Op::StartUp => "STARTUP".into(),
                Op::ShutDown => "SHUTDOWN".into(),
                Op::FindVariable { name, target } => {
                    format!("find variable {:?} on {}", name, self.path_str(target))
                }
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
    /// Global variables shared across actor scripts (e.g. "Levels_Completed").
    pub globals: HashMap<String, Value>,
    /// Engine state read before any write (spawned objects, save data, other files),
    /// in first-read order. Such reads yield 0.
    pub unbound: Vec<String>,
    /// State of the `random` result func's generator.
    pub rng: u64,
    /// Op offset of the actor's current `OpUserBehavior` (`actor+0xac`).
    pub behavior: Option<usize>,
    /// What `OpSetReference` pointed each reference variable at.
    pub(crate) refs: HashMap<Vec<Ref>, RefTarget>,
    /// Save data for "Get Save Param" calls; `None` leaves their `val` at its literal.
    pub save: Option<std::sync::Arc<dyn SaveParams>>,
    /// "Set Save Param" calls, in order: (save param, field index, value). Later reads in
    /// the same run see them.
    pub save_writes: Vec<(String, i32, i64)>,
    /// `OpSpawn` clones made so far by this actor (next `Effect::Spawn::serial`).
    pub spawned: u32,
    /// Setup blocks being run, innermost last.
    spawning: Vec<Spawning>,
}

/// Object a script reference points at, as far as the VM can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefTarget {
    Null,
    /// Engine object by name-hash path (`my`...).
    Engine(Vec<u32>),
    /// The actor's `serial`-th `OpSpawn` clone (`Effect::Spawn`).
    Spawned(u32),
    /// Something only the live engine knows (macro output...).
    Unknown,
}

/// Name hash of the engine's `null` reference.
const NULL_HASH: u32 = 0x7707_4ba4;
/// Name hash of `my`, the actor running the script.
const MY_HASH: u32 = 0x4c2d_fe93;

/// Object an effect acts on: an engine object of this actor, a clone being set up
/// (index into `Vm::spawning`, key as the clone's own `my` path), or an earlier clone
/// through a reference variable (`serial`, key as its `my` path).
enum Target {
    Engine(Vec<u32>),
    Clone(usize, Vec<u32>),
    Spawned(u32, Vec<u32>),
}

/// The save slot as scripts see it through `global.bld` "System - Save - Get Save Param"
/// (`param` = save param name, e.g. `"village generic vars"`). `None`: not in the save.
pub trait SaveParams: Send + Sync + std::fmt::Debug {
    fn get(&self, param: &str, index: i32) -> Option<i64>;
}

const GET_SAVE_PARAM: &str = "System - Save - Get Save Param";
const SET_SAVE_PARAM: &str = "Set Save Param";

/// Engine action requested by a script op.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// `OpSlideValue` at op `op` started: `lhs` goes `from` -> `to` over `seconds`,
    /// shaped by `ease_out` / `ease_in` (fractions of the slide, `ease_curve`).
    Slide {
        op: usize,
        lhs: Vec<Ref>,
        from: f32,
        to: f32,
        seconds: f32,
        ease_out: f32,
        ease_in: f32,
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
    /// `OpStartSequence` of state `state` on the engine sequence group `group`
    /// (`[ANIMATION_PALETTE]`: `my` plays `tfbAnimationState` `state`).
    StartSequence { group: Vec<u32>, state: i32 },
    /// `OpTurnTo` of the engine object `target`: face `facing` degrees.
    TurnTo { target: Vec<u32>, facing: f32 },
    /// Write of `value` to the engine field `key` (a clone's own `my` path).
    Set { key: Vec<u32>, value: Value },
    /// `effect` (targets as the clone's own `my` paths) on the actor's `serial`-th clone,
    /// reached through a reference variable after its setup block (`"My Lid".tint.alpha`).
    ForClone { serial: u32, effect: Box<Effect> },
    /// `OpSpawn` at op `op`: clone `template` at the engine object `at`, facing `facing`
    /// degrees. `serial` numbers the actor's spawns (`RefTarget::Spawned`). The setup block's
    /// writes to the clone are in `seed` (keys as the clone's own `my` paths) and its
    /// effects on the clone in `setup` (targets `[MY]`, ...).
    Spawn {
        op: usize,
        template: SpawnTemplate,
        at: Vec<u32>,
        facing: Option<f32>,
        serial: u32,
        seed: HashMap<Vec<u32>, Value>,
        setup: Vec<Effect>,
    },
}

/// A clone whose `OpSpawn` setup block is running.
#[derive(Debug, Clone)]
struct Spawning {
    op: usize,
    serial: u32,
    seed: HashMap<Vec<u32>, Value>,
    setup: Vec<Effect>,
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
                    Op::CheckValue { .. } | Op::LoopValue { .. } | Op::FindVariable { .. } => {
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
            globals: HashMap::new(),
            unbound: Vec::new(),
            rng: 0,
            behavior: None,
            refs: HashMap::new(),
            save: None,
            save_writes: Vec::new(),
            spawned: 0,
            spawning: Vec::new(),
        }
    }

    /// `null` / engine handles resolve to themselves; script reference variables to what
    /// `OpSetReference` stored (members of a `null` reference are `null`).
    fn ref_target(&self, path: &[Ref]) -> RefTarget {
        if let Some(k) = self.set.engine_key(path) {
            return match k[..] {
                [NULL_HASH] => RefTarget::Null,
                _ => RefTarget::Engine(k),
            };
        }
        let Ok(p) = self.canon(path) else {
            return RefTarget::Unknown;
        };
        if let Some((i, k)) = self.clone_key(&p) {
            if k == [MY_HASH] {
                return RefTarget::Spawned(self.spawning[i].serial);
            }
        }
        if let Some(t) = self.refs.get(&p) {
            return t.clone();
        }
        match p.first() {
            Some(Ref::Obj(_)) if self.refs.get(&p[..1]) == Some(&RefTarget::Null) => {
                RefTarget::Null
            }
            _ => RefTarget::Unknown,
        }
    }

    /// A path through an `OpSpawn` whose setup block is running means the clone
    /// (op +0x24, `0x8007c438`): its index in `spawning` and the clone's own `my` path.
    fn clone_key(&self, p: &[Ref]) -> Option<(usize, Vec<u32>)> {
        let Some(Ref::Obj(o)) = p.first() else {
            return None;
        };
        let i = self.spawning.iter().rposition(|s| s.op == *o)?;
        let rest = match &p[1..] {
            [] => Vec::new(),
            rest => self.set.engine_key(rest)?,
        };
        Some((i, [MY_HASH].into_iter().chain(rest).collect()))
    }

    /// A path through a reference variable that holds one of this actor's clones
    /// (`OpSetReference "My Lid" = @spawn`): the clone's serial and own `my` path.
    fn spawned_key(&self, p: &[Ref]) -> Option<(u32, Vec<u32>)> {
        let first @ Ref::Obj(_) = p.first()? else {
            return None;
        };
        let Some(RefTarget::Spawned(serial)) = self.refs.get(std::slice::from_ref(first)) else {
            return None;
        };
        let rest = match &p[1..] {
            [] => Vec::new(),
            rest => self.set.engine_key(rest)?,
        };
        Some((*serial, [MY_HASH].into_iter().chain(rest).collect()))
    }

    fn target(&self, path: &[Ref]) -> Option<Target> {
        if let Some(k) = self.set.engine_key(path) {
            return Some(Target::Engine(k));
        }
        let p = self.canon(path).ok()?;
        if let Some((i, k)) = self.clone_key(&p) {
            return Some(Target::Clone(i, k));
        }
        self.spawned_key(&p).map(|(s, k)| Target::Spawned(s, k))
    }

    fn emit(&mut self, t: Target, effect: impl FnOnce(Vec<u32>) -> Effect) {
        match t {
            Target::Engine(k) => self.effects.push(effect(k)),
            Target::Clone(i, k) => self.spawning[i].setup.push(effect(k)),
            Target::Spawned(serial, k) => self.effects.push(Effect::ForClone {
                serial,
                effect: Box::new(effect(k)),
            }),
        }
    }

    /// `key` (a `my` path) as script refs of this set, for a slider the clone runs.
    fn my_refs(&self, key: &[u32]) -> Option<Vec<Ref>> {
        key.iter()
            .map(|&id| {
                (0..self.set.handle_ids.len())
                    .find(|&i| {
                        self.set.handle_ids[i] == id
                            && self.set.engine_key(&[Ref::Handle(i as u32)]).is_some()
                    })
                    .map(|i| Ref::Handle(i as u32))
            })
            .collect()
    }

    /// Evaluate `rhs` against the current variable store.
    pub fn eval_rhs(&mut self, rhs: &Rhs) -> Result<Value, VmError> {
        self.eval(rhs)
    }

    /// Values written to `global.bld` exports so far, by export name.
    pub fn global_writes(&self) -> HashMap<String, Value> {
        let Some(global) = self.global else {
            return HashMap::new();
        };
        self.vars
            .iter()
            .filter_map(|(path, v)| match path[..] {
                [Ref::Handle(h)] => Some((self.set.global_handle(h, global)?.name.clone(), *v)),
                _ => None,
            })
            .collect()
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
    /// `global.bld` "System - Save - Get Save Param" / "Set Save Param" (`val`, `field
    /// index`, `save param` = `$648."<param>"`), with the call-site params already bound.
    fn save_param_call(&mut self, get: bool, site: &[(usize, u16, Option<&Rhs>)]) {
        let by_name = |n: &str| {
            site.iter()
                .find(|p| self.set.var_name(p.0) == n)
                .map(|p| p.0)
        };
        let (Some(val), Some(index), Some(param)) = (
            by_name("val"),
            by_name("field index"),
            by_name("save param"),
        ) else {
            return;
        };
        let Some(name) = self.set.opaque_operands.get(&param).and_then(|ops| {
            ops.iter().find_map(|(_, o)| match o {
                OpaqueOperand::Path { stack, path } if stack == "RHSReferenceStack" => {
                    match path.last()? {
                        Ref::Obj(o) => Some(self.set.var_name(*o).to_string()),
                        Ref::Handle(_) => None,
                    }
                }
                _ => None,
            })
        }) else {
            return;
        };
        let index = self.read(&[Ref::Obj(index)]).as_f32() as i32;
        if get {
            let written = self
                .save_writes
                .iter()
                .rev()
                .find(|w| w.0 == name && w.1 == index)
                .map(|w| w.2);
            let v = written.or_else(|| self.save.as_ref()?.get(&name, index));
            if let Some(v) = v {
                self.write(&[Ref::Obj(val)], Value::Int(v as i32));
            }
        } else {
            let v = self.read(&[Ref::Obj(val)]).as_f32() as i64;
            self.save_writes.push((name, index, v));
        }
    }

    fn use_macro(&mut self, si: usize, at: usize, target: Option<usize>) -> Result<usize, VmError> {
        let (_, after) = self.params(si, at);
        let site = self.param_rhs(si, at);
        let Some(&(tsi, toi)) = target.and_then(|t| self.index.macros.get(&t)) else {
            // Defined in another file: every parameter keeps its literal.
            let what = format!("macro @{:x}", target.unwrap_or(0));
            if !self.unbound.contains(&what) {
                self.unbound.push(what);
            }
            for &(var, _, rhs) in &site {
                match rhs {
                    Some(rhs) => {
                        let v = self.eval(rhs)?;
                        self.write(&[Ref::Obj(var)], v);
                    }
                    // Reference params: such macros look up live objects (`Combat - Check
                    // Hurt` loops over `hurts`); with none, they come back null.
                    None => {
                        self.refs.insert(vec![Ref::Obj(var)], RefTarget::Null);
                    }
                }
            }
            let name = target.and_then(|t| self.set.macro_names.get(&t));
            if let Some(name @ (GET_SAVE_PARAM | SET_SAVE_PARAM)) = name.map(String::as_str) {
                self.save_param_call(name == GET_SAVE_PARAM, &site);
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
                Op::SetRef { lhs, rhs, .. } => {
                    let t = self.ref_target(rhs);
                    if let Ok(p) = self.canon(lhs) {
                        self.refs.insert(p, t);
                    }
                }
                Op::CheckRef {
                    rel,
                    desc,
                    lhs,
                    rhs,
                } => {
                    let known = desc.starts_with("OpCheckReference") && matches!(rel, 1 | 5);
                    let holds = match (self.ref_target(lhs), self.ref_target(rhs)) {
                        (a, b) if known && a != RefTarget::Unknown && b != RefTarget::Unknown => {
                            (a == b) == (*rel == 1)
                        }
                        _ => *rel == 5,
                    };
                    if !holds {
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
                // `0x8008f6ac`: eases are stored as `0.02 * value` (slider +0x64 / +0x68),
                // 0 when they do not evaluate.
                Op::SlideValue {
                    lhs,
                    target,
                    seconds,
                    ease_out,
                    ease_in,
                } => {
                    let from = self.read(lhs).as_f32();
                    let to = self.eval(target)?.as_f32();
                    let mut opt = |r: &Option<Rhs>| -> Result<f32, VmError> {
                        Ok(match r {
                            Some(r) => self.eval(r)?.as_f32(),
                            None => 0.0,
                        })
                    };
                    let seconds = opt(seconds)?;
                    let ease_out = 0.02 * opt(ease_out)?;
                    let ease_in = 0.02 * opt(ease_in)?;
                    let slide = |lhs| Effect::Slide {
                        op: ins.offset,
                        lhs,
                        from,
                        to,
                        seconds,
                        ease_out,
                        ease_in,
                    };
                    // A clone's field slides on the clone (it runs the slider itself).
                    match self.target(lhs) {
                        Some(Target::Clone(i, k)) => {
                            if let Some(refs) = self.my_refs(&k) {
                                self.spawning[i].setup.push(slide(refs));
                            }
                        }
                        Some(Target::Spawned(serial, k)) => {
                            if let Some(refs) = self.my_refs(&k) {
                                self.effects.push(Effect::ForClone {
                                    serial,
                                    effect: Box::new(slide(refs)),
                                });
                            }
                        }
                        _ => {
                            if let Ok(lhs) = self.canon(lhs) {
                                self.effects.push(slide(lhs));
                            }
                        }
                    }
                }
                Op::Displace {
                    target,
                    len,
                    heading,
                    pitch,
                    ..
                } => {
                    if let Some(target) = self.target(target) {
                        let mut f = |r: &Option<Rhs>| -> Result<f32, VmError> {
                            Ok(match r {
                                Some(r) => self.eval(r)?.as_f32(),
                                None => 0.0,
                            })
                        };
                        let (len, heading, pitch) = (f(len)?, f(heading)?, f(pitch)?);
                        self.emit(target, |target| Effect::Displace {
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
                    if let Some(target) = self.target(target) {
                        if self.unbound.is_empty() {
                            self.emit(target, |target| Effect::Remove { target });
                        }
                    }
                }
                // `0x80087208`: an unresolved group skips the setup block, else the block
                // runs (`looping` and friends on the sequence).
                Op::StartSequence { group, index } => match self.set.engine_key(group) {
                    Some(group) => {
                        let state = match index {
                            Some(r) => self.eval(r)?.as_f32() as i32,
                            None => 0,
                        };
                        self.effects.push(Effect::StartSequence { group, state });
                    }
                    None => pc += skip,
                },
                // `0x8007d418`: `pc += br` when the target does not resolve.
                Op::TurnTo { facing, target } => match self.target(target) {
                    Some(target) => {
                        let facing = self.eval(facing)?.as_f32();
                        self.emit(target, |target| Effect::TurnTo { target, facing });
                    }
                    None => pc += skip,
                },
                // `0x8006fad0`: only swaps the current behaviour; it runs from next tick.
                Op::SetBehavior { target } => {
                    if target.is_some() {
                        self.behavior = *target;
                    }
                }
                // Behaviour blocks run only when current (`ScriptProgram::tick`).
                Op::Behavior { .. } => pc += skip,
                // Blocks of ops that need live objects (find variable, spawn, for each):
                // skipped, as if nothing was found.
                // `0x8007c6bc`: an unresolved template or position skips the setup block.
                // Templates behind references (variables, macro params, other spawns) and
                // `global.bld` / engine handles are not known here.
                Op::Spawn {
                    actor, at, facing, ..
                } => {
                    let (Some(template), Some(at)) = (actor, self.set.engine_key(at)) else {
                        pc += skip;
                        continue;
                    };
                    // `0x80091790`: a literal facing evaluates too (most spawns pass 0).
                    let facing = match facing {
                        Some(r) => Some(self.eval(r)?.as_f32()),
                        None => None,
                    };
                    let serial = self.spawned;
                    self.spawned += 1;
                    self.spawning.push(Spawning {
                        op: ins.offset,
                        serial,
                        seed: HashMap::new(),
                        setup: Vec::new(),
                    });
                    let end = (pc + skip).min(to);
                    let flow = self.run(si, pc, end);
                    let done = self.spawning.pop().expect("pushed above");
                    self.effects.push(Effect::Spawn {
                        op: ins.offset,
                        template: template.clone(),
                        at,
                        facing,
                        serial,
                        seed: done.seed,
                        setup: done.setup,
                    });
                    if let Flow::Return = flow? {
                        return Ok(Flow::Return);
                    }
                    pc = end;
                }
                Op::FindVariable { .. } | Op::Other(_) => pc += skip,
                Op::Interface | Op::Param { .. } | Op::CreateVariable { .. } | Op::PreScript => {}
            }
        }
        Ok(Flow::Next)
    }

    fn eval(&mut self, rhs: &Rhs) -> Result<Value, VmError> {
        let a = self.operand(&rhs.a)?;
        let v = match &rhs.b {
            Some(b) if rhs.arith >= 0 => {
                let b = self.operand(b)?;
                arith(a, rhs.arith, b)?
            }
            _ => a,
        };
        self.result_func(rhs.func, v)
    }

    /// `_resultFunc` table at `0x80506a38` (`id - 1`), applied after the arithmetic.
    fn result_func(&mut self, func: u32, v: Value) -> Result<Value, VmError> {
        use Value::{Float, Int};
        let f = v.as_f32();
        let round = |x: f32| Int((x + if x < 0.0 { -0.5 } else { 0.5 }) as i32);
        Ok(match (func, v) {
            (0, _) => v,
            // `0x80071404` random: int `[0, v]` (`rng & 0x7fffffff` mod `v ± 1`), float `[0, v)`.
            (1, Int(x)) => {
                let d = x.wrapping_add(if x < 0 { -1 } else { 1 });
                Int(((self.next_rand() & 0x7fff_ffff) as i32).wrapping_rem(d))
            }
            (1, Float(x)) => Float(x * (self.next_rand() as f32 / 4_294_967_296.0)),
            (2, _) => Int(f as i32),
            (3, _) => round(f),
            (4, _) => round(f.floor()),
            (5, _) => round(f.ceil()),
            (6, Int(x)) => Int(x.wrapping_abs()),
            (6, Float(x)) => Float(x.abs()),
            // `0x8007189c`: zero (float within 1e-9) stays 0, else ±1.
            (7, Float(x)) if x.abs() < 1e-9 => Int(0),
            (7, _) if f == 0.0 => Int(0),
            (7, _) => Int(if f < 0.0 { -1 } else { 1 }),
            (8, Int(x)) => Int(x.wrapping_mul(x)),
            (8, Float(x)) => Float(x * x),
            // `0x8007197c`: fmod 360 into (-180, 180].
            (10, _) => {
                let a = f % 360.0;
                Float(if a > 180.0 {
                    a - 360.0
                } else if a <= -180.0 {
                    a + 360.0
                } else {
                    a
                })
            }
            (11, _) => Float(f.to_radians().sin()),
            _ => return Err(VmError::Unsupported(format!("result func {func}"))),
        })
    }

    /// ponytail: the game draws from one global Mersenne Twister (`r13-0x39f0`) shared
    /// by every system, which cannot be replayed; this is a per-VM SplitMix64 the caller
    /// seeds (`rng`), deterministic per actor. Exact only if the global stream is ever modelled.
    fn next_rand(&mut self) -> u32 {
        self.rng = self.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 32) as u32
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
                Some(Op::FindVariable { name, target }) => {
                    // Try looking up the variable by name in globals
                    if let Some(v) = self.globals.get(name) {
                        return Err(match v {
                            Value::Int(i) => *i,
                            Value::Float(f) => *f as i32,
                        });
                    }
                    if let Some(base) = name.strip_suffix(" tag") {
                        if let Some(v) = self.globals.get(base) {
                            return Err(match v {
                                Value::Int(i) => *i,
                                Value::Float(f) => *f as i32,
                            });
                        }
                    }
                    // Try targeting path
                    if !target.is_empty() {
                        let mut resolved: Vec<Ref> =
                            target.iter().chain(&p[1..]).copied().collect();
                        if let Ok(c) = self.canon(&resolved) {
                            resolved = c;
                        }
                        if self.vars.contains_key(&resolved) {
                            p = resolved;
                            continue;
                        }
                    }
                    p = p[1..].to_vec();
                    if p.is_empty() {
                        return Err(0);
                    }
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
        if let Some(v) = self
            .clone_key(&path)
            .and_then(|(i, k)| self.spawning[i].seed.get(&k))
        {
            return *v;
        }
        if let Some(v) = self.set.engine_key(&path).and_then(|k| self.engine.get(&k)) {
            return *v;
        }
        let init = match path[..] {
            [Ref::Obj(o)] => {
                let name = self.set.var_name(o);
                if let Some(v) = self.globals.get(name) {
                    return *v;
                }
                return self
                    .set
                    .vars
                    .get(&o)
                    .and_then(|v| v.init)
                    .unwrap_or(Value::Int(0));
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
            if let Some((i, k)) = self.clone_key(&p) {
                self.spawning[i].seed.insert(k, v);
                return;
            }
            // Kept here too, so the script reads back what it wrote.
            if let Some((serial, key)) = self.spawned_key(&p) {
                self.effects.push(Effect::ForClone {
                    serial,
                    effect: Box::new(Effect::Set { key, value: v }),
                });
            }
            match self.set.engine_key(&p) {
                Some(k) => {
                    self.engine.insert(k, v);
                }
                None => {
                    if let [Ref::Obj(o)] = p[..] {
                        let name = self.set.var_name(o);
                        if self.globals.contains_key(name) {
                            self.globals.insert(name.to_string(), v);
                        }
                    }
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

/// `global.bld` tuning set at boot by `Init_Setup_Minions.ai` "Minion - Tune Minions"
/// (`global.bld` exports such as `HP_Minion_Medium` are 1.0 placeholders until then).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MinionTuning {
    pub values: HashMap<String, Value>,
}

impl MinionTuning {
    pub fn from_scripts(init_setup: &ScriptSet, global: &ScriptSet) -> Result<Self, VmError> {
        let mut vm = Vm::new(init_setup, Some(global));
        vm.call("Minion - Tune Minions", &[])?;
        Ok(Self {
            values: vm.global_writes(),
        })
    }

    pub fn get(&self, name: &str) -> Option<f32> {
        self.values.get(name).map(|v| v.as_f32())
    }
}

/// Level-1 max HP of a Skylander: the `<id>_HP_local` value its `<id> <Name> Def` macro
/// passes as `"max hp"` (Spyro: `HP_Minion_Medium + HP_Minion_NudgeAmount` = 270 + 10).
/// The Eons Elite branch (`* "Elite - HP Mult"`, Gill Grunt) is skipped: a normal toy.
/// Villains without an `_HP_local` pass `"max hp"` directly (Chompy Mage: 1000). `None`
/// when that reads a dangling `^` field (Dr. Krankcase `"^Ally - HP High"`, no value).
pub fn hero_max_hp(
    character: &ScriptSet,
    global: &ScriptSet,
    tuning: &MinionTuning,
) -> Option<f32> {
    let rhs = local_stat_sets(character)
        .find(|&(v, _)| character.var_name(v).ends_with("_HP_local"))
        .map(|(_, rhs)| rhs)
        .or_else(|| {
            character
                .scripts
                .iter()
                .flat_map(|s| &s.ops)
                .find_map(|i| match &i.op {
                    Op::Param {
                        var,
                        rhs: Some(rhs),
                        ..
                    } if character.var_name(*var) == "max hp" => Some(rhs),
                    _ => None,
                })
                .filter(|rhs| !character.rhs_str(rhs).contains('^'))
        })?;
    tuned_vm(character, global, tuning)
        .eval_rhs(rhs)
        .ok()
        .map(|v| v.as_f32())
}

/// Assignments to the character's `<id>_*_local` stat variables, in file order, without
/// the Eons Elite branch (`* "Elite - HP Mult"`): a normal toy.
fn local_stat_sets(character: &ScriptSet) -> impl Iterator<Item = (usize, &Rhs)> {
    character
        .scripts
        .iter()
        .flat_map(|s| &s.ops)
        .filter_map(move |i| match &i.op {
            Op::SetValue { lhs, rhs } => match lhs[..] {
                [Ref::Obj(v)]
                    if character.var_name(v).ends_with("_local")
                        && !character.rhs_str(rhs).contains("Elite") =>
                {
                    Some((v, rhs))
                }
                _ => None,
            },
            _ => None,
        })
}

/// `Vm` over `character` with its `global.bld` handles seeded from `tuning` and its
/// `*_local` stats assigned (their `ValueInfo` initial values can be stale: Gill Grunt's
/// `014_Movement_Speed_local` is 18 until the Def macro sets `Speed_Slow`).
fn tuned_vm<'a>(character: &'a ScriptSet, global: &'a ScriptSet, tuning: &MinionTuning) -> Vm<'a> {
    let mut vm = Vm::new(character, Some(global));
    for h in 0..character.handle_ids.len() as u32 {
        if let Some(v) = character
            .global_handle(h, global)
            .and_then(|info| tuning.values.get(&info.name))
        {
            vm.set_var(vec![Ref::Handle(h)], *v);
        }
    }
    let mut seen = Vec::new();
    for (var, rhs) in local_stat_sets(character) {
        if !seen.contains(&var) {
            seen.push(var);
            if let Ok(v) = vm.eval_rhs(rhs) {
                vm.set_var(vec![Ref::Obj(var)], v);
            }
        }
    }
    vm
}

/// Movement a Skylander's script passes to global `Minion.ai` "Minion - setup pre
/// descriptions", which writes them to `my.movement` (ft/s, deg/s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeroMovement {
    pub ground_max_speed: f32,
    pub air_max_speed: f32,
    pub ground_turn_rate: f32,
    pub air_turn_rate: f32,
}

/// Spyro: `default speed` 24 (`$acf`), `default air speed` 25, turn rates `$ade`.
pub fn hero_movement(
    character: &ScriptSet,
    global: &ScriptSet,
    tuning: &MinionTuning,
) -> Option<HeroMovement> {
    let mut vm = tuned_vm(character, global, tuning);
    let mut param = |name: &str| -> Option<f32> {
        let rhs = character
            .scripts
            .iter()
            .flat_map(|s| &s.ops)
            .find_map(|i| match &i.op {
                Op::Param {
                    var,
                    rhs: Some(rhs),
                    ..
                } if character.var_name(*var) == name => Some(rhs),
                _ => None,
            })?;
        let v = vm.eval_rhs(rhs).ok()?.as_f32();
        (v > 0.0).then_some(v)
    };
    Some(HeroMovement {
        ground_max_speed: param("default speed")?,
        air_max_speed: param("default air speed")?,
        ground_turn_rate: param("default ground turn rate")?,
        air_turn_rate: param("default air turn rate")?,
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
    fn vm_result_funcs() {
        let s = script_set(Vec::new(), &[]);
        let mut vm = Vm::new(&s, None);
        let mut f = |func, v| vm.result_func(func, v).unwrap();
        assert_eq!(f(2, F(-2.7)), I(-2));
        assert_eq!(f(3, F(-2.5)), I(-3));
        assert_eq!(f(4, F(-2.1)), I(-3));
        assert_eq!(f(5, F(2.1)), I(3));
        assert_eq!(f(6, I(-4)), I(4));
        assert_eq!(f(7, F(-0.5)), I(-1));
        assert_eq!(f(7, I(0)), I(0));
        assert_eq!(f(8, I(-3)), I(9));
        assert_eq!(f(10, F(270.0)), F(-90.0));
        assert_eq!(f(10, F(-180.0)), F(180.0));
        assert!(matches!(f(11, F(90.0)), Value::Float(x) if (x - 1.0).abs() < 1e-6));
        for _ in 0..200 {
            assert!(matches!(f(1, I(99)), Value::Int(0..=99)));
            assert!(matches!(f(1, F(2.0)), Value::Float(x) if (0.0..2.0).contains(&x)));
        }
        assert!(vm.result_func(9, F(4.0)).is_err());
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
    fn vm_save_param_calls() {
        #[derive(Debug)]
        struct Slot;
        impl SaveParams for Slot {
            fn get(&self, param: &str, index: i32) -> Option<i64> {
                (param == "village generic vars" && index == 33).then_some(1)
            }
        }
        let (get, set_m, vars) = (0x900, 0x901, 0x400);
        // One call site: use <target> (val, field index, save param = $648.vars), then
        // copy val into `out`.
        let call = |base: usize, target: usize, val: i32, index: i32, out: usize| {
            vec![
                ins(
                    base,
                    4,
                    Op::UseMacro {
                        target: Some(target),
                    },
                ),
                ins(base + 1, 3, Op::Interface),
                param(base + 2, 0, Some(lit(I(val)))),
                param(base + 3, 0, Some(lit(I(index)))),
                param(base + 4, 0, None),
                ins(
                    base + 5,
                    0,
                    set(
                        out,
                        Rhs {
                            a: rd(base + 2),
                            arith: -1,
                            b: None,
                            func: 0,
                        },
                    ),
                ),
            ]
        };
        let mut ops = vec![
            ins(
                1,
                19,
                Op::DefineMacro {
                    name: "main".into(),
                },
            ),
            ins(2, 0, Op::Interface),
        ];
        ops.extend(call(0x10, get, -1, 33, 0x500));
        ops.extend(call(0x20, set_m, 7, 2, 0x501));
        ops.extend(call(0x30, get, -1, 2, 0x502));
        let mut names = vec![(vars, "village generic vars")];
        for b in [0x10, 0x20, 0x30] {
            names.extend([
                (b + 2, "val"),
                (b + 3, "field index"),
                (b + 4, "save param"),
            ]);
        }
        let mut s = script_set(ops, &names);
        for b in [0x10, 0x20, 0x30] {
            s.opaque_operands.insert(
                b + 4,
                vec![(
                    0x30,
                    OpaqueOperand::Path {
                        stack: "RHSReferenceStack".into(),
                        path: vec![Ref::Handle(0), Ref::Obj(vars)],
                    },
                )],
            );
        }
        s.macro_names.insert(get, GET_SAVE_PARAM.into());
        s.macro_names.insert(set_m, SET_SAVE_PARAM.into());

        // Without save data `val` keeps its literal (-1).
        let mut vm = Vm::new(&s, None);
        vm.call("main", &[]).unwrap();
        assert_eq!(vm.read(&[Ref::Obj(0x500)]), I(-1));
        assert_eq!(vm.read(&[Ref::Obj(0x502)]), I(7), "own write is read back");

        let mut vm = Vm::new(&s, None);
        vm.save = Some(std::sync::Arc::new(Slot));
        vm.call("main", &[]).unwrap();
        assert_eq!(vm.read(&[Ref::Obj(0x500)]), I(1));
        assert_eq!(vm.save_writes, [("village generic vars".to_string(), 2, 7)]);
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
    fn vm_op_find_variable_lookup() {
        let fv = 0x20;
        let dest = 0x21;
        let mut s = script_set(
            vec![
                ins(
                    1,
                    5,
                    Op::DefineMacro {
                        name: "test_find_var".into(),
                    },
                ),
                ins(2, 1, Op::Interface),
                param(dest, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(
                    fv,
                    0,
                    Op::FindVariable {
                        name: "Levels_Completed tag".into(),
                        target: vec![Ref::Handle(127)],
                    },
                ),
                ins(
                    3,
                    0,
                    set(
                        dest,
                        Rhs {
                            a: rd(fv),
                            arith: -1,
                            b: None,
                            func: 0,
                        },
                    ),
                ),
            ],
            &[(dest, "out")],
        );
        s.vars.insert(
            fv,
            VarInfo {
                name: "".into(),
                type_name: "OpFindVariable".into(),
                init: None,
            },
        );
        let mut vm = Vm::new(&s, None);
        vm.globals
            .insert("Levels_Completed".to_string(), Value::Int(7));
        assert_eq!(out_of(vm.call("test_find_var", &[]).unwrap(), "out"), I(7));
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
                        lhs: Vec::new(),
                        rhs: Vec::new(),
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
    fn rom_spawn_ops() {
        let Some(hub) = read_bld("level", "Level_Hub_2014_01").and_then(|d| ScriptSet::parse(&d))
        else {
            return;
        };
        let ops = || hub.scripts.iter().flat_map(|s| &s.ops);
        assert!(!ops().any(|i| i.op == Op::Other("OpSpawn".into())));
        let spawns: Vec<_> = ops()
            .filter_map(|i| match &i.op {
                Op::Spawn {
                    template, actor, ..
                } => Some((template, actor)),
                _ => None,
            })
            .collect();
        assert_eq!(spawns.len(), 485);
        let coin = hub.script("2D_Coin.ai").unwrap();
        let (br, actor, at) = coin
            .ops
            .iter()
            .find_map(|i| match &i.op {
                Op::Spawn { actor, at, .. } => Some((i.br, actor.clone().unwrap(), at)),
                _ => None,
            })
            .unwrap();
        assert_eq!(br, 3);
        assert_eq!(
            (actor.ty.as_str(), actor.name.as_str(), actor.id),
            ("tfbActorInfo", "Coin_Art_01", 0)
        );
        assert_eq!(hub.engine_key(at), Some(vec![name_hash("my")]));
        assert_eq!(actor.script, None);
        // Clones keep the template's script: `Coin_00` runs `2D_Coin.ai`.
        assert!(spawns
            .iter()
            .any(|(_, a)| a.as_ref().is_some_and(|a| a.name == "Coin_00"
                && a.script
                    .as_deref()
                    .is_some_and(|s| s.ends_with("/2D_Coin.ai")))));
        // The template is a reference path: also handles and `object.field` paths.
        assert!(spawns
            .iter()
            .any(|(t, a)| a.is_none() && matches!(t[..], [Ref::Handle(_)])));
        assert!(spawns.iter().any(|(t, _)| t.len() > 1));
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
    fn rom_hero_max_hp_from_minion_tuning() {
        let (Some(g), Some(init), Some(spyro), Some(gill)) = (
            read_bld("permanent", "global").and_then(|b| ScriptSet::parse(&b)),
            read_bld("character", "Init_Setup").and_then(|b| ScriptSet::parse(&b)),
            read_bld("character", "016_Spyro").and_then(|b| ScriptSet::parse(&b)),
            read_bld("character", "014_GillGrunt").and_then(|b| ScriptSet::parse(&b)),
        ) else {
            return;
        };
        let tuning = MinionTuning::from_scripts(&init, &g).unwrap();
        assert_eq!(tuning.get("HP_Minion_Medium"), Some(270.0));
        assert_eq!(tuning.get("HP_Minion_VeryLow"), Some(200.0));
        assert_eq!(tuning.get("HP_Minion_NudgeAmount"), Some(10.0));
        assert_eq!(hero_max_hp(&spyro, &g, &tuning), Some(280.0));
        assert_eq!(tuning.get("Speed_Med"), Some(24.0));
        assert_eq!(
            hero_movement(&spyro, &g, &tuning),
            Some(HeroMovement {
                ground_max_speed: 24.0,
                air_max_speed: 25.0,
                ground_turn_rate: 1440.0,
                air_turn_rate: 1440.0,
            })
        );
        // Villains: `"max hp"` passed directly; a dangling `^` field gives nothing.
        let parse = |n| {
            read_bld("character", n)
                .and_then(|b| ScriptSet::parse(&b))
                .unwrap()
        };
        let mage = parse("1001_ChompyMage");
        assert_eq!(hero_max_hp(&mage, &g, &tuning), Some(1000.0));
        assert_eq!(
            hero_movement(&mage, &g, &tuning).unwrap().ground_max_speed,
            24.0
        );
        assert_eq!(hero_max_hp(&parse("1002_Crankcase"), &g, &tuning), None);
        // `014_Movement_Speed_local` = `Speed_Slow`, not its stale initial 18.
        let gill_move = hero_movement(&gill, &g, &tuning).unwrap();
        assert_eq!(gill_move.ground_max_speed, 23.0);
        // Eons Elite branch (x3) is not taken.
        assert_eq!(hero_max_hp(&gill, &g, &tuning), Some(270.0));
    }

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
