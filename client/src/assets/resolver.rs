//! Высокоуровневый резолвер ассетов.
//!
//! Приоритет (обновлён под Poly Haven Этап 1):
//!   1. Если задан src=/path= — читаем файл. При этом поддерживаем 2 формы:
//!         * `assets/textures/X.png` — файл ищется СНАЧАЛА в cache-files/ (Poly Haven),
//!           потом в assets/textures/.
//!         * `cache-files/X.png` — прямо из cache-files/.
//!         * абсолютный путь — как есть.
//!   2. Иначе ищем в assets.db по всем тегам.
//!   3. Если не найдено — генерируем ИИ-ассет, кэшируем в БД и возвращаем.
//!
//! PBR-набор: если рядом с diff-файлом лежат nor_gl/rough/ao/disp/arm —
//! они кладутся в `ResolvedAsset::extra_maps`, чтобы движок мог собрать
//! полноценный StandardMaterial.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::db::AssetDb;
use super::gen;
use super::svg;
use crate::paths::AppPaths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetKind {
    Texture,
    Model,
}

/// Набор PBR-карт рядом с основным файлом.
/// Ключ — канонизированное имя карты: "nor_gl", "rough", "ao", "disp", "arm".
#[derive(Debug, Clone, Default)]
pub struct PbrMaps {
    pub nor_gl: Option<Vec<u8>>,
    pub rough: Option<Vec<u8>>,
    pub ao: Option<Vec<u8>>,
    pub disp: Option<Vec<u8>>,
    pub arm: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct ResolvedAsset {
    pub kind: AssetKind,
    /// Основной файл (diff для текстур, gltf/glb для моделей).
    pub bytes: Vec<u8>,
    /// Откуда взят ассет: "cache-files" | "src" | "db" | "ai".
    pub source: &'static str,
    pub tags: Vec<String>,
    /// Полный путь к файлу (если был src/cache).
    pub path: Option<PathBuf>,
    /// PBR-карты того же ассета (только для текстур).
    pub pbr: PbrMaps,
    /// Абсолютные пути к include-файлам модели (для gltf).
    pub includes: Vec<PathBuf>,
}

pub struct Resolver {
    db: AssetDb,
    paths: AppPaths,
}

impl Resolver {
    pub fn open(paths: &AppPaths) -> Result<Self> {
        let db = AssetDb::open(&paths.assets_db)
            .map_err(|e| anyhow::anyhow!("открытие assets.db: {e}"))?;
        Ok(Self { db, paths: paths.clone() })
    }

    pub fn db(&self) -> &AssetDb {
        &self.db
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// Резолв текстуры.
    /// Поддерживает src (имя файла в cache-files/ или assets/textures/), теги и ИИ.
    pub fn resolve_texture(
        &self,
        src: Option<&str>,
        tags: &[String],
        extra_prompt: &str,
        size: u32,
    ) -> Result<ResolvedAsset> {
        // 1. src-файл имеет приоритет. Ищем в cache-files/, потом в assets/.
        if let Some(s) = src {
            if let Some(path) = self.locate_texture_file(s) {
                if let Ok(bytes) = std::fs::read(&path) {
                    let pbr = self.collect_pbr_maps(&path);
                    return Ok(ResolvedAsset {
                        kind: AssetKind::Texture,
                        bytes,
                        source: if path.starts_with(&self.paths.cache_files_dir) {
                            "cache-files"
                        } else {
                            "src"
                        },
                        tags: tags.to_vec(),
                        path: Some(path),
                        pbr,
                        includes: Vec::new(),
                    });
                }
            }
        }

        // 2. БД по полному набору тегов.
        let tags_ref: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
        if !tags.is_empty() {
            if let Ok(Some(bytes)) = self.db.find_texture(&tags_ref) {
                return Ok(ResolvedAsset {
                    kind: AssetKind::Texture,
                    bytes,
                    source: "db",
                    tags: tags.to_vec(),
                    path: None,
                    pbr: PbrMaps::default(),
                    includes: Vec::new(),
                });
            }
        }

        // 3. ИИ-генерация (или fallback-заглушка) + кэш.
        let png = gen::generate_texture_png(extra_prompt, tags, size)?;
        if !tags.is_empty() {
            let _ = self.db.insert_texture(&tags_ref, &png);
        }
        Ok(ResolvedAsset {
            kind: AssetKind::Texture,
            bytes: png,
            source: "ai",
            tags: tags.to_vec(),
            path: None,
            pbr: PbrMaps::default(),
            includes: Vec::new(),
        })
    }

    /// Резолв модели (gltf/glb из cache-files, OBJ из assets/ или ИИ-генерация).
    pub fn resolve_model(
        &self,
        src: Option<&str>,
        tags: &[String],
        extra_prompt: &str,
    ) -> Result<ResolvedAsset> {
        if let Some(s) = src {
            if let Some(path) = self.locate_model_file(s) {
                if let Ok(bytes) = std::fs::read(&path) {
                    let includes = self.collect_gltf_includes(&path);
                    return Ok(ResolvedAsset {
                        kind: AssetKind::Model,
                        bytes,
                        source: if path.starts_with(&self.paths.cache_files_dir) {
                            "cache-files"
                        } else {
                            "src"
                        },
                        tags: tags.to_vec(),
                        path: Some(path),
                        pbr: PbrMaps::default(),
                        includes,
                    });
                }
            }
        }

        let tags_ref: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
        if !tags.is_empty() {
            if let Ok(Some(bytes)) = self.db.find_model(&tags_ref) {
                return Ok(ResolvedAsset {
                    kind: AssetKind::Model,
                    bytes,
                    source: "db",
                    tags: tags.to_vec(),
                    path: None,
                    pbr: PbrMaps::default(),
                    includes: Vec::new(),
                });
            }
        }

        let obj = gen::generate_obj(extra_prompt, tags)?;
        let bytes = obj.into_bytes();
        if !tags.is_empty() {
            let _ = self.db.insert_model(&tags_ref, &bytes);
        }
        Ok(ResolvedAsset {
            kind: AssetKind::Model,
            bytes,
            source: "ai",
            tags: tags.to_vec(),
            path: None,
            pbr: PbrMaps::default(),
            includes: Vec::new(),
        })
    }

    // ------------------------------------------------------------------
    // Локаторы файлов
    // ------------------------------------------------------------------

    /// Ищем текстуру: cache-files/ → assets/textures/ → абсолютный путь.
    fn locate_texture_file(&self, src: &str) -> Option<PathBuf> {
        // Прямой абсолютный путь.
        let p = Path::new(src);
        if p.is_absolute() && p.is_file() {
            return Some(p.to_path_buf());
        }

        // Срезаем возможный префикс "assets/textures/" или "assets/".
        let bare = src
            .strip_prefix("assets/textures/")
            .or_else(|| src.strip_prefix("assets/"))
            .or_else(|| src.strip_prefix("cache-files/"))
            .unwrap_or(src);

        // 1. cache-files/
        let in_cache = self.paths.cache_files_dir.join(bare);
        if in_cache.is_file() {
            return Some(in_cache);
        }

        // 2. assets/textures/ и assets/
        let in_textures = self.paths.textures_dir.join(bare);
        if in_textures.is_file() {
            return Some(in_textures);
        }
        let in_assets = self.paths.assets_dir.join(bare);
        if in_assets.is_file() {
            return Some(in_assets);
        }

        // 3. src мог быть как есть относительным путём от cwd.
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        None
    }

    /// Ищем модель: cache-files/ → assets/models/ → абсолютный путь.
    /// Поддерживает формы: `assets/models/X.gltf`, `cache-files/X.gltf`,
    /// `X.gltf`, а также автоматически находит `X.gltf` внутри
    /// `cache-files/<Asset>_files/`.
    fn locate_model_file(&self, src: &str) -> Option<PathBuf> {
        let p = Path::new(src);
        if p.is_absolute() && p.is_file() {
            return Some(p.to_path_buf());
        }

        let bare = src
            .strip_prefix("assets/models/")
            .or_else(|| src.strip_prefix("assets/"))
            .or_else(|| src.strip_prefix("cache-files/"))
            .unwrap_or(src);

        // 1. cache-files/<bare>
        let in_cache = self.paths.cache_files_dir.join(bare);
        if in_cache.is_file() {
            return Some(in_cache);
        }

        // 2. cache-files/*_files/<bare>  (модели Poly Haven лежат в подпапке)
        if let Ok(rd) = std::fs::read_dir(&self.paths.cache_files_dir) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    let cand = path.join(bare);
                    if cand.is_file() {
                        return Some(cand);
                    }
                }
            }
        }

        // 3. assets/models/
        let in_models = self.paths.models_dir.join(bare);
        if in_models.is_file() {
            return Some(in_models);
        }
        let in_assets = self.paths.assets_dir.join(bare);
        if in_assets.is_file() {
            return Some(in_assets);
        }

        if p.is_file() {
            return Some(p.to_path_buf());
        }
        None
    }

    // ------------------------------------------------------------------
    // PBR: поиск сопутствующих карт
    // ------------------------------------------------------------------

    /// Ищем PBR-карты рядом с основным файлом (diff). Совпадение по префиксу
    /// без суффикса карты: `{base}_nor_gl.png`, `{base}_rough.png` и т.п.
    /// base — имя файла без `_{map}.{ext}`.
    fn collect_pbr_maps(&self, main_path: &Path) -> PbrMaps {
        let mut out = PbrMaps::default();

        let parent = match main_path.parent() {
            Some(p) => p,
            None => return out,
        };
        let stem = match main_path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => return out,
        };

        // Получаем base: срезаем "_diff" или "_nor_gl" и т.п. если попали не в diff.
        // Пример: "Brick_Floor_003_diff" -> "Brick_Floor_003".
        let base = strip_map_suffix(stem);

        for (map_name, slot) in [
            ("nor_gl", 0usize),
            ("rough", 1),
            ("ao", 2),
            ("disp", 3),
            ("arm", 4),
        ] {
            // Возможные имена: {base}_{map}.png | {base}_{map}.jpg
            let mut found: Option<Vec<u8>> = None;
            for ext in ["png", "jpg", "jpeg", "ktx2", "exr"] {
                let cand = parent.join(format!("{base}_{map_name}.{ext}"));
                if cand.is_file() {
                    if let Ok(b) = std::fs::read(&cand) {
                        found = Some(b);
                        break;
                    }
                }
            }
            if let Some(bytes) = found {
                match slot {
                    0 => out.nor_gl = Some(bytes),
                    1 => out.rough = Some(bytes),
                    2 => out.ao = Some(bytes),
                    3 => out.disp = Some(bytes),
                    4 => out.arm = Some(bytes),
                    _ => {}
                }
            }
        }
        out
    }

    /// Рядом с .gltf лежат include-файлы в подпапках (textures/, etc.).
    /// Возвращаем список абсолютных путей ко всем include-файлам.
    fn collect_gltf_includes(&self, main_path: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let parent = match main_path.parent() {
            Some(p) => p,
            None => return out,
        };
        // Просто рекурсивно собираем все файлы рядом, кроме самого главного.
        collect_files_recursive(parent, main_path, &mut out);
        out
    }
}

/// Убирает суффикс карты из имени файла: `Brick_Floor_003_diff` → `Brick_Floor_003`.
fn strip_map_suffix(stem: &str) -> String {
    for suffix in ["_diff", "_nor_gl", "_rough", "_ao", "_disp", "_arm"] {
        if let Some(prefix) = stem.strip_suffix(suffix) {
            return prefix.to_string();
        }
    }
    stem.to_string()
}

fn collect_files_recursive(dir: &Path, exclude: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p == *exclude {
            continue;
        }
        if p.is_dir() {
            collect_files_recursive(&p, exclude, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

/// Парсит строку с тегами «wall,yellow,backrooms» в Vec.
pub fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Проверка: является ли файл PNG (по сигнатуре).
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
}

/// Растеризовать inline SVG из XML уровня.
pub fn inline_svg_to_png(svg_code: &str, w: u32, h: u32) -> Result<Vec<u8>> {
    svg::svg_to_png(svg_code, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn mk_paths(dir: &Path) -> AppPaths {
        let p = AppPaths::from_root(dir.to_path_buf());
        p.ensure_dirs().unwrap();
        p
    }

    #[test]
    fn parse_tags_trims_and_skips_empty() {
        let v = parse_tags(" wall , yellow ,, backrooms ");
        assert_eq!(v, vec!["wall", "yellow", "backrooms"]);
    }

    #[test]
    fn resolver_reads_from_cache_files() {
        let dir = tempdir().unwrap();
        let paths = mk_paths(dir.path());
        // кладём файл в cache-files/
        std::fs::write(
            paths.cache_files_dir.join("Brick_Floor_003_diff.png"),
            b"\x89PNG\r\n\x1a\n_cache",
        )
        .unwrap();

        let r = Resolver::open(&paths).unwrap();
        let res = r
            .resolve_texture(
                Some("assets/textures/Brick_Floor_003_diff.png"),
                &[],
                "",
                16,
            )
            .unwrap();
        assert_eq!(res.source, "cache-files");
        assert!(is_png(&res.bytes));
    }

    #[test]
    fn resolver_collects_pbr_maps() {
        let dir = tempdir().unwrap();
        let paths = mk_paths(dir.path());
        let base = "Brick_Floor_003";
        for map in ["diff", "nor_gl", "rough", "ao", "disp", "arm"] {
            std::fs::write(
                paths.cache_files_dir.join(format!("{base}_{map}.png")),
                b"\x89PNG\r\n\x1a\n_",
            )
            .unwrap();
        }
        let r = Resolver::open(&paths).unwrap();
        let res = r
            .resolve_texture(Some(&format!("assets/textures/{base}_diff.png")), &[], "", 16)
            .unwrap();
        assert_eq!(res.source, "cache-files");
        assert!(res.pbr.nor_gl.is_some());
        assert!(res.pbr.rough.is_some());
        assert!(res.pbr.ao.is_some());
        assert!(res.pbr.disp.is_some());
        assert!(res.pbr.arm.is_some());
    }

    #[test]
    fn resolver_finds_model_in_subfolder() {
        let dir = tempdir().unwrap();
        let paths = mk_paths(dir.path());
        let sub = paths.cache_files_dir.join("Wooden_Chair_01_files");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("Wooden_Chair_01.gltf"), b"{\"asset\":{}}").unwrap();

        let r = Resolver::open(&paths).unwrap();
        let res = r
            .resolve_model(Some("assets/models/Wooden_Chair_01.gltf"), &[], "")
            .unwrap();
        assert_eq!(res.source, "cache-files");
    }

    #[test]
    fn resolver_generates_and_caches_ai_asset_when_not_found() {
        let dir = tempdir().unwrap();
        let paths = mk_paths(dir.path());
        let r = Resolver::open(&paths).unwrap();

        let tags = vec!["yellow".to_string(), "wall".to_string()];
        let res = r.resolve_texture(None, &tags, "", 16).unwrap();
        assert_eq!(res.source, "ai");
        assert!(is_png(&res.bytes));

        // Второй запрос должен вытащить из БД.
        let res2 = r.resolve_texture(None, &tags, "", 16).unwrap();
        assert_eq!(res2.source, "db");
    }

    #[test]
    fn strip_map_suffix_works() {
        assert_eq!(strip_map_suffix("Brick_Floor_003_diff"), "Brick_Floor_003");
        assert_eq!(strip_map_suffix("Brick_Floor_003_nor_gl"), "Brick_Floor_003");
        assert_eq!(strip_map_suffix("Brick_Floor_003"), "Brick_Floor_003");
    }
}
