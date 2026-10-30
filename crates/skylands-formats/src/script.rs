//! Alchemy visual-script (`tfbScriptInfo` / `OpCodeList`) decoder and a small evaluator for
//! pure value logic (macros, set/check value, arithmetic). Layout notes: `agent/SCRIPT_VM.md`.

use crate::igb::{IgbEndian, IgbModel, IgzFile};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
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
    /// `OpChangeMembership`: combine set `set` with `group` (`_combineOp` +0x20: 0 include,
    /// 1 exclude, 2 intersect_with, 3 be_replaced_by, 4 add, 5 exclude_all; enum table
    /// `0x805b5880`).
    ChangeMembership {
        set: Vec<Ref>,
        op: i32,
        group: Vec<Ref>,
    },
    /// `OpForEach`: run the `br` block once per member of `set`, the op meaning the member
    /// (+0x28 order: 0 forward, 1 backward, 2 randomly; enum table `0x805b58b0`).
    ForEach {
        set: Vec<Ref>,
        order: i32,
    },
    /// `OpFindSubSet` (`0x800901c8`): the members of `set` whose `field` compares `rel`
    /// (`RelOp`) to `rhs`; with none the `br` block is skipped, else it runs with the op
    /// meaning that subset. `+0x1c` = set path, a null entry, the member field path.
    FindSubSet {
        set: Vec<Ref>,
        field: Vec<Ref>,
        rel: i32,
        rhs: Option<Rhs>,
    },
    /// Anything else (movement, sound...): skipped by the evaluator.
    Other(String),
}

/// `ActorWaypoint` (0x38, no reflected fields): `+0x1c` position, likely in the owning
/// placement's space, and `+0x28` three angles (zero in the arena paths; scripts read
/// them as `orientation.facing / tilt / roll`, order not confirmed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Waypoint {
    pub local: [f32; 3],
    pub angles: [f32; 3],
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
    /// `CollisionInfo._weight` (+0xc4 -> +0x40), 0 without one.
    pub weight: f32,
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
    /// Per script: its `_masterVarList` variable objects (what `OpFindVariable` finds).
    pub master_vars: Vec<Vec<usize>>,
    /// Per `tfbActorInfo`: its tags (`_tagList` +0x18 `TagList`, `_initialTags` +0x1c count,
    /// +0x24 data), `OpParameter` variables every actor of that placement owns besides its
    /// script's (`"flight path"`, `"armored"`); `OpFindVariable` finds both.
    pub tags: HashMap<usize, Vec<usize>>,
    /// Reference tags' default (`OpParameter._RHS` +0x30): `null` or an engine handle.
    pub tag_refs: HashMap<usize, RefTarget>,
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
    /// `tfbActorInfo` objects in the file (what `OpSpawn ... at` can name).
    pub placements: HashSet<usize>,
    /// Every `tfbActorInfo` as an `OpSpawn` template, for spawns whose template is a
    /// reference (`spawn "Left 1"`, a macro param holding an enemy template).
    pub templates: HashMap<usize, SpawnTemplate>,
    /// `tfbActorInfo` +0xf4 `ActorWaypointList`s, by placement.
    pub waypoints: HashMap<usize, Vec<Waypoint>>,
    /// `OpSetReference._indexRHS` (+0x24) by op offset: the element of a list the
    /// reference takes (`.waypoints[i]`).
    pub ref_index: HashMap<usize, Rhs>,
    /// Record variables: typed (+0x24) by an `OpDefineStructure` (`"New Info"`, `"My Info"`),
    /// and `ScriptStructure`s. A record is an object a reference can point at; an unset
    /// variable of an engine type (`"defeated actor"`, `"Enemy - payload"`) is `null`.
    pub records: HashSet<usize>,
    /// `StringInfo` constants by offset (`RefTarget::String`).
    pub strings: HashSet<usize>,
    /// `OpUseMacro` ops whose target is a variable holding a macro (`ScriptGroupStack`
    /// path `"info"."Transport Type".<macro>`, `_editType` +0x18 = the base macro), by op.
    pub use_paths: HashMap<usize, Vec<Ref>>,
    /// `PlacementReference` objects: level-wide references (`Current_BattleMaster`) that
    /// one actor's `OpSetReference` sets and every script reads.
    pub placement_refs: HashSet<usize>,
    /// `ScriptSet` object -> its `+0x08` name (`global.bld` `$51a` = "Minions").
    pub set_names: HashMap<usize, String>,
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
    "OpFindSubSet",
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
    /// What an `OpSpawn` clone copies from the placement `a`.
    fn template(&self, a: usize) -> SpawnTemplate {
        SpawnTemplate {
            offset: a,
            ty: self.ty(a).into(),
            name: self.cstr(a + 0x08),
            id: self.u32(a + 0x20).unwrap_or(0),
            script: self
                .ptr(a + 0xa8)
                .filter(|&p| self.ty(p) == "tfbScriptInfo")
                .map(|p| self.cstr(p + 0x08)),
            scale: [0, 4, 8].map(|k| f32::from_bits(self.u32(a + 0xb8 + k).unwrap_or(0x42c8_0000))),
            tint: self
                .data
                .get(a + 0xc8..a + 0xcc)
                .map_or([0x80, 0x80, 0x80, 0xff], |b| [b[0], b[1], b[2], b[3]]),
            weight: self
                .ptr(a + 0xc4)
                .filter(|&c| self.ty(c) == "CollisionInfo")
                .and_then(|c| self.u32(c + 0x40))
                .map_or(0.0, f32::from_bits),
        }
    }

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
            "OpChangeMembership" => Op::ChangeMembership {
                set: self.lhs(o + 0x1c),
                op: self.u32(o + 0x20).unwrap_or(0) as i32,
                group: self.lhs(o + 0x24),
            },
            "OpFindSubSet" => {
                let words = self.ptr(o + 0x1c).map(|s| self.list(s)).unwrap_or_default();
                let split = words.iter().position(|&w| w == 0).unwrap_or(words.len());
                let refs = |ws: &[u32]| -> Vec<Ref> {
                    ws.iter()
                        .filter_map(|&w| {
                            if w & 0x8000_0000 != 0 {
                                Some(Ref::Handle(w & 0x7FFF_FFFF))
                            } else {
                                self.decode_ptr(w).map(Ref::Obj)
                            }
                        })
                        .collect()
                };
                Op::FindSubSet {
                    set: refs(&words[..split]),
                    field: refs(words.get(split + 1..).unwrap_or_default()),
                    rel: self.u32(o + 0x20).unwrap_or(0) as i32,
                    rhs: self.rhs_at(o + 0x24),
                }
            }
            "OpForEach" => Op::ForEach {
                set: self.lhs(o + 0x20),
                order: self.u32(o + 0x28).unwrap_or(0) as i32,
            },
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
                    [Ref::Obj(a)] if !self.ty(a).starts_with("Op") => Some(self.template(a)),
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
                // A tag: `_varName` +0x1c, default `_RHS` +0x30 (a literal for values).
                "OpParameter" => VarInfo {
                    name: r.cstr(o.offset + 0x1c),
                    type_name: t.into(),
                    init: r.rhs_at(o.offset + 0x30).and_then(|x| match x.a {
                        Operand::Lit(v) if x.b.is_none() => Some(v),
                        _ => None,
                    }),
                },
                _ => continue,
            };
            vars.insert(o.offset, info);
        }
        let of_type = |t: &str| {
            objects
                .iter()
                .filter(|o| o.type_name.as_deref() == Some(t))
                .map(|o| o.offset)
                .collect()
        };
        let placements: HashSet<usize> = of_type("tfbActorInfo");
        let templates = placements.iter().map(|&a| (a, r.template(a))).collect();
        let f32_at = |o: usize| r.u32(o).map_or(0.0, f32::from_bits);
        let waypoints = placements
            .iter()
            .filter_map(|&a| {
                let list = r
                    .ptr(a + 0xf4)
                    .filter(|&l| r.ty(l) == "ActorWaypointList")?;
                let wps = r
                    .list(list)
                    .into_iter()
                    .filter_map(|w| r.decode_ptr(w))
                    .map(|w| Waypoint {
                        local: [0x1c, 0x20, 0x24].map(|k| f32_at(w + k)),
                        angles: [0x28, 0x2c, 0x30].map(|k| f32_at(w + k)),
                    })
                    .collect();
                Some((a, wps))
            })
            .collect();
        let ref_index = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("OpSetReference"))
            .filter_map(|o| Some((o.offset, r.rhs_at(o.offset + 0x24)?)))
            .collect();
        // A variable's type (+0x24): an `OpDefineStructure` (a record), an
        // `OpDefineMacro` (holds a macro) or an engine handle (an engine class or value).
        // Its container (+0x20) `ScriptReference` makes it a reference to such a record
        // (`ArenaRules_AM2.ai` "Selected Mod"), `ScriptStructure` the record itself.
        let structure = |o: usize| {
            r.ptr(o + 0x24)
                .is_some_and(|t| r.ty(t) == "OpDefineStructure")
        };
        let records = objects
            .iter()
            .filter(|o| match o.type_name.as_deref() {
                Some("ScriptStructure") => true,
                Some("OpCreateVariable") => {
                    structure(o.offset) && r.ext(o.offset + 0x20) != "ScriptReference"
                }
                Some("OpMacroParameter") => structure(o.offset),
                _ => false,
            })
            .map(|o| o.offset)
            .collect();
        let use_paths = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("OpUseMacro"))
            .filter_map(|o| {
                let path = r.lhs(o.offset + 0x1c);
                let literal = path
                    .iter()
                    .any(|p| matches!(p, Ref::Obj(x) if r.ty(*x).starts_with("OpDefineMacro")));
                (!literal && !path.is_empty()).then_some((o.offset, path))
            })
            .collect();
        let placement_refs = of_type("PlacementReference");
        let set_names = objects
            .iter()
            .filter(|o| o.type_name.as_deref() == Some("ScriptSet"))
            .map(|o| (o.offset, r.cstr(o.offset + 0x08)))
            .collect();
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
        let (scripts, master_vars): (Vec<_>, Vec<_>) = objects
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
                // `_masterVarList` (+0x18): the script's own variables (`ValueInfo`...).
                let master = r
                    .ptr(info.offset + 0x18)
                    .map(|l| r.list(l))
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|w| r.decode_ptr(w))
                    .collect();
                Some((
                    Script {
                        name: r.cstr(info.offset + 0x08),
                        ops,
                    },
                    master,
                ))
            })
            .unzip();
        let tags: HashMap<usize, Vec<usize>> = placements
            .iter()
            .filter_map(|&a| {
                let t = r.ptr(a + 0x18).filter(|&t| r.ty(t) == "TagList")?;
                let data = r.ptr(t + 0x24)?;
                let n = r.u32(t + 0x1c)? as usize;
                let list: Vec<usize> = (0..n)
                    .filter_map(|i| r.ptr(data + 4 * i))
                    .filter(|&g| vars.contains_key(&g))
                    .collect();
                (!list.is_empty()).then_some((a, list))
            })
            .collect();
        let tag_paths: Vec<(usize, Vec<Ref>)> = tags
            .values()
            .flatten()
            .filter(|&&g| {
                r.ptr(g + 0x30)
                    .is_some_and(|p| r.ty(p) == "RHSReferenceStack")
            })
            .map(|&g| (g, r.lhs(g + 0x30)))
            .collect();
        let strings = of_type("StringInfo");
        let mut set = Self {
            strings,
            scripts,
            master_vars,
            tags,
            tag_refs: HashMap::new(),
            vars,
            handle_ids: file.exid_formats.clone(),
            handle_ns: file.exid_namespaces.clone(),
            roots: file.roots.clone(),
            export_base: file.external_types.len(),
            opaque_operands,
            sets,
            placements,
            templates,
            waypoints,
            ref_index,
            records,
            use_paths,
            placement_refs,
            set_names,
            macro_names,
        };
        set.tag_refs = tag_paths
            .into_iter()
            .map(|(g, path)| {
                let t = match set.engine_key(&path).as_deref() {
                    Some([NULL_HASH]) | None => RefTarget::Null,
                    Some(k) => RefTarget::Engine(k.to_vec()),
                };
                (g, t)
            })
            .collect();
        Some(set)
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

    /// Name of the `ScriptSet` behind `EXID` handle `h` exported by `global`.
    pub fn global_set_name<'g>(&self, h: u32, global: &'g ScriptSet) -> Option<&'g str> {
        let i = h as usize;
        if *self.handle_ns.get(i)? != name_hash("global.bld") {
            return None;
        }
        let id = (*self.handle_ids.get(i)? as usize).checked_sub(global.export_base)?;
        global
            .set_names
            .get(global.roots.get(id)?)
            .map(String::as_str)
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
                Op::ChangeMembership { set, op, group } => {
                    let o = [
                        "include",
                        "exclude",
                        "intersect_with",
                        "be_replaced_by",
                        "add",
                    ]
                    .get(*op as usize)
                    .copied()
                    .unwrap_or("exclude_all");
                    format!("set {} {o} {}", self.path_str(set), self.path_str(group))
                }
                Op::ForEach { set, order } => {
                    format!("for each in {} order={order}", self.path_str(set))
                }
                Op::FindSubSet {
                    set,
                    field,
                    rel,
                    rhs,
                } => format!(
                    "find subset of {} where .{} [{rel}] {}",
                    self.path_str(set),
                    self.path_str(field),
                    rhs.as_ref().map_or("-".into(), |r| self.rhs_str(r))
                ),
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
    /// Sizes of `global.bld` sets the engine fills (`"Minions"`: heroes in play), by set
    /// name; `<set>.count` reads them.
    pub set_counts: HashMap<String, i32>,
    /// Other actors' variables `OpFindVariable` can bind (`VarBoard`); writes land here.
    pub board: VarBoard,
    /// References of the same variables (`RefBoard`).
    pub ref_board: RefBoard,
    /// `OpFindVariable` ops bound this run: op offset -> (actor, variable name).
    found: HashMap<usize, (usize, String)>,
    /// This actor's own variables by name (`_masterVarList` and tags), and the
    /// `OpFindVariable` ops bound to one of them (`find variable "Mod Controller"` on its
    /// own placement): those read and write the variable itself.
    pub own_vars: HashMap<String, usize>,
    found_own: HashMap<usize, usize>,
    /// Script sets by canonical path: members as far as the VM can tell who they are.
    pub sets: HashMap<Vec<Ref>, Vec<RefTarget>>,
    /// Engine ids of this actor's clones by serial, and this actor's own board key, for
    /// `OpFindVariable` / `OpSpawn ... at` through references.
    pub clone_ids: HashMap<u32, u64>,
    pub self_key: Option<usize>,
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
    /// Members of `global.bld` sets the engine fills (`"Minions"`: the heroes), by name.
    pub engine_sets: HashMap<String, Vec<RefTarget>>,
    /// Debugging: when set, every op run is appended as (`global.bld`?, op offset).
    pub trace: Option<Vec<(bool, usize)>>,
    /// Engine id of the running actor, and the engine's answers for member fields.
    pub self_id: Option<u64>,
    pub fields: Option<std::sync::Arc<dyn MemberFields>>,
    /// "Set Save Param" calls, in order: (save param, field index, value). Later reads in
    /// the same run see them.
    pub save_writes: Vec<(String, i32, i64)>,
    /// `OpSpawn` clones made so far by this actor (next `Effect::Spawn::serial`).
    pub spawned: u32,
    /// `global.bld`'s index, to run its macros when a script calls them (`use ?`).
    pub global_index: Option<&'a VmIndex>,
    /// Locals and references of `global.bld` macros run for this actor (kept across
    /// calls and ticks, like a macro's own variables).
    pub global_vars: HashMap<Vec<Ref>, Value>,
    pub global_refs: HashMap<Vec<Ref>, RefTarget>,
    /// Sets of `global.bld` (`"active"` creatures...): one level-wide table, members as
    /// absolute targets (`Vm::absolute`).
    pub global_sets: HashMap<Vec<Ref>, Vec<RefTarget>>,
    /// The file that called into this one (`global.bld` runs: the level), for templates.
    home: Option<&'a ScriptSet>,
    /// Setup blocks being run, innermost last.
    spawning: Vec<Spawning>,
}

/// Script variables of other placed actors, by `tfbActorInfo` offset and variable name,
/// for `OpFindVariable` (`find variable "state" on _ArenaMaster2`).
pub type VarBoard = HashMap<usize, HashMap<String, Value>>;

/// References in other actors' variables, like `VarBoard` (absolute targets:
/// `"transport"."enemy1L" = @spawn` from the BattleMaster's dropship macro).
pub type RefBoard = HashMap<usize, HashMap<String, RefTarget>>;

/// Object a script reference points at, as far as the VM can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefTarget {
    Null,
    /// Engine object by name-hash path (`my`...).
    Engine(Vec<u32>),
    /// The actor's `serial`-th `OpSpawn` clone (`Effect::Spawn`).
    Spawned(u32),
    /// Another actor, by the engine's id for it (a level `PlacementReference` such as
    /// `Current_BattleMaster` that another script set).
    Actor(u64),
    /// A macro parameter that stands for the caller's variable (a record such as the
    /// transport's `"My Info"` passed as `"info"`): paths through it are that path.
    Alias(Vec<Ref>),
    /// A macro definition (`OpDefineMacro[Specialization]` offset) held in a variable,
    /// called by a dynamic `use` (`"Transport Type"."Movement" = "Airship - Movement"`).
    Macro(usize),
    /// Waypoint `index` of the placement `owner`'s `ActorWaypointList`
    /// (`"Nav - next wp" = "Nav - Flight Path".waypoints[i]`).
    Waypoint {
        owner: usize,
        index: usize,
    },
    /// A placement object itself (`tfbActorInfo` offset): an enemy or transport template
    /// held in a reference variable (`"natureHeavy" = Enemy_S4_NatureHeavy_`).
    Placement(usize),
    /// Record variable `var` of the actor on board slot `actor`, as other actors hold it
    /// (`"Selected Mod" = "Arena Mod - Artillery"` found on the arena master); its owner
    /// reads it as its own record (`Alias`).
    Record {
        actor: usize,
        var: usize,
    },
    /// A `StringInfo` constant (`"Mod - Artillery"`), which scripts use as an identity:
    /// `"New Mod" == "Arena Mod - Artillery"."Mod ID"`.
    String(usize),
    /// Something only the live engine knows (macro output...).
    Unknown,
}

/// Name hash of the engine's `null` reference.
const NULL_HASH: u32 = 0x7707_4ba4;
/// Name hash of `my`, the actor running the script.
const MY_HASH: u32 = 0x4c2d_fe93;
/// First object id of data-object instances (`spawn` of a record), above any file
/// offset; `[Obj(INSTANCE_BASE)]` in the shared store counts them.
const INSTANCE_BASE: usize = 1 << 40;

fn is_instance(p: &[Ref]) -> bool {
    matches!(p.first(), Some(Ref::Obj(o)) if *o >= INSTANCE_BASE)
}

/// Name hash of a set's `count` field.
const SET_COUNT: u32 = 0x39b1_ddf4;
/// `first` / `last` members of a list.
const FIRST: u32 = 0x4881_d841;
const LAST: u32 = 0x63e1_d819;
/// `waypoints` (a placement's `ActorWaypointList`), and `orientation.facing/tilt/roll`.
const WAYPOINTS: u32 = 0x7a0a_bb2b;
const ORIENTATION_HASH: u32 = 0xc545_bc21;
const FACING_HASH: u32 = 0x86d0_5dcf;
const TILT_HASH: u32 = 0x603a_6206;
const ROLL_HASH: u32 = 0x16ff_3d1e;

/// Object an effect acts on: an engine object of this actor, a clone being set up
/// (index into `Vm::spawning`, key as the clone's own `my` path), or an earlier clone
/// through a reference variable (`serial`, key as its `my` path).
enum Target {
    Engine(Vec<u32>),
    Clone(usize, Vec<u32>),
    Spawned(u32, Vec<u32>),
    Actor(u64, Vec<u32>),
}

/// Engine fields of an actor as seen from another (`distance`, `heading`...), for
/// `OpFindSubSet`: `field` (name hash) of actor `member` relative to actor `me` (engine
/// ids, `RefTarget::Actor`).
pub trait MemberFields: Send + Sync + std::fmt::Debug {
    fn field(&self, me: u64, member: u64, field: u32) -> Option<f32>;
    /// `field` of a waypoint of placement `owner` at `local` (its own space), from `me`.
    fn waypoint_field(
        &self,
        _me: u64,
        _owner: usize,
        _local: [f32; 3],
        _field: u32,
    ) -> Option<f32> {
        None
    }
}

/// The save slot as scripts see it through `global.bld` "System - Save - Get Save Param"
/// (`param` = save param name, e.g. `"village generic vars"`). `None`: not in the save.
pub trait SaveParams: Send + Sync + std::fmt::Debug {
    fn get(&self, param: &str, index: i32) -> Option<i64>;
}

const GET_SAVE_PARAM: &str = "System - Save - Get Save Param";
const SET_SAVE_PARAM: &str = "Set Save Param";

/// Engine action requested by a script op.
// `Spawn` carries its setup (seed, setup effects, named writes); spawns are rare.
#[allow(clippy::large_enum_variant)]
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
    /// `effect` (targets as that actor's own `my` paths) on another actor (`RefTarget::Actor`).
    ForActor { id: u64, effect: Box<Effect> },
    /// `OpSpawn` at op `op`: clone `template` at the engine object `at`, facing `facing`
    /// degrees. `serial` numbers the actor's spawns (`RefTarget::Spawned`). The setup block's
    /// writes to the clone are in `seed` (keys as the clone's own `my` paths) and its
    /// effects on the clone in `setup` (targets `[MY]`, ...).
    Spawn {
        op: usize,
        template: SpawnTemplate,
        at: SpawnAt,
        facing: Option<f32>,
        serial: u32,
        seed: HashMap<Vec<u32>, Value>,
        setup: Vec<Effect>,
        /// Script variables of the clone written in the setup block through
        /// `OpFindVariable` (`"flight path" = "Transport Path"`), by name.
        named_vars: HashMap<String, Value>,
        named_refs: HashMap<String, RefTarget>,
    },
}

/// Where an `OpSpawn` clone starts.
#[derive(Debug, Clone, PartialEq)]
pub enum SpawnAt {
    /// An engine object of the spawner (`[MY]`: its current transform).
    Engine(Vec<u32>),
    /// A placed `tfbActorInfo` (`spawn "_BattleMaster" at _BattleMaster`): its placement
    /// transform, inactive templates included.
    Placement(usize),
    /// Another actor's current transform (`spawn "Left 1" at Current_BattleMaster`).
    Actor(u64),
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
    /// Macro definitions by name (calls from other files name them).
    by_name: HashMap<String, (usize, usize)>,
}

impl VmIndex {
    pub fn new(set: &ScriptSet) -> Self {
        let mut index = Self::default();
        for (si, s) in set.scripts.iter().enumerate() {
            for (oi, ins) in s.ops.iter().enumerate() {
                match &ins.op {
                    Op::CheckValue { .. }
                    | Op::CheckRef { .. }
                    | Op::LoopValue { .. }
                    | Op::FindVariable { .. } => {
                        index.aliases.insert(ins.offset, (si, oi));
                    }
                    Op::DefineMacro { name } => {
                        index.macros.entry(ins.offset).or_insert((si, oi));
                        index.by_name.entry(name.clone()).or_insert((si, oi));
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
            set_counts: HashMap::new(),
            board: HashMap::new(),
            ref_board: HashMap::new(),
            found: HashMap::new(),
            own_vars: HashMap::new(),
            found_own: HashMap::new(),
            sets: HashMap::new(),
            clone_ids: HashMap::new(),
            self_key: None,
            unbound: Vec::new(),
            rng: 0,
            behavior: None,
            refs: HashMap::new(),
            save: None,
            engine_sets: HashMap::new(),
            self_id: None,
            fields: None,
            trace: None,
            save_writes: Vec::new(),
            spawned: 0,
            global_index: None,
            global_vars: HashMap::new(),
            global_refs: HashMap::new(),
            global_sets: HashMap::new(),
            home: None,
            spawning: Vec::new(),
        }
    }

    /// `null` / engine handles resolve to themselves; script reference variables to what
    /// `OpSetReference` stored (members of a `null` reference are `null`).
    fn ref_target(&self, path: &[Ref]) -> RefTarget {
        match self.ref_target_raw(path) {
            RefTarget::Record { actor, var } if Some(actor) == self.self_key => {
                RefTarget::Alias(vec![Ref::Obj(var)])
            }
            t => t,
        }
    }

    fn ref_target_raw(&self, path: &[Ref]) -> RefTarget {
        if let Some(k) = self.set.engine_key(path) {
            return match k[..] {
                [NULL_HASH] => RefTarget::Null,
                _ => RefTarget::Engine(k),
            };
        }
        let Ok(p) = self.canon(path) else {
            return RefTarget::Unknown;
        };
        // Another actor's variable (`OpFindVariable`).
        if let Some((a, name)) = self.found_slot(&p) {
            return self
                .ref_board
                .get(&a)
                .and_then(|v| v.get(&name))
                .cloned()
                .unwrap_or(RefTarget::Null);
        }
        if let [Ref::Obj(o)] = p[..] {
            if self.set.placements.contains(&o) {
                return RefTarget::Placement(o);
            }
            if self.set.strings.contains(&o) {
                return RefTarget::String(o);
            }
            if self.index.macros.contains_key(&o) {
                return RefTarget::Macro(o);
            }
        }
        if let Some((i, k)) = self.clone_key(&p) {
            if k == [MY_HASH] {
                return RefTarget::Spawned(self.spawning[i].serial);
            }
        }
        if let Some(t) = self.refs_of(&p).get(&p) {
            return t.clone();
        }
        // `<list>.first` / `.last`: its first / last member.
        if let Some((Ref::Handle(h), list)) = p.split_last() {
            let end = match self.set.engine_key(&[Ref::Handle(*h)]).as_deref() {
                Some([FIRST]) => Some(true),
                Some([LAST]) => Some(false),
                _ => None,
            };
            if let Some(first) = end {
                if let Some(s) = self.sets_of(list).get(list) {
                    let m = if first { s.first() } else { s.last() };
                    return m.cloned().unwrap_or(RefTarget::Null);
                }
            }
        }
        // A data-object instance itself (`@spawn` of a record, through its alias).
        if p.len() == 1 && is_instance(&p) {
            return RefTarget::Alias(p);
        }
        match p.first() {
            Some(Ref::Obj(_)) if self.refs.get(&p[..1]) == Some(&RefTarget::Null) => {
                RefTarget::Null
            }
            // A script's own variable that no `OpSetReference` ever pointed anywhere (nor
            // any record it is part of) is `null`: `Sky_Traps.ai` asks
            // `"trapping data".<defeat sequence> != null`, which only the trap sequence sets.
            // Fields of a data-object instance follow the same rule.
            Some(Ref::Obj(v))
                if (is_instance(&p)
                    || self.set.vars.get(v).is_some_and(|i| {
                        matches!(
                            i.type_name.as_str(),
                            "ValueInfo" | "OpCreateVariable" | "ScriptStructure"
                        )
                    }))
                    && (1..=p.len()).all(|k| !self.refs_of(&p[..k]).contains_key(&p[..k])) =>
            {
                // A record variable is the record itself (`"New Info"`, returned by
                // reference); a field of a record, record-typed or not, is a reference that
                // is `null` until set (`"trapping data"."villain just captured data"`).
                match p[..] {
                    [Ref::Obj(v)] if self.set.records.contains(&v) => RefTarget::Alias(p),
                    _ => RefTarget::Null,
                }
            }
            _ => RefTarget::Unknown,
        }
    }

    /// Members of a group path: a stored set's members, else the one object it refers to
    /// (nothing for `null` or what only the live engine knows).
    fn members(&self, group: &[Ref]) -> Vec<RefTarget> {
        if let Some(s) = self
            .canon(group)
            .ok()
            .and_then(|k| self.sets_of(&k).get(&k))
        {
            return s.clone();
        }
        if let Some(s) = self.engine_set(group) {
            return s.clone();
        }
        match self.ref_target(group) {
            RefTarget::Null | RefTarget::Unknown => Vec::new(),
            t => vec![t],
        }
    }

    /// Members the engine put in the `global.bld` set `group` names.
    fn engine_set(&self, group: &[Ref]) -> Option<&Vec<RefTarget>> {
        let [Ref::Handle(h)] = group else { return None };
        let name = self.set.global_set_name(*h, self.global?)?;
        self.engine_sets.get(name)
    }

    /// Engine id of a member (`None`: a placement, `null`, what only the engine knows).
    fn member_id(&self, m: &RefTarget) -> Option<u64> {
        match m {
            RefTarget::Actor(id) => Some(*id),
            RefTarget::Spawned(s) => self.clone_ids.get(s).copied(),
            RefTarget::Engine(k) if *k == [MY_HASH] => self.self_id,
            _ => None,
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

    /// Element `i` of the list a path names: `<placement>.waypoints[i]`.
    fn indexed_target(&self, path: &[Ref], i: i64) -> RefTarget {
        // `0x8008ca38`: the index wraps (`i % count`, plus `count` when negative); an empty
        // list gives `null`. `"active dropships"[count - loop]` visits 0, then count - 1...
        let wrap = |n: usize| (n > 0).then(|| i.rem_euclid(n as i64) as usize);
        let waypoints = match path.split_last() {
            Some((Ref::Handle(h), owner))
                if self.set.engine_key(&[Ref::Handle(*h)]) == Some(vec![WAYPOINTS]) =>
            {
                Some(owner)
            }
            _ => None,
        };
        // Element `i` of a set (`"active"[i]` in `Combat.ai` "Get Creature Info").
        let Some(owner) = waypoints else {
            let members = self.members(path);
            return wrap(members.len())
                .and_then(|i| members.get(i).cloned())
                .unwrap_or(RefTarget::Null);
        };
        match self.ref_target(owner) {
            RefTarget::Placement(o) => match wrap(self.home_waypoints(o).map_or(0, Vec::len)) {
                Some(index) => RefTarget::Waypoint { owner: o, index },
                None => RefTarget::Null,
            },
            _ => RefTarget::Null,
        }
    }

    /// Waypoints of placement `o` (in this file, or the level a `global.bld` run was
    /// called from).
    fn home_waypoints(&self, o: usize) -> Option<&Vec<Waypoint>> {
        self.set
            .waypoints
            .get(&o)
            .or_else(|| self.home?.waypoints.get(&o))
    }

    /// A field of the object a reference prefix of `p` holds (`"info"."Nav - next wp".
    /// heading`): waypoint data and engine answers relative to this actor. `None` when
    /// no prefix is such a reference or the field is not known.
    fn referenced_field(&self, p: &[Ref]) -> Option<Value> {
        let (k, target) = (1..p.len())
            .rev()
            .find_map(|k| Some((k, self.refs_of(&p[..k]).get(&p[..k])?.clone())))?;
        let rest = self.set.engine_key(&p[k..])?;
        match (target, &rest[..]) {
            (RefTarget::Placement(o), [WAYPOINTS, SET_COUNT]) => {
                Some(Value::Int(self.home_waypoints(o).map_or(0, Vec::len) as i32))
            }
            (RefTarget::Waypoint { owner, index }, [ORIENTATION_HASH, angle]) => {
                let wp = self.home_waypoints(owner)?.get(index)?;
                let i = [FACING_HASH, TILT_HASH, ROLL_HASH]
                    .iter()
                    .position(|a| a == angle)?;
                // Order assumed like placements (facing = y, tilt = x, roll = z).
                Some(Value::Float(wp.angles[[1, 0, 2][i]]))
            }
            (RefTarget::Waypoint { owner, index }, [f]) => {
                let wp = self.home_waypoints(owner)?.get(index)?;
                let v = self
                    .fields
                    .as_ref()?
                    .waypoint_field(self.self_id?, owner, wp.local, *f)?;
                Some(Value::Float(v))
            }
            // This actor itself: its own engine state ("info"."Transport".distance).
            (RefTarget::Engine(k), _) if k == [MY_HASH] => self
                .engine
                .get(&[MY_HASH].into_iter().chain(rest).collect::<Vec<_>>())
                .copied(),
            (RefTarget::Actor(id), _) if Some(id) == self.self_id => self
                .engine
                .get(&[MY_HASH].into_iter().chain(rest).collect::<Vec<_>>())
                .copied(),
            (t, [f]) => {
                let id = self.member_id(&t)?;
                let v = self.fields.as_ref()?.field(self.self_id?, id, *f)?;
                Some(Value::Float(v))
            }
            _ => None,
        }
    }

    /// Where a path's values, references and sets live: data-object instances are in the
    /// shared `global.bld` store (this VM's own when it runs `global.bld`).
    fn shared(&self, p: &[Ref]) -> bool {
        is_instance(p) && self.home.is_none()
    }
    fn vars_of(&self, p: &[Ref]) -> &HashMap<Vec<Ref>, Value> {
        if self.shared(p) {
            &self.global_vars
        } else {
            &self.vars
        }
    }
    fn vars_of_mut(&mut self, p: &[Ref]) -> &mut HashMap<Vec<Ref>, Value> {
        if self.shared(p) {
            &mut self.global_vars
        } else {
            &mut self.vars
        }
    }
    fn refs_of(&self, p: &[Ref]) -> &HashMap<Vec<Ref>, RefTarget> {
        if self.shared(p) {
            &self.global_refs
        } else {
            &self.refs
        }
    }
    fn refs_of_mut(&mut self, p: &[Ref]) -> &mut HashMap<Vec<Ref>, RefTarget> {
        if self.shared(p) {
            &mut self.global_refs
        } else {
            &mut self.refs
        }
    }
    fn sets_of(&self, p: &[Ref]) -> &HashMap<Vec<Ref>, Vec<RefTarget>> {
        if self.shared(p) {
            &self.global_sets
        } else {
            &self.sets
        }
    }
    fn sets_of_mut(&mut self, p: &[Ref]) -> &mut HashMap<Vec<Ref>, Vec<RefTarget>> {
        if self.shared(p) {
            &mut self.global_sets
        } else {
            &mut self.sets
        }
    }

    /// A fresh, empty data object in the shared store.
    fn new_instance(&mut self) -> Vec<Ref> {
        let counter = vec![Ref::Obj(INSTANCE_BASE)];
        let n = self
            .vars_of(&counter)
            .get(&counter)
            .map_or(0, |v| v.as_f32() as usize)
            + 1;
        self.vars_of_mut(&counter)
            .insert(counter.clone(), Value::Int(n as i32));
        vec![Ref::Obj(INSTANCE_BASE + n)]
    }

    /// The list a `<list>.count` path names, when it is a script variable path (not an
    /// engine or global-export field).
    fn list_of_count(&self, lhs: &[Ref]) -> Option<Vec<Ref>> {
        let p = self.canon(lhs).ok()?;
        let (Ref::Handle(c), list) = p.split_last()? else {
            return None;
        };
        (self.set.engine_key(&[Ref::Handle(*c)]) == Some(vec![SET_COUNT])
            && matches!(list.first(), Some(Ref::Obj(_))))
        .then(|| list.to_vec())
    }

    /// A new instance of the record at `template` (`spawn "New Info"`): a fresh object id
    /// in the shared store, with the template's current values, references and sets
    /// copied under it (as a clone copies its template's fields).
    fn instantiate(&mut self, template: &[Ref]) -> Vec<Ref> {
        let inst = self.new_instance();
        let under = |k: &Vec<Ref>| -> Option<Vec<Ref>> {
            k.starts_with(template)
                .then(|| inst.iter().chain(&k[template.len()..]).copied().collect())
        };
        let vars: Vec<_> = self
            .vars_of(template)
            .iter()
            .filter_map(|(k, v)| Some((under(k)?, *v)))
            .collect();
        let refs: Vec<_> = self
            .refs_of(template)
            .iter()
            .filter_map(|(k, v)| Some((under(k)?, v.clone())))
            .collect();
        let sets: Vec<_> = self
            .sets_of(template)
            .iter()
            .filter_map(|(k, v)| Some((under(k)?, v.clone())))
            .collect();
        self.vars_of_mut(&inst).extend(vars);
        self.refs_of_mut(&inst).extend(refs);
        self.sets_of_mut(&inst).extend(sets);
        inst
    }

    /// The slot an `OpSetReference` rebinds: the variable path itself, with only what leads
    /// to it canonicalized (a variable holding an alias is rebound, not its target).
    fn ref_slot(&self, lhs: &[Ref]) -> Result<Vec<Ref>, i32> {
        match lhs.split_last() {
            Some((last, prefix)) if !prefix.is_empty() => {
                let mut p = self.canon(prefix)?;
                p.push(*last);
                Ok(p)
            }
            // An `OpFindVariable` bound to this actor's own variable: that variable.
            Some((Ref::Obj(o), [])) if self.found_own.contains_key(o) => {
                Ok(vec![Ref::Obj(self.found_own[o])])
            }
            _ => Ok(lhs.to_vec()),
        }
    }

    /// A path through a reference to another actor (`Current_BattleMaster.x`): its id and
    /// that actor's own `my` path.
    fn actor_key(&self, p: &[Ref]) -> Option<(u64, Vec<u32>)> {
        let first @ Ref::Obj(_) = p.first()? else {
            return None;
        };
        let Some(RefTarget::Actor(id)) = self.refs.get(std::slice::from_ref(first)) else {
            return None;
        };
        if Some(*id) == self.self_id {
            return None;
        }
        let rest = match &p[1..] {
            [] => Vec::new(),
            rest => self.set.engine_key(rest)?,
        };
        Some((*id, [MY_HASH].into_iter().chain(rest).collect()))
    }

    /// Variable board key of the object `path` names: a board actor itself, or the actor
    /// a reference holds (another actor, one of this actor's clones, `my`).
    fn board_key(&self, path: &[Ref]) -> Option<usize> {
        if let [Ref::Obj(a)] = path {
            if self.board.contains_key(a) {
                return Some(*a);
            }
        }
        match self.ref_target(path) {
            RefTarget::Actor(id) => Some(id as usize),
            RefTarget::Spawned(s) => self.clone_ids.get(&s).map(|&id| id as usize),
            RefTarget::Engine(k) if k == [MY_HASH] => self.self_key,
            _ => None,
        }
    }

    /// A reference as every actor would read it: `my` and this actor's clones as engine
    /// ids. `global.bld` state is shared by all actors, so it stores these.
    fn absolute(&self, t: RefTarget) -> RefTarget {
        match t {
            RefTarget::Engine(ref k) if *k == [MY_HASH] => self.self_id.map_or(t, RefTarget::Actor),
            RefTarget::Spawned(s) => self.clone_ids.get(&s).map_or(t, |&id| RefTarget::Actor(id)),
            RefTarget::Alias(ref p) => match (&p[..], self.self_key) {
                (&[Ref::Obj(var)], Some(actor)) if self.set.records.contains(&var) => {
                    RefTarget::Record { actor, var }
                }
                _ => t,
            },
            t => t,
        }
    }

    /// A path through a reference that holds an engine object (`"mover" = my`, then
    /// `"mover".velocity.length`; also this actor by its engine id): that object's path.
    fn engine_ref_key(&self, p: &[Ref]) -> Option<Vec<u32>> {
        let first @ Ref::Obj(_) = p.first()? else {
            return None;
        };
        let k = match self.refs.get(std::slice::from_ref(first)) {
            Some(RefTarget::Engine(k)) => k.clone(),
            Some(RefTarget::Actor(id)) if Some(*id) == self.self_id => vec![MY_HASH],
            _ => return None,
        };
        let k = &k;
        let rest = match &p[1..] {
            [] => Vec::new(),
            rest => self.set.engine_key(rest)?,
        };
        Some(k.iter().copied().chain(rest).collect())
    }

    fn target(&self, path: &[Ref]) -> Option<Target> {
        if let Some(k) = self.set.engine_key(path) {
            return Some(Target::Engine(k));
        }
        let p = self.canon(path).ok()?;
        if let Some(k) = self.engine_ref_key(&p) {
            return Some(Target::Engine(k));
        }
        if let Some((i, k)) = self.clone_key(&p) {
            return Some(Target::Clone(i, k));
        }
        if let Some((id, k)) = self.actor_key(&p) {
            return Some(Target::Actor(id, k));
        }
        let (serial, k) = self.spawned_key(&p)?;
        // A reference to the clone whose setup block is still running means that clone.
        Some(
            match self.spawning.iter().rposition(|s| s.serial == serial) {
                Some(i) => Target::Clone(i, k),
                None => Target::Spawned(serial, k),
            },
        )
    }

    fn emit(&mut self, t: Target, effect: impl FnOnce(Vec<u32>) -> Effect) {
        match t {
            Target::Engine(k) => self.effects.push(effect(k)),
            Target::Clone(i, k) => self.spawning[i].setup.push(effect(k)),
            Target::Spawned(serial, k) => self.effects.push(Effect::ForClone {
                serial,
                effect: Box::new(effect(k)),
            }),
            Target::Actor(id, k) => self.effects.push(Effect::ForActor {
                id,
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

    /// The macro a call at `at` runs when its target path names none (a dynamic `use ?`):
    /// the definition its call-site params belong to (`OpMacroParameter` +0x38).
    fn site_macro(&self, si: usize, at: usize) -> Option<usize> {
        let (_, end) = self.params(si, at);
        self.set.scripts[si].ops[(at + 1).min(end)..end]
            .iter()
            .find_map(|i| match i.op {
                Op::Param { macro_def, .. } => macro_def,
                _ => None,
            })
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
        self.run_macro_with(si, oi, args, None)
    }

    /// Run the `global.bld` macro a stub `target` names, in a VM over `global.bld` that
    /// shares this actor's engine state, effects and budget. Arguments go in by parameter
    /// name (values; references from the call site's `RHSReferenceStack`), outputs come
    /// back by name. `false` (nothing changed): not a `global.bld` macro, or it hit
    /// something the VM does not model.
    fn call_global(&mut self, target: Option<usize>, site: &[(usize, u16, Option<&Rhs>)]) -> bool {
        let (Some(g), Some(gi)) = (self.global, self.global_index) else {
            return false;
        };
        let Some(name) = target.and_then(|t| self.set.macro_names.get(&t)) else {
            return false;
        };
        if matches!(name.as_str(), GET_SAVE_PARAM | SET_SAVE_PARAM) {
            return false;
        }
        let Some(&(gsi, goi)) = gi.by_name.get(name) else {
            return false;
        };
        let mut args = Vec::new();
        let mut ref_args = Vec::new();
        for &(var, _, rhs) in site {
            let pname = self.set.var_name(var).to_string();
            match rhs {
                Some(rhs) => match self.eval(rhs) {
                    Ok(v) => args.push((pname, v)),
                    Err(_) => return false,
                },
                None => {
                    if let Some(path) = self.ref_arg_path(var) {
                        let t = self.ref_target(&path);
                        ref_args.push((pname, t));
                    }
                }
            }
        }
        let mut sub = Vm::with_index(g, None, Cow::Borrowed(gi));
        sub.home = Some(self.home.unwrap_or(self.set));
        sub.global_index = None;
        sub.vars = std::mem::take(&mut self.global_vars);
        sub.refs = std::mem::take(&mut self.global_refs);
        sub.sets = std::mem::take(&mut self.global_sets);
        sub.engine = std::mem::take(&mut self.engine);
        sub.board = std::mem::take(&mut self.board);
        sub.ref_board = std::mem::take(&mut self.ref_board);
        sub.set_counts = std::mem::take(&mut self.set_counts);
        sub.globals = std::mem::take(&mut self.globals);
        sub.clone_ids = std::mem::take(&mut self.clone_ids);
        sub.self_key = self.self_key;
        sub.save = self.save.clone();
        sub.engine_sets = std::mem::take(&mut self.engine_sets);
        sub.self_id = self.self_id;
        sub.fields = self.fields.clone();
        sub.trace = self.trace.take();
        sub.rng = self.rng;
        sub.spawned = self.spawned;
        sub.steps = self.steps;
        sub.depth = self.depth + 1;
        let result = sub.run_macro_with(gsi, goi, &args, Some(&ref_args));
        // Outputs by name, read before the sub VM gives its state back.
        let (defs, _) = sub.params(gsi, goi);
        let outputs: Vec<(String, Value, Option<RefTarget>)> = defs
            .iter()
            .map(|&d| {
                let v = sub.read(&[Ref::Obj(d)]);
                let r = sub.refs.get(&vec![Ref::Obj(d)]).cloned();
                (g.var_name(d).to_string(), v, r)
            })
            .collect();
        self.global_vars = std::mem::take(&mut sub.vars);
        self.global_refs = std::mem::take(&mut sub.refs);
        self.global_sets = std::mem::take(&mut sub.sets);
        self.engine = std::mem::take(&mut sub.engine);
        self.board = std::mem::take(&mut sub.board);
        self.ref_board = std::mem::take(&mut sub.ref_board);
        self.set_counts = std::mem::take(&mut sub.set_counts);
        self.globals = std::mem::take(&mut sub.globals);
        self.clone_ids = std::mem::take(&mut sub.clone_ids);
        self.engine_sets = std::mem::take(&mut sub.engine_sets);
        self.trace = sub.trace.take();
        self.rng = sub.rng;
        self.spawned = sub.spawned;
        self.steps = sub.steps;
        if result.is_err() {
            return false;
        }
        for effect in std::mem::take(&mut sub.effects) {
            if let Some(e) = self.localize_global(g, effect) {
                self.effects.push(e);
            }
        }
        for &(var, ..) in site {
            let pname = self.set.var_name(var);
            if let Some((_, v, r)) = outputs.iter().find(|(n, ..)| n == pname) {
                match r {
                    Some(r) => {
                        self.refs.insert(vec![Ref::Obj(var)], r.clone());
                    }
                    None => self.write(&[Ref::Obj(var)], *v),
                }
            }
        }
        true
    }

    /// Call-site reference argument of param `var` (`RHSReferenceStack` at +0x30).
    fn ref_arg_path(&self, var: usize) -> Option<Vec<Ref>> {
        self.set
            .opaque_operands
            .get(&var)?
            .iter()
            .find_map(|(f, o)| match o {
                OpaqueOperand::Path { stack, path }
                    if *f == 0x30 && stack == "RHSReferenceStack" =>
                {
                    Some(path.clone())
                }
                _ => None,
            })
    }

    /// An effect made by a `global.bld` macro, in this file's terms: slider paths are
    /// script refs of the file that made them, so they are re-expressed through this
    /// file's handles (engine fields only; a slide of a `global.bld` local is dropped).
    fn localize_global(&self, g: &ScriptSet, effect: Effect) -> Option<Effect> {
        Some(match effect {
            Effect::Slide {
                op,
                lhs,
                from,
                to,
                seconds,
                ease_out,
                ease_in,
            } => Effect::Slide {
                op,
                lhs: self.my_refs(&g.engine_key(&lhs)?)?,
                from,
                to,
                seconds,
                ease_out,
                ease_in,
            },
            Effect::ForClone { serial, effect } => Effect::ForClone {
                serial,
                effect: Box::new(self.localize_global(g, *effect)?),
            },
            Effect::ForActor { id, effect } => Effect::ForActor {
                id,
                effect: Box::new(self.localize_global(g, *effect)?),
            },
            Effect::Spawn {
                op,
                template,
                at,
                facing,
                serial,
                seed,
                setup,
                named_vars,
                named_refs,
            } => Effect::Spawn {
                op,
                template,
                at,
                facing,
                serial,
                seed,
                setup: setup
                    .into_iter()
                    .filter_map(|e| self.localize_global(g, e))
                    .collect(),
                named_vars,
                named_refs,
            },
            e => e,
        })
    }

    /// `run_macro` with a script call's reference arguments by parameter name; there,
    /// reference parameters without one start `null` (not what the previous call left).
    /// Calls from the engine side (`None`) leave them unknown.
    fn run_macro_with(
        &mut self,
        si: usize,
        oi: usize,
        args: &[(String, Value)],
        ref_args: Option<&[(String, RefTarget)]>,
    ) -> Result<(), VmError> {
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
        for &(var, _, rhs) in params.iter().filter(|_| ref_args.is_some()) {
            if rhs.is_none() {
                let name = self.set.var_name(var);
                let ref_args = ref_args.unwrap_or_default();
                // Without an argument: the param's own default (`RHSReferenceStack` +0x30,
                // e.g. `my` for "Transport - Startup"'s "transport"), else `null`.
                let t = match ref_args.iter().find(|(n, _)| n == name) {
                    Some((_, t)) => t.clone(),
                    None => self
                        .ref_arg_path(var)
                        .map_or(RefTarget::Null, |p| self.ref_target(&p)),
                };
                self.refs.insert(vec![Ref::Obj(var)], t);
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
            if self.call_global(target, &site) {
                return Ok(after);
            }
            // Defined in another file we cannot run: every parameter keeps its literal.
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
        let mut ref_args = Vec::new();
        for &(var, flags, rhs) in &site {
            match (flags & PARAM_OUTPUT, rhs) {
                (0, Some(rhs)) => args.push((self.set.var_name(var).to_string(), self.eval(rhs)?)),
                // A reference argument: the call site's `RHSReferenceStack` (+0x30).
                (0, None) => {
                    if let Some(path) = self.ref_arg_path(var) {
                        // A variable no reference was stored in is passed as itself.
                        let t = match self.ref_target(&path) {
                            t @ (RefTarget::Null | RefTarget::Unknown) => match self.canon(&path) {
                                Ok(p)
                                    if matches!(p.first(), Some(Ref::Obj(v))
                                        if self.set.vars.contains_key(v)) =>
                                {
                                    RefTarget::Alias(p)
                                }
                                _ => t,
                            },
                            t => t,
                        };
                        ref_args.push((self.set.var_name(var).to_string(), t));
                    }
                }
                _ => {}
            }
        }
        self.run_macro_with(tsi, toi, &args, Some(&ref_args))?;
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
            if let Some(t) = self.trace.as_mut() {
                t.push((self.home.is_some(), ins.offset));
            }
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
                    // `inc <list>.count` appends a new (empty) record, `dec` drops the last
                    // (`Combat.ai`: `inc @e678c."active".count`, then `.last`).
                    if let Some(list) = self.list_of_count(lhs) {
                        if d == 0 {
                            let inst = self.new_instance();
                            self.sets_of_mut(&list)
                                .entry(list)
                                .or_default()
                                .push(RefTarget::Alias(inst));
                        } else if let Some(s) = self.sets_of_mut(&list).get_mut(&list) {
                            s.pop();
                        }
                        continue;
                    }
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
                    let t = match self.set.ref_index.get(&ins.offset) {
                        Some(i) => {
                            let i = self.eval(i)?.as_f32() as i64;
                            self.indexed_target(rhs, i)
                        }
                        None => self.ref_target(rhs),
                    };
                    // Shared state (`global.bld`, data-object instances): store what every
                    // actor can read.
                    if let Some((a, name)) = self.canon(lhs).ok().and_then(|p| self.found_slot(&p))
                    {
                        let t = self.absolute(t);
                        self.ref_board.entry(a).or_default().insert(name, t);
                        continue;
                    }
                    if let Ok(p) = self.ref_slot(lhs) {
                        let t = if self.home.is_some() || is_instance(&p) {
                            self.absolute(t)
                        } else {
                            t
                        };
                        self.refs_of_mut(&p).insert(p, t);
                    }
                }
                Op::CheckRef {
                    rel,
                    desc,
                    lhs,
                    rhs,
                } => {
                    let stored = desc
                        .starts_with("OpCheckMembership")
                        .then(|| self.canon(lhs).ok())
                        .flatten()
                        .and_then(|k| self.sets_of(&k).get(&k).cloned());
                    let group = self.members(rhs);
                    // `_membershipOp`: 0 includes, 1 excludes, 2 intersects_with, 3 includes_all
                    // (enum table `0x805b5658`); with one known member they agree pairwise.
                    if let (Some(set), [m]) = (&stored, &group[..]) {
                        let m = self.absolute(m.clone());
                        let has = set.iter().any(|x| self.absolute(x.clone()) == m);
                        if has == (*rel == 1) {
                            pc += skip;
                        }
                        continue;
                    }
                    let known = desc.starts_with("OpCheckReference") && matches!(rel, 1 | 5);
                    let holds = match (self.ref_target(lhs), self.ref_target(rhs)) {
                        (a, b) if known && a != RefTarget::Unknown && b != RefTarget::Unknown => {
                            (self.absolute(a) == self.absolute(b)) == (*rel == 1)
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
                Op::UseMacro { target } => {
                    // A dynamic call runs the macro its target variable holds; failing
                    // that, the base its call-site params belong to.
                    let held = self.set.use_paths.get(&ins.offset).and_then(|p| {
                        match self.ref_target(p) {
                            RefTarget::Macro(m) => Some(m),
                            _ => None,
                        }
                    });
                    let target = target.or(held).or_else(|| self.site_macro(si, pc - 1));
                    pc = self.use_macro(si, pc - 1, target)?
                }
                Op::ChangeMembership { set, op, group } => {
                    if let Ok(key) = self.canon(set) {
                        // Members compared (and, in shared `global.bld` sets, stored) as
                        // absolute targets.
                        let rel_group = self.members(group);
                        let group: Vec<RefTarget> =
                            rel_group.iter().map(|m| self.absolute(m.clone())).collect();
                        // What is stored: absolute in shared sets (`global.bld`, instances).
                        let stored = if self.home.is_some() || is_instance(&key) {
                            group.clone()
                        } else {
                            rel_group
                        };
                        let abs: Vec<RefTarget> = self
                            .sets_of(&key)
                            .get(&key)
                            .map(|s| s.iter().map(|x| self.absolute(x.clone())).collect())
                            .unwrap_or_default();
                        let keep = |i: usize, inside: bool| group.contains(&abs[i]) == inside;
                        let kept: Vec<bool> = match op {
                            1 => (0..abs.len()).map(|i| keep(i, false)).collect(),
                            2 => (0..abs.len()).map(|i| keep(i, true)).collect(),
                            _ => vec![true; abs.len()],
                        };
                        let s = self.sets_of_mut(&key).entry(key).or_default();
                        let mut i = 0;
                        s.retain(|_| {
                            i += 1;
                            kept[i - 1]
                        });
                        match op {
                            0 => {
                                for (m, a) in stored.into_iter().zip(&group) {
                                    if !abs.contains(a) && !s.contains(&m) {
                                        s.push(m);
                                    }
                                }
                            }
                            1 | 2 => {}
                            3 => *s = stored,
                            4 => s.extend(stored),
                            _ => s.clear(),
                        }
                    }
                }
                // Members compared through the engine (`MemberFields`); one it cannot answer
                // for does not match. Order: the set's (sorting by value is unconfirmed).
                Op::FindSubSet {
                    set,
                    field,
                    rel,
                    rhs,
                } => {
                    let threshold = match rhs {
                        Some(r) => self.eval(r)?,
                        None => Value::Int(0),
                    };
                    let field = self.set.engine_key(field);
                    let matches: Vec<RefTarget> = self
                        .members(set)
                        .into_iter()
                        .filter(|m| {
                            let (Some(me), Some(id), Some([f])) =
                                (self.self_id, self.member_id(m), field.as_deref())
                            else {
                                return false;
                            };
                            self.fields
                                .as_ref()
                                .and_then(|q| q.field(me, id, *f))
                                .is_some_and(|v| {
                                    compare(Value::Float(v), *rel, threshold).unwrap_or(false)
                                })
                        })
                        .collect();
                    match matches.first() {
                        None => pc += skip,
                        Some(first) => {
                            let me = vec![Ref::Obj(ins.offset)];
                            self.refs.insert(me.clone(), first.clone());
                            self.sets.insert(me, matches);
                        }
                    }
                }
                Op::ForEach { set, order } => {
                    let end = (pc + skip).min(to);
                    let mut members = self.members(set);
                    match order {
                        1 => members.reverse(),
                        2 => {
                            for i in (1..members.len()).rev() {
                                let j = self.next_rand() as usize % (i + 1);
                                members.swap(i, j);
                            }
                        }
                        _ => {}
                    }
                    let me = vec![Ref::Obj(ins.offset)];
                    for m in members {
                        self.refs.insert(me.clone(), m);
                        if let Flow::Return = self.run(si, pc, end)? {
                            self.refs.remove(&me);
                            return Ok(Flow::Return);
                        }
                    }
                    self.refs.remove(&me);
                    pc = end;
                }
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
                        Some(Target::Actor(id, k)) => {
                            if let Some(refs) = self.my_refs(&k) {
                                self.effects.push(Effect::ForActor {
                                    id,
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
                // A template behind a reference that holds a placement (variables, macro
                // params) resolves; `global.bld` / engine handles do not.
                Op::Spawn {
                    template,
                    actor,
                    at,
                    facing,
                } => {
                    let at = match (self.set.engine_key(at), &at[..]) {
                        (Some(k), _) => Some(SpawnAt::Engine(k)),
                        (None, [Ref::Obj(o)]) if self.set.placements.contains(o) => {
                            Some(SpawnAt::Placement(*o))
                        }
                        _ => match self.ref_target(at) {
                            RefTarget::Actor(id) => Some(SpawnAt::Actor(id)),
                            RefTarget::Spawned(s) => {
                                self.clone_ids.get(&s).map(|&id| SpawnAt::Actor(id))
                            }
                            RefTarget::Engine(k) => Some(SpawnAt::Engine(k)),
                            RefTarget::Placement(o) => Some(SpawnAt::Placement(o)),
                            _ => None,
                        },
                    };
                    let referenced = match actor {
                        Some(_) => None,
                        None => match self.ref_target(template) {
                            RefTarget::Placement(o) => self
                                .set
                                .templates
                                .get(&o)
                                .or_else(|| self.home?.templates.get(&o)),
                            _ => None,
                        },
                    };
                    // `0x8007c6bc` step 2: a template that is not a placement (a record such
                    // as `"New Info"`) makes a data object, not an actor; the op means it in
                    // its block and after. Here it stands for the template record itself
                    // (copy semantics not reversed; exact for one instance per actor).
                    if actor.is_none() && referenced.is_none() {
                        if let Ok(p) = self.canon(template) {
                            if matches!(p.last(), Some(Ref::Obj(v)) if self.set.records.contains(v))
                            {
                                let inst = self.instantiate(&p);
                                self.refs
                                    .insert(vec![Ref::Obj(ins.offset)], RefTarget::Alias(inst));
                                continue;
                            }
                        }
                    }
                    let (Some(template), Some(at)) = (actor.as_ref().or(referenced), at) else {
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
                    let slot = usize::MAX - self.spawning.len();
                    self.effects.push(Effect::Spawn {
                        op: ins.offset,
                        template: template.clone(),
                        at,
                        facing,
                        serial,
                        seed: done.seed,
                        setup: done.setup,
                        named_vars: self.board.remove(&slot).unwrap_or_default(),
                        named_refs: self.ref_board.remove(&slot).unwrap_or_default(),
                    });
                    if let Flow::Return = flow? {
                        return Ok(Flow::Return);
                    }
                    pc = end;
                }
                // A variable of another placed actor: the block runs with the op meaning
                // that variable. Anything else is not found: the block is skipped.
                Op::FindVariable { name, target } => {
                    // The clone being set up (`find variable "flight path" on @spawn` in its
                    // setup block): a slot that travels with its `Effect::Spawn`.
                    if let RefTarget::Spawned(s) = self.ref_target(target) {
                        if let Some(i) = self.spawning.iter().position(|x| x.serial == s) {
                            let slot = usize::MAX - i;
                            self.board
                                .entry(slot)
                                .or_default()
                                .entry(name.clone())
                                .or_insert(Value::Int(0));
                            self.found.insert(ins.offset, (slot, name.clone()));
                            continue;
                        }
                    }
                    // This actor's own variable.
                    let own = self.own_vars.get(name).copied();
                    if let (Some(var), true) = (own, self.board_key(target) == self.self_key) {
                        self.found.remove(&ins.offset);
                        self.found_own.insert(ins.offset, var);
                        continue;
                    }
                    self.found_own.remove(&ins.offset);
                    let actor = self
                        .board_key(target)
                        .filter(|a| self.board.get(a).is_some_and(|v| v.contains_key(name)));
                    match actor {
                        Some(a) => {
                            self.found.insert(ins.offset, (a, name.clone()));
                        }
                        None => pc += skip,
                    }
                }
                Op::Other(_) => pc += skip,
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
            // `0x80071a00` / `0x80071a4c` / `0x80071a98`: degrees times the f32
            // `0.017453292` (`fmuls`), then double `sin` / `cos` / `tan`, rounded to f32.
            (11..=13, _) => {
                let r = f64::from(f * 0.017_453_292);
                Float(match func {
                    11 => r.sin(),
                    12 => r.cos(),
                    _ => r.tan(),
                } as f32)
            }
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
            // A record reached through an alias at any prefix: that record's path.
            if let Some((k, a)) =
                (1..=p.len())
                    .rev()
                    .find_map(|k| match self.refs_of(&p[..k]).get(&p[..k]) {
                        Some(RefTarget::Alias(a)) => Some((k, a.clone())),
                        // One of this actor's records, as another actor stored it.
                        Some(&RefTarget::Record { actor, var }) if Some(actor) == self.self_key => {
                            Some((k, vec![Ref::Obj(var)]))
                        }
                        _ => None,
                    })
            {
                p = a.into_iter().chain(p[k..].iter().copied()).collect();
                continue;
            }
            let alias = self
                .index
                .aliases
                .get(o)
                .map(|&(si, oi)| &self.set.scripts[si].ops[oi].op);
            match alias {
                // `else if` checks name the first check: its LHS (`check @186d30 == ...`).
                Some(Op::CheckValue { lhs, .. } | Op::CheckRef { lhs, .. }) => {
                    p = lhs.iter().chain(&p[1..]).copied().collect();
                }
                Some(Op::FindVariable { .. }) if self.found_own.contains_key(o) => {
                    p = [Ref::Obj(self.found_own[o])]
                        .into_iter()
                        .chain(p[1..].iter().copied())
                        .collect();
                }
                Some(Op::FindVariable { .. }) if self.found.contains_key(o) => break,
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

    /// Board slot of a path that names a bound `OpFindVariable`.
    fn found_slot(&self, path: &[Ref]) -> Option<(usize, String)> {
        match path {
            [Ref::Obj(o)] => self.found.get(o).cloned(),
            _ => None,
        }
    }

    fn read(&mut self, path: &[Ref]) -> Value {
        let path = match self.canon(path) {
            Ok(p) => p,
            Err(i) => return Value::Int(i),
        };
        if let Some((a, name)) = self.found_slot(&path) {
            return self.board[&a][&name];
        }
        if let Some(v) = self.engine_ref_key(&path).and_then(|k| self.engine.get(&k)) {
            return *v;
        }
        if let Some(v) = self.referenced_field(&path) {
            return v;
        }
        if let Some(v) = self.vars_of(&path).get(&path) {
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
            // `<script set>.count`.
            [ref set @ .., Ref::Handle(c)]
                if self.set.engine_key(&[Ref::Handle(c)]) == Some(vec![SET_COUNT])
                    && self.sets_of(set).contains_key(set) =>
            {
                Some(Value::Int(self.sets_of(set)[set].len() as i32))
            }
            // `<global set>.count`: what the engine put in it, else its static members.
            [Ref::Handle(h), Ref::Handle(c)]
                if self.set.engine_key(&[Ref::Handle(c)]) == Some(vec![SET_COUNT]) =>
            {
                self.global.and_then(|g| {
                    let name = self.set.global_set_name(h, g)?;
                    Some(Value::Int(
                        match (self.engine_sets.get(name), self.set_counts.get(name)) {
                            (Some(m), _) => m.len() as i32,
                            (None, Some(&n)) => n,
                            _ => self.set.global_set(h, g)?.len() as i32,
                        },
                    ))
                })
            }
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
            if let Some((a, name)) = self.found_slot(&p) {
                self.board.entry(a).or_default().insert(name, v);
                return;
            }
            if let Some(k) = self.engine_ref_key(&p) {
                self.engine.insert(k, v);
                return;
            }
            match self.target(&p) {
                Some(Target::Clone(i, k)) => {
                    self.spawning[i].seed.insert(k, v);
                    return;
                }
                // Kept here too, so the script reads back what it wrote.
                Some(Target::Spawned(serial, key)) => self.effects.push(Effect::ForClone {
                    serial,
                    effect: Box::new(Effect::Set { key, value: v }),
                }),
                Some(Target::Actor(id, key)) => self.effects.push(Effect::ForActor {
                    id,
                    effect: Box::new(Effect::Set { key, value: v }),
                }),
                _ => {}
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
                    self.vars_of_mut(&p).insert(p, v);
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
        assert!(matches!(f(12, F(180.0)), Value::Float(x) if (x + 1.0).abs() < 1e-6));
        assert!(matches!(f(12, I(60)), Value::Float(x) if (x - 0.5).abs() < 1e-6));
        assert!(matches!(f(13, F(45.0)), Value::Float(x) if (x - 1.0).abs() < 1e-6));
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
    fn vm_runs_global_macros_by_name() {
        // `global.bld` "Double": `out = in * 2`, and it writes `my.<0x77>` (engine state).
        let (gin, gout) = (0x500, 0x510);
        let mut g = script_set(
            vec![
                ins(
                    1,
                    5,
                    Op::DefineMacro {
                        name: "Double".into(),
                    },
                ),
                ins(2, 2, Op::Interface),
                param(gin, 0, Some(lit(I(0)))),
                param(gout, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(3, 0, set(gout, bin(rd(gin), 2, Operand::Lit(I(2))))),
                ins(
                    4,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Handle(0), Ref::Handle(1)],
                        rhs: lit(I(5)),
                    },
                ),
            ],
            &[(gin, "in"), (gout, "out")],
        );
        g.handle_ids = vec![MY_HASH, 0x77];
        g.handle_ns = vec![name_hash("level.bld"); 2];
        // The level calls it through a stub (`use ?`): its own params, matched by name.
        let (stub, lin, lout, r) = (0xd00, 0x100, 0x110, 0x120);
        let mut l = script_set(
            vec![
                ins(1, 7, Op::DefineMacro { name: "m".into() }),
                ins(2, 1, Op::Interface),
                param(r, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(3, 3, Op::UseMacro { target: Some(stub) }),
                ins(4, 2, Op::Interface),
                param(lin, 0, Some(lit(I(21)))),
                param(lout, PARAM_OUTPUT, Some(lit(I(-1)))),
                ins(
                    5,
                    0,
                    set(
                        r,
                        Rhs {
                            a: rd(lout),
                            arith: -1,
                            b: None,
                            func: 0,
                        },
                    ),
                ),
            ],
            &[(lin, "in"), (lout, "out"), (r, "r")],
        );
        l.macro_names.insert(stub, "Double".into());
        let gi = VmIndex::new(&g);
        let mut vm = Vm::new(&l, Some(&g));
        vm.global_index = Some(&gi);
        assert_eq!(out_of(vm.call("m", &[]).unwrap(), "r"), I(42));
        assert_eq!(vm.engine.get(&vec![MY_HASH, 0x77]), Some(&I(5)));
        assert!(vm.unbound.is_empty(), "{:?}", vm.unbound);

        // Without `global.bld`'s index the call keeps its literals, as before.
        let mut vm = Vm::new(&l, Some(&g));
        assert_eq!(out_of(vm.call("m", &[]).unwrap(), "r"), I(-1));
    }

    #[test]
    fn vm_never_set_record_reference_is_null() {
        // `Sky_Traps.ai`: `"trapping data"."defeat data"."defeated actor" != null` is false
        // until the trap sequence sets it (it used to count as set).
        let (rec, f1, f2, r) = (0x100, 0x110, 0x120, 0x130);
        let mut s = script_set(
            vec![
                ins(1, 4, Op::DefineMacro { name: "m".into() }),
                ins(2, 1, Op::Interface),
                param(r, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(
                    3,
                    1,
                    Op::CheckRef {
                        rel: 5,
                        desc: "OpCheckReference".into(),
                        lhs: vec![Ref::Obj(rec), Ref::Obj(f1), Ref::Obj(f2)],
                        rhs: vec![Ref::Handle(0)],
                    },
                ),
                ins(4, 0, set(r, lit(I(1)))),
            ],
            &[
                (rec, "trapping data"),
                (f1, "defeat data"),
                (f2, "defeated actor"),
                (r, "r"),
            ],
        );
        s.vars.get_mut(&rec).unwrap().type_name = "ScriptStructure".into();
        s.handle_ids = vec![NULL_HASH];
        s.handle_ns = vec![name_hash("level.bld")];
        assert_eq!(out_of(Vm::new(&s, None).call("m", &[]).unwrap(), "r"), I(0));
    }

    #[test]
    fn vm_dynamic_use_runs_the_held_macro_on_the_callers_record() {
        // `Transport_Macros.ai`: `"My Info"."Transport Type" = "Airship - Movement"`, then
        // `use "info"."Transport Type" (info = "My Info")`: the specialization runs, and its
        // `"info".x = 7` lands in the caller's record.
        let (rec, tt, x, r) = (0x100, 0x110, 0x120, 0x130);
        let (site_info, def_info, spec, call) = (0x200, 0x210, 0x300, 0x400);
        let mut s = script_set(
            vec![
                ins(1, 7, Op::DefineMacro { name: "m".into() }),
                ins(2, 1, Op::Interface),
                param(r, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(
                    3,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(rec), Ref::Obj(tt)],
                        rhs: vec![Ref::Obj(spec)],
                        desc: String::new(),
                    },
                ),
                ins(call, 2, Op::UseMacro { target: None }),
                ins(5, 1, Op::Interface),
                param(site_info, 0, None),
                ins(
                    6,
                    0,
                    set(
                        r,
                        Rhs {
                            a: Operand::Read {
                                path: vec![Ref::Obj(rec), Ref::Obj(x)],
                                kind: NumKind::Int,
                            },
                            arith: -1,
                            b: None,
                            func: 0,
                        },
                    ),
                ),
                ins(
                    spec,
                    3,
                    Op::DefineMacro {
                        name: "Airship - Movement".into(),
                    },
                ),
                ins(7, 1, Op::Interface),
                param(def_info, PARAM_OUTPUT, None),
                ins(
                    8,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(def_info), Ref::Obj(x)],
                        rhs: lit(I(7)),
                    },
                ),
            ],
            &[(rec, "My Info"), (tt, "Transport Type"), (x, "x"), (r, "r")],
        );
        for (v, n) in [(site_info, "info"), (def_info, "info")] {
            s.vars.insert(
                v,
                VarInfo {
                    name: n.into(),
                    type_name: "OpMacroParameter".into(),
                    init: None,
                },
            );
        }
        s.use_paths.insert(call, vec![Ref::Obj(rec), Ref::Obj(tt)]);
        s.opaque_operands.insert(
            site_info,
            vec![(
                0x30,
                OpaqueOperand::Path {
                    stack: "RHSReferenceStack".into(),
                    path: vec![Ref::Obj(rec)],
                },
            )],
        );
        assert_eq!(out_of(Vm::new(&s, None).call("m", &[]).unwrap(), "r"), I(7));
    }

    #[test]
    fn vm_spawned_record_is_returned_by_reference() {
        // `Transport_Macros.ai` "Transport - Startup": fill `"New Info"`, `spawn "New Info"`,
        // `"returned info" = @spawn`; the caller then writes through `"returned info"`,
        // to a new instance with the template's fields.
        let (rec, f, out, r, sp) = (0x100, 0x110, 0x120, 0x130, 0x200);
        let mut s = script_set(
            vec![
                ins(1, 7, Op::DefineMacro { name: "m".into() }),
                ins(2, 1, Op::Interface),
                param(r, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(
                    3,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(rec), Ref::Obj(f)],
                        rhs: lit(I(2)),
                    },
                ),
                ins(
                    sp,
                    0,
                    Op::Spawn {
                        template: vec![Ref::Obj(rec)],
                        actor: None,
                        at: Vec::new(),
                        facing: Some(lit(I(0))),
                    },
                ),
                ins(
                    4,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(out)],
                        rhs: vec![Ref::Obj(sp)],
                        desc: String::new(),
                    },
                ),
                ins(
                    5,
                    0,
                    Op::IncValue {
                        lhs: vec![Ref::Obj(out), Ref::Obj(f)],
                    },
                ),
                ins(
                    6,
                    0,
                    set(
                        r,
                        Rhs {
                            a: Operand::Read {
                                path: vec![Ref::Obj(out), Ref::Obj(f)],
                                kind: NumKind::Int,
                            },
                            arith: -1,
                            b: None,
                            func: 0,
                        },
                    ),
                ),
            ],
            &[
                (rec, "New Info"),
                (f, "x"),
                (out, "returned info"),
                (r, "r"),
            ],
        );
        s.vars.get_mut(&rec).unwrap().type_name = "OpCreateVariable".into();
        s.vars.get_mut(&out).unwrap().type_name = "OpCreateVariable".into();
        // `"New Info"` has members (a path goes through it).
        s.records.insert(rec);
        let mut vm = Vm::new(&s, None);
        // The spawned instance copied x = 2 and was incremented; the template keeps 2.
        assert_eq!(out_of(vm.call("m", &[]).unwrap(), "r"), I(3));
        assert_eq!(vm.read(&[Ref::Obj(rec), Ref::Obj(f)]), I(2));
    }

    #[test]
    fn vm_global_sets_are_shared_and_absolute() {
        // `Combat.ai` registers each creature (`my`) in `global.bld`'s "active" set; the set
        // is one table for all actors, so `my` is stored as the actor's engine id.
        let active = 0x600;
        let mut g = script_set(
            vec![
                ins(
                    1,
                    3,
                    Op::DefineMacro {
                        name: "Register".into(),
                    },
                ),
                ins(2, 0, Op::Interface),
                ins(
                    3,
                    0,
                    Op::ChangeMembership {
                        set: vec![Ref::Obj(active)],
                        op: 0,
                        group: vec![Ref::Handle(0)],
                    },
                ),
            ],
            &[(active, "active")],
        );
        g.handle_ids = vec![MY_HASH];
        g.handle_ns = vec![name_hash("level.bld")];
        let stub = 0xd00;
        let mut l = script_set(
            vec![
                ins(1, 2, Op::DefineMacro { name: "m".into() }),
                ins(2, 0, Op::Interface),
                ins(3, 1, Op::UseMacro { target: Some(stub) }),
                ins(4, 0, Op::Interface),
            ],
            &[],
        );
        l.macro_names.insert(stub, "Register".into());
        let gi = VmIndex::new(&g);
        let mut shared = HashMap::new();
        for id in [5, 6] {
            let mut vm = Vm::new(&l, Some(&g));
            vm.global_index = Some(&gi);
            vm.self_id = Some(id);
            vm.global_sets = std::mem::take(&mut shared);
            vm.call("m", &[]).unwrap();
            shared = std::mem::take(&mut vm.global_sets);
        }
        assert_eq!(
            shared.get(&vec![Ref::Obj(active)]),
            Some(&vec![RefTarget::Actor(5), RefTarget::Actor(6)])
        );
    }

    #[test]
    fn vm_inc_count_grows_a_list_of_records() {
        // `Combat.ai`: `inc "active".count; "info" = "active".last; "info".creature = ...`.
        let (list, info, f, n, r) = (0x100, 0x110, 0x120, 0x130, 0x140);
        let mut s = script_set(
            vec![
                ins(1, 9, Op::DefineMacro { name: "m".into() }),
                ins(2, 2, Op::Interface),
                param(n, PARAM_OUTPUT, Some(lit(I(0)))),
                param(r, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(
                    3,
                    0,
                    Op::IncValue {
                        lhs: vec![Ref::Obj(list), Ref::Handle(0)],
                    },
                ),
                ins(
                    4,
                    0,
                    Op::IncValue {
                        lhs: vec![Ref::Obj(list), Ref::Handle(0)],
                    },
                ),
                ins(
                    5,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(info)],
                        rhs: vec![Ref::Obj(list), Ref::Handle(1)],
                        desc: String::new(),
                    },
                ),
                ins(
                    6,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(info), Ref::Obj(f)],
                        rhs: lit(I(5)),
                    },
                ),
                ins(
                    7,
                    0,
                    set(
                        n,
                        Rhs {
                            a: Operand::Read {
                                path: vec![Ref::Obj(list), Ref::Handle(0)],
                                kind: NumKind::Int,
                            },
                            arith: -1,
                            b: None,
                            func: 0,
                        },
                    ),
                ),
                ins(
                    8,
                    0,
                    set(
                        r,
                        Rhs {
                            a: Operand::Read {
                                path: vec![Ref::Obj(info), Ref::Obj(f)],
                                kind: NumKind::Int,
                            },
                            arith: -1,
                            b: None,
                            func: 0,
                        },
                    ),
                ),
            ],
            &[
                (list, "active"),
                (info, "info"),
                (f, "creature"),
                (n, "n"),
                (r, "r"),
            ],
        );
        s.handle_ids = vec![SET_COUNT, LAST];
        s.handle_ns = vec![name_hash("level.bld"); 2];
        let out = Vm::new(&s, None).call("m", &[]).unwrap();
        assert_eq!(out_of(out.clone(), "n"), I(2));
        assert_eq!(out_of(out, "r"), I(5));
    }

    #[test]
    fn vm_reference_param_takes_its_default() {
        // "Transport - Startup"'s "transport" defaults to `my` (`RHSReferenceStack` +0x30)
        // when the call passes none.
        let (p, out, r) = (0x200, 0x210, 0x220);
        let mut s = script_set(
            vec![
                ins(1, 3, Op::DefineMacro { name: "m".into() }),
                ins(2, 1, Op::Interface),
                param(p, 0, None),
                ins(
                    3,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(out)],
                        rhs: vec![Ref::Obj(p)],
                        desc: String::new(),
                    },
                ),
                ins(
                    10,
                    3,
                    Op::DefineMacro {
                        name: "caller".into(),
                    },
                ),
                ins(11, 1, Op::Interface),
                param(r, PARAM_OUTPUT, Some(lit(I(0)))),
                ins(12, 0, Op::UseMacro { target: Some(1) }),
            ],
            &[(p, "transport"), (out, "out"), (r, "r")],
        );
        s.handle_ids = vec![MY_HASH];
        s.handle_ns = vec![name_hash("level.bld")];
        s.opaque_operands.insert(
            p,
            vec![(
                0x30,
                OpaqueOperand::Path {
                    stack: "RHSReferenceStack".into(),
                    path: vec![Ref::Handle(0)],
                },
            )],
        );
        let mut vm = Vm::new(&s, None);
        vm.call("caller", &[]).unwrap();
        assert_eq!(
            vm.ref_target(&[Ref::Obj(out)]),
            RefTarget::Engine(vec![MY_HASH])
        );
    }

    #[test]
    fn vm_set_reference_rebinds_an_alias_variable() {
        // `"My Info" = <other>` rebinds the variable; the record it stood for keeps its own
        // binding.
        let (rec, v, other) = (0x300, 0x310, 0x320);
        let mut s = script_set(
            vec![
                ins(1, 2, Op::DefineMacro { name: "m".into() }),
                ins(2, 0, Op::Interface),
                ins(
                    3,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(v)],
                        rhs: vec![Ref::Obj(other)],
                        desc: String::new(),
                    },
                ),
            ],
            &[(rec, "New Info"), (v, "My Info"), (other, "Other")],
        );
        s.placements.insert(other);
        let mut vm = Vm::new(&s, None);
        vm.refs
            .insert(vec![Ref::Obj(v)], RefTarget::Alias(vec![Ref::Obj(rec)]));
        vm.call("m", &[]).unwrap();
        assert_eq!(
            vm.refs.get(&vec![Ref::Obj(v)]),
            Some(&RefTarget::Placement(other))
        );
        assert_eq!(vm.refs.get(&vec![Ref::Obj(rec)]), None);
    }

    #[test]
    fn vm_find_variable_on_itself_binds_its_own_variable() {
        // `ArenaRules_AM2.ai` "Artillery - Startup": `find variable "Mod Controller"` on its
        // own placement, then `set reference @find = @spawn`: its own variable changes (the
        // board copy is replaced by its export after the tick).
        let (me, mc, fv, other) = (0x300, 0x310, 0x320, 0x330);
        let mut s = script_set(
            vec![
                ins(1, 3, Op::DefineMacro { name: "m".into() }),
                ins(2, 0, Op::Interface),
                ins(
                    fv,
                    1,
                    Op::FindVariable {
                        name: "Mod Controller".into(),
                        target: vec![Ref::Obj(me)],
                    },
                ),
                ins(
                    4,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(fv)],
                        rhs: vec![Ref::Obj(other)],
                        desc: String::new(),
                    },
                ),
            ],
            &[(mc, "Mod Controller")],
        );
        s.placements.extend([me, other]);
        let mut vm = Vm::new(&s, None);
        vm.self_key = Some(me);
        vm.own_vars.insert("Mod Controller".into(), mc);
        vm.board
            .entry(me)
            .or_default()
            .insert("Mod Controller".into(), I(0));
        vm.call("m", &[]).unwrap();
        assert_eq!(
            vm.refs.get(&vec![Ref::Obj(mc)]),
            Some(&RefTarget::Placement(other))
        );
    }

    #[test]
    fn vm_string_ids_compare_and_else_if_checks_name_the_first() {
        // "ArenaMaster - Change Mod": `"New Mod" == "Mod - Artillery"` (`StringInfo`
        // constants), then `else if` checks that name the first check (`@186d30`).
        let (v, s1, s2, c1, c2, out1, out2) = (0x300, 0x310, 0x320, 0x330, 0x340, 0x350, 0x360);
        let check = |rhs: usize, lhs: usize| Op::CheckRef {
            rel: 1,
            desc: "OpCheckReference".into(),
            lhs: vec![Ref::Obj(lhs)],
            rhs: vec![Ref::Obj(rhs)],
        };
        let mut s = script_set(
            vec![
                ins(1, 7, Op::DefineMacro { name: "m".into() }),
                ins(2, 0, Op::Interface),
                ins(
                    3,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(v)],
                        rhs: vec![Ref::Obj(s2)],
                        desc: String::new(),
                    },
                ),
                ins(c1, 1, check(s1, v)),
                ins(5, 0, set(out1, lit(I(1)))),
                ins(c2, 1, check(s2, c1)),
                ins(7, 0, set(out2, lit(I(1)))),
            ],
            &[(v, "New Mod"), (out1, "o1"), (out2, "o2")],
        );
        s.strings.extend([s1, s2]);
        let mut vm = Vm::new(&s, None);
        vm.call("m", &[]).unwrap();
        assert_eq!(vm.vars.get(&vec![Ref::Obj(out1)]), None);
        assert_eq!(vm.vars.get(&vec![Ref::Obj(out2)]), Some(&I(1)));
    }

    #[test]
    fn vm_another_actors_reference_to_its_own_record_is_that_record() {
        // "Selected Mod" = "Arena Mod - Artillery", set by another actor as a `Record` of
        // this one: paths through it are the record's own.
        let (me, sel, rec, f) = (0x300, 0x310, 0x320, 0x330);
        let mut s = script_set(
            vec![
                ins(1, 2, Op::DefineMacro { name: "m".into() }),
                ins(2, 0, Op::Interface),
                ins(
                    3,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(sel), Ref::Obj(f)],
                        rhs: lit(I(5)),
                    },
                ),
            ],
            &[
                (sel, "Selected Mod"),
                (rec, "Arena Mod - Artillery"),
                (f, "x"),
            ],
        );
        s.records.insert(rec);
        s.vars.get_mut(&rec).unwrap().type_name = "OpCreateVariable".into();
        let mut vm = Vm::new(&s, None);
        vm.self_key = Some(me);
        vm.refs.insert(
            vec![Ref::Obj(sel)],
            RefTarget::Record {
                actor: me,
                var: rec,
            },
        );
        vm.call("m", &[]).unwrap();
        assert_eq!(vm.vars.get(&vec![Ref::Obj(rec), Ref::Obj(f)]), Some(&I(5)));
        assert_eq!(
            vm.ref_target(&[Ref::Obj(sel)]),
            RefTarget::Alias(vec![Ref::Obj(rec)])
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
    fn rom_records_are_structure_typed_variables() {
        let (Some(arena), Some(g)) = (
            read_bld("level", "Arena_Challenge_300_000").and_then(|d| ScriptSet::parse(&d)),
            read_bld("permanent", "global").and_then(|d| ScriptSet::parse(&d)),
        ) else {
            return;
        };
        // `Transport_Macros.ai` "New Info" (spawned and returned by reference) is typed by an
        // `OpDefineStructure`; `Sky_Traps.ai` "trapping data" is a `ScriptStructure`, its
        // "defeated actor" field is not a record.
        assert_eq!(arena.var_name(0x1bb1f8), "New Info");
        assert!(arena.records.contains(&0x1bb1f8));
        // "Enemy - payload" is dereferenced too, but typed by an engine class: a reference.
        assert_eq!(arena.var_name(0x1bd340), "Enemy - payload");
        assert!(!arena.records.contains(&0x1bd340));
        // `ArenaRules_AM2.ai`: "Arena Mod - Artillery" is a record (container
        // `ScriptStructure`), "Selected Mod" a reference to one (`ScriptReference`); mod ids
        // are `StringInfo` constants ("Mod - Artillery").
        assert_eq!(arena.var_name(0x1518d4), "Arena Mod - Artillery");
        assert!(arena.records.contains(&0x1518d4));
        assert_eq!(arena.var_name(0x18cf98), "Selected Mod");
        assert!(!arena.records.contains(&0x18cf98));
        assert!(arena.strings.contains(&0x120d30));
        assert!(g.records.contains(&0x296dec));
        assert!(!g.records.contains(&0x29fe78));
    }

    #[test]
    fn rom_placement_tags_are_its_variables() {
        let Some(arena) =
            read_bld("level", "Arena_Challenge_300_000").and_then(|d| ScriptSet::parse(&d))
        else {
            return;
        };
        // `GroupTransport_Helecopter` (0x1211a0): "flight path" (what `BattleMaster.ai` sets
        // through `find variable` on the spawned dropship), then its payload slots.
        let names: Vec<_> = arena.tags[&0x1211a0]
            .iter()
            .map(|&g| arena.var_name(g))
            .collect();
        assert_eq!(names[..3], ["flight path", "payload", "enemy1L"]);
        assert_eq!(
            arena.tag_refs[&0x1212e0],
            RefTarget::Engine(vec![0x9b61_ad43])
        );
        assert_eq!(arena.tag_refs[&0x12134c], RefTarget::Null);
        // `Enemy_S4_BirdCopter_(armored)`: value tags take their literal default.
        let armored = arena.tags[&0xaf6f0][2];
        assert_eq!(arena.var_name(armored), "armored");
        assert_eq!(arena.vars[&armored].init, Some(I(1)));
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
