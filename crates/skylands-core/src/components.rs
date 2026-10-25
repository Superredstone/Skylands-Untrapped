use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackKind {
    PrimaryMelee,
    SecondarySweep,
    SpecialProjectile,
    PrimaryBreath,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAttack {
    pub timer: f32,
    pub duration: f32,
    pub kind: AttackKind,
    pub has_hit: bool,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ActorIdentity {
    pub id: u32,
    pub character_id: u16,
    pub name: String,
    pub is_villain: bool,
    pub model_path: Option<String>,
    pub texture_path: Option<String>,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hero;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Villain;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ControlledVillain;

pub use skylands_formats::igb::EnemyClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyState {
    Idle,
    Chase,
    Windup,
    Recover,
}

/// Enemy HP and per-hit damage for one class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyClassStats {
    pub health: u32,
    pub damage: u32,
}

/// Per-level enemy stats from `LevelSystems.ai` "Game - Define Enemy Stats" (script VM)
/// at difficulty level 1, normal mode, single player. `Default` is the fallback for
/// levels without that macro.
// ponytail: one stat block per class; real levels pass a difficulty level and tuning
// multipliers per `"enemy type"` reference, which needs object references in the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyTuning {
    pub near: EnemyClassStats,
    pub far: EnemyClassStats,
}

impl Default for EnemyTuning {
    fn default() -> Self {
        Self {
            near: EnemyClassStats {
                health: 100,
                damage: 25,
            },
            far: EnemyClassStats {
                health: 100,
                damage: 20,
            },
        }
    }
}

impl EnemyTuning {
    /// Archetype IDs: 3 Near, 4 Far.
    pub fn from_scripts(
        level: &skylands_formats::ScriptSet,
        global: &skylands_formats::ScriptSet,
    ) -> Result<Self, skylands_formats::VmError> {
        let stats = |archetype| {
            let s = skylands_formats::enemy_stats(level, global, archetype, 1)?;
            Ok(EnemyClassStats {
                health: s.hp.round().max(1.0) as u32,
                damage: s.damage.round().max(1.0) as u32,
            })
        };
        Ok(Self {
            near: stats(3)?,
            far: stats(4)?,
        })
    }

    pub fn get(&self, class: EnemyClass) -> EnemyClassStats {
        match class {
            EnemyClass::Near => self.near,
            EnemyClass::Far => self.far,
        }
    }
}

/// Level enemy placed by an `Enemy_S4_*.ai` script.
/// Ranges are script literals in feet: `Enemy_S4_BirdHazard.ai` `aggroRange = 45`,
/// `Enemy_S4_UndeadLobber.ai` `attackRange = 60`.
// ponytail: windup/recover timings are fixed; real ones live in per-enemy `.ai` scripts.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Enemy {
    pub class: EnemyClass,
    pub state: EnemyState,
    pub timer: f32,
    pub aggro_range: f32,
    pub attack_range: f32,
    pub damage: u32,
}

impl Enemy {
    pub const WINDUP: f32 = 0.5;
    pub const RECOVER: f32 = 1.0;
    /// `Enemy_S4_WilikinLobberGooSpitter.ai` `myProjectileSpeed = 50` (ft/s).
    pub const PROJECTILE_SPEED: f32 = 50.0 * FEET_TO_UNITS;

    pub fn new(class: EnemyClass, damage: u32) -> Self {
        let attack_range = match class {
            EnemyClass::Near => 1.5,
            EnemyClass::Far => 60.0 * FEET_TO_UNITS,
        };
        Self {
            class,
            state: EnemyState::Idle,
            timer: 0.0,
            aggro_range: 45.0 * FEET_TO_UNITS,
            attack_range,
            damage,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct VillainEnergy {
    pub current: f32,
    pub max: f32,
    pub drain_rate: f32,
    pub recharge_rate: f32,
}

impl Default for VillainEnergy {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
            drain_rate: 3.333,
            recharge_rate: 5.0,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prop {
    pub destructible: bool,
}

/// Placed actor driven by its `.ai` script (`skylands_formats::ScriptProgram`).
/// Script orientation angles turn the actor relative to its placement; `vertical
/// offset` and `OpDisplace` move it, `scale.uniform` (percent) scales it.
#[derive(Component, Debug, Clone)]
pub struct ActorScript {
    /// Index into the level's `ScriptProgram`.
    pub script: usize,
    /// Level marker node; the renderer draws this actor's model apart from the level.
    pub marker_node: usize,
    pub state: skylands_formats::ActorScriptState,
    /// Placement transform.
    pub base: Transform,
}

/// Actor whose model is already part of the level render mesh; it needs no visual
/// of its own.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelDrawnModel;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collectible {
    pub value: u32,
    pub collected: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 500,
            max: 500,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CharacterCollider {
    pub radius: f32,
    pub height: f32,
    pub step_height: f32,
    pub grounded: bool,
}

impl Default for CharacterCollider {
    fn default() -> Self {
        Self {
            radius: 0.5,
            height: 1.8,
            step_height: 0.35,
            grounded: true,
        }
    }
}

impl CharacterCollider {
    pub fn from_model(model: &skylands_formats::IgbModel) -> Self {
        if let Some((min_pt, max_pt)) = model.primary_mesh_bounds() {
            let height = (max_pt[1] - min_pt[1]).max(0.5);
            let radius = (height * 0.28).clamp(0.45, 1.2);
            let step_height = (height * 0.2).clamp(0.35, 0.85);
            Self {
                radius,
                height,
                step_height,
                grounded: true,
            }
        } else {
            Self::default()
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deref, DerefMut)]
pub struct Velocity(pub Vec3);

#[derive(Component, Debug, Clone, Copy, PartialEq, Deref, DerefMut)]
pub struct MovementSpeed(pub f32);

impl Default for MovementSpeed {
    fn default() -> Self {
        Self(5.0)
    }
}

/// Per-character attack damage from the character script `BaseDamage_*` constants.
/// Fractional (Trap Team uses e.g. `12.5`); rounded when applied.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AttackDamage {
    pub primary: f32,
    pub secondary: f32,
    pub special: f32,
}

impl Default for AttackDamage {
    fn default() -> Self {
        Self {
            primary: 50.0,
            secondary: 100.0,
            special: 75.0,
        }
    }
}

const PROJECTILE_KEYWORDS: [&str; 9] = [
    "projectile",
    "missile",
    "arrow",
    "lob",
    "shot",
    "bullet",
    "disc",
    "frisbee",
    "beam",
];

impl AttackDamage {
    pub fn from_model(model: &skylands_formats::IgbModel) -> Option<Self> {
        Self::from_constants(&model.script_constants)
    }

    /// Older characters name slots (`BaseDamage_PrimaryAttack_*` / `_SecondaryAttack_*`);
    /// Trap Team ones don't, so fall back to file order (1st, 2nd, 3rd unused entry).
    /// Special = first projectile-like name, else next unused entry, else primary.
    // ponytail: base (level 1, no upgrades) damage only; apply `DamageMultiplier_*` /
    // `DmgMult_*` and global.bld level scaling once XP/upgrades exist.
    pub fn from_constants(constants: &[(String, skylands_formats::ScriptValue)]) -> Option<Self> {
        let mut entries: Vec<(&str, f32)> = Vec::new();
        for (name, value) in constants {
            if let Some(short) = name.strip_prefix("BaseDamage_") {
                if entries.iter().all(|(n, _)| *n != short) {
                    entries.push((short, value.as_f32().max(0.0)));
                }
            }
        }
        let named = |slot: &str| entries.iter().position(|(n, _)| n.starts_with(slot));
        let mut used = Vec::new();
        let pick = |preferred: Option<usize>, used: &mut Vec<usize>| {
            let i = preferred.or_else(|| (0..entries.len()).find(|i| !used.contains(i)))?;
            used.push(i);
            Some(entries[i].1)
        };
        let primary = pick(named("PrimaryAttack"), &mut used)?;
        let secondary = pick(named("SecondaryAttack"), &mut used).unwrap_or(primary);
        let projectile = entries.iter().position(|(n, _)| {
            let l = n.to_ascii_lowercase();
            PROJECTILE_KEYWORDS.iter().any(|k| l.contains(k))
        });
        let special = pick(projectile, &mut used).unwrap_or(primary);
        Some(Self {
            primary,
            secondary,
            special,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimaryAttackStyle {
    #[default]
    Melee,
    Projectile,
    Breath,
}

/// Per-character projectile flight, raw engine units (characters are native scale
/// in levels: `Placement._scaleVec` = 100%, Spyro 1.77 units tall).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ProjectileParams {
    pub speed: f32,
    pub lifetime: f32,
    /// Style of primary attack (Attack 1 / B button).
    pub primary_style: PrimaryAttackStyle,
}

impl Default for ProjectileParams {
    fn default() -> Self {
        Self {
            speed: 12.0,
            lifetime: 1.5,
            primary_style: PrimaryAttackStyle::Melee,
        }
    }
}

impl ProjectileParams {
    pub fn from_model(model: &skylands_formats::IgbModel) -> Self {
        let style = Self::deduce_style(model);
        Self {
            primary_style: style,
            ..Self::from_constants(&model.script_constants)
        }
    }

    /// Deduce whether the primary attack is melee, projectile, or a breath stream.
    /// Reverses the character's primary slot naming and animation set from the ROM.
    pub fn deduce_style(model: &skylands_formats::IgbModel) -> PrimaryAttackStyle {
        let primary_name = model.script_constants.iter().find_map(|(k, _)| {
            k.strip_prefix("BaseDamage_PrimaryAttack_")
                .or_else(|| k.strip_prefix("BaseDamage_"))
        });

        let has_breath_anim = model
            .animations
            .iter()
            .any(|a| a.name.contains("breath") || a.name.contains("breathe"));

        if let Some(name) = primary_name {
            let l = name.to_ascii_lowercase();
            // Explicit melee keywords always mean melee (even if the character has a secondary projectile actor)
            let is_melee = [
                "melee", "combo", "spin", "slash", "hammer", "fist", "tongue", "ballwhip", "whip",
                "slam", "jump",
            ]
            .iter()
            .any(|k| l.contains(k));
            if is_melee {
                return PrimaryAttackStyle::Melee;
            }

            // Breath/beam names or breath animations (e.g. Spyro `BaseDamage_PrimaryAttack_Fire` + `firebreathloop`)
            let is_breath = l.contains("breath")
                || (has_breath_anim
                    && (l.contains("fire") || l.contains("sun") || l.contains("potion")));
            if is_breath {
                return PrimaryAttackStyle::Breath;
            }

            // Explicit projectile keywords
            let is_proj = [
                "harpoon",
                "shot",
                "bullet",
                "arrow",
                "missile",
                "shell",
                "boomerang",
                "disc",
                "frisbee",
                "cannon",
                "starfish",
                "acorn",
                "sunprojectile",
                "fireball",
            ]
            .iter()
            .any(|k| l.contains(k));
            if is_proj || model.projectile.is_some() {
                return PrimaryAttackStyle::Projectile;
            }
        }

        if has_breath_anim {
            PrimaryAttackStyle::Breath
        } else if model.projectile.is_some() {
            PrimaryAttackStyle::Projectile
        } else {
            PrimaryAttackStyle::Melee
        }
    }

    /// First positive literal on a projectile/missile/harpoon variable whose name has
    /// speed|velocity (speed, ft/s like all script speeds) or lifetime|lifespan
    /// (lifetime, s). Zeros are runtime resets, skipped. Missing fields keep the default.
    // ponytail: ignores per-attack variants (e.g. Eruptor lava ball vs fire shot);
    // add attack-specific lookup when more than one ranged attack is wired.
    pub fn from_constants(constants: &[(String, skylands_formats::ScriptValue)]) -> Self {
        let find = |keys: &[&str]| {
            constants.iter().find_map(|(name, value)| {
                let l = name.to_ascii_lowercase();
                let v = value.as_f32();
                let hit = ["projectile", "missile", "harpoon"]
                    .iter()
                    .any(|k| l.contains(k))
                    && !l.contains("difference")
                    && keys.iter().any(|k| l.contains(k));
                (hit && v > 0.0 && v.is_finite()).then_some(v)
            })
        };
        let d = Self::default();
        Self {
            speed: find(&["speed", "velocity"]).map_or(d.speed, |v| v * FEET_TO_UNITS),
            lifetime: find(&["lifetime", "lifespan"]).unwrap_or(d.lifetime),
            primary_style: PrimaryAttackStyle::Melee,
        }
    }
}

// DOL 0x805a89fc, units verified in DOL 0x80586ef4 ("ft / sec", "ft / sec / sec", "deg / sec")
pub const FEET_TO_UNITS: f32 = 0.3048;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ModelParameters {
    pub ground_max_speed: f32,
    pub ground_accel: f32,
    pub ground_decel: f32,
    pub ground_turn_rate: f32, // deg/s
    pub air_max_speed: f32,
    pub air_accel: f32,
    pub air_decel: f32,
    pub air_turn_rate: f32, // deg/s
    pub jump_max_speed: f32,
    pub jump_accel: f32,
    pub jump_initial_speed: f32,
}

impl Default for ModelParameters {
    fn default() -> Self {
        Self {
            ground_max_speed: 20.0 * FEET_TO_UNITS,
            ground_accel: 160.0 * FEET_TO_UNITS,
            ground_decel: 80.0 * FEET_TO_UNITS,
            ground_turn_rate: 720.0,
            air_max_speed: 20.0 * FEET_TO_UNITS,
            air_accel: 160.0 * FEET_TO_UNITS,
            air_decel: 80.0 * FEET_TO_UNITS,
            air_turn_rate: 1440.0,
            jump_max_speed: 300.0 * FEET_TO_UNITS,
            jump_accel: 4.8 * FEET_TO_UNITS,
            jump_initial_speed: 32.0 * FEET_TO_UNITS,
        }
    }
}

impl ModelParameters {
    pub const fn raw_dol() -> Self {
        Self {
            ground_max_speed: 20.0,
            ground_accel: 160.0,
            ground_decel: 80.0,
            ground_turn_rate: 720.0,
            air_max_speed: 20.0,
            air_accel: 160.0,
            air_decel: 80.0,
            air_turn_rate: 1440.0,
            jump_max_speed: 300.0,
            jump_accel: 4.8,
            jump_initial_speed: 32.0,
        }
    }

    pub fn from_model(model: &skylands_formats::IgbModel) -> Self {
        Self::from_constants(&model.script_constants)
    }

    pub fn from_constants(constants: &[(String, skylands_formats::ScriptValue)]) -> Self {
        let mut params = Self::default();
        for (name, val) in constants {
            let l = name.to_ascii_lowercase();
            let v = val.as_f32();
            if v > 0.0 && v.is_finite() {
                if l.contains("base speed") || l == "speed" {
                    params.ground_max_speed = v * FEET_TO_UNITS;
                    params.air_max_speed = v * FEET_TO_UNITS;
                } else if l.contains("jump impulse vertical") || l.contains("jump initial speed") {
                    params.jump_initial_speed = v * FEET_TO_UNITS;
                }
            }
        }
        params
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skylands_formats::ScriptValue;

    #[test]
    fn test_character_collider_from_synthetic_model() {
        use skylands_formats::{IgbMesh, IgbModel, IgbVertex, Skeleton};

        let mut model = IgbModel::default();
        let skel = Skeleton {
            bones: vec![Default::default(); 10],
            root: 0,
            name_to_index: Default::default(),
        };
        model.skeletons.push(skel);

        // Body mesh attached to primary rig (height 3.0)
        let body_mesh = IgbMesh {
            name: "Body".into(),
            vertices: vec![
                IgbVertex {
                    position: [-0.5, 0.0, -0.5],
                    ..Default::default()
                },
                IgbVertex {
                    position: [0.5, 3.0, 0.5],
                    ..Default::default()
                },
            ],
            indices: vec![],
            material_name: None,
            skeleton: Some(0),
            image_index: None,
            base_color: None,
            texture_wrap: Default::default(),
            uv_scroll: None,
        };

        // Accessory mesh on secondary rig (height 10.0, should be ignored)
        let accessory_mesh = IgbMesh {
            name: "WingTrail".into(),
            vertices: vec![
                IgbVertex {
                    position: [0.0, 0.0, 0.0],
                    ..Default::default()
                },
                IgbVertex {
                    position: [0.0, 10.0, 0.0],
                    ..Default::default()
                },
            ],
            indices: vec![],
            material_name: None,
            skeleton: Some(1),
            image_index: None,
            base_color: None,
            texture_wrap: Default::default(),
            uv_scroll: None,
        };

        model.meshes.push(body_mesh);
        model.meshes.push(accessory_mesh);

        let collider = CharacterCollider::from_model(&model);
        assert!(
            (collider.height - 3.0).abs() < 1e-4,
            "height: {}",
            collider.height
        );
        assert!(
            (collider.radius - 0.84).abs() < 1e-4,
            "radius: {}",
            collider.radius
        );
        assert!(
            (collider.step_height - 0.6).abs() < 1e-4,
            "step_height: {}",
            collider.step_height
        );
    }

    #[test]
    fn test_projectile_params_from_constants() {
        let spyro = consts(&[
            ("projectile range/lifetime", 0.35),
            ("missile speed", 0.0),
            ("missile speed", 40.0),
            ("Action 2 displace speed", 5.0),
        ]);
        let p = ProjectileParams::from_constants(&spyro);
        assert_eq!((p.speed, p.lifetime), (40.0 * FEET_TO_UNITS, 0.35));

        // Gill Grunt `HarpoonVelocity` (ft/s).
        let gill = consts(&[
            ("water hose max velocity", 60.0),
            ("HarpoonVelocity", 150.0),
        ]);
        let p = ProjectileParams::from_constants(&gill);
        assert!(
            (p.speed - 150.0 * FEET_TO_UNITS).abs() < 1e-4,
            "{}",
            p.speed
        );

        let king = consts(&[("differenceBetweenProjectileLifetimeAndTotalLifeTime", 8.5)]);
        assert_eq!(
            ProjectileParams::from_constants(&king),
            ProjectileParams::default()
        );
    }

    #[test]
    fn test_primary_attack_style_deduction() {
        use skylands_formats::IgbModel;

        // Spyro: BaseDamage_PrimaryAttack_Fire + firebreathloop animation -> Breath
        let spyro_model = IgbModel {
            script_constants: consts(&[("BaseDamage_PrimaryAttack_Fire", 15.0)]),
            animations: vec![skylands_formats::AnimationClip::new(
                "firebreathloop",
                0.8,
                vec![],
            )],
            ..Default::default()
        };
        assert_eq!(
            ProjectileParams::deduce_style(&spyro_model),
            PrimaryAttackStyle::Breath
        );

        // Gill Grunt: Harpoon -> Projectile
        let gill_model = IgbModel {
            script_constants: consts(&[("BaseDamage_PrimaryAttack_Harpoon", 20.0)]),
            ..Default::default()
        };
        assert_eq!(
            ProjectileParams::deduce_style(&gill_model),
            PrimaryAttackStyle::Projectile
        );

        // Flame Knight (Ignitor): primary is ComboBase even though he has a projectile actor! -> Melee
        let ignitor_model = IgbModel {
            script_constants: consts(&[("BaseDamage_PrimaryAttack_ComboBase", 15.0)]),
            projectile: Some(skylands_formats::CharacterProjectile {
                actor: "010_Fireball".into(),
                model: "fireball.mdl".into(),
                meshes: vec![0],
            }),
            ..Default::default()
        };
        assert_eq!(
            ProjectileParams::deduce_style(&ignitor_model),
            PrimaryAttackStyle::Melee
        );

        // Stealth Elf: ComboBase, no projectile actor -> Melee
        let elf_model = IgbModel {
            script_constants: consts(&[("BaseDamage_PrimaryAttack_ComboBase", 9.0)]),
            ..Default::default()
        };
        assert_eq!(
            ProjectileParams::deduce_style(&elf_model),
            PrimaryAttackStyle::Melee
        );
    }

    fn consts(items: &[(&str, f32)]) -> Vec<(String, ScriptValue)> {
        items
            .iter()
            .map(|(n, v)| (n.to_string(), ScriptValue::Float(*v)))
            .collect()
    }

    #[test]
    fn test_attack_damage_slot_resolution() {
        // Named slots (Gill Grunt): no projectile keyword, 3rd unused entry.
        let gill = consts(&[
            ("BaseDamage_PrimaryAttack_Harpoon", 20.0),
            ("BaseDamage_SecondaryAttack_Hose", 7.0),
            ("BaseDamage_Anchor", 35.0),
        ]);
        let d = AttackDamage::from_constants(&gill).unwrap();
        assert_eq!((d.primary, d.secondary, d.special), (20.0, 7.0, 35.0));

        // Trap Team order fallback (Snap Shot); projectile keyword may reuse primary.
        let snap = consts(&[
            ("BaseDamage_Arrow_main", 12.5),
            ("BaseDamage_Melee_slam", 30.0),
            ("BaseDamage_JumpSlam", 25.0),
        ]);
        let d = AttackDamage::from_constants(&snap).unwrap();
        assert_eq!((d.primary, d.secondary, d.special), (12.5, 30.0, 12.5));

        // Keyword beats order; duplicates keep first value.
        let gusto = consts(&[
            ("BaseDamage_Melee", 10.0),
            ("BaseDamage_Melee", 99.0),
            ("BaseDamage_Dash", 5.0),
            ("BaseDamage_Cannon_Projectile", 8.0),
        ]);
        let d = AttackDamage::from_constants(&gusto).unwrap();
        assert_eq!((d.primary, d.secondary, d.special), (10.0, 5.0, 8.0));

        let one = consts(&[("BaseDamage_Only", 4.0)]);
        let d = AttackDamage::from_constants(&one).unwrap();
        assert_eq!((d.primary, d.secondary, d.special), (4.0, 4.0, 4.0));

        assert!(AttackDamage::from_constants(&consts(&[("speed", 1.0)])).is_none());
    }

    #[test]
    fn test_model_parameters_defaults_and_constants() {
        let raw = ModelParameters::raw_dol();
        assert_eq!(raw.ground_max_speed, 20.0);
        assert_eq!(raw.ground_accel, 160.0);
        assert_eq!(raw.ground_decel, 80.0);
        assert_eq!(raw.ground_turn_rate, 720.0);
        assert_eq!(raw.air_turn_rate, 1440.0);
        assert_eq!(raw.jump_initial_speed, 32.0);

        let def = ModelParameters::default();
        assert!((def.ground_max_speed - 20.0 * FEET_TO_UNITS).abs() < 1e-4);
        assert!((def.jump_initial_speed - 32.0 * FEET_TO_UNITS).abs() < 1e-4);

        let constants = consts(&[
            ("Return Value - Base Speed", 15.0),
            ("jump impulse vertical", 50.0),
        ]);
        let custom = ModelParameters::from_constants(&constants);
        assert!((custom.ground_max_speed - 15.0 * FEET_TO_UNITS).abs() < 1e-4);
        assert!((custom.jump_initial_speed - 50.0 * FEET_TO_UNITS).abs() < 1e-4);
    }
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ActorAnimation {
    pub current_clip: Option<String>,
    pub time: f32,
    pub speed: f32,
    /// `tfbAnimationState` id when state-driven (`ActorScript`).
    pub state: Option<i32>,
}

impl Default for ActorAnimation {
    fn default() -> Self {
        Self {
            current_clip: Some("idle".to_string()),
            time: 0.0,
            speed: 1.0,
            state: None,
        }
    }
}

/// Portal entrance: a Skylander just put on the Traptanium plays its `spawn` clip and
/// ignores input until `remaining` runs out.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PortalSpawn {
    pub remaining: f32,
}

impl PortalSpawn {
    /// Hero `spawn02` clips run 0.67-0.77 s (Spyro, Gill Grunt).
    pub const DURATION: f32 = 0.75;
}

impl Default for PortalSpawn {
    fn default() -> Self {
        Self {
            remaining: Self::DURATION,
        }
    }
}

#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct AttackCombo {
    pub step: u8,
    pub cooldown: f32,
    pub active: Option<ActiveAttack>,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct Projectile {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub damage: u32,
    pub radius: f32,
    /// Fired by an enemy: hits heroes only.
    pub hostile: bool,
    /// Actor that fired it; its character's projectile model is drawn.
    pub owner: Option<Entity>,
    /// Pierces targets instead of despawning on first hit (e.g. breath streams).
    pub piercing: bool,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct DamagePopup {
    pub text: String,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub color: [f32; 4],
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelEntity;

/// Talkable level character (`MarkerKind::Npc`). The level draws its model.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Npc {
    pub name: String,
    /// Hero distance at which the talk prompt shows.
    pub talk_radius: f32,
    /// From the level script and language pack (`skylands_formats::npc_dialogue`).
    /// Empty = nothing to say, no talk prompt.
    pub conversations: Vec<skylands_formats::Conversation>,
    /// Conversation the next talk plays; talks cycle through them in order.
    pub next_conversation: usize,
    /// Level marker node index.
    pub marker_node: Option<usize>,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ZoneTrigger {
    pub id: u32,
    pub radius: f32,
    pub target_zone: String,
    pub target_level: Option<String>,
    pub target_spawn: Vec3,
    pub prompt: String,
}
