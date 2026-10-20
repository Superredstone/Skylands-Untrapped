use crate::camera::MainCamera;
use crate::viewer::ViewerSettings;
use bevy::prelude::*;
use skylands_core::{
    CaptureProgress, DamagePopup, GameStats, Health, Hero, ZoneTransitionProgress,
};

#[derive(Component)]
pub struct HudHeroNameText;

#[derive(Component)]
pub struct HudHealthBarFill;

#[derive(Component)]
pub struct HudCoinText;

#[derive(Component)]
pub struct HudCaptureBanner;

#[derive(Component)]
pub struct HudCaptureText;

#[derive(Component)]
pub struct HudCaptureFill;

#[derive(Component)]
pub struct HudPromptBanner;

#[derive(Component)]
pub struct HudPromptText;

#[derive(Component)]
pub struct HudFadeOverlay;

#[derive(Component)]
pub struct HudViewerCard;

#[derive(Component)]
pub struct HudViewerText;

pub fn setup_hud_ui(mut commands: Commands) {
    // Root container
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|parent| {
            // Top Row: Hero Info & Coins
            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(80.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|top| {
                    // Hero card
                    top.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                            flex_direction: FlexDirection::Column,
                            border: UiRect::all(Val::Px(1.5)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.88)),
                        BorderColor(Color::srgba(0.2, 0.6, 1.0, 0.9)),
                        BorderRadius::all(Val::Px(6.0)),
                    ))
                    .with_children(|card| {
                        card.spawn((
                            Text::new("SNAP SHOT (WATER)"),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            HudHeroNameText,
                        ));

                        // Health bar container
                        card.spawn((
                            Node {
                                width: Val::Px(180.0),
                                height: Val::Px(10.0),
                                margin: UiRect::top(Val::Px(6.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 0.8)),
                            BorderRadius::all(Val::Px(3.0)),
                        ))
                        .with_children(|bar| {
                            bar.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.2, 0.9, 0.3)),
                                BorderRadius::all(Val::Px(3.0)),
                                HudHealthBarFill,
                            ));
                        });
                    });

                    // Coin counter pill
                    top.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.5)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.88)),
                        BorderColor(Color::srgb(1.0, 0.84, 0.0)),
                        BorderRadius::all(Val::Px(16.0)),
                    ))
                    .with_children(|pill| {
                        pill.spawn((
                            Text::new("COINS: 0"),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgb(1.0, 0.88, 0.2)),
                            HudCoinText,
                        ));
                    });
                });

            // Center area: Villain capture banner & Prompt banner
            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|center| {
                    // Villain Capture banner
                    center
                        .spawn((
                            Node {
                                width: Val::Px(420.0),
                                padding: UiRect::all(Val::Px(12.0)),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                margin: UiRect::bottom(Val::Px(10.0)),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.12, 0.04, 0.04, 0.92)),
                            BorderColor(Color::srgb(0.9, 0.2, 0.2)),
                            BorderRadius::all(Val::Px(8.0)),
                            Visibility::Hidden,
                            HudCaptureBanner,
                        ))
                        .with_children(|banner| {
                            banner.spawn((
                                Text::new("TRAPPING VILLAIN: KAOS (0%)"),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.4, 0.4)),
                                HudCaptureText,
                            ));

                            // Progress bar
                            banner
                                .spawn((
                                    Node {
                                        width: Val::Px(380.0),
                                        height: Val::Px(12.0),
                                        margin: UiRect::top(Val::Px(6.0)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.2, 0.1, 0.1)),
                                    BorderRadius::all(Val::Px(4.0)),
                                ))
                                .with_children(|bar| {
                                    bar.spawn((
                                        Node {
                                            width: Val::Percent(0.0),
                                            height: Val::Percent(100.0),
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgb(1.0, 0.3, 0.1)),
                                        BorderRadius::all(Val::Px(4.0)),
                                        HudCaptureFill,
                                    ));
                                });
                        });

                    // Prompt banner
                    center
                        .spawn((
                            Node {
                                padding: UiRect::axes(Val::Px(20.0), Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                margin: UiRect::bottom(Val::Px(20.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.05, 0.1, 0.18, 0.9)),
                            BorderColor(Color::srgb(0.2, 0.7, 1.0)),
                            BorderRadius::all(Val::Px(6.0)),
                            Visibility::Hidden,
                            HudPromptBanner,
                        ))
                        .with_children(|prompt| {
                            prompt.spawn((
                                Text::new("Press A to interact"),
                                TextFont {
                                    font_size: 13.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                                HudPromptText,
                            ));
                        });
                });

            // Viewer Stats Card on bottom left
            parent
                .spawn((
                    Node {
                        width: Val::Px(360.0),
                        padding: UiRect::all(Val::Px(12.0)),
                        flex_direction: FlexDirection::Column,
                        margin: UiRect::all(Val::Px(16.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.1, 0.15, 0.92)),
                    BorderColor(Color::srgb(0.3, 0.6, 0.9)),
                    BorderRadius::all(Val::Px(6.0)),
                    Visibility::Hidden,
                    HudViewerCard,
                ))
                .with_children(|card| {
                    card.spawn((
                        Text::new(
                            "SHOWCASE VIEWER\nVERTICES:  1420\nTRIANGLES: 2480\nBONES:     32\nANIMATION: IDLE\n[1-9 / N / P]: SWITCH   [SPACE]: ANIM",
                        ),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.9, 0.92, 0.95)),
                        HudViewerText,
                    ));
                });
        });

    // Fade overlay quad covering entire screen
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        HudFadeOverlay,
    ));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn hud_update_system(
    stats: Res<GameStats>,
    capture_progress: Res<CaptureProgress>,
    transition_progress: Res<ZoneTransitionProgress>,
    viewer_settings: Res<ViewerSettings>,
    hero_query: Query<(&Health, &skylands_core::ActorIdentity), With<Hero>>,
    mut text_query: Query<(
        &mut Text,
        Option<&HudHeroNameText>,
        Option<&HudCoinText>,
        Option<&HudCaptureText>,
        Option<&HudPromptText>,
        Option<&HudViewerText>,
    )>,
    mut health_fill: Query<
        (&mut Node, &mut BackgroundColor),
        (
            With<HudHealthBarFill>,
            Without<HudCaptureFill>,
            Without<HudFadeOverlay>,
        ),
    >,
    mut capture_banner: Query<
        &mut Visibility,
        (
            With<HudCaptureBanner>,
            Without<HudPromptBanner>,
            Without<HudViewerCard>,
        ),
    >,
    mut capture_fill: Query<
        &mut Node,
        (
            With<HudCaptureFill>,
            Without<HudHealthBarFill>,
            Without<HudViewerCard>,
        ),
    >,
    mut prompt_banner: Query<
        &mut Visibility,
        (
            With<HudPromptBanner>,
            Without<HudCaptureBanner>,
            Without<HudViewerCard>,
        ),
    >,
    mut fade_overlay: Query<
        &mut BackgroundColor,
        (With<HudFadeOverlay>, Without<HudHealthBarFill>),
    >,
    mut viewer_card: Query<
        &mut Visibility,
        (
            With<HudViewerCard>,
            Without<HudCaptureBanner>,
            Without<HudPromptBanner>,
        ),
    >,
) {
    let hero_info = hero_query.get_single().ok();

    if let Some((health, _)) = hero_info {
        if let Ok((mut bar_node, mut bar_color)) = health_fill.get_single_mut() {
            let pct = (health.current as f32 / health.max.max(1) as f32).clamp(0.0, 1.0);
            bar_node.width = Val::Percent(pct * 100.0);

            *bar_color = if pct > 0.5 {
                BackgroundColor(Color::srgb(0.2, 0.85, 0.3))
            } else if pct > 0.25 {
                BackgroundColor(Color::srgb(0.9, 0.8, 0.1))
            } else {
                BackgroundColor(Color::srgb(0.9, 0.2, 0.2))
            };
        }
    }

    for (mut text, is_hero, is_coin, is_capture, is_prompt, is_viewer) in &mut text_query {
        if is_hero.is_some() {
            if let Some((_, ident)) = hero_info {
                let (_, elem) = skylands_core::get_character_info(ident.character_id);
                text.0 = format!(
                    "{} ({})",
                    ident.name.to_uppercase(),
                    elem.name().to_uppercase()
                );
            }
        } else if is_coin.is_some() {
            text.0 = format!("COINS: {}", stats.coins);
        } else if is_capture.is_some() {
            let pct = (capture_progress.progress * 100.0) as u32;
            text.0 = format!("TRAPPING VILLAIN ({}%)", pct);
        } else if is_prompt.is_some() {
            if let Some(ref prompt) = stats.active_prompt {
                text.0 = prompt.clone();
            }
        } else if is_viewer.is_some() {
            let (name, elem) = skylands_core::get_character_info(viewer_settings.current_char_id);
            text.0 = format!(
                "SHOWCASE VIEWER: {} ({})\nVERTICES:  {}\nTRIANGLES: {}\nBONES:     {}\nANIMATION: {}\n[1-9 / N / P]: SWITCH   [SPACE]: ANIM",
                name.to_uppercase(),
                elem.name().to_uppercase(),
                viewer_settings.vert_count,
                viewer_settings.tri_count,
                viewer_settings.bone_count,
                viewer_settings.clip_name.to_uppercase(),
            );
        }
    }

    // Capture Banner
    if let Ok(mut vis) = capture_banner.get_single_mut() {
        if capture_progress.progress > 0.0 && capture_progress.progress < 1.0 {
            *vis = Visibility::Inherited;
            if let Ok(mut fill_node) = capture_fill.get_single_mut() {
                fill_node.width = Val::Percent(capture_progress.progress * 100.0);
            }
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // Interaction Prompt Banner
    if let Ok(mut vis) = prompt_banner.get_single_mut() {
        if stats.active_prompt.is_some() {
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // Zone Transition Fade Overlay
    if let Ok(mut bg) = fade_overlay.get_single_mut() {
        if transition_progress.duration > 0.0
            && transition_progress.timer < transition_progress.duration
        {
            let alpha = (transition_progress.timer / transition_progress.duration).clamp(0.0, 1.0);
            *bg = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, alpha));
        } else {
            *bg = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0));
        }
    }

    // Viewer Mode Stats Card
    if let Ok(mut vis) = viewer_card.get_single_mut() {
        if viewer_settings.viewer_mode {
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

pub fn damage_popup_ui_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &mut DamagePopup)>,
    _camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let dt = time.delta_secs();

    for (entity, mut tf, mut popup) in &mut query {
        popup.lifetime += dt;
        tf.translation.y += dt * 1.5;

        if popup.lifetime >= popup.max_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hud_update_coins() {
        let mut app = App::new();
        app.insert_resource(GameStats {
            coins: 42,
            active_zone: "Courtyard".into(),
            active_prompt: None,
        })
        .insert_resource(CaptureProgress::default())
        .insert_resource(ZoneTransitionProgress::default())
        .insert_resource(ViewerSettings::default());

        let coin_entity = app
            .world_mut()
            .spawn((Text::new("COINS: 0"), HudCoinText))
            .id();

        app.add_systems(Update, hud_update_system);
        app.update();

        let updated_text = app.world().get::<Text>(coin_entity).unwrap();
        assert_eq!(updated_text.0, "COINS: 42");
    }
}
