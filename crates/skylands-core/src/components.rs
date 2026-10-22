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
