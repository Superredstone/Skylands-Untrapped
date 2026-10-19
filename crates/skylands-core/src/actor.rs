use crate::input::WiiInputState;

#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    pub id: u32,
    pub name: String,
    pub character_id: u16,
    pub variant: u16,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub rotation: f32,
    pub is_villain: bool,
    pub active: bool,
    pub model_path: Option<String>,
    pub texture_path: Option<String>,
}

pub struct World {
    next_id: u32,
    pub actors: Vec<Actor>,
    pub active_hero_id: Option<u32>,
}

impl World {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            actors: Vec::new(),
            active_hero_id: None,
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

        let actor = Actor {
            id,
            name: name.into(),
            character_id,
            variant,
            position,
            velocity: [0.0, 0.0, 0.0],
            rotation: 0.0,
            is_villain,
            active: true,
            model_path,
            texture_path,
        };

        if !is_villain && self.active_hero_id.is_none() {
            self.active_hero_id = Some(id);
        }

        self.actors.push(actor);
        id
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

                hero.position[0] += hero.velocity[0] * dt;
                hero.position[2] += hero.velocity[2] * dt;

                if input.stick[0].abs() > 0.1 || input.stick[1].abs() > 0.1 {
                    hero.rotation = input.stick[0].atan2(-input.stick[1]);
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
        assert_eq!(hero.position[0], 5.0);

        assert!(world.despawn_by_character_id(400));
        assert!(world.actors.is_empty());
        assert_eq!(world.active_hero_id, None);
    }
}
