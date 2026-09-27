//! FPS-камера: обзор мышью, чувствительность, присед.

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use super::{Crouching, MouseSensitivity};

#[derive(Component, Debug, Clone)]
pub struct FpsCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub eye_height: f32,
    pub crouch_offset: f32,
    pub smoothing: f32,
}

impl Default for FpsCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0, pitch: -15.0,
            eye_height: 1.7, crouch_offset: 0.7, smoothing: 10.0,
        }
    }
}

pub fn clamp_pitch(p: f32) -> f32 { p.max(-89.0).min(89.0) }

pub fn mouse_look(
    mut motion: EventReader<MouseMotion>,
    sens: Res<MouseSensitivity>,
    mut q: Query<&mut FpsCamera>,
) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    for ev in motion.read() {
        dx += ev.delta.x;
        dy += ev.delta.y;
    }
    let s = sens.0.max(0.01);
    for mut cam in q.iter_mut() {
        // Знак «минус»: движение мыши вправо даёт поворот вправо (yaw уменьшается
        // относительно Bevy-конвенции, где камера смотрит в -Z).
        cam.yaw -= dx * s * 0.12;
        cam.pitch = clamp_pitch(cam.pitch - dy * s * 0.12);
    }
}

pub fn apply_camera_rotation(mut q: Query<(&FpsCamera, &mut Transform)>) {
    for (cam, mut tf) in q.iter_mut() {
        let yaw = cam.yaw.to_radians();
        let pitch = cam.pitch.to_radians();
        // Порядок: сначала yaw вокруг Y, потом pitch вокруг локальной X.
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    }
}

pub fn apply_camera_height(
    time: Res<Time>,
    crouch: Res<Crouching>,
    mut q: Query<(&FpsCamera, &mut Transform)>,
) {
    let dt = time.delta_seconds();
    for (cam, mut tf) in q.iter_mut() {
        let target_y = if crouch.0 { cam.eye_height - cam.crouch_offset } else { cam.eye_height };
        let a = (cam.smoothing * dt).min(1.0);
        tf.translation.y += (target_y - tf.translation.y) * a;
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MouseSensitivity>().add_systems(
            Update,
            (mouse_look, apply_camera_rotation, apply_camera_height).chain(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_pitch_bounds() {
        assert_eq!(clamp_pitch(100.0), 89.0);
        assert_eq!(clamp_pitch(-100.0), -89.0);
    }
}
