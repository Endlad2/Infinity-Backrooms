//! Компоненты игрока (FPS-контроллер), камеры, движения и логика ввода.
//! Реальная работа с Bevy — в controller.rs/camera.rs; здесь — только
//! структуры и чистая логика, которую можно тестировать.

pub mod controller;
pub mod camera;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use bevy::prelude::{Component, Resource};

/// Биндинги действий (из Settings). Соответствует §7.1 ТЗ.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bindings {
    #[serde(default = "default_bindings")]
    pub map: BTreeMap<String, String>,
}

fn default_bindings() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("move_forward".into(), "W".into());
    m.insert("move_back".into(), "S".into());
    m.insert("move_left".into(), "A".into());
    m.insert("move_right".into(), "D".into());
    m.insert("sprint".into(), "Shift".into());
    m.insert("crouch".into(), "Control".into());
    m.insert("jump".into(), "Space".into());
    m.insert("interact".into(), "E".into());
    m.insert("pause".into(), "Escape".into());
    m
}

impl Default for Bindings {
    fn default() -> Self {
        Self { map: default_bindings() }
    }
}

impl Bindings {
    pub fn key_for(&self, action: &str) -> String {
        self.map
            .get(action)
            .cloned()
            .or_else(|| default_bindings().get(action).cloned())
            .unwrap_or_default()
    }
}

/// Компонент: игрок.
#[derive(Debug, Clone, Component)]
pub struct Player {
    pub hp: f32,
    pub hp_max: f32,
    pub speed: f32,
    pub sprint_mult: f32,
    pub crouch_mult: f32,
    pub jump_velocity: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            hp: 100.0,
            hp_max: 100.0,
            speed: 5.0,
            sprint_mult: 1.75,
            crouch_mult: 0.5,
            jump_velocity: 6.0,
        }
    }
}

/// Компонент: игрок сейчас в приседе (toggle).
#[derive(Debug, Clone, Default, Resource)]
pub struct Crouching(pub bool);

/// Компонент: игрок сейчас бежит.
#[derive(Debug, Clone, Default, Resource)]
pub struct Sprinting(pub bool);

/// Компонент: вертикальная скорость.
#[derive(Debug, Clone, Default, Component)]
pub struct Velocity3(pub [f32; 3]);

/// Состояние ввода (из клавиатуры/мыши).
#[derive(Debug, Clone, Default, Resource)]
pub struct InputState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub sprint: bool,
    pub jump: bool,
    /// Нажатие клавиши приседания в текущем кадре (для toggle).
    pub crouch_pressed: bool,
}

/// Компонент: чувствительность мыши (из Settings).
#[derive(Debug, Clone, Resource)]
pub struct MouseSensitivity(pub f32);

impl Default for MouseSensitivity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Вычисляет вектор желаемого перемещения в плоскости XZ (yaw в градусах).
/// Возвращает нормализованное направление с учётом скорости и режима.
pub fn desired_move(
    yaw_deg: f32,
    input: &InputState,
    speed: f32,
    crouched: bool,
    sprint_mult: f32,
    crouch_mult: f32,
) -> [f32; 3] {
    let mut f = 0.0;
    let mut r = 0.0;
    if input.forward {
        f += 1.0;
    }
    if input.back {
        f -= 1.0;
    }
    if input.right {
        r += 1.0;
    }
    if input.left {
        r -= 1.0;
    }
    if f == 0.0 && r == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let yaw = yaw_deg.to_radians();
    // Базовая ось: (sin(yaw), cos(yaw)) — «вперёд»
    let dir_x = yaw.sin() * f + yaw.cos() * r;
    let dir_z = yaw.cos() * f - yaw.sin() * r;
    let len = (dir_x * dir_x + dir_z * dir_z).sqrt().max(1e-6);
    let mult = if crouched {
        crouch_mult
    } else if input.sprint {
        sprint_mult
    } else {
        1.0
    };
    let s = speed * mult;
    [dir_x / len * s, 0.0, dir_z / len * s]
}

/// Toggle-присед: если пришло нажатие — флип.
pub fn toggle_crouch(crouched: bool, pressed: bool) -> bool {
    if pressed {
        !crouched
    } else {
        crouched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_default_has_wasd() {
        let b = Bindings::default();
        assert_eq!(b.key_for("move_forward"), "W");
        assert_eq!(b.key_for("jump"), "Space");
    }

    #[test]
    fn move_forward_only_along_yaw0() {
        let mut input = InputState::default();
        input.forward = true;
        let v = desired_move(0.0, &input, 5.0, false, 1.75, 0.5);
        assert!((v[0]).abs() < 1e-5);
        assert!((v[2] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn move_strafe_at_yaw0() {
        let mut input = InputState::default();
        input.right = true;
        let v = desired_move(0.0, &input, 5.0, false, 1.75, 0.5);
        assert!((v[0] - 5.0).abs() < 1e-5);
        assert!((v[2]).abs() < 1e-5);
    }

    #[test]
    fn sprint_multiplies() {
        let mut input = InputState::default();
        input.forward = true;
        input.sprint = true;
        let v = desired_move(0.0, &input, 5.0, false, 1.75, 0.5);
        assert!((v[2] - 8.75).abs() < 1e-4);
    }

    #[test]
    fn crouch_multiplies_and_overrides_sprint() {
        let mut input = InputState::default();
        input.forward = true;
        input.sprint = true;
        let v = desired_move(0.0, &input, 5.0, true, 1.75, 0.5);
        assert!((v[2] - 2.5).abs() < 1e-4);
    }

    #[test]
    fn toggle_crouch_works() {
        assert_eq!(toggle_crouch(false, true), true);
        assert_eq!(toggle_crouch(true, true), false);
        assert_eq!(toggle_crouch(true, false), true);
    }
}