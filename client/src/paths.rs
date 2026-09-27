//! Пути данных приложения и Bevy-ресурс AppPathsResource.

use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = ".infinity-backrooms";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub assets_db: PathBuf,
    pub levels_json: PathBuf,
    pub settings_json: PathBuf,
    pub levels_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub textures_dir: PathBuf,
    pub models_dir: PathBuf,
    pub audio_dir: PathBuf,
    pub cache_files_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> std::io::Result<Self> {
        let base = base_data_dir();
        let root = base.join(APP_DIR_NAME);
        Ok(Self::from_root(root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        let levels_dir = root.join("levels");
        let assets_dir = root.join("assets");
        let textures_dir = assets_dir.join("textures");
        let models_dir = assets_dir.join("models");
        let audio_dir = assets_dir.join("audio");
        let cache_files_dir = root.join("cache-files");
        Self {
            assets_db: root.join("assets.db"),
            levels_json: root.join("levels.json"),
            settings_json: root.join("settings.json"),
            levels_dir,
            assets_dir,
            textures_dir,
            models_dir,
            audio_dir,
            cache_files_dir,
            root,
        }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.levels_dir)?;
        std::fs::create_dir_all(&self.assets_dir)?;
        std::fs::create_dir_all(&self.textures_dir)?;
        std::fs::create_dir_all(&self.models_dir)?;
        std::fs::create_dir_all(&self.audio_dir)?;
        std::fs::create_dir_all(&self.cache_files_dir)?;
        Ok(())
    }

    pub fn level_xml(&self, number: u32) -> PathBuf {
        self.levels_dir.join(format!("{}.xml", number))
    }

    pub fn level_xml_exists(&self, number: u32) -> bool {
        self.level_xml(number).is_file()
    }

    pub fn cache_file(&self, name: &str) -> PathBuf {
        self.cache_files_dir.join(name)
    }

    pub fn has_cache_file(&self, name: &str) -> bool {
        self.cache_file(name).is_file()
    }
}

pub fn base_data_dir() -> PathBuf {
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                return PathBuf::from(appdata);
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home);
        }
    }
    Path::new(".").to_path_buf()
}

use std::sync::Arc;
use bevy::prelude::Resource;

#[derive(Debug, Clone, Resource)]
pub struct AppPathsResource(pub Arc<AppPaths>);

impl AppPathsResource {
    pub fn new(paths: AppPaths) -> Self { Self(Arc::new(paths)) }
    pub fn paths(&self) -> &AppPaths { &self.0 }
}

impl Default for AppPathsResource {
    fn default() -> Self {
        let base = base_data_dir();
        let root = base.join(APP_DIR_NAME);
        Self(Arc::new(AppPaths::from_root(root)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_root_builds_expected_tree() {
        let p = AppPaths::from_root(PathBuf::from("/tmp/infinity"));
        assert!(p.levels_json.ends_with("levels.json"));
        assert!(p.cache_files_dir.ends_with("cache-files"));
    }
}
