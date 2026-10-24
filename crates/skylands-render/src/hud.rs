use crate::camera::MainCamera;
use crate::viewer::ViewerSettings;
use bevy::prelude::*;
use skylands_core::{
    ActiveDialogue, CaptureProgress, DamagePopup, GameStats, ZoneTransitionProgress,
};

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

/// Bottom-of-screen box for NPC conversations (`ActiveDialogue`).
#[derive(Component)]
pub struct HudDialogueBox;

#[derive(Component)]
pub struct HudDialogueSpeaker;

#[derive(Component)]
pub struct HudDialogueText;

#[derive(Component)]
pub struct HudDialogueHint;

pub fn setup_dialogue_box(mut commands: Commands) {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(70.0),
                    max_width: Val::Px(820.0),
                    min_height: Val::Px(110.0),
                    padding: UiRect::axes(Val::Px(22.0), Val::Px(14.0)),
                    margin: UiRect::bottom(Val::Px(28.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.07, 0.14, 0.94)),
                BorderColor(Color::srgb(0.95, 0.75, 0.25)),
                BorderRadius::all(Val::Px(10.0)),
                Visibility::Hidden,
                HudDialogueBox,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 17.0,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.8, 0.3)),
                    HudDialogueSpeaker,
                ));
                b.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.96, 1.0)),
                    HudDialogueText,
                ));
                b.spawn((
                    Node {
                        align_self: AlignSelf::FlexEnd,
                        ..default()
                    },
                    Text::new(""),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.6, 0.75, 0.95)),
                    HudDialogueHint,
                ));
            });
        });
}

#[allow(clippy::type_complexity)]
pub fn dialogue_box_system(
    dialogue: Res<ActiveDialogue>,
    mut boxes: Query<&mut Visibility, With<HudDialogueBox>>,
    mut texts: Query<(
        &mut Text,
        Has<HudDialogueSpeaker>,
        Has<HudDialogueText>,
        Has<HudDialogueHint>,
    )>,
) {
    let line = dialogue.current_line();
    for mut vis in &mut boxes {
        *vis = if line.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let Some((speaker, body)) = line else {
        return;
    };
    let last = dialogue.line + 1 >= dialogue.lines.len();
    let hint = if last { "[A] Close" } else { "[A] Next" };
    for (mut text, is_speaker, is_body, is_hint) in &mut texts {
        let want = if is_speaker {
            speaker
        } else if is_body {
            body
        } else if is_hint {
            hint
        } else {
            continue;
        };
        if text.0 != want {
            text.0 = want.to_string();
        }
    }
}

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
    mut text_query: Query<(
        &mut Text,
        Option<&HudCaptureText>,
        Option<&HudPromptText>,
        Option<&HudViewerText>,
    )>,
    mut capture_banner: Query<
        &mut Visibility,
        (
            With<HudCaptureBanner>,
            Without<HudPromptBanner>,
            Without<HudViewerCard>,
        ),
    >,
    mut capture_fill: Query<&mut Node, (With<HudCaptureFill>, Without<HudViewerCard>)>,
    mut prompt_banner: Query<
        &mut Visibility,
        (
            With<HudPromptBanner>,
            Without<HudCaptureBanner>,
            Without<HudViewerCard>,
        ),
    >,
    mut fade_overlay: Query<&mut BackgroundColor, With<HudFadeOverlay>>,
    mut viewer_card: Query<
        &mut Visibility,
        (
            With<HudViewerCard>,
            Without<HudCaptureBanner>,
            Without<HudPromptBanner>,
        ),
    >,
) {
    for (mut text, is_capture, is_prompt, is_viewer) in &mut text_query {
        if is_capture.is_some() {
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
    fn test_dialogue_box_shows_current_line() {
        let mut app = App::new();
        app.init_resource::<ActiveDialogue>()
            .add_systems(Startup, setup_dialogue_box)
            .add_systems(Update, dialogue_box_system);
        app.update();
        let hidden = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&Visibility, With<HudDialogueBox>>();
            *q.single(app.world()) == Visibility::Hidden
        };
        assert!(hidden(&mut app));

        let npc = app.world_mut().spawn_empty().id();
        *app.world_mut().resource_mut::<ActiveDialogue>() = ActiveDialogue {
            npc: Some(npc),
            npc_name: "Flynn".into(),
            lines: vec![
                skylands_formats::DialogueLine {
                    speaker: None,
                    text: "Boom!".into(),
                },
                skylands_formats::DialogueLine {
                    speaker: Some("Mags".into()),
                    text: "Exactly.".into(),
                },
            ],
            line: 0,
        };
        app.update();
        assert!(!hidden(&mut app));
        let text = |app: &mut App, marker: fn(&World, Entity) -> bool| {
            let mut q = app.world_mut().query::<(Entity, &Text)>();
            let w = app.world();
            q.iter(w)
                .find(|(e, _)| marker(w, *e))
                .map(|(_, t)| t.0.clone())
                .unwrap()
        };
        assert_eq!(
            text(&mut app, |w, e| w.get::<HudDialogueSpeaker>(e).is_some()),
            "Flynn"
        );
        assert_eq!(
            text(&mut app, |w, e| w.get::<HudDialogueText>(e).is_some()),
            "Boom!"
        );
        assert_eq!(
            text(&mut app, |w, e| w.get::<HudDialogueHint>(e).is_some()),
            "[A] Next"
        );

        app.world_mut().resource_mut::<ActiveDialogue>().line = 1;
        app.update();
        assert_eq!(
            text(&mut app, |w, e| w.get::<HudDialogueSpeaker>(e).is_some()),
            "Mags"
        );
        assert_eq!(
            text(&mut app, |w, e| w.get::<HudDialogueHint>(e).is_some()),
            "[A] Close"
        );

        app.world_mut().resource_mut::<ActiveDialogue>().close();
        app.update();
        assert!(hidden(&mut app));
    }

    #[test]
    fn test_hud_update_prompt() {
        let mut app = App::new();
        app.insert_resource(GameStats {
            coins: 42,
            active_zone: "Courtyard".into(),
            active_prompt: Some("Press A to talk".into()),
        })
        .insert_resource(CaptureProgress::default())
        .insert_resource(ZoneTransitionProgress::default())
        .insert_resource(ViewerSettings::default());

        let prompt_entity = app.world_mut().spawn((Text::new(""), HudPromptText)).id();

        app.add_systems(Update, hud_update_system);
        app.update();

        let updated_text = app.world().get::<Text>(prompt_entity).unwrap();
        assert_eq!(updated_text.0, "Press A to talk");
    }
}
