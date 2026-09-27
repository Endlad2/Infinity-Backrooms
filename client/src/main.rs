//! Точка входа Backrooms Infinity game client.

mod ai;
mod assets;
mod cli;
mod game;
mod level;
mod levels_json;
mod net;
mod paths;
mod player;
mod scripting;
mod settings;
mod ui;

use anyhow::{anyhow, Result};
use clap::Parser;
use std::sync::Arc;

use crate::assets::resolver::Resolver;
use crate::cli::{Cli, Mode};
use crate::game::GamePlugin;
use crate::level::assets_bridge::resolve_level_assets;
use crate::level::build::build_scene_plan;
use crate::level::parse::parse_level_xml;
use crate::paths::AppPaths;
use crate::scripting::ScriptEngine;
use crate::settings::Settings;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.validate().map_err(|e| anyhow!(e))?;

    // Разрешение текстур: --texture-quality или из пресета --graphics.
    let texture_res = cli.resolved_texture_res();
    println!(
        "[cli] graphics={:?} texture-quality={:?} → resolution={}",
        cli.graphics, cli.texture_quality, texture_res
    );

    let paths = AppPaths::discover()?;
    paths.ensure_dirs()?;

    let settings = Settings::load(&paths.settings_json);
    let _ = levels_json::load_or_init(&paths.levels_json);

    match cli.mode {
        Mode::Single => run_single(&cli, &paths, &settings, texture_res),
        Mode::Host => run_host(&cli, &paths, &settings, texture_res),
        Mode::Join => run_join(&cli, &paths),
    }
}

/// Копирует cache-files/X_files/ → assets/models/X_files/, если ещё не скопировано.
fn sync_gltf_to_assets(paths: &AppPaths) -> Result<()> {
    let cache = &paths.cache_files_dir;
    let target_models = &paths.models_dir;
    if !cache.is_dir() { return Ok(()); }

    for entry in std::fs::read_dir(cache)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() { continue; }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.ends_with("_files") { continue; }

        let target_dir = target_models.join(name);
        if target_dir.is_dir() { continue; }
        std::fs::create_dir_all(&target_dir)?;
        copy_dir_recursive(&path, &target_dir)?;
        println!("[assets] gltf скопирован: {} → {}", path.display(), target_dir.display());
    }
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_recursive(&p, &target)?;
        } else {
            std::fs::copy(&p, &target)?;
        }
    }
    Ok(())
}

fn run_single(
    cli: &Cli,
    paths: &AppPaths,
    settings: &Settings,
    texture_res: &str,
) -> Result<()> {
    let level_number = cli.level.unwrap_or(0);
    let level_xml_path = paths.level_xml(level_number);

    if !level_xml_path.is_file() || cli.notes.is_some() {
        println!("Генерация уровня {level_number}...");
        if let Err(e) = level::gen::generate_level_full(
            paths,
            level_number,
            cli.notes.as_deref(),
            texture_res,
        ) {
            eprintln!("Генерация не удалась: {e}. Используем встроенный offline-уровень.");
            let fallback = crate::level::gen::offline_level_xml(level_number);
            std::fs::write(&level_xml_path, fallback)?;
        }
    }

    if let Err(e) = sync_gltf_to_assets(paths) {
        eprintln!("[assets] не удалось синхронизировать gltf: {e}");
    }

    let xml = std::fs::read_to_string(&level_xml_path)?;
    let level = parse_level_xml(&xml)?;

    let resolver = Resolver::open(paths)?;
    let assets = resolve_level_assets(&level, &resolver, &paths.assets_dir)?;

    let plan = Arc::new(build_scene_plan(&level, &assets)?);
    let assets_arc = Arc::new(assets);

    let mut engine = ScriptEngine::new()?;
    let scripts: Vec<(String, String)> = level
        .scripts
        .iter()
        .map(|s| (s.id.clone(), s.lua.clone()))
        .collect();
    engine
        .load_scripts(scripts.iter().map(|(a, b)| (a, b)))
        .map_err(|e| anyhow!("Lua: {e}"))?;

    println!("Запуск одиночной игры: уровень {level_number} (textures={texture_res})");
    run_bevy(plan, assets_arc, Some(engine), level_number, settings);
    Ok(())
}

fn run_host(
    cli: &Cli,
    paths: &AppPaths,
    settings: &Settings,
    texture_res: &str,
) -> Result<()> {
    let level_number = cli.level.unwrap_or(0);
    println!("Мультиплеер — ХОСТ на уровне {level_number}");
    let _ = crate::net::host::start_lobby(27015)?;
    run_single(cli, paths, settings, texture_res)
}

fn run_join(cli: &Cli, _paths: &AppPaths) -> Result<()> {
    let ip = cli.ip.as_deref().unwrap_or("");
    println!("Мультиплеер — ПОДКЛЮЧЕНИЕ к {ip}");
    let _ = crate::net::client::connect(&format!("{}:{}", ip, 27015), "player")?;
    Ok(())
}

fn run_bevy(
    plan: Arc<level::build::ScenePlan>,
    assets: Arc<level::assets_bridge::ResolvedAssets>,
    engine: Option<ScriptEngine>,
    level_number: u32,
    settings: &Settings,
) {
    use bevy::prelude::*;

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("Backrooms Infinity — Level {level_number}"),
            ..default()
        }),
        ..default()
    }));

    app.insert_resource(player::MouseSensitivity(settings.mouse_sensitivity));
    app.add_plugins(player::controller::PlayerPlugin);
    app.add_plugins(player::camera::CameraPlugin);
    app.add_plugins(ui::PausePlugin::default());
    app.add_plugins(GamePlugin {
        plan,
        assets,
        engine: std::sync::Mutex::new(engine),
        level_number,
    });
    app.run();
}
