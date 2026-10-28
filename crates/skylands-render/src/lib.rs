pub mod camera;
pub mod debug_menu;
pub mod hud;
pub mod magic_moment;
pub mod mesh_conversion;
pub mod sky4_hud;
pub mod texture_conversion;
pub mod viewer;
pub mod visual_sync;

pub use camera::{
    camera_follow_system, free_cam_system, orbit_cam_system, setup_camera, setup_lighting,
    sky_camera_sync_system, FreeCamState, MainCamera, OrbitCameraState, SkyCamera, SKY_LAYER,
};
pub use debug_menu::{
    debug_menu_interaction_system, debug_menu_scroll_system, debug_menu_telemetry_system,
    debug_menu_toggle_system, setup_debug_menu_ui, DebugMenuState, DebugTab,
};
pub use hud::{
    damage_popup_ui_system, dialogue_box_system, hud_update_system, setup_dialogue_box,
    setup_hud_ui,
};
pub use magic_moment::{
    magic_moment_layer_system, magic_moment_system, setup_magic_moment, MagicMomentAssets,
    MagicMomentModel, MagicMomentPose, MM_LAYER,
};
pub use mesh_conversion::{
    combine_or_normalize_model_meshes, combine_scene_instances_to_bevy, igb_character_submeshes,
    igb_mesh_to_bevy, igb_model_to_bevy_mesh, igb_model_to_bevy_meshes, normalize_model_mesh,
    CharacterSubmesh, Mesh, Vertex,
};
pub use sky4_hud::{
    setup_sky4_hud, sky4_hud_layout_system, sky4_hud_portrait_system, sky4_hud_text_system,
    sky4_hud_update_system, Sky4HudAssets,
};
pub use texture_conversion::{
    create_colored_material, create_textured_material, rgba8_to_bevy_image,
};
pub use viewer::{viewer_controls_system, ViewerSettings, SHOWCASE_CHARACTERS};
pub use visual_sync::{
    actor_visual_sync_system, character_animation_system, level_render_mesh_sync_system,
    scripted_actor_mesh_follow_system, setup_environment_and_assets, spawn_level_render_mesh,
    spawned_actor_mesh_system, CharacterAnimator, CharacterAssetServer, CharacterVisualRegistry,
    LevelRenderMesh, LevelSkyMesh, ModelVisualRegistry, RenderMeshAssets, ScriptedActorMesh,
    SpawnedActorVisual, TemplateMeshes, TemplateRigs,
};

use bevy::prelude::*;

#[derive(Default)]
pub struct SkylandsRenderPlugin {
    pub viewer_mode: bool,
}

impl Plugin for SkylandsRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterAssetServer>()
            .init_resource::<CharacterVisualRegistry>()
            .init_resource::<ModelVisualRegistry>()
            .init_resource::<FreeCamState>()
            .init_resource::<DebugMenuState>()
            .insert_resource(OrbitCameraState {
                active: self.viewer_mode,
                ..default()
            })
            .insert_resource(ViewerSettings {
                viewer_mode: self.viewer_mode,
                ..default()
            })
            .add_systems(
                Startup,
                (
                    setup_camera,
                    setup_lighting,
                    setup_environment_and_assets,
                    setup_hud_ui,
                    setup_dialogue_box,
                    setup_sky4_hud,
                    setup_debug_menu_ui,
                    setup_magic_moment,
                ),
            )
            .add_systems(
                Update,
                (
                    actor_visual_sync_system,
                    character_animation_system,
                    level_render_mesh_sync_system,
                    scripted_actor_mesh_follow_system,
                    spawned_actor_mesh_system,
                    camera_follow_system,
                    free_cam_system,
                    orbit_cam_system,
                    sky_camera_sync_system
                        .after(camera_follow_system)
                        .after(free_cam_system)
                        .after(orbit_cam_system),
                    viewer_controls_system,
                    hud_update_system,
                    dialogue_box_system,
                    damage_popup_ui_system,
                    (
                        sky4_hud_portrait_system,
                        sky4_hud_update_system,
                        sky4_hud_text_system,
                        sky4_hud_layout_system,
                    )
                        .chain(),
                    debug_menu_toggle_system,
                    debug_menu_interaction_system,
                    debug_menu_telemetry_system,
                    debug_menu_scroll_system,
                    (magic_moment_system, magic_moment_layer_system)
                        .chain()
                        .before(actor_visual_sync_system)
                        .before(character_animation_system),
                ),
            );
    }
}
