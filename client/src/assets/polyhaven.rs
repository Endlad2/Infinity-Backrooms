//! Poly Haven API client (https://api.polyhaven.com).
//!
//! Реализует:
//!   * семантический поиск ассетов   (`GET /search?q=...&t=textures|models`)
//!   * метаданные одного ассета      (`GET /info/{id}`)
//!   * файловое дерево ассета        (`GET /files/{id}`)
//!   * скачивание файлов (CDN url).
//!
//! Retry-логика: при 429/503 — exponential backoff, максимум 3 попытки.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Deserialize;

pub const API_BASE: &str = "https://api.polyhaven.com";
pub const USER_AGENT: &str = "backrooms-infinity/0.2 (+https://example.invalid)";

/// Тип ассета в Poly Haven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolyKind {
    Textures,
    Models,
    Hdris,
}

impl PolyKind {
    pub fn as_query(&self) -> &'static str {
        match self {
            PolyKind::Textures => "textures",
            PolyKind::Models => "models",
            PolyKind::Hdris => "hdris",
        }
    }
}

/// Результат поиска одного ассета.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub slug: String,
    pub score: f64,
}

/// Метаданные ассета из `/info/{id}`.
#[derive(Debug, Clone, Default)]
pub struct AssetMeta {
    pub slug: String,
    pub name: String,
    pub description: String,
    /// 0=hdri, 1=texture, 2=model
    pub kind: u8,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub thumbnail_url: Option<String>,
    pub max_resolution: Option<[u32; 2]>,
}

/// Один файл внутри ассета.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub url: String,
    pub md5: Option<String>,
    pub size: Option<u64>,
    /// Для gltf/blend/fbx — include-файлы: { относительный путь -> FileEntry }.
    pub includes: BTreeMap<String, FileEntry>,
}

impl FileEntry {
    fn from_json(v: &serde_json::Value) -> Option<Self> {
        let url = v.get("url")?.as_str()?.to_string();
        let md5 = v.get("md5").and_then(|x| x.as_str()).map(|s| s.to_string());
        let size = v.get("size").and_then(|x| x.as_u64());
        let mut includes = BTreeMap::new();
        if let Some(inc) = v.get("include").and_then(|x| x.as_object()) {
            for (k, iv) in inc {
                if let Some(fe) = FileEntry::from_json(iv) {
                    includes.insert(k.clone(), fe);
                }
            }
        }
        Some(FileEntry { url, md5, size, includes })
    }
}

/// Файловое дерево ассета (плоский вид).
/// Ключ: (kind_alias, res, format), например ("diff", "8k", "png").
#[derive(Debug, Clone, Default)]
pub struct FilesTree {
    pub entries: BTreeMap<(String, String, String), FileEntry>,
}

impl FilesTree {
    /// Канонизируем имена карт: `Diffuse`/`diff`/`albedo` → `diff`, и т.д.
    pub fn map_kind_alias(kind: &str) -> &'static str {
        match kind.to_ascii_lowercase().as_str() {
            "diffuse" | "diff" | "albedo" | "color" => "diff",
            "nor_gl" | "normal" | "normal_gl" | "normalmap" | "nor" => "nor_gl",
            "rough" | "roughness" => "rough",
            "ao" | "occlusion" | "ambient_occlusion" => "ao",
            "disp" | "displacement" | "height" => "disp",
            "arm" | "ao_rough_metallic" | "orm" => "arm",
            "metal" | "metallic" => "metal",
            // для моделей
            "gltf" | "glb" => "gltf",
            "fbx" => "fbx",
            "usd" => "usd",
            "blend" => "blend",
            _ => "other",
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Найти файл по (kind_alias, res, форматы по предпочтению).
    pub fn find(
        &self,
        kind_alias: &str,
        res: &str,
        formats: &[&str],
    ) -> Option<&FileEntry> {
        for fmt in formats {
            let key = (
                kind_alias.to_string(),
                res.to_string(),
                fmt.to_string(),
            );
            if let Some(e) = self.entries.get(&key) {
                return Some(e);
            }
        }
        None
    }

    /// Все доступные резолюции для kind_alias (отсортированы по возрастанию).
    pub fn resolutions_for(&self, kind_alias: &str) -> Vec<String> {
        let mut s: Vec<String> = self
            .entries
            .keys()
            .filter(|(k, _, _)| k == kind_alias)
            .map(|(_, r, _)| r.clone())
            .collect();
        s.sort_by_key(|r| parse_resolution(r));
        s.dedup();
        s
    }
}

/// Парсит `"1k"` → 1024, `"8k"` → 8192. Неизвестное — 0.
pub fn parse_resolution(res: &str) -> u32 {
    let t = res.trim().to_ascii_lowercase();
    let num = t.trim_end_matches('k');
    num.parse::<u32>().map(|n| n * 1024).unwrap_or(0)
}

/// Клиент Poly Haven API.
pub struct PolyHavenClient {
    pub timeout: Duration,
    pub max_retries: u32,
}

impl Default for PolyHavenClient {
    fn default() -> Self {
        Self::new()
    }
}

impl PolyHavenClient {
    pub fn new() -> Self {
        Self { timeout: Duration::from_secs(120), max_retries: 3 }
    }

    fn get_json(&self, path_with_query: &str) -> Result<serde_json::Value> {
        let url = format!("{API_BASE}{path_with_query}");
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..self.max_retries {
            let resp = ureq::get(&url)
                .set("User-Agent", USER_AGENT)
                .set("Accept", "application/json")
                .timeout(self.timeout)
                .call();
            match resp {
                Ok(r) => {
                    let status = r.status();
                    if status == 429 || status == 503 {
                        let wait = 1u64 << attempt;
                        std::thread::sleep(Duration::from_secs(wait));
                        last_err = Some(anyhow!("Poly Haven HTTP {status} on {url}"));
                        continue;
                    }
                    let v: serde_json::Value = r
                        .into_json()
                        .map_err(|e| anyhow!("Poly Haven json parse error: {e}"))?;
                    return Ok(v);
                }
                Err(ureq::Error::Status(code, _)) if code == 429 || code == 503 => {
                    let wait = 1u64 << attempt;
                    std::thread::sleep(Duration::from_secs(wait));
                    last_err = Some(anyhow!("Poly Haven HTTP {code} on {url}"));
                }
                Err(e) => {
                    last_err = Some(anyhow!("Poly Haven request error: {e}"));
                    std::thread::sleep(Duration::from_millis(300 * (attempt as u64 + 1)));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("Poly Haven request failed")))
    }

    /// `GET /search?q=...&t=textures|models`
    pub fn search(
        &self,
        query: &str,
        kind: PolyKind,
        min_score: Option<f64>,
        limit: Option<u32>,
    ) -> Result<Vec<SearchHit>> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut path = format!("/search?q={}&t={}", urlencode(&q), kind.as_query());
        if let Some(m) = min_score {
            path.push_str(&format!("&min={m}"));
        }
        if let Some(l) = limit {
            path.push_str(&format!("&limit={l}"));
        }
        let v = self.get_json(&path)?;
        let results = v
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(results.len());
        for it in results {
            let slug = it
                .get("slug")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            if slug.is_empty() {
                continue;
            }
            let score = it.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
            out.push(SearchHit { slug, score });
        }
        Ok(out)
    }

    /// `GET /files/{id}` → плоское дерево.
    pub fn files(&self, slug: &str) -> Result<FilesTree> {
        let path = format!("/files/{}", urlencode(slug));
        let v = self.get_json(&path)?;
        let mut tree = FilesTree::default();
        let obj = v
            .as_object()
            .ok_or_else(|| anyhow!("/files/{slug}: not an object"))?;
        for (kind_raw, by_res) in obj {
            let kind_alias = FilesTree::map_kind_alias(kind_raw).to_string();
            let by_res = match by_res.as_object() {
                Some(o) => o,
                None => continue,
            };
            for (res, by_fmt) in by_res {
                let by_fmt = match by_fmt.as_object() {
                    Some(o) => o,
                    None => continue,
                };
                for (fmt, filev) in by_fmt {
                    if let Some(entry) = FileEntry::from_json(filev) {
                        tree.entries.insert(
                            (kind_alias.clone(), res.clone(), fmt.clone()),
                            entry,
                        );
                    }
                }
            }
        }
        Ok(tree)
    }

    /// `GET /info/{id}` → метаданные ассета.
    pub fn info(&self, slug: &str) -> Result<AssetMeta> {
        let path = format!("/info/{}", urlencode(slug));
        let v = self.get_json(&path)?;
        Ok(meta_from_json(slug, &v))
    }

    /// Скачать файл по прямой CDN-ссылке.
    pub fn download(&self, url: &str) -> Result<Vec<u8>> {
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..self.max_retries {
            match ureq::get(url)
                .set("User-Agent", USER_AGENT)
                .timeout(self.timeout)
                .call()
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status == 429 || status == 503 {
                        let wait = 1u64 << attempt;
                        std::thread::sleep(Duration::from_secs(wait));
                        last_err = Some(anyhow!("download HTTP {status} on {url}"));
                        continue;
                    }
                    let mut bytes = Vec::new();
                    use std::io::Read;
                    resp.into_reader()
                        .read_to_end(&mut bytes)
                        .map_err(|e| anyhow!("download read error: {e}"))?;
                    return Ok(bytes);
                }
                Err(ureq::Error::Status(code, _)) if code == 429 || code == 503 => {
                    let wait = 1u64 << attempt;
                    std::thread::sleep(Duration::from_secs(wait));
                    last_err = Some(anyhow!("download HTTP {code} on {url}"));
                }
                Err(e) => {
                    last_err = Some(anyhow!("download error: {e}"));
                    std::thread::sleep(Duration::from_millis(300 * (attempt as u64 + 1)));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("download failed")))
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn meta_from_json(slug: &str, v: &serde_json::Value) -> AssetMeta {
    let mut m = AssetMeta {
        slug: slug.to_string(),
        ..Default::default()
    };
    if let Some(s) = v.get("name").and_then(|x| x.as_str()) {
        m.name = s.to_string();
    }
    if let Some(s) = v.get("description").and_then(|x| x.as_str()) {
        m.description = s.to_string();
    }
    if let Some(k) = v.get("type").and_then(|x| x.as_u64()) {
        m.kind = k as u8;
    }
    if let Some(arr) = v.get("tags").and_then(|x| x.as_array()) {
        m.tags = arr
            .iter()
            .filter_map(|t| t.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(s) = v.get("category").and_then(|x| x.as_str()) {
        m.category = Some(s.to_string());
    }
    if let Some(s) = v.get("thumbnail_url").and_then(|x| x.as_str()) {
        m.thumbnail_url = Some(s.to_string());
    }
    if let Some(arr) = v.get("max_resolution").and_then(|x| x.as_array()) {
        if arr.len() == 2 {
            if let (Some(a), Some(b)) = (arr[0].as_u64(), arr[1].as_u64()) {
                m.max_resolution = Some([a as u32, b as u32]);
            }
        }
    }
    m
}

/// Минимальный percent-encode (достаточно для наших запросов).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resolution_basic() {
        assert_eq!(parse_resolution("1k"), 1024);
        assert_eq!(parse_resolution("8k"), 8192);
        assert_eq!(parse_resolution("16k"), 16384);
        assert_eq!(parse_resolution("garbage"), 0);
    }

    #[test]
    fn map_kind_alias_diffuse_and_diff() {
        assert_eq!(FilesTree::map_kind_alias("Diffuse"), "diff");
        assert_eq!(FilesTree::map_kind_alias("diff"), "diff");
        assert_eq!(FilesTree::map_kind_alias("albedo"), "diff");
        assert_eq!(FilesTree::map_kind_alias("nor_gl"), "nor_gl");
        assert_eq!(FilesTree::map_kind_alias("Rough"), "rough");
        assert_eq!(FilesTree::map_kind_alias("GLTF"), "gltf");
    }

    #[test]
    fn urlencode_plain_and_spaces() {
        assert_eq!(urlencode("aerial asphalt"), "aerial%20asphalt");
        assert_eq!(urlencode("Brick-Floor_003"), "Brick-Floor_003");
    }

    #[test]
    fn file_entry_from_json_basic() {
        let js = serde_json::json!({
            "url": "https://example.invalid/x.png",
            "md5": "abc",
            "size": 42,
        });
        let e = FileEntry::from_json(&js).unwrap();
        assert_eq!(e.url, "https://example.invalid/x.png");
        assert_eq!(e.md5.as_deref(), Some("abc"));
        assert_eq!(e.size, Some(42));
        assert!(e.includes.is_empty());
    }

    #[test]
    fn file_entry_from_json_with_includes() {
        let js = serde_json::json!({
            "url": "https://example.invalid/model.gltf",
            "include": {
                "textures/albedo.png": {"url": "https://example.invalid/albedo.png"}
            }
        });
        let e = FileEntry::from_json(&js).unwrap();
        assert_eq!(e.includes.len(), 1);
        assert!(e.includes.contains_key("textures/albedo.png"));
    }

    #[test]
    fn files_tree_find_falls_through_formats() {
        let mut t = FilesTree::default();
        t.entries.insert(
            ("diff".into(), "8k".into(), "jpg".into()),
            FileEntry {
                url: "u".into(),
                md5: None,
                size: None,
                includes: BTreeMap::new(),
            },
        );
        // png нет, но jpg — да
        let f = t.find("diff", "8k", &["png", "jpg"]).unwrap();
        assert_eq!(f.url, "u");
    }

    #[test]
    fn resolutions_for_sorts_numerically() {
        let mut t = FilesTree::default();
        for r in ["8k", "1k", "4k", "2k"] {
            t.entries.insert(
                ("diff".into(), r.into(), "png".into()),
                FileEntry {
                    url: "u".into(),
                    md5: None,
                    size: None,
                    includes: BTreeMap::new(),
                },
            );
        }
        let res = t.resolutions_for("diff");
        assert_eq!(res, vec!["1k", "2k", "4k", "8k"]);
    }
}
