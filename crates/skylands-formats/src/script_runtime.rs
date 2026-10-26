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
    t: f32,
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
}

impl ActorScriptState {
    pub fn get(&self, key: &[u32]) -> Option<f32> {
        self.engine.get(key).map(|v| v.as_f32())
    }
    pub fn set(&mut self, key: &[u32], v: f32) {
        self.engine.insert(key.to_vec(), Value::Float(v));
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
        if let Err(e) = result {
            st.error = Some(e);
        }

        for effect in std::mem::take(&mut vm.effects) {
            match effect {
                Effect::Slide {
                    op,
                    lhs,
                    from,
                    to,
                    seconds,
                } => {
                    st.sliders.retain(|s| s.op != op);
                    st.sliders.push(Slider {
                        op,
                        lhs,
                        from,
                        to,
                        seconds,
                        t: 0.0,
                    });
                }
                Effect::Displace {
                    target,
                    len,
                    heading,
                    pitch,
                } if target == [field::MY] => {
                    let (sh, ch) = heading.to_radians().sin_cos();
                    let (sp, cp) = pitch.to_radians().sin_cos();
                    for (d, v) in st.displacement.iter_mut().zip([cp * sh, sp, cp * ch]) {
                        *d += len * v;
                    }
                }
                Effect::Remove { target } if target == [field::MY] => st.removed = true,
                Effect::StartSequence { group, state } if group == [field::ANIMATION_PALETTE] => {
                    st.anim_state = Some(state)
                }
                // ponytail: the turn is applied at once; the engine turns over time (target
                // vtable +0x90, rate unknown). Add the rate once it is reversed.
                Effect::TurnTo { target, facing } if target.first() == Some(&field::MY) => {
                    vm.engine.insert(
                        vec![field::MY, field::ORIENTATION, field::FACING],
                        Value::Float(facing),
                    );
                }
                _ => {}
            }
        }

        use field::*;
        for (rate, angle) in [
            (RELATIVE_FACING, FACING),
            (RELATIVE_TILT, TILT),
            (RELATIVE_ROLL, ROLL),
        ] {
            let r = vm
                .engine
                .get(&[MY, ORIENTATION, rate][..])
                .map_or(0.0, |v| v.as_f32());
            if r != 0.0 {
                let k = vec![MY, ORIENTATION, angle];
                let a = vm.engine.get(&k).map_or(0.0, |v| v.as_f32());
                vm.engine.insert(k, Value::Float(a + r));
            }
        }
        st.vars = std::mem::take(&mut vm.vars);
        st.engine = std::mem::take(&mut vm.engine);
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

    /// Step every slider by `dt` and write its value. `looping` restarts it;
    /// `playback mode` 1 ping-pongs. Ease in/out is not modelled (linear).
    fn advance_sliders(&self, vm: &mut Vm, sliders: &mut Vec<Slider>, dt: f32) {
        sliders.retain_mut(|s| {
            s.t += dt;
            let done = s.seconds <= 0.0 || s.t >= s.seconds;
            let looping = self.slider_prop(&vm.vars, s.op, field::LOOPING) != 0;
            let value = if done && !looping {
                s.to
            } else {
                if done {
                    s.t -= s.seconds;
                    if self.slider_prop(&vm.vars, s.op, field::PLAYBACK_MODE) == 1 {
                        std::mem::swap(&mut s.from, &mut s.to);
                    }
                }
                s.from + (s.to - s.from) * (s.t / s.seconds)
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
    const NAMES: [u32; 9] = [
        MY,
        ORIENTATION,
        RELATIVE_FACING,
        FACING,
        VERTICAL_OFFSET,
        SCRIPT_TIMER,
        LOOPING,
        PLAYBACK_MODE,
        TILT,
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
