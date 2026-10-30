//! Title main menu scene, rebuilt from `level/Title.bld`. The page is a rotating rig
//! (`ui_menu_3d_carousel.ai`): `MainMenuCarousel` (`LevelSelect_Base.mdl`, 18 attach nodes)
//! spawned at `"3d menu pos"` (`Logo Pos`), one silver button per node, and on focus the gold
//! highlight. The focus glow (`MainMenuGlow`) is not drawn: its model does not decode. Menu state: `skylands_core::TitleMenu`. Notes: `agent/TITLE_MENU.md`.

use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use bevy::utils::HashMap;
use skylands_core::{GameState, TitleMenu, FEET_TO_UNITS, TITLE_BUTTONS, TITLE_RIG_NODES};
use skylands_formats::igb::{AnimationClip, Skeleton};

use crate::camera::{MainCamera, SkyCamera};
use crate::magic_moment::{load_parts, vertical_fov, MmPart};
use crate::visual_sync::CharacterAssetServer;

pub const TITLE_LAYER: usize = 3;
const TITLE_BLD: &str = "Title.bld";

/// "Menu - Main - Create": `MainMenuCarousel` uniform scale 60, every button 85.
const RIG_SCALE: f32 = 0.60;
const BUTTON_SCALE: f32 = 0.85;
/// "Button Focus": button and highlight scale to 120% over 0.2 s.
const FOCUS_SCALE: f32 = 1.2;
const FOCUS_SCALE_TIME: f32 = 0.2;
/// Highlight alpha 0 -> 255 over 2.5 s, displaced 1 foot at `heading + 180`.
const HIGHLIGHT_FADE_TIME: f32 = 2.5;
const HIGHLIGHT_OFFSET: f32 = FEET_TO_UNITS;
/// "bounce selected": the rig plays the node's state at `playback rate` 125 sliding to 25
/// over 9 s.
const BOUNCE_RATE_START: f32 = 1.25;
const BOUNCE_RATE_END: f32 = 0.25;
const BOUNCE_RATE_TIME: f32 = 9.0;
/// "Title - Show 3d Environment" camera: `look at` `"3d menu pos"` (the rig's centre), `look
/// from` the same actor displaced `look from distance offset` 100 (feet) at its heading (0),
/// `fov` 68 (horizontal), `far clip` 2000. The page itself passes no camera.
const CAMERA_DISTANCE: f32 = 100.0 * FEET_TO_UNITS;
const CAMERA_FOV: f32 = 68.0;
const CAMERA_FAR: f32 = 2000.0;

#[derive(Component)]
pub struct TitleCamera;

#[derive(Component)]
pub struct TitleScene;

/// What a title model is, for its per-frame pose.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlePiece {
    Rig,
    /// Silver button of button id `0..3`.
    Button(usize),
    /// Gold highlight of button id `0..3`.
    Highlight(usize),
}

struct CarouselRig {
    skeleton: Skeleton,
    /// Animation state id (= node index) -> bounce clip.
    clips: HashMap<i32, AnimationClip>,
    /// Attach node index -> bone index.
    nodes: Vec<usize>,
}

/// Title art, loaded on the first title frame.
#[derive(Resource, Default)]
pub struct TitleAssets {
    loaded: bool,
    rig: Option<CarouselRig>,
    rig_parts: Vec<MmPart>,
    silver: Vec<Vec<MmPart>>,
    gold: Vec<Vec<MmPart>>,
}

impl TitleAssets {
    fn load(
        &mut self,
        asset_server: Option<&CharacterAssetServer>,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
    ) {
        self.loaded = true;
        let Some(loader) = asset_server.and_then(|s| s.loader.lock().ok()) else {
            return;
        };
        let Ok(model) = loader.load_igb(TITLE_BLD) else {
            warn!("Title menu: {TITLE_BLD} not found");
            return;
        };
        let actor = |name: &str| model.actor_models.iter().find(|a| a.actor == name);
        let parts = |name: &str, meshes: &mut Assets<Mesh>, images: &mut Assets<Image>| {
            actor(name)
                .map(|a| load_parts(&model, &a.meshes, &loader, TITLE_BLD, meshes, images, false))
                .unwrap_or_default()
        };
        if let Some(rig) = actor("MainMenuCarousel") {
            self.rig = rig
                .skeleton
                .and_then(|s| model.skeletons.get(s))
                .map(|skel| {
                    let bone = |name: &str| skel.bones.iter().position(|b| b.name == name);
                    CarouselRig {
                        nodes: rig
                            .attach_nodes
                            .iter()
                            .filter_map(|n| bone(&n.bone))
                            .collect(),
                        clips: rig
                            .animation_states
                            .iter()
                            .filter_map(|(&id, &clip)| {
                                Some((id, model.animations.get(clip)?.clone()))
                            })
                            .collect(),
                        skeleton: skel.clone(),
                    }
                });
        }
        self.rig_parts = parts("MainMenuCarousel", meshes, images);
        // "Main Menu Buttons Silver" / "Gold", in button id order.
        for i in 0..TITLE_BUTTONS.len() {
            self.silver
                .push(parts(&format!("MainMenu Button S {i:03}"), meshes, images));
            self.gold
                .push(parts(&format!("MainMenu Button G {i:03}"), meshes, images));
        }
    }

    /// Global (rig model space) transform of every bone at the current bounce pose.
    fn joint_globals(&self, selected: usize, focus_elapsed: f32) -> Vec<Mat4> {
        let Some(rig) = &self.rig else {
            return Vec::new();
        };
        let node = (selected % TITLE_RIG_NODES) as i32;
        if let Some(clip) = rig.clips.get(&node) {
            let t = bounce_time(focus_elapsed) % clip.duration.max(1e-3);
            return clip
                .sample_global_transforms(t, &rig.skeleton)
                .iter()
                .map(Mat4::from_cols_array)
                .collect();
        }
        rig.skeleton
            .bones
            .iter()
            .map(|b| Mat4::from_cols_array(&b.inverse_bind_pose).inverse())
            .collect()
    }

    fn node(&self, globals: &[Mat4], button: usize) -> Mat4 {
        self.rig
            .as_ref()
            .and_then(|r| r.nodes.get(button % TITLE_RIG_NODES))
            .and_then(|&b| globals.get(b))
            .copied()
            .unwrap_or(Mat4::IDENTITY)
    }
}

/// Clip time after `t` seconds of the bounce, whose rate slides linearly from 125% to 25%.
pub fn bounce_time(t: f32) -> f32 {
    let ramp = t.min(BOUNCE_RATE_TIME);
    let slope = (BOUNCE_RATE_END - BOUNCE_RATE_START) / BOUNCE_RATE_TIME;
    BOUNCE_RATE_START * ramp
        + 0.5 * slope * ramp * ramp
        + BOUNCE_RATE_END * (t - BOUNCE_RATE_TIME).max(0.0)
}

/// Focused button scale factor `t` seconds after focus.
pub fn focus_scale(t: f32) -> f32 {
    1.0 + (FOCUS_SCALE - 1.0) * (t / FOCUS_SCALE_TIME).clamp(0.0, 1.0)
}

pub fn setup_title_menu(mut commands: Commands) {
    commands.init_resource::<TitleAssets>();
    commands.spawn((
        Camera3d::default(),
        Camera {
            // Over the world, magic moment and UI while the title is up.
            order: 2,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: CAMERA_FOV.to_radians(),
            near: 0.5,
            far: CAMERA_FAR,
            ..default()
        }),
        Transform::from_xyz(0.0, 0.0, CAMERA_DISTANCE).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(TITLE_LAYER),
        TitleCamera,
    ));
}

fn spawn_scene(
    commands: &mut Commands,
    assets: &TitleAssets,
    materials: &mut Assets<StandardMaterial>,
) {
    let layer = RenderLayers::layer(TITLE_LAYER);
    let root = commands
        .spawn((
            TitleScene,
            Transform::default(),
            Visibility::default(),
            layer.clone(),
        ))
        .id();
    let mut spawn = |parts: &[MmPart], piece: TitlePiece| {
        let holder = commands
            .spawn((
                Transform::default(),
                Visibility::Hidden,
                piece,
                layer.clone(),
            ))
            .set_parent(root)
            .id();
        for part in parts {
            commands
                .spawn((
                    Mesh3d(part.mesh.clone()),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        // Blended pieces at one node sort by origin: the highlight goes in front
                        // of its button.
                        depth_bias: match piece {
                            TitlePiece::Highlight(_) => 100.0,
                            _ => 0.0,
                        },
                        ..part.material.clone()
                    })),
                    Transform::default(),
                    Visibility::Inherited,
                    piece,
                    layer.clone(),
                ))
                .set_parent(holder);
        }
    };
    spawn(&assets.rig_parts, TitlePiece::Rig);
    for (i, parts) in assets.silver.iter().enumerate() {
        spawn(parts, TitlePiece::Button(i));
    }
    for (i, parts) in assets.gold.iter().enumerate() {
        spawn(parts, TitlePiece::Highlight(i));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn title_menu_scene_system(
    mut commands: Commands,
    state: Option<Res<State<GameState>>>,
    menu: Res<TitleMenu>,
    asset_server: Option<Res<CharacterAssetServer>>,
    mut assets: ResMut<TitleAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    scenes: Query<Entity, With<TitleScene>>,
    mut cameras: Query<(&mut Camera, &mut Projection), With<TitleCamera>>,
    mut world_cams: Query<
        &mut Camera,
        (
            Or<(With<MainCamera>, With<SkyCamera>)>,
            Without<TitleCamera>,
        ),
    >,
    mut holders: Query<(&TitlePiece, &mut Transform, &mut Visibility), Without<Mesh3d>>,
    parts: Query<(&TitlePiece, &MeshMaterial3d<StandardMaterial>), With<Mesh3d>>,
) {
    let on_title = state.is_some_and(|s| *s.get() == GameState::Title);
    for (mut camera, mut projection) in &mut cameras {
        camera.is_active = on_title;
        if let Projection::Perspective(p) = projection.as_mut() {
            p.fov = vertical_fov(CAMERA_FOV.to_radians(), p.aspect_ratio);
        }
    }
    // The world cameras (and the UI on the main one) are off under the title, as in
    // portal space; `magic_moment_system` turns them back on every frame otherwise.
    if on_title {
        for mut cam in &mut world_cams {
            cam.is_active = false;
        }
    }
    if !on_title {
        for e in &scenes {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    if !assets.loaded {
        assets.load(asset_server.as_deref(), &mut meshes, &mut images);
    }
    if scenes.is_empty() {
        spawn_scene(&mut commands, &assets, &mut materials);
        return;
    }

    // "rig facing val" is the rig's heading about the up axis.
    let rig = Mat4::from_rotation_y(menu.rig_facing().to_radians())
        * Mat4::from_scale(Vec3::splat(RIG_SCALE));
    let globals = assets.joint_globals(menu.selected, menu.focus_elapsed);
    let at_node = |button: usize, scale: f32, forward: f32| {
        let node = rig * assets.node(&globals, button);
        let (_, rotation, translation) = node.to_scale_rotation_translation();
        // `heading + 180`: out from the rig, toward the camera.
        let toward_camera = translation.normalize_or_zero() * forward;
        Transform {
            translation: translation + toward_camera,
            rotation,
            scale: Vec3::splat(scale),
        }
    };
    let focus = focus_scale(menu.focus_elapsed);
    let highlight_alpha = (menu.focus_elapsed / HIGHLIGHT_FADE_TIME).clamp(0.0, 1.0);

    for (piece, mut tf, mut vis) in &mut holders {
        let (shown, at) = match *piece {
            TitlePiece::Rig => (true, Transform::from_matrix(rig)),
            TitlePiece::Button(i) => {
                let scale = BUTTON_SCALE * if i == menu.selected { focus } else { 1.0 };
                (true, at_node(i, scale, 0.0))
            }
            TitlePiece::Highlight(i) => (
                i == menu.selected,
                at_node(i, BUTTON_SCALE * focus, HIGHLIGHT_OFFSET),
            ),
        };
        *tf = at;
        *vis = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (piece, mat) in &parts {
        let TitlePiece::Highlight(_) = piece else {
            continue;
        };
        let alpha = highlight_alpha;
        if let Some(m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_rate_slides_from_125_to_25_percent() {
        assert!((bounce_time(1.0) - (1.25 - 1.0 / 18.0)).abs() < 1e-5);
        // Area under the ramp: (1.25 + 0.25) / 2 * 9.
        assert!((bounce_time(9.0) - 6.75).abs() < 1e-4);
        assert!((bounce_time(13.0) - 7.75).abs() < 1e-4);
    }

    #[test]
    fn focused_button_grows_to_120_percent_in_a_fifth_of_a_second() {
        assert_eq!(focus_scale(0.0), 1.0);
        assert!((focus_scale(0.1) - 1.1).abs() < 1e-5);
        assert_eq!(focus_scale(1.0), 1.2);
    }
}
