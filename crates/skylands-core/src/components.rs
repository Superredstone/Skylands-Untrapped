use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackKind {
    PrimaryMelee,
    SecondarySweep,
    SpecialProjectile,
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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prop {
    pub destructible: bool,
}

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

/// Per-character projectile flight, raw engine units (characters are native scale
/// in levels: `Placement._scaleVec` = 100%, Spyro 1.77 units tall).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ProjectileParams {
    pub speed: f32,
    pub lifetime: f32,
}

impl Default for ProjectileParams {
    fn default() -> Self {
        Self {
            speed: 12.0,
            lifetime: 1.5,
        }
    }
}

impl ProjectileParams {
    pub fn from_model(model: &skylands_formats::IgbModel) -> Self {
        Self::from_constants(&model.script_constants)
    }

    /// First positive literal on a projectile/missile variable whose name has
    /// speed|velocity (speed) or lifetime|lifespan (lifetime). Zeros are runtime
    /// resets, skipped. Missing fields keep the default.
    // ponytail: ignores per-attack variants (e.g. Eruptor lava ball vs fire shot);
    // add attack-specific lookup when more than one ranged attack is wired.
    pub fn from_constants(constants: &[(String, skylands_formats::ScriptValue)]) -> Self {
        let find = |keys: &[&str]| {
            constants.iter().find_map(|(name, value)| {
                let l = name.to_ascii_lowercase();
                let v = value.as_f32();
                let hit = (l.contains("projectile") || l.contains("missile"))
                    && !l.contains("difference")
                    && keys.iter().any(|k| l.contains(k));
                (hit && v > 0.0 && v.is_finite()).then_some(v)
            })
        };
        let d = Self::default();
        Self {
            speed: find(&["speed", "velocity"]).unwrap_or(d.speed),
            lifetime: find(&["lifetime", "lifespan"]).unwrap_or(d.lifetime),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skylands_formats::ScriptValue;

    #[test]
    fn test_projectile_params_from_constants() {
        let spyro = consts(&[
            ("projectile range/lifetime", 0.35),
            ("missile speed", 0.0),
            ("missile speed", 40.0),
            ("Action 2 displace speed", 5.0),
        ]);
        let p = ProjectileParams::from_constants(&spyro);
        assert_eq!((p.speed, p.lifetime), (40.0, 0.35));

        let king = consts(&[("differenceBetweenProjectileLifetimeAndTotalLifeTime", 8.5)]);
        assert_eq!(
            ProjectileParams::from_constants(&king),
            ProjectileParams::default()
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
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ActorAnimation {
    pub current_clip: Option<String>,
    pub time: f32,
    pub speed: f32,
}

impl Default for ActorAnimation {
    fn default() -> Self {
        Self {
            current_clip: Some("idle".to_string()),
            time: 0.0,
            speed: 1.0,
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

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ZoneTrigger {
    pub id: u32,
    pub radius: f32,
    pub target_zone: String,
    pub target_level: Option<String>,
    pub target_spawn: Vec3,
    pub prompt: String,
}
