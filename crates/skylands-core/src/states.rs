use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Hub,
    VillainCapture,
    ZoneTransition,
    /// `level/Title.bld` main menu; gameplay waits until Adventure is chosen.
    Title,
}
