//! Настройки игрока: чувствительность мыши + переназначение клавиш.
//! Сохраняется в settings.json, читается и Rust-клиентом, и Flutter-лаунчером.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const DEFAULT_SENSITIVITY: f32 = 1.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, bevy::prelude::Resource)]
pub struct Settings {
    #[serde(default = "default_sensitivity")]
    pub mouse_sensitivity: f32,
    #[serde(default = "default_bindings")]
    pub bindings: BTreeMap<String, String>,
}

fn default_sensitivity() -> f32 {
    DEFAULT_SENSITIVITY
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: DEFAULT_SENSITIVITY,
            bindings: default_bindings(),
        }
    }
}

impl Settings {
    /// Чувствительность в допустимых пределах (0.1..=5.0).
    pub fn clamp_sensitivity(&mut self) {
        if self.mouse_sensitivity < 0.1 {
            self.mouse_sensitivity = 0.1;
        }
        if self.mouse_sensitivity > 5.0 {
            self.mouse_sensitivity = 5.0;
        }
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(txt) => serde_json::from_str::<Settings>(&txt).unwrap_or_default(),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let txt = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, txt)
    }

    /// Клавиша для действия (fallback на дефолт).
    pub fn key_for(&self, action: &str) -> String {
        self.bindings
            .get(action)
            .cloned()
            .or_else(|| default_bindings().get(action).cloned())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_has_expected_bindings() {
        let s = Settings::default();
        assert_eq!(s.key_for("move_forward"), "W");
        assert_eq!(s.key_for("jump"), "Space");
        assert_eq!(s.key_for("pause"), "Escape");
        assert!((s.mouse_sensitivity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clamp_works() {
        let mut s = Settings::default();
        s.mouse_sensitivity = -1.0;
        s.clamp_sensitivity();
        assert!((s.mouse_sensitivity - 0.1).abs() < 1e-6);
        s.mouse_sensitivity = 100.0;
        s.clamp_sensitivity();
        assert!((s.mouse_sensitivity - 5.0).abs() < 1e-6);
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut s = Settings::default();
        s.mouse_sensitivity = 2.5;
        s.bindings.insert("jump".into(), "F".into());
        s.save(&path).unwrap();
        let back = Settings::load(&path);
        assert!((back.mouse_sensitivity - 2.5).abs() < 1e-6);
        assert_eq!(back.key_for("jump"), "F");
    }

    #[test]
    fn load_missing_returns_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.json");
        let s = Settings::load(&path);
        assert_eq!(s, Settings::default());
    }
}