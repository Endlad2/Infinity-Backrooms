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
use crate::paths::{AppPaths, AppPathsResource};
use crate::scripting::ScriptEngine;
use crate::settings::Settings;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.validate().map_err(|e| anyhow!(e))?;

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
        Mode::Join => run_join(&cli, &paths, texture_res),
    }
}

fn sync_gltf_to_assets(paths: &AppPaths) -> Result<()> {
    let cache = &paths.cache_files_dir;
    let target_models = &paths.models_dir;
    if !cache.is_dir() { return Ok(()); }
    for entry in std::fs::read_dir(cache)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() { continue; }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n, None => continue,
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
        if p.is_dir() { copy_dir_recursive(&p, &target)?; }
        else { std::fs::copy(&p, &target)?; }
    }
    Ok(())
}

fn run_single(cli: &Cli, paths: &AppPaths, settings: &Settings, texture_res: &str) -> Result<()> {
    let level_number = cli.level.unwrap_or(0);
    let level_xml_path = paths.level_xml(level_number);

    if !level_xml_path.is_file() || cli.notes.is_some() {
        println!("Генерация уровня {level_number}...");
        if let Err(e) = level::gen::generate_level_full(
            paths, level_number, cli.notes.as_deref(), texture_res,
        ) {
            eprintln!("Генерация не удалась: {e}. Offline fallback.");
            let fallback = crate::level::gen::offline_level_xml(level_number);
            std::fs::write(&level_xml_path, fallback)?;
        }
    }
    if let Err(e) = sync_gltf_to_assets(paths) { eprintln!("[assets] gltf sync: {e}"); }

    let xml = std::fs::read_to_string(&level_xml_path)?;
    let level = parse_level_xml(&xml)?;
    let resolver = Resolver::open(paths)?;
    let assets = resolve_level_assets(&level, &resolver, &paths.assets_dir)?;
    let plan = Arc::new(build_scene_plan(&level, &assets)?);
    let assets_arc = Arc::new(assets);

    let mut engine = ScriptEngine::new()?;
    let scripts: Vec<(String, String)> = level.scripts.iter()
        .map(|s| (s.id.clone(), s.lua.clone())).collect();
    engine.load_scripts(scripts.iter().map(|(a, b)| (a, b)))?;

    println!("Запуск одиночной игры: уровень {level_number} (textures={texture_res})");
    run_bevy(plan, assets_arc, Some(engine), level_number, settings, paths.clone(), None);
    Ok(())
}

fn run_host(cli: &Cli, paths: &AppPaths, settings: &Settings, texture_res: &str) -> Result<()> {
    let level_number = cli.level.unwrap_or(0);
    println!("Мультиплеер — ХОСТ на уровне {level_number}");

    let level_xml_path = paths.level_xml(level_number);
    if !level_xml_path.is_file() {
        println!("Генерация уровня для хоста...");
        if let Err(e) = level::gen::generate_level_full(paths, level_number, None, texture_res) {
            eprintln!("Генерация не удалась: {e}. Offline fallback.");
            let fallback = crate::level::gen::offline_level_xml(level_number);
            std::fs::write(&level_xml_path, fallback)?;
        }
    }

    let handle = net::http_server::start_http_server(
        paths.clone(), level_number, cli.http_port,
        format!("Backrooms Lobby {level_number}"),
    )?;
    println!("[host] HTTP-сервер: {}", net::http_server::base_url_for_self(cli.http_port));
    println!("[host] IP: {}", net::host::display_lan_ip());
    std::mem::forget(handle);

    let _host = net::host::start_lobby_with_level(cli.net_port, level_number, cli.http_port)?;
    if let Err(e) = sync_gltf_to_assets(paths) { eprintln!("[assets] gltf sync: {e}"); }

    let xml = std::fs::read_to_string(&level_xml_path)?;
    let level = parse_level_xml(&xml)?;
    let resolver = Resolver::open(paths)?;
    let assets = resolve_level_assets(&level, &resolver, &paths.assets_dir)?;
    let plan = Arc::new(build_scene_plan(&level, &assets)?);
    let assets_arc = Arc::new(assets);

    let mut engine = ScriptEngine::new()?;
    let scripts: Vec<(String, String)> = level.scripts.iter()
        .map(|s| (s.id.clone(), s.lua.clone())).collect();
    engine.load_scripts(scripts.iter().map(|(a, b)| (a, b)))?;

    let net_ctx = crate::game::NetContext::host(cli.net_port, cli.http_port, level_number);
    run_bevy(plan, assets_arc, Some(engine), level_number, settings, paths.clone(), Some(net_ctx));
    Ok(())
}

fn run_join(cli: &Cli, paths: &AppPaths, texture_res: &str) -> Result<()> {
    let ip = cli.ip.as_deref().unwrap_or("").to_string();
    println!("Мультиплеер — ПОДКЛЮЧЕНИЕ к {ip}");
    let _ = texture_res;

    let mut client = net::client::connect(&format!("{}:{}", ip, cli.net_port), "player")?;
    let start = std::time::Instant::now();
    let mut host_base_url: Option<String> = None;
    let mut level_number: u32 = 0;
    while start.elapsed() < std::time::Duration::from_secs(10) {
        let _ = client.poll();
        if let Some(url) = client.host_base_url() {
            host_base_url = Some(url);
            level_number = client.host_level_number.unwrap_or(0);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let Some(base_url) = host_base_url else {
        return Err(anyhow!("Хост не ответил ConnectAck за 10 секунд"));
    };
    println!("[join] Хост HTTP: {base_url}, уровень {level_number}");

    if !net::http_server::probe_host(&base_url) {
        return Err(anyhow!("HTTP-сервер хоста {base_url} недоступен"));
    }
    net::http_server::download_level_xml(&base_url, paths, level_number)?;
    println!("[join] level.xml скачан");

    match net::http_server::fetch_index(&base_url) {
        Ok(files) => {
            println!("[join] файлов у хоста: {}", files.len());
            for f in &files {
                let target = paths.cache_files_dir.join(f);
                if target.is_file() { continue; }
                match net::http_server::download_file(&base_url, f, &paths.cache_files_dir) {
                    Ok(p) => println!("[join] + {}", p.display()),
                    Err(e) => eprintln!("[join] ошибка {f}: {e}"),
                }
            }
        }
        Err(e) => eprintln!("[join] index.json недоступен: {e}"),
    }

    if let Err(e) = sync_gltf_to_assets(paths) { eprintln!("[assets] gltf sync: {e}"); }
    let xml = std::fs::read_to_string(paths.level_xml(level_number))?;
    let level = parse_level_xml(&xml)?;
    let resolver = Resolver::open(paths)?;
    let assets = resolve_level_assets(&level, &resolver, &paths.assets_dir)?;
    let plan = Arc::new(build_scene_plan(&level, &assets)?);
    let assets_arc = Arc::new(assets);

    let mut engine = ScriptEngine::new()?;
    let scripts: Vec<(String, String)> = level.scripts.iter()
        .map(|s| (s.id.clone(), s.lua.clone())).collect();
    engine.load_scripts(scripts.iter().map(|(a, b)| (a, b)))?;

    let net_ctx = crate::game::NetContext::client(base_url, level_number);
    let settings = Settings::load(&paths.settings_json);
    run_bevy(plan, assets_arc, Some(engine), level_number, &settings, paths.clone(), Some(net_ctx));
    Ok(())
}

fn run_bevy(
    plan: Arc<level::build::ScenePlan>,
    assets: Arc<level::assets_bridge::ResolvedAssets>,
    engine: Option<ScriptEngine>,
    level_number: u32,
    settings: &Settings,
    paths: AppPaths,
    net_ctx: Option<crate::game::NetContext>,
) {
    use bevy::prelude::*;
    use bevy::window::{CursorGrabMode, CursorOptions};

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("Backrooms Infinity — Level {level_number}"),
            cursor: CursorOptions {
                visible: false,
                grab_mode: CursorGrabMode::Locked,
                ..default()
            },
            ..default()
        }),
        ..default()
    }));

    app.insert_resource(AppPathsResource::new(paths));
    app.insert_resource(player::MouseSensitivity(settings.mouse_sensitivity));
    app.add_plugins(player::controller::PlayerPlugin);
    app.add_plugins(player::camera::CameraPlugin);
    app.add_plugins(player::cursor_grab::CursorGrabPlugin);
    app.add_plugins(ui::PausePlugin::default());
    app.add_plugins(GamePlugin {
        plan, assets,
        engine: std::sync::Mutex::new(engine),
        level_number,
        net_ctx,
    });
    app.run();
}
