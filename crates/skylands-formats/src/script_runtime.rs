//! Per-actor script execution. A placed actor's `.ai` script is an `OpPreScript` block
//! holding an `OpStartUp` block (run once), an `OpShutDown` block (not run) and the
//! per-tick body (everything else). The actor itself is the engine object `my`; its
//! fields are name-hash paths (`ScriptSet::engine_key`). Notes: `agent/SCRIPT_VM.md`.

use crate::script::{
    name_hash, Effect, Op, Ref, RefTarget, ScriptSet, Value, Vm, VmError, VmIndex,
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
    /// The script hit an op the VM does not model; it no longer runs.
    pub error: Option<VmError>,
    /// Save slot read by "Get Save Param" (set by the engine each tick).
    pub save: Option<std::sync::Arc<dyn crate::script::SaveParams>>,
    /// "Set Save Param" writes since the engine last drained them.
    pub save_writes: Vec<(String, i32, i64)>,
    /// Effects on earlier clones by serial (`Effect::ForClone`), for the engine to
    /// deliver after the tick.
    pub clone_effects: Vec<(u32, Effect)>,
    /// `OpSpawn` clones made so far (next `Effect::Spawn::serial`).
    pub spawned: u32,
    /// `Effect::Spawn`s since the engine last drained them.
    pub spawns: Vec<Effect>,
}

impl ActorScriptState {
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
        Self { set, global, index }
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
        vm.engine
            .insert(vec![field::SCRIPT_TIMER], Value::Float(st.time));
        vm.rng = st.rng;
        vm.refs = std::mem::take(&mut st.refs);
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
    const NAMES: [u32; 11] = [
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
    ];
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

    fn coin() -> crate::script::SpawnTemplate {
        crate::script::SpawnTemplate {
            offset: 0x400,
            ty: "tfbActorInfo".into(),
            name: "Coin_Art_01".into(),
            id: 0,
            script: Some("Coin_Art.ai".into()),
            scale: [100.0; 3],
            tint: [0x80, 0x80, 0x80, 0xff],
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
                at: vec![MY],
                facing: Some(0.0),
                serial: 0,
                seed: HashMap::from([(vec![MY, ORIENTATION, FACING], Value::Float(90.0))]),
                setup: vec![Effect::Displace {
                    target: vec![MY],
                    len: 3.0,
                    heading: 0.0,
                    pitch: 90.0,
                }],
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
