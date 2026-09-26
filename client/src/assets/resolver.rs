//! Высокоуровневый резолвер ассетов.

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
    pub bytes: Vec<u8>,
    pub source: &'static str,
    pub tags: Vec<String>,
    pub path: Option<PathBuf>,
    pub pbr: PbrMaps,
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

    pub fn resolve_texture(
        &self,
        src: Option<&str>,
        tags: &[String],
        extra_prompt: &str,
        size: u32,
    ) -> Result<ResolvedAsset> {
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

    fn locate_texture_file(&self, src: &str) -> Option<PathBuf> {
        let p = Path::new(src);
        if p.is_absolute() && p.is_file() {
            return Some(p.to_path_buf());
        }

        let bare = src
            .strip_prefix("assets/textures/")
            .or_else(|| src.strip_prefix("assets/"))
            .or_else(|| src.strip_prefix("cache-files/"))
            .unwrap_or(src);

        let in_cache = self.paths.cache_files_dir.join(bare);
        if in_cache.is_file() {
            return Some(in_cache);
        }

        let in_textures = self.paths.textures_dir.join(bare);
        if in_textures.is_file() {
            return Some(in_textures);
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

        let in_cache = self.paths.cache_files_dir.join(bare);
        if in_cache.is_file() {
            return Some(in_cache);
        }

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
        let base = strip_map_suffix(stem);

        for (map_name, slot) in [
            ("nor_gl", 0usize),
            ("rough", 1),
            ("ao", 2),
            ("disp", 3),
            ("arm", 4),
        ] {
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

    fn collect_gltf_includes(&self, main_path: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let parent = match main_path.parent() {
            Some(p) => p,
            None => return out,
        };
        collect_files_recursive(parent, main_path, &mut out);
        out
    }
}

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

pub fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

pub fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
}

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
