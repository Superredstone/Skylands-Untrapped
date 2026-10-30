//! Per-actor script execution. A placed actor's `.ai` script is an `OpPreScript` block
//! holding an `OpStartUp` block (run once), an `OpShutDown` block (not run) and the
//! per-tick body (everything else). The actor itself is the engine object `my`; its
//! fields are name-hash paths (`ScriptSet::engine_key`). Notes: `agent/SCRIPT_VM.md`.

use crate::script::{
    name_hash, Effect, Op, Ref, RefBoard, RefTarget, ScriptSet, Value, VarBoard, Vm, VmError,
    VmIndex,
};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// FNV-1a name hashes of engine fields (names cracked against `main.dol` strings).
pub mod field {
    pub const MY: u32 = 0x4c2d_fe93;
    pub const SCRIPT_TIMER: u32 = 0x6fb9_e9ff;
    /// Name not recovered; parent of `facing` / `tilt` / `roll`.
    pub const ORIENTATION: u32 = 0xc545_bc21;
    pub const FACING: u32 = 0x86d0_5dcf;
    pub const TILT: u32 = 0x603a_6206;
    pub const ROLL: u32 = 0x16ff_3d1e;
    pub const RELATIVE_FACING: u32 = 0xe47f_9549;
    pub const RELATIVE_TILT: u32 = 0xe206_6570;
    pub const RELATIVE_ROLL: u32 = 0x86aa_d178;
    pub const VERTICAL_OFFSET: u32 = 0x6c74_9d1e;
    pub const SCALE: u32 = 0x8297_1c71;
    pub const UNIFORM: u32 = 0xde37_5a53;
    pub const LOOPING: u32 = 0x7593_992d;
    pub const PLAYBACK_MODE: u32 = 0x056c_6ca7;
    /// Sequence group of `my`'s `tfbAnimationState`s (`OpStartSequence` index = state id).
    pub const ANIMATION_PALETTE: u32 = 0x8d18_097e;
    pub const VISIBLE: u32 = 0x54fa_99a1;
    /// `my.tint.alpha`: 0..255, starts at `Placement._tint` alpha.
    pub const TINT: u32 = 0x5c35_de8c;
    pub const ALPHA: u32 = 0x5d8b_6dab;
    /// `my.collision volume.weight`: `CollisionInfo._weight`.
    pub const COLLISION_VOLUME: u32 = 0xdad6_4577;
    pub const WEIGHT: u32 = 0x50a0_9eb9;
    /// `my.velocity.length / heading / pitch` (`global.bld` "Movement - Set Velocity"):
    /// the engine moves the actor by it every tick (units per second, degrees).
    pub const VELOCITY: u32 = 0x3274_1c32;
    pub const LENGTH: u32 = 0x83d0_3615;
    pub const HEADING: u32 = 0x9f48_1715;
    pub const PITCH: u32 = 0xbd32_4ab1;
}

/// `my` fields that move, turn, scale or remove the actor.
fn is_transform_key(k: &[u32]) -> bool {
    use field::*;
    match k {
        [MY, ORIENTATION, f] => [
            FACING,
            TILT,
            ROLL,
            RELATIVE_FACING,
            RELATIVE_TILT,
            RELATIVE_ROLL,
        ]
        .contains(f),
        [MY, VERTICAL_OFFSET] | [MY, SCALE, UNIFORM] => true,
        _ => false,
    }
}

/// A level's scripts, ready to run for many actors.
pub struct ScriptProgram {
    pub set: ScriptSet,
    pub global: Option<ScriptSet>,
    index: VmIndex,
    /// `global.bld`'s index, to run its macros from level scripts.
    global_index: Option<VmIndex>,
}

/// `OpSlideValue` in flight: `lhs` moves `from` -> `to` over `seconds`.
#[derive(Debug, Clone, PartialEq)]
struct Slider {
    op: usize,
    lhs: Vec<Ref>,
    from: f32,
    to: f32,
    seconds: f32,
    ease_out: f32,
    ease_in: f32,
    t: f32,
}

/// Slider progress curve (`0x8008ec38`): accelerate over the first `a` of the slide,
/// constant speed, decelerate over the last `b`; linear when both are 0. `a + b`
/// over 1 is scaled down to 1.
fn ease_curve(u: f32, a: f32, b: f32) -> f32 {
    let sum = a + b;
    if sum == 0.0 {
        return u;
    }
    let (a, b) = if sum > 1.0 {
        (a / sum, b / sum)
    } else {
        (a, b)
    };
    let v = 1.0 / (2.0 - a - b);
    if u < a {
        v * u * u / a
    } else if u < 1.0 - b {
        v * (2.0 * u - a)
    } else {
        1.0 - v * (1.0 - u) * (1.0 - u) / b
    }
}

/// One actor's script state. The engine seeds and reads `engine`
/// (`[MY, ORIENTATION, FACING]` = degrees, ...).
#[derive(Debug, Clone, Default)]
pub struct ActorScriptState {
    vars: HashMap<Vec<Ref>, Value>,
    pub engine: HashMap<Vec<u32>, Value>,
    sliders: Vec<Slider>,
    started: bool,
    /// Seconds since the actor started (`Script Timer`).
    pub time: f32,
    /// Shared story/engine globals for this actor's scripts (e.g. "Levels_Completed").
    pub globals: HashMap<String, Value>,
    /// Engine-filled `global.bld` set sizes by name (`Vm::set_counts`).
    pub set_counts: HashMap<String, i32>,
    /// Engine-filled `global.bld` set members, this actor's engine id and the engine's
    /// member fields (`Vm::engine_sets`, `Vm::self_id`, `Vm::fields`).
    pub engine_sets: HashMap<String, Vec<RefTarget>>,
    pub self_id: Option<u64>,
    pub fields: Option<std::sync::Arc<dyn crate::script::MemberFields>>,
    /// Debugging: when `Some`, the ops each tick runs are appended (`Vm::trace`).
    pub trace: Option<Vec<(bool, usize)>>,
    /// Other actors' variables for `OpFindVariable`; the engine moves its board in for
    /// the tick and takes it back after.
    pub board: VarBoard,
    pub ref_board: RefBoard,
    /// Sum of `OpDisplace` moves of `my`, world units (heading 0 = +Z, pitch 90 = up).
    pub displacement: [f32; 3],
    /// `OpRemove my` ran.
    pub removed: bool,
    /// `tfbAnimationState` id last started on the `animation palette`.
    pub anim_state: Option<i32>,
    /// `random` generator state; seed it per actor for distinct, repeatable runs.
    pub rng: u64,
    /// Op offset of the current `OpUserBehavior`.
    behavior: Option<usize>,
    refs: HashMap<Vec<Ref>, RefTarget>,
    /// The script's sets (`OpChangeMembership`), by canonical path.
    sets: HashMap<Vec<Ref>, Vec<RefTarget>>,
    /// `global.bld` variables, references and sets. One level-wide table in the game:
    /// the engine keeps a single copy and moves it in for each actor's tick.
    pub global_vars: HashMap<Vec<Ref>, Value>,
    pub global_refs: HashMap<Vec<Ref>, RefTarget>,
    pub global_sets: HashMap<Vec<Ref>, Vec<RefTarget>>,
    /// The script hit an op the VM does not model; it no longer runs.
    pub error: Option<VmError>,
    /// Save slot read by "Get Save Param" (set by the engine each tick).
    pub save: Option<std::sync::Arc<dyn crate::script::SaveParams>>,
    /// "Set Save Param" writes since the engine last drained them.
    pub save_writes: Vec<(String, i32, i64)>,
    /// Effects on earlier clones by serial (`Effect::ForClone`), for the engine to
    /// deliver after the tick.
    pub clone_effects: Vec<(u32, Effect)>,
    /// Effects on other actors by engine id (`Effect::ForActor`).
    pub actor_effects: Vec<(u64, Effect)>,
    /// Engine ids of this actor's clones by serial, and its own variable board key
    /// (`Vm::clone_ids`, `Vm::self_key`), set by the engine before each tick.
    pub clone_ids: HashMap<u32, u64>,
    pub self_key: Option<usize>,
    /// Tags of this actor's placement (`ScriptSet::tags`): variables it owns besides its
    /// script's, set by the engine when the actor is made.
    pub tags: Vec<usize>,
    /// `OpSpawn` clones made so far (next `Effect::Spawn::serial`).
    pub spawned: u32,
    /// `Effect::Spawn`s since the engine last drained them.
    pub spawns: Vec<Effect>,
}

impl ActorScriptState {
    /// Op offset of the current `OpUserBehavior` (`ScriptProgram::behavior_name`).
    pub fn behavior(&self) -> Option<usize> {
        self.behavior
    }

    /// Every stored reference (path -> target), for debugging.
    pub fn references(&self) -> impl Iterator<Item = (&Vec<Ref>, &RefTarget)> {
        self.refs.iter()
    }

    /// What the reference variable or `PlacementReference` object `obj` holds.
    pub fn reference(&self, obj: usize) -> Option<&RefTarget> {
        self.refs.get(&vec![Ref::Obj(obj)])
    }

    /// Point `obj` (a level `PlacementReference`) at `target`, as the engine sees it now.
    pub fn set_reference(&mut self, obj: usize, target: RefTarget) {
        self.refs.insert(vec![Ref::Obj(obj)], target);
    }

    pub fn get(&self, key: &[u32]) -> Option<f32> {
        self.engine.get(key).map(|v| v.as_f32())
    }
    pub fn set(&mut self, key: &[u32], v: f32) {
        self.engine.insert(key.to_vec(), Value::Float(v));
    }

    /// Apply an engine effect aimed at `my` (others are ignored): `OpDisplace` moves,
    /// `OpRemove` despawns, `OpStartSequence` on the `animation palette` picks the
    /// animation state, `OpTurnTo` sets the facing, `Set` writes a field and `Slide`
    /// starts a slider. Also used for the effects on an `OpSpawn` clone
    /// (`Effect::Spawn::setup`, `clone_effects`).
    pub fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::Slide {
                op,
                lhs,
                from,
                to,
                seconds,
                ease_out,
                ease_in,
            } => {
                self.sliders.retain(|s| s.op != op);
                self.sliders.push(Slider {
                    op,
                    lhs,
                    from,
                    to,
                    seconds,
                    ease_out,
                    ease_in,
                    t: 0.0,
                });
            }
            Effect::Set { key, value } if key.first() == Some(&field::MY) => {
                self.engine.insert(key, value);
            }
            Effect::Displace {
                target,
                len,
                heading,
                pitch,
            } if target == [field::MY] => {
                let (sh, ch) = heading.to_radians().sin_cos();
                let (sp, cp) = pitch.to_radians().sin_cos();
                for (d, v) in self.displacement.iter_mut().zip([cp * sh, sp, cp * ch]) {
                    *d += len * v;
                }
            }
            Effect::Remove { target } if target == [field::MY] => self.removed = true,
            Effect::StartSequence { group, state } if group == [field::ANIMATION_PALETTE] => {
                self.anim_state = Some(state)
            }
            // ponytail: the turn is applied at once; the engine turns over time (target
            // vtable +0x90, rate unknown). Add the rate once it is reversed.
            Effect::TurnTo { target, facing } if target.first() == Some(&field::MY) => {
                self.set(&[field::MY, field::ORIENTATION, field::FACING], facing);
            }
            _ => {}
        }
    }
}

impl ScriptProgram {
    pub fn new(set: ScriptSet, global: Option<ScriptSet>) -> Self {
        let index = VmIndex::new(&set);
        let global_index = global.as_ref().map(VmIndex::new);
        Self {
            set,
            global,
            index,
            global_index,
        }
    }

    /// Variables of an actor running script `si` (`_masterVarList`, then its placement's
    /// tags): object and name.
    fn declared<'a>(
        &'a self,
        si: usize,
        st: &'a ActorScriptState,
    ) -> impl Iterator<Item = (usize, &'a str)> {
        self.set
            .master_vars
            .get(si)
            .into_iter()
            .flatten()
            .chain(&st.tags)
            .filter(|v| self.set.vars.contains_key(v))
            .map(|&v| (v, self.set.var_name(v)))
    }

    /// Values of script `si`'s variables by name (initial values until written), for
    /// other actors' `OpFindVariable`.
    pub fn export_vars(&self, si: usize, st: &ActorScriptState) -> HashMap<String, Value> {
        self.declared(si, st)
            .map(|(var, name)| {
                let v = st
                    .vars
                    .get(&vec![Ref::Obj(var)])
                    .copied()
                    .unwrap_or_else(|| {
                        self.set
                            .vars
                            .get(&var)
                            .and_then(|i| i.init)
                            .unwrap_or(Value::Int(0))
                    });
                (name.to_string(), v)
            })
            .collect()
    }

    /// References in script `si`'s variables by name, as any actor reads them (`my` and
    /// this actor's clones as engine ids), for other actors' `OpFindVariable`.
    pub fn export_refs(&self, si: usize, st: &ActorScriptState) -> HashMap<String, RefTarget> {
        self.declared(si, st)
            .filter_map(|(var, name)| {
                let t = st
                    .refs
                    .get(&vec![Ref::Obj(var)])
                    .or_else(|| self.set.tag_refs.get(&var))?
                    .clone();
                let t = match t {
                    RefTarget::Engine(ref k) if *k == [field::MY] => {
                        st.self_id.map_or(t, RefTarget::Actor)
                    }
                    RefTarget::Spawned(s) => {
                        st.clone_ids.get(&s).map_or(t, |&id| RefTarget::Actor(id))
                    }
                    t => t,
                };
                Some((name.to_string(), t))
            })
            .collect()
    }

    /// Takes other actors' reference writes to script `si`'s variables.
    pub fn import_refs(
        &self,
        si: usize,
        st: &mut ActorScriptState,
        refs: &HashMap<String, RefTarget>,
    ) {
        let found: Vec<_> = self
            .declared(si, st)
            .filter_map(|(var, name)| Some((var, refs.get(name)?.clone())))
            .collect();
        for (var, t) in found {
            st.refs.insert(vec![Ref::Obj(var)], t);
        }
    }

    /// Takes other actors' writes to script `si`'s variables (`export_vars` names).
    pub fn import_vars(&self, si: usize, st: &mut ActorScriptState, vals: &HashMap<String, Value>) {
        let vars: Vec<_> = self
            .declared(si, st)
            .filter_map(|(var, name)| Some((var, *vals.get(name)?)))
            .collect();
        for (var, v) in vars {
            st.vars.insert(vec![Ref::Obj(var)], v);
        }
    }

    /// `tfbActorInfo`-like objects scripts run `OpFindVariable` on: whose variables
    /// the engine keeps on the board.
    pub fn find_targets(&self) -> HashSet<usize> {
        self.set
            .scripts
            .iter()
            .flat_map(|s| &s.ops)
            .filter_map(|i| match &i.op {
                Op::FindVariable { target, .. } => match target[..] {
                    [Ref::Obj(o)] => Some(o),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    /// Name of the `OpUserBehavior` at op offset `offset` in script `si`.
    pub fn behavior_name(&self, si: usize, offset: usize) -> Option<&str> {
        self.set
            .scripts
            .get(si)?
            .ops
            .iter()
            .find_map(|i| match &i.op {
                Op::Behavior { name } if i.offset == offset => Some(name.as_str()),
                _ => None,
            })
    }

    /// Script index by `.ai` path (`tfbScriptInfo._name`).
    pub fn find(&self, name: &str) -> Option<usize> {
        self.set.scripts.iter().position(|s| s.name == name)
    }

    /// `(startup, body)` op ranges of script `si`; `None` without an `OpPreScript`.
    fn sections(&self, si: usize) -> Option<((usize, usize), (usize, usize))> {
        let ops = &self.set.scripts.get(si)?.ops;
        let pre = ops.iter().position(|i| i.op == Op::PreScript)?;
        let body = (pre + 1, (pre + 1 + ops[pre].br as usize).min(ops.len()));
        let startup = ops[body.0..body.1]
            .iter()
            .position(|i| i.op == Op::StartUp)
            .map(|k| {
                let at = body.0 + k;
                (at + 1, (at + 1 + ops[at].br as usize).min(body.1))
            })
            .unwrap_or((0, 0));
        Some((startup, body))
    }

    /// Whether script `si` moves, turns, scales or removes its own actor.
    pub fn animates_self(&self, si: usize) -> bool {
        let my = |p: &[Ref]| self.set.engine_key(p);
        self.sections(si).is_some()
            && self.set.scripts[si].ops.iter().any(|i| match &i.op {
                Op::SetValue { lhs, .. }
                | Op::IncValue { lhs }
                | Op::DecValue { lhs }
                | Op::SlideValue { lhs, .. } => my(lhs).is_some_and(|k| is_transform_key(&k)),
                Op::Remove { target } | Op::Displace { target, .. } => {
                    my(target).as_deref() == Some(&[field::MY])
                }
                _ => false,
            })
    }

    /// Names of every script that `animates_self`.
    pub fn animated_scripts(&self) -> HashSet<String> {
        (0..self.set.scripts.len())
            .filter(|&si| self.animates_self(si))
            .map(|si| self.set.scripts[si].name.clone())
            .collect()
    }

    /// Advance one tick of `dt` seconds: sliders, then `OpStartUp` (first tick only),
    /// then the body. Relative orientation rates are added once per tick.
    pub fn tick(&self, si: usize, st: &mut ActorScriptState, dt: f32) {
        if st.removed || st.error.is_some() {
            return;
        }
        let Some((startup, body)) = self.sections(si) else {
            return;
        };
        let mut vm = Vm::with_index(&self.set, self.global.as_ref(), Cow::Borrowed(&self.index));
        vm.vars = std::mem::take(&mut st.vars);
        vm.engine = std::mem::take(&mut st.engine);
        vm.globals = st.globals.clone();
        vm.set_counts = st.set_counts.clone();
        vm.engine_sets = st.engine_sets.clone();
        vm.self_id = st.self_id;
        vm.fields = st.fields.clone();
        vm.trace = st.trace.take();
        vm.board = std::mem::take(&mut st.board);
        vm.ref_board = std::mem::take(&mut st.ref_board);
        vm.engine
            .insert(vec![field::SCRIPT_TIMER], Value::Float(st.time));
        vm.rng = st.rng;
        vm.refs = std::mem::take(&mut st.refs);
        vm.sets = std::mem::take(&mut st.sets);
        vm.global_index = self.global_index.as_ref();
        vm.global_vars = std::mem::take(&mut st.global_vars);
        vm.global_refs = std::mem::take(&mut st.global_refs);
        vm.global_sets = std::mem::take(&mut st.global_sets);
        vm.clone_ids = std::mem::take(&mut st.clone_ids);
        vm.self_key = st.self_key;
        vm.save = st.save.clone();
        vm.spawned = st.spawned;
        self.advance_sliders(&mut vm, &mut st.sliders, dt);

        let ops = &self.set.scripts[si].ops;
        let mut result = Ok(());
        if !st.started {
            st.started = true;
            // `OpStartUp` (`0x8006f58c`): the first behaviour is the op after `OpPreScript`.
            st.behavior = ops
                .get(body.1)
                .filter(|i| matches!(i.op, Op::Behavior { .. }))
                .map(|i| i.offset);
            result = vm.run_range(si, startup.0, startup.1);
        }
        vm.behavior = st.behavior;
        if result.is_ok() {
            result = vm.run_range(si, body.0, body.1);
        }
        // End of `OpPreScript` (`0x8006f4d4`): jump into the current behaviour block, which
        // stops the tick when it ends (`0x8006f0c8`).
        let current = vm
            .behavior
            .and_then(|b| ops.iter().position(|i| i.offset == b));
        if let (Ok(()), Some(at)) = (&result, current) {
            let end = (at + 1 + ops[at].br as usize).min(ops.len());
            result = vm.run_range(si, at + 1, end);
        }
        st.behavior = vm.behavior;
        st.globals = vm.globals;
        st.refs = std::mem::take(&mut vm.refs);
        st.sets = std::mem::take(&mut vm.sets);
        st.global_vars = std::mem::take(&mut vm.global_vars);
        st.global_sets = std::mem::take(&mut vm.global_sets);
        st.trace = vm.trace.take();
        st.global_refs = std::mem::take(&mut vm.global_refs);
        st.clone_ids = std::mem::take(&mut vm.clone_ids);
        st.save_writes.append(&mut vm.save_writes);
        st.spawned = vm.spawned;
        if let Err(e) = result {
            st.error = Some(e);
        }

        st.engine = std::mem::take(&mut vm.engine);
        for effect in std::mem::take(&mut vm.effects) {
            match effect {
                spawn @ Effect::Spawn { .. } => st.spawns.push(spawn),
                Effect::ForClone { serial, effect } => st.clone_effects.push((serial, *effect)),
                Effect::ForActor { id, effect } => st.actor_effects.push((id, *effect)),
                effect => st.apply(effect),
            }
        }

        use field::*;
        for (rate, angle) in [
            (RELATIVE_FACING, FACING),
            (RELATIVE_TILT, TILT),
            (RELATIVE_ROLL, ROLL),
        ] {
            let r = st.get(&[MY, ORIENTATION, rate]).unwrap_or(0.0);
            if r != 0.0 {
                let a = st.get(&[MY, ORIENTATION, angle]).unwrap_or(0.0);
                st.set(&[MY, ORIENTATION, angle], a + r);
            }
        }
        st.vars = std::mem::take(&mut vm.vars);
        st.rng = vm.rng;
        st.board = std::mem::take(&mut vm.board);
        st.ref_board = std::mem::take(&mut vm.ref_board);
        st.time += dt;
    }

    /// Slider property (`looping`, `playback mode`) set by the slide's setup block.
    fn slider_prop(&self, vars: &HashMap<Vec<Ref>, Value>, op: usize, prop: u32) -> i32 {
        vars.iter()
            .find_map(|(k, v)| match k[..] {
                [Ref::Obj(o), Ref::Handle(h)]
                    if o == op && self.set.handle_ids.get(h as usize) == Some(&prop) =>
                {
                    Some(v.as_f32() as i32)
                }
                _ => None,
            })
            .unwrap_or(0)
    }

    /// Step every slider by `dt` and write its value (`0x8008ecb8`). `playback mode` 1
    /// ping-pongs: out and back over `2 * seconds`, ending at `from`. `looping` wraps
    /// the time instead of ending. The progress follows `ease_curve`.
    fn advance_sliders(&self, vm: &mut Vm, sliders: &mut Vec<Slider>, dt: f32) {
        sliders.retain_mut(|s| {
            s.t += dt;
            let looping = self.slider_prop(&vm.vars, s.op, field::LOOPING) != 0;
            let ping_pong = self.slider_prop(&vm.vars, s.op, field::PLAYBACK_MODE) == 1;
            let span = if ping_pong {
                2.0 * s.seconds
            } else {
                s.seconds
            };
            let done = s.seconds <= 0.0 || (!looping && s.t >= span);
            if looping && !done {
                s.t = s.t.rem_euclid(span);
            }
            let value = if done {
                if ping_pong && s.seconds > 0.0 {
                    s.from
                } else {
                    s.to
                }
            } else {
                // The far end is written as is (the curve is 0 / 0 there without ease in).
                let u = s.t / s.seconds;
                if u == 1.0 {
                    s.to
                } else {
                    let u = if u > 1.0 { 2.0 - u } else { u };
                    s.from + (s.to - s.from) * ease_curve(u, s.ease_out, s.ease_in)
                }
            };
            match self.set.engine_key(&s.lhs) {
                Some(k) => vm.engine.insert(k, Value::Float(value)),
                None => vm.vars.insert(s.lhs.clone(), Value::Float(value)),
            };
            !done || looping
        });
    }
}

/// Engine name -> `EXID` handle index in `set` (first match outside `global.bld`).
pub fn engine_handle(set: &ScriptSet, name: &str) -> Option<u32> {
    let (id, global) = (name_hash(name), name_hash("global.bld"));
    (0..set.handle_ids.len())
        .find(|&i| set.handle_ids[i] == id && set.handle_ns[i] != global)
        .map(|i| i as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{Instr, Operand, Rhs, Script, VarInfo};
    use field::*;

    const LOCAL: usize = 0x100;
    const SLIDE: usize = 0x200;

    fn ins(offset: usize, br: u16, op: Op) -> Instr {
        Instr {
            offset,
            flags: 0,
            br,
            op,
        }
    }
    fn lit(v: f32) -> Rhs {
        Rhs {
            a: Operand::Lit(Value::Float(v)),
            arith: -1,
            b: None,
            func: 0,
        }
    }
    fn rd(path: Vec<Ref>) -> Rhs {
        Rhs {
            a: Operand::Read {
                path,
                kind: crate::script::NumKind::Float,
            },
            arith: -1,
            b: None,
            func: 0,
        }
    }
    /// Handles: one per engine name, in this order.
    const NAMES: [u32; 19] = [
        MY,
        ORIENTATION,
        RELATIVE_FACING,
        FACING,
        VERTICAL_OFFSET,
        SCRIPT_TIMER,
        LOOPING,
        PLAYBACK_MODE,
        TILT,
        TINT,
        ALPHA,
        SCALE,
        UNIFORM,
        COUNT,
        DISTANCE,
        VELOCITY,
        LENGTH,
        WAYPOINTS,
        ROLL,
    ];
    const WAYPOINTS: u32 = 0x7a0a_bb2b;
    const COUNT: u32 = 0x39b1_ddf4;
    const DISTANCE: u32 = 0x2eb3_1462;
    fn h(id: u32) -> Ref {
        Ref::Handle(NAMES.iter().position(|&n| n == id).unwrap() as u32)
    }
    /// `PRESCRIPT { STARTUP { startup } SHUTDOWN {} body }`.
    fn program(startup: Vec<Instr>, body: Vec<Instr>) -> ScriptProgram {
        let mut ops = vec![
            ins(1, (startup.len() + body.len() + 2) as u16, Op::PreScript),
            ins(2, startup.len() as u16, Op::StartUp),
        ];
        ops.extend(startup);
        ops.push(ins(3, 0, Op::ShutDown));
        ops.extend(body);
        let mut vars = HashMap::new();
        vars.insert(
            LOCAL,
            VarInfo {
                name: "Height".into(),
                type_name: "OpCreateVariable".into(),
                init: None,
            },
        );
        let set = ScriptSet {
            scripts: vec![Script {
                name: "Test.ai".into(),
                ops,
            }],
            vars,
            handle_ids: NAMES.to_vec(),
            handle_ns: vec![name_hash("level.bld"); NAMES.len()],
            ..Default::default()
        };
        ScriptProgram::new(set, None)
    }

    const SPAWN: usize = 0x300;

    #[test]
    fn placement_tags_are_found_like_script_variables() {
        // A placement's tags (`ScriptSet::tags`) are variables of its actor beside the
        // script's: other actors find them (`export_vars` / `export_refs`) and write them
        // (`import_refs`); unset reference tags give their default.
        const ARMORED: usize = 0x500;
        const PATH: usize = 0x510;
        let mut p = program(Vec::new(), Vec::new());
        for (v, name, init) in [
            (ARMORED, "armored", Some(Value::Int(1))),
            (PATH, "flight path", None),
        ] {
            p.set.vars.insert(
                v,
                VarInfo {
                    name: name.into(),
                    type_name: "OpParameter".into(),
                    init,
                },
            );
        }
        p.set.tag_refs.insert(PATH, RefTarget::Null);
        let mut st = ActorScriptState::default();
        assert!(!p.export_vars(0, &st).contains_key("armored"));
        st.tags = vec![ARMORED, PATH];
        assert_eq!(p.export_vars(0, &st)["armored"], Value::Int(1));
        assert_eq!(p.export_refs(0, &st)["flight path"], RefTarget::Null);
        let path = HashMap::from([("flight path".to_string(), RefTarget::Placement(0x600))]);
        p.import_refs(0, &mut st, &path);
        assert_eq!(
            p.export_refs(0, &st)["flight path"],
            RefTarget::Placement(0x600)
        );
    }

    fn coin() -> crate::script::SpawnTemplate {
        crate::script::SpawnTemplate {
            offset: 0x400,
            ty: "tfbActorInfo".into(),
            name: "Coin_Art_01".into(),
            id: 0,
            script: Some("Coin_Art.ai".into()),
            scale: [100.0; 3],
            tint: [0x80, 0x80, 0x80, 0xff],
            weight: 0.0,
        }
    }

    #[test]
    fn spawn_setup_block_seeds_the_clone_not_the_spawner() {
        // `2D_Block_01.ai`: spawn at `my`, then `@spawn.facing = my.facing`, lift it 3 up,
        // and keep a reference to it.
        let clone = |rest: &[Ref]| [Ref::Obj(SPAWN)].iter().chain(rest).copied().collect();
        let p = program(
            vec![
                ins(
                    SPAWN,
                    3,
                    Op::Spawn {
                        template: vec![Ref::Obj(0x400)],
                        actor: Some(coin()),
                        at: vec![h(MY)],
                        facing: Some(lit(0.0)),
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetValue {
                        lhs: clone(&[h(ORIENTATION), h(FACING)]),
                        rhs: rd(vec![h(MY), h(ORIENTATION), h(FACING)]),
                    },
                ),
                ins(
                    12,
                    0,
                    Op::Displace {
                        target: clone(&[]),
                        mode: 0,
                        len: Some(lit(3.0)),
                        heading: Some(lit(0.0)),
                        pitch: Some(lit(90.0)),
                    },
                ),
                ins(
                    13,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(LOCAL)],
                        rhs: clone(&[]),
                        desc: String::new(),
                    },
                ),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        st.set(&[MY, ORIENTATION, FACING], 90.0);
        p.tick(0, &mut st, 1.0 / 60.0);
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none());
        assert_eq!(st.spawned, 1);
        assert_eq!(
            st.spawns,
            vec![Effect::Spawn {
                op: SPAWN,
                template: coin(),
                at: crate::script::SpawnAt::Engine(vec![MY]),
                facing: Some(0.0),
                serial: 0,
                seed: HashMap::from([(vec![MY, ORIENTATION, FACING], Value::Float(90.0))]),
                setup: vec![Effect::Displace {
                    target: vec![MY],
                    len: 3.0,
                    heading: 0.0,
                    pitch: 90.0,
                }],
                named_vars: HashMap::new(),
                named_refs: HashMap::new(),
            }]
        );
        // The spawner keeps its own facing and does not move.
        assert_eq!(st.get(&[MY, ORIENTATION, FACING]), Some(90.0));
        assert_eq!(st.displacement, [0.0; 3]);
        assert_eq!(
            st.refs.get(&vec![Ref::Obj(LOCAL)]),
            Some(&RefTarget::Spawned(0))
        );
    }

    #[test]
    fn clone_tint_through_spawn_op_and_reference() {
        // `UI_TheatricalReports_Hat.ai` / `Bauble_Pickup.ai`: the setup block hides the
        // clone and slides it in; later the spawner fades it through "My Lid".
        let alpha = [h(TINT), h(ALPHA)];
        let clone = |rest: &[Ref]| [Ref::Obj(SPAWN)].iter().chain(rest).copied().collect();
        let lid = |rest: &[Ref]| [Ref::Obj(LOCAL)].iter().chain(rest).copied().collect();
        let slide = |lhs, to| Op::SlideValue {
            lhs,
            target: lit(to),
            seconds: Some(lit(0.5)),
            ease_out: None,
            ease_in: None,
        };
        let p = program(
            vec![
                ins(
                    SPAWN,
                    3,
                    Op::Spawn {
                        template: vec![Ref::Obj(0x400)],
                        actor: Some(coin()),
                        at: vec![h(MY)],
                        facing: None,
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetValue {
                        lhs: clone(&alpha),
                        rhs: lit(0.0),
                    },
                ),
                ins(12, 0, slide(clone(&alpha), 255.0)),
                ins(
                    13,
                    0,
                    Op::SetRef {
                        lhs: lid(&[]),
                        rhs: clone(&[]),
                        desc: String::new(),
                    },
                ),
                ins(
                    14,
                    0,
                    Op::SetValue {
                        lhs: lid(&alpha),
                        rhs: lit(128.0),
                    },
                ),
                ins(15, 0, slide(lid(&alpha), 0.0)),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        let key = vec![MY, TINT, ALPHA];
        let my_alpha = vec![h(MY), h(TINT), h(ALPHA)];
        let Effect::Spawn { seed, setup, .. } = &st.spawns[0] else {
            panic!()
        };
        // In the block: the write seeds the clone, the slide runs on the clone.
        assert_eq!(seed.get(&key), Some(&Value::Float(0.0)));
        assert!(
            matches!(&setup[..], [Effect::Slide { lhs, to, .. }] if *lhs == my_alpha && *to == 255.0)
        );
        // After it, through the reference: delivered to clone 0, not kept as a spawner slide.
        assert_eq!(st.clone_effects.len(), 2);
        assert_eq!(
            st.clone_effects[0],
            (
                0,
                Effect::Set {
                    key,
                    value: Value::Float(128.0)
                }
            )
        );
        assert!(
            matches!(&st.clone_effects[1], (0, Effect::Slide { lhs, from, to, .. })
            if *lhs == my_alpha && *from == 128.0 && *to == 0.0)
        );
        assert!(st.sliders.is_empty());

        // Applied to the clone's own state: the write lands, the slider runs there.
        let mut clone_st = ActorScriptState::default();
        for (_, e) in std::mem::take(&mut st.clone_effects) {
            clone_st.apply(e);
        }
        assert_eq!(clone_st.get(&[MY, TINT, ALPHA]), Some(128.0));
        assert_eq!(clone_st.sliders.len(), 1);
    }

    #[test]
    fn reference_to_clone_in_its_setup_block_seeds_it() {
        // `TransitCrystal.ai`: `spawn "TS_Balloon" { "my Prop" = @spawn;
        // "my Prop".scale.uniform = 10; "my Prop".tint.alpha = 200 }`.
        let prop = |rest: &[Ref]| [Ref::Obj(LOCAL)].iter().chain(rest).copied().collect();
        let p = program(
            vec![
                ins(
                    SPAWN,
                    3,
                    Op::Spawn {
                        template: vec![Ref::Obj(0x400)],
                        actor: Some(coin()),
                        at: vec![h(MY)],
                        facing: None,
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetRef {
                        lhs: prop(&[]),
                        rhs: vec![Ref::Obj(SPAWN)],
                        desc: String::new(),
                    },
                ),
                ins(
                    12,
                    0,
                    Op::SetValue {
                        lhs: prop(&[h(SCALE), h(UNIFORM)]),
                        rhs: lit(10.0),
                    },
                ),
                ins(
                    13,
                    0,
                    Op::SetValue {
                        lhs: prop(&[h(TINT), h(ALPHA)]),
                        rhs: lit(200.0),
                    },
                ),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        let Effect::Spawn { seed, .. } = &st.spawns[0] else {
            panic!()
        };
        assert_eq!(
            seed.get(&vec![MY, SCALE, UNIFORM]),
            Some(&Value::Float(10.0))
        );
        assert_eq!(seed.get(&vec![MY, TINT, ALPHA]), Some(&Value::Float(200.0)));
        assert!(st.clone_effects.is_empty());
    }

    #[test]
    fn find_variable_binds_another_actors_variable() {
        // `_ArenaMaster2.ai` "Begin Arena": `find variable "state" on _ArenaMaster2 { inc @find }`.
        const FIND: usize = 0x500;
        const MASTER: usize = 0x900;
        let p = program(
            vec![
                ins(
                    FIND,
                    1,
                    Op::FindVariable {
                        name: "state".into(),
                        target: vec![Ref::Obj(MASTER)],
                    },
                ),
                ins(
                    11,
                    0,
                    Op::IncValue {
                        lhs: vec![Ref::Obj(FIND)],
                    },
                ),
                // After the block: the local "Height" is untouched by the lookup.
                ins(
                    12,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(LOCAL)],
                        rhs: rd(vec![Ref::Obj(FIND)]),
                    },
                ),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        st.board
            .insert(MASTER, HashMap::from([("state".into(), Value::Int(2))]));
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(st.board[&MASTER]["state"], Value::Int(3));
        assert_eq!(st.vars.get(&vec![Ref::Obj(LOCAL)]), Some(&Value::Int(3)));

        // Not on the board (actor without that variable): the block is skipped.
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.board.is_empty());
    }

    #[test]
    fn script_sets_track_clones_and_iterate_them() {
        // `"active dropships" include "active dropship"`, `.count`, `for each { fade it }`,
        // `check "active dropships" includes "active dropship"`, then exclude it.
        const SET: usize = 0x700;
        const N1: usize = 0x710;
        const N2: usize = 0x720;
        const IN: usize = 0x730;
        const EACH: usize = 0x600;
        const CHECK: usize = 0x610;
        let set = || vec![Ref::Obj(SET)];
        let count = |n| Op::SetValue {
            lhs: vec![Ref::Obj(n)],
            rhs: rd(vec![Ref::Obj(SET), h(COUNT)]),
        };
        let change = |op| Op::ChangeMembership {
            set: set(),
            op,
            group: vec![Ref::Obj(LOCAL)],
        };
        let p = program(
            vec![
                ins(
                    SPAWN,
                    1,
                    Op::Spawn {
                        template: vec![Ref::Obj(0x400)],
                        actor: Some(coin()),
                        at: vec![h(MY)],
                        facing: None,
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(LOCAL)],
                        rhs: vec![Ref::Obj(SPAWN)],
                        desc: String::new(),
                    },
                ),
                ins(12, 0, change(0)),
                ins(13, 0, change(0)),
                ins(14, 0, count(N1)),
                ins(
                    EACH,
                    1,
                    Op::ForEach {
                        set: set(),
                        order: 0,
                    },
                ),
                ins(
                    15,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(EACH), h(TINT), h(ALPHA)],
                        rhs: lit(7.0),
                    },
                ),
                ins(
                    CHECK,
                    1,
                    Op::CheckRef {
                        rel: 0,
                        desc: "OpCheckMembership".into(),
                        lhs: set(),
                        rhs: vec![Ref::Obj(LOCAL)],
                    },
                ),
                ins(
                    16,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(IN)],
                        rhs: lit(1.0),
                    },
                ),
                ins(17, 0, change(1)),
                ins(18, 0, count(N2)),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        let var = |n| st.vars.get(&vec![Ref::Obj(n)]).copied();
        // Included once (include does not duplicate), then excluded.
        assert_eq!(var(N1), Some(Value::Int(1)));
        assert_eq!(var(N2), Some(Value::Int(0)));
        assert_eq!(var(IN), Some(Value::Float(1.0)));
        // The loop body reached the clone through the member.
        assert_eq!(
            st.clone_effects,
            vec![(
                0,
                Effect::Set {
                    key: vec![MY, TINT, ALPHA],
                    value: Value::Float(7.0)
                }
            )]
        );
    }

    #[test]
    fn spawn_at_a_placement_uses_that_placement() {
        // `BattleMaster.ai` "Start BattleMaster": `spawn "_BattleMaster" at _BattleMaster`.
        let spawn = |at| Op::Spawn {
            template: vec![Ref::Obj(0x400)],
            actor: Some(coin()),
            at: vec![Ref::Obj(at)],
            facing: Some(lit(0.0)),
        };
        let mut p = program(
            vec![ins(SPAWN, 0, spawn(0x880)), ins(11, 0, spawn(0x881))],
            vec![],
        );
        p.set.placements.insert(0x880);
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        // 0x881 is not a placement (a reference variable, a macro param): skipped.
        assert_eq!(st.spawns.len(), 1);
        assert!(matches!(
            &st.spawns[0],
            Effect::Spawn {
                at: crate::script::SpawnAt::Placement(0x880),
                ..
            }
        ));
    }

    #[test]
    fn placement_reference_reaches_another_actor() {
        // `BattleMaster.ai`: `set Current_BattleMaster.x = 1`, `find variable "New Enemies"
        // on Current_BattleMaster`, and "Start BattleMaster" setting it to its clone.
        const PR: usize = 0x14cce8;
        const FIND: usize = 0x520;
        const OTHER: u64 = 0x1_0000_0042;
        let p = program(
            vec![
                ins(
                    11,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(PR), h(TINT), h(ALPHA)],
                        rhs: lit(9.0),
                    },
                ),
                ins(
                    FIND,
                    1,
                    Op::FindVariable {
                        name: "New Enemies".into(),
                        target: vec![Ref::Obj(PR)],
                    },
                ),
                ins(
                    12,
                    0,
                    Op::IncValue {
                        lhs: vec![Ref::Obj(FIND)],
                    },
                ),
                // The reference is set inside the setup block, where `@spawn` is the clone.
                ins(
                    SPAWN,
                    1,
                    Op::Spawn {
                        template: vec![Ref::Obj(0x400)],
                        actor: Some(coin()),
                        at: vec![Ref::Obj(PR)],
                        facing: None,
                    },
                ),
                ins(
                    13,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(PR)],
                        rhs: vec![Ref::Obj(SPAWN)],
                        desc: String::new(),
                    },
                ),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        st.set_reference(PR, RefTarget::Actor(OTHER));
        st.board.insert(
            OTHER as usize,
            HashMap::from([("New Enemies".into(), Value::Int(0))]),
        );
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(
            st.actor_effects,
            vec![(
                OTHER,
                Effect::Set {
                    key: vec![MY, TINT, ALPHA],
                    value: Value::Float(9.0)
                }
            )]
        );
        assert_eq!(st.board[&(OTHER as usize)]["New Enemies"], Value::Int(1));
        // `spawn ... at Current_BattleMaster` starts at that actor.
        assert!(matches!(
            &st.spawns[0],
            Effect::Spawn {
                at: crate::script::SpawnAt::Actor(OTHER),
                ..
            }
        ));
        // `Current_BattleMaster = @spawn`: the engine sees this actor's clone 0.
        assert_eq!(st.reference(PR), Some(&RefTarget::Spawned(0)));
    }

    #[test]
    fn spawn_of_a_template_held_in_a_reference() {
        // `ArenaScript_300_000.ai` startup: `"natureHeavy" = Enemy_S4_NatureHeavy_`; later
        // "BattleMaster - Spawn Dropship" runs `spawn "Left 1"` with it as the argument.
        const ENEMY: usize = 0x880;
        let p = {
            let mut p = program(
                vec![
                    ins(
                        11,
                        0,
                        Op::SetRef {
                            lhs: vec![Ref::Obj(LOCAL)],
                            rhs: vec![Ref::Obj(ENEMY)],
                            desc: String::new(),
                        },
                    ),
                    ins(
                        SPAWN,
                        0,
                        Op::Spawn {
                            template: vec![Ref::Obj(LOCAL)],
                            actor: None,
                            at: vec![h(MY)],
                            facing: None,
                        },
                    ),
                ],
                vec![],
            );
            p.set.placements.insert(ENEMY);
            p.set.templates.insert(
                ENEMY,
                crate::script::SpawnTemplate {
                    offset: ENEMY,
                    name: "Enemy_S4_NatureHeavy_".into(),
                    ..coin()
                },
            );
            p
        };
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(st.reference(LOCAL), Some(&RefTarget::Placement(ENEMY)));
        assert!(matches!(
            &st.spawns[..],
            [Effect::Spawn { template, .. }] if template.name == "Enemy_S4_NatureHeavy_"
        ));
    }

    /// Engine distances by member id, from any `me`.
    #[derive(Debug)]
    struct Distances(HashMap<u64, f32>);

    impl crate::script::MemberFields for Distances {
        fn field(&self, _me: u64, member: u64, field: u32) -> Option<f32> {
            (field == DISTANCE)
                .then(|| self.0.get(&member).copied())
                .flatten()
        }
    }

    #[test]
    fn find_subset_keeps_members_the_engine_measures_in_range() {
        // `Transport_Macros.ai`: `find subset of "Minions" where .distance < 40 { ... }`,
        // here over a script set of two clones, 25 and 90 away.
        const SET: usize = 0x700;
        const SUB: usize = 0x640;
        const N: usize = 0x710;
        let spawn = |off| {
            ins(
                off,
                1,
                Op::Spawn {
                    template: vec![Ref::Obj(0x400)],
                    actor: Some(coin()),
                    at: vec![h(MY)],
                    facing: None,
                },
            )
        };
        let keep = |off| {
            ins(
                off,
                0,
                Op::ChangeMembership {
                    set: vec![Ref::Obj(SET)],
                    op: 0,
                    group: vec![Ref::Obj(off - 1)],
                },
            )
        };
        let subset = |within: f32| {
            vec![
                ins(
                    SUB,
                    1,
                    Op::FindSubSet {
                        set: vec![Ref::Obj(SET)],
                        field: vec![h(DISTANCE)],
                        rel: 3,
                        rhs: Some(lit(within)),
                    },
                ),
                ins(
                    20,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(N)],
                        rhs: rd(vec![Ref::Obj(SUB), h(COUNT)]),
                    },
                ),
            ]
        };
        let run = |within| {
            let mut startup = vec![spawn(0x300), keep(0x301), spawn(0x310), keep(0x311)];
            startup.extend(subset(within));
            let p = program(startup, vec![]);
            let mut st = ActorScriptState {
                self_id: Some(1),
                clone_ids: HashMap::from([(0, 10), (1, 11)]),
                fields: Some(std::sync::Arc::new(Distances(HashMap::from([
                    (10, 25.0),
                    (11, 90.0),
                ])))),
                ..Default::default()
            };
            p.tick(0, &mut st, 1.0 / 60.0);
            assert!(st.error.is_none(), "{:?}", st.error);
            (
                st.vars.get(&vec![Ref::Obj(N)]).copied(),
                st.reference(SUB).cloned(),
            )
        };
        // Only the near clone; the op also refers to it.
        assert_eq!(
            run(40.0),
            (Some(Value::Int(1)), Some(RefTarget::Spawned(0)))
        );
        // Nothing in range: the block is skipped.
        assert_eq!(run(10.0), (None, None));
    }

    #[test]
    fn path_through_a_reference_to_my_reaches_my_engine_state() {
        // "Movement - Set Velocity": `"mover to set velocity of".velocity.length = 70`,
        // called with the mover being `my`.
        let mover = |rest: &[Ref]| [Ref::Obj(LOCAL)].iter().chain(rest).copied().collect();
        let p = program(
            vec![
                ins(
                    11,
                    0,
                    Op::SetRef {
                        lhs: mover(&[]),
                        rhs: vec![h(MY)],
                        desc: String::new(),
                    },
                ),
                ins(
                    12,
                    0,
                    Op::SetValue {
                        lhs: mover(&[h(VELOCITY), h(LENGTH)]),
                        rhs: lit(70.0),
                    },
                ),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(st.get(&[MY, VELOCITY, LENGTH]), Some(70.0));
    }

    /// Distance to a waypoint = its local x (placement at the origin).
    #[derive(Debug)]
    struct WaypointX;

    impl crate::script::MemberFields for WaypointX {
        fn field(&self, _: u64, _: u64, _: u32) -> Option<f32> {
            None
        }
        fn waypoint_field(&self, _: u64, _: usize, local: [f32; 3], f: u32) -> Option<f32> {
            (f == DISTANCE).then_some(local[0])
        }
    }

    #[test]
    fn indexed_reference_takes_a_waypoint_of_the_path() {
        // `Transport_Macros.ai`: `"Nav - next wp" = "Nav - Flight Path".waypoints["wp index"]`,
        // then `.waypoints.count`, `"Nav - next wp".orientation.roll`, `.distance`.
        const PATH: usize = 0x880;
        let (fp, next, setref) = (0x790, 0x7a0, 0x7b0);
        let (count_v, roll_v, dist_v) = (0x7c0, 0x7d0, 0x7e0);
        let set = |n: usize, path: Vec<Ref>| Op::SetValue {
            lhs: vec![Ref::Obj(n)],
            rhs: rd(path),
        };
        let mut p = program(
            vec![
                ins(
                    11,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(fp)],
                        rhs: vec![Ref::Obj(PATH)],
                        desc: String::new(),
                    },
                ),
                ins(
                    setref,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(next)],
                        rhs: vec![Ref::Obj(fp), h(WAYPOINTS)],
                        desc: String::new(),
                    },
                ),
                ins(
                    12,
                    0,
                    set(count_v, vec![Ref::Obj(fp), h(WAYPOINTS), h(COUNT)]),
                ),
                ins(
                    13,
                    0,
                    set(roll_v, vec![Ref::Obj(next), h(ORIENTATION), h(ROLL)]),
                ),
                ins(14, 0, set(dist_v, vec![Ref::Obj(next), h(DISTANCE)])),
            ],
            vec![],
        );
        p.set.placements.insert(PATH);
        let wp = |x, roll| crate::script::Waypoint {
            local: [x, 0.0, 0.0],
            angles: [0.0, 0.0, roll],
        };
        p.set
            .waypoints
            .insert(PATH, vec![wp(1.0, 0.0), wp(7.0, 2.0), wp(3.0, 0.0)]);
        p.set.ref_index.insert(setref, lit(1.0));
        let mut st = ActorScriptState {
            self_id: Some(1),
            fields: Some(std::sync::Arc::new(WaypointX)),
            ..Default::default()
        };
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(
            st.reference(next),
            Some(&RefTarget::Waypoint {
                owner: PATH,
                index: 1
            })
        );
        let var = |n| st.vars.get(&vec![Ref::Obj(n)]).copied();
        assert_eq!(var(count_v), Some(Value::Int(3)));
        assert_eq!(var(roll_v), Some(Value::Float(2.0)));
        assert_eq!(var(dist_v), Some(Value::Float(7.0)));
    }

    #[test]
    fn indexed_reference_takes_a_set_member() {
        // `Combat.ai` "Get Creature Info": `"entry" = "active"[i]` over a set.
        let (set_v, entry, setref) = (0x700, 0x7a0, 0x7b0);
        let (a, b) = (0x880, 0x890);
        let include = |off, o| {
            ins(
                off,
                0,
                Op::ChangeMembership {
                    set: vec![Ref::Obj(set_v)],
                    op: 0,
                    group: vec![Ref::Obj(o)],
                },
            )
        };
        let mut p = program(
            vec![
                include(11, a),
                include(12, b),
                ins(
                    setref,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(entry)],
                        rhs: vec![Ref::Obj(set_v)],
                        desc: String::new(),
                    },
                ),
            ],
            vec![],
        );
        p.set.placements.extend([a, b]);
        p.set.ref_index.insert(setref, lit(1.0));
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(st.reference(entry), Some(&RefTarget::Placement(b)));
    }

    #[test]
    fn find_variable_on_the_clone_being_set_up_travels_with_the_spawn() {
        // "BattleMaster - Spawn Dropship": `spawn "Transport Template" { find variable
        // "flight path" on @spawn { @find = "Transport Path" } }`.
        const FIND: usize = 0x520;
        const PATH: usize = 0x880;
        let p = {
            let mut p = program(
                vec![
                    ins(
                        SPAWN,
                        2,
                        Op::Spawn {
                            template: vec![Ref::Obj(0x400)],
                            actor: Some(coin()),
                            at: vec![h(MY)],
                            facing: None,
                        },
                    ),
                    ins(
                        FIND,
                        1,
                        Op::FindVariable {
                            name: "flight path".into(),
                            target: vec![Ref::Obj(SPAWN)],
                        },
                    ),
                    ins(
                        11,
                        0,
                        Op::SetRef {
                            lhs: vec![Ref::Obj(FIND)],
                            rhs: vec![Ref::Obj(PATH)],
                            desc: String::new(),
                        },
                    ),
                ],
                vec![],
            );
            p.set.placements.insert(PATH);
            p
        };
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        let Effect::Spawn { named_refs, .. } = &st.spawns[0] else {
            panic!()
        };
        assert_eq!(
            named_refs.get("flight path"),
            Some(&RefTarget::Placement(PATH))
        );
        assert!(st.board.is_empty() && st.ref_board.is_empty());
    }

    #[test]
    fn reference_written_through_find_variable_reaches_the_other_actor() {
        // `find variable "enemy1L" on "transport" { @find = @spawn }` from another actor:
        // the reference lands on that actor's board slot, as an engine id.
        const FIND: usize = 0x520;
        const OTHER: usize = 0x900;
        let p = program(
            vec![
                ins(
                    SPAWN,
                    0,
                    Op::Spawn {
                        template: vec![Ref::Obj(0x400)],
                        actor: Some(coin()),
                        at: vec![h(MY)],
                        facing: None,
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(LOCAL)],
                        rhs: vec![h(MY)],
                        desc: String::new(),
                    },
                ),
                ins(
                    FIND,
                    1,
                    Op::FindVariable {
                        name: "enemy1L".into(),
                        target: vec![Ref::Obj(OTHER)],
                    },
                ),
                ins(
                    12,
                    0,
                    Op::SetRef {
                        lhs: vec![Ref::Obj(FIND)],
                        rhs: vec![Ref::Obj(LOCAL)],
                        desc: String::new(),
                    },
                ),
            ],
            vec![],
        );
        let mut st = ActorScriptState {
            self_id: Some(42),
            ..Default::default()
        };
        st.board
            .insert(OTHER, HashMap::from([("enemy1L".into(), Value::Int(0))]));
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(st.ref_board[&OTHER]["enemy1L"], RefTarget::Actor(42));
    }

    #[test]
    fn spawn_of_unresolved_template_skips_its_block() {
        // Template behind a variable reference: `0x8007c6bc` skips the setup block.
        let p = program(
            vec![
                ins(
                    SPAWN,
                    1,
                    Op::Spawn {
                        template: vec![Ref::Obj(LOCAL)],
                        actor: None,
                        at: vec![h(MY)],
                        facing: None,
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetValue {
                        lhs: vec![h(MY), h(VERTICAL_OFFSET)],
                        rhs: lit(5.0),
                    },
                ),
            ],
            vec![],
        );
        let mut st = ActorScriptState::default();
        p.tick(0, &mut st, 1.0 / 60.0);
        assert!(st.spawns.is_empty());
        assert_eq!(st.get(&[MY, VERTICAL_OFFSET]), None);
    }

    #[test]
    fn windmill_spins_by_relative_facing_each_tick() {
        // `LL_327_Windmill.ai`: body sets `my.orientation.relative facing = -2`.
        let p = program(
            vec![],
            vec![ins(
                10,
                0,
                Op::SetValue {
                    lhs: vec![h(MY), h(ORIENTATION), h(RELATIVE_FACING)],
                    rhs: lit(-2.0),
                },
            )],
        );
        assert!(p.animates_self(0));
        let mut st = ActorScriptState::default();
        st.set(&[MY, ORIENTATION, FACING], 90.0);
        for _ in 0..3 {
            p.tick(0, &mut st, 1.0 / 60.0);
        }
        assert_eq!(st.get(&[MY, ORIENTATION, FACING]), Some(84.0));
        assert!(st.error.is_none());
    }

    #[test]
    fn looping_ping_pong_slider_drives_vertical_offset() {
        // `Chandelier.ai`: startup slides "Height" 0 -> 6 (playback mode 1, looping),
        // body copies it to `my.vertical offset`.
        let local = vec![Ref::Obj(LOCAL)];
        let p = program(
            vec![
                ins(
                    SLIDE,
                    2,
                    Op::SlideValue {
                        lhs: local.clone(),
                        target: lit(6.0),
                        seconds: Some(lit(2.0)),
                        ease_out: None,
                        ease_in: None,
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(SLIDE), h(LOOPING)],
                        rhs: lit(1.0),
                    },
                ),
                ins(
                    12,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(SLIDE), h(PLAYBACK_MODE)],
                        rhs: lit(1.0),
                    },
                ),
            ],
            vec![ins(
                13,
                0,
                Op::SetValue {
                    lhs: vec![h(MY), h(VERTICAL_OFFSET)],
                    rhs: rd(local),
                },
            )],
        );
        assert!(p.animates_self(0));
        let mut st = ActorScriptState::default();
        let mut offsets = Vec::new();
        for _ in 0..7 {
            p.tick(0, &mut st, 0.5);
            offsets.push(st.get(&[MY, VERTICAL_OFFSET]).unwrap());
        }
        assert_eq!(offsets, [0.0, 1.5, 3.0, 4.5, 6.0, 4.5, 3.0]);
    }

    #[test]
    fn ease_curve_matches_slider_profile() {
        assert_eq!(ease_curve(0.3, 0.0, 0.0), 0.3);
        // Ease 50 / 50 (0.02 * 50 = 1 each) is scaled to 0.5 / 0.5: s-curve.
        assert_eq!(ease_curve(0.25, 1.0, 1.0), 0.125);
        assert_eq!(ease_curve(0.5, 1.0, 1.0), 0.5);
        assert_eq!(ease_curve(0.75, 1.0, 1.0), 0.875);
        // Continuous at both joins, and 0 -> 1.
        let (a, b) = (0.2, 0.4);
        for x in [a, 1.0 - b] {
            let d = ease_curve(x - 1e-4, a, b) - ease_curve(x + 1e-4, a, b);
            assert!(d.abs() < 1e-3, "{x}: {d}");
        }
        assert_eq!(ease_curve(0.0, a, b), 0.0);
        assert!((ease_curve(1.0, a, b) - 1.0).abs() < 1e-6);
    }

    /// Slide `local` 0 -> 8 over 2 s with `mode` and eases, copied to `vertical offset`;
    /// offsets after each 0.5 s tick.
    fn slide_offsets(mode: f32, ease_out: f32, ease_in: f32, ticks: usize) -> Vec<f32> {
        let local = vec![Ref::Obj(LOCAL)];
        let p = program(
            vec![
                ins(
                    SLIDE,
                    1,
                    Op::SlideValue {
                        lhs: local.clone(),
                        target: lit(8.0),
                        seconds: Some(lit(2.0)),
                        ease_out: Some(lit(ease_out)),
                        ease_in: Some(lit(ease_in)),
                    },
                ),
                ins(
                    11,
                    0,
                    Op::SetValue {
                        lhs: vec![Ref::Obj(SLIDE), h(PLAYBACK_MODE)],
                        rhs: lit(mode),
                    },
                ),
            ],
            vec![ins(
                13,
                0,
                Op::SetValue {
                    lhs: vec![h(MY), h(VERTICAL_OFFSET)],
                    rhs: rd(local),
                },
            )],
        );
        let mut st = ActorScriptState::default();
        (0..ticks)
            .map(|_| {
                p.tick(0, &mut st, 0.5);
                st.get(&[MY, VERTICAL_OFFSET]).unwrap()
            })
            .collect()
    }

    #[test]
    fn ping_pong_slide_goes_out_and_back_once() {
        assert_eq!(
            slide_offsets(1.0, 0.0, 0.0, 7),
            [0.0, 2.0, 4.0, 6.0, 8.0, 6.0, 4.0]
        );
        // Not looping: it ends back at the start after 2 * seconds.
        assert_eq!(slide_offsets(1.0, 0.0, 0.0, 10)[8..], [0.0, 0.0]);
        assert_eq!(slide_offsets(0.0, 0.0, 0.0, 6)[4..], [8.0, 8.0]);
    }

    #[test]
    fn eased_ping_pong_mirrors_the_way_out() {
        // Ease out 25 (start, 0.5 of the slide), no ease in.
        let o = slide_offsets(1.0, 25.0, 0.0, 8);
        // u = 0.25: v = 1 / 1.5, 8 * v * 0.0625 / 0.5.
        assert!((o[1] - 8.0 / 1.5 * 0.125).abs() < 1e-5, "{o:?}");
        assert_eq!(o[4], 8.0);
        // The way back replays the same curve backwards (not a swapped slide).
        assert!(
            (o[5] - o[3]).abs() < 1e-5 && (o[7] - o[1]).abs() < 1e-5,
            "{o:?}"
        );
    }

    #[test]
    fn remove_self_after_timer() {
        let p = program(
            vec![],
            vec![
                ins(
                    20,
                    1,
                    Op::CheckValue {
                        lhs: vec![h(SCRIPT_TIMER)],
                        rel: 4,
                        rhs: lit(1.0),
                    },
                ),
                ins(
                    21,
                    0,
                    Op::Remove {
                        target: vec![h(MY)],
                    },
                ),
            ],
        );
        assert!(p.animates_self(0));
        let mut st = ActorScriptState::default();
        let mut ticks = 0;
        while !st.removed && ticks < 100 {
            p.tick(0, &mut st, 0.25);
            ticks += 1;
        }
        // Timer reads 0, 0.25, ..., 1.25 > 1 on the sixth tick.
        assert_eq!(ticks, 6);
    }

    #[test]
    fn startup_runs_once_and_static_scripts_do_not_animate() {
        let p = program(
            vec![ins(
                30,
                0,
                Op::IncValue {
                    lhs: vec![Ref::Obj(LOCAL)],
                },
            )],
            vec![],
        );
        assert!(!p.animates_self(0));
        let mut st = ActorScriptState::default();
        for _ in 0..5 {
            p.tick(0, &mut st, 0.1);
        }
        assert_eq!(st.vars.get(&vec![Ref::Obj(LOCAL)]), Some(&Value::Int(1)));
        assert_eq!(engine_handle(&p.set, "tilt"), Some(8));
    }

    #[test]
    fn behavior_switch_and_execution() {
        // PreScript (br=3):
        //   [1] StartUp (br=1):
        //     [2] SetBehavior(target=40)
        //   [3] PreScript body empty
        // [40] Behavior "StateA" (br=1):
        //   [41] SetBehavior(target=50)
        // [50] Behavior "StateB" (br=1):
        //   [51] IncValue(LOCAL)
        let ops = vec![
            ins(0, 3, Op::PreScript),
            ins(10, 1, Op::StartUp),
            ins(20, 0, Op::SetBehavior { target: Some(40) }),
            ins(30, 0, Op::Jump), // empty prescript body dummy
            ins(
                40,
                1,
                Op::Behavior {
                    name: "StateA".into(),
                },
            ),
            ins(41, 0, Op::SetBehavior { target: Some(50) }),
            ins(
                50,
                1,
                Op::Behavior {
                    name: "StateB".into(),
                },
            ),
            ins(
                51,
                0,
                Op::IncValue {
                    lhs: vec![Ref::Obj(LOCAL)],
                },
            ),
        ];
        let mut vars = HashMap::new();
        vars.insert(
            LOCAL,
            VarInfo {
                name: "Counter".into(),
                type_name: "OpCreateVariable".into(),
                init: None,
            },
        );
        let set = ScriptSet {
            scripts: vec![Script {
                name: "BehaviorTest.ai".into(),
                ops,
            }],
            vars,
            ..Default::default()
        };
        let p = ScriptProgram::new(set, None);
        let mut st = ActorScriptState::default();

        // Tick 1: StartUp sets behavior to 40 (StateA). Behavior 40 executes and sets behavior to 50 (StateB).
        p.tick(0, &mut st, 0.1);
        assert_eq!(st.behavior, Some(50));
        assert_eq!(st.vars.get(&vec![Ref::Obj(LOCAL)]), None);

        // Tick 2: Behavior 50 (StateB) executes, incrementing LOCAL.
        p.tick(0, &mut st, 0.1);
        assert_eq!(st.vars.get(&vec![Ref::Obj(LOCAL)]), Some(&Value::Int(1)));

        // Tick 3: Behavior 50 executes again.
        p.tick(0, &mut st, 0.1);
        assert_eq!(st.vars.get(&vec![Ref::Obj(LOCAL)]), Some(&Value::Int(2)));
    }

    fn read_level(name: &str) -> Option<Vec<u8>> {
        let p = ["assets/files/level", "../../assets/files/level"]
            .into_iter()
            .map(|d| std::path::Path::new(d).join(format!("{name}.bld")))
            .find(|p| p.exists())?;
        let mut m = crate::archive::ArchiveMount::new();
        m.mount(&p).ok()?;
        m.read_file("level.bld").ok()
    }

    #[test]
    fn rom_hub_bird_picks_idle_states_at_random() {
        let Some(b) = read_level("Level_Hub_2014_01") else {
            return;
        };
        let p = ScriptProgram::new(ScriptSet::parse(&b).unwrap(), None);
        let si = (0..p.set.scripts.len())
            .find(|&i| p.set.scripts[i].name.ends_with("Bird_Ambient_new.ai"))
            .unwrap();
        let run = |seed| {
            let mut st = ActorScriptState {
                rng: seed,
                ..Default::default()
            };
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..60 * 60 {
                p.tick(si, &mut st, 1.0 / 60.0);
                seen.extend(st.anim_state);
            }
            assert!(st.error.is_none(), "{:?}", st.error);
            (seen, st.get(&[MY, ORIENTATION, FACING]))
        };
        let (seen, facing) = run(7);
        // IdlePeck, IdleShake, IdleHop, IdleStill.
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), [7, 8, 9, 10]);
        assert!(facing.is_some());
        assert_eq!(run(7), run(7));
    }

    #[test]
    fn rom_hub_scripts_animate_props() {
        let Some(b) = read_level("Level_Hub_2014_01") else {
            return;
        };
        let p = ScriptProgram::new(ScriptSet::parse(&b).unwrap(), None);
        let animated = p.animated_scripts();
        let windmill = animated
            .iter()
            .find(|n| n.ends_with("LL_327_Windmill.ai"))
            .expect("windmill animates");
        let si = p.find(windmill).unwrap();
        let mut st = ActorScriptState::default();
        for _ in 0..60 {
            p.tick(si, &mut st, 1.0 / 60.0);
        }
        assert_eq!(st.get(&[MY, ORIENTATION, FACING]), Some(-120.0));
        let mut names: Vec<_> = animated.iter().collect();
        names.sort();
        for name in names {
            let si = p.find(name).unwrap();
            let mut st = ActorScriptState::default();
            for _ in 0..600 {
                p.tick(si, &mut st, 1.0 / 60.0);
            }
            eprintln!("{name}: removed={} error={:?}", st.removed, st.error);
        }
    }
}
