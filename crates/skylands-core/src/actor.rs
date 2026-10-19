use crate::collision::{CharacterCollider, Environment};
use crate::input::{WiiButton, WiiInputState};
use skylands_formats::{LevelMarker, MarkerKind};

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

#[derive(Debug, Clone, PartialEq)]
pub struct DamagePopup {
    pub text: String,
    pub world_pos: [f32; 3],
    pub timer: f32,
    pub max_lifetime: f32,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Projectile {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub damage: u32,
    pub radius: f32,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActorKind {
    Hero,
    Villain,
    Prop { destructible: bool },
    Collectible { value: u32, collected: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorldAudioCue {
    PickupCollected,
    HeroAttack(AttackKind),
    AttackHit,
    PropDestroyed,
    ZoneEntered(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZoneTrigger {
    pub id: u32,
    pub position: [f32; 3],
    pub radius: f32,
    pub target_zone: String,
    pub target_spawn: [f32; 3],
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub name: String,
    pub archive_path: String,
    pub bounding_box: ([f32; 3], [f32; 3]),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    pub id: u32,
    pub name: String,
    pub character_id: u16,
    pub variant: u16,
    pub kind: ActorKind,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub rotation: f32,
    pub is_villain: bool,
    pub active: bool,
    pub model_path: Option<String>,
    pub texture_path: Option<String>,
    pub collider: CharacterCollider,
    pub health: u32,
    pub max_health: u32,
    pub active_attack: Option<ActiveAttack>,
    pub combo_step: u8,
    pub combo_cooldown_timer: f32,
    pub animation_time: f32,
    pub current_anim: Option<String>,
    pub anim_speed: f32,
}

pub struct World {
    next_id: u32,
    pub actors: Vec<Actor>,
    pub active_hero_id: Option<u32>,
    pub environment: Environment,
    pub pending_audio_cues: Vec<WorldAudioCue>,
    pub coins: u32,
    pub damage_popups: Vec<DamagePopup>,
    pub projectiles: Vec<Projectile>,
    pub active_zone: String,
    pub zones: Vec<Zone>,
    pub triggers: Vec<ZoneTrigger>,
    pub active_prompt: Option<String>,
}

impl World {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            actors: Vec::new(),
            active_hero_id: None,
            environment: Environment::default(),
            pending_audio_cues: Vec::new(),
            coins: 0,
            damage_popups: Vec::new(),
            projectiles: Vec::new(),
            active_zone: "Courtyard".to_string(),
            zones: Vec::new(),
            triggers: Vec::new(),
            active_prompt: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        character_id: u16,
        variant: u16,
        name: impl Into<String>,
        position: [f32; 3],
        is_villain: bool,
        model_path: Option<String>,
        texture_path: Option<String>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let kind = if is_villain {
            ActorKind::Villain
        } else {
            ActorKind::Hero
        };

        let actor = Actor {
            id,
            name: name.into(),
            character_id,
            variant,
            kind,
            position,
            velocity: [0.0, 0.0, 0.0],
            rotation: 0.0,
            is_villain,
            active: true,
            model_path,
            texture_path,
            collider: CharacterCollider::default(),
            health: 500,
            max_health: 500,
            active_attack: None,
            combo_step: 0,
            combo_cooldown_timer: 0.0,
            animation_time: 0.0,
            current_anim: Some("idle".to_string()),
            anim_speed: 1.0,
        };

        if !is_villain && self.active_hero_id.is_none() {
            self.active_hero_id = Some(id);
        }

        self.actors.push(actor);
        id
    }

    pub fn spawn_prop(
        &mut self,
        name: impl Into<String>,
        position: [f32; 3],
        destructible: bool,
        model_path: Option<String>,
        texture_path: Option<String>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let actor = Actor {
            id,
            name: name.into(),
            character_id: 0,
            variant: 0,
            kind: ActorKind::Prop { destructible },
            position,
            velocity: [0.0, 0.0, 0.0],
            rotation: 0.0,
            is_villain: false,
            active: true,
            model_path,
            texture_path,
            collider: CharacterCollider {
                radius: 0.5,
                height: 1.0,
                step_height: 0.0,
                grounded: true,
            },
            health: 500,
            max_health: 500,
            active_attack: None,
            combo_step: 0,
            combo_cooldown_timer: 0.0,
            animation_time: 0.0,
            current_anim: Some("idle".to_string()),
            anim_speed: 1.0,
        };

        self.actors.push(actor);
        id
    }

    pub fn spawn_collectible(
        &mut self,
        name: impl Into<String>,
        position: [f32; 3],
        value: u32,
        model_path: Option<String>,
        texture_path: Option<String>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let actor = Actor {
            id,
            name: name.into(),
            character_id: 0,
            variant: 0,
            kind: ActorKind::Collectible {
                value,
                collected: false,
            },
            position,
            velocity: [0.0, 0.0, 0.0],
            rotation: 0.0,
            is_villain: false,
            active: true,
            model_path,
            texture_path,
            collider: CharacterCollider {
                radius: 0.75,
                height: 1.0,
                step_height: 0.0,
                grounded: true,
            },
            health: 500,
            max_health: 500,
            active_attack: None,
            combo_step: 0,
            combo_cooldown_timer: 0.0,
            animation_time: 0.0,
            current_anim: Some("idle".to_string()),
            anim_speed: 1.0,
        };

        self.actors.push(actor);
        id
    }

    pub fn drain_audio_cues(&mut self) -> Vec<WorldAudioCue> {
        std::mem::take(&mut self.pending_audio_cues)
    }

    pub fn unload_zone_entities(&mut self) {
        self.actors.retain(|a| Some(a.id) == self.active_hero_id);
        self.projectiles.clear();
        self.damage_popups.clear();
        self.triggers.clear();
        self.active_prompt = None;
    }

    pub fn load_level_entities(&mut self, markers: &[LevelMarker]) {
        for marker in markers {
            let lower = marker.name.to_ascii_lowercase();
            if lower.starts_with("sp_hero")
                || lower.starts_with("spawn_start")
                || lower.starts_with("sp_start")
            {
                self.environment.spawn_point = marker.position;
                if let Some(hero_id) = self.active_hero_id {
                    if let Some(hero) = self.actors.iter_mut().find(|a| a.id == hero_id) {
                        hero.position = marker.position;
                    }
                }
            } else {
                match marker.kind {
                    MarkerKind::Spawn => {}
                    MarkerKind::TriggerZone | MarkerKind::Doorway => {
                        let target_zone = if lower.contains("library") {
                            "Library".to_string()
                        } else if lower.contains("arena") {
                            "Arena".to_string()
                        } else if lower.contains("courtyard") {
                            "Courtyard".to_string()
                        } else {
                            "Next Zone".to_string()
                        };
                        let prompt = format!("Press A to enter {}", target_zone);
                        let trigger_id = (self.triggers.len() + 1) as u32;
                        self.triggers.push(ZoneTrigger {
                            id: trigger_id,
                            position: marker.position,
                            radius: 2.5,
                            target_zone,
                            target_spawn: [0.0, 0.0, 0.0],
                            prompt,
                        });
                    }
                    MarkerKind::Prop => {
                        let destructible = lower.contains("barrel")
                            || lower.contains("crate")
                            || lower.contains("destruct")
                            || lower.contains("pot");
                        self.spawn_prop(&marker.name, marker.position, destructible, None, None);
                    }
                    MarkerKind::Collectible => {
                        let value = if lower.contains("gem") {
                            5
                        } else if lower.contains("trap") {
                            10
                        } else {
                            1
                        };
                        self.spawn_collectible(&marker.name, marker.position, value, None, None);
                    }
                    MarkerKind::Other => {}
                }
            }
        }
    }

    pub fn despawn(&mut self, id: u32) -> bool {
        let initial_len = self.actors.len();
        self.actors.retain(|a| a.id != id);
        if self.active_hero_id == Some(id) {
            self.active_hero_id = self.actors.iter().find(|a| !a.is_villain).map(|a| a.id);
        }
        self.actors.len() < initial_len
    }

    pub fn despawn_by_character_id(&mut self, character_id: u16) -> bool {
        let ids: Vec<u32> = self
            .actors
            .iter()
            .filter(|a| a.character_id == character_id)
            .map(|a| a.id)
            .collect();

        let mut removed = false;
        for id in ids {
            if self.despawn(id) {
                removed = true;
            }
        }
        removed
    }

    pub fn find_by_character_id(&self, character_id: u16) -> Option<&Actor> {
        self.actors.iter().find(|a| a.character_id == character_id)
    }

    pub fn update(&mut self, dt: f32, input: &WiiInputState) {
        // 1. Hero input, movement, combo timers, and attack triggering
        let mut triggered_attack: Option<AttackKind> = None;
        let mut hero_pos = [0.0f32; 3];
        let mut hero_rot = 0.0f32;
        let mut has_hero = false;

        if let Some(hero_id) = self.active_hero_id {
            if let Some(hero) = self.actors.iter_mut().find(|a| a.id == hero_id) {
                has_hero = true;

                // Combo cooldown timer
                if hero.combo_cooldown_timer > 0.0 {
                    hero.combo_cooldown_timer -= dt;
                    if hero.combo_cooldown_timer <= 0.0 {
                        hero.combo_cooldown_timer = 0.0;
                        hero.combo_step = 0;
                    }
                }

                // Advance active attack timer
                if let Some(ref mut attack) = hero.active_attack {
                    attack.timer += dt;
                    if attack.timer >= attack.duration {
                        hero.active_attack = None;
                    }
                }

                // Attack input check when not currently mid-attack
                if hero.active_attack.is_none() {
                    if input.is_down(WiiButton::B) {
                        let next_step = (hero.combo_step % 3) + 1;
                        hero.combo_step = next_step;
                        hero.combo_cooldown_timer = 0.6;
                        hero.active_attack = Some(ActiveAttack {
                            timer: 0.0,
                            duration: 0.25,
                            kind: AttackKind::PrimaryMelee,
                            has_hit: false,
                        });
                        triggered_attack = Some(AttackKind::PrimaryMelee);
                    } else if input.is_down(WiiButton::Z) {
                        hero.combo_step = 0;
                        hero.combo_cooldown_timer = 0.0;
                        hero.active_attack = Some(ActiveAttack {
                            timer: 0.0,
                            duration: 0.45,
                            kind: AttackKind::SecondarySweep,
                            has_hit: false,
                        });
                        triggered_attack = Some(AttackKind::SecondarySweep);
                    } else if input.is_down(WiiButton::One) || input.is_down(WiiButton::Two) {
                        hero.combo_step = 0;
                        hero.combo_cooldown_timer = 0.0;
                        hero.active_attack = Some(ActiveAttack {
                            timer: 0.0,
                            duration: 0.35,
                            kind: AttackKind::SpecialProjectile,
                            has_hit: false,
                        });
                        triggered_attack = Some(AttackKind::SpecialProjectile);
                    }
                }

                const SPEED: f32 = 5.0;
                hero.velocity[0] = input.stick[0] * SPEED;
                hero.velocity[2] = -input.stick[1] * SPEED;

                if input.stick[0].abs() > 0.1 || input.stick[1].abs() > 0.1 {
                    hero.rotation = input.stick[0].atan2(-input.stick[1]);
                }

                // Jump trigger
                if hero.collider.grounded && input.is_down(WiiButton::A) {
                    hero.velocity[1] = self.environment.jump_impulse;
                    hero.collider.grounded = false;
                }

                // Apply gravity when airborne
                if !hero.collider.grounded {
                    hero.velocity[1] += self.environment.gravity * dt;
                }

                // Sub-stepping: step size bounded by half collider radius to prevent tunneling
                const MAX_STEP: f32 = 0.2;
                let h_speed = (hero.velocity[0] * hero.velocity[0]
                    + hero.velocity[2] * hero.velocity[2])
                    .sqrt();
                let v_speed = hero.velocity[1].abs();
                let total_move = (h_speed.max(v_speed) * dt).max(0.001);
                let num_steps = ((total_move / MAX_STEP).ceil() as usize).max(1);
                let sub_dt = dt / (num_steps as f32);

                for _ in 0..num_steps {
                    // Horizontal movement with obstacle pushout
                    hero.position[0] += hero.velocity[0] * sub_dt;
                    hero.position[2] += hero.velocity[2] * sub_dt;

                    let sphere_center = [
                        hero.position[0],
                        hero.position[1] + hero.collider.radius,
                        hero.position[2],
                    ];
                    if let Some(push) = self
                        .environment
                        .mesh
                        .resolve_sphere(sphere_center, hero.collider.radius)
                    {
                        hero.position[0] += push[0];
                        hero.position[2] += push[2];
                    }

                    // Vertical resolution and ground raycast
                    if hero.velocity[1] > 0.0 {
                        hero.position[1] += hero.velocity[1] * sub_dt;
                        hero.collider.grounded = false;
                    } else {
                        let ray_start = [
                            hero.position[0],
                            hero.position[1] + hero.collider.step_height,
                            hero.position[2],
                        ];
                        let max_ray_dist =
                            hero.collider.step_height + (-hero.velocity[1] * sub_dt).max(0.2);
                        if let Some((ground_y, _)) =
                            self.environment.mesh.raycast_down(ray_start, max_ray_dist)
                        {
                            hero.position[1] = ground_y;
                            hero.velocity[1] = 0.0;
                            hero.collider.grounded = true;
                        } else {
                            hero.position[1] += hero.velocity[1] * sub_dt;
                            hero.collider.grounded = false;
                        }
                    }

                    // Kill plane safety
                    if hero.position[1] < self.environment.kill_plane_y {
                        hero.position = self.environment.spawn_point;
                        hero.velocity = [0.0, 0.0, 0.0];
                        hero.collider.grounded = true;
                        break;
                    }
                }

                hero_pos = hero.position;
                hero_rot = hero.rotation;
            }
        }

        // 2. Attack Execution
        if let Some(kind) = triggered_attack {
            self.pending_audio_cues
                .push(WorldAudioCue::HeroAttack(kind));
            let fwd_x = hero_rot.sin();
            let fwd_z = -hero_rot.cos();

            match kind {
                AttackKind::SpecialProjectile => {
                    const PROJ_SPEED: f32 = 12.0;
                    self.projectiles.push(Projectile {
                        position: [hero_pos[0], hero_pos[1] + 0.8, hero_pos[2]],
                        velocity: [fwd_x * PROJ_SPEED, 0.0, fwd_z * PROJ_SPEED],
                        lifetime: 0.0,
                        max_lifetime: 1.5,
                        damage: 75,
                        radius: 0.5,
                        active: true,
                    });
                }
                AttackKind::PrimaryMelee | AttackKind::SecondarySweep => {
                    let (range, cos_half_angle, damage) = if kind == AttackKind::PrimaryMelee {
                        (1.8f32, (45.0f32.to_radians()).cos(), 50u32)
                    } else {
                        (2.8f32, -1.0f32, 100u32) // 360-degree sweep: all angles pass
                    };

                    let mut hits = Vec::new();
                    for target in &self.actors {
                        if !target.active {
                            continue;
                        }
                        let is_target = match target.kind {
                            ActorKind::Prop { destructible } => destructible,
                            ActorKind::Villain => true,
                            _ => false,
                        };
                        if !is_target {
                            continue;
                        }

                        let dx = target.position[0] - hero_pos[0];
                        let dz = target.position[2] - hero_pos[2];
                        let dist = (dx * dx + dz * dz).sqrt();
                        if dist <= range + target.collider.radius {
                            let in_cone = if dist > 0.001 && cos_half_angle > -0.999 {
                                let dir_x = dx / dist;
                                let dir_z = dz / dist;
                                let dot = dir_x * fwd_x + dir_z * fwd_z;
                                dot >= cos_half_angle
                            } else {
                                true
                            };

                            if in_cone {
                                hits.push((target.id, target.position, damage));
                            }
                        }
                    }

                    if !hits.is_empty() {
                        self.pending_audio_cues.push(WorldAudioCue::AttackHit);
                        for (tid, tpos, dmg) in hits {
                            self.apply_damage(tid, dmg, tpos);
                        }
                    }
                }
            }
        }

        // 3. Projectile Updates & Collision Resolution
        let mut proj_hits = Vec::new();
        for proj in &mut self.projectiles {
            if !proj.active {
                continue;
            }
            proj.lifetime += dt;
            if proj.lifetime >= proj.max_lifetime {
                proj.active = false;
                continue;
            }
            proj.position[0] += proj.velocity[0] * dt;
            proj.position[1] += proj.velocity[1] * dt;
            proj.position[2] += proj.velocity[2] * dt;

            // Check collision against destructible props and villains
            for target in &self.actors {
                if !target.active {
                    continue;
                }
                let is_target = match target.kind {
                    ActorKind::Prop { destructible } => destructible,
                    ActorKind::Villain => true,
                    _ => false,
                };
                if !is_target {
                    continue;
                }

                let dx = target.position[0] - proj.position[0];
                let dy = target.position[1] - proj.position[1];
                let dz = target.position[2] - proj.position[2];
                let dist_sq = dx * dx + dy * dy + dz * dz;
                let hit_radius = proj.radius + target.collider.radius;
                if dist_sq <= hit_radius * hit_radius {
                    proj.active = false;
                    proj_hits.push((target.id, proj.damage, target.position));
                    break;
                }
            }
        }
        self.projectiles.retain(|p| p.active);

        if !proj_hits.is_empty() {
            self.pending_audio_cues.push(WorldAudioCue::AttackHit);
            for (target_id, dmg, hit_pos) in proj_hits {
                self.apply_damage(target_id, dmg, hit_pos);
            }
        }

        // 4. Update Damage Popups
        for popup in &mut self.damage_popups {
            popup.timer += dt;
        }
        self.damage_popups.retain(|p| p.timer < p.max_lifetime);

        // 5. Proximity collection for active collectibles
        if has_hero {
            for actor in &mut self.actors {
                if let ActorKind::Collectible {
                    value,
                    ref mut collected,
                } = actor.kind
                {
                    if !*collected && actor.active {
                        let dx = actor.position[0] - hero_pos[0];
                        let dy = actor.position[1] - hero_pos[1];
                        let dz = actor.position[2] - hero_pos[2];
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        let pickup_radius = actor.collider.radius + 0.5;
                        if dist_sq <= pickup_radius * pickup_radius {
                            *collected = true;
                            actor.active = false;
                            self.coins += value;
                            self.pending_audio_cues.push(WorldAudioCue::PickupCollected);
                        }
                    }
                }
            }
        }

        // 6. Update triggers and interactive prompts
        let mut active_prompt = None;
        let mut triggered_transition: Option<(String, [f32; 3])> = None;

        if has_hero {
            for trigger in &self.triggers {
                let dx = hero_pos[0] - trigger.position[0];
                let dy = hero_pos[1] - trigger.position[1];
                let dz = hero_pos[2] - trigger.position[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist <= trigger.radius {
                    active_prompt = Some(trigger.prompt.clone());
                    if input.is_down(WiiButton::A) {
                        triggered_transition =
                            Some((trigger.target_zone.clone(), trigger.target_spawn));
                    }
                    break;
                }
            }
        }
        self.active_prompt = active_prompt;

        if let Some((target_zone, target_spawn)) = triggered_transition {
            self.pending_audio_cues
                .push(WorldAudioCue::ZoneEntered(target_zone.clone()));
            self.transition_to_zone(&target_zone, target_spawn);
        }

        // 7. Update actor animation state machine & time
        if let Some(hero_id) = self.active_hero_id {
            if let Some(hero) = self.actors.iter_mut().find(|a| a.id == hero_id) {
                let stick_mag =
                    (input.stick[0] * input.stick[0] + input.stick[1] * input.stick[1]).sqrt();
                let next_anim = if let Some(ref attack) = hero.active_attack {
                    match attack.kind {
                        AttackKind::PrimaryMelee => {
                            format!("attack_combo_{}", hero.combo_step.max(1))
                        }
                        AttackKind::SecondarySweep => "attack_sweep".to_string(),
                        AttackKind::SpecialProjectile => "attack_projectile".to_string(),
                    }
                } else if stick_mag > 0.5 {
                    "run".to_string()
                } else if stick_mag > 0.1 {
                    "walk".to_string()
                } else {
                    "idle".to_string()
                };

                if hero.current_anim.as_deref() != Some(&next_anim) {
                    hero.current_anim = Some(next_anim);
                    hero.animation_time = 0.0;
                }
            }
        }

        for actor in &mut self.actors {
            if actor.active {
                actor.animation_time += dt * actor.anim_speed;
            }
        }
    }

    pub fn tick(&mut self, dt: f32, input: &WiiInputState) {
        self.update(dt, input);
    }

    pub fn apply_damage(&mut self, target_id: u32, damage: u32, hit_pos: [f32; 3]) {
        self.damage_popups.push(DamagePopup {
            text: format!("-{}", damage),
            world_pos: [hit_pos[0], hit_pos[1] + 1.0, hit_pos[2]],
            timer: 0.0,
            max_lifetime: 1.0,
            color: [1.0, 0.2, 0.2, 1.0],
        });

        let mut target_died = false;
        let mut target_kind = None;
        let mut target_pos = [0.0f32; 3];

        if let Some(target) = self.actors.iter_mut().find(|a| a.id == target_id) {
            target.health = target.health.saturating_sub(damage);
            if target.health == 0 {
                target_died = true;
                target_kind = Some(target.kind.clone());
                target_pos = target.position;
            }
        }

        if target_died {
            if let Some(kind) = target_kind {
                match kind {
                    ActorKind::Prop { destructible: true } => {
                        self.pending_audio_cues.push(WorldAudioCue::PropDestroyed);
                        // Spawn 1-3 collectible coins/gems
                        self.spawn_collectible(
                            "coin_drop_1",
                            [target_pos[0], target_pos[1], target_pos[2]],
                            1,
                            None,
                            None,
                        );
                        self.spawn_collectible(
                            "coin_drop_2",
                            [target_pos[0] + 0.4, target_pos[1], target_pos[2] + 0.2],
                            1,
                            None,
                            None,
                        );
                        self.spawn_collectible(
                            "gem_drop_3",
                            [target_pos[0] - 0.3, target_pos[1], target_pos[2] - 0.3],
                            5,
                            None,
                            None,
                        );
                        self.despawn(target_id);
                    }
                    ActorKind::Villain => {
                        self.spawn_collectible(
                            "villain_loot_1",
                            [target_pos[0] + 0.3, target_pos[1], target_pos[2]],
                            5,
                            None,
                            None,
                        );
                        self.spawn_collectible(
                            "villain_loot_2",
                            [target_pos[0] - 0.3, target_pos[1], target_pos[2]],
                            5,
                            None,
                            None,
                        );
                        self.despawn(target_id);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn transition_to_zone(&mut self, target_zone: &str, target_spawn: [f32; 3]) {
        self.active_zone = target_zone.to_string();
        self.unload_zone_entities();
        self.environment.spawn_point = target_spawn;
        if let Some(hero_id) = self.active_hero_id {
            if let Some(hero) = self.actors.iter_mut().find(|a| a.id == hero_id) {
                hero.position = target_spawn;
                hero.velocity = [0.0, 0.0, 0.0];
            }
        }
    }

    pub fn hud_state(&self, state: &crate::state::GameStateMachine) -> HudState {
        let (hero_name, hero_element, health, max_health) = if let Some(hero) = self
            .active_hero_id
            .and_then(|id| self.actors.iter().find(|a| a.id == id))
        {
            let (name, elem) = crate::portal_gameplay::get_character_info(hero.character_id);
            let display_name = if hero.name.is_empty() {
                name.to_string()
            } else {
                hero.name.clone()
            };
            (Some(display_name), Some(elem), hero.health, hero.max_health)
        } else {
            (None, None, 0, 0)
        };

        let villain_capture = match state.current() {
            crate::state::GameState::VillainCapture {
                villain_id,
                progress,
            } => {
                let (vname, _) = crate::portal_gameplay::get_character_info(*villain_id);
                Some((vname.to_string(), *progress))
            }
            _ => None,
        };

        HudState {
            hero_name,
            hero_element,
            health,
            max_health,
            coins: self.coins,
            villain_capture,
            prompt: self.active_prompt.clone(),
            transition: match state.current() {
                crate::state::GameState::ZoneTransition {
                    from_zone,
                    to_zone,
                    timer,
                    fade_duration,
                } => Some((from_zone.clone(), to_zone.clone(), *timer, *fade_duration)),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HudState {
    pub hero_name: Option<String>,
    pub hero_element: Option<crate::portal_gameplay::Element>,
    pub health: u32,
    pub max_health: u32,
    pub coins: u32,
    pub villain_capture: Option<(String, f32)>,
    pub prompt: Option<String>,
    pub transition: Option<(String, String, f32, f32)>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marker_spawning_and_collectible_pickup() {
        use skylands_formats::mat4_identity;

        let mut world = World::new();

        let markers = vec![
            LevelMarker {
                name: "sp_hero".to_string(),
                kind: MarkerKind::Spawn,
                position: [1.0, 0.0, 2.0],
                world_transform: mat4_identity(),
                node_index: 0,
                geometry_index: None,
                mesh: None,
            },
            LevelMarker {
                name: "prop_crate_destructible".to_string(),
                kind: MarkerKind::Prop,
                position: [5.0, 0.0, 5.0],
                world_transform: mat4_identity(),
                node_index: 1,
                geometry_index: None,
                mesh: None,
            },
            LevelMarker {
                name: "pickup_gem_rare".to_string(),
                kind: MarkerKind::Collectible,
                position: [2.0, 0.0, 2.0],
                world_transform: mat4_identity(),
                node_index: 2,
                geometry_index: None,
                mesh: None,
            },
        ];

        // 1. Load entities from markers
        world.load_level_entities(&markers);
        assert_eq!(world.environment.spawn_point, [1.0, 0.0, 2.0]);
        assert_eq!(world.actors.len(), 2);

        let prop = world
            .actors
            .iter()
            .find(|a| a.name == "prop_crate_destructible")
            .unwrap();
        assert_eq!(prop.kind, ActorKind::Prop { destructible: true });
        assert_eq!(prop.position, [5.0, 0.0, 5.0]);

        let gem = world
            .actors
            .iter()
            .find(|a| a.name == "pickup_gem_rare")
            .unwrap();
        assert_eq!(
            gem.kind,
            ActorKind::Collectible {
                value: 5,
                collected: false
            }
        );
        assert!(gem.active);

        // 2. Spawn hero at spawn point
        let hero_id = world.spawn(
            400,
            0,
            "Snap Shot",
            world.environment.spawn_point,
            false,
            None,
            None,
        );
        assert_eq!(world.active_hero_id, Some(hero_id));

        // 3. Update world: hero at [1, 0, 2], gem at [2, 0, 2] (distance 1.0 < pickup radius 1.25)
        let input = WiiInputState::new();
        world.update(0.016, &input);

        let cues = world.drain_audio_cues();
        assert_eq!(cues, vec![WorldAudioCue::PickupCollected]);

        let gem_after = world
            .actors
            .iter()
            .find(|a| a.name == "pickup_gem_rare")
            .unwrap();
        assert_eq!(
            gem_after.kind,
            ActorKind::Collectible {
                value: 5,
                collected: true
            }
        );
        assert!(!gem_after.active);
        assert_eq!(world.coins, 5);

        let sm = crate::state::GameStateMachine::new();
        let hud = world.hud_state(&sm);
        assert_eq!(hud.hero_name.as_deref(), Some("Snap Shot"));
        assert_eq!(
            hud.hero_element,
            Some(crate::portal_gameplay::Element::Water)
        );
        assert_eq!(hud.health, 500);
        assert_eq!(hud.max_health, 500);
        assert_eq!(hud.coins, 5);
        assert!(hud.villain_capture.is_none());
    }

    #[test]
    fn test_actor_spawn_move() {
        let mut world = World::new();
        let id = world.spawn(
            400,
            0,
            "Snap Shot",
            [0.0, 0.0, 0.0],
            false,
            Some("models/characters/snapshot.igb".into()),
            Some("textures/characters/snapshot.png".into()),
        );
        assert_eq!(world.active_hero_id, Some(id));

        let mut input = WiiInputState::new();
        input.set_stick(1.0, 0.0);

        world.update(1.0, &input);
        let hero = world.actors.iter().find(|a| a.id == id).unwrap();
        assert!((hero.position[0] - 5.0).abs() < 1e-4);
        assert_eq!(hero.position[1], 0.0);
        assert_eq!(hero.position[2], 0.0);
        assert!(hero.collider.grounded);

        assert!(world.despawn_by_character_id(400));
        assert!(world.actors.is_empty());
        assert_eq!(world.active_hero_id, None);
    }

    #[test]
    fn test_actor_wall_collision_and_jump() {
        let mut world = World::new();
        let id = world.spawn(400, 0, "Snap Shot", [0.0, 0.0, -19.0], false, None, None);

        // Move north towards wall at z = -20
        let mut input = WiiInputState::new();
        input.set_stick(0.0, 1.0); // -Z movement
        world.update(0.5, &input);

        let hero = world.actors.iter().find(|a| a.id == id).unwrap();
        // North wall is at z = -20. With radius = 0.5, z cannot penetrate past -19.5
        assert!(
            hero.position[2] >= -19.5,
            "Hero penetrated wall: z={}",
            hero.position[2]
        );

        // Jump test
        input.press(WiiButton::A);
        world.update(0.1, &input);
        let hero_jumping = world.actors.iter().find(|a| a.id == id).unwrap();
        assert!(
            hero_jumping.position[1] > 0.0,
            "Hero should be airborne after jump"
        );
        assert!(!hero_jumping.collider.grounded);
    }

    #[test]
    fn test_hero_primary_attack_destroys_prop_and_drops_loot() {
        let mut world = World::new();
        let hero_id = world.spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);
        assert_eq!(world.active_hero_id, Some(hero_id));

        // Spawn a destructible barrel 1 meter directly in front (facing along -Z)
        let prop_id = world.spawn_prop("barrel", [0.0, 0.0, -1.0], true, None, None);
        // Set health to 50 so 1 primary attack (50 dmg) destroys it
        if let Some(prop) = world.actors.iter_mut().find(|a| a.id == prop_id) {
            prop.health = 50;
        }

        // Trigger primary melee attack (WiiButton::B)
        let mut input = WiiInputState::new();
        input.press(WiiButton::B);
        world.update(0.016, &input);

        // Prop should be despawned and replaced by 3 collectible drops
        assert!(world.actors.iter().find(|a| a.id == prop_id).is_none());
        let collectibles: Vec<_> = world
            .actors
            .iter()
            .filter(|a| matches!(a.kind, ActorKind::Collectible { .. }))
            .collect();
        assert_eq!(collectibles.len(), 3);

        // Damage popup should exist
        assert_eq!(world.damage_popups.len(), 1);
        assert_eq!(world.damage_popups[0].text, "-50");

        // Audio cues should have HeroAttack, AttackHit, PropDestroyed
        let cues = world.drain_audio_cues();
        assert!(cues.contains(&WorldAudioCue::HeroAttack(AttackKind::PrimaryMelee)));
        assert!(cues.contains(&WorldAudioCue::AttackHit));
        assert!(cues.contains(&WorldAudioCue::PropDestroyed));

        // Now collect drops by stepping close
        let mut move_input = WiiInputState::new();
        move_input.set_stick(0.0, 0.5); // move north towards drops
        world.update(0.5, &move_input);
        assert!(world.coins > 0);
    }

    #[test]
    fn test_hero_attack_combo_chain() {
        let mut world = World::new();
        let hero_id = world.spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);

        let mut input = WiiInputState::new();
        input.press(WiiButton::B);

        // Attack 1
        world.update(0.016, &input);
        let hero = world.actors.iter().find(|a| a.id == hero_id).unwrap();
        assert_eq!(hero.combo_step, 1);
        assert!(hero.active_attack.is_some());

        // Wait out attack duration (0.25s) without letting combo cooldown (0.6s) expire
        let neutral = WiiInputState::new();
        world.update(0.26, &neutral);
        let hero = world.actors.iter().find(|a| a.id == hero_id).unwrap();
        assert!(hero.active_attack.is_none());
        assert_eq!(hero.combo_step, 1);

        // Attack 2
        world.update(0.016, &input);
        let hero = world.actors.iter().find(|a| a.id == hero_id).unwrap();
        assert_eq!(hero.combo_step, 2);

        world.update(0.26, &neutral);

        // Attack 3
        world.update(0.016, &input);
        let hero = world.actors.iter().find(|a| a.id == hero_id).unwrap();
        assert_eq!(hero.combo_step, 3);

        // Wait longer than 0.6s cooldown -> combo resets to 0
        world.update(0.7, &neutral);
        let hero = world.actors.iter().find(|a| a.id == hero_id).unwrap();
        assert_eq!(hero.combo_step, 0);
    }

    #[test]
    fn test_projectile_flight_and_hit() {
        let mut world = World::new();
        let _hero_id = world.spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);

        // Spawn villain enemy 6 meters forward along -Z
        let enemy_id = world.spawn(1010, 0, "Gulper", [0.0, 0.0, -6.0], true, None, None);
        if let Some(enemy) = world.actors.iter_mut().find(|a| a.id == enemy_id) {
            enemy.health = 75;
        }

        // Hero fires special projectile (WiiButton::One)
        let mut input = WiiInputState::new();
        input.press(WiiButton::One);
        world.update(0.016, &input);
        assert_eq!(world.projectiles.len(), 1);

        // Advance 0.5s: at 12m/s forward, travels 6m reaching enemy
        let neutral = WiiInputState::new();
        world.update(0.5, &neutral);

        // Enemy took 75 damage, dropped loot and despawned
        assert!(world.actors.iter().find(|a| a.id == enemy_id).is_none());
        assert!(world.projectiles.is_empty());
        assert!(!world.damage_popups.is_empty());
        assert_eq!(world.damage_popups[0].text, "-75");
    }

    #[test]
    fn test_zone_trigger_detection_and_prompt() {
        let mut world = World::new();
        let hero_id = world.spawn(400, 0, "Snap Shot", [0.0, 0.0, 0.0], false, None, None);
        world.active_hero_id = Some(hero_id);

        world.triggers.push(ZoneTrigger {
            id: 1,
            position: [0.0, 0.0, 5.0],
            radius: 2.0,
            target_zone: "Library".to_string(),
            target_spawn: [1.0, 0.0, 1.0],
            prompt: "Press A to enter Library".to_string(),
        });

        let sm = crate::state::GameStateMachine::new();

        // 1. Outside radius -> no prompt
        let neutral = WiiInputState::new();
        world.update(0.016, &neutral);
        assert_eq!(world.active_prompt, None);
        let hud = world.hud_state(&sm);
        assert_eq!(hud.prompt, None);

        // 2. Move hero inside trigger radius (at [0.0, 0.0, 4.5])
        if let Some(hero) = world.actors.iter_mut().find(|a| a.id == hero_id) {
            hero.position = [0.0, 0.0, 4.5];
        }
        world.update(0.016, &neutral);
        assert_eq!(
            world.active_prompt,
            Some("Press A to enter Library".to_string())
        );
        let hud = world.hud_state(&sm);
        assert_eq!(hud.prompt, Some("Press A to enter Library".to_string()));
    }

    #[test]
    fn test_zone_transition_hero_reposition() {
        let mut world = World::new();
        let hero_id = world.spawn(400, 0, "Snap Shot", [0.0, 0.0, 4.5], false, None, None);
        world.active_hero_id = Some(hero_id);

        // Spawn a prop that should be unloaded during zone transition
        let prop_id = world.spawn_prop("prop_barrel", [1.0, 0.0, 1.0], true, None, None);
        assert!(world.actors.iter().any(|a| a.id == prop_id));

        world.triggers.push(ZoneTrigger {
            id: 1,
            position: [0.0, 0.0, 5.0],
            radius: 2.0,
            target_zone: "Academy Arena".to_string(),
            target_spawn: [10.0, 1.0, -15.0],
            prompt: "Press A to enter Academy Arena".to_string(),
        });

        // Press A while inside trigger
        let mut input = WiiInputState::new();
        input.press(WiiButton::A);
        world.update(0.016, &input);

        // Zone entered audio cue emitted
        let cues = world.drain_audio_cues();
        assert_eq!(
            cues,
            vec![WorldAudioCue::ZoneEntered("Academy Arena".to_string())]
        );

        // Active zone updated, hero repositioned at target spawn
        assert_eq!(world.active_zone, "Academy Arena");
        let hero = world.actors.iter().find(|a| a.id == hero_id).unwrap();
        assert_eq!(hero.position, [10.0, 1.0, -15.0]);

        // Old dynamic prop entity unloaded; hero preserved
        assert!(world.actors.iter().all(|a| a.id == hero_id));
    }
}
