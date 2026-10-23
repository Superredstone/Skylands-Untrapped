use crate::components::*;
use crate::events::*;
use crate::portal_gameplay::{get_character_asset_paths, get_character_info};
use crate::resources::*;
use crate::states::GameState;
use bevy::prelude::*;
use skylands_portal::protocol::PortalZone;
use skylands_portal::SkylanderTag;

pub fn poll_portal_system(
    mut portal: ResMut<ActivePortal>,
    mut portal_events: EventWriter<PortalEvent>,
) {
    let events = portal.poll_events();
    for ev in events {
        portal_events.send(ev);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn portal_gameplay_system(
    mut commands: Commands,
    mut portal: ResMut<ActivePortal>,
    mut gameplay_state: ResMut<PortalGameplayState>,
    mut portal_events: EventReader<PortalEvent>,
    mut next_state: ResMut<NextState<GameState>>,
    mut capture_progress: ResMut<CaptureProgress>,
    mut audio_streaming: ResMut<PortalAudioStreaming>,
    actors: Query<(Entity, &ActorIdentity)>,
    env: Option<Res<EnvironmentCollision>>,
    asset_server: Option<Res<GameAssetServer>>,
) {
    let spawn_origin = env.map(|e| e.spawn_point).unwrap_or(Vec3::ZERO);
    for event in portal_events.read() {
        match event {
            PortalEvent::TagArrived { tag_index, zone } => {
                let is_trap = *zone == PortalZone::TrapSlot;
                let tag_char_id = match &*portal {
                    ActivePortal::Mock(mock) => {
                        mock.loaded_tags.get(tag_index).and_then(|blocks| {
                            blocks.get(1).map(|b1| {
                                if is_trap {
                                    u16::from_le_bytes([b1[2], b1[3]])
                                } else {
                                    u16::from_le_bytes([b1[0], b1[1]])
                                }
                            })
                        })
                    }
                    _ => None,
                };
                let char_id = if is_trap {
                    tag_char_id
                        .or(gameplay_state.active_villain_id)
                        .unwrap_or(1010)
                } else {
                    tag_char_id
                        .or(gameplay_state.active_hero_character_id)
                        .unwrap_or(16)
                };
                let (name, elem) = get_character_info(char_id);
                let (model_path, tex_path) = get_character_asset_paths(char_id);

                if is_trap {
                    gameplay_state.active_villain_id = Some(char_id);
                    capture_progress.villain_id = char_id;
                    capture_progress.progress = 0.0;
                    next_state.set(GameState::VillainCapture);

                    let (r, g, b) = elem.rgb();
                    portal.set_trap_light(r, g, b);

                    audio_streaming.sink.queue_villain_speech(char_id, 1.5);
                    while let Some(pkt) = audio_streaming.sink.pop_packet() {
                        audio_streaming.queue.push_back(pkt);
                    }

                    // Spawn villain entity
                    let villain_pos = spawn_origin + Vec3::new(2.5, 0.2, 0.0);
                    commands.spawn((
                        ActorIdentity {
                            id: 1000 + char_id as u32,
                            character_id: char_id,
                            name: name.to_string(),
                            is_villain: true,
                            model_path: Some(model_path.to_string()),
                            texture_path: tex_path.map(|s| s.to_string()),
                        },
                        Villain,
                        Transform::from_translation(villain_pos),
                        CharacterCollider {
                            radius: 0.6,
                            height: 1.8,
                            step_height: 0.35,
                            grounded: true,
                        },
                        Velocity::default(),
                        MovementSpeed::default(),
                        Health::default(),
                        ActorAnimation::default(),
                    ));
                } else {
                    gameplay_state.active_hero_character_id = Some(char_id);
                    let (r, g, b) = elem.rgb();
                    portal.set_color(*zone, r, g, b);

                    // Despawn existing heroes first
                    for (entity, ident) in &actors {
                        if !ident.is_villain {
                            commands.entity(entity).despawn();
                        }
                    }

                    let damage = asset_server
                        .as_ref()
                        .and_then(|s| s.loader.lock().ok()?.load_igb(model_path).ok())
                        .and_then(|m| AttackDamage::from_model(&m))
                        .unwrap_or_default();

                    // Spawn hero entity
                    let hero_pos = spawn_origin + Vec3::new(0.0, 0.2, 0.0);
                    commands.spawn((
                        ActorIdentity {
                            id: char_id as u32,
                            character_id: char_id,
                            name: name.to_string(),
                            is_villain: false,
                            model_path: Some(model_path.to_string()),
                            texture_path: tex_path.map(|s| s.to_string()),
                        },
                        Hero,
                        Transform::from_translation(hero_pos),
                        CharacterCollider {
                            radius: 0.5,
                            height: 1.8,
                            step_height: 0.35,
                            grounded: true,
                        },
                        Velocity::default(),
                        MovementSpeed(5.0),
                        Health::default(),
                        AttackCombo::default(),
                        damage,
                        ActorAnimation::default(),
                    ));
                }

                let tag = SkylanderTag {
                    uid: [*tag_index, 0, 0, 0],
                    character_id: char_id,
                    variant: 0,
                    nickname: name.to_string(),
                    xp: 0,
                    money: 0,
                    is_trap,
                    villain_id: if is_trap { Some(char_id) } else { None },
                };
                gameplay_state.tags.push(tag);
            }
            PortalEvent::TagRemoved { tag_index: _ } => {
                portal.set_color(PortalZone::All, 0, 0, 0);
                portal.set_trap_light(0, 0, 0);
                for (entity, ident) in &actors {
                    if ident.is_villain {
                        commands.entity(entity).despawn();
                    }
                }
                gameplay_state.active_villain_id = None;
                next_state.set(GameState::Hub);
            }
            _ => {}
        }
    }
}

pub fn input_mapping_system(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut input_state: ResMut<WiiInputState>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    let mut stick_x = 0.0f32;
    let mut stick_y = 0.0f32;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        stick_y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        stick_y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        stick_x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        stick_x += 1.0;
    }

    input_state.set_stick(stick_x, stick_y);

    input_state.set_button(WiiButton::A, keyboard.pressed(KeyCode::Space));
    input_state.set_button(
        WiiButton::B,
        keyboard.pressed(KeyCode::KeyJ) || keyboard.pressed(KeyCode::KeyZ),
    );
    input_state.set_button(
        WiiButton::Z,
        keyboard.pressed(KeyCode::KeyK) || keyboard.pressed(KeyCode::KeyX),
    );
    input_state.set_button(
        WiiButton::One,
        keyboard.pressed(KeyCode::Digit1) || keyboard.pressed(KeyCode::Numpad1),
    );
    input_state.set_button(
        WiiButton::Two,
        keyboard.pressed(KeyCode::Digit2) || keyboard.pressed(KeyCode::Numpad2),
    );
}

pub fn hero_movement_system(
    input: Res<WiiInputState>,
    env: Res<EnvironmentCollision>,
    mut query: Query<
        (
            &mut Velocity,
            &MovementSpeed,
            &CharacterCollider,
            &mut Transform,
        ),
        With<Hero>,
    >,
) {
    for (mut velocity, speed, collider, mut transform) in &mut query {
        velocity.0.x = input.stick[0] * speed.0;
        velocity.0.z = -input.stick[1] * speed.0;

        if input.stick[0].abs() > 0.1 || input.stick[1].abs() > 0.1 {
            let angle = input.stick[0].atan2(-input.stick[1]);
            transform.rotation = Quat::from_rotation_y(angle);
        }

        if collider.grounded && input.is_down(WiiButton::A) {
            velocity.0.y = env.jump_impulse;
        }
    }
}

pub fn actor_physics_system(
    time: Res<Time<Fixed>>,
    env: Res<EnvironmentCollision>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut CharacterCollider)>,
) {
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };
    if dt <= 0.0 {
        return;
    }

    for (mut transform, mut velocity, mut collider) in &mut query {
        if !collider.grounded {
            velocity.0.y += env.gravity * dt;
        }

        const MAX_STEP: f32 = 0.2;
        let h_speed = (velocity.0.x * velocity.0.x + velocity.0.z * velocity.0.z).sqrt();
        let v_speed = velocity.0.y.abs();
        let total_move = (h_speed.max(v_speed) * dt).max(0.001);
        let num_steps = ((total_move / MAX_STEP).ceil() as usize).max(1);
        let sub_dt = dt / (num_steps as f32);

        for _ in 0..num_steps {
            transform.translation.x += velocity.0.x * sub_dt;
            transform.translation.z += velocity.0.z * sub_dt;

            let sphere_center = [
                transform.translation.x,
                transform.translation.y + collider.radius,
                transform.translation.z,
            ];
            if let Some(push) = env.mesh.resolve_sphere(sphere_center, collider.radius) {
                transform.translation.x += push[0];
                transform.translation.z += push[2];
            }

            if velocity.0.y > 0.0 {
                transform.translation.y += velocity.0.y * sub_dt;
                collider.grounded = false;
            } else {
                let ray_start = [
                    transform.translation.x,
                    transform.translation.y + collider.step_height,
                    transform.translation.z,
                ];
                let max_ray_dist = collider.step_height + (-velocity.0.y * sub_dt).max(0.2);
                if let Some((ground_y, _)) = env.mesh.raycast_down(ray_start, max_ray_dist) {
                    transform.translation.y = ground_y;
                    velocity.0.y = 0.0;
                    collider.grounded = true;
                } else {
                    transform.translation.y += velocity.0.y * sub_dt;
                    collider.grounded = false;
                }
            }

            if transform.translation.y < env.kill_plane_y {
                transform.translation = env.spawn_point;
                velocity.0 = Vec3::ZERO;
                collider.grounded = true;
                break;
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn hero_combat_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    input: Res<WiiInputState>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    mut damage_events: EventWriter<DamageEvent>,
    mut hero_query: Query<
        (Entity, &Transform, &mut AttackCombo, Option<&AttackDamage>),
        With<Hero>,
    >,
    target_query: Query<
        (
            Entity,
            &Transform,
            &CharacterCollider,
            Option<&Prop>,
            Option<&Villain>,
        ),
        Without<Hero>,
    >,
) {
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };

    for (_hero_entity, hero_tf, mut combo, attack_damage) in &mut hero_query {
        let attack_damage = attack_damage.copied().unwrap_or_default();
        if combo.cooldown > 0.0 {
            combo.cooldown -= dt;
            if combo.cooldown <= 0.0 {
                combo.cooldown = 0.0;
                combo.step = 0;
            }
        }

        if let Some(ref mut attack) = combo.active {
            attack.timer += dt;
            if attack.timer >= attack.duration {
                combo.active = None;
            }
        }

        let mut triggered_attack = None;
        if combo.active.is_none() {
            if input.is_down(WiiButton::B) {
                let next_step = (combo.step % 3) + 1;
                combo.step = next_step;
                combo.cooldown = 0.6;
                combo.active = Some(ActiveAttack {
                    timer: 0.0,
                    duration: 0.25,
                    kind: AttackKind::PrimaryMelee,
                    has_hit: false,
                });
                triggered_attack = Some(AttackKind::PrimaryMelee);
            } else if input.is_down(WiiButton::Z) {
                combo.step = 0;
                combo.cooldown = 0.0;
                combo.active = Some(ActiveAttack {
                    timer: 0.0,
                    duration: 0.45,
                    kind: AttackKind::SecondarySweep,
                    has_hit: false,
                });
                triggered_attack = Some(AttackKind::SecondarySweep);
            } else if input.is_down(WiiButton::One) || input.is_down(WiiButton::Two) {
                combo.step = 0;
                combo.cooldown = 0.0;
                combo.active = Some(ActiveAttack {
                    timer: 0.0,
                    duration: 0.35,
                    kind: AttackKind::SpecialProjectile,
                    has_hit: false,
                });
                triggered_attack = Some(AttackKind::SpecialProjectile);
            }
        }

        if let Some(kind) = triggered_attack {
            audio_events.send(WorldAudioCueEvent(WorldAudioCue::HeroAttack(kind)));

            let forward = hero_tf.rotation * Vec3::NEG_Z;
            let fwd_norm = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();

            match kind {
                AttackKind::SpecialProjectile => {
                    const PROJ_SPEED: f32 = 12.0;
                    commands.spawn((
                        Projectile {
                            lifetime: 0.0,
                            max_lifetime: 1.5,
                            damage: attack_damage.special.round() as u32,
                            radius: 0.5,
                        },
                        Velocity(fwd_norm * PROJ_SPEED),
                        Transform::from_translation(hero_tf.translation + Vec3::new(0.0, 0.8, 0.0)),
                    ));
                }
                AttackKind::PrimaryMelee | AttackKind::SecondarySweep => {
                    let (range, cos_half_angle, damage) = if kind == AttackKind::PrimaryMelee {
                        (1.8f32, (45.0f32.to_radians()).cos(), attack_damage.primary)
                    } else {
                        (2.8f32, -1.0f32, attack_damage.secondary)
                    };
                    let damage = damage.round() as u32;

                    let hero_pos = hero_tf.translation;
                    let mut hits = Vec::new();

                    for (target_entity, target_tf, collider, maybe_prop, maybe_villain) in
                        &target_query
                    {
                        let is_target =
                            maybe_villain.is_some() || maybe_prop.is_some_and(|p| p.destructible);
                        if !is_target {
                            continue;
                        }

                        let diff = target_tf.translation - hero_pos;
                        let dist_2d = Vec2::new(diff.x, diff.z).length();
                        if dist_2d <= range + collider.radius {
                            let in_cone = if dist_2d > 0.001 && cos_half_angle > -0.999 {
                                let dir_2d = Vec2::new(diff.x, diff.z).normalize();
                                let fwd_2d = Vec2::new(fwd_norm.x, fwd_norm.z).normalize();
                                dir_2d.dot(fwd_2d) >= cos_half_angle
                            } else {
                                true
                            };

                            if in_cone {
                                hits.push((target_entity, target_tf.translation, damage));
                            }
                        }
                    }

                    if !hits.is_empty() {
                        audio_events.send(WorldAudioCueEvent(WorldAudioCue::AttackHit));
                        for (entity, pos, dmg) in hits {
                            damage_events.send(DamageEvent {
                                target: entity,
                                damage: dmg,
                                position: pos,
                            });
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn projectile_flight_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    mut damage_events: EventWriter<DamageEvent>,
    mut proj_query: Query<(Entity, &mut Transform, &Velocity, &mut Projectile)>,
    target_query: Query<
        (
            Entity,
            &Transform,
            &CharacterCollider,
            Option<&Prop>,
            Option<&Villain>,
        ),
        (Without<Hero>, Without<Projectile>),
    >,
) {
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };

    for (proj_entity, mut proj_tf, velocity, mut proj) in &mut proj_query {
        proj.lifetime += dt;
        if proj.lifetime >= proj.max_lifetime {
            commands.entity(proj_entity).despawn();
            continue;
        }

        proj_tf.translation += velocity.0 * dt;

        let mut hit = false;
        for (target_entity, target_tf, collider, maybe_prop, maybe_villain) in &target_query {
            let is_target = maybe_villain.is_some() || maybe_prop.is_some_and(|p| p.destructible);
            if !is_target {
                continue;
            }

            let dist = proj_tf.translation.distance(target_tf.translation);
            if dist <= proj.radius + collider.radius {
                hit = true;
                audio_events.send(WorldAudioCueEvent(WorldAudioCue::AttackHit));
                damage_events.send(DamageEvent {
                    target: target_entity,
                    damage: proj.damage,
                    position: target_tf.translation,
                });
                break;
            }
        }

        if hit {
            commands.entity(proj_entity).despawn();
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn damage_system(
    mut commands: Commands,
    mut damage_events: EventReader<DamageEvent>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    mut target_query: Query<(
        Entity,
        &mut Health,
        &Transform,
        Option<&Prop>,
        Option<&Villain>,
    )>,
) {
    for ev in damage_events.read() {
        commands.spawn((
            DamagePopup {
                text: format!("-{}", ev.damage),
                lifetime: 0.0,
                max_lifetime: 1.0,
                color: [1.0, 0.2, 0.2, 1.0],
            },
            Transform::from_translation(ev.position + Vec3::new(0.0, 1.0, 0.0)),
        ));

        if let Ok((entity, mut health, transform, maybe_prop, maybe_villain)) =
            target_query.get_mut(ev.target)
        {
            health.current = health.current.saturating_sub(ev.damage);
            if health.current == 0 {
                let pos = transform.translation;
                if let Some(prop) = maybe_prop {
                    if prop.destructible {
                        audio_events.send(WorldAudioCueEvent(WorldAudioCue::PropDestroyed));
                        commands.spawn((
                            Collectible {
                                value: 1,
                                collected: false,
                            },
                            Transform::from_xyz(pos.x, pos.y, pos.z),
                            CharacterCollider {
                                radius: 0.5,
                                height: 0.5,
                                step_height: 0.0,
                                grounded: true,
                            },
                        ));
                        commands.spawn((
                            Collectible {
                                value: 5,
                                collected: false,
                            },
                            Transform::from_xyz(pos.x + 0.3, pos.y, pos.z + 0.3),
                            CharacterCollider {
                                radius: 0.5,
                                height: 0.5,
                                step_height: 0.0,
                                grounded: true,
                            },
                        ));
                        commands.entity(entity).despawn();
                    }
                } else if maybe_villain.is_some() {
                    commands.spawn((
                        Collectible {
                            value: 10,
                            collected: false,
                        },
                        Transform::from_xyz(pos.x, pos.y, pos.z),
                        CharacterCollider {
                            radius: 0.5,
                            height: 0.5,
                            step_height: 0.0,
                            grounded: true,
                        },
                    ));
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

pub fn collectible_pickup_system(
    mut commands: Commands,
    mut stats: ResMut<GameStats>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    hero_query: Query<&Transform, With<Hero>>,
    mut collectible_query: Query<(Entity, &Transform, &mut Collectible, &CharacterCollider)>,
) {
    let Ok(hero_tf) = hero_query.get_single() else {
        return;
    };

    for (entity, coll_tf, mut collectible, collider) in &mut collectible_query {
        if !collectible.collected {
            let dist = hero_tf.translation.distance(coll_tf.translation);
            if dist <= collider.radius + 0.7 {
                collectible.collected = true;
                stats.coins += collectible.value;
                audio_events.send(WorldAudioCueEvent(WorldAudioCue::PickupCollected));
                commands.entity(entity).despawn();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn zone_trigger_system(
    input: Res<WiiInputState>,
    mut stats: ResMut<GameStats>,
    mut transition_events: EventWriter<ZoneTransitionEvent>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    mut next_state: ResMut<NextState<GameState>>,
    mut transition_progress: ResMut<ZoneTransitionProgress>,
    hero_query: Query<&Transform, With<Hero>>,
    trigger_query: Query<(&Transform, &ZoneTrigger)>,
) {
    let Ok(hero_tf) = hero_query.get_single() else {
        stats.active_prompt = None;
        return;
    };

    let mut found_prompt = None;
    for (trig_tf, trigger) in &trigger_query {
        let dist = hero_tf.translation.distance(trig_tf.translation);
        if dist <= trigger.radius {
            found_prompt = Some(trigger.prompt.clone());
            if input.is_down(WiiButton::A) {
                transition_events.send(ZoneTransitionEvent {
                    target_zone: trigger.target_zone.clone(),
                    target_level: trigger.target_level.clone(),
                    target_spawn: trigger.target_spawn,
                });
                audio_events.send(WorldAudioCueEvent(WorldAudioCue::ZoneEntered(
                    trigger.target_zone.clone(),
                )));
                transition_progress.target_zone = trigger.target_zone.clone();
                transition_progress.target_level = trigger.target_level.clone();
                transition_progress.target_spawn = trigger.target_spawn;
                transition_progress.timer = 0.0;
                transition_progress.duration = 1.0;
                next_state.set(GameState::ZoneTransition);
            }
            break;
        }
    }

    stats.active_prompt = found_prompt;
}

pub fn animation_state_system(
    time: Res<Time<Fixed>>,
    mut query: Query<(&mut ActorAnimation, Option<&AttackCombo>, Option<&Velocity>)>,
) {
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };

    for (mut anim, maybe_combo, maybe_vel) in &mut query {
        anim.time += dt * anim.speed;

        if let Some(combo) = maybe_combo {
            let next_clip = if let Some(ref attack) = combo.active {
                match attack.kind {
                    AttackKind::PrimaryMelee => format!("attack_combo_{}", combo.step.max(1)),
                    AttackKind::SecondarySweep => "attack_sweep".to_string(),
                    AttackKind::SpecialProjectile => "attack_projectile".to_string(),
                }
            } else if let Some(vel) = maybe_vel {
                let h_speed = Vec2::new(vel.0.x, vel.0.z).length();
                if h_speed > 3.0 {
                    "run".to_string()
                } else if h_speed > 0.1 {
                    "walk".to_string()
                } else {
                    "idle".to_string()
                }
            } else {
                "idle".to_string()
            };

            if anim.current_clip.as_deref() != Some(&next_clip) {
                anim.current_clip = Some(next_clip);
                anim.time = 0.0;
            }
        }
    }
}

pub fn portal_audio_stream_system(
    mut portal: ResMut<ActivePortal>,
    mut streaming: ResMut<PortalAudioStreaming>,
) {
    if let Some(pkt) = streaming.queue.pop_front() {
        portal.send_audio(&pkt);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn state_transition_update_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    current_state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut capture_progress: ResMut<CaptureProgress>,
    mut transition_progress: ResMut<ZoneTransitionProgress>,
    mut stats: ResMut<GameStats>,
    mut hero_query: Query<&mut Transform, With<Hero>>,
    mut active_level: Option<ResMut<ActiveLevel>>,
    mut campaign_progress: Option<ResMut<CampaignProgress>>,
    mut env_col: Option<ResMut<EnvironmentCollision>>,
    asset_server: Option<Res<GameAssetServer>>,
    level_entities_query: Query<Entity, With<LevelEntity>>,
    mut level_changed_events: EventWriter<LevelChangedEvent>,
) {
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };

    match current_state.get() {
        GameState::VillainCapture => {
            capture_progress.progress = (capture_progress.progress + dt * 0.5).min(1.0);
            if capture_progress.progress >= 1.0 {
                next_state.set(GameState::Hub);
            }
        }
        GameState::ZoneTransition => {
            transition_progress.timer += dt;
            if transition_progress.timer >= transition_progress.duration {
                stats.active_zone = transition_progress.target_zone.clone();

                let current_level_name = active_level
                    .as_ref()
                    .map(|l| l.name.clone())
                    .unwrap_or_default();

                let needs_level_swap =
                    if let Some(ref target_lvl) = transition_progress.target_level {
                        !target_lvl.eq_ignore_ascii_case(&current_level_name)
                    } else {
                        false
                    };

                if needs_level_swap {
                    let target_lvl = transition_progress.target_level.take().unwrap();
                    let prev_level = current_level_name.clone();

                    if let Some(ref mut camp) = campaign_progress {
                        if let Some(ch) = camp.chapter_for_level(&current_level_name) {
                            camp.complete_chapter(ch);
                        }
                        camp.record_level_visit(&target_lvl);
                    }

                    for ent in &level_entities_query {
                        commands.entity(ent).despawn_recursive();
                    }

                    if let Some(ref mut lvl) = active_level {
                        lvl.name = target_lvl.clone();
                    }

                    let default_camp = CampaignProgress::default();
                    let camp_ref = campaign_progress.as_deref().unwrap_or(&default_camp);
                    let mut spawned_real = false;

                    if let Some(ref server) = asset_server {
                        if let Ok(loader) = server.loader.lock() {
                            spawned_real = spawn_level_content(
                                &mut commands,
                                &loader,
                                &target_lvl,
                                env_col.as_deref_mut(),
                                camp_ref,
                            );
                        }
                    }

                    if !spawned_real {
                        spawn_fallback_courtyard_content(&mut commands);
                    }

                    level_changed_events.send(LevelChangedEvent {
                        previous_level: prev_level,
                        new_level: target_lvl,
                    });
                }

                let spawn_pos = if transition_progress.target_spawn != Vec3::ZERO {
                    transition_progress.target_spawn
                } else if let Some(ref env) = env_col {
                    env.spawn_point
                } else {
                    Vec3::ZERO
                };

                for mut tf in &mut hero_query {
                    tf.translation = spawn_pos;
                }

                next_state.set(GameState::Hub);
            }
        }
        _ => {}
    }
}

pub fn spawn_level_content(
    commands: &mut Commands,
    loader: &skylands_formats::AssetLoader,
    level_name: &str,
    mut env_col: Option<&mut EnvironmentCollision>,
    campaign: &CampaignProgress,
) -> bool {
    let resolved = if loader.has_file(level_name) {
        Some(level_name.to_string())
    } else {
        crate::portal_gameplay::resolve_model_archive("level", level_name)
    };

    let Some(path) = resolved else {
        return false;
    };

    let Ok(model) = loader.load_igb(&path) else {
        return false;
    };

    let col_mesh = crate::collision::CollisionMesh::from_igb(&model);
    if !col_mesh.triangles.is_empty() {
        if let Some(ref mut env) = env_col {
            env.mesh = col_mesh;
            env.kill_plane_y = -350.0;
        }
    }

    let markers = model.extract_level_markers();
    let mut marker_id_counter = 5000u32;
    let mut has_spawn = false;

    for marker in &markers {
        match marker.kind {
            skylands_formats::igb::MarkerKind::Spawn => {
                if !has_spawn {
                    if let Some(ref mut env) = env_col {
                        env.spawn_point =
                            Vec3::new(marker.position[0], marker.position[1], marker.position[2]);
                    }
                    has_spawn = true;
                }
            }
            skylands_formats::igb::MarkerKind::Doorway
            | skylands_formats::igb::MarkerKind::TriggerZone => {
                let dest = crate::doorway::resolve_doorway_destination(
                    level_name,
                    &marker.name,
                    marker.position,
                    &markers,
                    campaign,
                );
                commands.spawn((
                    LevelEntity,
                    ZoneTrigger {
                        id: marker_id_counter,
                        prompt: dest.prompt,
                        target_zone: dest.target_zone,
                        target_level: dest.target_level,
                        target_spawn: dest.target_spawn,
                        radius: 2.5,
                    },
                    Transform::from_xyz(marker.position[0], marker.position[1], marker.position[2]),
                ));
                marker_id_counter += 1;
            }
            skylands_formats::igb::MarkerKind::Prop => {
                commands.spawn((
                    LevelEntity,
                    ActorIdentity {
                        id: marker_id_counter,
                        character_id: 0,
                        name: marker.name.clone(),
                        is_villain: false,
                        model_path: None,
                        texture_path: None,
                    },
                    Prop { destructible: true },
                    Health {
                        current: 50,
                        max: 50,
                    },
                    Transform::from_xyz(marker.position[0], marker.position[1], marker.position[2]),
                    CharacterCollider {
                        radius: 0.5,
                        height: 1.0,
                        step_height: 0.0,
                        grounded: true,
                    },
                ));
                marker_id_counter += 1;
            }
            skylands_formats::igb::MarkerKind::Collectible => {
                commands.spawn((
                    LevelEntity,
                    ActorIdentity {
                        id: marker_id_counter,
                        character_id: 0,
                        name: marker.name.clone(),
                        is_villain: false,
                        model_path: None,
                        texture_path: None,
                    },
                    Collectible {
                        value: 1,
                        collected: false,
                    },
                    Transform::from_xyz(
                        marker.position[0],
                        marker.position[1] + 0.5,
                        marker.position[2],
                    ),
                ));
                marker_id_counter += 1;
            }
            _ => {}
        }
    }

    if !has_spawn {
        if let Some(ref mut env) = env_col {
            if let Some((hit_y, _)) = env.mesh.raycast_down([0.0, 50.0, 0.0], 500.0) {
                env.spawn_point = Vec3::new(0.0, hit_y, 0.0);
            }
        }
    }

    true
}

pub fn spawn_fallback_courtyard_content(commands: &mut Commands) {
    commands.spawn((
        LevelEntity,
        ZoneTrigger {
            id: 1,
            prompt: "Press A to Enter Academy".into(),
            target_zone: "Academy Library".into(),
            target_level: None,
            target_spawn: Vec3::new(0.0, 0.0, 5.0),
            radius: 2.5,
        },
        Transform::from_xyz(0.0, 0.0, -8.0),
    ));

    commands.spawn((
        LevelEntity,
        ActorIdentity {
            id: 2001,
            character_id: 0,
            name: "prop_wooden_barrel".into(),
            is_villain: false,
            model_path: None,
            texture_path: None,
        },
        Prop { destructible: true },
        Health {
            current: 50,
            max: 50,
        },
        Transform::from_xyz(5.0, 0.0, 0.0),
        CharacterCollider {
            radius: 0.5,
            height: 1.0,
            step_height: 0.0,
            grounded: true,
        },
    ));

    commands.spawn((
        LevelEntity,
        ActorIdentity {
            id: 3001,
            character_id: 0,
            name: "pickup_coin_gold".into(),
            is_villain: false,
            model_path: None,
            texture_path: None,
        },
        Collectible {
            value: 1,
            collected: false,
        },
        Transform::from_xyz(0.0, 0.5, -2.0),
    ));

    commands.spawn((
        LevelEntity,
        ActorIdentity {
            id: 3002,
            character_id: 0,
            name: "pickup_gem_emerald".into(),
            is_villain: false,
            model_path: None,
            texture_path: None,
        },
        Collectible {
            value: 5,
            collected: false,
        },
        Transform::from_xyz(0.0, 0.5, -10.0),
    ));
}

pub fn setup_default_scene(
    mut commands: Commands,
    asset_server: Option<Res<GameAssetServer>>,
    active_level: Option<Res<ActiveLevel>>,
    mut env_col: Option<ResMut<EnvironmentCollision>>,
    mut game_stats: Option<ResMut<GameStats>>,
    mut campaign_progress: Option<ResMut<CampaignProgress>>,
) {
    let level_name = active_level
        .map(|l| l.name.clone())
        .unwrap_or_else(|| "Level_Hub_2014_01.bld".to_string());

    let default_campaign = CampaignProgress::default();
    let campaign_ref = campaign_progress.as_deref().unwrap_or(&default_campaign);

    let mut loaded_real_level = false;

    if let Some(server) = asset_server {
        if let Ok(loader) = server.loader.lock() {
            loaded_real_level = spawn_level_content(
                &mut commands,
                &loader,
                &level_name,
                env_col.as_deref_mut(),
                campaign_ref,
            );
        }
    }

    if loaded_real_level {
        if let Some(ref mut stats) = game_stats {
            stats.active_zone = level_name
                .strip_suffix(".bld")
                .unwrap_or(&level_name)
                .to_string();
        }
        if let Some(ref mut camp) = campaign_progress {
            camp.record_level_visit(&level_name);
        }
    } else {
        spawn_fallback_courtyard_content(&mut commands);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SkylandsCorePlugin;

    fn build_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(SkylandsCorePlugin);
        app
    }

    fn step_fixed(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
    }

    #[test]
    fn test_hero_movement_and_jump() {
        let mut app = build_test_app();

        let hero_id = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 400,
                    character_id: 400,
                    name: "Snap Shot".into(),
                    is_villain: false,
                    model_path: None,
                    texture_path: None,
                },
                Hero,
                Transform::from_xyz(0.0, 0.0, 0.0),
                CharacterCollider {
                    radius: 0.5,
                    height: 1.8,
                    step_height: 0.35,
                    grounded: true,
                },
                Velocity::default(),
                MovementSpeed(5.0),
                Health::default(),
                AttackCombo::default(),
                ActorAnimation::default(),
            ))
            .id();

        // 1. Set stick input
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .set_stick(1.0, 0.0);
        app.world_mut().run_schedule(FixedUpdate);

        let vel = app.world().get::<Velocity>(hero_id).unwrap().0;
        eprintln!("DEBUG: vel = {:?}", vel);
        assert!((vel.x - 5.0).abs() < 1e-4);

        // 2. Trigger jump on A
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::A);
        step_fixed(&mut app);

        let vel = app.world().get::<Velocity>(hero_id).unwrap().0;
        let jump_imp = app.world().resource::<EnvironmentCollision>().jump_impulse;
        assert_eq!(vel.y, jump_imp);
    }

    #[test]
    fn test_combat_damage_and_collectible_pickup() {
        let mut app = build_test_app();

        // Spawn Hero
        app.world_mut().spawn((
            ActorIdentity {
                id: 400,
                character_id: 400,
                name: "Snap Shot".into(),
                is_villain: false,
                model_path: None,
                texture_path: None,
            },
            Hero,
            Transform::from_xyz(0.0, 0.0, 0.0),
            CharacterCollider::default(),
            Velocity::default(),
            MovementSpeed(5.0),
            AttackCombo::default(),
            ActorAnimation::default(),
        ));

        // Spawn Destructible Prop 1.6 units forward (within melee reach 2.3, outside pickup radius 1.2)
        let prop_entity = app
            .world_mut()
            .spawn((
                Prop { destructible: true },
                Health {
                    current: 50,
                    max: 50,
                },
                Transform::from_xyz(0.0, 0.0, -1.6),
                CharacterCollider {
                    radius: 0.5,
                    height: 1.0,
                    step_height: 0.0,
                    grounded: true,
                },
            ))
            .id();

        // Press B for PrimaryMelee attack
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::B);
        step_fixed(&mut app);

        // Prop health was 50, melee damage is 50 -> destroyed!
        assert!(
            app.world().get_entity(prop_entity).is_err(),
            "Prop should be destroyed on 0 health"
        );

        // Collectibles should have spawned at prop location
        let mut coll_query = app
            .world_mut()
            .query::<(&Transform, &Collectible, &CharacterCollider)>();
        let spawned_colls: Vec<Transform> =
            coll_query.iter(app.world()).map(|(t, _, _)| *t).collect();
        assert!(!spawned_colls.is_empty(), "Loot should drop from prop");

        // Move hero to collect items
        let mut hero_query = app
            .world_mut()
            .query_filtered::<&mut Transform, With<Hero>>();
        for mut tf in hero_query.iter_mut(app.world_mut()) {
            tf.translation = Vec3::new(0.0, 0.0, -1.6);
        }

        step_fixed(&mut app);
        let coins = app.world().resource::<GameStats>().coins;
        assert!(coins > 0, "Coins collected by hero: {}", coins);
    }

    #[test]
    fn test_melee_uses_character_attack_damage() {
        let mut app = build_test_app();
        app.world_mut().spawn((
            Hero,
            Transform::from_xyz(0.0, 0.0, 0.0),
            CharacterCollider::default(),
            Velocity::default(),
            MovementSpeed(5.0),
            AttackCombo::default(),
            AttackDamage {
                primary: 15.0,
                secondary: 10.0,
                special: 30.0,
            },
            ActorAnimation::default(),
        ));
        let villain = app
            .world_mut()
            .spawn((
                Villain,
                Health {
                    current: 100,
                    max: 100,
                },
                Transform::from_xyz(0.0, 0.0, -1.0),
                CharacterCollider::default(),
            ))
            .id();

        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::B);
        step_fixed(&mut app);

        assert_eq!(app.world().get::<Health>(villain).unwrap().current, 85);
    }

    #[test]
    fn test_projectile_uses_character_special_damage() {
        let mut app = build_test_app();
        app.world_mut().spawn((
            Hero,
            Transform::from_xyz(0.0, 0.0, 0.0),
            CharacterCollider::default(),
            Velocity::default(),
            MovementSpeed(5.0),
            AttackCombo::default(),
            AttackDamage {
                primary: 15.0,
                secondary: 10.0,
                special: 12.5,
            },
            ActorAnimation::default(),
        ));
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::One);
        step_fixed(&mut app);

        let mut q = app.world_mut().query::<&Projectile>();
        let dmg: Vec<u32> = q.iter(app.world()).map(|p| p.damage).collect();
        assert_eq!(dmg, vec![13]);
    }

    #[test]
    fn test_projectile_spawn_and_hit() {
        let mut app = build_test_app();

        // Spawn Hero facing forward (-Z)
        app.world_mut().spawn((
            Hero,
            Transform::from_xyz(0.0, 0.0, 0.0),
            CharacterCollider::default(),
            Velocity::default(),
            MovementSpeed(5.0),
            AttackCombo::default(),
            ActorAnimation::default(),
        ));

        // Spawn Villain 3 units ahead
        let villain_entity = app
            .world_mut()
            .spawn((
                Villain,
                Health {
                    current: 100,
                    max: 100,
                },
                Transform::from_xyz(0.0, 0.0, -3.0),
                CharacterCollider {
                    radius: 0.6,
                    height: 1.8,
                    step_height: 0.0,
                    grounded: true,
                },
            ))
            .id();

        // Press One for projectile
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::One);
        step_fixed(&mut app);

        // Check projectile spawned
        let mut proj_query = app.world_mut().query::<&Projectile>();
        assert_eq!(proj_query.iter(app.world()).count(), 1);

        // Advance several ticks for projectile flight
        for _ in 0..20 {
            step_fixed(&mut app);
        }

        // Villain took damage (75 damage)
        if let Some(health) = app.world().get::<Health>(villain_entity) {
            assert_eq!(health.current, 25);
        }
    }

    #[test]
    fn test_portal_tag_spawns_hero_entity() {
        let mut app = build_test_app();

        app.world_mut()
            .resource_mut::<Events<PortalEvent>>()
            .send(PortalEvent::TagArrived {
                tag_index: 1,
                zone: PortalZone::LeftRing,
            });

        step_fixed(&mut app);

        let mut hero_query = app.world_mut().query::<(&Hero, &ActorIdentity)>();
        let count = hero_query.iter(app.world()).count();
        assert_eq!(count, 1);

        let (_, ident) = hero_query.iter(app.world()).next().unwrap();
        assert_eq!(ident.character_id, 16);
        assert_eq!(ident.name, "Spyro");
    }

    #[test]
    fn test_setup_default_scene_spawns_courtyard_entities() {
        let mut app = build_test_app();
        app.update();

        let mut trigger_query = app.world_mut().query::<&ZoneTrigger>();
        assert_eq!(trigger_query.iter(app.world()).count(), 1);

        let mut collect_query = app.world_mut().query::<&Collectible>();
        assert_eq!(collect_query.iter(app.world()).count(), 2);

        let mut prop_query = app.world_mut().query::<&Prop>();
        assert_eq!(prop_query.iter(app.world()).count(), 1);
    }

    #[test]
    fn test_active_level_resource_defaults() {
        let lvl = ActiveLevel::default();
        assert_eq!(lvl.name, "Level_Hub_2014_01.bld");
        let custom = ActiveLevel::new("Level_313_Tutorial.bld");
        assert_eq!(custom.name, "Level_313_Tutorial.bld");
    }

    #[test]
    fn test_level_swap_despawns_level_entities_and_preserves_hero() {
        let mut app = build_test_app();
        app.update();

        // Courtyard spawned 4 LevelEntities: 1 trigger, 1 prop, 2 collectibles
        let initial_level_entities = app
            .world_mut()
            .query_filtered::<Entity, With<LevelEntity>>()
            .iter(app.world())
            .count();
        assert_eq!(initial_level_entities, 4);

        // Spawn hero at (0, 0, 0)
        let hero_entity = app
            .world_mut()
            .spawn((
                Hero,
                Transform::from_xyz(0.0, 0.0, 0.0),
                CharacterCollider::default(),
                Velocity::default(),
                MovementSpeed(5.0),
                AttackCombo::default(),
                ActorAnimation::default(),
            ))
            .id();

        // Initiate level swap transition to Chapter 1
        let mut transition_progress = app.world_mut().resource_mut::<ZoneTransitionProgress>();
        transition_progress.target_zone = "Soda Springs".to_string();
        transition_progress.target_level = Some("Level_313_Tutorial.bld".to_string());
        transition_progress.target_spawn = Vec3::new(10.0, 5.0, -20.0);
        transition_progress.timer = 0.0;
        transition_progress.duration = 0.1;

        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::ZoneTransition);

        // Tick past transition duration
        for _ in 0..10 {
            step_fixed(&mut app);
        }

        // Active level must have changed
        let active_level = app.world().resource::<ActiveLevel>();
        assert_eq!(active_level.name, "Level_313_Tutorial.bld");

        // Hero entity must still exist and be placed at target_spawn
        let hero_tf = app
            .world()
            .get::<Transform>(hero_entity)
            .expect("Hero survived level swap");
        assert_eq!(hero_tf.translation.x, 10.0);
        assert!((hero_tf.translation.y - 5.0).abs() < 0.2);
        assert_eq!(hero_tf.translation.z, -20.0);

        // State returned to Hub
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::Hub);

        // New level entities were spawned (fallback or real)
        let new_level_entities = app
            .world_mut()
            .query_filtered::<Entity, With<LevelEntity>>()
            .iter(app.world())
            .count();
        assert!(new_level_entities > 0);
    }

    #[test]
    fn test_campaign_progress_progression() {
        let mut campaign = CampaignProgress::default();
        assert_eq!(campaign.highest_unlocked_chapter, 1);
        assert!(campaign.is_chapter_unlocked(1));
        assert!(!campaign.is_chapter_unlocked(2));

        campaign.record_level_visit("Level_313_Tutorial.bld");
        assert_eq!(campaign.current_chapter, Some(1));
        assert!(campaign
            .visited_levels
            .contains(&"Level_313_Tutorial.bld".to_string()));

        campaign.complete_chapter(1);
        assert!(campaign.completed_chapters.contains(&1));
        assert_eq!(campaign.highest_unlocked_chapter, 2);
        assert!(campaign.is_chapter_unlocked(2));

        // Completing final chapter caps at 18
        campaign.complete_chapter(18);
        assert_eq!(campaign.highest_unlocked_chapter, 18);
    }
}
