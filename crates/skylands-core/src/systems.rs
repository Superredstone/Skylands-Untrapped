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

/// DOL `ModelParameters` with the speed and turn rates the character's script passes to
/// "Minion - setup pre descriptions" (`hero_movement`).
fn character_model_params(
    model: Option<&skylands_formats::IgbModel>,
    asset_server: Option<&GameAssetServer>,
    model_path: &str,
) -> ModelParameters {
    let mut params = model.map(ModelParameters::from_model).unwrap_or_default();
    if let Some(m) = asset_server.and_then(|s| s.hero_movement(model_path)) {
        params.ground_max_speed = m.ground_max_speed * FEET_TO_UNITS;
        params.air_max_speed = m.air_max_speed * FEET_TO_UNITS;
        params.ground_turn_rate = m.ground_turn_rate;
        params.air_turn_rate = m.air_turn_rate;
    }
    params
}

/// Script max HP (`hero_max_hp`), else the 500 default.
fn character_health(asset_server: Option<&GameAssetServer>, model_path: &str) -> Health {
    asset_server
        .and_then(|s| s.hero_max_hp(model_path))
        .map(|hp| Health {
            current: hp,
            max: hp,
        })
        .unwrap_or_default()
}

pub fn spawn_hero_actor(
    commands: &mut Commands,
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    char_id: u16,
    asset_server: Option<&GameAssetServer>,
) -> Entity {
    let (name, _elem) = get_character_info(char_id);
    let (model_path, tex_path) = get_character_asset_paths(char_id);
    let model = asset_server.and_then(|s| s.loader.lock().ok()?.load_igb(model_path).ok());
    let damage = model
        .as_deref()
        .and_then(AttackDamage::from_model)
        .unwrap_or_default();
    let projectile = model
        .as_deref()
        .map(ProjectileParams::from_model)
        .unwrap_or_default();
    let collider = model
        .as_deref()
        .map(CharacterCollider::from_model)
        .unwrap_or_default();
    let model_params = character_model_params(model.as_deref(), asset_server, model_path);
    let health = character_health(asset_server, model_path);

    commands
        .spawn((
            ActorIdentity {
                id: char_id as u32,
                character_id: char_id,
                name: name.to_string(),
                is_villain: false,
                model_path: Some(model_path.to_string()),
                texture_path: tex_path.map(|s| s.to_string()),
            },
            Hero,
            Transform::from_translation(pos).with_rotation(rot),
            collider,
            Velocity(vel),
            MovementSpeed(model_params.ground_max_speed),
            model_params,
            health,
            AttackCombo::default(),
            damage,
            projectile,
            ActorAnimation::default(),
        ))
        .id()
}

pub fn spawn_controlled_villain_actor(
    commands: &mut Commands,
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    char_id: u16,
    energy: VillainEnergy,
    asset_server: Option<&GameAssetServer>,
) -> Entity {
    let (name, _elem) = get_character_info(char_id);
    let (model_path, tex_path) = get_character_asset_paths(char_id);
    let model = asset_server.and_then(|s| s.loader.lock().ok()?.load_igb(model_path).ok());
    let damage = model
        .as_deref()
        .and_then(AttackDamage::from_model)
        .unwrap_or(AttackDamage {
            primary: 35.0,
            secondary: 50.0,
            special: 40.0,
        });
    let projectile = model
        .as_deref()
        .map(ProjectileParams::from_model)
        .unwrap_or(ProjectileParams {
            speed: 30.0,
            lifetime: 1.0,
            primary_style: PrimaryAttackStyle::Melee,
        });
    let collider = model
        .as_deref()
        .map(CharacterCollider::from_model)
        .unwrap_or(CharacterCollider {
            radius: 0.6,
            height: 1.8,
            step_height: 0.35,
            grounded: true,
        });
    let model_params = character_model_params(model.as_deref(), asset_server, model_path);

    commands
        .spawn((
            ActorIdentity {
                id: 1000 + char_id as u32,
                character_id: char_id,
                name: name.to_string(),
                is_villain: true,
                model_path: Some(model_path.to_string()),
                texture_path: tex_path.map(|s| s.to_string()),
            },
            Hero,
            ControlledVillain,
            Transform::from_translation(pos).with_rotation(rot),
            collider,
            Velocity(vel),
            MovementSpeed(model_params.ground_max_speed),
            model_params,
            Health {
                current: energy.current.round() as u32,
                max: energy.max.round() as u32,
            },
            energy,
            AttackCombo::default(),
            damage,
            projectile,
            ActorAnimation::default(),
        ))
        .id()
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
    mut trapped_state: ResMut<TrappedVillainState>,
    mut magic_moment: ResMut<MagicMoment>,
    actors: Query<(Entity, &ActorIdentity, Has<Hero>)>,
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
                    capture_progress.duration = 2.0;
                    trapped_state.trapped_villain_id = Some(char_id);
                    trapped_state.energy.current = trapped_state.energy.max;
                    next_state.set(GameState::VillainCapture);

                    let (r, g, b) = elem.rgb();
                    portal.set_trap_light(r, g, b);

                    audio_streaming.sink.queue_villain_speech(char_id, 1.5);
                    while let Some(pkt) = audio_streaming.sink.pop_packet() {
                        audio_streaming.queue.push_back(pkt);
                    }

                    let model = asset_server
                        .as_ref()
                        .and_then(|s| s.loader.lock().ok()?.load_igb(model_path).ok());
                    let collider = model
                        .as_deref()
                        .map(CharacterCollider::from_model)
                        .unwrap_or(CharacterCollider {
                            radius: 0.6,
                            height: 1.8,
                            step_height: 0.35,
                            grounded: true,
                        });
                    let model_params = character_model_params(
                        model.as_deref(),
                        asset_server.as_deref(),
                        model_path,
                    );
                    let health = character_health(asset_server.as_deref(), model_path);

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
                        collider,
                        Velocity::default(),
                        MovementSpeed(model_params.ground_max_speed),
                        model_params,
                        health,
                        ActorAnimation::default(),
                    ));
                } else if gameplay_state.defeated_heroes.contains(&char_id) {
                    // `Portal_Manager.ai` "Request Player Needs Tag Message": defeated
                    // Skylanders stay out until the level changes.
                    continue;
                } else {
                    gameplay_state.active_hero_character_id = Some(char_id);
                    let (r, g, b) = elem.rgb();
                    portal.set_color(*zone, r, g, b);

                    // Despawn existing heroes first
                    for (entity, ident, is_hero) in &actors {
                        if is_hero && !ident.is_villain {
                            commands.entity(entity).despawn_recursive();
                        }
                    }

                    // Spawn hero entity
                    let hero_pos = spawn_origin + Vec3::new(0.0, 0.2, 0.0);
                    let hero = spawn_hero_actor(
                        &mut commands,
                        hero_pos,
                        Quat::IDENTITY,
                        Vec3::ZERO,
                        char_id,
                        asset_server.as_deref(),
                    );
                    commands.entity(hero).insert(PortalSpawn::default());
                    magic_moment.start(
                        char_id,
                        name,
                        toy_alive_duration(char_id, asset_server.as_deref()),
                    );
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
                if trapped_state.control_mode == ActiveControlMode::Villain {
                    for (entity, ident, _) in &actors {
                        if ident.is_villain {
                            commands.entity(entity).despawn_recursive();
                        }
                    }
                    trapped_state.control_mode = ActiveControlMode::Hero;
                    let hero_id = gameplay_state.active_hero_character_id.unwrap_or(16);
                    spawn_hero_actor(
                        &mut commands,
                        spawn_origin,
                        Quat::IDENTITY,
                        Vec3::ZERO,
                        hero_id,
                        asset_server.as_deref(),
                    );
                } else {
                    for (entity, ident, _) in &actors {
                        if ident.is_villain {
                            commands.entity(entity).despawn_recursive();
                        }
                    }
                }
                gameplay_state.active_villain_id = None;
                trapped_state.is_unlocked = false;
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
    input_state.set_button(
        WiiButton::Minus,
        keyboard.pressed(KeyCode::Minus)
            || keyboard.pressed(KeyCode::NumpadSubtract)
            || keyboard.pressed(KeyCode::Escape)
            || keyboard.pressed(KeyCode::Backspace)
            || keyboard.pressed(KeyCode::Tab),
    );
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn villain_tag_team_swap_system(
    mut commands: Commands,
    time: Option<Res<Time<Fixed>>>,
    input: Res<WiiInputState>,
    mut trapped_state: ResMut<TrappedVillainState>,
    gameplay_state: Res<PortalGameplayState>,
    mut portal: ResMut<ActivePortal>,
    mut audio_streaming: ResMut<PortalAudioStreaming>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    asset_server: Option<Res<GameAssetServer>>,
    hero_query: Query<(Entity, &Transform, &Velocity), (With<Hero>, Without<ControlledVillain>)>,
    villain_query: Query<(Entity, &Transform, &Velocity), (With<Hero>, With<ControlledVillain>)>,
) {
    let dt = time
        .as_ref()
        .map(|t| {
            if t.delta_secs() > 0.0 {
                t.delta_secs()
            } else {
                t.timestep().as_secs_f32()
            }
        })
        .unwrap_or(1.0 / 60.0);

    if trapped_state.swap_cooldown > 0.0 {
        trapped_state.swap_cooldown = (trapped_state.swap_cooldown - dt).max(0.0);
    }

    if input.is_down(WiiButton::Minus) && trapped_state.swap_cooldown <= 0.0 {
        trapped_state.swap_cooldown = 0.5;

        match trapped_state.control_mode {
            ActiveControlMode::Hero => {
                if trapped_state.is_unlocked
                    && trapped_state.energy.current >= trapped_state.min_swap_energy
                {
                    if let Some(villain_id) = trapped_state.trapped_villain_id {
                        if let Ok((hero_ent, hero_tf, hero_vel)) = hero_query.get_single() {
                            let pos = hero_tf.translation;
                            let rot = hero_tf.rotation;
                            let vel = hero_vel.0;
                            commands.entity(hero_ent).despawn_recursive();

                            spawn_controlled_villain_actor(
                                &mut commands,
                                pos,
                                rot,
                                vel,
                                villain_id,
                                trapped_state.energy,
                                asset_server.as_deref(),
                            );

                            trapped_state.control_mode = ActiveControlMode::Villain;
                            let (_, elem) = get_character_info(villain_id);
                            let (r, g, b) = elem.rgb();
                            portal.set_trap_light(r, g, b);

                            audio_streaming.sink.queue_villain_speech(villain_id, 1.5);
                            while let Some(pkt) = audio_streaming.sink.pop_packet() {
                                audio_streaming.queue.push_back(pkt);
                            }

                            audio_events.send(WorldAudioCueEvent(WorldAudioCue::VillainSwapped));
                        }
                    }
                }
            }
            ActiveControlMode::Villain => {
                if let Ok((villain_ent, villain_tf, villain_vel)) = villain_query.get_single() {
                    let pos = villain_tf.translation;
                    let rot = villain_tf.rotation;
                    let vel = villain_vel.0;
                    commands.entity(villain_ent).despawn_recursive();

                    let hero_id = gameplay_state.active_hero_character_id.unwrap_or(16);
                    spawn_hero_actor(
                        &mut commands,
                        pos,
                        rot,
                        vel,
                        hero_id,
                        asset_server.as_deref(),
                    );

                    trapped_state.control_mode = ActiveControlMode::Hero;
                    let (_, elem) =
                        get_character_info(trapped_state.trapped_villain_id.unwrap_or(1010));
                    let (r, g, b) = elem.rgb();
                    portal.set_trap_light(r / 3, g / 3, b / 3);

                    audio_events.send(WorldAudioCueEvent(WorldAudioCue::VillainTrapReturned));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn villain_energy_system(
    mut commands: Commands,
    time: Option<Res<Time<Fixed>>>,
    mut trapped_state: ResMut<TrappedVillainState>,
    gameplay_state: Res<PortalGameplayState>,
    mut portal: ResMut<ActivePortal>,
    mut audio_streaming: ResMut<PortalAudioStreaming>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    mut stats: ResMut<GameStats>,
    asset_server: Option<Res<GameAssetServer>>,
    mut villain_query: Query<
        (
            Entity,
            &Transform,
            &Velocity,
            &mut Health,
            &mut VillainEnergy,
        ),
        (With<Hero>, With<ControlledVillain>),
    >,
) {
    let dt = time
        .as_ref()
        .map(|t| {
            if t.delta_secs() > 0.0 {
                t.delta_secs()
            } else {
                t.timestep().as_secs_f32()
            }
        })
        .unwrap_or(1.0 / 60.0);

    match trapped_state.control_mode {
        ActiveControlMode::Villain => {
            trapped_state.energy.current =
                (trapped_state.energy.current - trapped_state.energy.drain_rate * dt).max(0.0);

            if let Ok((villain_ent, villain_tf, villain_vel, mut health, mut energy)) =
                villain_query.get_single_mut()
            {
                energy.current = trapped_state.energy.current;
                health.current = trapped_state.energy.current.round() as u32;

                if trapped_state.energy.current <= 0.0 {
                    let pos = villain_tf.translation;
                    let rot = villain_tf.rotation;
                    let vel = villain_vel.0;
                    commands.entity(villain_ent).despawn_recursive();

                    let hero_id = gameplay_state.active_hero_character_id.unwrap_or(16);
                    spawn_hero_actor(
                        &mut commands,
                        pos,
                        rot,
                        vel,
                        hero_id,
                        asset_server.as_deref(),
                    );

                    trapped_state.control_mode = ActiveControlMode::Hero;
                    trapped_state.swap_cooldown = 0.5;

                    portal.set_trap_light(255, 0, 0);

                    if let Some(villain_id) = trapped_state.trapped_villain_id {
                        audio_streaming.sink.queue_villain_speech(villain_id, 1.5);
                        while let Some(pkt) = audio_streaming.sink.pop_packet() {
                            audio_streaming.queue.push_back(pkt);
                        }
                    }

                    audio_events.send(WorldAudioCueEvent(WorldAudioCue::VillainTrapReturned));
                    stats.active_prompt =
                        Some("Villain energy depleted! Returned to trap.".to_string());
                }
            }
        }
        ActiveControlMode::Hero => {
            if trapped_state.is_unlocked && trapped_state.energy.current < trapped_state.energy.max
            {
                trapped_state.energy.current = (trapped_state.energy.current
                    + trapped_state.energy.recharge_rate * dt)
                    .min(trapped_state.energy.max);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn hero_movement_system(
    time: Option<Res<Time<Fixed>>>,
    input: Res<WiiInputState>,
    dialogue: Option<Res<ActiveDialogue>>,
    env: Res<EnvironmentCollision>,
    mut query: Query<
        (
            &mut Velocity,
            &MovementSpeed,
            &CharacterCollider,
            &mut Transform,
            Option<&ModelParameters>,
        ),
        With<Hero>,
    >,
    spawning: Query<&PortalSpawn, With<Hero>>,
    magic_moment: Option<Res<MagicMoment>>,
) {
    // The hero stands still while talking or entering through the portal.
    let idle = WiiInputState::default();
    let input = match dialogue {
        Some(d) if d.is_active() => &idle,
        _ if spawning.iter().any(|s| s.remaining > 0.0) => &idle,
        _ if magic_moment.is_some_and(|m| m.is_active()) => &idle,
        _ => &*input,
    };
    let dt = time
        .as_ref()
        .map(|t| {
            if t.delta_secs() > 0.0 {
                t.delta_secs()
            } else {
                t.timestep().as_secs_f32()
            }
        })
        .unwrap_or(1.0 / 60.0);

    for (mut velocity, speed, collider, mut transform, maybe_params) in &mut query {
        if let Some(params) = maybe_params {
            let (max_speed, accel, decel, turn_rate) = if collider.grounded {
                (
                    params.ground_max_speed,
                    params.ground_accel,
                    params.ground_decel,
                    params.ground_turn_rate,
                )
            } else {
                (
                    params.air_max_speed,
                    params.air_accel,
                    params.air_decel,
                    params.air_turn_rate,
                )
            };

            let stick_len =
                (input.stick[0] * input.stick[0] + input.stick[1] * input.stick[1]).sqrt();
            if stick_len > 0.1 {
                let stick_mag = stick_len.min(1.0);
                let target_dir = Vec2::new(input.stick[0], -input.stick[1]) / stick_len;
                let target_h = target_dir * (max_speed * stick_mag);
                let current_h = Vec2::new(velocity.0.x, velocity.0.z);
                let diff = target_h - current_h;
                let max_step = accel * dt;
                let new_h = if diff.length() <= max_step {
                    target_h
                } else {
                    current_h + diff.normalize() * max_step
                };
                velocity.0.x = new_h.x;
                velocity.0.z = new_h.y;

                let target_angle = input.stick[0].atan2(-input.stick[1]);
                let current_angle = transform.rotation.to_euler(EulerRot::YXZ).0;
                let diff = (target_angle - current_angle).rem_euclid(std::f32::consts::TAU);
                let signed_diff = if diff > std::f32::consts::PI {
                    diff - std::f32::consts::TAU
                } else {
                    diff
                };
                let max_turn = turn_rate.to_radians() * dt;
                let turn = signed_diff.clamp(-max_turn, max_turn);
                transform.rotation = Quat::from_rotation_y(current_angle + turn);
            } else {
                let current_h = Vec2::new(velocity.0.x, velocity.0.z);
                let speed_len = current_h.length();
                if speed_len > 0.0 {
                    let drop = (decel * dt).min(speed_len);
                    let new_h = current_h * ((speed_len - drop) / speed_len);
                    velocity.0.x = new_h.x;
                    velocity.0.z = new_h.y;
                }
            }

            if collider.grounded && input.is_down(WiiButton::A) {
                velocity.0.y = params.jump_initial_speed;
            }
        } else {
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

            let feet_center = [
                transform.translation.x,
                transform.translation.y + collider.radius,
                transform.translation.z,
            ];
            if let Some(push) = env.mesh.resolve_sphere(feet_center, collider.radius) {
                transform.translation.x += push[0];
                transform.translation.z += push[2];
            }

            if collider.height > 2.0 * collider.radius {
                let head_center = [
                    transform.translation.x,
                    transform.translation.y + collider.height - collider.radius,
                    transform.translation.z,
                ];
                if let Some(push) = env.mesh.resolve_sphere(head_center, collider.radius) {
                    transform.translation.x += push[0];
                    transform.translation.z += push[2];
                }
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
#[allow(clippy::too_many_arguments)]
pub fn hero_combat_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    input: Res<WiiInputState>,
    dialogue: Option<Res<ActiveDialogue>>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    mut damage_events: EventWriter<DamageEvent>,
    mut hero_query: Query<
        (
            Entity,
            &Transform,
            &mut AttackCombo,
            Option<&AttackDamage>,
            Option<&ProjectileParams>,
        ),
        With<Hero>,
    >,
    target_query: Query<
        (
            Entity,
            &Transform,
            &CharacterCollider,
            Option<&Prop>,
            Option<&Villain>,
            Option<&Enemy>,
        ),
        Without<Hero>,
    >,
    spawning: Query<&PortalSpawn, With<Hero>>,
    magic_moment: Option<Res<MagicMoment>>,
) {
    if dialogue.is_some_and(|d| d.is_active())
        || spawning.iter().any(|s| s.remaining > 0.0)
        || magic_moment.is_some_and(|m| m.is_active())
    {
        return;
    }
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };

    for (hero_entity, hero_tf, mut combo, attack_damage, projectile) in &mut hero_query {
        let attack_damage = attack_damage.copied().unwrap_or_default();
        let projectile = projectile.copied().unwrap_or_default();
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
                let (duration, cooldown, kind) = match projectile.primary_style {
                    PrimaryAttackStyle::Breath => (0.15, 0.18, AttackKind::PrimaryBreath),
                    PrimaryAttackStyle::Projectile => (0.25, 0.40, AttackKind::PrimaryMelee),
                    PrimaryAttackStyle::Melee => {
                        let next_step = (combo.step % 3) + 1;
                        combo.step = next_step;
                        (0.25, 0.60, AttackKind::PrimaryMelee)
                    }
                };
                combo.cooldown = cooldown;
                combo.active = Some(ActiveAttack {
                    timer: 0.0,
                    duration,
                    kind,
                    has_hit: false,
                });
                triggered_attack = Some(kind);
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

            let forward = hero_tf.rotation * Vec3::Z; // movement turns +Z toward the stick
            let fwd_norm = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();

            let mut fire = |damage: f32, piercing: bool| {
                commands.spawn((
                    Projectile {
                        lifetime: 0.0,
                        max_lifetime: projectile.lifetime,
                        damage: damage.round() as u32,
                        radius: 0.5,
                        hostile: false,
                        owner: Some(hero_entity),
                        piercing,
                    },
                    Velocity(fwd_norm * projectile.speed),
                    Transform::from_translation(hero_tf.translation + Vec3::new(0.0, 0.8, 0.0))
                        .with_rotation(hero_tf.rotation),
                ));
            };
            match kind {
                AttackKind::SpecialProjectile => fire(attack_damage.special, false),
                AttackKind::PrimaryBreath => fire(attack_damage.primary, true),
                AttackKind::PrimaryMelee
                    if projectile.primary_style == PrimaryAttackStyle::Projectile =>
                {
                    fire(attack_damage.primary, false)
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

                    for (
                        target_entity,
                        target_tf,
                        collider,
                        maybe_prop,
                        maybe_villain,
                        maybe_enemy,
                    ) in &target_query
                    {
                        let is_target = maybe_villain.is_some()
                            || maybe_enemy.is_some()
                            || maybe_prop.is_some_and(|p| p.destructible);
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
            Option<&Enemy>,
            Has<Hero>,
        ),
        Without<Projectile>,
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
            commands.entity(proj_entity).despawn_recursive();
            continue;
        }

        proj_tf.translation += velocity.0 * dt;

        let mut hit = false;
        for (target_entity, target_tf, collider, maybe_prop, maybe_villain, maybe_enemy, is_hero) in
            &target_query
        {
            let is_target = if proj.hostile {
                is_hero
            } else {
                !is_hero
                    && (maybe_villain.is_some()
                        || maybe_enemy.is_some()
                        || maybe_prop.is_some_and(|p| p.destructible))
            };
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
                if !proj.piercing {
                    break;
                }
            }
        }

        if hit && !proj.piercing {
            commands.entity(proj_entity).despawn_recursive();
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
        Option<&ControlledVillain>,
        Option<&ActorIdentity>,
        Has<Enemy>,
        Has<Hero>,
    )>,
    mut trapped_state: Option<ResMut<TrappedVillainState>>,
    mut capture_progress: Option<ResMut<CaptureProgress>>,
    mut next_state: Option<ResMut<NextState<GameState>>>,
    mut portal: Option<ResMut<ActivePortal>>,
    mut audio_streaming: Option<ResMut<PortalAudioStreaming>>,
    mut stats: Option<ResMut<GameStats>>,
    mut gameplay_state: Option<ResMut<PortalGameplayState>>,
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

        if let Ok((
            entity,
            mut health,
            transform,
            maybe_prop,
            maybe_villain,
            maybe_controlled,
            maybe_ident,
            is_enemy,
            is_hero,
        )) = target_query.get_mut(ev.target)
        {
            if maybe_controlled.is_some() {
                if let Some(ref mut ts) = trapped_state {
                    ts.energy.current = (ts.energy.current - ev.damage as f32).max(0.0);
                    health.current = ts.energy.current.round() as u32;
                }
            } else {
                health.current = health.current.saturating_sub(ev.damage);
                if health.current == 0 {
                    let pos = transform.translation;
                    if is_hero {
                        // `Portal_Manager.ai` "Tag Defeated": the Skylander is out for this
                        // level and the player must place another one on the portal.
                        let char_id = maybe_ident.map(|i| i.character_id).unwrap_or(0);
                        if let Some(ref mut gs) = gameplay_state {
                            gs.defeated_heroes.push(char_id);
                            gs.active_hero_character_id = None;
                        }
                        if let Some(ref mut s) = stats {
                            let name = maybe_ident.map(|i| i.name.as_str()).unwrap_or("Skylander");
                            s.active_prompt = Some(format!(
                                "{name} has been defeated! Place a different Skylander on the Portal."
                            ));
                        }
                        commands.entity(entity).despawn_recursive();
                    } else if is_enemy {
                        // `LootSystem.ai` "Creature Death Generate Loot": `LootValue = 5`.
                        // ponytail: fixed drop; real one rolls rarity/drop-rate in the script VM.
                        commands.spawn((
                            LevelEntity,
                            Collectible {
                                value: 5,
                                collected: false,
                            },
                            Transform::from_translation(pos),
                            CharacterCollider {
                                radius: 0.5,
                                height: 0.5,
                                step_height: 0.0,
                                grounded: true,
                            },
                        ));
                        commands.entity(entity).despawn_recursive();
                    } else if let Some(prop) = maybe_prop {
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
                            commands.entity(entity).despawn_recursive();
                        }
                    } else if maybe_villain.is_some() {
                        let has_trap = gameplay_state
                            .as_ref()
                            .is_some_and(|gs| gs.tags.iter().any(|t| t.is_trap));
                        if has_trap && capture_progress.is_some() && next_state.is_some() {
                            let char_id = maybe_ident.map(|i| i.character_id).unwrap_or(1001);
                            if let Some(ref mut cp) = capture_progress {
                                cp.villain_id = char_id;
                                cp.progress = 0.0;
                                cp.duration = 2.0;
                                cp.target_position = pos;
                            }
                            if let Some(ref mut ns) = next_state {
                                ns.set(GameState::VillainCapture);
                            }
                            let (_, elem) = get_character_info(char_id);
                            let (r, g, b) = elem.rgb();
                            if let Some(ref mut p) = portal {
                                p.set_trap_light(r, g, b);
                            }
                            if let Some(ref mut stream) = audio_streaming {
                                stream.sink.queue_villain_speech(char_id, 2.0);
                                while let Some(pkt) = stream.sink.pop_packet() {
                                    stream.queue.push_back(pkt);
                                }
                            }
                            commands.entity(entity).despawn_recursive();
                        } else {
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
                            if let Some(ref mut s) = stats {
                                s.active_prompt =
                                    Some("Insert a Trap to capture Villain!".to_string());
                            }
                            commands.entity(entity).despawn_recursive();
                        }
                    }
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
                commands.entity(entity).despawn_recursive();
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

/// Hero distance for the NPC talk prompt. Unverified against the DOL.
pub const NPC_TALK_RADIUS: f32 = 3.0;

/// Talk prompt for the nearest NPC with something to say (ties go to entity order),
/// unless a zone prompt already shows. A fresh A press starts that NPC's next
/// conversation, each further press shows the next line, and the last one closes it.
/// Walking out of range also closes it. The hero cannot move or attack meanwhile.
pub fn npc_interaction_system(
    input: Res<WiiInputState>,
    mut stats: ResMut<GameStats>,
    mut dialogue: ResMut<ActiveDialogue>,
    mut events: EventWriter<NpcInteractEvent>,
    mut a_was_down: Local<bool>,
    hero_query: Query<&Transform, With<Hero>>,
    mut npc_query: Query<(Entity, &Transform, &mut Npc)>,
) {
    let a_down = input.is_down(WiiButton::A);
    let a_pressed = a_down && !*a_was_down;
    *a_was_down = a_down;

    let Ok(hero_tf) = hero_query.get_single() else {
        dialogue.close();
        return;
    };

    if let Some(entity) = dialogue.npc {
        let in_range = npc_query.get(entity).is_ok_and(|(_, tf, npc)| {
            hero_tf.translation.distance(tf.translation) <= npc.talk_radius
        });
        if !in_range {
            dialogue.close();
        } else {
            if a_pressed {
                dialogue.line += 1;
                if dialogue.line >= dialogue.lines.len() {
                    dialogue.close();
                }
            }
            // The dialogue box replaces the prompt banner.
            stats.active_prompt = None;
            return;
        }
    }

    if stats.active_prompt.is_some() {
        return;
    }
    let mut nearest: Option<(f32, Entity)> = None;
    for (entity, tf, npc) in &npc_query {
        let dist = hero_tf.translation.distance(tf.translation);
        if !npc.conversations.is_empty()
            && dist <= npc.talk_radius
            && nearest.is_none_or(|(d, _)| dist < d)
        {
            nearest = Some((dist, entity));
        }
    }
    let Some((_, entity)) = nearest else {
        return;
    };
    let Ok((_, _, mut npc)) = npc_query.get_mut(entity) else {
        return;
    };
    if !a_pressed {
        stats.active_prompt = Some(format!("Press A to talk to {}", npc.name));
        return;
    }
    let index = npc.next_conversation % npc.conversations.len();
    npc.next_conversation = index + 1;
    *dialogue = ActiveDialogue {
        npc: Some(entity),
        npc_name: npc.name.clone(),
        lines: npc.conversations[index].lines.clone(),
        line: 0,
    };
    events.send(NpcInteractEvent {
        npc: entity,
        name: npc.name.clone(),
    });
}

/// Logical animation state per actor. Clip names are resolved to real clips by the renderer.
pub fn animation_state_name(
    combo: Option<&AttackCombo>,
    enemy: Option<&Enemy>,
    velocity: Option<&Velocity>,
    collider: Option<&CharacterCollider>,
) -> String {
    if let Some(attack) = combo.and_then(|c| c.active.as_ref().map(|a| (c.step, a))) {
        return match attack.1.kind {
            AttackKind::PrimaryMelee => format!("attack_combo_{}", attack.0.max(1)),
            AttackKind::SecondarySweep => "attack_sweep".to_string(),
            AttackKind::SpecialProjectile => "attack_projectile".to_string(),
            AttackKind::PrimaryBreath => "attack_breath".to_string(),
        };
    }
    if enemy.is_some_and(|e| e.state == EnemyState::Windup) {
        return "attack".to_string();
    }
    let vel = velocity.map(|v| v.0).unwrap_or(Vec3::ZERO);
    if collider.is_some_and(|c| !c.grounded) {
        return if vel.y > 0.0 { "jump" } else { "fall" }.to_string();
    }
    let h_speed = Vec2::new(vel.x, vel.z).length();
    if h_speed > 3.0 {
        "run"
    } else if h_speed > 0.1 {
        "walk"
    } else {
        "idle"
    }
    .to_string()
}

#[allow(clippy::type_complexity)]
pub fn animation_state_system(
    time: Res<Time<Fixed>>,
    mut query: Query<(
        &mut ActorAnimation,
        Option<&AttackCombo>,
        Option<&Enemy>,
        Option<&Velocity>,
        Option<&CharacterCollider>,
        Option<&mut PortalSpawn>,
    )>,
    magic_moment: Option<Res<MagicMoment>>,
) {
    // The in-world entrance plays when the Magic Moment hands back to the world.
    let in_portal_space = magic_moment.is_some_and(|m| m.is_active());
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };

    for (mut anim, combo, enemy, vel, collider, spawn) in &mut query {
        anim.time += dt * anim.speed;

        let next_clip = match spawn {
            Some(mut s) if s.remaining > 0.0 => {
                if !in_portal_space {
                    s.remaining -= dt;
                }
                "spawn".to_string()
            }
            _ => animation_state_name(combo, enemy, vel, collider),
        };
        if anim.current_clip.as_deref() != Some(&next_clip) {
            anim.current_clip = Some(next_clip);
            anim.time = 0.0;
        }
    }
}

/// Length of the character's `ToyAlive` clip (animation state 80, the Magic Moment reveal).
pub fn toy_alive_duration(char_id: u16, asset_server: Option<&GameAssetServer>) -> Option<f32> {
    let (model_path, _) = get_character_asset_paths(char_id);
    let model = asset_server?
        .loader
        .lock()
        .ok()?
        .load_igb(model_path)
        .ok()?;
    model.animation_for_state(80).map(|clip| clip.duration)
}

pub fn magic_moment_system(time: Res<Time<Fixed>>, mut magic_moment: ResMut<MagicMoment>) {
    if !magic_moment.is_active() {
        return;
    }
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };
    magic_moment.elapsed += dt;
}

pub fn portal_audio_stream_system(
    mut portal: ResMut<ActivePortal>,
    mut streaming: ResMut<PortalAudioStreaming>,
) {
    if let Some(pkt) = streaming.queue.pop_front() {
        portal.send_audio(&pkt);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn villain_capture_system(
    mut commands: Commands,
    time: Option<Res<Time<Fixed>>>,
    current_state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut capture_progress: ResMut<CaptureProgress>,
    mut trapped_state: ResMut<TrappedVillainState>,
    mut portal: ResMut<ActivePortal>,
    mut audio_events: EventWriter<WorldAudioCueEvent>,
    mut stats: ResMut<GameStats>,
    villain_actors: Query<(Entity, &ActorIdentity), (With<Villain>, Without<ControlledVillain>)>,
) {
    if *current_state.get() != GameState::VillainCapture {
        return;
    }

    let dt = time
        .as_ref()
        .map(|t| {
            if t.delta_secs() > 0.0 {
                t.delta_secs()
            } else {
                t.timestep().as_secs_f32()
            }
        })
        .unwrap_or(1.0 / 60.0);

    let speed = if capture_progress.duration > 0.0 {
        1.0 / capture_progress.duration
    } else {
        0.5
    };
    capture_progress.progress = (capture_progress.progress + dt * speed).min(1.0);

    let (_, elem) = get_character_info(capture_progress.villain_id);
    let (r, g, b) = elem.rgb();
    let pulse = (capture_progress.progress * std::f32::consts::PI * 6.0)
        .sin()
        .abs()
        * 0.5
        + 0.5;
    portal.set_trap_light(
        (r as f32 * pulse) as u8,
        (g as f32 * pulse) as u8,
        (b as f32 * pulse) as u8,
    );

    if capture_progress.progress >= 1.0 {
        trapped_state.is_unlocked = true;
        trapped_state.trapped_villain_id = Some(capture_progress.villain_id);
        trapped_state.energy.current = trapped_state.energy.max;
        portal.set_trap_light(r, g, b);

        for (ent, _) in &villain_actors {
            commands.entity(ent).despawn_recursive();
        }
        audio_events.send(WorldAudioCueEvent(WorldAudioCue::VillainTrapped));
        let (name, _) = get_character_info(capture_progress.villain_id);
        stats.active_prompt = Some(format!("Captured {} in Trap! Press Minus to Swap.", name));
        next_state.set(GameState::Hub);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn state_transition_update_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    current_state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut transition_progress: ResMut<ZoneTransitionProgress>,
    mut stats: ResMut<GameStats>,
    mut hero_query: Query<&mut Transform, With<Hero>>,
    mut active_level: Option<ResMut<ActiveLevel>>,
    mut campaign_progress: Option<ResMut<CampaignProgress>>,
    mut env_col: Option<ResMut<EnvironmentCollision>>,
    asset_server: Option<Res<GameAssetServer>>,
    level_entities_query: Query<Entity, With<LevelEntity>>,
    mut level_changed_events: EventWriter<LevelChangedEvent>,
    mut gameplay_state: Option<ResMut<PortalGameplayState>>,
) {
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };

    match current_state.get() {
        GameState::VillainCapture => {}
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
                    if let Some(ref mut gs) = gameplay_state {
                        gs.defeated_heroes.clear();
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
    let mut scripts: Option<Option<LevelScripts>> = None;
    let mut marker_id_counter = 5000u32;
    let mut has_spawn = false;
    let mut tuning: Option<EnemyTuning> = None;
    let mut level_text: Option<Option<skylands_formats::LevelText>> = None;

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
                let script = marker
                    .script
                    .as_deref()
                    .filter(|_| marker.animated)
                    .and_then(|name| {
                        let program = scripts.get_or_insert_with(|| level_scripts(loader, &path));
                        Some((program.as_ref()?.0.find(name)?, program.clone()?))
                    });
                let base = Transform::from_matrix(Mat4::from_cols_array(&marker.world_transform));
                let mut prop = commands.spawn((
                    LevelEntity,
                    ActorIdentity {
                        id: marker_id_counter,
                        character_id: 0,
                        name: marker.name.clone(),
                        is_villain: false,
                        model_path: None,
                        texture_path: None,
                    },
                    // Script-animated scenery (windmills, birds) cannot be smashed.
                    Prop {
                        destructible: !marker.animated,
                    },
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
                    ActorAnimation::default(),
                ));
                if marker.drawn_by_level {
                    prop.insert(LevelDrawnModel);
                }
                if let Some((si, _)) = script {
                    let mut state = skylands_formats::ActorScriptState::default();
                    use skylands_formats::script_runtime::field;
                    state.set(&[field::MY, field::SCALE, field::UNIFORM], 100.0);
                    prop.insert((
                        ActorScript {
                            script: si,
                            marker_node: marker.node_index,
                            state,
                            base,
                        },
                        base,
                    ));
                }
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
            skylands_formats::igb::MarkerKind::Enemy(class) => {
                let stats = tuning
                    .get_or_insert_with(|| level_enemy_tuning(loader, &path))
                    .get(class);
                spawn_enemy(
                    commands,
                    marker_id_counter,
                    &marker.name,
                    class,
                    stats,
                    Vec3::from(marker.position),
                );
                marker_id_counter += 1;
            }
            skylands_formats::igb::MarkerKind::Npc => {
                let name = marker
                    .script
                    .as_deref()
                    .and_then(skylands_formats::npc_name)
                    .unwrap_or_else(|| marker.name.clone());
                let script = marker.script.as_deref().and_then(|sname| {
                    let program = scripts.get_or_insert_with(|| level_scripts(loader, &path));
                    Some((program.as_ref()?.0.find(sname)?, program.clone()?))
                });
                let base = Transform::from_matrix(Mat4::from_cols_array(&marker.world_transform));
                let mut npc = commands.spawn((
                    LevelEntity,
                    ActorIdentity {
                        id: marker_id_counter,
                        character_id: 0,
                        name: marker.name.clone(),
                        is_villain: false,
                        model_path: None,
                        texture_path: None,
                    },
                    Npc {
                        conversations: marker
                            .script
                            .as_deref()
                            .map(|script| {
                                let text = level_text
                                    .get_or_insert_with(|| load_level_text(loader, &path));
                                npc_dialogue(loader, &path, text.as_ref(), script, &name)
                            })
                            .unwrap_or_default(),
                        name,
                        talk_radius: NPC_TALK_RADIUS,
                        next_conversation: 0,
                        marker_node: Some(marker.node_index),
                    },
                    Transform::from_matrix(Mat4::from_cols_array(&marker.world_transform)),
                    ActorAnimation::default(),
                ));
                if marker.drawn_by_level {
                    npc.insert(LevelDrawnModel);
                }
                if let Some((si, _)) = script {
                    let mut state = skylands_formats::ActorScriptState::default();
                    use skylands_formats::script_runtime::field;
                    state.set(&[field::MY, field::SCALE, field::UNIFORM], 100.0);
                    npc.insert((
                        ActorScript {
                            script: si,
                            marker_node: marker.node_index,
                            state,
                            base,
                        },
                        base,
                    ));
                }
                marker_id_counter += 1;
            }
            _ => {}
        }
    }

    match scripts.flatten() {
        Some(s) => commands.insert_resource(s),
        None => commands.remove_resource::<LevelScripts>(),
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

/// The level's strings: `level.bld` `StringInfo` objects filled from the language pack
/// in the same archive (`<Level>/ENGLISH.pak`, EU en).
fn load_level_text(
    loader: &skylands_formats::AssetLoader,
    level_path: &str,
) -> Option<skylands_formats::LevelText> {
    let stem = level_path.strip_suffix(".bld").unwrap_or(level_path);
    let pak = loader.load_raw(&format!("{stem}/ENGLISH.pak")).ok()?;
    let pack = skylands_formats::parse_language_pack(&pak)?;
    skylands_formats::LevelText::new(&loader.load_raw(level_path).ok()?, &pack)
}

fn npc_dialogue(
    loader: &skylands_formats::AssetLoader,
    level_path: &str,
    text: Option<&skylands_formats::LevelText>,
    script: &str,
    name: &str,
) -> Vec<skylands_formats::Conversation> {
    let (Some(text), Ok(level)) = (text, loader.load_raw(level_path)) else {
        return Vec::new();
    };
    let Some(set) = skylands_formats::ScriptSet::parse(&level) else {
        return Vec::new();
    };
    let file = script.rsplit(['/', '\\']).next().unwrap_or(script);
    skylands_formats::npc_dialogue(&level, &set, text, file, name)
}

/// The level's scripts with `global.bld` for global handles.
fn level_scripts(loader: &skylands_formats::AssetLoader, level_path: &str) -> Option<LevelScripts> {
    let parse = |p: &str| skylands_formats::ScriptSet::parse(&loader.load_raw(p).ok()?);
    let program = skylands_formats::ScriptProgram::new(parse(level_path)?, parse("global.bld"));
    Some(LevelScripts(std::sync::Arc::new(program)))
}

/// One script tick per `ActorScript`, then its engine state is applied to the actor:
/// `orientation` angles (degrees, relative to the placement; facing = yaw about +Y,
/// tilt = X, roll = Z, composed `Rz * Ry * Rx` like placements), `vertical offset`,
/// `OpDisplace` and `scale.uniform`. `OpRemove my` despawns the actor.
#[allow(clippy::type_complexity)]
pub fn actor_script_system(
    mut commands: Commands,
    time: Res<Time>,
    scripts: Option<Res<LevelScripts>>,
    dialogue: Option<Res<ActiveDialogue>>,
    mut actors: Query<(
        Entity,
        &mut ActorScript,
        &mut Transform,
        Option<&mut ActorAnimation>,
        Option<&Npc>,
    )>,
) {
    use skylands_formats::script_runtime::field::*;
    let Some(scripts) = scripts else { return };
    let dt = time.delta_secs();
    let talking_npc = dialogue.as_ref().and_then(|d| d.npc);
    for (entity, mut actor, mut transform, mut anim, npc) in &mut actors {
        let actor = &mut *actor;
        scripts.0.tick(actor.script, &mut actor.state, dt);
        let st = &actor.state;
        if st.removed {
            commands.entity(entity).despawn_recursive();
            continue;
        }
        let angle = |f| st.get(&[MY, ORIENTATION, f]).unwrap_or(0.0).to_radians();
        let turn = Quat::from_euler(EulerRot::ZYX, angle(ROLL), angle(FACING), angle(TILT));
        let lift = Vec3::Y * st.get(&[MY, VERTICAL_OFFSET]).unwrap_or(0.0);
        let scale = st.get(&[MY, SCALE, UNIFORM]).unwrap_or(100.0) / 100.0;
        transform.translation = actor.base.translation + lift + Vec3::from(st.displacement);
        transform.rotation = actor.base.rotation * turn;
        transform.scale = actor.base.scale * scale;

        if let Some(ref mut anim) = anim {
            if npc.is_some() && talking_npc == Some(entity) {
                // Talk state: slot 9 or 151 or requested in script, fallback to current
                anim.state = if st.anim_state != Some(0) {
                    st.anim_state
                } else {
                    Some(9)
                };
            } else {
                anim.state = st.anim_state;
            }
        }
    }
}

/// Enemy stats from the level's scripts and `global.bld`; defaults when either is
/// missing or the evaluation fails (hub levels have no "Game - Define Enemy Stats").
fn level_enemy_tuning(loader: &skylands_formats::AssetLoader, level_path: &str) -> EnemyTuning {
    let parse = |p: &str| {
        let bytes = loader.load_raw(p).ok()?;
        skylands_formats::ScriptSet::parse(&bytes)
    };
    parse(level_path)
        .zip(parse("global.bld"))
        .and_then(|(level, global)| EnemyTuning::from_scripts(&level, &global).ok())
        .unwrap_or_default()
}

pub fn spawn_enemy(
    commands: &mut Commands,
    id: u32,
    name: &str,
    class: EnemyClass,
    stats: EnemyClassStats,
    pos: Vec3,
) -> Entity {
    commands
        .spawn((
            LevelEntity,
            ActorIdentity {
                id,
                character_id: 0,
                name: name.to_string(),
                is_villain: false,
                model_path: None,
                texture_path: None,
            },
            Enemy::new(class, stats.damage),
            Health {
                current: stats.health,
                max: stats.health,
            },
            Transform::from_translation(pos),
            CharacterCollider::default(),
            Velocity::default(),
            ModelParameters::default(),
            ActorAnimation::default(),
        ))
        .id()
}

/// Per-tick enemy brain: Idle until a hero enters `aggro_range`, Chase into
/// `attack_range`, Windup, then strike (melee hit or hostile projectile) and Recover.
/// Target = nearest hero, ties broken by entity order, so runs are deterministic.
#[allow(clippy::type_complexity)]
pub fn enemy_ai_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut damage_events: EventWriter<DamageEvent>,
    heroes: Query<(Entity, &Transform), (With<Hero>, Without<Enemy>)>,
    mut enemies: Query<(
        &mut Enemy,
        &mut Transform,
        &mut Velocity,
        &CharacterCollider,
        &ModelParameters,
    )>,
) {
    let dt = if time.delta_secs() > 0.0 {
        time.delta_secs()
    } else {
        time.timestep().as_secs_f32()
    };
    let mut hero_list: Vec<(Entity, Vec3)> =
        heroes.iter().map(|(e, t)| (e, t.translation)).collect();
    hero_list.sort_by_key(|(e, _)| *e);

    for (mut enemy, mut tf, mut vel, collider, params) in &mut enemies {
        let pos = tf.translation;
        let target = hero_list
            .iter()
            .map(|&(e, p)| (e, p, Vec2::new(p.x - pos.x, p.z - pos.z)))
            .min_by(|a, b| a.2.length().total_cmp(&b.2.length()));
        let Some((target, target_pos, to_target)) = target else {
            enemy.state = EnemyState::Idle;
            vel.0.x = 0.0;
            vel.0.z = 0.0;
            continue;
        };
        let dist = to_target.length();
        let dir = to_target.normalize_or_zero();
        let in_range = dist <= enemy.attack_range + collider.radius;

        enemy.timer = (enemy.timer - dt).max(0.0);
        enemy.state = match enemy.state {
            EnemyState::Idle | EnemyState::Chase if dist > enemy.aggro_range => EnemyState::Idle,
            EnemyState::Idle | EnemyState::Chase if in_range => {
                enemy.timer = Enemy::WINDUP;
                EnemyState::Windup
            }
            EnemyState::Idle | EnemyState::Chase => EnemyState::Chase,
            EnemyState::Windup if enemy.timer > 0.0 => EnemyState::Windup,
            EnemyState::Windup => {
                match enemy.class {
                    EnemyClass::Near if in_range => {
                        damage_events.send(DamageEvent {
                            target,
                            damage: enemy.damage,
                            position: target_pos,
                        });
                    }
                    EnemyClass::Near => {}
                    EnemyClass::Far => {
                        commands.spawn((
                            Projectile {
                                lifetime: 0.0,
                                max_lifetime: enemy.attack_range * 1.5 / Enemy::PROJECTILE_SPEED,
                                damage: enemy.damage,
                                radius: 0.5,
                                hostile: true,
                                owner: None,
                                piercing: false,
                            },
                            Velocity(Vec3::new(dir.x, 0.0, dir.y) * Enemy::PROJECTILE_SPEED),
                            Transform::from_translation(pos + Vec3::new(0.0, 0.8, 0.0)),
                        ));
                    }
                }
                enemy.timer = Enemy::RECOVER;
                EnemyState::Recover
            }
            EnemyState::Recover if enemy.timer > 0.0 => EnemyState::Recover,
            EnemyState::Recover => EnemyState::Chase,
        };

        let target_h = if enemy.state == EnemyState::Chase {
            dir * params.ground_max_speed
        } else {
            Vec2::ZERO
        };
        let current_h = Vec2::new(vel.0.x, vel.0.z);
        let diff = target_h - current_h;
        let max_step = params.ground_accel * dt;
        let new_h = if diff.length() <= max_step {
            target_h
        } else {
            current_h + diff.normalize() * max_step
        };
        vel.0.x = new_h.x;
        vel.0.z = new_h.y;
        if dir != Vec2::ZERO && enemy.state != EnemyState::Idle {
            tf.rotation = Quat::from_rotation_y((-dir.x).atan2(-dir.y));
        }
    }
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
    fn test_hero_movement_with_model_parameters() {
        let mut app = build_test_app();

        let params = ModelParameters::default();
        let hero_id = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 16,
                    character_id: 16,
                    name: "Spyro".into(),
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
                MovementSpeed(params.ground_max_speed),
                params,
                Health::default(),
                AttackCombo::default(),
                ActorAnimation::default(),
            ))
            .id();

        // 1. First tick accelerates towards stick
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .set_stick(1.0, 0.0);
        step_fixed(&mut app);

        let vel1 = app.world().get::<Velocity>(hero_id).unwrap().0;
        assert!(vel1.x > 0.0 && vel1.x < params.ground_max_speed);

        // 2. Run enough ticks to reach max speed
        for _ in 0..20 {
            step_fixed(&mut app);
        }
        let vel_max = app.world().get::<Velocity>(hero_id).unwrap().0;
        assert!((vel_max.x - params.ground_max_speed).abs() < 1e-3);

        // 3. Releasing stick decelerates smoothly
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .set_stick(0.0, 0.0);
        step_fixed(&mut app);
        let vel_decel = app.world().get::<Velocity>(hero_id).unwrap().0;
        assert!(vel_decel.x < params.ground_max_speed);
        assert!(vel_decel.x > 0.0);

        for _ in 0..20 {
            step_fixed(&mut app);
        }
        let vel_stopped = app.world().get::<Velocity>(hero_id).unwrap().0;
        assert!(vel_stopped.x.abs() < 1e-4);

        // 4. Jump uses ModelParameters.jump_initial_speed
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::A);
        step_fixed(&mut app);
        let vel_jump = app.world().get::<Velocity>(hero_id).unwrap().0;
        assert!((vel_jump.y - params.jump_initial_speed).abs() < 1e-3);
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
                Transform::from_xyz(0.0, 0.0, 1.6),
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
            tf.translation = Vec3::new(0.0, 0.0, 1.6);
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
                Transform::from_xyz(0.0, 0.0, 1.0),
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
            ProjectileParams {
                speed: 40.0,
                lifetime: 0.35,
                primary_style: PrimaryAttackStyle::Melee,
            },
            ActorAnimation::default(),
        ));
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::One);
        step_fixed(&mut app);

        let mut q = app.world_mut().query::<(&Projectile, &Velocity)>();
        let spawned: Vec<(u32, f32, f32)> = q
            .iter(app.world())
            .map(|(p, v)| (p.damage, p.max_lifetime, v.0.length()))
            .collect();
        assert_eq!(spawned.len(), 1);
        let (damage, lifetime, speed) = spawned[0];
        assert_eq!(damage, 13);
        assert_eq!(lifetime, 0.35);
        assert!((speed - 40.0).abs() < 1e-3, "speed {speed}");
    }

    #[test]
    fn test_ranged_primary_fires_projectile() {
        let mut app = build_test_app();
        let hero = app
            .world_mut()
            .spawn((
                Hero,
                Transform::from_xyz(0.0, 0.0, 0.0),
                CharacterCollider::default(),
                Velocity::default(),
                MovementSpeed(5.0),
                AttackCombo::default(),
                AttackDamage {
                    primary: 20.0,
                    secondary: 7.0,
                    special: 12.0,
                },
                ProjectileParams {
                    speed: 45.0,
                    lifetime: 1.0,
                    primary_style: PrimaryAttackStyle::Projectile,
                },
                ActorAnimation::default(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::B);
        step_fixed(&mut app);

        let mut q = app.world_mut().query::<(&Projectile, &Velocity)>();
        let spawned: Vec<(u32, Option<Entity>, Vec3)> = q
            .iter(app.world())
            .map(|(p, v)| (p.damage, p.owner, v.0))
            .collect();
        assert_eq!(spawned.len(), 1);
        assert_eq!((spawned[0].0, spawned[0].1), (20, Some(hero)));
        // Facing = rotated +Z (hero_movement_system turns +Z toward the stick).
        assert!(spawned[0].2.z > 44.0, "fired backwards: {:?}", spawned[0].2);
    }

    #[test]
    fn test_breath_primary_fires_piercing_burst() {
        let mut app = build_test_app();
        let hero = app
            .world_mut()
            .spawn((
                Hero,
                Transform::from_xyz(0.0, 0.0, 0.0),
                CharacterCollider::default(),
                Velocity::default(),
                MovementSpeed(5.0),
                AttackCombo::default(),
                AttackDamage {
                    primary: 15.0,
                    secondary: 10.0,
                    special: 12.0,
                },
                ProjectileParams {
                    speed: 40.0 * crate::components::FEET_TO_UNITS,
                    lifetime: 0.35,
                    primary_style: PrimaryAttackStyle::Breath,
                },
                ActorAnimation::default(),
            ))
            .id();

        // 2 villains in line with enough HP to survive multiple frame contacts
        let v1 = app
            .world_mut()
            .spawn((
                Villain,
                Health {
                    current: 500,
                    max: 500,
                },
                CharacterCollider {
                    radius: 0.5,
                    height: 1.0,
                    step_height: 0.1,
                    grounded: true,
                },
                Transform::from_xyz(0.0, 0.8, 1.5),
            ))
            .id();
        let v2 = app
            .world_mut()
            .spawn((
                Villain,
                Health {
                    current: 500,
                    max: 500,
                },
                CharacterCollider {
                    radius: 0.5,
                    height: 1.0,
                    step_height: 0.1,
                    grounded: true,
                },
                Transform::from_xyz(0.0, 0.8, 2.8),
            ))
            .id();

        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::B);
        step_fixed(&mut app);

        // Breath projectile spawned and marked piercing
        let mut q = app.world_mut().query::<&Projectile>();
        let proj = q
            .iter(app.world())
            .next()
            .expect("breath projectile spawned");
        assert!(proj.piercing);
        assert_eq!(proj.owner, Some(hero));

        // Advance enough ticks for projectile to travel past both villains (~15 ticks at ~0.2u/tick)
        for _ in 0..16 {
            step_fixed(&mut app);
        }

        let h1 = app.world().get::<Health>(v1).unwrap().current;
        let h2 = app.world().get::<Health>(v2).unwrap().current;
        assert!(h1 < 500, "v1 took damage: {h1}");
        assert!(h2 < 500, "v2 took damage: {h2}");
    }

    #[test]
    fn test_melee_primary_character_does_not_fire_projectile() {
        let mut app = build_test_app();
        app.world_mut().spawn((
            Hero,
            Transform::from_xyz(0.0, 0.0, 0.0),
            CharacterCollider::default(),
            Velocity::default(),
            MovementSpeed(5.0),
            AttackCombo::default(),
            AttackDamage {
                primary: 25.0,
                secondary: 10.0,
                special: 12.0,
            },
            ProjectileParams {
                speed: 40.0,
                lifetime: 1.0,
                primary_style: PrimaryAttackStyle::Melee,
            },
            ActorAnimation::default(),
        ));
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::B);
        step_fixed(&mut app);

        let mut q = app.world_mut().query::<&Projectile>();
        assert_eq!(
            q.iter(app.world()).count(),
            0,
            "melee primary should not spawn projectile"
        );
    }

    #[test]
    fn test_projectile_spawn_and_hit() {
        let mut app = build_test_app();

        // Spawn Hero facing forward (+Z)
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
                Transform::from_xyz(0.0, 0.0, 3.0),
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
    fn test_portal_tag_plays_spawn_animation_and_locks_input() {
        let mut app = build_test_app();
        app.world_mut()
            .resource_mut::<Events<PortalEvent>>()
            .send(PortalEvent::TagArrived {
                tag_index: 1,
                zone: PortalZone::LeftRing,
            });
        step_fixed(&mut app);

        let hero = app
            .world_mut()
            .query_filtered::<Entity, With<Hero>>()
            .single(app.world());
        assert!(app.world().get::<PortalSpawn>(hero).is_some());
        assert_eq!(
            app.world()
                .get::<ActorAnimation>(hero)
                .unwrap()
                .current_clip
                .as_deref(),
            Some("spawn")
        );

        // Input is ignored while the entrance plays.
        app.world_mut().resource_mut::<WiiInputState>().stick = [0.0, 1.0];
        step_fixed(&mut app);
        assert_eq!(app.world().get::<Velocity>(hero).unwrap().0.x, 0.0);
        assert_eq!(app.world().get::<Velocity>(hero).unwrap().0.z, 0.0);

        // The in-world entrance waits for the Magic Moment to hand back to the world.
        while app.world().resource::<MagicMoment>().is_active() {
            step_fixed(&mut app);
        }
        let spawn = |app: &App| app.world().get::<PortalSpawn>(hero).unwrap().remaining;
        // At most the tick that ends the Magic Moment has counted.
        assert!(
            spawn(&app) >= PortalSpawn::DURATION - 1.5 / 60.0,
            "{}",
            spawn(&app)
        );

        // One tick to run out (float rounding), one to switch clip.
        let ticks = (PortalSpawn::DURATION * 60.0).ceil() as usize + 2;
        for _ in 0..ticks {
            step_fixed(&mut app);
        }
        assert_ne!(
            app.world()
                .get::<ActorAnimation>(hero)
                .unwrap()
                .current_clip
                .as_deref(),
            Some("spawn")
        );
    }

    #[test]
    fn test_portal_tag_starts_magic_moment() {
        let mut app = build_test_app();
        app.world_mut()
            .resource_mut::<Events<PortalEvent>>()
            .send(PortalEvent::TagArrived {
                tag_index: 1,
                zone: PortalZone::LeftRing,
            });
        step_fixed(&mut app);

        let mm = app.world().resource::<MagicMoment>().clone();
        assert!(mm.is_active());
        assert_eq!(mm.character_id, Some(16));
        assert_eq!(mm.name, "Spyro");
        assert_eq!(mm.serial, 1);
        // No asset server: the default `ToyAlive` length.
        assert_eq!(mm.toy_alive, MM_DEFAULT_TOY_ALIVE);
        assert!((mm.duration() - (MM_ALIVE + MM_DEFAULT_TOY_ALIVE + MM_HYPER_JUMP)).abs() < 1e-5);

        // Hero input stays locked until the Magic Moment is over.
        let hero = app
            .world_mut()
            .query_filtered::<Entity, With<Hero>>()
            .single(app.world());
        app.world_mut().resource_mut::<WiiInputState>().stick = [0.0, 1.0];
        for _ in 0..(MM_ALIVE * 60.0) as usize {
            step_fixed(&mut app);
        }
        assert_eq!(app.world().get::<Velocity>(hero).unwrap().0.z, 0.0);

        let ticks = (mm.duration() * 60.0).ceil() as usize;
        for _ in 0..ticks {
            step_fixed(&mut app);
        }
        assert!(!app.world().resource::<MagicMoment>().is_active());
    }

    #[test]
    fn test_magic_moment_stage_times_follow_controller_script() {
        // mmsky4_controller.ai stage timers: 0.5, 1.2, 1.0, 0.42, 0.25 + 0.15, 0.3.
        assert_eq!(MM_PORTAL_SPACE, 0.5);
        assert!((MM_SHOW_MINION - 1.7).abs() < 1e-5);
        assert!((MM_REVEAL - 2.7).abs() < 1e-5);
        assert!((MM_TUNNEL_TINT - 3.12).abs() < 1e-5);
        assert!((MM_BACKGROUND - 3.52).abs() < 1e-5);
        assert!((MM_ALIVE - 3.82).abs() < 1e-5);
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

    #[test]
    fn test_villain_tag_team_swap_and_manual_return() {
        let mut app = build_test_app();

        // 1. Setup trapped villain ready in trap
        {
            let mut trapped_state = app.world_mut().resource_mut::<TrappedVillainState>();
            trapped_state.is_unlocked = true;
            trapped_state.trapped_villain_id = Some(1010);
            trapped_state.energy.current = 100.0;
        }

        // Spawn Hero at (5, 0, 5)
        let hero_ent = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 16,
                    character_id: 16,
                    name: "Spyro".into(),
                    is_villain: false,
                    model_path: None,
                    texture_path: None,
                },
                Hero,
                Transform::from_xyz(5.0, 0.0, 5.0),
                CharacterCollider::default(),
                Velocity::default(),
                MovementSpeed(5.0),
                Health::default(),
                AttackCombo::default(),
                ActorAnimation::default(),
            ))
            .id();
        // Render-side children (skeleton joints, skinned submeshes) go with the actor.
        let joint = app.world_mut().spawn(Transform::default()).id();
        app.world_mut().entity_mut(hero_ent).add_child(joint);

        // Press Minus to swap into Kaos
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::Minus);
        step_fixed(&mut app);

        // Previous hero entity is despawned
        assert!(app.world().get_entity(hero_ent).is_err());
        assert!(app.world().get_entity(joint).is_err(), "orphaned child");

        // Control mode is Villain
        let state = app.world().resource::<TrappedVillainState>();
        assert_eq!(state.control_mode, ActiveControlMode::Villain);

        // Active controlled villain spawned at the hero's position
        let mut villain_q = app
            .world_mut()
            .query_filtered::<(&Transform, &ActorIdentity), (With<Hero>, With<ControlledVillain>)>(
            );
        let (v_tf, v_ident) = villain_q.iter(app.world()).next().unwrap();
        assert_eq!(v_ident.character_id, 1010);
        assert_eq!(v_ident.name, "Kaos");
        assert_eq!(v_tf.translation, Vec3::new(5.0, 0.0, 5.0));

        // Advance past swap cooldown (0.5s = 30 ticks)
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .release(WiiButton::Minus);
        for _ in 0..35 {
            step_fixed(&mut app);
        }

        // Press Minus to swap back to Hero
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::Minus);
        step_fixed(&mut app);

        let state = app.world().resource::<TrappedVillainState>();
        assert_eq!(state.control_mode, ActiveControlMode::Hero);

        // Controlled villain entity despawned and hero entity restored
        let mut hero_q = app
            .world_mut()
            .query_filtered::<(&Transform, &ActorIdentity), (With<Hero>, Without<ControlledVillain>)>();
        let (h_tf, h_ident) = hero_q.iter(app.world()).next().unwrap();
        assert_eq!(h_ident.character_id, 16);
        assert_eq!(h_tf.translation, Vec3::new(5.0, 0.0, 5.0));
    }

    #[test]
    fn test_villain_energy_drain_and_forced_trap_return() {
        let mut app = build_test_app();

        // Setup active controlled villain with low energy and rapid drain
        {
            let mut trapped_state = app.world_mut().resource_mut::<TrappedVillainState>();
            trapped_state.control_mode = ActiveControlMode::Villain;
            trapped_state.is_unlocked = true;
            trapped_state.trapped_villain_id = Some(1010);
            trapped_state.energy.current = 1.0;
            trapped_state.energy.drain_rate = 60.0; // drains to 0 on next tick
        }

        let villain_ent = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 2010,
                    character_id: 1010,
                    name: "Kaos".into(),
                    is_villain: true,
                    model_path: None,
                    texture_path: None,
                },
                Hero,
                ControlledVillain,
                Transform::from_xyz(2.0, 0.0, -4.0),
                CharacterCollider::default(),
                Velocity::default(),
                MovementSpeed(5.0),
                Health {
                    current: 1,
                    max: 100,
                },
                VillainEnergy {
                    current: 1.0,
                    max: 100.0,
                    drain_rate: 60.0,
                    recharge_rate: 5.0,
                },
                AttackCombo::default(),
                ActorAnimation::default(),
            ))
            .id();

        // Step one tick -> energy hits 0 -> triggers forced trap return
        step_fixed(&mut app);

        assert!(app.world().get_entity(villain_ent).is_err());

        let state = app.world().resource::<TrappedVillainState>();
        assert_eq!(state.control_mode, ActiveControlMode::Hero);
        assert_eq!(state.energy.current, 0.0);

        // Hero respawned at (2, 0, -4)
        let mut hero_q = app
            .world_mut()
            .query_filtered::<&Transform, (With<Hero>, Without<ControlledVillain>)>();
        let h_tf = hero_q.iter(app.world()).next().unwrap();
        assert_eq!(h_tf.translation, Vec3::new(2.0, 0.0, -4.0));

        // Audio streaming queued exhausted speech packet
        let audio = app.world().resource::<PortalAudioStreaming>();
        assert!(
            !audio.queue.is_empty(),
            "Speech packets queued on forced return"
        );
    }

    #[test]
    fn test_damage_drains_controlled_villain_energy() {
        let mut app = build_test_app();

        {
            let mut trapped_state = app.world_mut().resource_mut::<TrappedVillainState>();
            trapped_state.control_mode = ActiveControlMode::Villain;
            trapped_state.is_unlocked = true;
            trapped_state.trapped_villain_id = Some(1010);
            trapped_state.energy.current = 100.0;
            trapped_state.energy.drain_rate = 0.0; // isolate damage testing
        }

        let villain_ent = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 2010,
                    character_id: 1010,
                    name: "Kaos".into(),
                    is_villain: true,
                    model_path: None,
                    texture_path: None,
                },
                Hero,
                ControlledVillain,
                Transform::from_xyz(0.0, 0.0, 0.0),
                Health {
                    current: 100,
                    max: 100,
                },
                VillainEnergy::default(),
            ))
            .id();

        // Deal 40 damage to controlled villain
        app.world_mut()
            .resource_mut::<Events<DamageEvent>>()
            .send(DamageEvent {
                target: villain_ent,
                damage: 40,
                position: Vec3::ZERO,
            });

        step_fixed(&mut app);

        let state = app.world().resource::<TrappedVillainState>();
        assert_eq!(state.energy.current, 60.0);
    }

    #[test]
    fn test_in_trap_energy_recharging() {
        let mut app = build_test_app();

        {
            let mut trapped_state = app.world_mut().resource_mut::<TrappedVillainState>();
            trapped_state.control_mode = ActiveControlMode::Hero;
            trapped_state.is_unlocked = true;
            trapped_state.energy.current = 50.0;
            trapped_state.energy.recharge_rate = 10.0; // 10 units / sec
        }

        // Run 60 ticks (1.0 second)
        for _ in 0..60 {
            step_fixed(&mut app);
        }

        let state = app.world().resource::<TrappedVillainState>();
        assert!((state.energy.current - 60.0).abs() < 1e-2);
    }

    #[test]
    fn test_trap_tag_removal_forces_return_to_hero() {
        let mut app = build_test_app();

        {
            let mut trapped_state = app.world_mut().resource_mut::<TrappedVillainState>();
            trapped_state.control_mode = ActiveControlMode::Villain;
            trapped_state.is_unlocked = true;
            trapped_state.trapped_villain_id = Some(1010);
        }

        app.world_mut().spawn((
            ActorIdentity {
                id: 2010,
                character_id: 1010,
                name: "Kaos".into(),
                is_villain: true,
                model_path: None,
                texture_path: None,
            },
            Hero,
            ControlledVillain,
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        // Send TagRemoved event
        app.world_mut()
            .resource_mut::<Events<PortalEvent>>()
            .send(PortalEvent::TagRemoved { tag_index: 0 });

        step_fixed(&mut app);

        let state = app.world().resource::<TrappedVillainState>();
        assert_eq!(state.control_mode, ActiveControlMode::Hero);
        assert!(!state.is_unlocked);

        // Controlled villain despawned and hero restored
        let mut villain_q = app
            .world_mut()
            .query_filtered::<Entity, With<ControlledVillain>>();
        assert_eq!(villain_q.iter(app.world()).count(), 0);
    }

    #[test]
    fn test_enemy_defeat_trapping_sequence_and_unlock() {
        let mut app = build_test_app();

        // Place a mock trap tag on portal
        app.world_mut()
            .resource_mut::<PortalGameplayState>()
            .tags
            .push(SkylanderTag {
                uid: [0, 0, 0, 0],
                character_id: 1001,
                variant: 0,
                nickname: "Trap".to_string(),
                xp: 0,
                money: 0,
                is_trap: true,
                villain_id: None,
            });

        // Spawn an enemy villain with 30 HP
        let enemy_ent = app
            .world_mut()
            .spawn((
                ActorIdentity {
                    id: 1001,
                    character_id: 1001,
                    name: "Chompy Mage".into(),
                    is_villain: true,
                    model_path: None,
                    texture_path: None,
                },
                Villain,
                Transform::from_xyz(0.0, 0.0, -2.0),
                Health {
                    current: 30,
                    max: 30,
                },
                CharacterCollider::default(),
            ))
            .id();

        // Deal fatal 30 damage to enemy villain
        app.world_mut()
            .resource_mut::<Events<DamageEvent>>()
            .send(DamageEvent {
                target: enemy_ent,
                damage: 30,
                position: Vec3::new(0.0, 0.0, -2.0),
            });

        step_fixed(&mut app);

        // State transitioned to VillainCapture
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::VillainCapture);

        // Advance simulation until capture finishes (duration 2.0s = 120 ticks)
        for _ in 0..130 {
            step_fixed(&mut app);
        }

        // State returned to Hub and villain unlocked in trap
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::Hub);

        let trapped = app.world().resource::<TrappedVillainState>();
        assert!(trapped.is_unlocked);
        assert_eq!(trapped.trapped_villain_id, Some(1001));
        assert_eq!(trapped.energy.current, trapped.energy.max);
    }

    fn spawn_test_hero(app: &mut App, pos: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                ActorIdentity {
                    id: 16,
                    character_id: 16,
                    name: "Spyro".into(),
                    is_villain: false,
                    model_path: None,
                    texture_path: None,
                },
                Hero,
                Transform::from_translation(pos),
                CharacterCollider::default(),
                Velocity::default(),
                MovementSpeed(5.0),
                Health {
                    current: 100,
                    max: 100,
                },
                AttackCombo::default(),
                ActorAnimation::default(),
            ))
            .id()
    }

    fn spawn_test_enemy(app: &mut App, class: EnemyClass, pos: Vec3) -> Entity {
        let mut commands = app.world_mut().commands();
        let stats = EnemyTuning::default().get(class);
        let e = spawn_enemy(&mut commands, 9000, "Enemy_S4_Test", class, stats, pos);
        app.world_mut().flush();
        e
    }

    fn hero_health(app: &App, hero: Entity) -> u32 {
        app.world().get::<Health>(hero).map_or(0, |h| h.current)
    }

    #[test]
    fn test_melee_enemy_chases_and_hits_hero() {
        let mut app = build_test_app();
        let hero = spawn_test_hero(&mut app, Vec3::ZERO);
        let enemy = spawn_test_enemy(&mut app, EnemyClass::Near, Vec3::new(0.0, 0.0, -6.0));

        step_fixed(&mut app);
        assert_eq!(
            app.world().get::<Enemy>(enemy).unwrap().state,
            EnemyState::Chase
        );
        for _ in 0..240 {
            step_fixed(&mut app);
        }
        let z = app.world().get::<Transform>(enemy).unwrap().translation.z;
        assert!(z > -3.0, "enemy closed in, z = {z}");
        assert!(hero_health(&app, hero) < 100, "melee enemy hit the hero");
    }

    #[test]
    fn test_enemy_ignores_hero_outside_aggro_range() {
        let mut app = build_test_app();
        let hero = spawn_test_hero(&mut app, Vec3::ZERO);
        let enemy = spawn_test_enemy(&mut app, EnemyClass::Near, Vec3::new(0.0, 0.0, -30.0));
        for _ in 0..60 {
            step_fixed(&mut app);
        }
        assert_eq!(
            app.world().get::<Enemy>(enemy).unwrap().state,
            EnemyState::Idle
        );
        assert_eq!(hero_health(&app, hero), 100);
    }

    #[test]
    fn test_ranged_enemy_projectile_hurts_hero_not_enemies() {
        let mut app = build_test_app();
        let hero = spawn_test_hero(&mut app, Vec3::ZERO);
        let shooter = spawn_test_enemy(&mut app, EnemyClass::Far, Vec3::new(0.0, 0.0, -10.0));
        // Melee ally sits in the line of fire but outside its own reach of the hero.
        let ally = spawn_test_enemy(&mut app, EnemyClass::Near, Vec3::new(0.0, 0.0, -5.0));
        app.world_mut().get_mut::<Enemy>(ally).unwrap().aggro_range = 0.0;

        step_fixed(&mut app);
        assert_eq!(
            app.world().get::<Enemy>(shooter).unwrap().state,
            EnemyState::Windup,
            "10 units is inside 60 ft attack range: no chase"
        );
        for _ in 0..90 {
            step_fixed(&mut app);
        }
        assert!(hero_health(&app, hero) < 100, "projectile hit the hero");
        assert_eq!(
            app.world().get::<Health>(ally).unwrap().current,
            EnemyTuning::default().near.health,
            "hostile projectile passes through enemies"
        );
    }

    #[test]
    fn test_hero_stats_from_character_script() {
        let mut app = App::new();
        let dir = ["assets/files", "../../assets/files"]
            .into_iter()
            .map(std::path::Path::new)
            .find(|d| d.join("character/Init_Setup.bld").exists());
        let Some(dir) = dir else { return };
        let mut loader = skylands_formats::AssetLoader::new(Default::default());
        loader.mount(dir.join("permanent/global.bld")).unwrap();
        loader.mount(dir.join("character/Init_Setup.bld")).unwrap();
        loader.mount(dir.join("character/016_Spyro.bld")).unwrap();
        loader
            .mount(dir.join("character/1001_ChompyMage.bld"))
            .unwrap();
        let server = GameAssetServer::new(loader);
        let hero = spawn_hero_actor(
            &mut app.world_mut().commands(),
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            16,
            Some(&server),
        );
        app.world_mut().flush();
        // `HP_Minion_Medium` 270 + `HP_Minion_NudgeAmount` 10, not the 500 default.
        assert_eq!(
            app.world().get::<Health>(hero),
            Some(&Health {
                current: 280,
                max: 280
            })
        );
        assert_eq!(server.hero_max_hp("missing.bld"), None);
        // Spyro's "default speed" `Speed_Med` (24 ft/s), not the 20 ft/s DOL default.
        let params = app.world().get::<ModelParameters>(hero).unwrap();
        assert!((params.ground_max_speed - 24.0 * FEET_TO_UNITS).abs() < 1e-4);
        assert!((params.air_max_speed - 25.0 * FEET_TO_UNITS).abs() < 1e-4);
        assert_eq!(params.ground_turn_rate, 1440.0);
        let speed = app.world().get::<MovementSpeed>(hero).unwrap().0;
        assert!((speed - params.ground_max_speed).abs() < 1e-4);

        // Villains: Chompy Mage passes "max hp" 1000 and `Speed_Med`.
        assert_eq!(
            character_health(Some(&server), "1001_ChompyMage.bld"),
            Health {
                current: 1000,
                max: 1000
            }
        );
        let villain = spawn_controlled_villain_actor(
            &mut app.world_mut().commands(),
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            1001,
            VillainEnergy::default(),
            Some(&server),
        );
        app.world_mut().flush();
        let params = app.world().get::<ModelParameters>(villain).unwrap();
        assert!((params.ground_max_speed - 24.0 * FEET_TO_UNITS).abs() < 1e-4);
    }

    #[test]
    fn test_level_enemy_tuning_from_scripts() {
        let empty = skylands_formats::AssetLoader::new(Default::default());
        assert_eq!(
            level_enemy_tuning(&empty, "Level_302_Chompywurm.bld"),
            EnemyTuning::default()
        );
        let dir = ["assets/files", "../../assets/files"]
            .into_iter()
            .map(std::path::Path::new)
            .find(|d| d.join("permanent/global.bld").exists());
        let Some(dir) = dir else { return };
        let mut loader = skylands_formats::AssetLoader::new(Default::default());
        loader.mount(dir.join("permanent/global.bld")).unwrap();
        loader
            .mount(dir.join("level/Level_302_Chompywurm.bld"))
            .unwrap();
        let t = level_enemy_tuning(&loader, "Level_302_Chompywurm.bld");
        // Near: 10 * 5.5 HP, 100 / 9 dmg. Far: 10 * 4 HP, 100 / 15 dmg.
        assert_eq!(
            t.near,
            EnemyClassStats {
                health: 55,
                damage: 11
            }
        );
        assert_eq!(
            t.far,
            EnemyClassStats {
                health: 40,
                damage: 7
            }
        );
    }

    #[test]
    fn test_enemy_defeat_drops_loot() {
        let mut app = build_test_app();
        let enemy = spawn_test_enemy(&mut app, EnemyClass::Near, Vec3::new(3.0, 0.0, 0.0));
        app.world_mut()
            .resource_mut::<Events<DamageEvent>>()
            .send(DamageEvent {
                target: enemy,
                damage: EnemyTuning::default().near.health,
                position: Vec3::new(3.0, 0.0, 0.0),
            });
        step_fixed(&mut app);
        assert!(app.world().get_entity(enemy).is_err());
        let loot: Vec<u32> = app
            .world_mut()
            .query::<&Collectible>()
            .iter(app.world())
            .map(|c| c.value)
            .collect();
        assert_eq!(loot, vec![5]);
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Hub
        );
    }

    #[test]
    fn test_defeated_hero_is_out_until_level_change() {
        let mut app = build_test_app();
        let hero = spawn_test_hero(&mut app, Vec3::ZERO);
        app.world_mut()
            .resource_mut::<Events<DamageEvent>>()
            .send(DamageEvent {
                target: hero,
                damage: 1000,
                position: Vec3::ZERO,
            });
        step_fixed(&mut app);
        assert!(app.world().get_entity(hero).is_err());
        assert_eq!(
            app.world()
                .resource::<PortalGameplayState>()
                .defeated_heroes,
            vec![16]
        );

        // Same Skylander back on the portal: refused.
        app.world_mut()
            .resource_mut::<Events<PortalEvent>>()
            .send(PortalEvent::TagArrived {
                tag_index: 1,
                zone: PortalZone::LeftRing,
            });
        step_fixed(&mut app);
        let heroes = app
            .world_mut()
            .query_filtered::<Entity, With<Hero>>()
            .iter(app.world())
            .count();
        assert_eq!(heroes, 0);
    }

    #[test]
    fn test_enemy_ai_is_deterministic() {
        let run = || {
            let mut app = build_test_app();
            let hero = spawn_test_hero(&mut app, Vec3::ZERO);
            spawn_test_enemy(&mut app, EnemyClass::Near, Vec3::new(4.0, 0.0, -5.0));
            spawn_test_enemy(&mut app, EnemyClass::Far, Vec3::new(-6.0, 0.0, 3.0));
            for _ in 0..300 {
                step_fixed(&mut app);
            }
            let mut q = app.world_mut().query::<(&Enemy, &Transform)>();
            let enemies: Vec<_> = q
                .iter(app.world())
                .map(|(e, t)| (e.state, t.translation.to_array().map(f32::to_bits)))
                .collect();
            (hero_health(&app, hero), enemies)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn test_animation_state_airborne_and_enemy() {
        let airborne = CharacterCollider {
            grounded: false,
            ..Default::default()
        };
        let grounded = CharacterCollider::default();
        let up = Velocity(Vec3::new(0.0, 5.0, 0.0));
        let down = Velocity(Vec3::new(4.0, -5.0, 0.0));
        let fast = Velocity(Vec3::new(4.0, 0.0, 0.0));
        assert_eq!(
            animation_state_name(None, None, Some(&up), Some(&airborne)),
            "jump"
        );
        assert_eq!(
            animation_state_name(None, None, Some(&down), Some(&airborne)),
            "fall"
        );
        assert_eq!(
            animation_state_name(None, None, Some(&fast), Some(&grounded)),
            "run"
        );

        let mut enemy = Enemy::new(EnemyClass::Near, 10);
        assert_eq!(
            animation_state_name(None, Some(&enemy), None, Some(&grounded)),
            "idle"
        );
        enemy.state = EnemyState::Windup;
        assert_eq!(
            animation_state_name(None, Some(&enemy), Some(&fast), Some(&grounded)),
            "attack"
        );
    }

    #[test]
    fn test_animation_state_system_drives_enemies() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = build_test_app();
        let enemy = app
            .world_mut()
            .spawn((
                Enemy::new(EnemyClass::Near, 10),
                Velocity(Vec3::new(1.0, 0.0, 0.0)),
                CharacterCollider::default(),
                ActorAnimation::default(),
            ))
            .id();
        app.world_mut()
            .run_system_once(animation_state_system)
            .unwrap();
        let anim = app.world().get::<ActorAnimation>(enemy).unwrap();
        assert_eq!(anim.current_clip.as_deref(), Some("walk"));
    }

    /// `PRESCRIPT { STARTUP {} SHUTDOWN {} body }` over handles `my`,
    /// `orientation`, `relative facing`, `Script Timer`.
    fn scripted_level(body: Vec<skylands_formats::script::Op>) -> LevelScripts {
        use skylands_formats::script::{name_hash, Instr, Op, Script};
        use skylands_formats::script_runtime::field::*;
        let ins = |offset, br, op| Instr {
            offset,
            flags: 0,
            br,
            op,
        };
        let mut ops = vec![
            ins(1, body.len() as u16 + 2, Op::PreScript),
            ins(2, 0, Op::StartUp),
            ins(3, 0, Op::ShutDown),
        ];
        ops.extend(
            body.into_iter()
                .enumerate()
                .map(|(i, op)| ins(10 + i, 0, op)),
        );
        let ids = vec![MY, ORIENTATION, RELATIVE_FACING, SCRIPT_TIMER];
        let set = skylands_formats::ScriptSet {
            scripts: vec![Script {
                name: "Windmill.ai".into(),
                ops,
            }],
            handle_ns: vec![name_hash("level.bld"); ids.len()],
            handle_ids: ids,
            ..Default::default()
        };
        LevelScripts(std::sync::Arc::new(skylands_formats::ScriptProgram::new(
            set, None,
        )))
    }

    fn spawn_scripted(app: &mut App, scripts: LevelScripts, base: Transform) -> Entity {
        app.insert_resource(scripts);
        app.world_mut()
            .spawn((
                ActorScript {
                    script: 0,
                    marker_node: 7,
                    state: Default::default(),
                    base,
                },
                base,
            ))
            .id()
    }

    #[test]
    fn test_actor_script_spins_placed_prop() {
        use skylands_formats::script::{Op, Operand, Ref, Rhs, Value};
        let spin = Op::SetValue {
            lhs: vec![Ref::Handle(0), Ref::Handle(1), Ref::Handle(2)],
            rhs: Rhs {
                a: Operand::Lit(Value::Int(-2)),
                arith: -1,
                b: None,
                func: 0,
            },
        };
        let mut app = build_test_app();
        let base = Transform::from_xyz(1.0, 2.0, 3.0);
        let mill = spawn_scripted(&mut app, scripted_level(vec![spin]), base);
        for _ in 0..45 {
            step_fixed(&mut app);
        }
        let t = app.world().get::<Transform>(mill).unwrap();
        // 45 ticks x -2 degrees = a quarter turn clockwise about +Y.
        let expected = Quat::from_rotation_y((-90f32).to_radians());
        assert!(
            t.rotation.dot(expected).abs() > 1.0 - 1e-6,
            "{:?}",
            t.rotation
        );
        assert_eq!(t.translation, base.translation);
    }

    #[test]
    fn test_actor_script_remove_despawns_prop() {
        use skylands_formats::script::{Op, Operand, Ref, Rhs, Value};
        let check = Op::CheckValue {
            lhs: vec![Ref::Handle(3)],
            rel: 2,
            rhs: Rhs {
                a: Operand::Lit(Value::Int(0)),
                arith: -1,
                b: None,
                func: 0,
            },
        };
        let remove = Op::Remove {
            target: vec![Ref::Handle(0)],
        };
        let mut app = build_test_app();
        let scripts = scripted_level(vec![check, remove]);
        // `br` 1 on the check guards the remove.
        let mut program = std::sync::Arc::try_unwrap(scripts.0).ok().unwrap();
        program.set.scripts[0].ops[3].br = 1;
        let scripts = LevelScripts(std::sync::Arc::new(skylands_formats::ScriptProgram::new(
            program.set,
            None,
        )));
        let prop = spawn_scripted(&mut app, scripts, Transform::IDENTITY);
        step_fixed(&mut app);
        assert!(app.world().get_entity(prop).is_err());
    }

    fn npc(name: &str, conversations: &[&[&str]]) -> Npc {
        Npc {
            name: name.into(),
            talk_radius: NPC_TALK_RADIUS,
            conversations: conversations
                .iter()
                .map(|lines| skylands_formats::Conversation {
                    lines: lines
                        .iter()
                        .map(|t| skylands_formats::DialogueLine {
                            speaker: None,
                            text: t.to_string(),
                        })
                        .collect(),
                })
                .collect(),
            next_conversation: 0,
            marker_node: None,
        }
    }

    fn tap_a(app: &mut App) {
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::A);
        step_fixed(app);
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .release(WiiButton::A);
        step_fixed(app);
    }

    #[test]
    fn test_npc_talk_prompt_and_dialogue() {
        let mut app = build_test_app();
        let world = app.world_mut();
        let hero = world.spawn((Hero, Transform::from_xyz(0.0, 0.0, 0.0))).id();
        world.spawn((npc("Far", &[&["far"]]), Transform::from_xyz(2.5, 0.0, 0.0)));
        // Nothing to say: no prompt even when closest.
        world.spawn((npc("Mute", &[]), Transform::from_xyz(0.5, 0.0, 0.0)));
        let flynn = world
            .spawn((
                npc("Flynn", &[&["one", "two"], &["three"]]),
                Transform::from_xyz(1.0, 0.0, 0.0),
            ))
            .id();
        let prompt = |app: &App| app.world().resource::<GameStats>().active_prompt.clone();
        let line = |app: &App| {
            let d = app.world().resource::<ActiveDialogue>();
            d.current_line().map(|(s, t)| format!("{s}: {t}"))
        };

        step_fixed(&mut app);
        assert_eq!(prompt(&app).as_deref(), Some("Press A to talk to Flynn"));

        // Holding A only counts once.
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .press(WiiButton::A);
        step_fixed(&mut app);
        step_fixed(&mut app);
        assert_eq!(line(&app).as_deref(), Some("Flynn: one"));
        assert_eq!(prompt(&app), None);
        let sent: Vec<_> = app
            .world_mut()
            .resource_mut::<Events<NpcInteractEvent>>()
            .drain()
            .collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].npc, flynn);
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .release(WiiButton::A);
        step_fixed(&mut app);

        tap_a(&mut app);
        assert_eq!(line(&app).as_deref(), Some("Flynn: two"));
        tap_a(&mut app);
        assert_eq!(line(&app), None, "last line closes");

        // The next talk plays the next conversation, then wraps around.
        tap_a(&mut app);
        assert_eq!(line(&app).as_deref(), Some("Flynn: three"));
        tap_a(&mut app);
        tap_a(&mut app);
        assert_eq!(line(&app).as_deref(), Some("Flynn: one"));

        // Walking away ends the conversation and the prompt.
        app.world_mut()
            .get_mut::<Transform>(hero)
            .unwrap()
            .translation
            .x = -20.0;
        step_fixed(&mut app);
        assert_eq!(line(&app), None);
        assert_eq!(prompt(&app), None);
    }

    #[test]
    fn test_hero_holds_still_while_talking() {
        let mut app = build_test_app();
        let world = app.world_mut();
        let hero = world
            .spawn((
                Hero,
                Transform::default(),
                Velocity::default(),
                MovementSpeed(5.0),
                CharacterCollider {
                    radius: 0.5,
                    height: 1.0,
                    step_height: 0.0,
                    grounded: true,
                },
            ))
            .id();
        world.spawn((npc("Flynn", &[&["hi"]]), Transform::from_xyz(1.0, 0.0, 0.0)));
        tap_a(&mut app);
        assert!(app.world().resource::<ActiveDialogue>().is_active());
        app.world_mut()
            .resource_mut::<WiiInputState>()
            .set_stick(1.0, 0.0);
        for _ in 0..10 {
            step_fixed(&mut app);
        }
        let v = app.world().get::<Velocity>(hero).unwrap().0;
        assert!(v.x.abs() < 1e-4 && v.z.abs() < 1e-4, "{v:?}");
    }

    #[test]
    fn test_hub_npcs_spawn() {
        let dir = ["assets/files", "../../assets/files"]
            .into_iter()
            .map(std::path::Path::new)
            .find(|d| d.join("level/Level_Hub_2014_01.bld").exists());
        let Some(dir) = dir else { return };
        let mut loader = skylands_formats::AssetLoader::new(Default::default());
        loader
            .mount(dir.join("level/Level_Hub_2014_01.bld"))
            .unwrap();
        let mut app = build_test_app();
        let world = app.world_mut();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, world);
        assert!(spawn_level_content(
            &mut commands,
            &loader,
            "Level_Hub_2014_01.bld",
            None,
            &CampaignProgress::default(),
        ));
        queue.apply(world);
        let mut q = world.query::<(&Npc, Has<LevelDrawnModel>)>();
        let mut names: Vec<_> = q.iter(world).map(|(n, _)| n.name.clone()).collect();
        let all: Vec<(String, String)> = q
            .iter(world)
            .flat_map(|(n, _)| {
                n.conversations
                    .iter()
                    .flat_map(|c| &c.lines)
                    .map(|l| (n.name.clone(), l.text.clone()))
            })
            .collect();
        let lines = |who: &str| -> Vec<&str> {
            all.iter()
                .filter(|(n, _)| n == who)
                .map(|(_, t)| t.as_str())
                .collect()
        };
        names.sort();
        eprintln!("hub npcs: {names:?}");
        for n in [
            "Auric",
            "Buzz",
            "Cali",
            "Flynn",
            "Persephone",
            "T-Bone",
            "Tessa",
        ] {
            assert!(names.iter().any(|x| x == n), "missing {n}");
        }
        assert!(
            lines("Flynn")
                .iter()
                .any(|t| t.contains("talk to Persephone")),
            "Flynn's lines come from the language pack"
        );
        assert!(!lines("Cali").is_empty() && !lines("Buzz").is_empty());
        assert!(lines("Whiskers").is_empty());
        // The inactive store template is not spawned.
        assert_eq!(names.iter().filter(|x| *x == "Auric").count(), 1);
        assert!(
            q.iter(world).any(|(_, drawn)| drawn),
            "NPC models come from the level"
        );
    }

    #[test]
    fn test_hub_props_run_their_scripts() {
        let dir = ["assets/files", "../../assets/files"]
            .into_iter()
            .map(std::path::Path::new)
            .find(|d| d.join("level/Level_Hub_2014_01.bld").exists());
        let Some(dir) = dir else { return };
        let mut loader = skylands_formats::AssetLoader::new(Default::default());
        loader.mount(dir.join("permanent/global.bld")).unwrap();
        loader
            .mount(dir.join("level/Level_Hub_2014_01.bld"))
            .unwrap();
        let mut app = build_test_app();
        let world = app.world_mut();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, world);
        assert!(spawn_level_content(
            &mut commands,
            &loader,
            "Level_Hub_2014_01.bld",
            None,
            &CampaignProgress::default(),
        ));
        queue.apply(world);
        let mut q = world.query::<(&ActorScript, &Transform)>();
        let before: Vec<_> = q.iter(world).map(|(a, t)| (a.marker_node, *t)).collect();
        eprintln!("hub scripted actors: {}", before.len());
        assert!(before.len() >= 10, "{}", before.len());
        for _ in 0..120 {
            step_fixed(&mut app);
        }
        let world = app.world_mut();
        let mut q = world.query::<(&ActorScript, &Transform)>();
        let moved = q
            .iter(world)
            .filter(|(a, t)| {
                before.iter().any(|(n, b)| {
                    *n == a.marker_node
                        && (b.rotation != t.rotation || b.translation != t.translation)
                })
            })
            .count();
        let errors = q
            .iter(world)
            .filter(|(a, _)| a.state.error.is_some())
            .count();
        eprintln!("moved after 2 s: {moved}, errors: {errors}");
        assert!(moved >= 5, "{moved}");
        assert_eq!(errors, 0);
    }
}
