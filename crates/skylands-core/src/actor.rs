use crate::collision::{CharacterCollider, Environment};
use crate::input::{WiiButton, WiiInputState};
use skylands_formats::{LevelMarker, MarkerKind};

#[derive(Debug, Clone, PartialEq)]
pub enum ActorKind {
    Hero,
    Villain,
    Prop { destructible: bool },
    Collectible { value: u32, collected: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldAudioCue {
    PickupCollected,
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
}

pub struct World {
    next_id: u32,
    pub actors: Vec<Actor>,
    pub active_hero_id: Option<u32>,
    pub environment: Environment,
    pub pending_audio_cues: Vec<WorldAudioCue>,
}

impl World {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            actors: Vec::new(),
            active_hero_id: None,
            environment: Environment::default(),
            pending_audio_cues: Vec::new(),
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
        };

        self.actors.push(actor);
        id
    }

    pub fn drain_audio_cues(&mut self) -> Vec<WorldAudioCue> {
        std::mem::take(&mut self.pending_audio_cues)
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
        if let Some(hero_id) = self.active_hero_id {
            if let Some(hero) = self.actors.iter_mut().find(|a| a.id == hero_id) {
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
            }
        }

        // Proximity collection for active collectibles
        let hero_pos = self
            .active_hero_id
            .and_then(|id| self.actors.iter().find(|a| a.id == id))
            .map(|h| h.position);

        if let Some(h_pos) = hero_pos {
            for actor in &mut self.actors {
                if let ActorKind::Collectible {
                    ref mut collected, ..
                } = actor.kind
                {
                    if !*collected && actor.active {
                        let dx = actor.position[0] - h_pos[0];
                        let dy = actor.position[1] - h_pos[1];
                        let dz = actor.position[2] - h_pos[2];
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        let pickup_radius = actor.collider.radius + 0.5;
                        if dist_sq <= pickup_radius * pickup_radius {
                            *collected = true;
                            actor.active = false;
                            self.pending_audio_cues.push(WorldAudioCue::PickupCollected);
                        }
                    }
                }
            }
        }
    }
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
}
