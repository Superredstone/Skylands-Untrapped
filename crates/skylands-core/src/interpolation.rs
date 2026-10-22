//! Render-side smoothing between 60Hz fixed ticks for high refresh displays.
//! Simulation always sees the exact tick result (`restore` runs before every tick),
//! so interpolation never affects determinism.

use crate::components::Velocity;
use bevy::prelude::*;

/// ponytail: distance treated as a teleport (respawn); swap for an explicit marker
/// if fast actors legitimately move this far in one tick.
pub const SNAP_DISTANCE: f32 = 5.0;

#[derive(Component, Debug, Clone, Copy)]
pub struct TickTransform {
    pub prev: Transform,
    pub curr: Transform,
}

pub fn restore_tick_transform(mut q: Query<(&mut Transform, &mut TickTransform)>) {
    for (mut tf, mut tick) in &mut q {
        *tf = tick.curr;
        tick.prev = tick.curr;
    }
}

type Untracked<'w, 's> =
    Query<'w, 's, (Entity, &'static Transform), (With<Velocity>, Without<TickTransform>)>;

pub fn record_tick_transform(
    mut commands: Commands,
    mut tracked: Query<(&Transform, &mut TickTransform)>,
    untracked: Untracked,
) {
    for (tf, mut tick) in &mut tracked {
        tick.curr = *tf;
    }
    for (entity, tf) in &untracked {
        commands.entity(entity).insert(TickTransform {
            prev: *tf,
            curr: *tf,
        });
    }
}

pub fn blend(prev: &Transform, curr: &Transform, t: f32) -> Transform {
    if prev.translation.distance(curr.translation) > SNAP_DISTANCE {
        return *curr;
    }
    Transform {
        translation: prev.translation.lerp(curr.translation, t),
        rotation: prev.rotation.slerp(curr.rotation, t),
        scale: prev.scale.lerp(curr.scale, t),
    }
}

pub fn blend_tick_transform(
    time: Res<Time<Fixed>>,
    mut q: Query<(&mut Transform, &TickTransform)>,
) {
    let t = time.overstep_fraction();
    for (mut tf, tick) in &mut q {
        *tf = blend(&tick.prev, &tick.curr, t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn test_blend_midpoint_restore_exact_and_snap() {
        let mut world = World::new();
        let prev = Transform::from_xyz(0.0, 0.0, 0.0);
        let curr = Transform::from_xyz(2.0, 0.0, 0.0);
        let e = world.spawn((curr, TickTransform { prev, curr })).id();

        let mut q = world.query::<(&mut Transform, &TickTransform)>();
        for (mut tf, tick) in q.iter_mut(&mut world) {
            *tf = blend(&tick.prev, &tick.curr, 0.5);
        }
        assert_eq!(world.get::<Transform>(e).unwrap().translation.x, 1.0);

        world.run_system_once(restore_tick_transform).unwrap();
        assert_eq!(*world.get::<Transform>(e).unwrap(), curr);
        assert_eq!(world.get::<TickTransform>(e).unwrap().prev, curr);

        let far = Transform::from_xyz(100.0, 0.0, 0.0);
        assert_eq!(blend(&prev, &far, 0.5), far);
    }
}
