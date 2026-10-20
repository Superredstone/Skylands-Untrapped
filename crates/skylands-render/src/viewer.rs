use bevy::prelude::*;
use skylands_core::{ActorIdentity, Hero};

pub const SHOWCASE_CHARACTERS: &[u16] = &[
    400,  // Snap Shot
    401,  // Food Fight
    402,  // Wildfire
    403,  // Wallop
    404,  // Gearshift
    405,  // Krypt King
    406,  // Enigma
    407,  // Gusto
    14,   // Gill Grunt
    16,   // Spyro
    19,   // Trigger Happy
    1010, // Kaos
    1001, // Chompy Mage
];

#[derive(Resource, Debug, Clone)]
pub struct ViewerSettings {
    pub viewer_mode: bool,
    pub current_char_id: u16,
    pub char_index: usize,
    pub turntable_rot: f32,
    pub clip_index: usize,
    pub clip_name: String,
    pub vert_count: usize,
    pub tri_count: usize,
    pub bone_count: usize,
}

impl Default for ViewerSettings {
    fn default() -> Self {
        Self {
            viewer_mode: false,
            current_char_id: 16,
            char_index: SHOWCASE_CHARACTERS
                .iter()
                .position(|&x| x == 16)
                .unwrap_or(0),
            turntable_rot: 0.0,
            clip_index: 0,
            clip_name: "idle".to_string(),
            vert_count: 1420,
            tri_count: 2480,
            bone_count: 32,
        }
    }
}

pub fn viewer_controls_system(
    time: Res<Time>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut viewer: ResMut<ViewerSettings>,
    mut hero_query: Query<(&mut Transform, &mut ActorIdentity), With<Hero>>,
) {
    if !viewer.viewer_mode {
        return;
    }

    let dt = time.delta_secs();
    viewer.turntable_rot += dt * 0.8;

    if let Ok((mut tf, _)) = hero_query.get_single_mut() {
        tf.rotation = Quat::from_rotation_y(viewer.turntable_rot);
        tf.translation = Vec3::new(0.0, 0.2, 0.0);
    }

    let Some(keyboard) = keyboard else {
        return;
    };

    if keyboard.just_pressed(KeyCode::Space) {
        viewer.clip_index = (viewer.clip_index + 1) % 3;
        let clips = ["idle", "walk", "attack"];
        viewer.clip_name = clips[viewer.clip_index].to_string();
    }

    let mut new_id = None;
    if keyboard.just_pressed(KeyCode::KeyN) {
        viewer.char_index = (viewer.char_index + 1) % SHOWCASE_CHARACTERS.len();
        new_id = Some(SHOWCASE_CHARACTERS[viewer.char_index]);
    } else if keyboard.just_pressed(KeyCode::KeyP) {
        viewer.char_index = if viewer.char_index == 0 {
            SHOWCASE_CHARACTERS.len() - 1
        } else {
            viewer.char_index - 1
        };
        new_id = Some(SHOWCASE_CHARACTERS[viewer.char_index]);
    } else if keyboard.just_pressed(KeyCode::Digit1) || keyboard.just_pressed(KeyCode::Numpad1) {
        new_id = Some(SHOWCASE_CHARACTERS[0]);
    } else if keyboard.just_pressed(KeyCode::Digit2) || keyboard.just_pressed(KeyCode::Numpad2) {
        new_id = Some(SHOWCASE_CHARACTERS[1]);
    } else if keyboard.just_pressed(KeyCode::Digit3) || keyboard.just_pressed(KeyCode::Numpad3) {
        new_id = Some(SHOWCASE_CHARACTERS[2]);
    } else if keyboard.just_pressed(KeyCode::Digit4) || keyboard.just_pressed(KeyCode::Numpad4) {
        new_id = Some(SHOWCASE_CHARACTERS[3]);
    } else if keyboard.just_pressed(KeyCode::Digit5) || keyboard.just_pressed(KeyCode::Numpad5) {
        new_id = Some(SHOWCASE_CHARACTERS[4]);
    } else if keyboard.just_pressed(KeyCode::Digit6) || keyboard.just_pressed(KeyCode::Numpad6) {
        new_id = Some(SHOWCASE_CHARACTERS[5]);
    } else if keyboard.just_pressed(KeyCode::Digit7) || keyboard.just_pressed(KeyCode::Numpad7) {
        new_id = Some(SHOWCASE_CHARACTERS[6]);
    } else if keyboard.just_pressed(KeyCode::Digit8) || keyboard.just_pressed(KeyCode::Numpad8) {
        new_id = Some(SHOWCASE_CHARACTERS[7]);
    } else if keyboard.just_pressed(KeyCode::Digit9) || keyboard.just_pressed(KeyCode::Numpad9) {
        new_id = Some(SHOWCASE_CHARACTERS[8]);
    }

    if let Some(id) = new_id {
        viewer.current_char_id = id;
        if let Some(pos) = SHOWCASE_CHARACTERS.iter().position(|&x| x == id) {
            viewer.char_index = pos;
        }
        let (name, _) = skylands_core::get_character_info(id);
        let (model_path, tex_path) = skylands_core::get_character_asset_paths(id);
        let is_villain = (1000..=2000).contains(&id);

        if let Ok((_, mut ident)) = hero_query.get_single_mut() {
            ident.character_id = id;
            ident.name = name.to_string();
            ident.is_villain = is_villain;
            ident.model_path = Some(model_path.to_string());
            ident.texture_path = tex_path.map(|s| s.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewer_settings_default_spyro() {
        let settings = ViewerSettings::default();
        assert_eq!(settings.current_char_id, 16);
        assert_eq!(
            SHOWCASE_CHARACTERS[settings.char_index], 16,
            "char_index should point to Spyro (16)"
        );
    }

    #[test]
    fn test_viewer_turntable_rotation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(ViewerSettings {
            viewer_mode: true,
            current_char_id: 16,
            ..default()
        });

        let hero = app
            .world_mut()
            .spawn((
                Hero,
                ActorIdentity {
                    id: 16,
                    character_id: 16,
                    name: "Spyro".to_string(),
                    is_villain: false,
                    model_path: None,
                    texture_path: None,
                },
                Transform::from_xyz(0.0, 0.2, 0.0),
            ))
            .id();

        app.add_systems(Update, viewer_controls_system);
        app.update();

        let viewer = app.world().resource::<ViewerSettings>();
        assert!(viewer.turntable_rot >= 0.0);
        let tf = app.world().get::<Transform>(hero).unwrap();
        assert_eq!(tf.translation, Vec3::new(0.0, 0.2, 0.0));
    }
}
