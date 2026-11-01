pub mod xml;
mod schema;
mod lua;
mod runtime;

pub use runtime::{LevelCollider, LevelRuntimePlugin};
pub use schema::{LevelDefinition, PlayerDef};

use anyhow::{anyhow, bail, Context, Result};
use bevy::prelude::Resource;
use std::{
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use zip::ZipArchive;

#[derive(Resource, Clone)]
pub struct LoadedLevel {
    pub root: PathBuf,
    pub definition: Arc<LevelDefinition>,
}

pub fn prepare_level(level_arg: &str, data_dir: Option<&Path>) -> Result<LoadedLevel> {
    let direct = PathBuf::from(level_arg);
    let root = if direct.is_dir() {
        direct
    } else if direct.is_file() {
        if direct.extension().and_then(|v| v.to_str()).map(|v| v.eq_ignore_ascii_case("zip")) != Some(true) {
            bail!("level file must be a .zip archive: {}", direct.display());
        }
        extract_zip(&direct, data_dir)?
    } else {
        let base = data_dir.map(Path::to_path_buf).unwrap_or_else(default_data_dir);
        let installed = base.join("levels").join(level_arg);
        if !installed.is_dir() {
            bail!("level not found: {level_arg:?}; expected a directory, .zip, or {}", installed.display());
        }
        installed
    };

    let xml_path = root.join("level.xml");
    let source = fs::read_to_string(&xml_path).with_context(|| format!("reading {}", xml_path.display()))?;
    let root_node = xml::parse(&source).map_err(|err| anyhow!("level.xml: {err}"))?;
    let definition = schema::parse_level(&root_node)?;
    validate_level_files(&root, &definition)?;
    Ok(LoadedLevel { root, definition: Arc::new(definition) })
}

pub(crate) fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() {
        bail!("level asset path must be relative: {relative:?}");
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            bail!("level asset path escapes package root: {relative:?}");
        }
    }
    Ok(root.join(path))
}

fn validate_level_files(root: &Path, definition: &LevelDefinition) -> Result<()> {
    for texture in definition.assets.textures.values() {
        require_file(root, &texture.src, "texture")?;
    }
    for model in definition.assets.models.values() {
        require_file(root, &model.src, "model")?;
    }
    for sound in definition.assets.sounds.values() {
        require_file(root, &sound.src, "sound")?;
    }
    for script in definition.scripts.iter().chain(definition.chunks.iter().flat_map(|chunk| chunk.scripts.iter())) {
        if let Some(src) = &script.src { require_file(root, src, "Lua script")?; }
    }
    if let Some(preview) = &definition.meta.preview {
        require_file(root, preview, "preview")?;
    }
    Ok(())
}

fn require_file(root: &Path, relative: &str, kind: &str) -> Result<()> {
    let path = safe_join(root, relative)?;
    if !path.is_file() { bail!("{kind} does not exist: {}", path.display()); }
    Ok(())
}

fn extract_zip(archive_path: &Path, data_dir: Option<&Path>) -> Result<PathBuf> {
    let base = data_dir.map(Path::to_path_buf).unwrap_or_else(default_data_dir);
    let stem = archive_path.file_stem().and_then(|v| v.to_str()).unwrap_or("direct-level");
    let output = base.join("levels").join("_direct").join(stem);
    if output.exists() { fs::remove_dir_all(&output).with_context(|| format!("clearing {}", output.display()))?; }
    fs::create_dir_all(&output)?;

    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).context("opening level zip")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name().map(Path::to_path_buf) else {
            bail!("unsafe path in level zip: {}", entry.name());
        };
        let target = output.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
            let mut out = File::create(&target)?;
            io::copy(&mut entry, &mut out)?;
        }
    }
    if !output.join("level.xml").is_file() {
        bail!("level zip must contain level.xml at archive root");
    }
    Ok(output)
}

fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        return base.join(".backrooms_infinity");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let base = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join(".backrooms_infinity")
    }
}
