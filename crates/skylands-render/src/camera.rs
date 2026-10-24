use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::camera::ClearColorConfig;
use bevy::render::view::RenderLayers;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use skylands_core::{GameState, Hero};

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MainCamera;

/// Render layer of the level sky (`LevelSkyMesh`).
pub const SKY_LAYER: usize = 1;

/// Draws the sky layer before the main camera, from the origin with the main
/// camera's rotation, so the sky stays around the player like the game's skydome.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SkyCamera;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct FreeCamState {
    pub enabled: bool,
    pub speed: f32,
    pub sensitivity: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for FreeCamState {
    fn default() -> Self {
        Self {
            enabled: false,
            speed: 15.0,
            sensitivity: 0.003,
            yaw: 0.0,
            pitch: -0.4,
        }
    }
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct OrbitCameraState {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub active: bool,
}

impl Default for OrbitCameraState {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 0.8, 0.0),
            distance: 4.0,
            yaw: 0.0,
            pitch: 0.25,
            min_distance: 1.0,
            max_distance: 15.0,
            active: false,
        }
    }
}

impl OrbitCameraState {
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-1.4, 1.4);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance + delta).clamp(self.min_distance, self.max_distance);
    }
}

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            // The sky camera has already cleared and drawn the background.
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Transform::from_xyz(0.0, 15.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
        MainCamera,
    ));
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1,
            ..default()
        },
        // Sky layers span tens of thousands of units.
        Projection::Perspective(PerspectiveProjection {
            near: 1.0,
            far: 200_000.0,
            ..default()
        }),
        RenderLayers::layer(SKY_LAYER),
        SkyCamera,
    ));
}

pub fn sky_camera_sync_system(
    main: Query<&Transform, (With<MainCamera>, Without<SkyCamera>)>,
    mut sky: Query<&mut Transform, With<SkyCamera>>,
) {
    let Ok(main) = main.get_single() else {
        return;
    };
    for mut tf in &mut sky {
        *tf = Transform::from_rotation(main.rotation);
    }
}

pub fn setup_lighting(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.95, 0.85),
            illuminance: 12000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.6, 0.0)),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.7, 0.8, 1.0),
        brightness: 400.0,
    });
}

pub fn camera_follow_system(
    time: Res<Time>,
    state: Option<Res<State<GameState>>>,
    free_cam: Res<FreeCamState>,
    orbit_cam: Res<OrbitCameraState>,
    hero_query: Query<&Transform, (With<Hero>, Without<MainCamera>)>,
    mut cam_query: Query<&mut Transform, With<MainCamera>>,
) {
    if free_cam.enabled || orbit_cam.active {
        return;
    }

    let Ok(mut cam_tf) = cam_query.get_single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    let is_capture = state.is_some_and(|s| *s.get() == GameState::VillainCapture);

    if is_capture {
        // Portal Cinematic camera volume: close up on pedestal
        let target_eye = Vec3::new(2.5, 2.0, 3.5);
        let look_at = Vec3::new(2.5, 0.8, 0.0);
        let target_tf = Transform::from_translation(target_eye).looking_at(look_at, Vec3::Y);
        let factor = (dt * 5.0).clamp(0.0, 1.0);
        cam_tf.translation = cam_tf.translation.lerp(target_tf.translation, factor);
        cam_tf.rotation = cam_tf.rotation.slerp(target_tf.rotation, factor);
    } else if let Ok(hero_tf) = hero_query.get_single() {
        // Courtyard Overhead camera volume
        let target_eye = hero_tf.translation + Vec3::new(0.0, 12.0, 16.0);
        let look_at = hero_tf.translation + Vec3::new(0.0, 1.0, 0.0);
        let target_tf = Transform::from_translation(target_eye).looking_at(look_at, Vec3::Y);
        let factor = (dt * 4.0).clamp(0.0, 1.0);
        cam_tf.translation = cam_tf.translation.lerp(target_tf.translation, factor);
        cam_tf.rotation = cam_tf.rotation.slerp(target_tf.rotation, factor);
    }
}

pub fn free_cam_cursor(captured: bool) -> CursorOptions {
    let grab_mode = match (captured, cfg!(target_os = "macos")) {
        (false, _) => CursorGrabMode::None,
        (true, true) => CursorGrabMode::Locked,
        (true, false) => CursorGrabMode::Confined,
    };
    CursorOptions {
        grab_mode,
        visible: !captured,
        ..default()
    }
}

pub fn free_cam_system(
    time: Res<Time>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut free_cam: ResMut<FreeCamState>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut cam_query: Query<&mut Transform, With<MainCamera>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    if free_cam.enabled && keyboard.just_pressed(KeyCode::Escape) {
        free_cam.enabled = false;
        if let Ok(mut window) = windows.get_single_mut() {
            window.cursor_options = free_cam_cursor(false);
        }
    }

    if !free_cam.enabled {
        return;
    }

    let Ok(mut cam_tf) = cam_query.get_single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    for ev in mouse_motion.read() {
        free_cam.yaw -= ev.delta.x * free_cam.sensitivity;
        free_cam.pitch = (free_cam.pitch - ev.delta.y * free_cam.sensitivity).clamp(-1.5, 1.5);
    }

    cam_tf.rotation = Quat::from_euler(EulerRot::YXZ, free_cam.yaw, free_cam.pitch, 0.0);

    let mut move_dir = Vec3::ZERO;
    let forward = cam_tf.forward();
    let right = cam_tf.right();
    let up = Vec3::Y;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        move_dir += *forward;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        move_dir -= *forward;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        move_dir -= *right;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        move_dir += *right;
    }
    if keyboard.pressed(KeyCode::Space) {
        move_dir += up;
    }
    if keyboard.pressed(KeyCode::ShiftLeft) {
        move_dir -= up;
    }

    if move_dir.length_squared() > 1e-4 {
        cam_tf.translation += move_dir.normalize() * free_cam.speed * dt;
    }
}

pub fn orbit_cam_system(
    mut orbit_cam: ResMut<OrbitCameraState>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    mut cam_query: Query<&mut Transform, With<MainCamera>>,
) {
    if !orbit_cam.active {
        return;
    }

    if let Some(buttons) = mouse_buttons {
        if buttons.pressed(MouseButton::Left) {
            for ev in mouse_motion.read() {
                orbit_cam.orbit(ev.delta.x * 0.008, ev.delta.y * 0.008);
            }
        }
    }

    for ev in mouse_wheel.read() {
        orbit_cam.zoom(-ev.y * 0.5);
    }

    let Ok(mut cam_tf) = cam_query.get_single_mut() else {
        return;
    };

    let cos_p = orbit_cam.pitch.cos();
    let sin_p = orbit_cam.pitch.sin();
    let cos_y = orbit_cam.yaw.cos();
    let sin_y = orbit_cam.yaw.sin();

    let eye = orbit_cam.target
        + Vec3::new(
            orbit_cam.distance * cos_p * sin_y,
            orbit_cam.distance * sin_p,
            orbit_cam.distance * cos_p * cos_y,
        );

    *cam_tf = Transform::from_translation(eye).looking_at(orbit_cam.target, Vec3::Y);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sky_camera_takes_rotation_not_position() {
        let mut app = App::new();
        app.add_systems(Update, sky_camera_sync_system);
        let rotation = Quat::from_rotation_y(0.7);
        app.world_mut().spawn((
            Transform::from_xyz(100.0, 20.0, -50.0).with_rotation(rotation),
            MainCamera,
        ));
        let sky = app
            .world_mut()
            .spawn((Transform::default(), SkyCamera))
            .id();
        app.update();
        let tf = app.world().get::<Transform>(sky).unwrap();
        assert_eq!(tf.translation, Vec3::ZERO);
        assert!(tf.rotation.angle_between(rotation) < 1e-5);
    }

    #[test]
    fn test_orbit_camera_state() {
        let mut orbit = OrbitCameraState::default();
        orbit.orbit(0.5, 0.2);
        assert!((orbit.yaw - 0.5).abs() < 1e-4);
        assert!((orbit.pitch - (0.25 + 0.2)).abs() < 1e-4);

        let initial_dist = orbit.distance;
        orbit.zoom(-1.0);
        assert_eq!(orbit.distance, initial_dist - 1.0);

        orbit.zoom(-50.0);
        assert_eq!(orbit.distance, orbit.min_distance);
    }

    #[test]
    fn test_free_cam_state() {
        let free_cam = FreeCamState::default();
        assert!(!free_cam.enabled);
        assert_eq!(free_cam.speed, 15.0);
    }

    #[test]
    fn test_free_cam_escape_releases_cursor() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<FreeCamState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_event::<MouseMotion>()
            .add_systems(Update, free_cam_system);
        let win = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();

        // Enable free cam and set locked cursor
        app.world_mut().resource_mut::<FreeCamState>().enabled = true;
        app.world_mut()
            .get_mut::<Window>(win)
            .unwrap()
            .cursor_options = free_cam_cursor(true);
        app.update();

        let opts = &app.world().get::<Window>(win).unwrap().cursor_options;
        assert_ne!(opts.grab_mode, CursorGrabMode::None);
        assert!(!opts.visible);

        // Press Escape to exit free cam
        let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        kb.press(KeyCode::Escape);
        app.update();

        assert!(!app.world().resource::<FreeCamState>().enabled);
        let opts = &app.world().get::<Window>(win).unwrap().cursor_options;
        assert_eq!(opts.grab_mode, CursorGrabMode::None);
        assert!(opts.visible);
    }
}
