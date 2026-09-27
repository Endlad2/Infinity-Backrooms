//! Компоненты игрока и чистая логика движения.

pub mod controller;
pub mod camera;
pub mod cursor_grab;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use bevy::prelude::{Component, Resource};

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
    fn default() -> Self { Self { map: default_bindings() } }
}

impl Bindings {
    pub fn key_for(&self, action: &str) -> String {
        self.map.get(action).cloned()
            .or_else(|| default_bindings().get(action).cloned())
            .unwrap_or_default()
    }
}

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
            hp: 100.0, hp_max: 100.0,
            speed: 5.0, sprint_mult: 1.75, crouch_mult: 0.5, jump_velocity: 6.0,
        }
    }
}

#[derive(Debug, Clone, Default, Resource)]
pub struct Crouching(pub bool);

#[derive(Debug, Clone, Default, Resource)]
pub struct Sprinting(pub bool);

#[derive(Debug, Clone, Default, Component)]
pub struct Velocity3(pub [f32; 3]);

#[derive(Debug, Clone, Default, Resource)]
pub struct InputState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub sprint: bool,
    pub jump: bool,
    pub crouch_pressed: bool,
}

#[derive(Debug, Clone, Resource)]
pub struct MouseSensitivity(pub f32);

impl Default for MouseSensitivity {
    fn default() -> Self { Self(1.0) }
}

/// Вектор желаемого перемещения в плоскости XZ.
/// Bevy-камера смотрит в -Z при yaw=0; forward = (-sin, -cos), right = (cos, -sin).
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
    if input.forward { f += 1.0; }
    if input.back { f -= 1.0; }
    if input.right { r += 1.0; }
    if input.left { r -= 1.0; }
    if f == 0.0 && r == 0.0 { return [0.0, 0.0, 0.0]; }

    let yaw = yaw_deg.to_radians();
    let (sy, cy) = (yaw.sin(), yaw.cos());

    let dx = -sy * f + cy * r;
    let dz = -cy * f - sy * r;

    let len = (dx * dx + dz * dz).sqrt().max(1e-6);
    let mult = if crouched { crouch_mult }
        else if input.sprint { sprint_mult } else { 1.0 };
    let s = speed * mult;
    [dx / len * s, 0.0, dz / len * s]
}

pub fn toggle_crouch(crouched: bool, pressed: bool) -> bool {
    if pressed { !crouched } else { crouched }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_at_yaw0_is_neg_z() {
        let mut i = InputState::default();
        i.forward = true;
        let v = desired_move(0.0, &i, 5.0, false, 1.75, 0.5);
        assert!(v[0].abs() < 1e-5);
        assert!((v[2] + 5.0).abs() < 1e-5);
    }

    #[test]
    fn back_at_yaw0_is_pos_z() {
        let mut i = InputState::default();
        i.back = true;
        let v = desired_move(0.0, &i, 5.0, false, 1.75, 0.5);
        assert!((v[2] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn right_at_yaw0_is_pos_x() {
        let mut i = InputState::default();
        i.right = true;
        let v = desired_move(0.0, &i, 5.0, false, 1.75, 0.5);
        assert!((v[0] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn left_at_yaw0_is_neg_x() {
        let mut i = InputState::default();
        i.left = true;
        let v = desired_move(0.0, &i, 5.0, false, 1.75, 0.5);
        assert!((v[0] + 5.0).abs() < 1e-5);
    }

    #[test]
    fn forward_at_yaw90_is_neg_x() {
        let mut i = InputState::default();
        i.forward = true;
        let v = desired_move(90.0, &i, 5.0, false, 1.75, 0.5);
        assert!((v[0] + 5.0).abs() < 1e-4);
    }

    #[test]
    fn forward_at_yaw180_is_pos_z() {
        let mut i = InputState::default();
        i.forward = true;
        let v = desired_move(180.0, &i, 5.0, false, 1.75, 0.5);
        assert!((v[2] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn sprint_multiplies() {
        let mut i = InputState::default();
        i.forward = true; i.sprint = true;
        let v = desired_move(0.0, &i, 5.0, false, 1.75, 0.5);
        assert!((v[2] + 8.75).abs() < 1e-3);
    }

    #[test]
    fn crouch_overrides_sprint() {
        let mut i = InputState::default();
        i.forward = true; i.sprint = true;
        let v = desired_move(0.0, &i, 5.0, true, 1.75, 0.5);
        assert!((v[2] + 2.5).abs() < 1e-3);
    }
}
