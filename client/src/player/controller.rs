//! Bevy-системы FPS-контроллера.

use bevy::prelude::*;

use super::camera::FpsCamera;
use super::{
    desired_move, toggle_crouch, Crouching, InputState, Player, Sprinting, Velocity3,
};

pub fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<InputState>,
) {
    state.forward = keys.pressed(KeyCode::KeyW);
    state.back = keys.pressed(KeyCode::KeyS);
    state.left = keys.pressed(KeyCode::KeyA);
    state.right = keys.pressed(KeyCode::KeyD);
    state.sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    state.jump = keys.pressed(KeyCode::Space);
    state.crouch_pressed = keys.just_pressed(KeyCode::ControlLeft)
        || keys.just_pressed(KeyCode::ControlRight);
}

pub fn apply_crouch_toggle(
    input: Res<InputState>,
    mut crouch: ResMut<Crouching>,
) {
    if input.crouch_pressed { crouch.0 = toggle_crouch(crouch.0, true); }
}

pub fn apply_sprint(
    input: Res<InputState>,
    crouch: Res<Crouching>,
    mut sprint: ResMut<Sprinting>,
) {
    sprint.0 = input.sprint && !crouch.0;
}

/// Движение игрока — ТЕПЕРЬ с учётом yaw камеры.
pub fn move_player(
    time: Res<Time>,
    input: Res<InputState>,
    crouch: Res<Crouching>,
    _sprint: Res<Sprinting>,
    cam_q: Query<&FpsCamera>,
    mut q: Query<(&Player, &mut Transform, &mut Velocity3)>,
) {
    let dt = time.delta_seconds();
    let yaw = cam_q.iter().next().map(|c| c.yaw).unwrap_or(0.0);

    for (player, mut tf, mut vel) in q.iter_mut() {
        let dir = desired_move(
            yaw, &input, player.speed,
            crouch.0, player.sprint_mult, player.crouch_mult,
        );
        tf.translation.x += dir[0] * dt;
        tf.translation.z += dir[2] * dt;
        vel.0[0] = dir[0];
        vel.0[2] = dir[2];
    }
}

pub fn vertical_physics(
    time: Res<Time>,
    input: Res<InputState>,
    gravity: Res<Gravity>,
    mut q: Query<(&Player, &mut Transform, &mut Velocity3, &Grounded)>,
) {
    let dt = time.delta_seconds();
    for (player, mut tf, mut vel, grounded) in q.iter_mut() {
        if grounded.0 && input.jump { vel.0[1] = player.jump_velocity; }
        vel.0[1] += gravity.0 * dt;
        tf.translation.y += vel.0[1] * dt;
        if tf.translation.y <= 0.9 {
            tf.translation.y = 0.9;
            vel.0[1] = 0.0;
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct Gravity(pub f32);
impl Default for Gravity {
    fn default() -> Self { Self(-9.81) }
}

#[derive(Component, Debug, Clone, Default)]
pub struct Grounded(pub bool);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputState>()
            .init_resource::<Crouching>()
            .init_resource::<Sprinting>()
            .init_resource::<Gravity>()
            .add_systems(Update, (
                read_input,
                apply_crouch_toggle,
                apply_sprint,
                move_player,
                vertical_physics,
            ).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_builds() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.add_plugins(PlayerPlugin);
        app.update();
    }
}
