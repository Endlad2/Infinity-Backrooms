//! FPS-камера: обзор мышью (yaw/pitch), чувствительность из настроек,
//! приседание опускает камеру.

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use super::{Crouching, MouseSensitivity};

/// Компонент: состояние камеры игрока (yaw/pitch и текущая высота).
#[derive(Component, Debug, Clone)]
pub struct FpsCamera {
    pub yaw: f32,
    pub pitch: f32,
    /// Базовая высота (стоя).
    pub eye_height: f32,
    /// Смещение камеры в приседе.
    pub crouch_offset: f32,
    /// Сглаживание — коэффициент интерполяции (1 = моментально).
    pub smoothing: f32,
}

impl Default for FpsCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: -15.0,
            eye_height: 1.7,
            crouch_offset: 0.7,
            smoothing: 10.0,
        }
    }
}

/// Ограничиваем pitch, чтобы не перевернуться.
pub fn clamp_pitch(p: f32) -> f32 {
    p.max(-89.0).min(89.0)
}

/// Обработка движения мыши: обновляет yaw/pitch камеры.
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
        cam.yaw -= dx * s * 0.12;
        cam.pitch = clamp_pitch(cam.pitch - dy * s * 0.12);
    }
}

/// Применение yaw/pitch к Transform камеры.
pub fn apply_camera_rotation(mut q: Query<(&FpsCamera, &mut Transform)>) {
    for (cam, mut tf) in q.iter_mut() {
        let yaw = cam.yaw.to_radians();
        let pitch = cam.pitch.to_radians();
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    }
}

/// Применение высоты камеры: присед опускает.
pub fn apply_camera_height(
    time: Res<Time>,
    crouch: Res<Crouching>,
    mut q: Query<(&FpsCamera, &mut Transform)>,
) {
    let dt = time.delta_seconds();
    for (cam, mut tf) in q.iter_mut() {
        let target_y = if crouch.0 {
            cam.eye_height - cam.crouch_offset
        } else {
            cam.eye_height
        };
        let a = (cam.smoothing * dt).min(1.0);
        tf.translation.y += (target_y - tf.translation.y) * a;
    }
}

/// Плагин камеры.
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
        assert_eq!(clamp_pitch(30.0), 30.0);
    }

    #[test]
    fn camera_plugin_runs() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_event::<bevy::input::mouse::MouseMotion>();
        app.init_resource::<crate::settings::Settings>();
        app.init_resource::<crate::player::Crouching>();
        app.init_resource::<crate::player::Sprinting>();
        app.add_plugins(CameraPlugin);
        app.update();
    }

    #[test]
    fn default_fps_camera_ok() {
        let c = FpsCamera::default();
        assert!((c.eye_height - 1.7).abs() < 1e-6);
        assert!(c.crouch_offset > 0.0);
    }
}