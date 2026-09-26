//! Список 999 уровней (0..=998), читается/пишется в levels.json.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const LEVEL_COUNT: u32 = 999;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LevelEntry {
    pub name: String,
    pub number: u32,
}

/// Сгенерировать дефолтный список: имена совпадают с номерами.
pub fn default_levels() -> Vec<LevelEntry> {
    (0..LEVEL_COUNT)
        .map(|n| LevelEntry {
            name: n.to_string(),
            number: n,
        })
        .collect()
}

/// Прочитать levels.json, либо создать дефолтный и сохранить.
pub fn load_or_init(path: &Path) -> Vec<LevelEntry> {
    match std::fs::read_to_string(path) {
        Ok(txt) => match serde_json::from_str::<Vec<LevelEntry>>(&txt) {
            Ok(v) if !v.is_empty() => v,
            _ => {
                let v = default_levels();
                let _ = save(path, &v);
                v
            }
        },
        Err(_) => {
            let v = default_levels();
            let _ = save(path, &v);
            v
        }
    }
}

pub fn save(path: &Path, levels: &[LevelEntry]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let txt = serde_json::to_string_pretty(levels).unwrap_or_else(|_| "[]".to_string());
    std::fs::write(path, txt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn defaults_have_999_entries() {
        let v = default_levels();
        assert_eq!(v.len(), LEVEL_COUNT as usize);
        assert_eq!(v[0].name, "0");
        assert_eq!(v[998].number, 998);
    }

    #[test]
    fn load_or_init_creates_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("levels.json");
        assert!(!path.exists());
        let v = load_or_init(&path);
        assert_eq!(v.len(), LEVEL_COUNT as usize);
        assert!(path.exists());
    }

    #[test]
    fn roundtrip_keeps_values() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("levels.json");
        let mut v = default_levels();
        v[5].name = "Level 5 test".into();
        save(&path, &v).unwrap();
        let back = load_or_init(&path);
        assert_eq!(back[5].name, "Level 5 test");
    }
}