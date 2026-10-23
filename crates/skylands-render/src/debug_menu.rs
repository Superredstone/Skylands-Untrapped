use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use skylands_core::{
    ActiveLevel, ActivePortal, ActorAnimation, ActorIdentity, EnvironmentCollision, GameState,
    GameStats, Health, Hero, PortalGameplayState, Velocity, ZoneTransitionEvent,
    ZoneTransitionProgress,
};

use crate::camera::{free_cam_cursor, FreeCamState, MainCamera};
use crate::viewer::ViewerSettings;
use crate::visual_sync::CharacterAnimator;

pub const DEBUG_ROSTER: &[(u16, &str, &str, Color)] = &[
    (400, "Snap Shot", "WATER", Color::srgb(0.16, 0.50, 0.73)),
    (401, "Food Fight", "LIFE", Color::srgb(0.18, 0.80, 0.44)),
    (402, "Wildfire", "FIRE", Color::srgb(0.90, 0.30, 0.24)),
    (403, "Wallop", "EARTH", Color::srgb(0.85, 0.55, 0.13)),
    (404, "Gearshift", "TECH", Color::srgb(0.58, 0.65, 0.65)),
    (405, "Krypt King", "UNDEAD", Color::srgb(0.55, 0.27, 0.68)),
    (406, "Enigma", "MAGIC", Color::srgb(0.70, 0.30, 0.90)),
    (407, "Gusto", "AIR", Color::srgb(0.10, 0.74, 0.61)),
    (14, "Gill Grunt", "WATER", Color::srgb(0.20, 0.60, 0.86)),
    (16, "Spyro", "MAGIC", Color::srgb(0.60, 0.35, 0.71)),
    (19, "Trigger Happy", "TECH", Color::srgb(0.95, 0.77, 0.06)),
    (1010, "Kaos", "KAOS", Color::srgb(0.75, 0.10, 0.25)),
    (1001, "Chompy Mage", "LIFE", Color::srgb(0.30, 0.69, 0.31)),
    (1002, "Wolfgang", "UNDEAD", Color::srgb(0.40, 0.20, 0.60)),
    (1004, "Golden Queen", "EARTH", Color::srgb(0.85, 0.65, 0.13)),
    (
        1005,
        "Chef Pepper Jack",
        "FIRE",
        Color::srgb(0.80, 0.25, 0.15),
    ),
];

pub const DEBUG_ANIMATIONS: &[(&str, &str)] = &[
    ("idle", "IDLE"),
    ("walk", "WALK"),
    ("run", "RUN"),
    ("attack_combo_1", "ATTACK 1"),
    ("attack_sweep", "SWEEP"),
    ("attack_projectile", "PROJECTILE"),
];

pub const DEBUG_LEVELS: &[(&str, &str)] = &[
    ("Academy Hub", "Level_Hub_2014_01.bld"),
    ("Ch 1: Soda Springs", "Level_313_Tutorial.bld"),
    ("Ch 2: Know-It-All Island", "Level_300_BirdCage.bld"),
    ("Ch 3: Chompy Mountain", "Level_301_The_Machine.bld"),
    ("Ch 4: Phoenix Sanctuary", "Level_302_Chompywurm.bld"),
    ("Ch 5: Chef Zeppelin", "Level_303_DOTA.bld"),
    ("Ch 6: Rainfish Riviera", "Level_304_Water_Wave.bld"),
    ("Ch 7: Monster Marsh", "Level_305_WilikinFactory.bld"),
    ("Ch 8: Telescope Towers", "Level_306_Copter.bld"),
    ("Ch 9: Mystic Mill", "Level_307_clockworks.bld"),
    ("Ch 10: Secret Sewers", "Level_308_NiteLite.bld"),
    ("Ch 11: Wilikin Workshop", "Level_309_Sewers.bld"),
    ("Ch 12: Time Town", "Level_310_DreamLand.bld"),
    (
        "Ch 13: Future of Skylands",
        "Level_311_Arkeyan_Mechanism.bld",
    ),
    ("Ch 14: Troll Rocket Steal", "Level_312_Starlands.bld"),
    ("Ch 15: Skyhighlands", "Level_316_PirateFortress.bld"),
    ("Ch 16: Golden Desert", "Level_318_DOTAFortress.bld"),
    ("Ch 17: Golden Queen Lair", "Level_319_Pyramid.bld"),
    ("Ch 18: Ultimate Weapon", "Level_317_FinalBattle.bld"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugTab {
    #[default]
    Characters,
    Levels,
    Telemetry,
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct DebugMenuState {
    pub is_open: bool,
    pub active_tab: DebugTab,
    pub selected_char_id: u16,
    pub active_animation: String,
}

impl Default for DebugMenuState {
    fn default() -> Self {
        Self {
            is_open: false,
            active_tab: DebugTab::Characters,
            selected_char_id: 16,
            active_animation: "idle".to_string(),
        }
    }
}

#[derive(Component)]
pub struct DebugMenuRoot;

#[derive(Component)]
pub struct DebugTabContent(pub DebugTab);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum DebugMenuAction {
    Close,
    SelectTab(DebugTab),
    SelectCharacter(u16),
    PlayAnimation(&'static str),
    ChangeLevel(&'static str, &'static str),
    ToggleFreeCam,
}

#[derive(Component)]
pub struct TelemetryText;

#[derive(Component)]
pub struct FreeCamButtonText;

pub fn setup_debug_menu_ui(mut commands: Commands) {
    // Fullscreen centered backdrop overlay
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            Visibility::Hidden,
            DebugMenuRoot,
        ))
        .with_children(|root| {
            // Main Modal Window (Skylanders stone/gold aesthetic)
            root.spawn((
                Node {
                    width: Val::Px(880.0),
                    height: Val::Px(580.0),
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(2.5)),
                    padding: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.07, 0.12, 0.96)),
                BorderColor(Color::srgb(0.85, 0.68, 0.22)),
                BorderRadius::all(Val::Px(8.0)),
            ))
            .with_children(|modal| {
                // Top Header Row
                modal
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(44.0),
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        margin: UiRect::bottom(Val::Px(12.0)),
                        border: UiRect::bottom(Val::Px(1.5)),
                        padding: UiRect::bottom(Val::Px(8.0)),
                        ..default()
                    })
                    .insert(BorderColor(Color::srgba(0.85, 0.68, 0.22, 0.6)))
                    .with_children(|header| {
                        header.spawn((
                            Text::new("✦ SKYLANDS RUNTIME DEBUGGER ✦"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::srgb(1.0, 0.88, 0.35)),
                        ));

                        // Close button
                        header
                            .spawn((
                                Button,
                                Node {
                                    padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                                    border: UiRect::all(Val::Px(1.5)),
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.18, 0.08, 0.08, 0.9)),
                                BorderColor(Color::srgb(0.85, 0.3, 0.3)),
                                BorderRadius::all(Val::Px(4.0)),
                                DebugMenuAction::Close,
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("✕ CLOSE (F12)"),
                                    TextFont {
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(1.0, 0.6, 0.6)),
                                ));
                            });
                    });

                // Tab Bar
                modal
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(38.0),
                        flex_direction: FlexDirection::Row,
                        margin: UiRect::bottom(Val::Px(14.0)),
                        column_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|tabs| {
                        let tab_items = [
                            (DebugTab::Characters, "⚔ HEROES & ANIMS"),
                            (DebugTab::Levels, "🗺 LEVEL WARP"),
                            (DebugTab::Telemetry, "⚙ TELEMETRY & CAMERA"),
                        ];
                        for (tab, label) in tab_items {
                            tabs.spawn((
                                Button,
                                Node {
                                    padding: UiRect::axes(Val::Px(18.0), Val::Px(8.0)),
                                    border: UiRect::all(Val::Px(1.5)),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.10, 0.13, 0.20, 0.95)),
                                BorderColor(Color::srgb(0.85, 0.68, 0.22)),
                                BorderRadius::all(Val::Px(6.0)),
                                DebugMenuAction::SelectTab(tab),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new(label),
                                    TextFont {
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.9, 0.92, 0.98)),
                                ));
                            });
                        }
                    });

                // Content Panel 1: Characters & Animations
                modal
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            display: Display::Flex,
                            row_gap: Val::Px(10.0),
                            ..default()
                        },
                        Visibility::Inherited,
                        DebugTabContent(DebugTab::Characters),
                    ))
                    .with_children(|char_tab| {
                        char_tab.spawn((
                            Text::new("SELECT SKYLANDER HERO:"),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.75, 0.85, 1.0)),
                        ));

                        // 4-column grid of Skylander buttons
                        char_tab
                            .spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                row_gap: Val::Px(6.0),
                                column_gap: Val::Px(6.0),
                                ..default()
                            })
                            .with_children(|grid| {
                                for &(id, name, element, color) in DEBUG_ROSTER {
                                    grid.spawn((
                                        Button,
                                        Node {
                                            width: Val::Px(205.0),
                                            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                                            border: UiRect::all(Val::Px(1.2)),
                                            justify_content: JustifyContent::SpaceBetween,
                                            align_items: AlignItems::Center,
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgba(0.09, 0.12, 0.18, 0.9)),
                                        BorderColor(color),
                                        BorderRadius::all(Val::Px(4.0)),
                                        DebugMenuAction::SelectCharacter(id),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new(name),
                                            TextFont {
                                                font_size: 12.0,
                                                ..default()
                                            },
                                            TextColor(Color::WHITE),
                                        ));
                                        btn.spawn((
                                            Text::new(format!("[{element}]")),
                                            TextFont {
                                                font_size: 10.0,
                                                ..default()
                                            },
                                            TextColor(color),
                                        ));
                                    });
                                }
                            });

                        char_tab.spawn((
                            Text::new("PLAY ANIMATION SEQUENCE:"),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.75, 0.85, 1.0)),
                            Node {
                                margin: UiRect::top(Val::Px(8.0)),
                                ..default()
                            },
                        ));

                        // Animation row
                        char_tab
                            .spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(8.0),
                                ..default()
                            })
                            .with_children(|row| {
                                for &(clip, label) in DEBUG_ANIMATIONS {
                                    row.spawn((
                                        Button,
                                        Node {
                                            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                            border: UiRect::all(Val::Px(1.2)),
                                            justify_content: JustifyContent::Center,
                                            align_items: AlignItems::Center,
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgba(0.12, 0.16, 0.24, 0.95)),
                                        BorderColor(Color::srgb(0.3, 0.7, 0.9)),
                                        BorderRadius::all(Val::Px(4.0)),
                                        DebugMenuAction::PlayAnimation(clip),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new(label),
                                            TextFont {
                                                font_size: 11.0,
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.85, 0.92, 1.0)),
                                        ));
                                    });
                                }
                            });
                    });

                // Content Panel 2: Level Warp
                modal
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            display: Display::None,
                            row_gap: Val::Px(8.0),
                            ..default()
                        },
                        Visibility::Hidden,
                        DebugTabContent(DebugTab::Levels),
                    ))
                    .with_children(|level_tab| {
                        level_tab.spawn((
                            Text::new("WARP TO LEVEL / CHAPTER:"),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.75, 0.85, 1.0)),
                        ));

                        // 2-column grid of Level buttons
                        level_tab
                            .spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                row_gap: Val::Px(6.0),
                                column_gap: Val::Px(8.0),
                                ..default()
                            })
                            .with_children(|grid| {
                                for &(title, bld) in DEBUG_LEVELS {
                                    grid.spawn((
                                        Button,
                                        Node {
                                            width: Val::Px(415.0),
                                            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                            border: UiRect::all(Val::Px(1.2)),
                                            justify_content: JustifyContent::SpaceBetween,
                                            align_items: AlignItems::Center,
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgba(0.09, 0.12, 0.18, 0.9)),
                                        BorderColor(Color::srgb(0.75, 0.62, 0.25)),
                                        BorderRadius::all(Val::Px(4.0)),
                                        DebugMenuAction::ChangeLevel(title, bld),
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new(title),
                                            TextFont {
                                                font_size: 12.0,
                                                ..default()
                                            },
                                            TextColor(Color::WHITE),
                                        ));
                                        btn.spawn((
                                            Text::new(bld),
                                            TextFont {
                                                font_size: 10.0,
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.65, 0.75, 0.85)),
                                        ));
                                    });
                                }
                            });
                    });

                // Content Panel 3: Telemetry & Camera
                modal
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            display: Display::None,
                            row_gap: Val::Px(12.0),
                            ..default()
                        },
                        Visibility::Hidden,
                        DebugTabContent(DebugTab::Telemetry),
                    ))
                    .with_children(|telemetry_tab| {
                        // Free Cam button card
                        telemetry_tab
                            .spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    padding: UiRect::all(Val::Px(12.0)),
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::SpaceBetween,
                                    align_items: AlignItems::Center,
                                    border: UiRect::all(Val::Px(1.5)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.08, 0.12, 0.20, 0.92)),
                                BorderColor(Color::srgb(0.24, 0.88, 0.95)),
                                BorderRadius::all(Val::Px(6.0)),
                            ))
                            .with_children(|card| {
                                card.spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                })
                                .with_children(|left| {
                                    left.spawn((
                                        Text::new("FREE FLYING CAMERA CONTROLLER"),
                                        TextFont {
                                            font_size: 13.0,
                                            ..default()
                                        },
                                        TextColor(Color::srgb(0.24, 0.88, 0.95)),
                                    ));
                                    left.spawn((
                                        Text::new(
                                            "Controls: WASD translate, Mouse look, Space up, C down, Esc to exit.",
                                        ),
                                        TextFont {
                                            font_size: 11.0,
                                            ..default()
                                        },
                                        TextColor(Color::srgb(0.7, 0.75, 0.85)),
                                    ));
                                });

                                card.spawn((
                                    Button,
                                    Node {
                                        padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                                        border: UiRect::all(Val::Px(1.5)),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.12, 0.22, 0.35, 0.95)),
                                    BorderColor(Color::srgb(0.24, 0.88, 0.95)),
                                    BorderRadius::all(Val::Px(4.0)),
                                    DebugMenuAction::ToggleFreeCam,
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("TOGGLE FREE CAM: OFF"),
                                        TextFont {
                                            font_size: 12.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        FreeCamButtonText,
                                    ));
                                });
                            });

                        // Live Telemetry readout card
                        telemetry_tab
                            .spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Px(280.0),
                                    padding: UiRect::all(Val::Px(14.0)),
                                    border: UiRect::all(Val::Px(1.5)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.04, 0.06, 0.10, 0.92)),
                                BorderColor(Color::srgb(0.75, 0.62, 0.25)),
                                BorderRadius::all(Val::Px(6.0)),
                            ))
                            .with_children(|card| {
                                card.spawn((
                                    Text::new("TELEMETRY LOADING..."),
                                    TextFont {
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.85, 0.90, 0.96)),
                                    TelemetryText,
                                ));
                            });
                    });
            });
        });
}

#[allow(clippy::type_complexity)]
pub fn debug_menu_toggle_system(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut menu_state: ResMut<DebugMenuState>,
    free_cam: Res<FreeCamState>,
    mut root_query: Query<(&mut Visibility, &mut Node), With<DebugMenuRoot>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    if keyboard.just_pressed(KeyCode::F12) || keyboard.just_pressed(KeyCode::Backquote) {
        menu_state.is_open = !menu_state.is_open;
    }

    if menu_state.is_changed() {
        if let Ok((mut vis, mut node)) = root_query.get_single_mut() {
            if menu_state.is_open {
                *vis = Visibility::Inherited;
                node.display = Display::Flex;
            } else {
                *vis = Visibility::Hidden;
                node.display = Display::None;
            }
        }

        if let Ok(mut window) = windows.get_single_mut() {
            if menu_state.is_open {
                window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
                window.cursor_options.visible = true;
            } else {
                window.cursor_options = free_cam_cursor(free_cam.enabled);
            }
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn debug_menu_interaction_system(
    mut commands: Commands,
    mut menu_state: ResMut<DebugMenuState>,
    mut free_cam: ResMut<FreeCamState>,
    mut transition_progress: ResMut<ZoneTransitionProgress>,
    mut transition_events: EventWriter<ZoneTransitionEvent>,
    mut next_state: Option<ResMut<NextState<GameState>>>,
    mut viewer_settings: Option<ResMut<ViewerSettings>>,
    mut hero_query: Query<(Entity, &mut ActorIdentity, Option<&mut ActorAnimation>), With<Hero>>,
    mut animator_query: Query<&mut CharacterAnimator>,
    mut button_query: Query<
        (&Interaction, &DebugMenuAction, &mut BorderColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut tab_contents: Query<(&DebugTabContent, &mut Visibility, &mut Node)>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    for (interaction, action, mut border_color) in &mut button_query {
        match *interaction {
            Interaction::Hovered => {
                *border_color = BorderColor(Color::srgb(0.24, 0.88, 0.95));
            }
            Interaction::None => {
                *border_color = BorderColor(Color::srgb(0.75, 0.62, 0.25));
            }
            Interaction::Pressed => {
                *border_color = BorderColor(Color::srgb(1.0, 0.9, 0.3));
                match *action {
                    DebugMenuAction::Close => {
                        menu_state.is_open = false;
                    }
                    DebugMenuAction::SelectTab(tab) => {
                        menu_state.active_tab = tab;
                        for (content, mut vis, mut node) in &mut tab_contents {
                            if content.0 == tab {
                                *vis = Visibility::Inherited;
                                node.display = Display::Flex;
                            } else {
                                *vis = Visibility::Hidden;
                                node.display = Display::None;
                            }
                        }
                    }
                    DebugMenuAction::SelectCharacter(char_id) => {
                        menu_state.selected_char_id = char_id;
                        let (name, _elem) = skylands_core::get_character_info(char_id);
                        let (model_path, tex_path) =
                            skylands_core::get_character_asset_paths(char_id);
                        let is_villain = (1000..=2000).contains(&char_id);

                        if let Ok((entity, mut ident, _)) = hero_query.get_single_mut() {
                            ident.character_id = char_id;
                            ident.name = name.to_string();
                            ident.is_villain = is_villain;
                            ident.model_path = Some(model_path.to_string());
                            ident.texture_path = tex_path.map(|s| s.to_string());

                            // Reset animator current clip
                            commands.entity(entity).insert(ActorAnimation {
                                current_clip: Some(menu_state.active_animation.clone()),
                                time: 0.0,
                                speed: 1.0,
                            });
                        }

                        if let Some(ref mut vs) = viewer_settings {
                            vs.current_char_id = char_id;
                        }
                    }
                    DebugMenuAction::PlayAnimation(clip) => {
                        menu_state.active_animation = clip.to_string();

                        if let Ok((_, _, Some(mut anim))) = hero_query.get_single_mut() {
                            anim.current_clip = Some(clip.to_string());
                            anim.time = 0.0;
                        }

                        for mut animator in &mut animator_query {
                            animator.current_clip = clip.to_string();
                            animator.elapsed = 0.0;
                        }

                        if let Some(ref mut vs) = viewer_settings {
                            vs.clip_name = clip.to_string();
                        }
                    }
                    DebugMenuAction::ChangeLevel(zone, level_bld) => {
                        transition_progress.target_zone = zone.to_string();
                        transition_progress.target_level = Some(level_bld.to_string());
                        transition_progress.target_spawn = Vec3::ZERO;
                        transition_progress.timer = 0.0;
                        transition_progress.duration = 0.5;

                        if let Some(ref mut next) = next_state {
                            next.set(GameState::ZoneTransition);
                        }

                        transition_events.send(ZoneTransitionEvent {
                            target_zone: zone.to_string(),
                            target_level: Some(level_bld.to_string()),
                            target_spawn: Vec3::ZERO,
                        });

                        menu_state.is_open = false;
                    }
                    DebugMenuAction::ToggleFreeCam => {
                        free_cam.enabled = !free_cam.enabled;
                        if free_cam.enabled {
                            menu_state.is_open = false;
                            if let Ok(mut window) = windows.get_single_mut() {
                                window.cursor_options = free_cam_cursor(true);
                            }
                        } else if let Ok(mut window) = windows.get_single_mut() {
                            window.cursor_options = free_cam_cursor(false);
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn debug_menu_telemetry_system(
    time: Res<Time>,
    menu_state: Res<DebugMenuState>,
    free_cam: Res<FreeCamState>,
    game_state: Option<Res<State<GameState>>>,
    active_level: Option<Res<ActiveLevel>>,
    stats: Option<Res<GameStats>>,
    portal_gameplay: Option<Res<PortalGameplayState>>,
    active_portal: Option<ResMut<ActivePortal>>,
    env_collision: Option<Res<EnvironmentCollision>>,
    hero_query: Query<
        (
            &Transform,
            Option<&Velocity>,
            Option<&Health>,
            Option<&ActorIdentity>,
        ),
        With<Hero>,
    >,
    cam_query: Query<&Transform, With<MainCamera>>,
    mut text_query: Query<&mut Text, With<TelemetryText>>,
    mut free_cam_btn_text: Query<&mut Text, (With<FreeCamButtonText>, Without<TelemetryText>)>,
) {
    if !menu_state.is_open {
        return;
    }

    if let Ok(mut btn_text) = free_cam_btn_text.get_single_mut() {
        btn_text.0 = if free_cam.enabled {
            "FREE CAM: ACTIVE (CLICK TO DISABLE)".to_string()
        } else {
            "FREE CAM: OFF (CLICK TO ACTIVATE)".to_string()
        };
    }

    let Ok(mut telemetry) = text_query.get_single_mut() else {
        return;
    };

    let dt = time.delta_secs();
    let fps = if dt > 0.0 { 1.0 / dt } else { 60.0 };
    let ms = dt * 1000.0;

    let state_str = game_state
        .as_ref()
        .map(|s| format!("{:?}", s.get()))
        .unwrap_or_else(|| "N/A".to_string());
    let level_str = active_level
        .as_ref()
        .map(|l| l.name.as_str())
        .unwrap_or("Level_Hub_2014_01.bld");
    let coins = stats.as_ref().map(|s| s.coins).unwrap_or(0);
    let zone = stats
        .as_ref()
        .map(|s| s.active_zone.as_str())
        .unwrap_or("Courtyard");

    let hero_data = hero_query.get_single().ok();
    let hero_pos = hero_data
        .map(|(t, _, _, _)| t.translation)
        .unwrap_or(Vec3::ZERO);
    let hero_speed = hero_data
        .and_then(|(_, v, _, _)| v)
        .map(|v| v.0.length())
        .unwrap_or(0.0);
    let (hp_cur, hp_max) = hero_data
        .and_then(|(_, _, h, _)| h)
        .map(|h| (h.current, h.max))
        .unwrap_or((500, 500));
    let hero_name = hero_data
        .and_then(|(_, _, _, id)| id)
        .map(|id| id.name.as_str())
        .unwrap_or("Spyro");

    let cam_tf = cam_query.get_single().ok();
    let cam_pos = cam_tf.map(|t| t.translation).unwrap_or(Vec3::ZERO);

    let portal_type = match active_portal.as_deref() {
        Some(ActivePortal::Usb(_)) => "USB Traptanium Hardware (VID:0x1430 PID:0x0150)",
        Some(ActivePortal::Mock(_)) => "Mock Traptanium Virtual Simulator",
        _ => "None",
    };

    let tags_count = portal_gameplay.as_ref().map(|p| p.tags.len()).unwrap_or(0);
    let active_hero_id = portal_gameplay
        .as_ref()
        .and_then(|p| p.active_hero_character_id)
        .unwrap_or(menu_state.selected_char_id);
    let (active_hero_name, active_hero_elem) = skylands_core::get_character_info(active_hero_id);
    let villain_id = portal_gameplay.as_ref().and_then(|p| p.active_villain_id);
    let villain_name = villain_id
        .map(|id| skylands_core::get_character_info(id).0)
        .unwrap_or("None");

    let col_tris = env_collision
        .as_ref()
        .map(|c| c.mesh.triangles.len())
        .unwrap_or(0);

    telemetry.0 = format!(
        "═ PERFORMANCE & RUNTIME ════════════════════════════════════════════════════\n\
         FPS: {:>5.1} ({:.2} ms)  |  Tick Rate: 60Hz Fixed  |  Game State: {}\n\
         Active Level: {}  |  Active Zone: {}  |  Collision Triangles: {}\n\n\
         ═ HERO ACTOR ═══════════════════════════════════════════════════════════════\n\
         Name: {} (ID: {})  |  Element: {:?}\n\
         Position: X: {:>7.2}, Y: {:>7.2}, Z: {:>7.2}  |  Speed: {:>5.2} m/s\n\
         Health: {} / {} HP  |  Coins: {}  |  Animation: {}\n\n\
         ═ CAMERA & VIEW ════════════════════════════════════════════════════════════\n\
         Eye: X: {:>7.2}, Y: {:>7.2}, Z: {:>7.2}\n\
         Free Cam Mode: {}  |  Speed: {:.1}  |  Yaw: {:>6.2}°  |  Pitch: {:>6.2}°\n\n\
         ═ TRAPTANIUM PORTAL ════════════════════════════════════════════════════════\n\
         Device: {}\n\
         Tags on Portal: {}  |  Active Hero: {}  |  Trapped Villain: {}\n",
        fps,
        ms,
        state_str,
        level_str,
        zone,
        col_tris,
        hero_name,
        active_hero_id,
        active_hero_elem,
        hero_pos.x,
        hero_pos.y,
        hero_pos.z,
        hero_speed,
        hp_cur,
        hp_max,
        coins,
        menu_state.active_animation.to_uppercase(),
        cam_pos.x,
        cam_pos.y,
        cam_pos.z,
        if free_cam.enabled {
            "ACTIVE"
        } else {
            "INACTIVE"
        },
        free_cam.speed,
        free_cam.yaw.to_degrees(),
        free_cam.pitch.to_degrees(),
        portal_type,
        tags_count,
        active_hero_name,
        villain_name,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_menu_state_defaults() {
        let state = DebugMenuState::default();
        assert!(!state.is_open);
        assert_eq!(state.active_tab, DebugTab::Characters);
        assert_eq!(state.selected_char_id, 16);
    }

    #[test]
    fn test_debug_menu_toggle() {
        let mut app = App::new();
        app.init_resource::<DebugMenuState>()
            .init_resource::<FreeCamState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, debug_menu_toggle_system);

        let win = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();

        // Spawn root UI
        app.world_mut()
            .spawn((Node::default(), Visibility::Hidden, DebugMenuRoot));

        // Press F12
        let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        kb.press(KeyCode::F12);
        app.update();

        assert!(app.world().resource::<DebugMenuState>().is_open);
        let opts = &app.world().get::<Window>(win).unwrap().cursor_options;
        assert!(opts.visible);

        // Press F12 again to close
        let mut kb = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        kb.reset_all();
        kb.press(KeyCode::F12);
        app.update();

        assert!(!app.world().resource::<DebugMenuState>().is_open);
    }

    #[test]
    fn test_debug_menu_select_character() {
        let mut app = App::new();
        app.init_resource::<DebugMenuState>()
            .init_resource::<FreeCamState>()
            .init_resource::<ZoneTransitionProgress>()
            .add_event::<ZoneTransitionEvent>()
            .add_systems(Update, debug_menu_interaction_system);

        let hero = app
            .world_mut()
            .spawn((
                Hero,
                ActorIdentity {
                    id: 1,
                    character_id: 16,
                    name: "Spyro".to_string(),
                    is_villain: false,
                    model_path: None,
                    texture_path: None,
                },
                ActorAnimation::default(),
            ))
            .id();

        // Spawn a character select button clicked
        app.world_mut().spawn((
            Button,
            Interaction::Pressed,
            DebugMenuAction::SelectCharacter(400), // Snap Shot
            BorderColor::default(),
        ));

        app.update();

        assert_eq!(
            app.world().resource::<DebugMenuState>().selected_char_id,
            400
        );
        let ident = app.world().get::<ActorIdentity>(hero).unwrap();
        assert_eq!(ident.character_id, 400);
        assert_eq!(ident.name, "Snap Shot");
    }

    #[test]
    fn test_debug_menu_play_animation() {
        let mut app = App::new();
        app.init_resource::<DebugMenuState>()
            .init_resource::<FreeCamState>()
            .init_resource::<ZoneTransitionProgress>()
            .add_event::<ZoneTransitionEvent>()
            .add_systems(Update, debug_menu_interaction_system);

        let hero = app
            .world_mut()
            .spawn((
                Hero,
                ActorIdentity {
                    id: 1,
                    character_id: 16,
                    name: "Spyro".to_string(),
                    is_villain: false,
                    model_path: None,
                    texture_path: None,
                },
                ActorAnimation {
                    current_clip: Some("idle".to_string()),
                    time: 0.0,
                    speed: 1.0,
                },
            ))
            .id();

        app.world_mut().spawn((
            Button,
            Interaction::Pressed,
            DebugMenuAction::PlayAnimation("attack_combo_1"),
            BorderColor::default(),
        ));

        app.update();

        assert_eq!(
            app.world().resource::<DebugMenuState>().active_animation,
            "attack_combo_1"
        );
        let anim = app.world().get::<ActorAnimation>(hero).unwrap();
        assert_eq!(anim.current_clip.as_deref(), Some("attack_combo_1"));
    }

    #[test]
    fn test_debug_menu_change_level() {
        let mut app = App::new();
        app.init_resource::<DebugMenuState>()
            .init_resource::<FreeCamState>()
            .init_resource::<ZoneTransitionProgress>()
            .add_event::<ZoneTransitionEvent>()
            .add_systems(Update, debug_menu_interaction_system);

        app.world_mut().spawn((
            Button,
            Interaction::Pressed,
            DebugMenuAction::ChangeLevel("Soda Springs", "Level_313_Tutorial.bld"),
            BorderColor::default(),
        ));

        app.update();

        let progress = app.world().resource::<ZoneTransitionProgress>();
        assert_eq!(progress.target_zone, "Soda Springs");
        assert_eq!(
            progress.target_level.as_deref(),
            Some("Level_313_Tutorial.bld")
        );
    }

    #[test]
    fn test_debug_menu_toggle_free_cam() {
        let mut app = App::new();
        app.init_resource::<DebugMenuState>()
            .init_resource::<FreeCamState>()
            .init_resource::<ZoneTransitionProgress>()
            .add_event::<ZoneTransitionEvent>()
            .add_systems(Update, debug_menu_interaction_system);

        app.world_mut().spawn((Window::default(), PrimaryWindow));

        app.world_mut().resource_mut::<DebugMenuState>().is_open = true;

        app.world_mut().spawn((
            Button,
            Interaction::Pressed,
            DebugMenuAction::ToggleFreeCam,
            BorderColor::default(),
        ));

        app.update();

        assert!(app.world().resource::<FreeCamState>().enabled);
        assert!(!app.world().resource::<DebugMenuState>().is_open);
    }
}
