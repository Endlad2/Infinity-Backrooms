//! Оркестратор Этапа 2 (генерация XML).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::ai::client::AiClient;
use crate::ai::parse::{extract_level_xml_diag, looks_like_level};
use crate::ai::prompt::{build_level_request, build_level_request_reasoner, wiki_url};
use crate::assets::stage1::{fetch_wiki_html, run_stage1, scan_cache_files};
use crate::paths::AppPaths;

/// Публичный интерфейс — оставляем для совместимости.
pub fn fetch_wiki_html_pub(number: u32) -> Option<String> {
    fetch_wiki_html(number)
}

const MAX_PRINT_CHARS: usize = 12_000;

fn dump_ai_answer(label: &str, content: &str, reasoning: &str) {
    println!("\n========== [stage2] {label} ==========");
    if !content.is_empty() {
        let shown: String = content.chars().take(MAX_PRINT_CHARS).collect();
        println!("--- content ({} символов{}) ---", content.len(),
            if content.len() > MAX_PRINT_CHARS { ", обрезано" } else { "" });
        println!("{shown}");
    } else {
        println!("--- content: (пусто) ---");
    }
    if !reasoning.is_empty() {
        let shown: String = reasoning.chars().take(MAX_PRINT_CHARS).collect();
        println!("--- reasoning_content ({} символов{}) ---", reasoning.len(),
            if reasoning.len() > MAX_PRINT_CHARS { ", обрезано" } else { "" });
        println!("{shown}");
    }
    println!("========== [stage2] конец {label} ==========\n");
}

fn try_extract(label: &str, content: &str, reasoning: &str) -> Option<String> {
    if !content.trim().is_empty() {
        match extract_level_xml_diag(content) {
            Ok(r) => {
                println!("[stage2] {label}: XML извлечён из content ({}, {} символов)",
                    r.note, r.xml.len());
                return Some(r.xml);
            }
            Err(e) => println!("[stage2] {label}: не удалось из content: {e}"),
        }
    }
    if !reasoning.trim().is_empty() {
        match extract_level_xml_diag(reasoning) {
            Ok(r) => {
                println!("[stage2] {label}: XML извлечён из reasoning_content ({}, {})",
                    r.note, r.xml.len());
                return Some(r.xml);
            }
            Err(e) => println!("[stage2] {label}: не удалось из reasoning: {e}"),
        }
    }
    if !looks_like_level(content) && !looks_like_level(reasoning) {
        println!("[stage2] {label}: нигде нет '<level'");
    }
    None
}

pub fn generate_level_xml_with_cache(
    number: u32,
    notes: Option<&str>,
    cached_files: &[String],
    wiki_html: Option<&str>,
) -> Result<String> {
    let client = AiClient::new(crate::ai::client::DEFAULT_ENDPOINT);

    println!("[stage2] Запрос к модели {} ...", crate::ai::client::MODEL_LEVEL_SEARCH);
    let req = build_level_request(number, wiki_html, notes, cached_files);
    match client.chat(&req) {
        Ok(resp) => {
            dump_ai_answer("ответ (search-модель)", &resp.content,
                resp.reasoning_content.as_deref().unwrap_or(""));
            if let Some(xml) = try_extract("search-модель", &resp.content,
                resp.reasoning_content.as_deref().unwrap_or("")) {
                return Ok(xml);
            }
        }
        Err(e) => println!("[stage2] Ошибка search: {e}"),
    }

    println!("[stage2] Запрос к модели {} ...", crate::ai::client::MODEL_REASONER);
    let req2 = build_level_request_reasoner(number, wiki_html, notes, cached_files);
    match client.chat(&req2) {
        Ok(resp) => {
            dump_ai_answer("ответ (reasoner)", &resp.content,
                resp.reasoning_content.as_deref().unwrap_or(""));
            if let Some(xml) = try_extract("reasoner", &resp.content,
                resp.reasoning_content.as_deref().unwrap_or("")) {
                return Ok(xml);
            }
        }
        Err(e) => println!("[stage2] Ошибка reasoner: {e}"),
    }

    println!("[stage2] Оба запроса не дали валидный XML — offline fallback.");
    Ok(offline_fallback_level(number))
}

pub fn generate_level_xml(number: u32, notes: Option<&str>) -> Result<String> {
    generate_level_xml_with_cache(number, notes, &[], None)
}

pub fn generate_and_save(number: u32, notes: Option<&str>, out_path: &Path) -> Result<PathBuf> {
    let xml = generate_level_xml(number, notes)?;
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out_path, xml.as_bytes())?;
    Ok(out_path.to_path_buf())
}

pub fn generate_level_full(
    paths: &AppPaths,
    number: u32,
    notes: Option<&str>,
    resolution: &str,
) -> Result<PathBuf> {
    println!("=== Этап 1: резолв ассетов через Poly Haven ===");

    // Один раз качаем HTML вики — используется и на Этапе 1, и на Этапе 2.
    let wiki_html = fetch_wiki_html(number);

    match run_stage1_with_html(paths, number, notes, resolution, wiki_html.as_deref()) {
        Ok(rep) => {
            println!(
                "[stage1] Готово: {} текстур, {} моделей, {} ИИ-фоллбэков, wiki={} симв.",
                rep.downloaded_textures, rep.downloaded_models,
                rep.ai_fallbacks, rep.wiki_html_len
            );
        }
        Err(e) => eprintln!("[stage1] Ошибка: {e}. Продолжаем без Poly Haven."),
    }

    println!("=== Этап 2: генерация XML уровня через ИИ ===");
    let cached = scan_cache_files(&paths.cache_files_dir).unwrap_or_default();
    println!("[stage2] Файлов в cache-files/ передано ИИ: {}", cached.len());

    let xml = generate_level_xml_with_cache(
        number, notes, &cached, wiki_html.as_deref(),
    )?;

    let out = paths.level_xml(number);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out, xml.as_bytes())?;
    println!("[stage2] XML сохранён в {}", out.display());
    println!("[stage2] Размер XML: {} байт", xml.len());

    if xml.contains("Offline fallback") {
        println!("[stage2] ⚠️ сохранён offline-fallback, а не ИИ-уровень.");
    }

    Ok(out)
}

/// Обёртка над run_stage1, которая принимает уже скачанный HTML,
/// чтобы не делать повторный HTTP-запрос.
pub fn run_stage1_with_html(
    paths: &AppPaths,
    number: u32,
    notes: Option<&str>,
    resolution: &str,
    wiki_html: Option<&str>,
) -> Result<crate::assets::stage1::Stage1Report> {
    crate::assets::stage1::run_stage1_with_wiki(paths, number, notes, resolution, wiki_html)
}

pub fn offline_fallback_level(number: u32) -> String {
    let head = format!(
        "<level id=\"level_{n}\" name=\"Level {n}\" version=\"2.0\" engine=\"bevy\" engine_min=\"0.14\" format=\"bds-level/2\" gravity=\"0 -9.81 0\" ambient=\"0.3 0.3 0.35\" spawn_point=\"player_start\" time_scale=\"1.0\" seed=\"{seed}\">",
        n = number, seed = 1337u64 ^ (number as u64)
    );
    let body = concat!(
        "<meta><author>auto</author><description>Offline fallback (chunked v2)</description><tags>fallback, infinite, chunks</tags></meta>",
        "<chunk_size x=\"32\" y=\"16\" z=\"32\"/>",
        "<resources><texture id=\"tex_wall\" color=\"#c7c08a\"/><texture id=\"tex_floor\" color=\"#7a6f4a\"/></resources>",
        "<materials><material id=\"mat_wall\" albedo=\"tex_wall\" roughness=\"0.9\"/><material id=\"mat_floor\" albedo=\"tex_floor\" roughness=\"0.95\"/></materials>",
        "<prefabs>",
        "<prefab id=\"pf_floor\"><entity type=\"static_body\"><transform pos=\"0 -7.9 0\" scale=\"32 0.2 32\"/><render mesh=\"cube\" material=\"mat_floor\"/><physics body=\"static\" collider=\"box\"/></entity></prefab>",
        "<prefab id=\"pf_wall\"><entity type=\"static_body\"><transform pos=\"0 0 -15.75\" scale=\"32 16 0.5\"/><render mesh=\"cube\" material=\"mat_wall\"/><physics body=\"static\" collider=\"box\"/></entity></prefab>",
        "</prefabs>",
        "<chunks>",
        "<chunk id=\"chunk_corridor\" generator=\"default\" chance=\"80\">",
        "<entity inherit=\"pf_floor\" id=\"floor\"/>",
        "<entity inherit=\"pf_wall\" id=\"wall_n\"/>",
        "<entity id=\"lamp\" type=\"light\"><transform pos=\"0 6 0\"/><light kind=\"point\" color=\"#ffe9b0\" intensity=\"2.0\" range=\"12.0\"/></entity>",
        "</chunk>",
        "<chunk id=\"chunk_spawn\" generator=\"none\" chance=\"0\">",
        "<entity id=\"player_start\" type=\"player\"><transform pos=\"0 1 0\"/><stats hp=\"100\" hp_max=\"100\" speed=\"5\" jump=\"6\" faction=\"players\"/><camera mode=\"first_person\" fov=\"75\"/><physics body=\"kinematic\" collider=\"capsule\" radius=\"0.4\" height=\"1.8\"/></entity>",
        "<entity inherit=\"pf_floor\" id=\"floor\"/>",
        "<entity id=\"spawn_lamp\" type=\"light\"><transform pos=\"0 3 0\"/><light kind=\"point\" color=\"#ffe9b0\" intensity=\"3.0\" range=\"14.0\"/></entity>",
        "</chunk>",
        "</chunks>",
        "<scripts><script id=\"intro\"><lua>function on_level_start() end</lua></script></scripts>",
        "</level>"
    );
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}{}", head, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_is_valid_and_has_player() {
        let x = offline_fallback_level(7);
        assert!(x.contains("player_start"));
        assert!(x.contains("type=\"player\""));
        assert!(x.contains("bds-level/2"));
    }

    #[test]
    fn wiki_url_ok() {
        assert!(wiki_url(5).ends_with("level-5"));
    }
}

pub fn generate_level(number: u32, out_path: &Path, notes: Option<&str>) -> Result<()> {
    generate_and_save(number, notes, out_path)?;
    Ok(())
}

pub fn offline_level_xml(number: u32) -> String {
    offline_fallback_level(number)
}
