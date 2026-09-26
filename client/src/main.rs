//! Точка входа Backrooms Infinity game client.
//! Парсит CLI, готовит каталоги данных, при необходимости запускает
//! двухэтапную генерацию уровня (Poly Haven → ИИ → XML), затем запускает Bevy.

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
use crate::game::{GamePlugin, ScenePlanResource};
use crate::level::assets_bridge::resolve_level_assets;
use crate::level::build::build_scene_plan;
use crate::level::parse::parse_level_xml;
use crate::paths::AppPaths;
use crate::scripting::ScriptEngine;
use crate::settings::Settings;

/// Разрешение по умолчанию для скачиваемых текстур (Этап 1).
const DEFAULT_TEXTURE_RES: &str = "8k";

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.validate().map_err(|e| anyhow!(e))?;

    let paths = AppPaths::discover()?;
    paths.ensure_dirs()?;

    let settings = Settings::load(&paths.settings_json);
    let _ = levels_json::load_or_init(&paths.levels_json);

    match cli.mode {
        Mode::Single => run_single(&cli, &paths, &settings),
        Mode::Host => run_host(&cli, &paths, &settings),
        Mode::Join => run_join(&cli, &paths),
    }
}

fn run_single(cli: &Cli, paths: &AppPaths, settings: &Settings) -> Result<()> {
    let level_number = cli.level.unwrap_or(0);
    let level_xml_path = paths.level_xml(level_number);

    // Двухэтапная генерация: только если XML ещё нет (или есть --notes).
    if !level_xml_path.is_file() || cli.notes.is_some() {
        println!("Генерация уровня {level_number}...");
        if let Err(e) = level::gen::generate_level_full(
            paths,
            level_number,
            cli.notes.as_deref(),
            DEFAULT_TEXTURE_RES,
        ) {
            eprintln!("Генерация не удалась: {e}. Используем встроенный offline-уровень.");
            let fallback = crate::level::gen::offline_level_xml(level_number);
            std::fs::write(&level_xml_path, fallback)?;
        }
    }

    // Загружаем XML.
    let xml = std::fs::read_to_string(&level_xml_path)?;
    let level = parse_level_xml(&xml)?;

    // Готовим ассеты (cache-files + БД + ИИ).
    let resolver = Resolver::open(paths)?;
    let assets = resolve_level_assets(&level, &resolver, &paths.assets_dir)?;

    // Строим план сцены.
    let plan = Arc::new(build_scene_plan(&level, &assets)?);

    // Lua-скрипты.
    let mut engine = ScriptEngine::new()?;
    let scripts: Vec<(String, String)> = level
        .scripts
        .iter()
        .map(|s| (s.id.clone(), s.lua.clone()))
        .collect();
    engine
        .load_scripts(scripts.iter().map(|(a, b)| (a, b)))
        .map_err(|e| anyhow!("Lua: {e}"))?;

    println!("Запуск одиночной игры: уровень {level_number}");
    run_bevy(plan, Some(engine), level_number, settings);
    Ok(())
}

fn run_host(cli: &Cli, paths: &AppPaths, settings: &Settings) -> Result<()> {
    let level_number = cli.level.unwrap_or(0);
    println!("Мультиплеер — ХОСТ на уровне {level_number}");
    let _ = crate::net::host::start_lobby(27015)?;
    println!("LAN IP показывается в окне игры (см. UI)");
    run_single(cli, paths, settings)
}

fn run_join(cli: &Cli, _paths: &AppPaths) -> Result<()> {
    let ip = cli.ip.as_deref().unwrap_or("");
    println!("Мультиплеер — ПОДКЛЮЧЕНИЕ к {ip}");
    let _ = crate::net::client::connect(&format!("{}:{}", ip, 27015), "player")?;
    Ok(())
}

fn run_bevy(
    plan: Arc<level::build::ScenePlan>,
    engine: Option<ScriptEngine>,
    level_number: u32,
    settings: &Settings,
) {
    use bevy::prelude::*;

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Backrooms Infinity — Level {level_number}"),
                ..default()
            }),
            ..default()
        }),
    );
    app.insert_resource(player::MouseSensitivity(settings.mouse_sensitivity));
    app.add_plugins(player::controller::PlayerPlugin);
    app.add_plugins(player::camera::CameraPlugin);
    app.add_plugins(ui::PausePlugin::default());
    app.add_plugins(GamePlugin {
        plan,
        engine: std::sync::Mutex::new(engine),
        level_number,
    });
    app.run();
}

#[allow(dead_code)]
fn _type_hint(_: ScenePlanResource) {}
