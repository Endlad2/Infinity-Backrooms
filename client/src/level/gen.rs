//! Оркестратор генерации уровня (§6 ТЗ) — с двумя этапами.
//!
//! Этап 1: `assets::stage1::run_stage1` — наполняет cache-files/ ассетами
//!         из Poly Haven (текстуры + gltf-модели). Запускается только если
//!         XML уровня ещё не существует (см. main.rs).
//! Этап 2: HTML вики → промт → ИИ (search → reasoner) → парсинг XML → сохранение.
//!         В промт добавляется список файлов из cache-files/.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::ai::client::AiClient;
use crate::ai::parse::extract_level_xml;
use crate::ai::prompt::{build_level_request, build_level_request_reasoner, wiki_url};
use crate::assets::stage1::{run_stage1, scan_cache_files};
use crate::paths::AppPaths;

/// Скачать HTML страницы вики (best-effort, без паники при отсутствии сети).
pub fn fetch_wiki_html(number: u32) -> Option<String> {
    let url = wiki_url(number);
    match ureq::get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
    {
        Ok(resp) => resp.into_string().ok(),
        Err(_) => None,
    }
}

/// Сгенерировать XML уровня (без сохранения), уже имея список файлов кэша.
/// Порядок: search-модель -> reasoner -> offline fallback.
pub fn generate_level_xml_with_cache(
    number: u32,
    notes: Option<&str>,
    cached_files: &[String],
) -> Result<String> {
    let client = AiClient::new(crate::ai::client::DEFAULT_ENDPOINT);
    let wiki_html = fetch_wiki_html(number);
    let wiki_ref = wiki_html.as_deref();

    let req = build_level_request(number, wiki_ref, notes, cached_files);
    if let Ok(resp) = client.chat(&req) {
        let raw = if !resp.content.trim().is_empty() {
            resp.content.clone()
        } else {
            resp.reasoning_content.clone().unwrap_or_default()
        };
        if let Ok(xml) = extract_level_xml(&raw) {
            return Ok(xml);
        }
    }

    let req2 = build_level_request_reasoner(number, wiki_ref, notes, cached_files);
    if let Ok(resp) = client.chat(&req2) {
        let raw = if !resp.content.trim().is_empty() {
            resp.content.clone()
        } else {
            resp.reasoning_content.clone().unwrap_or_default()
        };
        if let Ok(xml) = extract_level_xml(&raw) {
            return Ok(xml);
        }
    }

    Ok(offline_fallback_level(number))
}

/// Совместимость со старым кодом: без cache_files.
pub fn generate_level_xml(number: u32, notes: Option<&str>) -> Result<String> {
    generate_level_xml_with_cache(number, notes, &[])
}

/// Сгенерировать и сохранить XML уровня в levels/{number}.xml.
pub fn generate_and_save(number: u32, notes: Option<&str>, out_path: &Path) -> Result<PathBuf> {
    let xml = generate_level_xml(number, notes)?;
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow!("cannot create levels dir: {}", e))?;
    }
    fs::write(out_path, xml.as_bytes())
        .map_err(|e| anyhow!("cannot write level xml: {}", e))?;
    Ok(out_path.to_path_buf())
}

/// Полный конвейер Этапа 1 + Этапа 2.
/// Вызывается из main.rs, когда level{N}.xml ещё не существует.
pub fn generate_level_full(
    paths: &AppPaths,
    number: u32,
    notes: Option<&str>,
    resolution: &str,
) -> Result<PathBuf> {
    println!("=== Этап 1: резолв ассетов через Poly Haven ===");
    match run_stage1(paths, number, notes, resolution) {
        Ok(rep) => {
            println!(
                "[stage1] Готово: скачано {} текстур, {} моделей, {} ИИ-фоллбэков",
                rep.downloaded_textures, rep.downloaded_models, rep.ai_fallbacks
            );
        }
        Err(e) => {
            eprintln!("[stage1] Ошибка резолва ассетов: {e}. Продолжаем без Poly Haven.");
        }
    }

    println!("=== Этап 2: генерация XML уровня через ИИ ===");
    let cached = scan_cache_files(&paths.cache_files_dir).unwrap_or_default();
    let xml = generate_level_xml_with_cache(number, notes, &cached)?;
    let out = paths.level_xml(number);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out, xml.as_bytes())?;
    println!("[stage2] XML сохранён в {}", out.display());
    Ok(out)
}

/// Синтетический уровень на случай полного отсутствия ИИ/сети.
pub fn offline_fallback_level(number: u32) -> String {
    let head = format!(
        "<level id=\"level_{n}\" name=\"Level {n}\" version=\"2.0\" engine=\"bevy\" engine_min=\"0.14\" format=\"bds-level/2\" gravity=\"0 -9.81 0\" ambient=\"0.3 0.3 0.35\" spawn_point=\"player_start\" time_scale=\"1.0\" seed=\"{seed}\">",
        n = number,
        seed = 1337u64 ^ (number as u64)
    );
    let body = concat!(
        "<meta><author>auto</author><description>Offline fallback (chunked v2)</description><tags>fallback, infinite, chunks</tags></meta>",
        "<chunk_size x=\"32\" y=\"16\" z=\"32\"/>",
        "<resources>",
        "<texture id=\"tex_wall\" color=\"#c7c08a\"/>",
        "<texture id=\"tex_floor\" color=\"#7a6f4a\"/>",
        "<material id=\"mat_wall\" texture=\"tex_wall\" roughness=\"0.9\" metallic=\"0.0\"/>",
        "<material id=\"mat_floor\" texture=\"tex_floor\" roughness=\"0.95\" metallic=\"0.0\"/>",
        "</resources>",
        "<prefabs>",
        "<prefab id=\"pf_floor\"><entity id=\"floor_root\" type=\"prop\" material=\"mat_floor\"><transform pos=\"0 -7.9 0\" rot=\"0 0 0\" scale=\"32 0.2 32\"/><render mesh=\"cube\"/><physics body=\"static\" collider=\"box\" size=\"32 0.2 32\"/></entity></prefab>",
        "<prefab id=\"pf_wall\"><entity id=\"wall_root\" type=\"prop\" material=\"mat_wall\"><transform pos=\"0 0 -15.75\" rot=\"0 0 0\" scale=\"32 16 0.5\"/><render mesh=\"cube\"/><physics body=\"static\" collider=\"box\" size=\"32 16 0.5\"/></entity></prefab>",
        "</prefabs>",
        "<chunks>",
        "<chunk id=\"chunk_corridor\" generator=\"default\" chance=\"80\">",
        "<entity inherit=\"pf_floor\" id=\"floor\"/>",
        "<entity inherit=\"pf_wall\" id=\"wall_n\"/>",
        "<entity id=\"lamp\" type=\"light\"><transform pos=\"0 6 0\" rot=\"0 0 0\" scale=\"1 1 1\"/><light kind=\"point\" color=\"#ffe9b0\" intensity=\"2.0\" range=\"12.0\"/></entity>",
        "</chunk>",
        "<chunk id=\"chunk_room\" generator=\"default\" chance=\"20\">",
        "<entity inherit=\"pf_floor\" id=\"floor\"/>",
        "<entity id=\"pillar\" type=\"prop\" material=\"mat_wall\"><transform pos=\"8 -4 8\" rot=\"0 0 0\" scale=\"1 8 1\"/><render mesh=\"cube\"/><physics body=\"static\" collider=\"box\" size=\"1 8 1\"/></entity>",
        "<entity id=\"lamp\" type=\"light\"><transform pos=\"0 6 0\" rot=\"0 0 0\" scale=\"1 1 1\"/><light kind=\"point\" color=\"#ffe9b0\" intensity=\"2.5\" range=\"14.0\"/></entity>",
        "</chunk>",
        "<chunk id=\"chunk_spawn\" generator=\"none\" chance=\"0\">",
        "<entity inherit=\"pf_floor\" id=\"floor\"/>",
        "</chunk>",
        "</chunks>",
        "<scripts>",
        "<script id=\"intro\"><lua>function on_level_start() api.log(\"fallback v2 loaded\") end</lua></script>",
        "</scripts>",
        "</level>"
    );
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}{}", head, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_fallback_is_valid_xml() {
        let x = offline_fallback_level(7);
        assert!(x.contains("bds-level/2"));
        assert!(x.contains("chunk_size"));
        assert!(x.contains("generator=\"default\""));
        assert!(x.contains("level_7"));
        crate::ai::parse::validate_xml(&x).expect("fallback must be valid xml");
    }

    #[test]
    fn wiki_url_contains_number() {
        assert!(wiki_url(5).ends_with("level-5"));
    }
}

/// Совместимость с main.rs: сгенерировать и сохранить уровень.
pub fn generate_level(number: u32, out_path: &Path, notes: Option<&str>) -> Result<()> {
    generate_and_save(number, notes, out_path)?;
    Ok(())
}

/// Совместимость с main.rs: алиас на offline_fallback_level.
pub fn offline_level_xml(number: u32) -> String {
    offline_fallback_level(number)
}
