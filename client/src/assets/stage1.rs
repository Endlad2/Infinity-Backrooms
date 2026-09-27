//! Этап 1: наполнение cache-files/ ассетами с Poly Haven.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::ai::prompt::{build_stage1_user_prompt, wiki_url};
use crate::paths::AppPaths;

use super::polyhaven::{PolyHavenClient, PolyKind};

const AI_ENDPOINT: &str = "http://localhost:9655/v1/chat/completions";
const AI_MODEL_STAGE1: &str = "deepseek-chat";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRequest {
    pub kind: String,
    pub query: String,
    #[serde(default)]
    pub used_for: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Stage1Report {
    pub cached_before: usize,
    pub requested: usize,
    pub downloaded_textures: usize,
    pub downloaded_models: usize,
    pub ai_fallbacks: usize,
    pub failed: usize,
    pub nothing_needed: bool,
    pub wiki_html_len: usize,
    pub downloaded_files: Vec<PathBuf>,
}

/// Скачивает HTML страницы вики (best-effort).
pub fn fetch_wiki_html(number: u32) -> Option<String> {
    let url = wiki_url(number);
    println!("[stage1] Скачиваю HTML вики: {url}");
    match ureq::get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
    {
        Ok(resp) => {
            let s = resp.into_string().ok();
            match &s {
                Some(t) => println!("[stage1] HTML вики получен ({} символов)", t.len()),
                None => println!("[stage1] HTML вики пуст"),
            }
            s
        }
        Err(e) => {
            println!("[stage1] Не удалось скачать HTML вики: {e}");
            None
        }
    }
}

pub fn scan_cache_files(cache_dir: &Path) -> Result<Vec<String>> {
    if !cache_dir.is_dir() { return Ok(Vec::new()); }
    let mut v = Vec::new();
    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if let Some(name) = entry.file_name().to_str() {
            if ft.is_file() {
                v.push(name.to_string());
            } else if ft.is_dir() {
                v.push(format!("{name}/"));
            }
        }
    }
    v.sort();
    Ok(v)
}

fn ask_ai_for_requests(
    level_number: u32,
    existing_files: &[String],
    wiki_html: Option<&str>,
    notes: Option<&str>,
) -> Result<Vec<AssetRequest>> {
    let user = build_stage1_user_prompt(level_number, wiki_html, existing_files, notes);

    let body = json!({
        "model": AI_MODEL_STAGE1,
        "messages": [
            {"role": "system", "content": crate::ai::prompt::STAGE1_INSTRUCTION},
            {"role": "user", "content": user},
        ],
        "temperature": 0.4,
        "stream": false,
    });

    let resp = ureq::post(AI_ENDPOINT)
        .timeout(std::time::Duration::from_secs(180))
        .send_json(body)
        .map_err(|e| anyhow!("stage1 AI backend error: {e}"))?;
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| anyhow!("stage1 AI json error: {e}"))?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("stage1 AI ответ без content"))?;
    parse_stage1_json(content)
}

fn parse_stage1_json(raw: &str) -> Result<Vec<AssetRequest>> {
    let cleaned = strip_code_fence(raw);
    let start = cleaned.find('[')
        .ok_or_else(|| anyhow!("stage1: в ответе нет '['"))?;
    let end = cleaned.rfind(']')
        .ok_or_else(|| anyhow!("stage1: в ответе нет ']'"))?;
    let slice = &cleaned[start..=end];
    let arr: Vec<AssetRequest> = serde_json::from_str(slice)
        .map_err(|e| anyhow!("stage1: не удалось распарсить JSON: {e}\n---\n{slice}\n---"))?;
    Ok(arr)
}

fn strip_code_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let inner = match rest.find('\n') {
            Some(nl) => &rest[nl + 1..],
            None => rest,
        };
        let inner = inner.trim_end();
        let inner = inner.strip_suffix("```").unwrap_or(inner);
        return inner.trim().to_string();
    }
    t.to_string()
}

const PBR_MAPS: &[&str] = &["diff", "nor_gl", "rough", "ao", "disp", "arm"];

pub fn download_texture(
    client: &PolyHavenClient,
    slug: &str,
    meta_name: &str,
    cache_dir: &Path,
    resolution: &str,
) -> Result<Vec<PathBuf>> {
    let tree = client.files(slug)?;
    if tree.is_empty() {
        return Err(anyhow!("Poly Haven: /files/{slug} пустое дерево"));
    }
    let available = tree.resolutions_for("diff");
    let res = if available.iter().any(|r| r == resolution) {
        resolution.to_string()
    } else {
        available.last().cloned().unwrap_or_else(|| resolution.to_string())
    };

    let safe_name = sanitize_asset_name(meta_name);
    let mut created = Vec::new();

    for map in PBR_MAPS {
        let entry = match tree.find(map, &res, &["png", "jpg"]) {
            Some(e) => e, None => continue,
        };
        let bytes = client.download(&entry.url)?;
        let ext = guess_ext(&entry.url);
        let file_name = format!("{safe_name}_{map}.{ext}");
        let out = cache_dir.join(&file_name);
        let mut f = fs::File::create(&out)?;
        f.write_all(&bytes)?;
        created.push(out);
    }
    if created.is_empty() {
        return Err(anyhow!("Poly Haven: у {slug} нет ни одной PBR-карты"));
    }
    Ok(created)
}

pub fn download_model(
    client: &PolyHavenClient,
    slug: &str,
    meta_name: &str,
    cache_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let tree = client.files(slug)?;
    if tree.is_empty() {
        return Err(anyhow!("Poly Haven: /files/{slug} пустое дерево (model)"));
    }
    let resolutions = tree.resolutions_for("gltf");
    let res = resolutions.last().cloned().unwrap_or_else(|| "1k".to_string());
    let gltf_entry = tree.find("gltf", &res, &["gltf", "glb"])
        .ok_or_else(|| anyhow!("Poly Haven: у {slug} нет gltf {res}"))?;

    let safe_name = sanitize_asset_name(meta_name);
    let model_dir = cache_dir.join(format!("{safe_name}_files"));
    fs::create_dir_all(&model_dir)?;

    let mut created = Vec::new();
    let main_bytes = client.download(&gltf_entry.url)?;
    let main_ext = guess_ext(&gltf_entry.url);
    let main_path = model_dir.join(format!("{safe_name}.{main_ext}"));
    fs::write(&main_path, &main_bytes)?;
    created.push(main_path.clone());

    for (rel, entry) in &gltf_entry.includes {
        let target = model_dir.join(rel);
        if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
        let bytes = client.download(&entry.url)?;
        fs::write(&target, &bytes)?;
        created.push(target);
    }
    Ok(created)
}

fn try_polyhaven_for(
    client: &PolyHavenClient,
    req: &AssetRequest,
    cache_dir: &Path,
    resolution: &str,
) -> Result<Option<Vec<PathBuf>>> {
    let is_model = req.kind.eq_ignore_ascii_case("model");
    let kind = if is_model { PolyKind::Models } else { PolyKind::Textures };
    let hits = client.search(&req.query, kind, Some(0.35), Some(10))?;
    if hits.is_empty() { return Ok(None); }
    let slug = &hits[0].slug;

    let meta = client.info(slug).unwrap_or_else(|_| super::polyhaven::AssetMeta {
        slug: slug.clone(),
        name: slug.clone(),
        ..Default::default()
    });

    let created = if is_model {
        download_model(client, slug, &meta.name, cache_dir)?
    } else {
        download_texture(client, slug, &meta.name, cache_dir, resolution)?
    };
    Ok(Some(created))
}

pub fn run_stage1(
    paths: &AppPaths,
    level_number: u32,
    notes: Option<&str>,
    resolution: &str,
) -> Result<Stage1Report> {
    // Публичный вход: если вызывают напрямую — HTML качаем сами.
    let wiki_html = fetch_wiki_html(level_number);
    run_stage1_with_wiki(paths, level_number, notes, resolution, wiki_html.as_deref())
}

/// Внутренний вход с уже скачанным HTML, чтобы не делать повторный запрос.
pub fn run_stage1_with_wiki(
    paths: &AppPaths,
    level_number: u32,
    notes: Option<&str>,
    resolution: &str,
    wiki_html: Option<&str>,
) -> Result<Stage1Report> {
    let cache_dir = &paths.cache_files_dir;
    fs::create_dir_all(cache_dir)?;

    let mut report = Stage1Report::default();
    report.wiki_html_len = wiki_html.map(|s| s.len()).unwrap_or(0);

    let existing = scan_cache_files(cache_dir)?;
    report.cached_before = existing.len();
    println!("[stage1] В cache-files/ уже {} файлов", report.cached_before);

    let requests = ask_ai_for_requests(
        level_number,
        &existing,
        wiki_html,
        notes,
    ).map_err(|e| anyhow!("stage1: ИИ не ответил: {e}"))?;
    report.requested = requests.len();

    if requests.is_empty() {
        println!("[stage1] ИИ вернул пустой список — всё уже есть в кэше.");
        report.nothing_needed = true;
        return Ok(report);
    }
    println!("[stage1] ИИ запросил {} ассетов", requests.len());

    let client = PolyHavenClient::new();
    let mut ai_fallback_requests: Vec<AssetRequest> = Vec::new();

    for req in &requests {
        let is_model = req.kind.eq_ignore_ascii_case("model");
        match try_polyhaven_for(&client, req, cache_dir, resolution) {
            Ok(Some(files)) => {
                if is_model { report.downloaded_models += 1; }
                else { report.downloaded_textures += 1; }
                for f in files {
                    println!("[stage1] + {}", f.display());
                    report.downloaded_files.push(f);
                }
            }
            Ok(None) => {
                println!("[stage1] Poly Haven не нашёл '{}' ({})", req.query, req.kind);
                ai_fallback_requests.push(req.clone());
            }
            Err(e) => {
                println!("[stage1] Ошибка '{}': {e}", req.query);
                report.failed += 1;
                ai_fallback_requests.push(req.clone());
            }
        }
    }

    if !ai_fallback_requests.is_empty() {
        let manifest = cache_dir.join("_ai_fallback_requests.json");
        let s = serde_json::to_string_pretty(&ai_fallback_requests)?;
        fs::write(&manifest, s)?;
        println!(
            "[stage1] {} ассетов уйдут на ИИ-генерацию ({})",
            ai_fallback_requests.len(),
            manifest.display()
        );
        report.ai_fallbacks = ai_fallback_requests.len();
    }

    Ok(report)
}

pub fn sanitize_asset_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if ch == ' ' || ch == '-' || ch == '_' {
            out.push('_');
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_us = false;
    for ch in out.chars() {
        if ch == '_' {
            if !prev_us { collapsed.push('_'); }
            prev_us = true;
        } else {
            collapsed.push(ch);
            prev_us = false;
        }
    }
    collapsed.trim_matches('_').to_string()
}

fn guess_ext(url: &str) -> String {
    let no_q = url.split('?').next().unwrap_or(url);
    let last = no_q.rsplit('/').next().unwrap_or("");
    match last.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && ext.len() <= 5 => ext.to_lowercase(),
        _ => "png".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_basic() {
        assert_eq!(sanitize_asset_name("Aerial Asphalt 01"), "Aerial_Asphalt_01");
        assert_eq!(sanitize_asset_name("Brick-Floor_003"), "Brick_Floor_003");
    }

    #[test]
    fn parse_stage1_empty_array() {
        let v = parse_stage1_json("[]").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn parse_stage1_basic() {
        let raw = r#"[{"kind":"texture","query":"concrete wall","used_for":"walls","tags":["concrete"]}]"#;
        let v = parse_stage1_json(raw).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, "texture");
    }
}
