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
            paths, level_number, cli.notes.as_deref(), texture_res,
        ) {
            eprintln!("Генерация не удалась: {e}. Используем offline fallback.");
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
        .scripts.iter().map(|s| (s.id.clone(), s.lua.clone())).collect();
    engine.load_scripts(scripts.iter().map(|(a, b)| (a, b)))
        .map_err(|e| anyhow!("Lua: {e}"))?;

    println!("Запуск одиночной игры: уровень {level_number} (textures={texture_res})");
    run_bevy(plan, assets_arc, Some(engine), level_number, settings, None);
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

    // 1. Генерируем уровень, если его нет.
    let level_xml_path = paths.level_xml(level_number);
    if !level_xml_path.is_file() {
        println!("Генерация уровня для хоста...");
        if let Err(e) = level::gen::generate_level_full(
            paths, level_number, None, texture_res,
        ) {
            eprintln!("Генерация не удалась: {e}. Offline fallback.");
            let fallback = crate::level::gen::offline_level_xml(level_number);
            std::fs::write(&level_xml_path, fallback)?;
        }
    }

    // 2. UDP-лобби.
    let mut host = net::host::start_lobby_with_level(cli.net_port, level_number, cli.http_port)?;
    let lan_ip = net::http_server::lan_ip();
    println!("[host] UDP-лобби: {}:{}", lan_ip.map(|ip| ip.to_string()).unwrap_or_else(|| "0.0.0.0".into()), cli.net_port);

    // 3. HTTP-сервер для раздачи ассетов.
    let handle = net::http_server::start_http_server(
        paths.clone(),
        level_number,
        cli.http_port,
        format!("Backrooms Lobby {level_number}"),
    )?;
    println!(
        "[host] HTTP-сервер: {}",
        net::http_server::base_url_for_self(cli.http_port)
    );
    println!("[host] Скажи друзьям IP: {}", net::host::display_lan_ip());

    // 4. Обработка входящих пакетов в фоне (простой polling loop тут же в main).
    //    В реальности хост-цикл должен быть асинхронным, но для простоты
    //    запустим лобби-поток и перейдём в Bevy.
    let _ = host.poll();
    let _ = &handle; // держим handle живым на всё время работы
    std::mem::forget(handle); // отключаем Drop (сервер работает до конца процесса)

    // 5. Запускаем игру.
    if let Err(e) = sync_gltf_to_assets(paths) {
        eprintln!("[assets] gltf sync: {e}");
    }
    let xml = std::fs::read_to_string(&level_xml_path)?;
    let level = parse_level_xml(&xml)?;
    let resolver = Resolver::open(paths)?;
    let assets = resolve_level_assets(&level, &resolver, &paths.assets_dir)?;
    let plan = Arc::new(build_scene_plan(&level, &assets)?);
    let assets_arc = Arc::new(assets);

    let mut engine = ScriptEngine::new()?;
    let scripts: Vec<(String, String)> = level
        .scripts.iter().map(|s| (s.id.clone(), s.lua.clone())).collect();
    engine.load_scripts(scripts.iter().map(|(a, b)| (a, b)))?;

    let net_ctx = crate::game::NetContext::host(cli.net_port, cli.http_port, level_number);
    run_bevy(plan, assets_arc, Some(engine), level_number, settings, Some(net_ctx));
    Ok(())
}

fn run_join(cli: &Cli, paths: &AppPaths, texture_res: &str) -> Result<()> {
    let ip = cli.ip.as_deref().unwrap_or("").to_string();
    println!("Мультиплеер — ПОДКЛЮЧЕНИЕ к {ip}");
    let _ = texture_res;

    let client = net::client::connect(&format!("{}:{}", ip, cli.net_port), "player")?;
    let mut client = client;

    // Ждём ConnectAck, чтобы узнать http_port и level_number.
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

    // Скачиваем level.xml и все ассеты с хоста.
    if !net::http_server::probe_host(&base_url) {
        return Err(anyhow!("HTTP-сервер хоста {base_url} недоступен"));
    }

    net::http_server::download_level_xml(&base_url, paths, level_number)?;
    println!("[join] level.xml скачан");

    // Скачиваем все ассеты, на которые ссылается XML.
    download_all_assets_from_host(&base_url, paths, level_number)?;

    // Запускаем игру с NetContext::client.
    if let Err(e) = sync_gltf_to_assets(paths) {
        eprintln!("[assets] gltf sync: {e}");
    }
    let xml = std::fs::read_to_string(paths.level_xml(level_number))?;
    let level = parse_level_xml(&xml)?;
    let resolver = Resolver::open(paths)?;
    let assets = resolve_level_assets(&level, &resolver, &paths.assets_dir)?;
    let plan = Arc::new(build_scene_plan(&level, &assets)?);
    let assets_arc = Arc::new(assets);

    let mut engine = ScriptEngine::new()?;
    let scripts: Vec<(String, String)> = level
        .scripts.iter().map(|s| (s.id.clone(), s.lua.clone())).collect();
    engine.load_scripts(scripts.iter().map(|(a, b)| (a, b)))?;

    let net_ctx = crate::game::NetContext::client(base_url, level_number);
    let settings = Settings::load(&paths.settings_json);
    run_bevy(plan, assets_arc, Some(engine), level_number, &settings, Some(net_ctx));
    Ok(())
}

fn download_all_assets_from_host(
    base_url: &str,
    paths: &AppPaths,
    level_number: u32,
) -> Result<()> {
    let xml = std::fs::read_to_string(paths.level_xml(level_number))?;
    let level = parse_level_xml(&xml)?;

    // Все src=/path= из resources.
    let mut refs: Vec<String> = Vec::new();
    for t in &level.resources.textures {
        if let crate::level::model::TextureSource::File(p) = &t.source {
            refs.push(p.clone());
        }
    }
    for m in &level.resources.models {
        if let crate::level::model::ModelSource::File(p) = &m.source {
            refs.push(p.clone());
        }
    }

    for r in &refs {
        // Нормализуем: "assets/textures/X.png" → "X.png".
        let bare = r.strip_prefix("assets/textures/")
            .or_else(|| r.strip_prefix("assets/models/"))
            .or_else(|| r.strip_prefix("assets/"))
            .or_else(|| r.strip_prefix("cache-files/"))
            .unwrap_or(r);

        // Пробуем скачать напрямую.
        let target = paths.cache_files_dir.join(bare);
        if target.is_file() {
            continue; // уже есть
        }

        match net::http_server::download_file(base_url, bare, &paths.cache_files_dir) {
            Ok(p) => println!("[join] скачан {}", p.display()),
            Err(e) => {
                eprintln!("[join] не удалось скачать {bare}: {e}");
            }
        }
    }

    // Модели в подпапках X_files/: для каждого .gltf из <resources><model>/
    // скачиваем всю папку. Итеративно — идём по /manifest.json? Проще:
    // выкачать всю cache-files по индексу (нужен серверный /index).
    // Пока — только прямые ссылки, которые уже прошли.

    Ok(())
}

fn run_bevy(
    plan: Arc<level::build::ScenePlan>,
    assets: Arc<level::assets_bridge::ResolvedAssets>,
    engine: Option<ScriptEngine>,
    level_number: u32,
    settings: &Settings,
    net_ctx: Option<crate::game::NetContext>,
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
        net_ctx,
    });
    app.run();
}
