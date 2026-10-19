#[derive(Debug, Clone, PartialEq)]
pub enum GameState {
    Boot,
    Title,
    Hub { academy_area: String },
    Level { level_id: u32, name: String },
    VillainCapture { villain_id: u16, progress: f32 },
}

pub struct GameStateMachine {
    current: GameState,
    previous: Option<GameState>,
}

impl GameStateMachine {
    pub fn new() -> Self {
        Self {
            current: GameState::Boot,
            previous: None,
        }
    }

    pub fn current(&self) -> &GameState {
        &self.current
    }

    pub fn set_state(&mut self, next: GameState) {
        let prev = std::mem::replace(&mut self.current, next);
        self.previous = Some(prev);
    }

    pub fn previous(&self) -> Option<&GameState> {
        self.previous.as_ref()
    }

    pub fn tick(&mut self, dt: f32) {
        if let GameState::VillainCapture {
            villain_id: _,
            progress,
        } = &mut self.current
        {
            *progress = (*progress + dt * 0.5).min(1.0);
            if *progress >= 1.0 {
                // Restore state prior to capture, or default to Hub
                let next = self
                    .previous
                    .take()
                    .unwrap_or_else(|| GameState::Hub {
                        academy_area: "Courtyard".into(),
                    });
                self.current = next;
            }
        }
    }

    pub fn is_villain_capture(&self) -> bool {
        matches!(self.current, GameState::VillainCapture { .. })
    }
}

impl Default for GameStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        let mut sm = GameStateMachine::new();
        assert_eq!(sm.current(), &GameState::Boot);

        sm.set_state(GameState::Title);
        assert_eq!(sm.current(), &GameState::Title);

        sm.set_state(GameState::Hub {
            academy_area: "Library".into(),
        });
        assert_eq!(
            sm.current(),
            &GameState::Hub {
                academy_area: "Library".into()
            }
        );

        sm.set_state(GameState::VillainCapture {
            villain_id: 1010,
            progress: 0.0,
        });
        assert!(sm.is_villain_capture());

        // 2 seconds advances progress by 1.0 -> transition back to Hub
        sm.tick(2.1);
        assert!(!sm.is_villain_capture());
        assert_eq!(
            sm.current(),
            &GameState::Hub {
                academy_area: "Library".into()
            }
        );
    }
}
