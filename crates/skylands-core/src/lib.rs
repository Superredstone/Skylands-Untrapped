pub mod audio;
pub mod collision;
pub mod components;
pub mod events;
pub mod input;
pub mod portal_gameplay;
pub mod resources;
pub mod states;
pub mod systems;

pub use audio::{resample_to_8khz_u8, PortalSpeakerSink};
pub use collision::{Aabb, CollisionMesh, Triangle};
pub use components::*;
pub use events::*;
pub use input::{InputMapper, KeyCode, WiiButton, WiiInputState};
pub use portal_gameplay::{
    get_character_asset_paths, get_character_info, get_character_voice_path,
    resolve_character_id_by_name, resolve_character_model_archive, resolve_model_archive, Element,
};
pub use resources::*;
pub use states::GameState;
pub use systems::*;

use bevy::prelude::*;

pub struct SkylandsCorePlugin;

impl Plugin for SkylandsCorePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::state::app::StatesPlugin>() {
            app.add_plugins(bevy::state::app::StatesPlugin);
        }

        app.insert_resource(Time::<Fixed>::from_hz(60.0))
            .init_state::<GameState>()
            .add_event::<PortalEvent>()
            .add_event::<DamageEvent>()
            .add_event::<WorldAudioCueEvent>()
            .add_event::<ZoneTransitionEvent>()
            .init_resource::<PortalGameplayState>()
            .init_resource::<GameStats>()
            .init_resource::<WiiInputState>()
            .init_resource::<PortalAudioStreaming>()
            .init_resource::<CaptureProgress>()
            .init_resource::<ZoneTransitionProgress>()
            .init_resource::<EnvironmentCollision>()
            .init_resource::<ActivePortal>()
            .add_systems(Startup, systems::setup_default_scene)
            .add_systems(
                FixedUpdate,
                (
                    systems::poll_portal_system,
                    systems::portal_gameplay_system,
                    systems::input_mapping_system,
                    apply_deferred,
                    systems::hero_movement_system,
                    systems::actor_physics_system,
                    systems::hero_combat_system,
                    apply_deferred,
                    systems::projectile_flight_system,
                    systems::damage_system,
                    apply_deferred,
                    systems::collectible_pickup_system,
                    systems::zone_trigger_system,
                    systems::animation_state_system,
                    systems::portal_audio_stream_system,
                    systems::state_transition_update_system,
                    apply_state_transitions,
                )
                    .chain(),
            );
    }
}

fn apply_state_transitions(world: &mut World) {
    let _ = world.try_run_schedule(bevy::state::state::StateTransition);
}
