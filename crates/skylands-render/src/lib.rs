pub mod camera;
pub mod hud;
pub mod mesh_conversion;
pub mod texture_conversion;
pub mod viewer;
pub mod visual_sync;

pub use camera::{
    camera_follow_system, free_cam_system, orbit_cam_system, setup_camera, setup_lighting,
    FreeCamState, MainCamera, OrbitCameraState,
};
pub use hud::{damage_popup_ui_system, hud_update_system, setup_hud_ui};
pub use mesh_conversion::{
    igb_mesh_to_bevy, igb_model_to_bevy_meshes, normalize_model_mesh, Mesh, Vertex,
};
pub use texture_conversion::{
    create_colored_material, create_textured_material, rgba8_to_bevy_image,
};
pub use viewer::{viewer_controls_system, ViewerSettings, SHOWCASE_CHARACTERS};
pub use visual_sync::{
    actor_visual_sync_system, setup_environment_and_assets, CharacterAssetServer,
    CharacterVisualRegistry, RenderMeshAssets,
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
            .init_resource::<FreeCamState>()
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
                ),
            )
            .add_systems(
                Update,
                (
                    actor_visual_sync_system,
                    camera_follow_system,
                    free_cam_system,
                    orbit_cam_system,
                    viewer_controls_system,
                    hud_update_system,
                    damage_popup_ui_system,
                ),
            );
    }
}
